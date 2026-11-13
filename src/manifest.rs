//! `idf.toml`: the single source of truth for a project.
//!
//! ```toml
//! [project]
//! name = "blink"
//! target = "esp32s3"
//! idf = "6.0"
//!
//! [config]
//! FREERTOS_HZ = 1000
//!
//! [dependencies]
//! "espressif/led_strip" = "*"
//!
//! [[partition]]
//! name = "nvs"
//! type = "data"
//! subtype = "nvs"
//! size = "24K"
//!
//! [profile.release.config]
//! COMPILER_OPTIMIZATION_PERF = true
//! ```

use crate::chip::Chip;
use crate::diagnostic::{self, FileError, Source};
use crate::idf_support::{self, IdfSpec};
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;
use toml::Spanned;

pub const FILE_NAME: &str = "idf.toml";

/// Profiles that exist without being declared. `dev` is the default;
/// `-r/--release` selects `release`.
pub const BUILTIN_PROFILES: &[&str] = &["dev", "release"];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub project: Project,
    /// Kconfig symbols without the `CONFIG_` prefix.
    #[serde(default)]
    pub config: BTreeMap<ConfigKey, ConfigValue>,
    /// Component name → requirement. Registry names are `namespace/name`.
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
    /// The partition table, in order.
    #[serde(default, rename = "partition")]
    pub partitions: Vec<Partition>,
    #[serde(default)]
    pub profile: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: Spanned<String>,
    pub target: Chip,
    pub idf: Spanned<IdfSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Overrides layered on top of the top-level `[config]`.
    #[serde(default)]
    pub config: BTreeMap<ConfigKey, ConfigValue>,
}

// ---------------------------------------------------------------- config ---

/// A Kconfig symbol name as written in `idf.toml` (no `CONFIG_` prefix).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigKey(String);

impl ConfigKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name as it appears in sdkconfig files.
    pub fn sdkconfig_name(&self) -> String {
        format!("CONFIG_{}", self.0)
    }
}

impl std::str::FromStr for ConfigKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = s.strip_prefix("CONFIG_") {
            return Err(format!("drop the CONFIG_ prefix: write {rest:?}"));
        }
        let valid = !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && !s.as_bytes()[0].is_ascii_digit();
        if valid {
            Ok(ConfigKey(s.to_owned()))
        } else {
            Err(format!(
                "{s:?} is not a Kconfig symbol; expected UPPER_SNAKE_CASE"
            ))
        }
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ConfigKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(de::Error::custom)
    }
}

/// A Kconfig value. `bool` maps to y/n, integers to int/hex, strings to string
/// (or to a `choice` member when the symbol is a choice).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged, expecting = "a boolean, an integer or a string")]
pub enum ConfigValue {
    Bool(bool),
    Int(i64),
    String(String),
}

// ---------------------------------------------------------- dependencies ---

