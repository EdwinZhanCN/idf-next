//! ESP-IDF's `tools/tools.json`: which toolchain packages an SDK needs, where
//! to get them, and how to put them on PATH.

use miette::{IntoDiagnostic, Result, WrapErr, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct ToolsJson {
    pub version: u32,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub install: Install,
    #[serde(default)]
    pub export_paths: Vec<Vec<String>>,
    #[serde(default)]
    pub export_vars: BTreeMap<String, String>,
    #[serde(default)]
    pub strip_container_dirs: usize,
    #[serde(default)]
    pub platform_overrides: Vec<Override>,
    #[serde(default)]
    pub version_cmd: Vec<String>,
    #[serde(default)]
    pub version_regex: String,
    pub versions: Vec<ToolVersion>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Install {
    Always,
    OnRequest,
    #[default]
    Never,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Override {
    pub platforms: Vec<String>,
    pub install: Option<Install>,
    pub export_paths: Option<Vec<Vec<String>>>,
    pub export_vars: Option<BTreeMap<String, String>>,
    pub strip_container_dirs: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolVersion {
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(flatten)]
    pub downloads: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Download {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// File name to store the download under, when the URL's is ambiguous.
    pub rename_dist: Option<String>,
}

impl Download {
    pub fn file_name(&self) -> &str {
        self.rename_dist
            .as_deref()
            .unwrap_or_else(|| self.url.rsplit('/').next().unwrap_or(&self.url))
    }
}

/// Tools idf-next installs even though tools.json marks them `on_request` on
/// macOS and Linux: idf-next provides CMake and Ninja itself instead of
/// expecting them on the system.
pub const ALSO_INSTALL: &[&str] = &["cmake", "ninja"];

impl ToolsJson {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", path.display()))?;
        let parsed: Self = serde_json::from_str(&text)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {}", path.display()))?;
        if parsed.version != 3 {
            crate::ui::warn(format!(
                "{} has format version {}; idf-next was written against version 3",
                path.display(),
                parsed.version
            ));
        }
        Ok(parsed)
    }

    /// Tools to install on `platform`, each at its recommended version.
    pub fn selected(&self, platform: &str) -> Result<Vec<Selected>> {
        let mut out = Vec::new();
        for tool in &self.tools {
            let resolved = tool.for_platform(platform);
            let wanted =
                resolved.install == Install::Always || ALSO_INSTALL.contains(&tool.name.as_str());
            if !wanted {
                continue;
            }
            let Some(version) = tool.recommended() else {
                bail!("tools.json: {} has no recommended version", tool.name);
            };
            let Some(download) = version.download_for(platform)? else {
                // Not offered for this host (e.g. on_request-only tools).
                continue;
            };
            out.push(Selected {
                name: tool.name.clone(),
                version: version.name.clone(),
                download,
                strip: resolved.strip_container_dirs,
                export_paths: resolved.export_paths,
                export_vars: resolved.export_vars,
                version_cmd: tool.version_cmd.clone(),
                version_regex: tool.version_regex.clone(),
            });
        }
        Ok(out)
    }

    pub fn tool(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }
}

/// Tool fields after applying the overrides for one platform.
pub struct Resolved {
    pub install: Install,
    pub export_paths: Vec<Vec<String>>,
    pub export_vars: BTreeMap<String, String>,
    pub strip_container_dirs: usize,
}

impl Tool {
    pub fn for_platform(&self, platform: &str) -> Resolved {
        let mut r = Resolved {
            install: self.install,
            export_paths: self.export_paths.clone(),
            export_vars: self.export_vars.clone(),
            strip_container_dirs: self.strip_container_dirs,
        };
        for o in self
            .platform_overrides
            .iter()
            .filter(|o| o.platforms.iter().any(|p| p == platform))
        {
            if let Some(v) = o.install {
                r.install = v;
            }
            if let Some(v) = &o.export_paths {
                r.export_paths = v.clone();
            }
            if let Some(v) = &o.export_vars {
                r.export_vars = v.clone();
            }
            if let Some(v) = o.strip_container_dirs {
                r.strip_container_dirs = v;
            }
        }
        r
    }

    pub fn recommended(&self) -> Option<&ToolVersion> {
        self.versions
            .iter()
            .find(|v| v.status == "recommended")
            .or(self.versions.first())
    }
}

impl ToolVersion {
    /// The archive for `platform`, falling back to a platform-independent `any`.
    pub fn download_for(&self, platform: &str) -> Result<Option<Download>> {
        let value = self
            .downloads
            .get(platform)
            .or_else(|| self.downloads.get("any"));
        match value {
            None => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .into_diagnostic(),
        }
    }
}

/// One tool chosen for installation.
#[derive(Debug, Clone)]
pub struct Selected {
    pub name: String,
    pub version: String,
    pub download: Download,
    pub strip: usize,
    pub export_paths: Vec<Vec<String>>,
    pub export_vars: BTreeMap<String, String>,
    pub version_cmd: Vec<String>,
    pub version_regex: String,
}

impl Selected {
    /// Where this tool is unpacked inside an SDK.
    pub fn install_dir(&self, sdk: &Path) -> PathBuf {
        tool_dir(sdk, &self.name, &self.version)
    }

    /// Directories to put on PATH.
    pub fn paths(&self, sdk: &Path) -> Vec<PathBuf> {
        let base = self.install_dir(sdk);
        self.export_paths
            .iter()
            .map(|parts| {
                parts
                    .iter()
                    .filter(|p| !p.is_empty())
                    .fold(base.clone(), |acc, p| acc.join(p))
            })
            .collect()
    }

    /// Extra environment variables, with `${TOOL_PATH}` expanded.
    pub fn vars(&self, sdk: &Path) -> Vec<(String, String)> {
        let base = self.install_dir(sdk);
        let base = base.to_string_lossy();
        self.export_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.replace("${TOOL_PATH}", &base)))
            .collect()
    }
}

pub fn tool_dir(sdk: &Path, name: &str, version: &str) -> PathBuf {
    sdk.join("tools").join(name).join(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "version": 3,
      "tools": [
        {
          "name": "xtensa-esp-elf", "install": "always",
          "export_paths": [["xtensa-esp-elf", "bin"]], "export_vars": {},
          "versions": [{"name": "esp-15.2.0_20251204", "status": "recommended",
            "macos-arm64": {"sha256": "aa", "size": 1, "url": "https://github.com/x/xtensa.tar.xz"},
            "linux-amd64": {"sha256": "bb", "size": 2, "url": "https://github.com/x/xtensa-linux.tar.xz"}}]
        },
        {
          "name": "cmake", "install": "on_request", "strip_container_dirs": 1,
          "export_paths": [["bin"]],
          "platform_overrides": [
            {"install": "always", "platforms": ["win64"]},
            {"export_paths": [["CMake.app", "Contents", "bin"]], "platforms": ["macos", "macos-arm64"]}
          ],
          "version_cmd": ["cmake", "--version"], "version_regex": "cmake version ([0-9.]+)",
          "versions": [
            {"name": "3.22.1", "status": "supported", "macos-arm64": {"sha256": "00", "size": 1, "url": "u"}},
            {"name": "4.0.3", "status": "recommended", "macos-arm64": {"sha256": "cc", "size": 3, "url": "https://github.com/k/cmake.tar.gz"}}
          ]
        },
        {
          "name": "esp-rom-elfs", "install": "always", "export_paths": [[""]],
          "export_vars": {"ESP_ROM_ELF_DIR": "${TOOL_PATH}/"},
          "versions": [{"name": "20260528", "status": "recommended", "any": {"sha256": "dd", "size": 4, "url": "https://github.com/e/rom.tar.gz"}}]
        },
        {
          "name": "ninja", "install": "on_request", "export_paths": [[""]],
          "versions": [{"name": "1.12.1", "status": "recommended",
            "macos-arm64": {"sha256": "ee", "size": 5, "url": "https://github.com/n/ninja-mac.zip", "rename_dist": "ninja-mac-v1.12.1.zip"}}]
        },
        {
          "name": "qemu-xtensa", "install": "on_request",
          "versions": [{"name": "9", "status": "recommended", "macos-arm64": {"sha256": "ff", "size": 6, "url": "q"}}]
        },
        {
          "name": "idf-exe", "install": "never",
          "platform_overrides": [{"install": "always", "platforms": ["win64"]}],
          "versions": [{"name": "1", "status": "recommended", "win64": {"sha256": "ff", "size": 6, "url": "q"}}]
        }
      ]
    }"#;

    fn tools() -> ToolsJson {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn selects_always_plus_cmake_and_ninja() {
        let sel = tools().selected("macos-arm64").unwrap();
        let names: Vec<_> = sel.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["xtensa-esp-elf", "cmake", "esp-rom-elfs", "ninja"]);
        let cmake = &sel[1];
        assert_eq!(cmake.version, "4.0.3");
        assert_eq!(cmake.strip, 1);
        let sdk = Path::new("/sdk");
        assert_eq!(
            cmake.paths(sdk),
            [PathBuf::from(
                "/sdk/tools/cmake/4.0.3/CMake.app/Contents/bin"
            )]
        );
        assert_eq!(sel[3].download.file_name(), "ninja-mac-v1.12.1.zip");
    }

    #[test]
    fn linux_has_no_cmake_override_and_skips_missing_platforms() {
        let sel = tools().selected("linux-amd64").unwrap();
        let names: Vec<_> = sel.iter().map(|s| s.name.as_str()).collect();
        // cmake/ninja fixtures only list macos-arm64 archives.
        assert_eq!(names, ["xtensa-esp-elf", "esp-rom-elfs"]);
        assert_eq!(
            sel[0].download.url,
            "https://github.com/x/xtensa-linux.tar.xz"
        );
    }

    #[test]
    fn expands_tool_path() {
        let sel = tools().selected("macos-arm64").unwrap();
        let rom = &sel[2];
        assert_eq!(
            rom.vars(Path::new("/sdk")),
            [(
                "ESP_ROM_ELF_DIR".to_string(),
                "/sdk/tools/esp-rom-elfs/20260528/".to_string()
            )]
        );
        assert_eq!(
            rom.paths(Path::new("/sdk")),
            [PathBuf::from("/sdk/tools/esp-rom-elfs/20260528")]
        );
    }

    #[test]
    fn parses_real_v6_0_tools_json_shape() {
        // Spot-check against fields seen in ESP-IDF v6.0.3's tools.json.
        let t = tools();
        assert_eq!(
            t.tool("idf-exe").unwrap().for_platform("win64").install,
            Install::Always
        );
        assert_eq!(
            t.tool("idf-exe")
                .unwrap()
                .for_platform("macos-arm64")
                .install,
            Install::Never
        );
    }
}
