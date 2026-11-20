//! The serial monitor: prints what the board writes, decoding code
//! addresses and RISC-V stack dumps against the firmware's ELF files, and
//! sends keystrokes back.
//!
//! The decoding is espflash's `ResolvingPrinter`; the port handling, keys
//! and reconnects are ours.

use crate::serial::{self, PortChoice};
use crate::ui;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use espflash::cli::monitor::UnwindTables;
use espflash::cli::monitor::parser::ResolvingPrinter;
use espflash::connection::Port;
use miette::{IntoDiagnostic, Result};
use std::io::{ErrorKind, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Options {
    pub port: PortChoice,
    pub baud: u32,
    /// ELFs to resolve addresses with: the app first, then the ROM.
    pub elfs: Vec<PathBuf>,
    /// Reset the chip once the port is open, to see it boot.
    pub reset: bool,
}

/// Run until the user exits (Ctrl+C or Ctrl+]).
pub fn run(opts: Options) -> Result<()> {
    let elf_data: Vec<Vec<u8>> = opts
        .elfs
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .collect();
    let elfs: Vec<&[u8]> = elf_data.iter().map(Vec::as_slice).collect();

    let mut serial = serial::open(&opts.port, opts.baud)?;
    if opts.reset {
        serial::hard_reset(&mut serial, &opts.port)?;
    }

    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    ui::status(
        "Monitoring",
        format!(
            "{} at {} baud{}",
            opts.port.describe(),
            opts.baud,
            if interactive {
                "  (Ctrl+R reset, Ctrl+C exit)"
            } else {
                ""
            }
        ),
    );
    let _raw = interactive.then(RawMode::enable).transpose()?;

    let stdout = std::io::stdout();
    let color = stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    crossterm::style::force_color_output(color);
    let writer = StripColor::new(stdout.lock(), !color);
    let mut out = ResolvingPrinter::new(elfs, writer, false, UnwindTables::Auto);
    let mut buf = [0u8; 1024];
    loop {
        match serial.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let _ = out.write_all(&buf[..n]);
                let _ = out.flush();
            }
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(_) => {
                // The board was unplugged, or its USB device re-enumerated
                // (USB-OTG CDC does on every reset); wait for the port to
                // come back under the same name.
                drop(serial);
                match reopen(&opts, interactive)? {
                    Some(s) => serial = s,
                    None => break,
                }
                continue;
            }
        }
        if interactive {
            match keys()? {
                Action::Exit => break,
                Action::Reset => serial::hard_reset(&mut serial, &opts.port)?,
                Action::Send(bytes) if !bytes.is_empty() => {
                    let _ = serial.write_all(&bytes);
                    let _ = serial.flush();
                }
                Action::Send(_) => {}
            }
        }
    }
    let _ = out.flush();
    Ok(())
}

/// Wait for the port to reappear. `None` if the user gave up.
fn reopen(opts: &Options, interactive: bool) -> Result<Option<Port>> {
    let started = Instant::now();
    let mut told = false;
    loop {
        if let Ok(s) = serial::open(&opts.port, opts.baud) {
            if told {
                message(interactive, &format!("{} is back", opts.port.name));
            }
            return Ok(Some(s));
        }
        if !told && started.elapsed() > Duration::from_secs(1) {
            message(
                interactive,
                &format!(
                    "{} disconnected; waiting for it (Ctrl+C to exit)",
                    opts.port.name
                ),
            );
            told = true;
        }
        if interactive && matches!(keys()?, Action::Exit) {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A line of our own between the board's output.
fn message(raw: bool, text: &str) {
    let eol = if raw { "\r\n" } else { "\n" };
    let mut out = std::io::stdout();
    let _ = if std::env::var_os("NO_COLOR").is_none() && out.is_terminal() {
        write!(out, "{eol}\x1b[2m--- {text} ---\x1b[0m{eol}")
    } else {
        write!(out, "{eol}--- {text} ---{eol}")
    };
    let _ = out.flush();
}

enum Action {
    Send(Vec<u8>),
    Reset,
    Exit,
}

/// Drain pending key presses.
fn keys() -> Result<Action> {
    let mut bytes = Vec::new();
    while event::poll(Duration::ZERO).into_diagnostic()? {
        let Event::Key(key) = event::read().into_diagnostic()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                // Ctrl+] arrives as Ctrl+5 on some terminals (0x1d).
                KeyCode::Char('c' | ']' | '5') => return Ok(Action::Exit),
                KeyCode::Char('r') => return Ok(Action::Reset),
                _ => {}
            }
        }
        if let Some(b) = encode(key) {
            bytes.extend_from_slice(&b);
        }
    }
    Ok(Action::Send(bytes))
}

/// Bytes a terminal would send for a key.
fn encode(key: KeyEvent) -> Option<Vec<u8>> {
    let seq: &[u8] = match key.code {
        KeyCode::Backspace => b"\x08",
        KeyCode::Enter => b"\r",
        KeyCode::Left => b"\x1b[D",
        KeyCode::Right => b"\x1b[C",
        KeyCode::Up => b"\x1b[A",
        KeyCode::Down => b"\x1b[B",
        KeyCode::Home => b"\x1b[H",
        KeyCode::End => b"\x1b[F",
        KeyCode::Tab => b"\t",
        KeyCode::Delete => b"\x1b[3~",
        KeyCode::Insert => b"\x1b[2~",
        KeyCode::Esc => b"\x1b",
        KeyCode::Char(ch) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if ch.is_ascii_lowercase() || ch == ' ' {
                    return Some(vec![ch as u8 & 0x1f]);
                }
                if ('4'..='7').contains(&ch) {
                    // Ctrl+4…7 are how crossterm reports 0x1c…0x1f.
                    return Some(vec![(ch as u8 + 8) & 0x1f]);
                }
            }
            let mut b = [0; 4];
            return Some(ch.encode_utf8(&mut b).as_bytes().to_vec());
        }
        _ => return None,
    };
    Some(seq.to_vec())
}

