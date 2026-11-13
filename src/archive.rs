//! Unpacking toolchain archives (`.tar.gz`, `.tar.xz`, `.zip`).
//!
//! Everything is unpacked into a staging directory next to the destination
//! and renamed into place at the end, so a half-extracted tool never appears
//! at its final path.

use miette::{Context, IntoDiagnostic, Result, bail};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    TarGz,
    TarXz,
    Zip,
}

impl Format {
    pub fn from_name(name: &str) -> Option<Self> {
        let n = name.to_ascii_lowercase();
        if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if n.ends_with(".tar.xz") {
            Some(Self::TarXz)
        } else if n.ends_with(".zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }
}

/// Unpack `archive` to `dest` (which must not exist), dropping `strip`
/// leading directory levels.
pub fn unpack(archive: &Path, format: Format, dest: &Path, strip: usize) -> Result<()> {
    let parent = dest.parent().expect("dest has a parent");
    crate::fsx::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".unpack-")
        .tempdir_in(parent)
        .into_diagnostic()?;

    let file = File::open(archive)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", archive.display()))?;
    let reader = BufReader::with_capacity(1 << 20, file);
    match format {
        Format::TarGz => unpack_tar(flate2::read::GzDecoder::new(reader), staging.path()),
        Format::TarXz => unpack_tar(liblzma::read::XzDecoder::new(reader), staging.path()),
        Format::Zip => unpack_zip(reader, staging.path()),
    }
    .wrap_err_with(|| format!("unpacking {}", archive.display()))?;

    let mut root = staging.path().to_path_buf();
    for _ in 0..strip {
        root = single_subdir(&root)?;
    }
    fs::rename(&root, dest)
        .into_diagnostic()
        .wrap_err_with(|| format!("moving into {}", dest.display()))?;
    Ok(())
}

fn single_subdir(dir: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = fs::read_dir(dir)
        .into_diagnostic()?
        .collect::<Result<_, _>>()
        .into_diagnostic()?;
    match entries.as_slice() {
        [only] if only.file_type().into_diagnostic()?.is_dir() => Ok(only.path()),
        _ => bail!(
            "expected a single top-level directory to strip in {}",
            dir.display()
        ),
    }
}

fn unpack_tar(reader: impl Read, dest: &Path) -> Result<()> {
    let mut tar = tar::Archive::new(reader);
    tar.set_preserve_permissions(true);
    tar.set_overwrite(true);
    // `unpack` refuses entries that would escape `dest`, including via
    // symlinks and hard links.
    tar.unpack(dest).into_diagnostic()
}

fn unpack_zip(reader: impl Read + std::io::Seek, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(reader).into_diagnostic()?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).into_diagnostic()?;
        let Some(rel) = entry.enclosed_name() else {
            bail!(
                "unsafe path in archive: {:?}",
                entry.name().map(|n| n.into_owned()).unwrap_or_default()
            );
        };
        let mode = entry.unix_mode();
        write_entry(&mut entry, &rel, mode, dest, None)?;
    }
    Ok(())
}

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
const S_IFDIR: u32 = 0o040000;

/// Receives each extracted file's (or symlink's) content.
pub(crate) type EntrySink<'a> = &'a mut dyn FnMut(EntryKind, &[u8]);

