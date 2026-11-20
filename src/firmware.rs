//! What a finished build left behind for the device commands: the images
//! and offsets from `flasher_args.json`, values from `sdkconfig`, and the
//! chip requirements in each image's header.

use crate::fsx;
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One file to write, as `flasher_args.json` lists it.
#[derive(Debug, Clone)]
pub struct FlashFile {
    pub offset: u32,
    pub path: PathBuf,
    /// `bootloader`, `partition-table`, `app`, `otadata`, … when the build
    /// names it; anything else IDF adds (e.g. `esptool_py_flash_target`
    /// images) has no role.
    pub role: Option<String>,
}

impl FlashFile {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn read(&self) -> Result<Vec<u8>> {
        std::fs::read(&self.path)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", self.path.display()))
    }
}

/// `flash_settings` in `flasher_args.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct FlashSettings {
    pub flash_mode: String,
    pub flash_size: String,
    pub flash_freq: String,
}

/// The parts of `flasher_args.json` the device commands use.
#[derive(Debug, Clone)]
pub struct FlasherArgs {
    /// Every file of a full flash, sorted by offset.
    pub files: Vec<FlashFile>,
    pub settings: FlashSettings,
}

#[derive(Deserialize)]
struct RawArgs {
    flash_files: BTreeMap<String, String>,
    flash_settings: FlashSettings,
    #[serde(flatten)]
    named: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct RawNamed {
    offset: String,
    file: String,
}

fn parse_offset(s: &str) -> Result<u32> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => s.parse(),
    };
    parsed
        .into_diagnostic()
        .wrap_err_with(|| format!("bad offset {s:?} in flasher_args.json"))
}

impl FlasherArgs {
    pub fn load(build_dir: &Path) -> Result<FlasherArgs> {
        let path = build_dir.join("flasher_args.json");
        let text = fsx::read_to_string(&path)?;
        Self::parse(&text, build_dir).wrap_err_with(|| format!("reading {}", path.display()))
    }

    fn parse(text: &str, build_dir: &Path) -> Result<FlasherArgs> {
        let raw: RawArgs = serde_json::from_str(text).into_diagnostic()?;
        // Roles come from the named entries (`"app": {offset, file}`, …).
        let mut roles = BTreeMap::new();
        for (name, v) in &raw.named {
            // Only objects: serde would read a two-element array as one too.
            if let Some(Ok(n)) = v.is_object().then(|| RawNamed::deserialize(v)) {
                roles.insert((parse_offset(&n.offset)?, n.file), name.clone());
            }
        }
        let mut files = Vec::new();
        for (offset, file) in raw.flash_files {
            let offset = parse_offset(&offset)?;
            let role = roles.get(&(offset, file.clone())).cloned();
            files.push(FlashFile {
                offset,
                path: build_dir.join(file),
                role,
            });
        }
        files.sort_by_key(|f| f.offset);
        Ok(FlasherArgs {
            files,
            settings: raw.flash_settings,
        })
    }

    /// The file with this role (`app`, `bootloader`, `partition-table`).
    pub fn role(&self, role: &str) -> Option<&FlashFile> {
        self.files.iter().find(|f| f.role.as_deref() == Some(role))
    }
}

/// `CONFIG_*` values from a generated `sdkconfig`, keys without the prefix
/// and strings unquoted.
#[derive(Debug, Default)]
pub struct Sdkconfig(BTreeMap<String, String>);

impl Sdkconfig {
    pub fn load(path: &Path) -> Result<Sdkconfig> {
        Ok(Self::parse(&fsx::read_to_string(path)?))
    }

    fn parse(text: &str) -> Sdkconfig {
        let mut map = BTreeMap::new();
        for line in text.lines() {
            let Some((k, v)) = line.strip_prefix("CONFIG_").and_then(|l| l.split_once('=')) else {
                continue;
            };
            let v = match v.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                Some(s) => s.replace("\\\"", "\"").replace("\\\\", "\\"),
                None => v.to_owned(),
            };
            map.insert(k.to_owned(), v);
        }
        Sdkconfig(map)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn is_set(&self, key: &str) -> bool {
        self.get(key) == Some("y")
    }

