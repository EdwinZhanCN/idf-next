//! The managed Python environment of an SDK, built with the uv that ships in
//! the idf-next wheel.
//!
//! Resolution is `uv pip compile --universal --generate-hashes` against the
//! SDK's requirement files and Espressif's constraints file, cut off at a
//! fixed `--exclude-newer` time. The resolved file is content-addressed: its
//! digest goes into `idf.lock`, and the venv lives in a directory named after
//! it, so projects that resolved differently never share a venv.

use crate::home::Home;
use crate::lockfile::{Digest, LockedPython};
use crate::net::Net;
use crate::ui;
use miette::{Context, IntoDiagnostic, Result, bail, miette};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Interpreter series used for new resolutions.
pub const DEFAULT_PYTHON: &str = "3.12";

pub fn constraints_url(series: &str) -> String {
    format!("https://dl.espressif.com/dl/esp-idf/espidf.constraints.v{series}.txt")
}

pub fn constraints_file_name(series: &str) -> String {
    format!("espidf.constraints.v{series}.txt")
}

// --------------------------------------------------------------------- uv ---

pub struct Uv {
    exe: PathBuf,
    home: Home,
}

impl Uv {
    /// The uv next to the `idf` binary (as installed from the wheel), else
    /// `$IDF_NEXT_UV`, else `uv` on PATH.
    pub fn locate(home: &Home) -> Result<Self> {
        let exe = find_uv().ok_or_else(|| {
            miette!("cannot find uv; reinstall idf-next with `uv tool install idf-next` or set IDF_NEXT_UV")
        })?;
        Ok(Self {
            exe,
            home: home.clone(),
        })
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.exe);
        c.env("UV_CACHE_DIR", self.home.uv_cache())
            .env("UV_PYTHON_INSTALL_DIR", self.home.pythons())
            .env("UV_NO_CONFIG", "1")
            .env_remove("VIRTUAL_ENV")
            .env_remove("UV_PYTHON")
            .env_remove("UV_INDEX_URL")
            .env_remove("UV_EXTRA_INDEX_URL");
        // No `--quiet`: it also silences stdout that we parse (`python find`
        // prints nothing with it). Output is captured and shown on failure.
        c
    }

    fn run(&self, what: &str, mut cmd: Command) -> Result<String> {
        ui::detail("Running", format!("{cmd:?}"));
        let out = cmd
            .output()
            .into_diagnostic()
            .wrap_err_with(|| format!("running {}", self.exe.display()))?;
        if !out.status.success() {
            bail!(
                "{what} failed:\n{}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn version(&self) -> Result<String> {
        let mut c = Command::new(&self.exe);
        c.arg("--version");
        Ok(self.run("uv --version", c)?.trim().to_owned())
    }

    /// Path of a managed interpreter matching `request` ("3.12" or "3.12.11"),
    /// installing it unless offline.
    pub fn python(&self, request: &str, offline: bool) -> Result<PathBuf> {
        let find = |uv: &Self| {
            let mut c = uv.cmd();
            c.args(["python", "find", "--managed-python", request]);
            let out = uv.run("uv python find", c)?;
            match out.trim() {
                "" => bail!("`uv python find {request}` printed no path"),
                p => Ok(PathBuf::from(p)),
            }
        };
        if let Ok(p) = find(self) {
            return Ok(p);
        }
        if offline {
            bail!("Python {request} is not installed, and --offline was given");
        }
        ui::status("Installing", format!("Python {request}"));
        let mut c = self.cmd();
        c.args(["python", "install", request]);
        self.run(&format!("installing Python {request}"), c)?;
        find(self)
    }

    /// Resolve requirement files to a fully pinned, hashed requirements text.
    pub fn compile(
        &self,
        requirements: &[PathBuf],
        constraints: &Path,
        python_series: &str,
        exclude_newer: &str,
    ) -> Result<String> {
        let mut c = self.cmd();
        c.args([
            "pip",
            "compile",
            "--universal",
            "--generate-hashes",
            "--no-header",
            "--no-annotate",
        ]);
        c.args([
            "--python-version",
            python_series,
            "--exclude-newer",
            exclude_newer,
        ]);
        c.arg("--constraint").arg(constraints);
        c.args(requirements);
        self.run("resolving Python requirements", c)
    }

    pub fn create_venv(&self, python: &Path, dir: &Path) -> Result<()> {
        let mut c = self.cmd();
        c.arg("venv").arg("--python").arg(python).arg(dir);
        self.run("creating the venv", c).map(drop)
    }

    pub fn sync(&self, venv_python: &Path, requirements: &Path, offline: bool) -> Result<()> {
        let mut c = self.cmd();
        c.args(["pip", "sync", "--require-hashes", "--python"])
            .arg(venv_python)
            .arg(requirements);
        if offline {
            c.arg("--offline");
        }
        self.run("installing Python packages", c).map(drop)
    }
}

fn find_uv() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("IDF_NEXT_UV").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
    }
    // `uv tool install` links `idf` into ~/.local/bin; the real binary, with
    // uv beside it, is in the tool's venv.
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| fs::canonicalize(p).ok());
    if let Some(dir) = exe.as_deref().and_then(Path::parent) {
        let uv = dir.join("uv");
        if uv.is_file() {
            return Some(uv);
        }
    }
    which(OsStr::new("uv"))
}

