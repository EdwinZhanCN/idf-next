//! `idf flash`, `monitor`, `run`, `erase` and `image`.

use crate::build::{self, Layout};
use crate::cli::{GlobalArgs, ImageFormat};
use crate::commands::build::{Prepared, build as build_project};
use crate::firmware::{self, FlashFile, FlasherArgs, Sdkconfig};
use crate::manifest::ByteSize;
use crate::project::Project;
use crate::sdk::{Ctx, Sdk};
use crate::serial::{self, FlashProgress, PortChoice};
use crate::{fsx, image, monitor, ui};
use espflash::connection::ResetAfterOperation;
use espflash::flasher::Flasher;
use espflash::image_format::Segment;
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Which images `idf flash` writes; none set means all of them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub app: bool,
    pub bootloader: bool,
    pub partition_table: bool,
}

impl Selection {
    fn roles(&self) -> Option<Vec<&'static str>> {
        let roles: Vec<&str> = [
            (self.app, "app"),
            (self.bootloader, "bootloader"),
            (self.partition_table, "partition-table"),
        ]
        .into_iter()
        .filter_map(|(on, r)| on.then_some(r))
        .collect();
        (!roles.is_empty()).then_some(roles)
    }
}

/// A board that was just flashed.
struct Flashed {
    port: PortChoice,
    revision: Option<u16>,
}

pub fn flash(ctx: &Ctx, global: &GlobalArgs, sel: Selection, baud: Option<u32>) -> Result<()> {
    let p = build_project(ctx, global, "flash")?;
    flash_built(&p, global, sel, baud, true).map(|_| ())
}

pub fn run(ctx: &Ctx, global: &GlobalArgs, baud: Option<u32>) -> Result<()> {
    let p = build_project(ctx, global, "run")?;
    // The monitor resets the chip once its port is open. Resetting after
    // flashing and keeping espflash's port instead loses nothing on a UART
    // bridge, but on ESP32-P4 USB-Serial-JTAG it reboots the chip a second
    // time, mid-boot.
    let flashed = flash_built(&p, global, Selection::default(), baud, false)?;
    let opts = monitor_options(
        flashed.port,
        &p.layout,
        Some(&p.installed.sdk),
        None,
        flashed.revision,
        true,
    )?;
    monitor::run(opts)
}

fn flash_built(
    p: &Prepared,
    global: &GlobalArgs,
    sel: Selection,
    baud: Option<u32>,
    reset: bool,
) -> Result<Flashed> {
    let target = p.layout.target.as_str();
    let args = FlasherArgs::load(&p.layout.dir())?;
    let files: Vec<&FlashFile> = match sel.roles() {
        None => args.files.iter().collect(),
        Some(roles) => roles
            .iter()
            .map(|r| {
                args.role(r)
                    .ok_or_else(|| miette::miette!("this build has no {r} image"))
            })
            .collect::<Result<_>>()?,
    };
    let images: Vec<(&FlashFile, Vec<u8>)> = files
        .iter()
        .map(|f| Ok((*f, f.read()?)))
        .collect::<Result<_>>()?;

    let chip = chip_for(target)?;
    let port = serial::select_port(global.port.as_deref(), global.no_input)?;
    // espflash resets the chip as the write finishes, per `after`.
    let after = if reset {
        ResetAfterOperation::HardReset
    } else {
        ResetAfterOperation::NoResetNoStub
    };
    let mut flasher = serial::connect(
        &port,
        Some(chip),
        baud.unwrap_or(serial::DEFAULT_FLASH_BAUD),
        after,
    )?;
    let info = flasher
        .device_info()
        .into_diagnostic()
        .wrap_err("reading chip information")?;
    let revision = info
        .revision
        .map(|(major, minor)| (major * 100 + minor) as u16);
    ui::status(
        "Connected",
        format!(
            "{target}{} on {}, {} flash",
            revision
                .map(|r| format!(" {}", firmware::rev_name(r)))
                .unwrap_or_default(),
            port.describe(),
            info.flash_size
        ),
    );

    if let Err(e) = preflight(target, revision, &info, &images, &args) {
        // Leave the board running what it had.
        let _ = reset_into_app(&mut flasher);
        return Err(e);
    }

    let started = Instant::now();
    let segments: Vec<Segment<'_>> = images
        .iter()
        .map(|(f, d)| {
            // esptool pads every image to a multiple of 4 with 0xFF.
            let mut data = Cow::Borrowed(d.as_slice());
            if !data.len().is_multiple_of(4) {
                let pad = 4 - data.len() % 4;
                data.to_mut().extend(std::iter::repeat_n(0xFF, pad));
            }
            Segment {
                addr: f.offset,
                data,
            }
        })
        .collect();
    let labels = images.iter().map(|(f, _)| (f.offset, f.name())).collect();
    flasher
        .write_bins_to_flash(&segments, &mut FlashProgress::new(labels))
        .map_err(|e| miette::miette!("flashing failed: {e}"))?;
    ui::status(
        "Finished",
        format!("flashing in {:.1}s", started.elapsed().as_secs_f64()),
    );
    Ok(Flashed { port, revision })
}

