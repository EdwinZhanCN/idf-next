//! Finding the project and reading/writing its two files.

use crate::idf_support::IdfSpec;
use crate::lockfile::{self, Lockfile};
use crate::manifest::{self, ConfigKey, ConfigValue, Manifest};
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use std::path::{Path, PathBuf};

pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub lock: Option<Lockfile>,
}

impl Project {
    /// Look for `idf.toml` in `start` (or the current directory) and its
    /// parents.
    pub fn discover(start: Option<&Path>) -> Result<Option<Self>> {
        let start = match start {
            Some(p) => {
                if !p.is_dir() {
                    bail!("{} is not a directory", p.display());
                }
                p.canonicalize().into_diagnostic()?
            }
            None => std::env::current_dir().into_diagnostic()?,
        };
        let Some(root) = start
            .ancestors()
            .find(|d| d.join(manifest::FILE_NAME).is_file())
        else {
            return Ok(None);
        };
        Ok(Some(Self::load(root)?))
    }

    /// Like [`discover`](Self::discover), but a project is required.
    pub fn require(start: Option<&Path>, command: &str) -> Result<Self> {
        Self::discover(start)?.ok_or_else(|| {
            miette::miette!(
                help = "run it inside a project, or point at one with -C <PATH>",
                "`idf {command}` needs a project, but no {} was found here or in any parent directory",
                manifest::FILE_NAME
            )
        })
    }

    pub fn load(root: &Path) -> Result<Self> {
        let manifest = Manifest::load(&root.join(manifest::FILE_NAME))?;
        let lock_path = root.join(lockfile::FILE_NAME);
        let lock = if lock_path.is_file() {
            Some(Lockfile::load(&lock_path)?)
        } else {
            None
        };
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
            lock,
        })
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join(manifest::FILE_NAME)
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join(lockfile::FILE_NAME)
    }

    pub fn idf_spec(&self) -> IdfSpec {
        *self.manifest.project.idf.get_ref()
    }

    /// The lock, if it still agrees with `idf.toml`.
    pub fn current_lock(&self) -> Option<&Lockfile> {
        self.lock
            .as_ref()
            .filter(|l| self.idf_spec().matches(&l.idf.version))
    }

    pub fn save_lock(&mut self, lock: Lockfile) -> Result<()> {
        lock.save(&self.lock_path())
            .into_diagnostic()
            .wrap_err("writing idf.lock")?;
        self.lock = Some(lock);
        Ok(())
    }

    /// Change `[project] idf` in idf.toml, keeping the rest of the file as written.
    pub fn set_idf_spec(&mut self, spec: IdfSpec) -> Result<()> {
        self.edit_manifest(|doc| doc["project"]["idf"] = toml_edit::value(spec.to_string()))
    }

    pub fn set_target(&mut self, chip: &crate::chip::Chip) -> Result<()> {
        self.edit_manifest(|doc| doc["project"]["target"] = toml_edit::value(chip.as_str()))
    }

    /// Set (`Some`) or remove (`None`) entries of `[config]`, or of
    /// `[profile.<name>.config]` when `profile` is given.
    pub fn edit_config(
        &mut self,
        profile: Option<&str>,
        edits: &[(ConfigKey, Option<ConfigValue>)],
    ) -> Result<()> {
        self.edit_manifest(|doc| {
            let root = doc.as_table_mut();
            let table = match profile {
                None => sub_table(root, "config", false),
                Some(name) => {
                    let profiles = sub_table(root, "profile", true);
                    let profile = sub_table(profiles, name, true);
                    sub_table(profile, "config", false)
                }
            };
            for (key, value) in edits {
                match value {
                    Some(v) => {
                        let mut v: toml_edit::Value = match v {
                            ConfigValue::Bool(b) => (*b).into(),
                            ConfigValue::Int(n) => (*n).into(),
                            ConfigValue::String(s) => s.as_str().into(),
                        };
                        // Keep a comment after the old value.
                        if let Some(old) = table.get(key.as_str()).and_then(|i| i.as_value()) {
                            *v.decor_mut() = old.decor().clone();
                        }
                        table.insert(key.as_str(), toml_edit::Item::Value(v));
                    }
                    None => {
                        table.remove(key.as_str());
                    }
                }
            }
        })
    }

    /// Change `idf.toml` in place, keeping its comments and formatting. The
    /// result is validated before it is written, so a bad edit never reaches
    /// disk.
    fn edit_manifest(&mut self, edit: impl FnOnce(&mut toml_edit::DocumentMut)) -> Result<()> {
        let path = self.manifest_path();
        let text = crate::fsx::read_to_string(&path)?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .into_diagnostic()
            .wrap_err("parsing idf.toml")?;
        edit(&mut doc);
        let new = doc.to_string();
        let manifest = Manifest::parse(&new, &path.display().to_string())?;
        crate::fsx::write(&path, new)?;
        self.manifest = manifest;
        Ok(())
    }
}

