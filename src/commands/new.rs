//! `idf new`, `idf init` and `idf target`.

use crate::chip::{Chip, KNOWN_CHIPS};
use crate::cli::{GlobalArgs, TargetAction};
use crate::idf_support::DEFAULT_IDF;
use crate::manifest::is_project_name;
use crate::project::Project;
use crate::{fsx, ui};
use miette::{Result, bail};
use std::path::Path;

const DEFAULT_CHIP: &str = "esp32";

/// `idf new <path>`: a project in a new directory.
pub fn new(global: &GlobalArgs, path: &Path) -> Result<()> {
    if path.exists() {
        bail!(
            help = "Use `idf init` inside an existing directory.",
            "{} already exists",
            path.display()
        );
    }
    create(global, path)
}

/// `idf init`: a project in the current (or `-C`) directory.
pub fn init(global: &GlobalArgs) -> Result<()> {
    let dir = match &global.project {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| miette::miette!("{e}"))?,
    };
    if dir.join("idf.toml").exists() {
        bail!("{} already has an idf.toml", dir.display());
    }
    create(global, &dir)
}

fn create(global: &GlobalArgs, dir: &Path) -> Result<()> {
    let abs = std::path::absolute(dir).map_err(|e| miette::miette!("{e}"))?;
    let name = abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    if !is_project_name(&name) {
        bail!(
            help = "Names start with a letter and use only letters, digits, `-` and `_`.",
            "`{name}` cannot be used as a project name"
        );
    }
    let chip = global
        .target
        .clone()
        .unwrap_or_else(|| DEFAULT_CHIP.parse().expect("valid chip"));

    fsx::write_new(abs.join("idf.toml"), manifest(&name, &chip))?;
    let has_sources = abs.join("src").is_dir()
        && std::fs::read_dir(abs.join("src")).is_ok_and(|mut d| d.next().is_some());
    if !has_sources {
        fsx::create_dir_all(abs.join("src"))?;
        fsx::write_new(abs.join("src/main.c"), main_c(&name))?;
    }
    let ignore = abs.join(".gitignore");
    if !ignore.exists() {
        fsx::write_new(&ignore, "/build/\n/compile_commands.json\n")?;
    }
    ui::status(
        "Created",
        format!(
            "{name} for {chip} (ESP-IDF {DEFAULT_IDF}) in {}",
            dir.display()
        ),
    );
    Ok(())
}

fn manifest(name: &str, chip: &Chip) -> String {
    format!("[project]\nname = \"{name}\"\ntarget = \"{chip}\"\nidf = \"{DEFAULT_IDF}\"\n")
}

fn main_c(name: &str) -> String {
    format!(
        "#include <stdio.h>\n\nvoid app_main(void)\n{{\n    printf(\"Hello from {name}!\\n\");\n}}\n"
    )
}

pub fn target(global: &GlobalArgs, action: &TargetAction) -> Result<()> {
    match action {
        TargetAction::Set { chip } => {
            let mut project = Project::require(global.project.as_deref(), "target set")?;
            if !chip.is_known() {
                ui::warn(format!(
                    "{chip} is not a chip ESP-IDF 6.0 supports as stable; the SDK will decide"
                ));
            }
            project.set_target(chip)?;
            ui::status("Target", format!("{chip} in idf.toml"));
            Ok(())
        }
        TargetAction::List => {
            let current = Project::discover(global.project.as_deref())?
                .map(|p| p.manifest.project.target.clone());
            for chip in KNOWN_CHIPS {
                let mark = if current.as_ref().is_some_and(|c| c.as_str() == *chip) {
                    "  (this project)"
                } else {
                    ""
                };
                println!("{chip}{mark}");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;

    #[test]
    fn template_manifest_is_valid() {
        let m =
            Manifest::parse(&manifest("blink", &"esp32c3".parse().unwrap()), "idf.toml").unwrap();
        assert_eq!(m.project.name.get_ref(), "blink");
        assert_eq!(m.project.target.as_str(), "esp32c3");
    }
}
