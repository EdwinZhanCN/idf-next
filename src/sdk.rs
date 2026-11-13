//! Installing, locating and describing SDKs (source + toolchains + venv).

use crate::archive;
use crate::home::Home;
use crate::idf_source;
use crate::idf_support::{self, IdfSpec};
use crate::lockfile::{self, Archive, CommitHash, Digest, LockedIdf, LockedTool, Lockfile};
use crate::net::{self, Expected, Net};
use crate::platform;
use crate::python::{self, PythonEnv, Uv};
use crate::tools_json::{Selected, ToolsJson};
use crate::ui;
use miette::{Context, IntoDiagnostic, Result, bail, miette};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Ctx {
    pub home: Home,
    pub net: Net,
}

// ---------------------------------------------------------------- release ---

#[derive(Debug, Clone)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub commit: CommitHash,
}

/// Parse a release tag (`v6.0`, `v6.0.3`); pre-releases are ignored.
pub fn parse_tag(tag: &str) -> Option<Version> {
    let spec: IdfSpec = tag.parse().ok()?;
    Some(Version::new(
        spec.major,
        spec.minor,
        spec.patch.unwrap_or(0),
    ))
}

/// The newest release matching `spec`, from GitHub.
pub fn resolve_release(net: &Net, spec: &IdfSpec) -> Result<Release> {
    let refs = net.github_json(&format!(
        "/repos/espressif/esp-idf/git/matching-refs/tags/v{}.{}",
        spec.major, spec.minor
    ))?;
    let mut best: Option<(Version, String, String, String)> = None;
    for r in refs.as_array().into_iter().flatten() {
        let Some(tag) = r["ref"].as_str().and_then(|s| s.strip_prefix("refs/tags/")) else {
            continue;
        };
        let Some(version) = parse_tag(tag) else {
            continue;
        };
        if !spec.matches(&version) || best.as_ref().is_some_and(|b| b.0 >= version) {
            continue;
        }
        let sha = r["object"]["sha"].as_str().unwrap_or_default().to_owned();
        let kind = r["object"]["type"].as_str().unwrap_or_default().to_owned();
        best = Some((version, tag.to_owned(), sha, kind));
    }
    let Some((version, tag, sha, kind)) = best else {
        bail!("no ESP-IDF release matches {spec}");
    };
    let commit = if kind == "tag" {
        let tag_obj = net.github_json(&format!("/repos/espressif/esp-idf/git/tags/{sha}"))?;
        tag_obj["object"]["sha"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    } else {
        sha
    };
    let commit = commit
        .parse()
        .map_err(|e: String| miette!("GitHub returned a bad commit for {tag}: {e}"))?;
    Ok(Release {
        version,
        tag,
        commit,
    })
}

// ----------------------------------------------------------------- record ---

/// `sdk.toml`: written when source and tools are complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SdkRecord {
    pub idf: LockedIdf,
    pub platform: String,
    #[serde(rename = "tool")]
    pub tools: Vec<InstalledTool>,
    pub installed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledTool {
    pub name: String,
    pub version: String,
    pub sha256: String,
}

/// An SDK whose source and tools are present.
pub struct Sdk {
    pub dir: PathBuf,
    pub record: SdkRecord,
    pub tools: Vec<Selected>,
}

impl Sdk {
    pub fn idf_path(&self) -> PathBuf {
        self.dir.join("esp-idf")
    }

    pub fn series(&self) -> String {
        series(&self.record.idf.version)
    }

    /// Load an installed SDK, or `None` if absent or incomplete.
    pub fn load(home: &Home, version: &Version) -> Result<Option<Self>> {
        let dir = home.sdk(version);
        let Ok(text) = fs::read_to_string(dir.join("sdk.toml")) else {
            return Ok(None);
        };
        let record: SdkRecord = toml::from_str(&text)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}/sdk.toml", dir.display()))?;
        let tools_json = ToolsJson::load(&dir.join("esp-idf/tools/tools.json"))?;
        let tools = tools_json.selected(&record.platform)?;
        Ok(Some(Self { dir, record, tools }))
    }

    /// Every complete Python env in this SDK, newest first.
    pub fn python_envs(&self) -> Vec<PythonEnv> {
        let mut envs: Vec<(std::time::SystemTime, PythonEnv)> =
            fs::read_dir(self.dir.join("python"))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let path = e.path().join("env.toml");
                    let mtime = fs::metadata(&path).and_then(|m| m.modified()).ok()?;
                    let record: python::EnvRecord =
                        toml::from_str(&fs::read_to_string(&path).ok()?).ok()?;
                    Some((
                        mtime,
                        PythonEnv {
                            dir: e.path(),
                            record,
                        },
                    ))
                })
                .collect();
        envs.sort_by(|a, b| b.0.cmp(&a.0));
        envs.into_iter().map(|(_, e)| e).collect()
    }
}

