//! The global cache, `~/.idf-next/` (override with `IDF_NEXT_HOME`).
//!
//! ```text
//! ~/.idf-next/
//!   sdk/<version>/                  one SDK, installed and removed as a unit
//!     esp-idf/                      source (no .git; version.txt added)
//!     tools/<tool>/<version>/       toolchains from the SDK's tools.json
//!     python/<digest>/              one venv per resolved requirement set
//!       venv/
//!       espidf.constraints.vX.Y.txt
//!       requirements.txt
//!       env.toml                    written last: marks the env complete
//!     sdk.toml                      written last: marks source + tools complete
//!   downloads/                      content-addressed archives (offline reinstall)
//!   python/                         uv-managed interpreters
//!   components/                     component manager downloads (shared by projects)
//!   uv-cache/                       uv's wheel cache (hard-linked into venvs)
//! ```

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    pub fn from_env() -> miette::Result<Self> {
        if let Some(dir) = std::env::var_os("IDF_NEXT_HOME").filter(|v| !v.is_empty()) {
            return Ok(Self::at(PathBuf::from(dir)));
        }
        let home = std::env::home_dir()
            .ok_or_else(|| miette::miette!("cannot find the home directory; set IDF_NEXT_HOME"))?;
        Ok(Self::at(home.join(".idf-next")))
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sdks(&self) -> PathBuf {
        self.root.join("sdk")
    }

    pub fn sdk(&self, version: &semver::Version) -> PathBuf {
        self.sdks().join(version.to_string())
    }

    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn pythons(&self) -> PathBuf {
        self.root.join("python")
    }

    pub fn components(&self) -> PathBuf {
        self.root.join("components")
    }

    pub fn uv_cache(&self) -> PathBuf {
        self.root.join("uv-cache")
    }

    /// Lock file guarding all writes under the home directory.
    pub fn lock_path(&self) -> PathBuf {
        self.root.join(".lock")
    }
}