    pub fn int(&self, key: &str) -> Option<u64> {
        let v = self.get(key)?;
        match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(hex, 16).ok(),
            None => v.parse().ok(),
        }
    }
}

/// The chip fields of an ESP image header (`esp_image_header_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHeader {
    pub chip_id: u16,
    /// `major * 100 + minor`
    pub min_rev: u16,
    pub max_rev: u16,
}

pub const IMAGE_MAGIC: u8 = 0xE9;

impl ImageHeader {
    /// The header of an application or bootloader image, if `data` is one.
    pub fn parse(data: &[u8]) -> Option<ImageHeader> {
        if data.len() < 24 || data[0] != IMAGE_MAGIC {
            return None;
        }
        let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
        Some(ImageHeader {
            chip_id: u16_at(12),
            min_rev: u16_at(15),
            max_rev: u16_at(17),
        })
    }
}

/// `v3.1` for 301.
pub fn rev_name(rev: u16) -> String {
    format!("v{}.{}", rev / 100, rev % 100)
}

/// Image chip IDs (`esp_chip_id_t`) by IDF target name.
pub fn image_chip_id(target: &str) -> Option<u16> {
    Some(match target {
        "esp32" => 0x0000,
        "esp32s2" => 0x0002,
        "esp32c3" => 0x0005,
        "esp32s3" => 0x0009,
        "esp32c2" => 0x000C,
        "esp32c6" => 0x000D,
        "esp32h2" => 0x0010,
        "esp32p4" => 0x0012,
        "esp32c5" => 0x0017,
        "esp32c61" => 0x0014,
        _ => return None,
    })
}

/// Check that a chip at `rev` runs the images, the way esptool checks
/// before writing.
pub fn check_revision(target: &str, rev: u16, images: &[(&FlashFile, &[u8])]) -> Result<()> {
    for (file, data) in images {
        let Some(h) = ImageHeader::parse(data) else {
            continue;
        };
        if let Some(id) = image_chip_id(target)
            && h.chip_id != id
        {
            bail!(
                "{} was built for chip ID {:#x}, not {target}",
                file.name(),
                h.chip_id
            );
        }
        if rev < h.min_rev || rev > h.max_rev {
            let help = if target == "esp32p4" && rev < 300 {
                "ESP-IDF builds for ESP32-P4 v3.x unless told otherwise. For this chip, add\n\
                 \n    [config]\n    ESP32P4_SELECTS_REV_LESS_V3 = true\n\n\
                 to idf.toml (it is a per-board choice: v3.x and earlier revisions need different builds)."
                    .to_owned()
            } else {
                "Set the minimum (and maximum) supported revision in [config] to match this chip."
                    .to_owned()
            };
            bail!(
                help = help,
                "{} supports {target} revisions {} to {}, but this chip is {}",
                file.name(),
                rev_name(h.min_rev),
                rev_name(h.max_rev),
                rev_name(rev)
            );
        }
    }
    Ok(())
}

/// The ROM ELF for a chip at `rev`: `<target>_rev<N>_rom.elf` with the
/// largest `N` not above `rev`, as esp-idf-monitor picks it.
pub fn rom_elf(dir: &Path, target: &str, rev: u16) -> Option<PathBuf> {
    let prefix = format!("{target}_rev");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            let n: u16 = name
                .strip_prefix(&prefix)?
                .strip_suffix("_rom.elf")?
                .parse()
                .ok()?;
            (n <= rev).then_some((n, dir.join(name)))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, p)| p)
}

