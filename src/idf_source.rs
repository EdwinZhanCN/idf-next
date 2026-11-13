//! Fetching ESP-IDF source from the official release zip.
//!
//! The release zip (`esp-idf-vX.Y.Z.zip`) contains the source with all
//! submodules checked out, plus the full `.git` history, which is over 90% of
//! its ~2 GB. We read the zip's central directory over HTTP and fetch only the
//! byte ranges of the entries we keep, into a sparse local file. The result is
//! a valid zip as far as the kept entries are concerned.
//!
//! Integrity: each entry's CRC-32 is checked on extraction, and the extracted
//! tree's digest is compared with the one recorded in `idf.lock`.

use crate::archive::{self, EntryKind};
use crate::net::{self, Net};
use crate::ui;
use miette::{Context, IntoDiagnostic, Result, bail, miette};
use sha2::{Digest as _, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// The release tag for a version: 6.0.0 is tagged `v6.0`, 6.0.3 `v6.0.3`.
pub fn tag_for(version: &semver::Version) -> String {
    if version.patch == 0 {
        format!("v{}.{}", version.major, version.minor)
    } else {
        format!("v{version}")
    }
}

pub fn zip_url(tag: &str) -> String {
    net::mirror(&format!(
        "https://github.com/espressif/esp-idf/releases/download/{tag}/esp-idf-{tag}.zip"
    ))
}

/// Entries we never extract: git metadata of the repo and its submodules.
fn excluded(rel: &Path) -> bool {
    rel.components().any(|c| c.as_os_str() == ".git")
}

// ------------------------------------------------------- central directory ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CdEntry {
    pub name: String,
    pub header_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CdLocation {
    pub offset: u64,
    pub size: u64,
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

/// Find the central directory from the last bytes of a zip. `tail_start` is
/// the file offset of `tail[0]`. Returns `Err(offset)` when the zip64 record
/// lies before the tail and must be fetched separately.
pub(crate) fn locate_cd(
    tail: &[u8],
    tail_start: u64,
) -> Result<std::result::Result<CdLocation, u64>> {
    const EOCD: u32 = 0x0605_4b50;
    let pos = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(tail, i) == EOCD)
        .ok_or_else(|| miette!("not a zip file (no end-of-central-directory record)"))?;
    let size = u32_at(tail, pos + 12);
    let offset = u32_at(tail, pos + 16);
    if size != u32::MAX && offset != u32::MAX {
        return Ok(Ok(CdLocation {
            offset: offset.into(),
            size: size.into(),
        }));
    }
    // Zip64: the locator sits right before the EOCD record.
    if pos < 20 || u32_at(tail, pos - 20) != 0x0706_4b50 {
        bail!("zip64 archive without a zip64 locator");
    }
    let record = u64_at(tail, pos - 20 + 8);
    if record < tail_start {
        return Ok(Err(record));
    }
    parse_zip64_record(&tail[(record - tail_start) as usize..]).map(Ok)
}

pub(crate) fn parse_zip64_record(b: &[u8]) -> Result<CdLocation> {
    if b.len() < 56 || u32_at(b, 0) != 0x0606_4b50 {
        bail!("bad zip64 end-of-central-directory record");
    }
    Ok(CdLocation {
        size: u64_at(b, 40),
        offset: u64_at(b, 48),
    })
}

pub(crate) fn parse_cd(cd: &[u8]) -> Result<Vec<CdEntry>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 46 <= cd.len() && u32_at(cd, i) == 0x0201_4b50 {
        let name_len = u16_at(cd, i + 28) as usize;
        let extra_len = u16_at(cd, i + 30) as usize;
        let comment_len = u16_at(cd, i + 32) as usize;
        let csize = u32_at(cd, i + 20);
        let usize_ = u32_at(cd, i + 24);
        let mut offset = u64::from(u32_at(cd, i + 42));
        let name_start = i + 46;
        let extra_start = name_start + name_len;
        if extra_start + extra_len > cd.len() {
            bail!("truncated central directory");
        }
        let name = String::from_utf8_lossy(&cd[name_start..extra_start]).into_owned();
        if offset == u64::from(u32::MAX) {
            // Zip64 extra field: fields present only for values that overflowed,
            // in the order uncompressed size, compressed size, offset.
            let extra = &cd[extra_start..extra_start + extra_len];
            let mut j = 0;
            while j + 4 <= extra.len() {
                let id = u16_at(extra, j);
                let len = u16_at(extra, j + 2) as usize;
                if id == 0x0001 {
                    let mut k = j + 4;
                    if usize_ == u32::MAX {
                        k += 8;
                    }
                    if csize == u32::MAX {
                        k += 8;
                    }
                    if k + 8 > extra.len() {
                        bail!("bad zip64 extra field for {name}");
                    }
                    offset = u64_at(extra, k);
                }
                j += 4 + len;
            }
        }
        out.push(CdEntry {
            name,
            header_offset: offset,
        });
        i = extra_start + extra_len + comment_len;
    }
    Ok(out)
}

