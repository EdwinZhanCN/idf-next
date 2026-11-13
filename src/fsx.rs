//! `std::fs` wrappers whose errors name the path involved.

use miette::{IntoDiagnostic, Result, WrapErr};
use std::path::Path;

pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    std::fs::write(path, data)
        .into_diagnostic()
        .wrap_err_with(|| format!("writing {}", path.display()))
}

pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))
}

pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("creating {}", path.display()))
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_dir_all(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("removing {}", path.display()))
}

pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    std::fs::rename(from, to)
        .into_diagnostic()
        .wrap_err_with(|| format!("moving {} to {}", from.display(), to.display()))
}

/// Create `path` with `data`, failing if it already exists.
pub fn write_new(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(data.as_ref()))
        .into_diagnostic()
        .wrap_err_with(|| format!("creating {}", path.display()))
}