/// The application ELF next to `app_bin`.
pub fn app_elf(build_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(build_dir.join("project_description.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(build_dir.join(v.get("app_elf")?.as_str()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARGS: &str = r#"{
        "write_flash_args" : [ "--flash-mode", "dio" ],
        "flash_settings" : { "flash_mode": "dio", "flash_size": "2MB", "flash_freq": "80m" },
        "flash_files" : {
            "0x2000" : "bootloader/bootloader.bin",
            "0x8000" : "partition_table/partition-table.bin",
            "0xd000" : "ota_data_initial.bin",
            "0x10000" : "hello.bin"
        },
        "bootloader" : { "offset" : "0x2000", "file" : "bootloader/bootloader.bin", "encrypted" : "false" },
        "partition-table" : { "offset" : "0x8000", "file" : "partition_table/partition-table.bin", "encrypted" : "false" },
        "otadata" : { "offset" : "0xd000", "file" : "ota_data_initial.bin", "encrypted" : "false" },
        "app" : { "offset" : "0x10000", "file" : "hello.bin", "encrypted" : "false" },
        "extra_esptool_args" : { "after" : "hard-reset", "before" : "default-reset", "stub" : true, "chip" : "esp32p4" }
    }"#;

    #[test]
    fn reads_flasher_args() {
        let a = FlasherArgs::parse(ARGS, Path::new("/b")).unwrap();
        let got: Vec<_> = a
            .files
            .iter()
            .map(|f| (f.offset, f.name(), f.role.clone().unwrap_or_default()))
            .collect();
        assert_eq!(
            got,
            [
                (0x2000, "bootloader.bin".into(), "bootloader".into()),
                (
                    0x8000,
                    "partition-table.bin".into(),
                    "partition-table".into()
                ),
                (0xd000, "ota_data_initial.bin".into(), "otadata".into()),
                (0x10000, "hello.bin".into(), "app".into()),
            ]
        );
        assert_eq!(a.role("app").unwrap().path, Path::new("/b/hello.bin"));
        assert_eq!(a.settings.flash_size, "2MB");
    }

    #[test]
    fn reads_sdkconfig() {
        let c = Sdkconfig::parse(
            "# x\nCONFIG_A=y\nCONFIG_B=\"say \\\"hi\\\"\"\nCONFIG_C=0x8000\n# CONFIG_D is not set\nCONFIG_E=115200\n",
        );
        assert!(c.is_set("A"));
        assert_eq!(c.get("B"), Some("say \"hi\""));
        assert_eq!(c.int("C"), Some(0x8000));
        assert_eq!(c.get("D"), None);
        assert_eq!(c.int("E"), Some(115200));
    }

    #[test]
    fn checks_chip_revision() {
        // The header of an ESP32-P4 bootloader built for v3.1–v3.99.
        let mut data = hex("e903021fcab2fa4fee0000001200002d018f010000000001");
        data.resize(64, 0);
        let h = ImageHeader::parse(&data).unwrap();
        assert_eq!(
            h,
            ImageHeader {
                chip_id: 0x12,
                min_rev: 301,
                max_rev: 399
            }
        );
        let f = FlashFile {
            offset: 0x2000,
            path: "bootloader.bin".into(),
            role: Some("bootloader".into()),
        };
        assert!(check_revision("esp32p4", 301, &[(&f, &data)]).is_ok());
        let err = check_revision("esp32p4", 103, &[(&f, &data)]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "bootloader.bin supports esp32p4 revisions v3.1 to v3.99, but this chip is v1.3"
        );
        assert!(
            err.help()
                .unwrap()
                .to_string()
                .contains("ESP32P4_SELECTS_REV_LESS_V3")
        );
        assert!(check_revision("esp32s3", 0, &[(&f, &data)]).is_err());
    }

    #[test]
    fn picks_the_rom_elf_for_a_revision() {
        let dir = tempfile::tempdir().unwrap();
        for f in [
            "esp32p4_rev0_rom.elf",
            "esp32p4_rev300_rom.elf",
            "esp32c3_rev101_rom.elf",
        ] {
            std::fs::write(dir.path().join(f), "").unwrap();
        }
        let pick = |t, r| {
            rom_elf(dir.path(), t, r).map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        };
        assert_eq!(
            pick("esp32p4", 103).as_deref(),
            Some("esp32p4_rev0_rom.elf")
        );
        assert_eq!(
            pick("esp32p4", 301).as_deref(),
            Some("esp32p4_rev300_rom.elf")
        );
        assert_eq!(pick("esp32c3", 100), None);
        assert_eq!(pick("esp32", 300), None);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