/// Byte ranges `[start, end)` covering the kept entries. Entries span from
/// their local header to the next entry's header; neighbouring kept ranges
/// separated by less than `merge_gap` bytes are merged.
pub(crate) fn needed_ranges(
    entries: &[CdEntry],
    cd_offset: u64,
    merge_gap: u64,
) -> Vec<(u64, u64)> {
    let mut sorted: Vec<&CdEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.header_offset);
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for (i, e) in sorted.iter().enumerate() {
        let end = sorted.get(i + 1).map_or(cd_offset, |n| n.header_offset);
        let rel = strip_root(&e.name);
        if rel.as_os_str().is_empty() || excluded(&rel) {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if e.header_offset <= last.1 + merge_gap => last.1 = end,
            _ => ranges.push((e.header_offset, end)),
        }
    }
    ranges
}

/// Drop the archive's top-level directory (`esp-idf-v6.0.3/`).
fn strip_root(name: &str) -> PathBuf {
    let mut parts = Path::new(name).components();
    parts.next();
    parts.as_path().to_path_buf()
}

// ------------------------------------------------------------------ fetch ---

/// Make sure the sparse release zip for `tag` is in the download store.
pub fn fetch(net: &Net, downloads: &Path, tag: &str) -> Result<PathBuf> {
    let dir = downloads.join("esp-idf");
    let zip_path = dir.join(format!("esp-idf-{tag}.zip"));
    let done = dir.join(format!("esp-idf-{tag}.zip.complete"));
    if done.exists() && zip_path.exists() {
        return Ok(zip_path);
    }
    if net.offline() {
        bail!("ESP-IDF {tag} source is not in the download cache, and --offline was given");
    }
    crate::fsx::create_dir_all(&dir)?;
    let url = zip_url(tag);
    let size = net
        .probe(&url)
        .wrap_err_with(|| format!("ESP-IDF {tag} release zip"))?;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&zip_path)
        .into_diagnostic()?;
    // A sparse file: only the ranges we write take up disk space.
    file.set_len(size).into_diagnostic()?;

    let hidden = indicatif::ProgressBar::hidden();
    let tail_len = size.min(256 * 1024);
    net.fetch_range(&url, size - tail_len, size, &mut file, &hidden)?;
    let mut tail = vec![0u8; tail_len as usize];
    file.seek(SeekFrom::Start(size - tail_len))
        .into_diagnostic()?;
    file.read_exact(&mut tail).into_diagnostic()?;
    let cd = match locate_cd(&tail, size - tail_len)? {
        Ok(cd) => cd,
        Err(record) => {
            net.fetch_range(&url, record, record + 56, &mut file, &hidden)?;
            let mut buf = vec![0u8; 56];
            file.seek(SeekFrom::Start(record)).into_diagnostic()?;
            file.read_exact(&mut buf).into_diagnostic()?;
            parse_zip64_record(&buf)?
        }
    };
    // Central directory through to the end of the file.
    net.fetch_range(&url, cd.offset, size - tail_len, &mut file, &hidden)?;
    let mut cd_bytes = vec![0u8; cd.size as usize];
    file.seek(SeekFrom::Start(cd.offset)).into_diagnostic()?;
    file.read_exact(&mut cd_bytes).into_diagnostic()?;
    let entries = parse_cd(&cd_bytes)?;

    let ranges = needed_ranges(&entries, cd.offset, 1 << 20);
    let total: u64 = ranges.iter().map(|(a, b)| b - a).sum();
    ui::status(
        "Downloading",
        format!(
            "ESP-IDF {tag} source ({} of {}, skipping git history)",
            ui::bytes(total),
            ui::bytes(size)
        ),
    );
    let bar = ui::bytes_bar(total, format!("esp-idf-{tag}.zip"));
    let chunks = split(&ranges, 16 << 20);
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| -> Result<()> {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                s.spawn(|| -> Result<()> {
                    let mut f = OpenOptions::new()
                        .write(true)
                        .open(&zip_path)
                        .into_diagnostic()?;
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&(a, b)) = chunks.get(i) else {
                            return Ok(());
                        };
                        net.fetch_range(&url, a, b, &mut f, &bar)?;
                    }
                })
            })
            .collect();
        for w in workers {
            w.join()
                .map_err(|_| miette!("download thread panicked"))??;
        }
        Ok(())
    })?;
    bar.finish_and_clear();
    file.sync_all().into_diagnostic()?;
    crate::fsx::write(&done, format!("{url}\n"))?;
    Ok(zip_path)
}

