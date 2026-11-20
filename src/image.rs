//! Exported firmware images, byte-for-byte what the upstream tools write:
//!
//! - raw merged binary: `esptool merge-bin` (what `idf.py merge-bin` runs)
//! - UF2: `esptool merge-bin --format uf2` (what `mkuf2.py` runs)
//! - DFU: `tools/mkdfu.py`
//!
//! The build already wrote the flash settings into the bootloader header,
//! and `idf.py` passes those same settings, so esptool's header rewrite is
//! a no-op for these inputs and is not reproduced.

use md5::{Digest, Md5};

/// One input image at its flash offset.
#[derive(Debug, Clone)]
pub struct Part {
    pub offset: u32,
    /// File name, for DFU entries.
    pub name: String,
    pub data: Vec<u8>,
}

/// `esptool merge-bin` (raw): images at their offsets from address 0, gaps
/// filled with 0xFF.
pub fn merge_raw(parts: &[Part]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in sorted(parts) {
        let at = p.offset as usize;
        if out.len() < at {
            out.resize(at, 0xFF);
        }
        out.extend_from_slice(&p.data);
    }
    out
}

fn sorted(parts: &[Part]) -> Vec<&Part> {
    let mut v: Vec<&Part> = parts.iter().collect();
    v.sort_by_key(|p| p.offset);
    v
}

/// UF2 family IDs from esptool's chip definitions.
pub fn uf2_family_id(target: &str) -> Option<u32> {
    Some(match target {
        "esp32" => 0x1C5F21B0,
        "esp32s2" => 0xBFDD4EEE,
        "esp32s3" => 0xC47E5767,
        "esp32c2" => 0x2B88D29C,
        "esp32c3" => 0xD42BA06C,
        "esp32c5" => 0xF71C0343,
        "esp32c6" => 0x540DDF62,
        "esp32c61" => 0x77D850C4,
        "esp32h2" => 0x332726F6,
        "esp32p4" => 0x3D308E94,
        _ => return None,
    })
}

const UF2_FIRST_MAGIC: u32 = 0x0A32_4655;
const UF2_SECOND_MAGIC: u32 = 0x9E5D_5157;
const UF2_FINAL_MAGIC: u32 = 0x0AB1_6F30;
const UF2_FLAG_FAMILYID_PRESENT: u32 = 0x2000;
const UF2_FLAG_MD5_PRESENT: u32 = 0x4000;
const UF2_DATA_SIZE: usize = 476;
const UF2_MD5_PART_SIZE: usize = 24;

/// esptool's `UF2Writer` with default chunk size and MD5 enabled.
pub fn uf2(parts: &[Part], family_id: u32) -> Vec<u8> {
    let chunk_size = UF2_DATA_SIZE - UF2_MD5_PART_SIZE;
    let mut out = Vec::new();
    for p in sorted(parts) {
        let blocks = p.data.len().div_ceil(chunk_size) as u32;
        let mut addr = p.offset;
        for (i, chunk) in p.data.chunks(chunk_size).enumerate() {
            let start = out.len();
            for word in [
                UF2_FIRST_MAGIC,
                UF2_SECOND_MAGIC,
                UF2_FLAG_FAMILYID_PRESENT | UF2_FLAG_MD5_PRESENT,
                addr,
                chunk.len() as u32,
                i as u32,
                blocks,
                family_id,
            ] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            out.extend_from_slice(chunk);
            out.extend_from_slice(&addr.to_le_bytes());
            out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            out.extend_from_slice(&Md5::digest(chunk));
            out.resize(
                out.len() + UF2_DATA_SIZE - UF2_MD5_PART_SIZE - chunk.len(),
                0,
            );
            out.extend_from_slice(&UF2_FINAL_MAGIC.to_le_bytes());
            debug_assert_eq!(out.len() - start, 512);
            addr += chunk.len() as u32;
        }
    }
    out
}

/// The DFU product ID `tools/cmake/dfu.cmake` passes for a target. mkdfu
/// reads it as hex, so P4's "12" is 0x12.
pub fn dfu_pid(target: &str) -> Option<u16> {
    Some(match target {
        "esp32s2" => 0x2,
        "esp32s3" => 0x9,
        "esp32p4" => 0x12,
        _ => return None,
    })
}

/// mkdfu's default (`ESP_DFU_PART_SIZE`).
pub const DFU_PART_SIZE: usize = 512 * 1024;

const ESPRESSIF_VID: u16 = 0x303A;
const DFU_INFO_FLAG_PARAM: u32 = 1 << 2;
const DFU_INFO_FLAG_NOERASE: u32 = 1 << 3;
const DFU_INFO_FLAG_IGNORE_MD5: u32 = 1 << 4;