/// The checks esptool makes before writing: the chip runs these images,
/// and they fit its flash.
fn preflight(
    target: &str,
    revision: Option<u16>,
    info: &espflash::flasher::DeviceInfo,
    images: &[(&FlashFile, Vec<u8>)],
    args: &FlasherArgs,
) -> Result<()> {
    if let Some(rev) = revision {
        let executables: Vec<(&FlashFile, &[u8])> = images
            .iter()
            .filter(|(f, _)| matches!(f.role.as_deref(), Some("app" | "bootloader")))
            .map(|(f, d)| (*f, d.as_slice()))
            .collect();
        firmware::check_revision(target, rev, &executables)?;
    }
    let flash_bytes = u64::from(info.flash_size.size());
    if let Some(end) = images
        .iter()
        .map(|(f, d)| u64::from(f.offset) + d.len() as u64)
        .max()
        && end > flash_bytes
    {
        bail!(
            "the images end at {end:#x}, past the end of this board's {} flash",
            info.flash_size
        );
    }
    if let Some(configured) = image::flash_size_bytes(&args.settings.flash_size)
        && configured > flash_bytes
    {
        ui::warn(format!(
            "the build is configured for {} flash, but this board has {}",
            args.settings.flash_size, info.flash_size
        ));
    }

    Ok(())
}

/// Hard-reset into the application, whatever the connection's `after`.
fn reset_into_app(flasher: &mut Flasher) -> Result<()> {
    flasher
        .connection()
        .reset()
        .map_err(|e| miette::miette!("resetting the chip: {e}"))
}

fn chip_for(target: &str) -> Result<espflash::target::Chip> {
    serial::espflash_chip(target)
        .ok_or_else(|| miette::miette!("flashing {target} is not supported by espflash"))
}

/// The project and its (target, profile) layout, if run inside one. Device
/// commands that only read the build don't need the SDK.
fn project_layout(global: &GlobalArgs) -> Result<Option<(Project, Layout)>> {
    let Some(project) = Project::discover(global.project.as_deref())? else {
        return Ok(None);
    };
    let target = build::target(&project.manifest, global.target.as_ref());
    let layout = Layout::new(&project.root, &target, global.profile_name());
    Ok(Some((project, layout)))
}

fn layout(global: &GlobalArgs) -> Result<Option<Layout>> {
    Ok(project_layout(global)?.map(|(_, l)| l))
}

fn sdkconfig(layout: Option<&Layout>) -> Sdkconfig {
    layout
        .and_then(|l| Sdkconfig::load(&l.sdkconfig()).ok())
        .unwrap_or_default()
}

fn monitor_options(
    port: PortChoice,
    layout: &Layout,
    sdk: Option<&Sdk>,
    baud: Option<u32>,
    revision: Option<u16>,
    reset: bool,
) -> Result<monitor::Options> {
    let config = sdkconfig(Some(layout));
    let baud = baud
        .or_else(|| config.int("ESP_CONSOLE_UART_BAUDRATE").map(|b| b as u32))
        .or_else(|| config.int("ESPTOOLPY_MONITOR_BAUD").map(|b| b as u32))
        .unwrap_or(115_200);
    let elf = firmware::app_elf(&layout.dir()).filter(|p| p.is_file());
    if elf.is_none() {
        ui::warn("no firmware ELF in the build; addresses will not be decoded (run `idf build`)");
    }
    // Without a connection the revision is unknown; the build's minimum
    // picks the same ROM family.
    let revision = revision.or_else(|| config.int("ESP_REV_MIN_FULL").map(|r| r as u16));
    let rom_elf = revision
        .zip(sdk.and_then(Sdk::rom_elf_dir))
        .and_then(|(rev, dir)| firmware::rom_elf(&dir, layout.target.as_str(), rev));
    Ok(monitor::Options {
        port,
        baud,
        elfs: elf.into_iter().chain(rom_elf).collect(),
        reset,
    })
}