fn split(ranges: &[(u64, u64)], max: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    for &(mut a, b) in ranges {
        while b - a > max {
            out.push((a, a + max));
            a += max;
        }
        out.push((a, b));
    }
    out
}

// ---------------------------------------------------------------- extract ---

/// Extract the kept entries of the release zip into `dest` (must not exist).
/// Returns the tree digest of the extracted source.
pub fn extract(zip_path: &Path, tag: &str, dest: &Path) -> Result<String> {
    let parent = dest.parent().expect("dest has a parent");
    crate::fsx::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".esp-idf-")
        .tempdir_in(parent)
        .into_diagnostic()?;

    let file = File::open(zip_path).into_diagnostic()?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 20, file))
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", zip_path.display()))?;
    let spinner = ui::spinner(format!("Extracting ESP-IDF {tag}"));
    let mut tree = Tree::default();
    let count = zip.len();
    for i in 0..count {
        // Decide from the central directory alone: skipped entries were never
        // downloaded, so their local headers are zeros in the sparse file.
        let skip = zip.name_for_index(i).and_then(Result::ok).is_none_or(|n| {
            let rel = strip_root(&n);
            rel.as_os_str().is_empty() || excluded(&rel)
        });
        if skip {
            continue;
        }
        let mut entry = zip.by_index(i).into_diagnostic()?;
        let Some(name) = entry.enclosed_name() else {
            bail!(
                "unsafe path in archive: {:?}",
                entry.name().map(|n| n.into_owned()).unwrap_or_default()
            );
        };
        let rel = strip_root(&name.to_string_lossy());
        if rel.as_os_str().is_empty() || excluded(&rel) {
            continue;
        }
        let mode = entry.unix_mode();
        let rel_for_tree = rel.clone();
        archive::write_entry(
            &mut entry,
            &rel,
            mode,
            staging.path(),
            Some(&mut |kind, data| tree.add(&rel_for_tree, kind, data)),
        )
        .wrap_err_with(|| format!("extracting {}", rel.display()))?;
        if i % 512 == 0 {
            spinner.set_message(format!("Extracting ESP-IDF {tag} ({i}/{count})"));
        }
    }
    spinner.finish_and_clear();
    let digest = tree.digest();
    // IDF reads its version from version.txt when there is no .git, the same
    // way it does for release tarballs.
    crate::fsx::write(staging.path().join("version.txt"), format!("{tag}\n"))?;
    crate::fsx::rename(staging.keep(), dest)?;
    Ok(digest)
}

/// Digest of a source tree: one line per file or symlink, sorted by path.
#[derive(Default)]
pub(crate) struct Tree {
    lines: Vec<(String, String)>,
}

