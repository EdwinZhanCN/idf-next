//! Which ESP-IDF releases this build of idf-next knows how to drive.
//!
//! The tool and the SDK are versioned independently (invariant 3). The range
//! below is the single place that ties them together: `idf --version`,
//! manifest validation and (from P1) `idf doctor` all read it.

use semver::{Version, VersionReq};
use std::sync::LazyLock;

// A macro so the literal can also be spliced into `LONG_VERSION` by `concat!`.
macro_rules! supported_idf_range {
    () => {
        ">=6.0.0, <6.1.0"
    };
}

/// Supported ESP-IDF releases, as a semver requirement.
pub const SUPPORTED_IDF_RANGE: &str = supported_idf_range!();

/// Text shown by `idf --version` (clap prefixes it with `idf `).
pub const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nsupported ESP-IDF: ",
    supported_idf_range!()
);

/// The IDF release `idf new` / `idf init` write into a fresh `idf.toml`.
pub const DEFAULT_IDF: &str = "6.0";

static SUPPORTED: LazyLock<VersionReq> =
    LazyLock::new(|| VersionReq::parse(SUPPORTED_IDF_RANGE).expect("valid built-in range"));

/// Whether a concrete IDF release (e.g. from `idf.lock`) is supported.
pub fn supports(version: &Version) -> bool {
    SUPPORTED.matches(version)
}

/// Whether every release matched by an `idf = "…"` spec is supported.
pub fn supports_spec(spec: &IdfSpec) -> bool {
    match spec.patch {
        Some(patch) => supports(&Version::new(spec.major, spec.minor, patch)),
        // "6.0" means any 6.0.x. The range is closed on minor boundaries, so
        // checking x.y.0 is enough to cover the whole series.
        None => supports(&Version::new(spec.major, spec.minor, 0)),
    }
}

/// The `idf` field of `[project]`: a release series ("6.0") or an exact
/// release ("6.0.1"). Deliberately not a full semver requirement: a project
/// tracks one series and upgrades are explicit (invariant 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdfSpec {
    pub major: u64,
    pub minor: u64,
    pub patch: Option<u64>,
}

impl IdfSpec {
    pub fn matches(&self, version: &Version) -> bool {
        version.major == self.major
            && version.minor == self.minor
            && self.patch.is_none_or(|p| version.patch == p)
            && version.pre.is_empty()
    }
}

impl std::str::FromStr for IdfSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || format!("expected an ESP-IDF release like \"6.0\" or \"6.0.1\", got {s:?}");
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        let num = |p: &str| -> Result<u64, String> {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            p.parse().map_err(|_| bad())
        };
        match parts.as_slice() {
            [major, minor] => Ok(Self {
                major: num(major)?,
                minor: num(minor)?,
                patch: None,
            }),
            [major, minor, patch] => Ok(Self {
                major: num(major)?,
                minor: num(minor)?,
                patch: Some(num(patch)?),
            }),
            _ => Err(bad()),
        }
    }
}

impl std::fmt::Display for IdfSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.patch {
            Some(p) => write!(f, "{}.{}.{}", self.major, self.minor, p),
            None => write!(f, "{}.{}", self.major, self.minor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_specs() {
        assert_eq!(
            "6.0".parse(),
            Ok(IdfSpec {
                major: 6,
                minor: 0,
                patch: None
            })
        );
        assert_eq!(
            "v6.0.1".parse(),
            Ok(IdfSpec {
                major: 6,
                minor: 0,
                patch: Some(1)
            })
        );
        assert!("6".parse::<IdfSpec>().is_err());
        assert!("6.0.x".parse::<IdfSpec>().is_err());
        assert!("^6.0".parse::<IdfSpec>().is_err());
        assert!("6.0.1.2".parse::<IdfSpec>().is_err());
    }

    #[test]
    fn spec_matching() {
        let series: IdfSpec = "6.0".parse().unwrap();
        assert!(series.matches(&Version::new(6, 0, 3)));
        assert!(!series.matches(&Version::new(6, 1, 0)));
        assert!(!series.matches(&Version::parse("6.0.0-rc1").unwrap()));
        let exact: IdfSpec = "6.0.1".parse().unwrap();
        assert!(exact.matches(&Version::new(6, 0, 1)));
        assert!(!exact.matches(&Version::new(6, 0, 2)));
    }

    #[test]
    fn support_range() {
        assert!(supports_spec(&"6.0".parse().unwrap()));
        assert!(supports_spec(&"6.0.2".parse().unwrap()));
        assert!(!supports_spec(&"5.4".parse().unwrap()));
        assert!(!supports_spec(&"6.1".parse().unwrap()));
        assert!(supports_spec(&DEFAULT_IDF.parse().unwrap()));
    }
}
