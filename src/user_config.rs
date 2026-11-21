//! Per-user defaults, `~/.config/idf/config.toml`.
//!
//! ```toml
//! port = "/dev/cu.usbmodem1101"   # used when -p and IDF_PORT are absent
//! flash_baud = 921600             # used when --baud is absent (flash, run, erase)
//! ```
//!
//! The file is optional. Location: `$IDF_CONFIG`, else
//! `$XDG_CONFIG_HOME/idf/config.toml`, else `~/.config/idf/config.toml`.
//! Command-line flags and environment variables win over the file.

use miette::{IntoDiagnostic, Result, WrapErr};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
    pub port: Option<String>,
    pub flash_baud: Option<u32>,
}

impl UserConfig {
    pub fn load() -> Result<Self> {
        match path() {
            Some(p) => Self::load_from(&p),
            None => Ok(Self::default()),
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => {
                return Err(e)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("reading {}", path.display()));
            }
        };
        toml::from_str(&text)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {}", path.display()))
    }
}

fn path() -> Option<PathBuf> {
    let env = |k| std::env::var_os(k).filter(|v| !v.is_empty());
    if let Some(p) = env("IDF_CONFIG") {
        return Some(PathBuf::from(p));
    }
    let base = match env("XDG_CONFIG_HOME") {
        Some(x) => PathBuf::from(x),
        None => std::env::home_dir()?.join(".config"),
    };
    Some(base.join("idf").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_default() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            UserConfig::load_from(&d.path().join("nope.toml")).unwrap(),
            UserConfig::default()
        );
    }

    #[test]
    fn reads_keys_and_rejects_unknown() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("c.toml");
        std::fs::write(&f, "port = \"/dev/cu.x\"\nflash_baud = 921600\n").unwrap();
        let c = UserConfig::load_from(&f).unwrap();
        assert_eq!(c.port.as_deref(), Some("/dev/cu.x"));
        assert_eq!(c.flash_baud, Some(921_600));
        std::fs::write(&f, "prot = \"x\"\n").unwrap();
        assert!(UserConfig::load_from(&f).is_err());
    }
}