/// Drops SGR (color) escape sequences when color is off: the board's log
/// colors, and the empty `ESC[m` resets crossterm writes even with color
/// disabled. Sequences split across writes are handled.
struct StripColor<W: Write> {
    inner: W,
    enabled: bool,
    /// A partial `ESC [ params` held back until its final byte arrives.
    pending: Vec<u8>,
}

impl<W: Write> StripColor<W> {
    fn new(inner: W, enabled: bool) -> Self {
        StripColor {
            inner,
            enabled,
            pending: Vec::new(),
        }
    }
}

impl<W: Write> Write for StripColor<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.enabled {
            return self.inner.write_all(buf).map(|_| buf.len());
        }
        let mut out = Vec::with_capacity(buf.len());
        for &b in buf {
            match (self.pending.as_slice(), b) {
                ([], 0x1b) => self.pending.push(b),
                ([], _) => out.push(b),
                ([0x1b], b'[') => self.pending.push(b),
                ([0x1b, b'[', ..], b'0'..=b'9' | b';') => self.pending.push(b),
                ([0x1b, b'[', ..], b'm') => self.pending.clear(),
                _ => {
                    // Not a color sequence: pass it through untouched.
                    out.append(&mut self.pending);
                    if b == 0x1b {
                        self.pending.push(b);
                    } else {
                        out.push(b);
                    }
                }
            }
        }
        self.inner.write_all(&out)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

struct RawMode;

impl RawMode {
    fn enable() -> Result<RawMode> {
        crossterm::terminal::enable_raw_mode().into_diagnostic()?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_color_across_writes() {
        let mut out = Vec::new();
        let mut w = StripColor::new(&mut out, true);
        for part in [
            &b"\x1b[0;32mI (27) boot\x1b"[..],
            b"[0m ok \x1b[m",
            b"\x1b[",
            b"2K x\x1b",
        ] {
            w.write_all(part).unwrap();
        }
        w.write_all(b"[A").unwrap();
        assert_eq!(out, b"I (27) boot ok \x1b[2K x\x1b[A");
        let mut kept = Vec::new();
        StripColor::new(&mut kept, false)
            .write_all(b"\x1b[0m")
            .unwrap();
        assert_eq!(kept, b"\x1b[0m");
    }

    #[test]
    fn encodes_keys() {
        let k = |code, m| KeyEvent::new(code, m);
        assert_eq!(
            encode(k(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(b"a".to_vec())
        );
        assert_eq!(
            encode(k(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(vec![4])
        );
        assert_eq!(
            encode(k(KeyCode::Enter, KeyModifiers::NONE)),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            encode(k(KeyCode::Up, KeyModifiers::NONE)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode(k(KeyCode::Char('é'), KeyModifiers::NONE)),
            Some("é".as_bytes().to_vec())
        );
    }
}