pub fn which(name: &OsStr) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

// -------------------------------------------------------------- env setup ---

/// Record written into a finished Python env (`env.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct EnvRecord {
    pub python: LockedPython,
    pub uv: String,
}

pub struct PythonEnv {
    pub dir: PathBuf,
    pub record: EnvRecord,
}

impl PythonEnv {
    /// `IDF_TOOLS_PATH` for this env: holds the constraints file IDF checks against.
    pub fn tools_path(&self) -> &Path {
        &self.dir
    }

    pub fn venv(&self) -> PathBuf {
        self.dir.join("venv")
    }

    pub fn python(&self) -> PathBuf {
        self.venv().join("bin").join("python")
    }

    pub fn bin(&self) -> PathBuf {
        self.venv().join("bin")
    }
}

pub fn env_dir(sdk: &Path, requirements: &Digest) -> PathBuf {
    let hex = requirements.as_str().trim_start_matches("sha256:");
    sdk.join("python").join(&hex[..16])
}

/// Load a finished env, if present.
pub fn existing(sdk: &Path, requirements: &Digest) -> Option<PythonEnv> {
    let dir = env_dir(sdk, requirements);
    let text = fs::read_to_string(dir.join("env.toml")).ok()?;
    let record: EnvRecord = toml::from_str(&text).ok()?;
    (record.python.requirements == *requirements).then_some(PythonEnv { dir, record })
}

pub struct Inputs<'a> {
    pub home: &'a Home,
    pub net: &'a Net,
    pub uv: &'a Uv,
    pub sdk: &'a Path,
    pub idf_source: &'a Path,
    /// IDF release series, e.g. "6.0".
    pub series: &'a str,
}

/// Make sure the SDK has a Python env. With `locked`, reproduce exactly that
/// resolution; without, resolve now and return the new lock entry.
pub fn ensure(inputs: &Inputs, locked: Option<&LockedPython>) -> Result<PythonEnv> {
    let Inputs {
        home,
        net,
        uv,
        sdk,
        idf_source,
        series,
    } = inputs;
    let store = home.downloads();

    let (python_request, exclude_newer, constraints_text) = match locked {
        Some(l) => {
            if let Some(env) = existing(sdk, &l.requirements) {
                return Ok(env);
            }
            let text = constraints_by_digest(net, &store, series, &l.constraints)?;
            (l.version.to_string(), l.exclude_newer.clone(), text)
        }
        None => {
            let text = net
                .get_text(&constraints_url(series))
                .wrap_err("downloading Espressif's Python constraints")?;
            let now = humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string();
            (DEFAULT_PYTHON.to_owned(), now, text)
        }
    };
    let constraints = Digest::of(constraints_text.as_bytes());
    put(
        &store,
        "constraints",
        &constraints,
        constraints_text.as_bytes(),
    )?;

    let interpreter = uv.python(&python_request, net.offline())?;
    let python_version = interpreter_version(&interpreter)?;
    let python_series = format!("{}.{}", python_version.major, python_version.minor);

    // The resolved requirements: from the store when we have them, otherwise
    // resolved again from the same inputs.
    let resolved = match locked.and_then(|l| get(&store, "python", &l.requirements)) {
        Some(text) => text,
        None => {
            if net.offline() {
                bail!(
                    "the resolved Python requirements are not in the download cache, and --offline was given"
                );
            }
            let spinner = ui::spinner(format!("Resolving Python packages for ESP-IDF {series}"));
            let scratch = tempfile::tempdir()
                .into_diagnostic()
                .wrap_err("creating a temporary directory")?;
            let cfile = scratch.path().join(constraints_file_name(series));
            crate::fsx::write(&cfile, &constraints_text)?;
            let reqs = requirement_files(idf_source)?;
            let text = uv.compile(&reqs, &cfile, &python_series, &exclude_newer);
            spinner.finish_and_clear();
            text?
        }
    };
    let requirements = Digest::of(resolved.as_bytes());
    if let Some(l) = locked
        && requirements != l.requirements
    {
        bail!(
            "resolving the Python environment again gave a different result than idf.lock records\n  \
                 locked {}\n  got    {requirements}\n\
                 Upstream inputs changed since the lock was written. Run `idf sdk use` to re-pin.",
            l.requirements
        );
    }
    put(&store, "python", &requirements, resolved.as_bytes())?;

    let record = EnvRecord {
        python: LockedPython {
            version: python_version,
            exclude_newer,
            constraints,
            requirements: requirements.clone(),
        },
        uv: uv.version()?,
    };
    if let Some(env) = existing(sdk, &requirements) {
        return Ok(env);
    }

    let dir = env_dir(sdk, &requirements);
    ui::status(
        "Creating",
        format!(
            "Python {} environment ({} packages)",
            record.python.version,
            count_pins(&resolved)
        ),
    );
    if dir.exists() {
        crate::fsx::remove_dir_all(&dir)?;
    }
    crate::fsx::create_dir_all(&dir)?;
    crate::fsx::write(dir.join(constraints_file_name(series)), &constraints_text)?;
    crate::fsx::write(dir.join("requirements.txt"), &resolved)?;
    let venv = dir.join("venv");
    uv.create_venv(&interpreter, &venv)?;
    let spinner = ui::spinner("Installing Python packages");
    let synced = uv.sync(
        &venv.join("bin").join("python"),
        &dir.join("requirements.txt"),
        net.offline(),
    );
    spinner.finish_and_clear();
    synced?;
    fs::write(
        dir.join("env.toml"),
        toml::to_string_pretty(&record).into_diagnostic()?,
    )
    .into_diagnostic()?;
    Ok(PythonEnv { dir, record })
}