pub fn monitor(global: &GlobalArgs, baud: Option<u32>, no_reset: bool) -> Result<()> {
    let port = serial::select_port(global.port.as_deref(), global.no_input)?;
    let opts = match project_layout(global)? {
        Some((project, l)) => {
            // The SDK only for its ROM ELFs: never install one to monitor.
            let sdk = project.lock.as_ref().and_then(|lock| {
                let home = crate::home::Home::from_env().ok()?;
                Sdk::load(&home, &lock.idf.version).ok().flatten()
            });
            monitor_options(port, &l, sdk.as_ref(), baud, None, !no_reset)?
        }
        None => monitor::Options {
            port,
            baud: baud.unwrap_or(115_200),
            elfs: Vec::new(),
            reset: !no_reset,
        },
    };
    monitor::run(opts)
}

/// What `idf erase` erases.
pub enum EraseWhat {
    All,
    Partition(String),
    Region(ByteSize, ByteSize),
}

/// Largest partition table IDF accepts (`ESP_PARTITION_TABLE_MAX_LEN`).
const PARTITION_TABLE_MAX: u32 = 0xC00;
const SECTOR: u64 = 0x1000;

pub fn erase(global: &GlobalArgs, what: EraseWhat, baud: Option<u32>) -> Result<()> {
    if let EraseWhat::Region(offset, size) = &what {
        check_region(offset.0, size.0)?;
    }
    let layout = layout(global)?;
    let chip = layout
        .as_ref()
        .map(|l| chip_for(l.target.as_str()))
        .transpose()?;
    let port = serial::select_port(global.port.as_deref(), global.no_input)?;
    let mut flasher = serial::connect(
        &port,
        chip,
        baud.unwrap_or(serial::DEFAULT_FLASH_BAUD),
        ResetAfterOperation::HardReset,
    )?;
    let started = Instant::now();
    match what {
        EraseWhat::All => {
            let spinner = ui::spinner("Erasing the entire flash");
            let r = flasher.erase_flash();
            spinner.finish_and_clear();
            r.map_err(|e| miette::miette!("erasing flash: {e}"))?;
            ui::status("Erased", "entire flash");
        }
        EraseWhat::Region(offset, size) => {
            erase_region(&mut flasher, offset.0, size.0, "region")?;
        }
        EraseWhat::Partition(name) => {
            // The table on the board, as parttool.py reads it.
            let table_offset = sdkconfig(layout.as_ref())
                .int("PARTITION_TABLE_OFFSET")
                .unwrap_or(0x8000) as u32;
            let table = read_partition_table(&mut flasher, table_offset)?;
            let Some(part) = table.find(&name) else {
                let names: Vec<String> = table.partitions().iter().map(|p| p.name()).collect();
                bail!(
                    help = format!("The board's partitions: {}.", names.join(", ")),
                    "no partition named `{name}` on the board"
                );
            };
            erase_region(
                &mut flasher,
                u64::from(part.offset()),
                u64::from(part.size()),
                &format!("partition {name}"),
            )?;
        }
    }
    reset_into_app(&mut flasher)?;
    ui::detail(
        "Finished",
        format!("in {:.1}s", started.elapsed().as_secs_f64()),
    );
    Ok(())
}

