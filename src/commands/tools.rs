//! `idf size`, `efuse`, `secure`, `coredump` and `debug`: the upstream tools
//! with the chip, port and file arguments the project already knows filled in.
//! Everything after the command name goes to the tool unchanged.

use crate::build;
use crate::cli::GlobalArgs;
use crate::commands::build::{Prepared, prepare};
use crate::sdk::Ctx;
use crate::{firmware, serial, ui};
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use std::io::IsTerminal;
use std::process::Command;

fn has(args: &[String], names: &[&str]) -> bool {
    args.iter().any(|a| {
        names
            .iter()
            .any(|n| a == n || a.strip_prefix(n).is_some_and(|r| r.starts_with('=')))
    })
}

/// Run `python -m <module> <args>` in the SDK's environment, passing the
/// tool's exit status on.
fn python_module(p: &Prepared, module: &str, args: &[String]) -> Result<()> {
    let mut cmd = Command::new(p.installed.python.python());
    p.build().env().apply(&mut cmd);
    cmd.arg("-m").arg(module).args(args);
    finish(cmd, module)
}

fn finish(mut cmd: Command, what: &str) -> Result<()> {
    ui::detail("Running", format!("{cmd:?}"));
    let status = cmd
        .status()
        .into_diagnostic()
        .wrap_err_with(|| format!("running {what}"))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

// ------------------------------------------------------------------ size ---

pub fn size(ctx: &Ctx, global: &GlobalArgs, args: &[String]) -> Result<()> {
    let p = prepare(ctx, global, "size")?;
    let mut args = args.to_vec();
    if !args.iter().any(|a| a.ends_with(".map")) {
        args.push(map_file(&p)?.display().to_string());
    }
    python_module(&p, "esp_idf_size", &args)
}

fn map_file(p: &Prepared) -> Result<std::path::PathBuf> {
    let dir = p.layout.dir();
    let name = p.project.manifest.project.name.get_ref();
    let map = dir.join(format!("{name}.map"));
    if map.is_file() {
        return Ok(map);
    }
    Err(miette::miette!(
        help = "Run `idf build` first.",
        "{} not found: this project has not been built for {} ({})",
        map.display(),
        p.layout.target,
        p.layout.profile
    ))
}

// ----------------------------------------------------------------- efuse ---

/// `espefuse` commands that change nothing on the chip.
pub const EFUSE_READ_ONLY: &[&str] = &[
    "summary",
    "dump",
    "get-custom-mac",
    "adc-info",
    "check-error",
];

/// `espefuse` commands that burn eFuses, which cannot be undone.
pub const EFUSE_BURN: &[&str] = &[
    "burn-efuse",
    "burn-block-data",
    "burn-bit",
    "burn-key",
    "burn-key-digest",
    "burn-custom-mac",
    "set-flash-voltage",
    "write-protect-efuse",
    "read-protect-efuse",
    "execute-scripts",
];

/// Whether `args` would burn anything on a real chip.
fn efuse_burns(args: &[String]) -> bool {
    args.iter().any(|a| EFUSE_BURN.contains(&a.as_str()))
}

pub fn efuse(ctx: &Ctx, global: &GlobalArgs, args: &[String]) -> Result<()> {
    let p = prepare(ctx, global, "efuse")?;
    let virt = has(args, &["--virt"]);
    if !virt {
        if has(args, &["--do-not-confirm"]) {
            bail!(
                help = "Use --virt to try a command without a chip.",
                "`idf efuse` does not pass --do-not-confirm on a real chip"
            );
        }
        if efuse_burns(args) {
            confirm_burn()?;
        }
    }
    let mut full = Vec::new();
    if !has(args, &["--chip", "-c"]) {
        full.extend(["--chip".to_owned(), p.layout.target.as_str().to_owned()]);
    }
    if !virt && !has(args, &["--port", "-p"]) && !args.iter().any(|a| a == "--help" || a == "-h") {
        let port = serial::select_port(global.port.as_deref(), global.no_input)?;
        full.extend(["--port".to_owned(), port.name]);
    }
    full.extend(args.iter().cloned());
    python_module(&p, "espefuse", &full)
}

fn confirm_burn() -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        bail!(
            help = "Run it from a terminal. Use --virt to try a command without a chip.",
            "burning eFuses cannot be undone, and needs a person at the keyboard"
        );
    }
    use std::io::{BufRead, Write};
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "This burns eFuses on the connected chip. It cannot be undone, and a wrong value can make the chip unusable."
    );
    let _ = write!(err, "Type \"burn\" to continue: ");
    let _ = err.flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .into_diagnostic()?;
    if line.trim() != "burn" {
        bail!("not confirmed; nothing was burned");
    }
    Ok(())
}

// ---------------------------------------------------------------- secure ---

