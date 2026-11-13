//! Host platforms, named the way ESP-IDF's tools.json names them.

/// Platforms idf-next ships for. `idf.lock` records tool archives for all of
/// them, so a lock written on one host reproduces on the others.
pub const SUPPORTED: &[&str] = &["macos-arm64", "linux-amd64", "linux-arm64"];

/// The tools.json key for the machine we are running on.
pub fn host() -> miette::Result<&'static str> {
    host_for(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        miette::miette!(
            "unsupported host {}-{}; idf-next runs on {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            SUPPORTED.join(", ")
        )
    })
}

pub fn host_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("linux", "x86_64") => Some("linux-amd64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn maps_hosts() {
        assert_eq!(super::host_for("macos", "aarch64"), Some("macos-arm64"));
        assert_eq!(super::host_for("windows", "x86_64"), None);
        assert!(super::host().is_ok());
    }
}
