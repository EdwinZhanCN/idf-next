//! Finding the board's serial port and connecting to its ROM loader.

use crate::ui;
use espflash::connection::{Connection, Port, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::target::ProgressCallbacks;
use indicatif::ProgressBar;
use miette::{IntoDiagnostic, Result, WrapErr};
use serialport::{FlowControl, SerialPortType, UsbPortInfo};
use std::time::Duration;

/// Flash baud rate when `-b` is not given; `idf.py`'s default.
pub const DEFAULT_FLASH_BAUD: u32 = 460_800;

/// USB-to-UART bridges found on Espressif and common third-party boards,
/// plus Espressif's own USB devices (USB-Serial-JTAG, USB-OTG CDC).
const KNOWN: &[(u16, Option<u16>, &str)] = &[
    (0x303a, None, "Espressif USB"),
    (0x10c4, Some(0xea60), "CP210x"),
    (0x1a86, Some(0x7523), "CH340"),
    (0x1a86, Some(0x55d3), "CH343"),
    (0x1a86, Some(0x55d4), "CH9102"),
    (0x0403, Some(0x6001), "FT232"),
    (0x0403, Some(0x6010), "FT2232"),
    (0x0403, Some(0x6014), "FT232H"),
    (0x0403, Some(0x6015), "FT231X"),
];

/// A serial port and the USB device behind it.
#[derive(Debug, Clone)]
pub struct PortChoice {
    pub name: String,
    pub usb: UsbPortInfo,
}

impl PortChoice {
    /// Espressif's USB-Serial-JTAG peripheral, which resets differently.
    pub fn is_usb_serial_jtag(&self) -> bool {
        self.usb.vid == 0x303a && self.usb.pid == 0x1001
    }

    pub fn describe(&self) -> String {
        let kind = KNOWN
            .iter()
            .find(|(v, p, _)| *v == self.usb.vid && p.is_none_or(|p| p == self.usb.pid))
            .map(|(_, _, k)| *k);
        match (self.is_usb_serial_jtag(), kind) {
            (true, _) => format!("{} (USB-Serial-JTAG)", self.name),
            (false, Some(k)) => format!("{} ({k})", self.name),
            (false, None) if self.usb.vid != 0 => {
                format!("{} ({:04x}:{:04x})", self.name, self.usb.vid, self.usb.pid)
            }
            (false, None) => self.name.clone(),
        }
    }
}

fn no_usb() -> UsbPortInfo {
    UsbPortInfo {
        vid: 0,
        pid: 0,
        serial_number: None,
        manufacturer: None,
        product: None,
    }
}

fn is_known(usb: &UsbPortInfo) -> bool {
    KNOWN
        .iter()
        .any(|(v, p, _)| *v == usb.vid && p.is_none_or(|p| p == usb.pid))
}

/// Every USB serial port, without macOS's `/dev/tty.*` twins of `/dev/cu.*`
/// (opening a tty.* waits for carrier detect).
fn usb_ports() -> Result<Vec<PortChoice>> {
    let ports = serialport::available_ports()
        .into_diagnostic()
        .wrap_err("listing serial ports")?;
    let mut out: Vec<PortChoice> = ports
        .into_iter()
        .filter(|p| !p.port_name.starts_with("/dev/tty."))
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(usb) => Some(PortChoice {
                name: p.port_name,
                usb,
            }),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

/// The port to use: `-p`/`IDF_PORT`, else the one known board on USB.
pub fn select_port(requested: Option<&str>, no_input: bool) -> Result<PortChoice> {
    let ports = usb_ports()?;
    if let Some(name) = requested {
        // Keep the USB IDs when the port is one we can see; they pick the
        // reset sequence.
        let usb = ports
            .iter()
            .find(|p| p.name == name || p.name == name.replacen("/dev/tty.", "/dev/cu.", 1))
            .map(|p| p.usb.clone())
            .unwrap_or_else(no_usb);
        return Ok(PortChoice {
            name: name.to_owned(),
            usb,
        });
    }
    let known: Vec<PortChoice> = ports.iter().filter(|p| is_known(&p.usb)).cloned().collect();
    match known.as_slice() {
        [one] => Ok(one.clone()),
        [] => {
            let others: Vec<String> = ports.iter().map(PortChoice::describe).collect();
            let help = if others.is_empty() {
                "Connect the board over USB, or pass -p <PORT> (or set IDF_PORT).".to_owned()
            } else {
                format!(
                    "Other USB serial ports: {}. Pass one with -p <PORT> (or set IDF_PORT).",
                    others.join(", ")
                )
            };
            Err(miette::miette!(help = help, "no ESP board found on USB"))
        }
        many if no_input || !stdin_is_terminal() => {
            let list: Vec<String> = many.iter().map(PortChoice::describe).collect();
            Err(miette::miette!(
                help = "Pick one with -p <PORT> (or set IDF_PORT).",
                "several boards found: {}",
                list.join(", ")
            ))
        }
        many => prompt(many),
    }
}

fn stdin_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn prompt(ports: &[PortChoice]) -> Result<PortChoice> {
    use std::io::{BufRead, Write};
    let mut err = std::io::stderr();
    let _ = writeln!(err, "Several boards are connected:");
    for (i, p) in ports.iter().enumerate() {
        let _ = writeln!(err, "  {}) {}", i + 1, p.describe());
    }
    let _ = write!(err, "Which one? [1-{}] ", ports.len());
    let _ = err.flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .into_diagnostic()?;
    match line.trim().parse::<usize>() {
        Ok(n) if (1..=ports.len()).contains(&n) => Ok(ports[n - 1].clone()),
        _ => Err(miette::miette!("no port selected")),
    }
}

/// Open the port the way the monitor wants it: no flow control, the
/// reset lines released, a short read timeout.
pub fn open(port: &PortChoice, baud: u32) -> Result<Port> {
    let mut serial = serialport::new(&port.name, baud)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(20))
        .open_native()
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", port.name))?;
    use serialport::SerialPort;
    // Opening raises both lines. Released lines are "run" for both reset
    // circuits (UART bridge and USB-Serial-JTAG); RTS goes first, because
    // RTS high with DTR low is "reset" and would reboot the chip on open.
    let _ = serial.write_request_to_send(false);
    let _ = serial.write_data_terminal_ready(false);
    Ok(serial)
}

/// Reset the chip into its application: espflash's `reset_after_flash`
/// sequence (EN low through RTS; on USB-Serial-JTAG, through the
/// peripheral's own RTS/DTR mapping).
pub fn hard_reset(serial: &mut Port, port: &PortChoice) -> Result<()> {
    use serialport::SerialPort;
    use std::thread::sleep;
    let ms = |n| Duration::from_millis(n);
    let r = (|| -> serialport::Result<()> {
        sleep(ms(100));
        if port.is_usb_serial_jtag() {
            serial.write_data_terminal_ready(false)?;
            sleep(ms(100));
            serial.write_request_to_send(true)?;
            serial.write_data_terminal_ready(false)?;
            serial.write_request_to_send(true)?;
            sleep(ms(100));
            serial.write_request_to_send(false)?;
        } else {
            serial.write_request_to_send(true)?;
            sleep(ms(100));
            serial.write_request_to_send(false)?;
        }
        Ok(())
    })();
    r.into_diagnostic().wrap_err("resetting the chip")
}

/// Connect to the ROM loader, load the flasher stub and switch to `baud`.
/// `after` is what espflash does once a write finishes: reset into the
/// application, or leave the chip in the stub.
pub fn connect(
    port: &PortChoice,
    chip: Option<espflash::target::Chip>,
    baud: u32,
    after: ResetAfterOperation,
) -> Result<Flasher> {
    let spinner = ui::spinner(format!("Connecting to {}", port.describe()));
    let serial = serialport::new(&port.name, 115_200)
        .flow_control(FlowControl::None)
        .open_native()
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", port.name))?;
    let before = if port.is_usb_serial_jtag() {
        ResetBeforeOperation::UsbReset
    } else {
        ResetBeforeOperation::DefaultReset
    };
    let connection = Connection::new(serial, port.usb.clone(), after, before, 115_200);
    let flasher = Flasher::connect(connection, true, true, true, chip, Some(baud));
    spinner.finish_and_clear();
    flasher.map_err(|e| connect_error(e, port))
}

fn connect_error(e: espflash::Error, port: &PortChoice) -> miette::Report {
    use espflash::Error as E;
    match e {
        E::ChipMismatch(want, found) => miette::miette!(
            help = format!(
                "Build for it with `idf -t {found} …`, or set it with `idf target set {found}`."
            ),
            "the board on {} is an {found}, but the project targets {want}",
            port.name
        ),
        E::Connection(_) => miette::miette!(
            help = "Hold BOOT and tap RESET (EN) to enter download mode, then try again. \
                    Close any other program that has the port open.",
            "could not talk to the ROM loader on {}: {e}",
            port.name
        ),
        other => miette::miette!("connecting on {}: {other}", port.name),
    }
}

/// Our espflash chip for an IDF target name.
pub fn espflash_chip(target: &str) -> Option<espflash::target::Chip> {
    target.parse().ok()
}

/// `major * 100 + minor`, for comparing with image headers.
pub fn revision(flasher: &mut Flasher) -> Result<Option<u16>> {
    let info = flasher
        .device_info()
        .into_diagnostic()
        .wrap_err("reading chip information")?;
    Ok(info
        .revision
        .map(|(major, minor)| (major * 100 + minor) as u16))
}

/// One progress bar per image written. espflash reports progress in
/// (compressed) chunks, and reports a skipped image with `finish` alone, so
/// images are labelled in the order they are written.
pub struct FlashProgress {
    labels: Vec<(u32, String)>,
    next: usize,
    current: Option<String>,
    bar: Option<ProgressBar>,
}

impl FlashProgress {
    /// `(offset, name)` for each image, in write order.
    pub fn new(labels: Vec<(u32, String)>) -> Self {
        FlashProgress {
            labels,
            next: 0,
            current: None,
            bar: None,
        }
    }

    fn label_for(&self, addr: Option<u32>) -> String {
        let found = addr.and_then(|a| self.labels.iter().find(|(o, _)| *o == a));
        match found.or_else(|| self.labels.get(self.next)) {
            Some((o, name)) => format!("{name} @ {o:#x}"),
            None => addr.map(|a| format!("{a:#x}")).unwrap_or_default(),
        }
    }
}

impl ProgressCallbacks for FlashProgress {
    fn init(&mut self, addr: u32, total: usize) {
        let label = self.label_for(Some(addr));
        let bar = ui::bar(total as u64, "Flashing", label.clone());
        self.current = Some(label);
        self.bar = Some(bar);
    }

    fn update(&mut self, current: usize) {
        if let Some(b) = &self.bar {
            b.set_position(current as u64);
        }
    }

    fn verifying(&mut self) {
        if let Some(b) = &self.bar {
            b.set_prefix("Verifying");
        }
    }

    fn finish(&mut self, skipped: bool) {
        if let Some(b) = self.bar.take() {
            b.finish_and_clear();
        }
        let label = self.current.take().unwrap_or_else(|| self.label_for(None));
        self.next += 1;
        ui::status(if skipped { "Unchanged" } else { "Flashed" }, label);
    }
}