pub fn series(v: &Version) -> String {
    format!("{}.{}", v.major, v.minor)
}

/// Versions with an `sdk.toml`, plus directories left by interrupted installs.
pub fn installed_versions(home: &Home) -> Vec<(Version, bool)> {
    let mut out: Vec<(Version, bool)> = fs::read_dir(home.sdks())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let v = Version::parse(e.file_name().to_str()?).ok()?;
            Some((v, e.path().join("sdk.toml").is_file()))
        })
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------- install ---

/// What to install: exactly what a lock says, or a release resolved now.
pub enum Target<'a> {
    Locked(&'a Lockfile),
    Fresh(Release),
}

pub struct Installed {
    pub sdk: Sdk,
    pub python: PythonEnv,
}

/// Exclusive lock on the home directory for the duration of an install.
pub struct HomeLock(#[allow(dead_code)] fs::File);

pub fn lock_home(home: &Home) -> Result<HomeLock> {
    crate::fsx::create_dir_all(home.root())?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.lock_path())
        .into_diagnostic()?;
    match file.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => {
            ui::status(
                "Blocking",
                "waiting for another idf to finish with the SDK cache",
            );
            file.lock().into_diagnostic()?;
        }
        Err(fs::TryLockError::Error(e)) => return Err(e).into_diagnostic(),
    }
    Ok(HomeLock(file))
}

pub fn install(ctx: &Ctx, target: &Target) -> Result<Installed> {
    let _guard = lock_home(&ctx.home)?;
    let host = platform::host()?;
    let (version, tag, commit, locked) = match target {
        Target::Locked(l) => (
            l.idf.version.clone(),
            idf_source::tag_for(&l.idf.version),
            l.idf.commit.clone(),
            Some(*l),
        ),
        Target::Fresh(r) => (r.version.clone(), r.tag.clone(), r.commit.clone(), None),
    };
    if !idf_support::supports(&version) {
        bail!(
            "ESP-IDF {version} is outside the supported range {}",
            idf_support::SUPPORTED_IDF_RANGE
        );
    }
    let dir = ctx.home.sdk(&version);

    let sdk = match Sdk::load(&ctx.home, &version)? {
        Some(sdk) if locked.is_none_or(|l| sdk.record.idf == l.idf) => sdk,
        Some(sdk) => bail!(
            "the installed ESP-IDF {version} ({}) differs from idf.lock ({}); remove it with `idf sdk remove {version}`",
            sdk.record.idf.commit,
            locked.unwrap().idf.commit
        ),
        None => {
            ui::status(
                "Installing",
                format!("ESP-IDF {version} into {}", dir.display()),
            );
            install_source_and_tools(ctx, &dir, &version, &tag, commit, locked, host)?
        }
    };

    let series = sdk.series();
    let uv = Uv::locate(&ctx.home)?;
    let inputs = python::Inputs {
        home: &ctx.home,
        net: &ctx.net,
        uv: &uv,
        sdk: &dir,
        idf_source: &sdk.idf_path(),
        series: &series,
    };
    let python = match locked {
        Some(l) => python::ensure(&inputs, Some(&l.python))?,
        // Without a lock, reuse an env this SDK already has.
        None => match sdk.python_envs().into_iter().next() {
            Some(env) => env,
            None => python::ensure(&inputs, None)?,
        },
    };
    Ok(Installed { sdk, python })
}