/// Write one zip entry under `dest`. Returns the bytes written for regular
/// files (or the link target for symlinks) when `content` is requested.
pub(crate) fn write_entry(
    entry: &mut zip::read::ZipFile<'_, impl Read>,
    rel: &Path,
    mode: Option<u32>,
    dest: &Path,
    mut content: Option<EntrySink<'_>>,
) -> Result<()> {
    let path = dest.join(rel);
    let kind = match mode.map(|m| m & S_IFMT) {
        Some(S_IFLNK) => EntryKind::Symlink,
        Some(S_IFDIR) => EntryKind::Dir,
        _ if entry.is_dir() => EntryKind::Dir,
        _ => EntryKind::File {
            executable: mode.is_some_and(|m| m & 0o111 != 0),
        },
    };
    match kind {
        EntryKind::Dir => crate::fsx::create_dir_all(&path)?,
        EntryKind::Symlink => {
            let mut target = String::new();
            entry.read_to_string(&mut target).into_diagnostic()?;
            if !link_stays_inside(rel, Path::new(&target)) {
                bail!(
                    "symlink {} -> {target} points outside the archive",
                    rel.display()
                );
            }
            if let Some(parent) = path.parent() {
                crate::fsx::create_dir_all(parent)?;
            }
            let _ = fs::remove_file(&path);
            std::os::unix::fs::symlink(&target, &path).into_diagnostic()?;
            if let Some(f) = content.as_mut() {
                f(kind, target.as_bytes());
            }
        }
        EntryKind::File { executable } => {
            if let Some(parent) = path.parent() {
                crate::fsx::create_dir_all(parent)?;
            }
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut data)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {}", rel.display()))?;
            crate::fsx::write(&path, &data)?;
            use std::os::unix::fs::PermissionsExt;
            let perm = if executable { 0o755 } else { 0o644 };
            fs::set_permissions(&path, fs::Permissions::from_mode(perm)).into_diagnostic()?;
            if let Some(f) = content.as_mut() {
                f(kind, &data);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    Symlink,
    File { executable: bool },
}

/// Whether a relative symlink at `link` (relative to the archive root)
/// resolves to somewhere inside the archive.
fn link_stays_inside(link: &Path, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth: i64 = link.components().count() as i64 - 1;
    for c in target.components() {
        match c {
            Component::ParentDir => depth -= 1,
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            _ => return false,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_targz(path: &Path) {
        let f = File::create(path).unwrap();
        let gz = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        let mut add = |name: &str, data: &[u8], mode: u32| {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(mode);
            h.set_cksum();
            tar.append_data(&mut h, name, data).unwrap();
        };
        add("cmake-4.0/bin/cmake", b"#!/bin/sh\n", 0o755);
        add("cmake-4.0/share/x.txt", b"x", 0o644);
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn unpacks_tar_and_strips() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("cmake.tar.gz");
        make_targz(&archive);
        let dest = dir.path().join("out");
        unpack(&archive, Format::TarGz, &dest, 1).unwrap();
        let bin = dest.join("bin/cmake");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&bin).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(dest.join("share/x.txt").exists());
        // Staging directory is gone.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers.len(), 2, "{leftovers:?}");
    }

    #[test]
    fn unpacks_zip_with_modes_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("ninja.zip");
        let mut zw = zip::ZipWriter::new(File::create(&archive).unwrap());
        let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zw.start_file("ninja", opts).unwrap();
        zw.write_all(b"bin").unwrap();
        zw.add_symlink(
            "ninja-link",
            "ninja",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zw.finish().unwrap();

        let dest = dir.path().join("out");
        unpack(&archive, Format::Zip, &dest, 0).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dest.join("ninja"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::read_link(dest.join("ninja-link")).unwrap(),
            Path::new("ninja")
        );
    }

    #[test]
    fn rejects_escaping_symlinks() {
        assert!(link_stays_inside(Path::new("a/b/link"), Path::new("../c")));
        assert!(!link_stays_inside(
            Path::new("a/link"),
            Path::new("../../etc/passwd")
        ));
        assert!(!link_stays_inside(
            Path::new("link"),
            Path::new("/etc/passwd")
        ));
    }

    #[test]
    fn formats() {
        assert_eq!(Format::from_name("x.tar.xz"), Some(Format::TarXz));
        assert_eq!(
            Format::from_name("ninja-mac-v1.12.1.zip"),
            Some(Format::Zip)
        );
        assert_eq!(Format::from_name("x.tar.bz2"), None);
    }
}