/// The table at `key` in `parent`, created if missing. An `implicit` table
/// prints no header of its own (`[profile]` above `[profile.dev.config]`).
fn sub_table<'a>(
    parent: &'a mut dyn toml_edit::TableLike,
    key: &str,
    implicit: bool,
) -> &'a mut dyn toml_edit::TableLike {
    let item = parent.entry(key).or_insert_with(|| {
        let mut t = toml_edit::Table::new();
        t.set_implicit(implicit);
        toml_edit::Item::Table(t)
    });
    if item.as_table_like().is_none() {
        *item = toml_edit::table();
    }
    item.as_table_like_mut().expect("a table")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str =
        "# my project\n[project]\nname = \"blink\"  # name\ntarget = \"esp32\"\nidf = \"6.0\"\n";

    #[test]
    fn discovers_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("idf.toml"), TOML).unwrap();
        std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
        let p = Project::discover(Some(&dir.path().join("src/deep")))
            .unwrap()
            .unwrap();
        assert_eq!(p.root, dir.path().canonicalize().unwrap());
        assert!(p.lock.is_none());
        let empty = tempfile::tempdir().unwrap();
        assert!(Project::discover(Some(empty.path())).unwrap().is_none());
    }

    #[test]
    fn set_idf_keeps_formatting() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("idf.toml"), TOML).unwrap();
        let mut p = Project::load(dir.path()).unwrap();
        p.set_idf_spec("6.0.3".parse().unwrap()).unwrap();
        let text = std::fs::read_to_string(dir.path().join("idf.toml")).unwrap();
        assert_eq!(text, TOML.replace("idf = \"6.0\"", "idf = \"6.0.3\""));
        assert_eq!(p.idf_spec().to_string(), "6.0.3");
        assert!(p.set_idf_spec("5.4".parse().unwrap()).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("idf.toml")).unwrap(),
            text
        );
    }

    #[test]
    fn edits_config_tables() {
        let dir = tempfile::tempdir().unwrap();
        let toml = format!("{TOML}\n[config]\nFREERTOS_HZ = 100  # tick\n");
        std::fs::write(dir.path().join("idf.toml"), &toml).unwrap();
        let mut p = Project::load(dir.path()).unwrap();
        let key = |k: &str| k.parse::<ConfigKey>().unwrap();
        p.edit_config(
            None,
            &[
                (key("FREERTOS_HZ"), Some(ConfigValue::Int(1000))),
                (
                    key("LOG_DEFAULT_LEVEL_DEBUG"),
                    Some(ConfigValue::Bool(true)),
                ),
            ],
        )
        .unwrap();
        p.edit_config(
            Some("release"),
            &[(
                key("COMPILER_OPTIMIZATION_SIZE"),
                Some(ConfigValue::Bool(true)),
            )],
        )
        .unwrap();
        p.edit_config(None, &[(key("LOG_DEFAULT_LEVEL_DEBUG"), None)])
            .unwrap();
        let text = std::fs::read_to_string(dir.path().join("idf.toml")).unwrap();
        assert_eq!(
            text,
            format!(
                "{TOML}\n[config]\nFREERTOS_HZ = 1000  # tick\n\n\
                 [profile.release.config]\nCOMPILER_OPTIMIZATION_SIZE = true\n"
            )
        );
        assert_eq!(p.manifest.effective_config("release").len(), 2);
    }
}