/// The ROM erases whole 4K sectors only.
fn check_region(offset: u64, size: u64) -> Result<(u32, u32)> {
    if !offset.is_multiple_of(SECTOR) || !size.is_multiple_of(SECTOR) || size == 0 {
        bail!(
            "offset and size must be non-zero multiples of 4K (0x1000); got {offset:#x} and {size:#x}"
        );
    }
    match (u32::try_from(offset), u32::try_from(size)) {
        (Ok(o), Ok(s)) if o.checked_add(s).is_some() => Ok((o, s)),
        _ => Err(miette::miette!(
            "{offset:#x}+{size:#x} is past the end of any flash"
        )),
    }
}

fn erase_region(flasher: &mut Flasher, offset: u64, size: u64, what: &str) -> Result<()> {
    let (o, s) = check_region(offset, size)?;
    let spinner = ui::spinner(format!("Erasing {what} ({size:#x} bytes at {offset:#x})"));
    let r = flasher.erase_region(o, s);
    spinner.finish_and_clear();
    r.map_err(|e| miette::miette!("erasing {what}: {e}"))?;
    ui::status("Erased", format!("{what}, {size:#x} bytes at {offset:#x}"));
    Ok(())
}

fn read_partition_table(
    flasher: &mut Flasher,
    offset: u32,
) -> Result<esp_idf_part::PartitionTable> {
    let tmp = tempfile::NamedTempFile::new().into_diagnostic()?;
    flasher
        .read_flash(
            offset,
            PARTITION_TABLE_MAX,
            0x1000,
            64,
            tmp.path().to_owned(),
        )
        .map_err(|e| miette::miette!("reading the partition table at {offset:#x}: {e}"))?;
    let data = std::fs::read(tmp.path()).into_diagnostic()?;
    esp_idf_part::PartitionTable::try_from_bytes(data).map_err(|e| {
        miette::miette!(
            help = "Flash the firmware first, or erase a region by offset with --region.",
            "no valid partition table at {offset:#x} on the board: {e}"
        )
    })
}

pub fn image(
    ctx: &Ctx,
    global: &GlobalArgs,
    format: ImageFormat,
    output: Option<&Path>,
) -> Result<()> {
    let p = build_project(ctx, global, "image")?;
    let dir = p.layout.dir();
    let target = p.layout.target.as_str();
    let args = FlasherArgs::load(&dir)?;
    let parts: Vec<image::Part> = args
        .files
        .iter()
        .map(|f| {
            Ok(image::Part {
                offset: f.offset,
                name: f.name(),
                data: f.read()?,
            })
        })
        .collect::<Result<_>>()?;
    let (bytes, default_name, note) = match format {
        ImageFormat::Bin => (
            image::merge_raw(&parts),
            "merged-binary.bin",
            ", flash at 0x0".to_owned(),
        ),
        ImageFormat::Uf2 => {
            let id = image::uf2_family_id(target)
                .ok_or_else(|| miette::miette!("no UF2 family ID is known for {target}"))?;
            (image::uf2(&parts, id), "uf2.bin", String::new())
        }
        ImageFormat::Dfu => {
            let pid = image::dfu_pid(target).ok_or_else(|| {
                miette::miette!(
                    help = "USB DFU is available on esp32s2, esp32s3 and esp32p4.",
                    "{target} cannot be flashed over USB DFU"
                )
            })?;
            let size = image::flash_size_bytes(&args.settings.flash_size).ok_or_else(|| {
                miette::miette!(
                    "unsupported flash size {:?} for DFU",
                    args.settings.flash_size
                )
            })?;
            let part_size = std::env::var("ESP_DFU_PART_SIZE")
                .ok()
                .and_then(|s| parse_int(&s))
                .unwrap_or(image::DFU_PART_SIZE);
            (
                image::dfu(&parts, pid, size, part_size),
                "dfu.bin",
                String::new(),
            )
        }
    };
    let out: PathBuf = match output {
        Some(o) => o.to_owned(),
        None => dir.join(default_name),
    };
    if let Some(parent) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        fsx::create_dir_all(parent)?;
    }
    fsx::write(&out, &bytes)?;
    let shown = out.strip_prefix(&p.project.root).unwrap_or(&out);
    ui::status(
        "Wrote",
        format!(
            "{} ({}{note})",
            shown.display(),
            ui::bytes(bytes.len() as u64)
        ),
    );
    Ok(())
}

fn parse_int(s: &str) -> Option<usize> {
    match s.strip_prefix("0x") {
        Some(h) => usize::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}