/// `mkdfu.py write`: a newc cpio archive of the images (split into
/// `part_size` pieces) plus an index, padded to 10 KiB, with a DFU suffix.
pub fn dfu(parts: &[Part], pid: u16, flash_size: u64, part_size: usize) -> Vec<u8> {
    let mut index = Vec::new();
    let mut entries = Vec::new();
    let mut add = |name: &str, addr: u32, data: &[u8], flags: u32| {
        index.extend_from_slice(&addr.to_le_bytes());
        index.extend_from_slice(&flags.to_le_bytes());
        let mut n = [0u8; 64];
        let len = name.len().min(64);
        n[..len].copy_from_slice(&name.as_bytes()[..len]);
        index.extend_from_slice(&n);
        index.extend_from_slice(&Md5::digest(data));
        entries.push(cpio_entry(name, data, false));
    };

    // The ROM's "flashchip" parameters (esptool flash_set_parameters).
    let mut params = Vec::new();
    for v in [0, 0, 0, flash_size as u32, 64 * 1024, 4 * 1024, 256, 0xFFFF] {
        params.extend_from_slice(&u32::to_le_bytes(v));
    }
    add(
        "flash_params.dat",
        0,
        &params,
        DFU_INFO_FLAG_PARAM | DFU_INFO_FLAG_NOERASE | DFU_INFO_FLAG_IGNORE_MD5,
    );
    for p in sorted(parts) {
        let mut addr = p.offset;
        for (i, chunk) in p.data.chunks(part_size).enumerate() {
            let name = if i == 0 {
                p.name.clone()
            } else {
                format!("{}.{i}", p.name)
            };
            add(&name, addr, chunk, 0);
            addr += chunk.len() as u32;
        }
    }

    let mut out = cpio_entry("dfuinfo0.dat", &index, false);
    for e in entries {
        out.extend_from_slice(&e);
    }
    out.extend_from_slice(&cpio_entry("TRAILER!!!", &[], true));
    pad(&mut out, 10240, 0);
    for v in [0xFFFF, pid, ESPRESSIF_VID, 0x0100] {
        out.extend_from_slice(&u16::to_le_bytes(v));
    }
    out.extend_from_slice(b"UFD");
    out.push(16);
    // CRC32/JAMCRC: zlib's CRC without the final inversion.
    let crc = !crc32fast::hash(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

fn cpio_entry(name: &str, data: &[u8], trailer: bool) -> Vec<u8> {
    let namesize = name.len() + 1;
    let (mode, nlink) = if trailer { (0, 1) } else { (0o100644, 0) };
    let mut e = b"070701".to_vec();
    for v in [0, mode, 0, 0, nlink, 0, data.len(), 0, 0, 0, 0, namesize, 0] {
        e.extend_from_slice(format!("{v:08x}").as_bytes());
    }
    e.extend_from_slice(name.as_bytes());
    e.push(0);
    pad(&mut e, 4, 0);
    e.extend_from_slice(data);
    pad(&mut e, 4, 0);
    e
}

fn pad(v: &mut Vec<u8>, multiple: usize, byte: u8) {
    let len = v.len().div_ceil(multiple) * multiple;
    v.resize(len, byte);
}

/// mkdfu's `--flash-size` (`"4MB"` → bytes); only whole megabytes.
pub fn flash_size_bytes(s: &str) -> Option<u64> {
    let mb: u64 = s.strip_suffix("MB")?.parse().ok()?;
    Some(mb * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> Vec<Part> {
        vec![
            Part {
                offset: 0x10,
                name: "app.bin".into(),
                data: vec![0xAA; 1000],
            },
            Part {
                offset: 0x4,
                name: "boot.bin".into(),
                data: vec![1, 2, 3],
            },
        ]
    }

    #[test]
    fn merges_with_ff_padding() {
        let m = merge_raw(&parts());
        assert_eq!(&m[..8], &[0xFF, 0xFF, 0xFF, 0xFF, 1, 2, 3, 0xFF]);
        assert_eq!(m.len(), 0x10 + 1000);
    }

    #[test]
    fn uf2_blocks() {
        let u = uf2(&parts(), 0x3D308E94);
        // 3 bytes → 1 block, 1000 bytes → 3 blocks of up to 452.
        assert_eq!(u.len(), 4 * 512);
        let word = |b: usize, i: usize| {
            u32::from_le_bytes(u[b * 512 + i * 4..b * 512 + i * 4 + 4].try_into().unwrap())
        };
        assert_eq!(word(0, 3), 0x4); // address
        assert_eq!(word(1, 3), 0x10);
        assert_eq!(word(2, 3), 0x10 + 452);
        assert_eq!(word(3, 4), 1000 - 2 * 452); // payload size
        assert_eq!(word(3, 5), 2); // block number
        assert_eq!(word(3, 6), 3); // blocks in file
        assert_eq!(word(3, 7), 0x3D308E94);
        assert_eq!(word(3, 127), UF2_FINAL_MAGIC);
    }

    #[test]
    fn dfu_suffix_and_crc() {
        let d = dfu(&parts(), 0x12, 4 << 20, DFU_PART_SIZE);
        let body = d.len() - 16;
        assert_eq!(body % 10240, 0);
        assert_eq!(&d[body + 8..body + 11], b"UFD");
        let crc = u32::from_le_bytes(d[d.len() - 4..].try_into().unwrap());
        assert_eq!(crc, !crc32fast::hash(&d[..d.len() - 4]));
        // The index comes first in the archive.
        assert_eq!(&d[110..122], b"dfuinfo0.dat");
    }
}