fn install_source_and_tools(
    ctx: &Ctx,
    dir: &Path,
    version: &Version,
    tag: &str,
    commit: CommitHash,
    locked: Option<&Lockfile>,
    host: &str,
) -> Result<Sdk> {
    crate::fsx::create_dir_all(dir)?;

    // Source. The directory appears atomically, with its digest beside it.
    let src = dir.join("esp-idf");
    let digest_file = dir.join("source.digest");
    let source = match fs::read_to_string(&digest_file)
        .ok()
        .and_then(|s| s.trim().parse::<Digest>().ok())
    {
        Some(d) if src.is_dir() => d,
        _ => {
            if src.exists() {
                crate::fsx::remove_dir_all(&src)?;
            }
            let zip = idf_source::fetch(&ctx.net, &ctx.home.downloads(), tag)?;
            let d: Digest = idf_source::extract(&zip, tag, &src)?
                .parse()
                .map_err(|e: String| miette!(e))?;
            crate::fsx::write(&digest_file, format!("{d}\n"))?;
            d
        }
    };
    if let Some(l) = locked
        && l.idf.source != source
    {
        bail!(
            "ESP-IDF {version} source does not match idf.lock\n  locked {}\n  got    {source}",
            l.idf.source
        );
    }

    // Tools from the SDK's own tools.json.
    let tools_json = ToolsJson::load(&src.join("tools/tools.json"))?;
    let tools = tools_json.selected(host)?;
    if let Some(l) = locked {
        check_tools_against_lock(&tools, &l.tools, host)?;
    }
    install_tools(ctx, dir, &tools)?;

    let record = SdkRecord {
        idf: LockedIdf {
            version: version.clone(),
            commit,
            source,
        },
        platform: host.to_owned(),
        tools: tools
            .iter()
            .map(|t| InstalledTool {
                name: t.name.clone(),
                version: t.version.clone(),
                sha256: t.download.sha256.clone(),
            })
            .collect(),
        installed_by: format!("idf-next {}", env!("CARGO_PKG_VERSION")),
    };
    fs::write(
        dir.join("sdk.toml"),
        toml::to_string_pretty(&record).into_diagnostic()?,
    )
    .into_diagnostic()?;
    Ok(Sdk {
        dir: dir.to_path_buf(),
        record,
        tools,
    })
}

fn check_tools_against_lock(tools: &[Selected], locked: &[LockedTool], host: &str) -> Result<()> {
    let want: BTreeMap<&str, (&str, &str)> = tools
        .iter()
        .map(|t| {
            (
                t.name.as_str(),
                (t.version.as_str(), t.download.sha256.as_str()),
            )
        })
        .collect();
    let have: BTreeMap<&str, (&str, &str)> = locked
        .iter()
        .filter_map(|t| {
            t.archives
                .get(host)
                .map(|a| (t.name.as_str(), (t.version.as_str(), a.sha256.as_str())))
        })
        .collect();
    if want != have {
        bail!(
            "the SDK's tools.json does not match the tools in idf.lock for {host}; the lock may be from another idf-next version"
        );
    }
    Ok(())
}

fn install_tools(ctx: &Ctx, sdk: &Path, tools: &[Selected]) -> Result<()> {
    let todo: Vec<&Selected> = tools
        .iter()
        .filter(|t| !t.install_dir(sdk).is_dir())
        .collect();
    if todo.is_empty() {
        return Ok(());
    }
    let total: u64 = todo.iter().map(|t| t.download.size).sum();
    ui::status(
        "Installing",
        format!("{} tools ({})", todo.len(), ui::bytes(total)),
    );
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| -> Result<()> {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                s.spawn(|| -> Result<()> {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(tool) = todo.get(i) else {
                            return Ok(());
                        };
                        install_tool(ctx, sdk, tool).wrap_err_with(|| {
                            format!("installing {} {}", tool.name, tool.version)
                        })?;
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().map_err(|_| miette!("install thread panicked"))??;
        }
        Ok(())
    })
}

fn install_tool(ctx: &Ctx, sdk: &Path, tool: &Selected) -> Result<()> {
    let d = &tool.download;
    let file_name = d.file_name();
    let format = archive::Format::from_name(file_name)
        .ok_or_else(|| miette!("unknown archive format: {file_name}"))?;
    let stored = net::store_path(&ctx.home.downloads(), &d.sha256, file_name);
    let expected = Expected {
        sha256: d.sha256.clone(),
        size: d.size,
    };
    ctx.net
        .download_verified(&net::mirror(&d.url), &expected, &stored, &tool.name)?;
    archive::unpack(&stored, format, &tool.install_dir(sdk), tool.strip)?;
    ui::status("Installed", format!("{} {}", tool.name, tool.version));
    Ok(())
}

