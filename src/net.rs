//! HTTP: verified downloads into a content-addressed store, range reads, and
//! the GitHub API calls used to resolve ESP-IDF releases.

use crate::ui;
use miette::{Context, IntoDiagnostic, Result, bail, miette};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const USER_AGENT: &str = concat!("idf-next/", env!("CARGO_PKG_VERSION"));
const RETRIES: usize = 3;

#[derive(Clone)]
pub struct Net {
    client: reqwest::blocking::Client,
    offline: bool,
}

impl Net {
    pub fn new(offline: bool) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(30))
            // No overall timeout: toolchain archives are hundreds of MB.
            .timeout(None)
            .build()
            .into_diagnostic()?;
        Ok(Self { client, offline })
    }

    pub fn offline(&self) -> bool {
        self.offline
    }

    fn ensure_online(&self, what: &str) -> Result<()> {
        if self.offline {
            bail!("{what} needs the network, but --offline was given");
        }
        Ok(())
    }

    fn get(&self, url: &str) -> Result<reqwest::blocking::Response> {
        let mut last = None;
        for attempt in 1..=RETRIES {
            match self
                .client
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
            {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    ui::detail(
                        "Retrying",
                        format!("{url} ({e}), attempt {attempt}/{RETRIES}"),
                    );
                    last = Some(e);
                    std::thread::sleep(Duration::from_secs(attempt as u64));
                }
            }
        }
        Err(last.expect("at least one attempt"))
            .into_diagnostic()
            .wrap_err_with(|| format!("GET {url}"))
    }

    /// Fetch a small text resource.
    pub fn get_text(&self, url: &str) -> Result<String> {
        self.ensure_online(url)?;
        self.get(url)?.text().into_diagnostic()
    }

    /// Size of a remote file, and whether the server honours Range requests.
    pub fn probe(&self, url: &str) -> Result<u64> {
        self.ensure_online(url)?;
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .and_then(|r| r.error_for_status())
            .into_diagnostic()
            .wrap_err_with(|| format!("GET {url}"))?;
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            bail!("{url} does not support range requests");
        }
        let range = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| miette!("{url}: missing Content-Range"))?;
        range
            .rsplit('/')
            .next()
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| miette!("{url}: bad Content-Range {range:?}"))
    }

    /// Copy bytes `[start, end)` of `url` into `file` at the same offset.
    pub fn fetch_range(
        &self,
        url: &str,
        start: u64,
        end: u64,
        file: &mut File,
        bar: &indicatif::ProgressBar,
    ) -> Result<()> {
        self.ensure_online(url)?;
        if start >= end {
            return Ok(());
        }
        let mut done = 0u64;
        let mut attempt = 0;
        while start + done < end {
            attempt += 1;
            let from = start + done;
            let result =
                (|| -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
                    let mut resp = self
                        .client
                        .get(url)
                        .header(reqwest::header::RANGE, format!("bytes={from}-{}", end - 1))
                        .send()?
                        .error_for_status()?;
                    if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                        return Err("server ignored the Range header".into());
                    }
                    file.seek(SeekFrom::Start(from))?;
                    let mut buf = vec![0u8; 256 * 1024];
                    loop {
                        let n = resp.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        let n = n.min((end - start - done) as usize);
                        file.write_all(&buf[..n])?;
                        done += n as u64;
                        bar.inc(n as u64);
                        if start + done >= end {
                            break;
                        }
                    }
                    Ok(())
                })();
            match result {
                Ok(()) if start + done >= end => return Ok(()),
                Ok(()) => {}
                Err(e) if attempt < RETRIES => {
                    ui::detail("Retrying", format!("{url} bytes {from}-{end}: {e}"));
                    std::thread::sleep(Duration::from_secs(attempt as u64));
                }
                Err(e) => return Err(miette!("GET {url} bytes {from}-{end}: {e}")),
            }
            if attempt >= RETRIES && start + done < end {
                bail!("GET {url}: connection kept closing early");
            }
        }
        Ok(())
    }

    /// Download `url` into `dest`, verifying size and SHA-256. Writes to a
    /// temporary file first, so `dest` only ever holds verified content.
    pub fn download_verified(
        &self,
        url: &str,
        expected: &Expected,
        dest: &Path,
        label: &str,
    ) -> Result<()> {
        if dest.exists() {
            return Ok(());
        }
        self.ensure_online(&format!("downloading {label}"))?;
        let dir = dest.parent().expect("dest has a parent");
        std::fs::create_dir_all(dir).into_diagnostic()?;

        let mut last_err = None;
        for attempt in 1..=RETRIES {
            match self.try_download(url, expected, dir, label) {
                Ok(tmp) => {
                    tmp.persist(dest).into_diagnostic()?;
                    return Ok(());
                }
                Err(e) => {
                    if attempt < RETRIES {
                        ui::detail("Retrying", format!("{label}: {e}"));
                        std::thread::sleep(Duration::from_secs(attempt as u64));
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.expect("at least one attempt"))
    }

    fn try_download(
        &self,
        url: &str,
        expected: &Expected,
        dir: &Path,
        label: &str,
    ) -> Result<tempfile::NamedTempFile> {
        let mut resp = self.get(url)?;
        let bar = ui::bytes_bar(expected.size, label);
        let mut tmp = tempfile::NamedTempFile::new_in(dir).into_diagnostic()?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut total = 0u64;
        loop {
            let n = resp
                .read(&mut buf)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {url}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            tmp.write_all(&buf[..n]).into_diagnostic()?;
            total += n as u64;
            bar.set_position(total);
        }
        bar.finish_and_clear();
        let got = hex(&hasher.finalize());
        if total != expected.size {
            bail!(
                "{label}: expected {} bytes from {url}, got {total}",
                expected.size
            );
        }
        if got != expected.sha256 {
            bail!(
                "{label}: checksum mismatch for {url}\n  expected sha256 {}\n  got      sha256 {got}",
                expected.sha256
            );
        }
        tmp.as_file().sync_all().into_diagnostic()?;
        Ok(tmp)
    }

    /// GitHub REST call; honours `GITHUB_TOKEN` to lift the anonymous rate limit.
    pub fn github_json(&self, path: &str) -> Result<serde_json::Value> {
        self.ensure_online("resolving ESP-IDF releases")?;
        let url = format!("https://api.github.com{path}");
        let mut req = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .into_diagnostic()
            .wrap_err_with(|| format!("GET {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            let hint = if status == reqwest::StatusCode::FORBIDDEN
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            {
                " (GitHub rate limit? set GITHUB_TOKEN)"
            } else {
                ""
            };
            bail!("GET {url}: {status}{hint}\n{}", body.trim());
        }
        let text = resp.text().into_diagnostic()?;
        serde_json::from_str(&text)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {url}"))
    }
}

/// What a download must hash to.
#[derive(Debug, Clone)]
pub struct Expected {
    pub sha256: String,
    pub size: u64,
}

/// Where a verified archive with this hash lives in the download store.
pub fn store_path(downloads: &Path, sha256: &str, file_name: &str) -> PathBuf {
    downloads.join(format!("sha256-{sha256}")).join(file_name)
}

/// Apply the `IDF_GITHUB_ASSETS` mirror (same meaning as in idf_tools.py):
/// `https://github.com/...` becomes `https://$IDF_GITHUB_ASSETS/...`.
pub fn mirror(url: &str) -> String {
    match std::env::var("IDF_GITHUB_ASSETS") {
        Ok(host) if !host.trim().is_empty() => match url.strip_prefix("https://github.com/") {
            Some(rest) => format!("https://{}/{rest}", host.trim().trim_end_matches('/')),
            None => url.to_owned(),
        },
        _ => url.to_owned(),
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).into_diagnostic()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn sha256_bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_digest() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }
}