pub fn secure(ctx: &Ctx, global: &GlobalArgs, args: &[String]) -> Result<()> {
    let p = prepare(ctx, global, "secure")?;
    python_module(&p, "espsecure", args)
}

// -------------------------------------------------------------- coredump ---

pub fn coredump(ctx: &Ctx, global: &GlobalArgs, args: &[String]) -> Result<()> {
    let p = prepare(ctx, global, "coredump")?;
    let mut full = Vec::new();
    if !has(args, &["--chip"]) {
        full.extend(["--chip".to_owned(), p.layout.target.as_str().to_owned()]);
    }
    let help = args.iter().any(|a| a == "--help" || a == "-h");
    // Without a core file the dump is read from flash.
    if !help && !has(args, &["--core", "-c", "--port", "-p"]) && !args.is_empty() {
        let port = serial::select_port(global.port.as_deref(), global.no_input)?;
        full.extend(["--port".to_owned(), port.name]);
    }
    full.extend(args.iter().cloned());
    if !help && !args.is_empty() && !args.iter().any(|a| a.ends_with(".elf")) {
        let elf = firmware::app_elf(&p.layout.dir())
            .filter(|e| e.is_file())
            .ok_or_else(|| {
                miette::miette!(
                    help = "Run `idf build` first.",
                    "no firmware ELF in the build"
                )
            })?;
        full.push(elf.display().to_string());
    }
    python_module(&p, "esp_coredump", &full)
}

// ----------------------------------------------------------------- debug ---

/// `idf.py openocd gdb`, with the OpenOCD board file picked for the
/// USB-Serial-JTAG built into the chip when that is what is connected.
pub fn debug(ctx: &Ctx, global: &GlobalArgs, args: &[String]) -> Result<()> {
    let p = prepare(ctx, global, "debug")?;
    build::generate(&p.project, &p.layout)?;
    let b = p.build();
    let mut cmd = Command::new(p.installed.python.python());
    b.env().apply(&mut cmd);
    if std::env::var_os("OPENOCD_COMMANDS").is_none()
        && let Some(mut commands) = builtin_jtag_commands(global, &b, p.layout.target.as_str())
    {
        // OpenOCD's flash stub faults on ESP32-P4 before v3 (seen on v1.3), and
        // GDB's connection is then refused. Without the flash memory map GDB
        // still works, with hardware breakpoints only.
        let rev_less_v3 = firmware::Sdkconfig::load(&p.layout.sdkconfig())
            .ok()
            .and_then(|c| {
                c.get("ESP32P4_SELECTS_REV_LESS_V3")
                    .map(str::to_owned)
            })
            .is_some_and(|v| v == "y");
        if rev_less_v3 {
            ui::status(
                "Debugging",
                "ESP32-P4 before v3: OpenOCD cannot read the flash map, so breakpoints in flash use the chip's hardware breakpoints",
            );
            commands.push_str(" -c 'gdb_memory_map disable'");
        }
        ui::detail(
            "Debugging",
            format!("through the chip's USB-Serial-JTAG: {commands}"),
        );
        cmd.env("OPENOCD_COMMANDS", commands);
    }
    cmd.arg(p.installed.sdk.idf_path().join("tools/idf.py"))
        .arg("-C")
        .arg(p.layout.project_dir())
        .arg("-B")
        .arg(p.layout.dir())
        .args(["openocd", "gdb"])
        .args(args);
    finish(cmd, "idf.py")
}

fn builtin_jtag_commands(global: &GlobalArgs, b: &build::Build, chip: &str) -> Option<String> {
    let port = serial::select_port(global.port.as_deref(), true).ok()?;
    if !port.is_usb_serial_jtag() {
        return None;
    }
    let scripts = b
        .env()
        .vars
        .iter()
        .find(|(k, _)| k == "OPENOCD_SCRIPTS")
        .map(|(_, v)| std::path::PathBuf::from(v))?;
    let board = format!("board/{chip}-builtin.cfg");
    scripts
        .join(&board)
        .is_file()
        .then(|| format!("-f {board}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn finds_burning_commands() {
        assert!(efuse_burns(&v(&["--port", "x", "burn-efuse", "A", "1"])));
        assert!(efuse_burns(&v(&["write-protect-efuse", "A"])));
        assert!(!efuse_burns(&v(&["summary"])));
        assert!(!efuse_burns(&v(&["--virt", "dump"])));
        for read in EFUSE_READ_ONLY {
            assert!(!EFUSE_BURN.contains(read));
        }
    }

    #[test]
    fn option_detection_accepts_equals_form() {
        assert!(has(&v(&["--chip=esp32"]), &["--chip"]));
        assert!(has(&v(&["-c", "esp32"]), &["--chip", "-c"]));
        assert!(!has(&v(&["--chips"]), &["--chip"]));
    }
}