/// The lock entry for what was just installed: tools for every supported
/// host platform, so the lock works across machines.
pub fn lock_for(installed: &Installed, previous: Option<&Lockfile>) -> Result<Lockfile> {
    let tools_json = ToolsJson::load(&installed.sdk.idf_path().join("tools/tools.json"))?;
    let mut tools: BTreeMap<String, LockedTool> = BTreeMap::new();
    for plat in platform::SUPPORTED {
        for sel in tools_json.selected(plat)? {
            let entry = tools.entry(sel.name.clone()).or_insert_with(|| LockedTool {
                name: sel.name.clone(),
                version: sel.version.clone(),
                archives: BTreeMap::new(),
            });
            entry.archives.insert(
                (*plat).to_owned(),
                Archive {
                    url: sel.download.url.clone(),
                    sha256: sel
                        .download
                        .sha256
                        .parse()
                        .map_err(|e: String| miette!("tools.json: {e}"))?,
                    size: sel.download.size,
                },
            );
        }
    }
    Ok(Lockfile {
        version: lockfile::FORMAT_VERSION,
        idf: installed.sdk.record.idf.clone(),
        python: installed.python.record.python.clone(),
        tools: tools.into_values().collect(),
        components: previous.map(|p| p.components.clone()).unwrap_or_default(),
    })
}

// -------------------------------------------------------------------- env ---

/// The environment upstream tools need, as `export.sh` would set it.
pub struct Environment {
    pub vars: Vec<(String, String)>,
    pub path: Vec<PathBuf>,
}

impl Installed {
    pub fn environment(&self) -> Environment {
        let sdk = &self.sdk;
        let mut vars = vec![
            (
                "IDF_PATH".to_owned(),
                sdk.idf_path().to_string_lossy().into_owned(),
            ),
            (
                "IDF_TOOLS_PATH".to_owned(),
                self.python.tools_path().to_string_lossy().into_owned(),
            ),
            (
                "IDF_PYTHON_ENV_PATH".to_owned(),
                self.python.venv().to_string_lossy().into_owned(),
            ),
            ("ESP_IDF_VERSION".to_owned(), sdk.series()),
        ];
        let mut path = vec![self.python.bin()];
        for tool in &sdk.tools {
            path.extend(tool.paths(&sdk.dir));
            vars.extend(tool.vars(&sdk.dir));
        }
        path.push(sdk.idf_path().join("tools"));
        Environment { vars, path }
    }
}

impl Environment {
    /// PATH with our directories in front of the current one.
    pub fn joined_path(&self) -> std::ffi::OsString {
        let current = std::env::var_os("PATH").unwrap_or_default();
        let dirs = self
            .path
            .iter()
            .cloned()
            .chain(std::env::split_paths(&current));
        std::env::join_paths(dirs).unwrap_or(current)
    }

    pub fn apply(&self, cmd: &mut std::process::Command) {
        for (k, v) in &self.vars {
            cmd.env(k, v);
        }
        cmd.env("PATH", self.joined_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_series() {
        assert_eq!(parse_tag("v6.0"), Some(Version::new(6, 0, 0)));
        assert_eq!(parse_tag("v6.0.3"), Some(Version::new(6, 0, 3)));
        assert_eq!(parse_tag("v6.0-rc1"), None);
        assert_eq!(parse_tag("v6.0-dev"), None);
        assert_eq!(series(&Version::new(6, 0, 3)), "6.0");
    }

    #[test]
    fn installed_versions_lists_complete_and_partial() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path().to_path_buf());
        fs::create_dir_all(home.sdk(&Version::new(6, 0, 3))).unwrap();
        fs::write(home.sdk(&Version::new(6, 0, 3)).join("sdk.toml"), "").unwrap();
        fs::create_dir_all(home.sdk(&Version::new(6, 0, 2))).unwrap();
        fs::create_dir_all(home.sdks().join("junk")).unwrap();
        assert_eq!(
            installed_versions(&home),
            [
                (Version::new(6, 0, 2), false),
                (Version::new(6, 0, 3), true)
            ]
        );
    }
}
