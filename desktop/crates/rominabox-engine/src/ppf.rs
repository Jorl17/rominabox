//! PPF patches, which contain bytes to write at positions in a disc image.
//!
//! The PlayStation emulator, PCSX ReARMed, can load one PPF while the game
//! runs, so we merge several PPFs for one disc into one (`merged`). For any
//! other console, we write a PPF's bytes into the game at export (`apply`).
//! We read the same PPF 1.0, 2.0 and 3.0 layouts as `libpcsxcore/ppf.c` in
//! PCSX ReARMed, as follows.
//!
//! - every version: `PPFx0`, an encoding byte, a 50-byte description.
//! - 1.0: then records of a 4-byte position, a length byte and the bytes.
//! - 2.0: then the image's size (4 bytes) and 1,024 bytes of it to check,
//!   then the 1.0 records, and an optional FILE_ID.DIZ at the end, with its
//!   length in the last 4 bytes and `.DIZ` in the 4 before.
//! - 3.0: then an image type, whether the 1,024-byte check is there, whether
//!   each record contains the bytes it replaces (undo) and a spare byte, then
//!   records with an 8-byte position, and an optional FILE_ID.DIZ with its
//!   length in the last 2 bytes and `.DIZ` in the 4 before.
//!
//! Positions are little-endian.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

const DESCRIPTION_END: usize = 56;
const CHECK_BYTES: usize = 1024;

/// One record: bytes to write at a position.
pub type Record = (u64, Vec<u8>);

/// Whether `head`, the start of a file, is the start of a PPF patch:
/// `PPF10`, `PPF20` or `PPF30`.
pub fn is_ppf(head: &[u8]) -> bool {
    head.len() >= 5 && &head[..3] == b"PPF" && matches!(&head[3..5], b"10" | b"20" | b"30")
}

/// The records of the PPF patch `patch`, in its order.
pub fn records(patch: &[u8]) -> Result<Vec<Record>, String> {
    let bad = |why: &str| format!("not a PPF patch this builder can read: {why}");
    if patch.len() < DESCRIPTION_END || &patch[..3] != b"PPF" {
        return Err(bad("no PPF header"));
    }
    let (start, end, position_bytes, undo) = match &patch[3..5] {
        b"10" => (DESCRIPTION_END, patch.len(), 4, false),
        b"20" => {
            let start = DESCRIPTION_END + 4 + CHECK_BYTES;
            let diz = if patch.len() >= start + 8 && &patch[patch.len() - 8..patch.len() - 4] == b".DIZ" {
                let length = u32::from_le_bytes(patch[patch.len() - 4..].try_into().unwrap()) as usize;
                length.checked_add(18 + 16 + 4).ok_or_else(|| bad("its description's length"))?
            } else {
                0
            };
            (start, patch.len().checked_sub(diz).ok_or_else(|| bad("its description's length"))?, 4, false)
        }
        b"30" => {
            let flags = patch.get(DESCRIPTION_END..DESCRIPTION_END + 4).ok_or_else(|| bad("a short header"))?;
            let start = DESCRIPTION_END + 4 + if flags[1] != 0 { CHECK_BYTES } else { 0 };
            let diz = if patch.len() >= start + 6 && &patch[patch.len() - 6..patch.len() - 2] == b".DIZ" {
                let length = u16::from_le_bytes(patch[patch.len() - 2..].try_into().unwrap()) as usize;
                length + 18 + 16 + 2
            } else {
                0
            };
            (start, patch.len().checked_sub(diz).ok_or_else(|| bad("its description's length"))?, 8, flags[2] != 0)
        }
        _ => return Err(bad("an unknown version")),
    };
    if start > end {
        return Err(bad("a short header"));
    }
    let mut found = Vec::new();
    let mut at = start;
    while at < end {
        let position_end = at + position_bytes;
        let position = patch.get(at..position_end).ok_or_else(|| bad("a record cut short"))?;
        let position = if position_bytes == 4 {
            u64::from(u32::from_le_bytes(position.try_into().unwrap()))
        } else {
            u64::from_le_bytes(position.try_into().unwrap())
        };
        let length = *patch.get(position_end).ok_or_else(|| bad("a record cut short"))? as usize;
        let bytes = patch.get(position_end + 1..position_end + 1 + length).ok_or_else(|| bad("a record cut short"))?;
        found.push((position, bytes.to_vec()));
        at = position_end + 1 + length + if undo { length } else { 0 };
    }
    if at != end {
        return Err(bad("a record cut short"));
    }
    Ok(found)
}

/// One PPF 3.0 patch with the writes of all `patches`, each in turn, so where
/// two write the same position, the byte from the later patch is the result.
pub fn merged(patches: &[Vec<Record>]) -> Vec<u8> {
    let mut bytes: BTreeMap<u64, u8> = BTreeMap::new();
    for (position, data) in patches.iter().flatten() {
        for (offset, byte) in data.iter().enumerate() {
            bytes.insert(position + offset as u64, *byte);
        }
    }
    let mut patch = b"PPF30\x02".to_vec();
    patch.extend_from_slice(&[b' '; 50]);
    patch.extend_from_slice(&[0, 0, 0, 0]); // image type, no check, no undo, spare
    let mut run: Option<(u64, Vec<u8>)> = None;
    let flush = |run: &mut Option<(u64, Vec<u8>)>, patch: &mut Vec<u8>| {
        if let Some((position, data)) = run.take() {
            patch.extend_from_slice(&position.to_le_bytes());
            patch.push(data.len() as u8);
            patch.extend_from_slice(&data);
        }
    };
    for (position, byte) in bytes {
        match &mut run {
            Some((start, data)) if *start + data.len() as u64 == position && data.len() < 255 => data.push(byte),
            _ => {
                flush(&mut run, &mut patch);
                run = Some((position, vec![byte]));
            }
        }
    }
    flush(&mut run, &mut patch);
    patch
}

/// Write the game in `game` with `records` into the new file `made`. We copy
/// the game, then write each record's bytes at its position. We extend the
/// file for a record past the game's end.
pub fn apply(records: &[Record], game: &Path, made: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(made)?;
    let written = (|| {
        io::copy(&mut fs::File::open(game)?, &mut file)?;
        for (position, bytes) in records {
            file.seek(SeekFrom::Start(*position))?;
            file.write_all(bytes)?;
        }
        file.flush()
    })();
    if written.is_err() {
        drop(file);
        let _ = fs::remove_file(made);
    }
    written
}