impl Tree {
    pub fn add(&mut self, rel: &Path, kind: EntryKind, data: &[u8]) {
        let path = rel.to_string_lossy().into_owned();
        let line = match kind {
            EntryKind::Dir => return,
            EntryKind::Symlink => format!("L {} {path}\n", net::sha256_bytes(data)),
            EntryKind::File { executable } => {
                format!(
                    "{} {} {path}\n",
                    if executable { "X" } else { "F" },
                    net::sha256_bytes(data)
                )
            }
        };
        self.lines.push((path, line));
    }

    pub fn digest(mut self) -> String {
        self.lines.sort();
        let mut h = Sha256::new();
        for (_, line) in &self.lines {
            h.update(line.as_bytes());
        }
        format!("sha256:{}", net::hex(&h.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn tags() {
        assert_eq!(tag_for(&semver::Version::new(6, 0, 0)), "v6.0");
        assert_eq!(tag_for(&semver::Version::new(6, 0, 3)), "v6.0.3");
    }

    fn build_zip(path: &Path) {
        let mut zw = zip::ZipWriter::new(File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("esp-idf-v6.0.3/CMakeLists.txt", &b"cmake"[..]),
            (
                "esp-idf-v6.0.3/.git/objects/pack/big.pack",
                &[7u8; 4096][..],
            ),
            (
                "esp-idf-v6.0.3/components/a/.git",
                &b"gitdir: ../../.git/modules/a"[..],
            ),
            ("esp-idf-v6.0.3/components/a/a.c", &b"int a;"[..]),
        ] {
            zw.start_file(name, o.unix_permissions(0o644)).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.start_file("esp-idf-v6.0.3/tools/idf.py", o.unix_permissions(0o755))
            .unwrap();
        zw.write_all(b"#!/usr/bin/env python").unwrap();
        zw.finish().unwrap();
    }

    #[test]
    fn central_directory_and_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idf.zip");
        build_zip(&path);
        let bytes = fs::read(&path).unwrap();
        let cd = locate_cd(&bytes, 0).unwrap().unwrap();
        let entries = parse_cd(&bytes[cd.offset as usize..(cd.offset + cd.size) as usize]).unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].header_offset, 0);

        // Without merging, the .git pack and the submodule gitfile are skipped;
        // a.c and idf.py are adjacent and share one range.
        let ranges = needed_ranges(&entries, cd.offset, 0);
        assert_eq!(ranges.len(), 2, "{ranges:?}");
        for skipped in [&entries[1], &entries[2]] {
            assert!(skipped.name.contains(".git"));
            let o = skipped.header_offset;
            assert!(
                ranges.iter().all(|&(a, b)| !(a <= o && o < b)),
                "{} fetched",
                skipped.name
            );
        }
        // With a generous gap everything merges into one range.
        assert_eq!(needed_ranges(&entries, cd.offset, 1 << 20).len(), 1);

        // Simulate a sparse download: zero everything outside the ranges and
        // the central directory, then extract.
        let mut sparse = vec![0u8; bytes.len()];
        for &(a, b) in ranges
            .iter()
            .chain([(cd.offset, bytes.len() as u64)].iter())
        {
            sparse[a as usize..b as usize].copy_from_slice(&bytes[a as usize..b as usize]);
        }
        let sparse_path = dir.path().join("sparse.zip");
        fs::write(&sparse_path, &sparse).unwrap();
        let dest = dir.path().join("esp-idf");
        let digest = extract(&sparse_path, "v6.0.3", &dest).unwrap();
        assert!(digest.starts_with("sha256:"));
        assert_eq!(
            fs::read_to_string(dest.join("components/a/a.c")).unwrap(),
            "int a;"
        );
        assert_eq!(
            fs::read_to_string(dest.join("version.txt")).unwrap(),
            "v6.0.3\n"
        );
        assert!(!dest.join(".git").exists());
        assert!(!dest.join("components/a/.git").exists());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dest.join("tools/idf.py"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0o111
        );

        // Same content, same digest.
        let dest2 = dir.path().join("esp-idf2");
        assert_eq!(extract(&path, "v6.0.3", &dest2).unwrap(), digest);
    }
}
