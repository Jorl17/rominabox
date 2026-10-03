//! Write an uncompressed CHD (version 5) of a CD in the same form as
//! `chdman`, for the `chd` crate. It has a header, a map of hunk positions,
//! the track table as `CHT2` metadata, and the frames, each 2,352 bytes of
//! sector and 96 of subcode, with a track's frames padded to a multiple of
//! four. This is the only CHD writer in the repository.

use std::fs;
use std::path::Path;

const FRAME_BYTES: usize = 2352 + 96;
const HUNK_BYTES: usize = FRAME_BYTES * 8;
const HEADER_BYTES: usize = 124;

/// A track, with its `TYPE` in the track table, the bytes we write out for
/// it, and the frames of pregap among them.
pub struct CdTrack<'a> {
    pub kind: &'a str,
    pub bytes: &'a [u8],
    pub pregap: usize,
}

fn sector_bytes(kind: &str) -> usize {
    match kind {
        "MODE1" | "MODE2_FORM1" => 2048,
        "MODE2_FORM2" => 2324,
        "MODE2" | "MODE2_FORM_MIX" => 2336,
        _ => 2352,
    }
}

pub fn write_cd(path: &Path, tracks: &[CdTrack]) {
    let mut frames: Vec<u8> = Vec::new();
    let mut table = Vec::new();
    for (index, track) in tracks.iter().enumerate() {
        let sector = sector_bytes(track.kind);
        assert_eq!(track.bytes.len() % sector, 0, "a track is whole sectors");
        let count = track.bytes.len() / sector;
        for chunk in track.bytes.chunks(sector) {
            let mut frame = vec![0u8; FRAME_BYTES];
            if track.kind == "AUDIO" {
                // Audio in a CHD is big-endian.
                for (pair, stored) in chunk.chunks_exact(2).zip(frame.chunks_exact_mut(2)) {
                    stored.copy_from_slice(&[pair[1], pair[0]]);
                }
            } else {
                frame[..sector].copy_from_slice(chunk);
            }
            frames.extend_from_slice(&frame);
        }
        frames.resize(frames.len() + (count.div_ceil(4) * 4 - count) * FRAME_BYTES, 0);
        let pregap_type = if track.kind == "AUDIO" { "VAUDIO" } else { "VMODE1" };
        table.push(format!(
            "TRACK:{} TYPE:{} SUBTYPE:NONE FRAMES:{count} PREGAP:{} PGTYPE:{} PGSUB:NONE POSTGAP:0\0",
            index + 1,
            track.kind,
            track.pregap,
            if track.pregap > 0 { pregap_type } else { "MODE1" },
        ));
    }
    let logical = frames.len();
    frames.resize(logical.div_ceil(HUNK_BYTES) * HUNK_BYTES, 0);
    let hunks = frames.len() / HUNK_BYTES;

    let map_offset = HEADER_BYTES;
    let meta_offset = map_offset + hunks * 4;
    let mut metadata = Vec::new();
    for (index, row) in table.iter().enumerate() {
        let start = meta_offset + metadata.len();
        let next = if index + 1 == table.len() { 0 } else { start + 16 + row.len() };
        metadata.extend_from_slice(b"CHT2");
        metadata.extend_from_slice(&((1u32 << 24) | row.len() as u32).to_be_bytes());
        metadata.extend_from_slice(&(next as u64).to_be_bytes());
        metadata.extend_from_slice(row.as_bytes());
    }
    let first_hunk = (meta_offset + metadata.len()).div_ceil(HUNK_BYTES);

    let mut file = Vec::new();
    file.extend_from_slice(b"MComprHD");
    file.extend_from_slice(&(HEADER_BYTES as u32).to_be_bytes());
    file.extend_from_slice(&5u32.to_be_bytes());
    file.extend_from_slice(&[0u8; 16]); // no compressors
    file.extend_from_slice(&(logical as u64).to_be_bytes());
    file.extend_from_slice(&(map_offset as u64).to_be_bytes());
    file.extend_from_slice(&(meta_offset as u64).to_be_bytes());
    file.extend_from_slice(&(HUNK_BYTES as u32).to_be_bytes());
    file.extend_from_slice(&(FRAME_BYTES as u32).to_be_bytes());
    file.resize(HEADER_BYTES, 0); // the three digests
    for hunk in 0..hunks {
        file.extend_from_slice(&((first_hunk + hunk) as u32).to_be_bytes());
    }
    file.extend_from_slice(&metadata);
    file.resize(first_hunk * HUNK_BYTES, 0);
    file.extend_from_slice(&frames);
    fs::write(path, file).unwrap();
}
