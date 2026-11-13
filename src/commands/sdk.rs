use crate::cli::{GlobalArgs, SdkAction};
use crate::idf_support::{self, IdfSpec};
use crate::project::Project;
use crate::sdk::{self, Ctx, Installed, Release, Sdk, Target};
use crate::ui;
use miette::{Result, bail, miette};
use semver::Version;

pub fn run(ctx: &Ctx, global: &GlobalArgs, action: &SdkAction) -> Result<()> {
    match action {
        SdkAction::Install { version: Some(v) } => {
            let spec = parse_spec(v)?;
            let release = resolve_fresh(ctx, &spec)?;
            let installed = sdk::install(ctx, &Target::Fresh(release))?;
            ready(&installed);
            Ok(())
        }
        SdkAction::Install { version: None } => {
            let mut project = Project::require(global.project.as_deref(), "sdk install")?;
            let installed = ensure_project(ctx, &mut project, global.locked)?;
            ready(&installed);
            Ok(())
        }
        SdkAction::Use { version } => {
            let spec = parse_spec(version)?;
            let mut project = Project::require(global.project.as_deref(), "sdk use")?;
            if project.idf_spec() != spec {
                if global.locked {
                    bail!("`idf sdk use` would change idf.toml, but --locked was given");
                }
                project.set_idf_spec(spec)?;
                ui::status("Updated", format!("idf.toml: idf = \"{spec}\""));
            }
            let installed = ensure_project(ctx, &mut project, global.locked)?;
            ready(&installed);
            Ok(())
        }
        SdkAction::List => list(ctx, global),
        SdkAction::Remove { version } => remove(ctx, global, version),
        SdkAction::Path { version } => {
            let version = match version {
                Some(v) => exact_version(v)?,
                None => {
                    let project = Project::require(global.project.as_deref(), "sdk path")?;
                    let lock = project.current_lock().ok_or_else(|| {
                        miette!("idf.lock does not pin an SDK yet; run `idf sdk install`")
                    })?;
                    lock.idf.version.clone()
                }
            };
            let sdk = Sdk::load(&ctx.home, &version)?.ok_or_else(|| {
                miette!("ESP-IDF {version} is not installed; run `idf sdk install {version}`")
            })?;
            println!("{}", sdk.idf_path().display());
            Ok(())
        }
    }
}

fn ready(installed: &Installed) {
    ui::status(
        "Ready",
        format!(
            "ESP-IDF {} with Python {} at {}",
            installed.sdk.record.idf.version,
            installed.python.record.python.version,
            installed.sdk.dir.display()
        ),
    );
}

/// Install what the project's lock says, writing the lock first if needed.
pub fn ensure_project(ctx: &Ctx, project: &mut Project, locked: bool) -> Result<Installed> {
    if let Some(lock) = project.current_lock().cloned() {
        return sdk::install(ctx, &Target::Locked(&lock));
    }
    if locked {
        let why = if project.lock.is_some() {
            "does not match idf.toml"
        } else {
            "is missing"
        };
        bail!("idf.lock {why}, and --locked forbids updating it");
    }
    let spec = project.idf_spec();
    let release = resolve_fresh(ctx, &spec)?;
    let installed = sdk::install(ctx, &Target::Fresh(release))?;
    let lock = sdk::lock_for(&installed, project.lock.as_ref())?;
    project.save_lock(lock)?;
    ui::status(
        "Locked",
        format!("ESP-IDF {} in idf.lock", installed.sdk.record.idf.version),
    );
    Ok(installed)
}

/// Newest release matching `spec`: from GitHub, or from installed SDKs offline.
fn resolve_fresh(ctx: &Ctx, spec: &IdfSpec) -> Result<Release> {
    if ctx.net.offline() {
        let found = sdk::installed_versions(&ctx.home)
            .into_iter()
            .rev()
            .filter(|(v, complete)| *complete && spec.matches(v))
            .find_map(|(v, _)| Sdk::load(&ctx.home, &v).ok().flatten());
        return match found {
            Some(sdk) => Ok(Release {
                tag: crate::idf_source::tag_for(&sdk.record.idf.version),
                version: sdk.record.idf.version,
                commit: sdk.record.idf.commit,
            }),
            None => bail!("no installed SDK matches {spec}, and --offline was given"),
        };
    }
    let spinner = ui::spinner(format!("Resolving ESP-IDF {spec}"));
    let release = sdk::resolve_release(&ctx.net, spec);
    spinner.finish_and_clear();
    release
}

fn parse_spec(s: &str) -> Result<IdfSpec> {
    let spec: IdfSpec = s.parse().map_err(|e: String| miette!(e))?;
    if !idf_support::supports_spec(&spec) {
        bail!(
            "ESP-IDF {spec} is not supported by idf-next {}; supported: {}",
            env!("CARGO_PKG_VERSION"),
            idf_support::SUPPORTED_IDF_RANGE
        );
    }
    Ok(spec)
}

fn exact_version(s: &str) -> Result<Version> {
    let spec: IdfSpec = s.parse().map_err(|e: String| miette!(e))?;
    match spec.patch {
        Some(p) => Ok(Version::new(spec.major, spec.minor, p)),
        None => bail!("give an exact version such as {spec}.0"),
    }
}

fn list(ctx: &Ctx, global: &GlobalArgs) -> Result<()> {
    let current = Project::discover(global.project.as_deref())
        .ok()
        .flatten()
        .and_then(|p| p.current_lock().map(|l| l.idf.version.clone()));
    let versions = sdk::installed_versions(&ctx.home);
    if versions.is_empty() {
        ui::status(
            "Empty",
            format!(
                "no SDKs in {}; run `idf sdk install {}`",
                ctx.home.sdks().display(),
                idf_support::DEFAULT_IDF
            ),
        );
        return Ok(());
    }
    for (v, complete) in versions {
        let mut notes = Vec::new();
        if !complete {
            notes.push(format!("incomplete: `idf sdk install {v}` finishes it"));
        } else if let Some(sdk) = Sdk::load(&ctx.home, &v)? {
            let envs = sdk.python_envs();
            notes.push(match envs.as_slice() {
                [] => "no Python env".to_owned(),
                [one] => format!("Python {}", one.record.python.version),
                many => format!("{} Python envs", many.len()),
            });
        }
        if !idf_support::supports(&v) {
            notes.push("not supported by this idf-next".into());
        }
        if current.as_ref() == Some(&v) {
            notes.push("used by this project".into());
        }
        println!(
            "{v:<10} {}  ({})",
            ctx.home.sdk(&v).display(),
            notes.join(", ")
        );
    }
    Ok(())
}

fn remove(ctx: &Ctx, global: &GlobalArgs, version: &str) -> Result<()> {
    let version = exact_version(version)?;
    let dir = ctx.home.sdk(&version);
    if !dir.exists() {
        bail!("ESP-IDF {version} is not installed");
    }
    let _guard = sdk::lock_home(&ctx.home)?;
    std::fs::remove_dir_all(&dir).map_err(|e| miette!("removing {}: {e}", dir.display()))?;
    ui::status("Removed", format!("ESP-IDF {version} ({})", dir.display()));
    let used = Project::discover(global.project.as_deref())
        .ok()
        .flatten()
        .is_some_and(|p| p.current_lock().is_some_and(|l| l.idf.version == version));
    if used {
        ui::warn(
            "this project's idf.lock uses it; `idf sdk install` reinstalls it (offline works from the download cache)",
        );
    }
    Ok(())
}
