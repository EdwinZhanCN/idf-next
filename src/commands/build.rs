//! `idf build`, `idf clean` and `idf native`.

use crate::build::{self, Build, Layout};
use crate::cli::GlobalArgs;
use crate::commands::sdk::ensure_project;
use crate::project::Project;
use crate::sdk::{Ctx, Installed};
use crate::{fsx, ui};
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use std::time::Instant;

/// Everything a build-dir command needs: project, SDK and layout.
pub struct Prepared {
    pub project: Project,
    pub installed: Installed,
    pub layout: Layout,
}

impl Prepared {
    pub fn build(&self) -> Build<'_> {
        Build::new(self.layout.clone(), &self.installed)
    }
}

/// Load the project, make sure its SDK is installed, and pick target and
/// profile from the command line and `idf.toml`.
pub fn prepare(ctx: &Ctx, global: &GlobalArgs, command: &str) -> Result<Prepared> {
    let mut project = Project::require(global.project.as_deref(), command)?;
    let profile = global.profile_name();
    if !project.manifest.has_profile(profile) {
        bail!(
            help = "Built-in profiles are `dev` and `release`; others are declared as [profile.<name>] in idf.toml.",
            "no profile named `{profile}`"
        );
    }
    let target = build::target(&project.manifest, global.target.as_ref());
    let installed = ensure_project(ctx, &mut project, global.locked)?;
    let layout = Layout::new(&project.root, &target, profile);
    Ok(Prepared {
        project,
        installed,
        layout,
    })
}

/// Generate, configure and compile; returns the prepared build for `run`
/// and `flash` to continue with.
pub fn build(ctx: &Ctx, global: &GlobalArgs, command: &str) -> Result<Prepared> {
    let p = prepare(ctx, global, command)?;
    let started = Instant::now();
    let generated = build::generate(&p.project, &p.layout)?;
    let b = p.build();
    if b.configure(&generated)? {
        crate::commands::config::check_build(&p)?;
    }
    ui::status(
        "Building",
        format!(
            "{} for {} ({})",
            p.project.manifest.project.name.get_ref(),
            p.layout.target,
            p.layout.profile
        ),
    );
    b.compile()?;
    b.link_compile_commands()?;
    let image = match b.app_bin() {
        Some(bin) => {
            let size = std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0);
            let shown = bin.strip_prefix(&p.project.root).unwrap_or(&bin);
            format!("{} ({})", shown.display(), ui::bytes(size))
        }
        None => p.layout.dir().display().to_string(),
    };
    ui::status(
        "Finished",
        format!("{image} in {:.1}s", started.elapsed().as_secs_f64()),
    );
    Ok(p)
}

pub fn run_build(ctx: &Ctx, global: &GlobalArgs) -> Result<()> {
    build(ctx, global, "build").map(|_| ())
}

/// Remove this target and profile's build directory, or all of `build/`.
pub fn clean(global: &GlobalArgs, all: bool) -> Result<()> {
    let project = Project::require(global.project.as_deref(), "clean")?;
    let target = build::target(&project.manifest, global.target.as_ref());
    let layout = Layout::new(&project.root, &target, global.profile_name());
    let dir = if all {
        layout.build_root()
    } else {
        layout.dir()
    };
    if dir.exists() {
        fsx::remove_dir_all(&dir)?;
        let shown = dir.strip_prefix(&project.root).unwrap_or(&dir);
        ui::status("Removed", shown.display().to_string());
    }
    // A link into a removed build is dangling; drop it.
    let link = project.root.join("compile_commands.json");
    let dangling = std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink())
        && !link.exists();
    if dangling {
        std::fs::remove_file(&link)
            .into_diagnostic()
            .wrap_err_with(|| format!("removing {}", link.display()))?;
    }
    Ok(())
}

/// `idf.py` on the same generated project and build directory as `idf build`.
pub fn native(ctx: &Ctx, global: &GlobalArgs, args: &[String]) -> Result<()> {
    let p = prepare(ctx, global, "native")?;
    build::generate(&p.project, &p.layout)?;
    let b = p.build();
    let mut cmd = std::process::Command::new(p.installed.python.python());
    b.env().apply(&mut cmd);
    cmd.arg(p.installed.sdk.idf_path().join("tools/idf.py"))
        .arg("-C")
        .arg(p.layout.project_dir())
        .arg("-B")
        .arg(p.layout.dir())
        .args(args);
    ui::detail("Running", format!("{cmd:?}"));
    let status = cmd.status().into_diagnostic().wrap_err("running idf.py")?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