/// The SDK's non-optional requirement files (from `tools/requirements.json`).
fn requirement_files(idf_source: &Path) -> Result<Vec<PathBuf>> {
    #[derive(Deserialize)]
    struct Features {
        features: Vec<Feature>,
    }
    #[derive(Deserialize)]
    struct Feature {
        optional: bool,
        requirement_path: String,
    }
    let path = idf_source.join("tools/requirements.json");
    let text = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let f: Features = serde_json::from_str(&text).into_diagnostic()?;
    Ok(f.features
        .into_iter()
        .filter(|f| !f.optional)
        .map(|f| idf_source.join(f.requirement_path))
        .collect())
}

fn interpreter_version(python: &Path) -> Result<semver::Version> {
    let out = Command::new(python)
        .args(["-c", "import platform; print(platform.python_version())"])
        .output()
        .into_diagnostic()
        .wrap_err_with(|| format!("running {}", python.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    semver::Version::parse(text.trim())
        .into_diagnostic()
        .wrap_err_with(|| format!("Python version {text:?}"))
}

fn count_pins(resolved: &str) -> usize {
    let mut names: Vec<&str> = resolved
        .lines()
        .filter(|l| !l.starts_with([' ', '#']) && l.contains("=="))
        .filter_map(|l| l.split("==").next())
        .collect();
    names.dedup();
    names.len()
}

fn constraints_by_digest(net: &Net, store: &Path, series: &str, digest: &Digest) -> Result<String> {
    if let Some(text) = get(store, "constraints", digest) {
        return Ok(text);
    }
    let text = net
        .get_text(&constraints_url(series))
        .wrap_err("downloading Espressif's Python constraints")?;
    if Digest::of(text.as_bytes()) != *digest {
        bail!(
            "Espressif's constraints file for ESP-IDF {series} changed since idf.lock was written, \
             and the original is not in the download cache. Run `idf sdk use` to re-pin."
        );
    }
    Ok(text)
}

fn store_file(store: &Path, kind: &str, digest: &Digest) -> PathBuf {
    store
        .join(kind)
        .join(format!("{}.txt", digest.as_str().replace(':', "-")))
}

fn get(store: &Path, kind: &str, digest: &Digest) -> Option<String> {
    let text = fs::read_to_string(store_file(store, kind, digest)).ok()?;
    (Digest::of(text.as_bytes()) == *digest).then_some(text)
}

fn put(store: &Path, kind: &str, digest: &Digest, data: &[u8]) -> Result<()> {
    let path = store_file(store, kind, digest);
    if path.exists() {
        return Ok(());
    }
    crate::fsx::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    crate::fsx::write(&tmp, data)?;
    crate::fsx::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_unique_pins() {
        let text = "click==8.1.8 \\\n    --hash=sha256:aa\nfoo==1 ; python_full_version < '3.16' \\\nfoo==2 ; python_full_version >= '3.16' \\\n";
        assert_eq!(count_pins(text), 2);
    }

    #[test]
    fn store_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let d = Digest::of(b"x");
        assert!(get(dir.path(), "python", &d).is_none());
        put(dir.path(), "python", &d, b"x").unwrap();
        assert_eq!(get(dir.path(), "python", &d).as_deref(), Some("x"));
    }
}