/// One entry under `[dependencies]`: either a bare requirement string or a
/// table with exactly one source (`version`, `git` or `path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub source: DependencySource,
    /// Only pull the dependency in for these chips.
    pub targets: Option<Vec<Chip>>,
    /// Only pull the dependency in for this IDF series.
    pub idf: Option<IdfSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    /// From the component registry. The requirement string is kept verbatim;
    /// its exact grammar follows the component manager (checked in P4).
    Registry {
        version: String,
    },
    Git {
        url: String,
        rev: Option<String>,
        subdir: Option<String>,
    },
    Path {
        path: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DependencyTable {
    version: Option<String>,
    git: Option<String>,
    rev: Option<String>,
    subdir: Option<String>,
    path: Option<String>,
    targets: Option<Vec<Chip>>,
    idf: Option<IdfSpec>,
}

impl TryFrom<DependencyTable> for Dependency {
    type Error = String;

    fn try_from(t: DependencyTable) -> Result<Self, String> {
        let source = match (t.version, t.git, t.path) {
            (Some(version), None, None) => {
                if t.rev.is_some() || t.subdir.is_some() {
                    return Err("`rev` and `subdir` only apply to `git` dependencies".into());
                }
                DependencySource::Registry {
                    version: non_empty("version", version)?,
                }
            }
            (None, Some(url), None) => DependencySource::Git {
                url: non_empty("git", url)?,
                rev: t.rev,
                subdir: t.subdir,
            },
            (None, None, Some(path)) => {
                if t.rev.is_some() || t.subdir.is_some() {
                    return Err("`rev` and `subdir` only apply to `git` dependencies".into());
                }
                DependencySource::Path {
                    path: non_empty("path", path)?,
                }
            }
            (None, None, None) => {
                return Err("a dependency needs one of `version`, `git` or `path`".into());
            }
            _ => return Err("use only one of `version`, `git` or `path`".into()),
        };
        if t.targets.as_ref().is_some_and(Vec::is_empty) {
            return Err("`targets` is empty; remove it or list at least one chip".into());
        }
        Ok(Dependency {
            source,
            targets: t.targets,
            idf: t.idf,
        })
    }
}

fn non_empty(field: &str, value: String) -> Result<String, String> {
    if value.trim().is_empty() {
        Err(format!("`{field}` is empty"))
    } else {
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for Dependency {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Dependency;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a version requirement such as \"*\" or a table with `version`, `git` or `path`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Dependency, E> {
                Ok(Dependency {
                    source: DependencySource::Registry {
                        version: non_empty("version", v.to_owned()).map_err(E::custom)?,
                    },
                    targets: None,
                    idf: None,
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Dependency, A::Error> {
                let table =
                    DependencyTable::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Dependency::try_from(table).map_err(de::Error::custom)
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for IdfSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(de::Error::custom)
    }
}

// ------------------------------------------------------------ partitions ---

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Partition {
    pub name: Spanned<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub subtype: String,
    pub size: ByteSize,
    /// Omit to let the partition follow the previous one (with alignment).
    pub offset: Option<ByteSize>,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub readonly: bool,
}

/// Longest partition name gen_esp32part accepts (16 bytes incl. NUL).
pub const MAX_PARTITION_NAME: usize = 15;

/// A byte count written as an integer, `"0x…"`, or with a `K`/`M` suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteSize(pub u64);

impl std::str::FromStr for ByteSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || format!("{s:?} is not a size; use bytes, \"0x…\", \"24K\" or \"1M\"");
        let t = s.trim();
        let (digits, mul) = match t.as_bytes().last() {
            Some(b'K' | b'k') => (&t[..t.len() - 1], 1024),
            Some(b'M' | b'm') => (&t[..t.len() - 1], 1024 * 1024),
            _ => (t, 1),
        };
        let n = if let Some(hex) = digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"))
        {
            if mul != 1 {
                return Err(bad());
            }
            u64::from_str_radix(hex, 16).map_err(|_| bad())?
        } else {
            digits.parse::<u64>().map_err(|_| bad())?
        };
        n.checked_mul(mul).map(ByteSize).ok_or_else(bad)
    }
}

impl<'de> Deserialize<'de> for ByteSize {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = ByteSize;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a size such as 4096, \"0x1000\", \"24K\" or \"1M\"")
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<ByteSize, E> {
                u64::try_from(v)
                    .map(ByteSize)
                    .map_err(|_| E::custom("size cannot be negative"))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<ByteSize, E> {
                Ok(ByteSize(v))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<ByteSize, E> {
                v.parse().map_err(E::custom)
            }
        }
        d.deserialize_any(V)
    }
}

// ------------------------------------------------------------- loading ---

impl Manifest {
    /// Read and validate `idf.toml` at `path`.
    pub fn load(path: &Path) -> Result<Self, FileError> {
        let text = diagnostic::read(path)?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parse and validate manifest text. `name` is used in diagnostics.
    pub fn parse(text: &str, name: &str) -> Result<Self, FileError> {
        let src = Source { name, text };
        let manifest: Manifest = toml::from_str(text).map_err(|e| src.toml_error(e))?;
        manifest.validate(&src)?;
        Ok(manifest)
    }

    fn validate(&self, src: &Source) -> Result<(), FileError> {
        let name = &self.project.name;
        if !is_project_name(name.get_ref()) {
            return Err(src
                .error(
                    "invalid project name",
                    Some(name.span()),
                    "not a valid project name",
                )
                .with_help("use letters, digits, `-` and `_`, starting with a letter"));
        }

        let idf = &self.project.idf;
        if !idf_support::supports_spec(idf.get_ref()) {
            return Err(src
                .error(
                    format!(
                        "ESP-IDF {} is not supported by this idf-next",
                        idf.get_ref()
                    ),
                    Some(idf.span()),
                    "unsupported ESP-IDF release",
                )
                .with_help(format!(
                    "idf-next {} supports ESP-IDF {}",
                    env!("CARGO_PKG_VERSION"),
                    idf_support::SUPPORTED_IDF_RANGE
                )));
        }

        let mut seen: HashMap<&str, &Spanned<String>> = HashMap::new();
        for part in &self.partitions {
            let pname = part.name.get_ref();
            if pname.is_empty() || pname.len() > MAX_PARTITION_NAME || !pname.is_ascii() {
                return Err(src.error(
                    "invalid partition name",
                    Some(part.name.span()),
                    format!("must be 1–{MAX_PARTITION_NAME} ASCII characters"),
                ));
            }
            if let Some(first) = seen.insert(pname, &part.name) {
                let line = line_of(src.text, first.span().start);
                return Err(src
                    .error(
                        format!("duplicate partition {pname:?}"),
                        Some(part.name.span()),
                        format!("already defined on line {line}"),
                    )
                    .with_help("partition names must be unique"));
            }
        }

        Ok(())
    }

    /// `[config]` with the selected profile's overrides applied.
    pub fn effective_config(&self, profile: &str) -> BTreeMap<ConfigKey, ConfigValue> {
        let mut config = self.config.clone();
        if let Some(p) = self.profile.get(profile) {
            config.extend(p.config.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        config
    }

    /// Whether `profile` can be selected: built in, or declared under `[profile]`.
    pub fn has_profile(&self, profile: &str) -> bool {
        BUILTIN_PROFILES.contains(&profile) || self.profile.contains_key(profile)
    }
}

fn is_project_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const EXAMPLE: &str = r#"
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"

[config]
FREERTOS_HZ = 1000

[dependencies]
"espressif/led_strip" = "*"

[[partition]]
name = "nvs"
type = "data"
subtype = "nvs"
size = "24K"

[[partition]]
name = "factory"
type = "app"
subtype = "factory"
size = "1M"

[profile.release.config]
COMPILER_OPTIMIZATION_PERF = true
"#;

    fn parse(text: &str) -> Result<Manifest, FileError> {
        Manifest::parse(text, "idf.toml")
    }

    /// Message, label and the 1-based line the label points at.
    fn err(text: &str) -> (String, String, usize) {
        let err = parse(text).unwrap_err();
        let e = err
            .invalid()
            .unwrap_or_else(|| panic!("unexpected error: {err}"));
        let line = e.span.map_or(0, |s| line_of(text, s.offset()));
        (e.message.clone(), e.label.clone(), line)
    }

    fn with_project(rest: &str) -> String {
        format!("[project]\nname = \"a\"\ntarget = \"esp32\"\nidf = \"6.0\"\n{rest}")
    }

    #[test]
    fn plan_example_parses() {
        let m = parse(EXAMPLE).unwrap();
        assert_eq!(m.project.name.get_ref(), "blink");
        assert_eq!(m.project.target.as_str(), "esp32s3");
        assert_eq!(m.project.idf.get_ref().to_string(), "6.0");
        assert_eq!(
            m.config[&"FREERTOS_HZ".parse().unwrap()],
            ConfigValue::Int(1000)
        );
        assert_eq!(
            m.dependencies["espressif/led_strip"].source,
            DependencySource::Registry {
                version: "*".into()
            }
        );
        let names: Vec<_> = m
            .partitions
            .iter()
            .map(|p| p.name.get_ref().as_str())
            .collect();
        assert_eq!(names, ["nvs", "factory"]);
        assert_eq!(m.partitions[0].size, ByteSize(24 * 1024));
        assert_eq!(m.partitions[1].size, ByteSize(1024 * 1024));

        let release = m.effective_config("release");
        assert_eq!(release.len(), 2);
        assert_eq!(
            release[&"COMPILER_OPTIMIZATION_PERF".parse().unwrap()],
            ConfigValue::Bool(true)
        );
        assert_eq!(m.effective_config("dev").len(), 1);
        assert!(m.has_profile("release") && m.has_profile("dev") && !m.has_profile("fast"));
    }

    #[test]
    fn detailed_dependencies() {
        let m = parse(&with_project(
            r#"
[dependencies]
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"] }
mylib = { git = "https://example.com/mylib.git", rev = "v1.0", subdir = "comp" }
local = { path = "../local" }
"#,
        ))
        .unwrap();
        let mdns = &m.dependencies["espressif/mdns"];
        assert_eq!(mdns.targets.as_ref().unwrap().len(), 2);
        assert!(
            matches!(&m.dependencies["mylib"].source, DependencySource::Git { rev: Some(r), .. } if r == "v1.0")
        );
        assert!(matches!(
            &m.dependencies["local"].source,
            DependencySource::Path { .. }
        ));
    }

    #[test]
    fn rejects_bad_dependencies() {
        let (_, label, line) = err(&with_project(
            "[dependencies]\nx = { version = \"1\", path = \"p\" }\n",
        ));
        assert!(label.contains("only one of"), "{label}");
        assert_eq!(line, 6);
        let (_, label, _) = err(&with_project(
            "[dependencies]\nx = { path = \"p\", rev = \"a\" }\n",
        ));
        assert!(label.contains("only apply to `git`"), "{label}");
        let (_, label, _) = err(&with_project("[dependencies]\nx = {}\n"));
        assert!(label.contains("needs one of"), "{label}");
    }

    #[test]
    fn errors_point_at_the_line() {
        let (_, label, line) =
            err("[project]\nname = \"a\"\ntarget = \"esp8266\"\nidf = \"6.0\"\n");
        assert!(label.contains("not a chip name"), "{label}");
        assert_eq!(line, 3);

        let (msg, _, line) = err("[project]\nname = \"a\"\ntarget = \"esp32\"\nidf = \"5.3\"\n");
        assert!(msg.contains("ESP-IDF 5.3 is not supported"), "{msg}");
        assert_eq!(line, 4);

        let (_, label, line) = err(&with_project("[config]\nCONFIG_FOO = 1\n"));
        assert!(label.contains("drop the CONFIG_ prefix"), "{label}");
        assert_eq!(line, 6);

        let (_, label, _) = err(&with_project("[config]\nFOO = 1.5\n"));
        assert!(label.contains("boolean, an integer or a string"), "{label}");

        let (_, label, _) =
            err("[project]\nname = \"a\"\ntarget = \"esp32\"\nidf = \"6.0\"\nedition = 2\n");
        assert!(label.contains("unknown field"), "{label}");
    }

    #[test]
    fn partition_checks() {
        let part = |name: &str, size: &str| {
            format!(
                "[[partition]]\nname = \"{name}\"\ntype = \"data\"\nsubtype = \"nvs\"\nsize = {size}\n"
            )
        };
        let text = with_project(&(part("nvs", "\"24K\"") + &part("nvs", "4096")));
        let (msg, label, line) = err(&text);
        assert!(msg.contains("duplicate partition"), "{msg}");
        assert_eq!(label, "already defined on line 6");
        assert_eq!(line, 11);

        let (_, label, _) = err(&with_project(&part("nvs", "\"24Q\"")));
        assert!(label.contains("is not a size"), "{label}");
        let (_, label, _) = err(&with_project(&part("a_very_long_partition", "1")));
        assert!(label.contains("1–15 ASCII"), "{label}");
    }

    #[test]
    fn byte_sizes() {
        assert_eq!("0x6000".parse(), Ok(ByteSize(0x6000)));
        assert_eq!("4k".parse(), Ok(ByteSize(4096)));
        assert!("0x10K".parse::<ByteSize>().is_err());
        assert!("".parse::<ByteSize>().is_err());
    }
}
