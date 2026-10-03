//! A compressed CD image (CHD) as its tracks, so that we can check a patch
//! made for a track against it, and write the disc out as `.bin` tracks and
//! a `.cue` sheet, which is how we export a patched CHD game.
//!
//! A CHD contains each CD frame as 2,352 bytes of sector and 96 of subcode,
//! with a track's frames padded to a multiple of four, and its track table as
//! metadata (`CHT2`, or the older `CHTR`). We write the tracks as `chdman
//! extractcd` does: each frame's data bytes, with audio swapped back to
//! little-endian, named as Redump names a disc's tracks. We do not read
//! GD-ROM CHDs (`CHGD`) here.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

/// The bytes in a CHD for one CD frame: the sector, then its subcode.
const FRAME_BYTES: u64 = 2352 + 96;
/// The frames of a track in a CHD fill a multiple of this.
const TRACK_PADDING: u64 = 4;
const CHT2: u32 = u32::from_be_bytes(*b"CHT2");
const CHTR: u32 = u32::from_be_bytes(*b"CHTR");
const CHGD: u32 = u32::from_be_bytes(*b"CHGD");

/// A track's sector format, as a CHD's track table names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Mode1,
    Mode1Raw,
    Mode2,
    Mode2Form1,
    Mode2Form2,
    Mode2FormMix,
    Mode2Raw,
    Audio,
}

impl TrackKind {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "MODE1" => Self::Mode1,
            "MODE1_RAW" => Self::Mode1Raw,
            "MODE2" => Self::Mode2,
            "MODE2_FORM1" => Self::Mode2Form1,
            "MODE2_FORM2" => Self::Mode2Form2,
            "MODE2_FORM_MIX" => Self::Mode2FormMix,
            "MODE2_RAW" => Self::Mode2Raw,
            "AUDIO" => Self::Audio,
            _ => return None,
        })
    }

    /// The bytes of each frame that are the track's.
    pub fn sector_bytes(self) -> u64 {
        match self {
            Self::Mode1 | Self::Mode2Form1 => 2048,
            Self::Mode2Form2 => 2324,
            Self::Mode2 | Self::Mode2FormMix => 2336,
            Self::Mode1Raw | Self::Mode2Raw | Self::Audio => 2352,
        }
    }

    /// The track's type in a cue sheet.
    fn cue_type(self) -> String {
        match self {
            Self::Audio => "AUDIO".into(),
            Self::Mode1 | Self::Mode1Raw => format!("MODE1/{}", self.sector_bytes()),
            _ => format!("MODE2/{}", self.sector_bytes()),
        }
    }
}

/// One track of the disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub kind: TrackKind,
    /// Frames in the CHD for it, including its stored pregap.
    pub frames: u64,
    /// Frames of pregap before INDEX 01.
    pub pregap: u64,
    /// Whether the pregap's frames are among `frames`. Otherwise the sheet
    /// declares them as silence.
    pub pregap_stored: bool,
    /// The file we write it as.
    pub name: String,
    /// The frame of the CHD it starts at.
    first_frame: u64,
}

impl Track {
    /// The size of the file we write it as.
    pub fn bytes(&self) -> u64 {
        self.frames * self.kind.sector_bytes()
    }
}

/// A CD image's tracks, as the files we write them as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disc {
    pub tracks: Vec<Track>,
    /// The sheet's name.
    pub sheet_name: String,
}

impl Disc {
    /// The bytes of all the tracks together, as we write them out.
    pub fn bytes(&self) -> u64 {
        self.tracks.iter().map(Track::bytes).sum()
    }

    /// The cue sheet naming the tracks.
    pub fn sheet(&self) -> String {
        let mut sheet = String::new();
        for (index, track) in self.tracks.iter().enumerate() {
            sheet.push_str(&format!("FILE \"{}\" BINARY\n", track.name));
            sheet.push_str(&format!("  TRACK {:02} {}\n", index + 1, track.kind.cue_type()));
            if track.pregap > 0 && track.pregap_stored {
                sheet.push_str("    INDEX 00 00:00:00\n");
                sheet.push_str(&format!("    INDEX 01 {}\n", msf(track.pregap)));
            } else {
                if track.pregap > 0 {
                    sheet.push_str(&format!("    PREGAP {}\n", msf(track.pregap)));
                }
                sheet.push_str("    INDEX 01 00:00:00\n");
            }
        }
        sheet
    }
}

/// Minutes, seconds and frames, 75 frames a second.
fn msf(frames: u64) -> String {
    format!("{:02}:{:02}:{:02}", frames / 75 / 60, frames / 75 % 60, frames % 75)
}

/// The disc in the CHD at `path`, with its files named after `stem` in the
/// Redump way: `stem.bin` for a disc of one track, otherwise
/// `stem (Track N).bin`, the number with two digits when there are ten or
/// more, and `stem.cue`.
pub fn read(path: &Path, stem: &str) -> Result<Disc, String> {
    let unreadable = |error: chd::Error| format!("{} could not be read as a CHD: {error}", path.display());
    let mut file = BufReader::new(File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?);
    let mut image = chd::Chd::open(&mut file, None).map_err(unreadable)?;
    let mut rows = Vec::new();
    let refs: Vec<_> = image.metadata_refs().collect();
    for entry in refs {
        let metadata = entry.read(image.inner()).map_err(unreadable)?;
        match metadata.metatag {
            CHT2 | CHTR => rows.push(String::from_utf8_lossy(&metadata.value).trim_end_matches('\0').to_string()),
            CHGD => return Err("A GD-ROM CHD cannot be patched yet.".into()),
            _ => {}
        }
    }
    if rows.is_empty() {
        return Err(format!("{} is not a CD image.", path.display()));
    }
    let count = rows.len();
    let mut tracks = Vec::with_capacity(count);
    let mut first_frame = 0;
    for (index, row) in rows.iter().enumerate() {
        let field = |key: &str| {
            row.split_whitespace().find_map(|pair| pair.strip_prefix(key).and_then(|rest| rest.strip_prefix(':')))
        };
        let number = |key: &str| field(key).map_or(Ok(0), |value| value.parse::<u64>());
        let bad = || format!("{} has a track it does not describe: {row}", path.display());
        let kind = field("TYPE").and_then(TrackKind::named).ok_or_else(bad)?;
        let frames = number("FRAMES").map_err(|_| bad())?;
        let pregap = number("PREGAP").map_err(|_| bad())?;
        let pregap_stored = field("PGTYPE").is_some_and(|value| value.starts_with('V'));
        let name = match count {
            1 => format!("{stem}.bin"),
            2..=9 => format!("{stem} (Track {}).bin", index + 1),
            _ => format!("{stem} (Track {:02}).bin", index + 1),
        };
        tracks.push(Track { kind, frames, pregap, pregap_stored, name, first_frame });
        first_frame += frames.div_ceil(TRACK_PADDING) * TRACK_PADDING;
    }
    let held = image.header().logical_bytes() / FRAME_BYTES;
    if first_frame > held.div_ceil(TRACK_PADDING) * TRACK_PADDING {
        return Err(format!("{}'s track table describes more than it holds.", path.display()));
    }
    Ok(Disc { tracks, sheet_name: format!("{stem}.cue") })
}

/// Write `track` of the CHD at `path` to `out`, a hunk at a time.
pub fn write_track(path: &Path, track: &Track, out: &mut impl Write) -> io::Result<()> {
    let failed = |error: chd::Error| io::Error::other(format!("{}: {error}", path.display()));
    let mut file = BufReader::new(File::open(path)?);
    let mut image = chd::Chd::open(&mut file, None).map_err(failed)?;
    let hunk_bytes = u64::from(image.header().hunk_size());
    let hunk_count = image.header().hunk_count();
    let mut hunk = image.get_hunksized_buffer();
    let mut compressed = Vec::new();
    let mut loaded = None;
    let keep = track.kind.sector_bytes() as usize;
    let mut sector = vec![0u8; keep];
    for frame in track.first_frame..track.first_frame + track.frames {
        let at = frame * FRAME_BYTES;
        let index = u32::try_from(at / hunk_bytes).map_err(io::Error::other)?;
        if index >= hunk_count {
            return Err(io::Error::other(format!("{} ends inside a track", path.display())));
        }
        if loaded != Some(index) {
            image.hunk(index).and_then(|mut read| read.read_hunk_in(&mut compressed, &mut hunk)).map_err(failed)?;
            loaded = Some(index);
        }
        let start = (at % hunk_bytes) as usize;
        let frame_data = hunk.get(start..start + keep).ok_or_else(|| io::Error::other("a frame crosses a hunk"))?;
        if track.kind == TrackKind::Audio {
            for (pair, swapped) in frame_data.chunks_exact(2).zip(sector.chunks_exact_mut(2)) {
                swapped.copy_from_slice(&[pair[1], pair[0]]);
            }
            out.write_all(&sector)?;
        } else {
            out.write_all(frame_data)?;
        }
    }
    Ok(())
}

/// The CRC-32 of `track` as we write it out. To compute it we decompress the
/// track, so we compute it once for each CHD and track, and do not decompress
/// the same file (by its size and modification time) again.
pub fn track_crc(path: &Path, track: &Track) -> io::Result<u32> {
    type Key = (PathBuf, u64, Option<SystemTime>, u64, u64);
    static KNOWN: OnceLock<Mutex<HashMap<Key, u32>>> = OnceLock::new();
    let metadata = fs::metadata(path)?;
    let key = (path.to_path_buf(), metadata.len(), metadata.modified().ok(), track.first_frame, track.frames);
    let known = KNOWN.get_or_init(Default::default);
    if let Some(crc) = known.lock().ok().and_then(|known| known.get(&key).copied()) {
        return Ok(crc);
    }
    let crc = decompressed_crc(path, track)?;
    if let Ok(mut known) = known.lock() {
        known.insert(key, crc);
    }
    Ok(crc)
}

fn decompressed_crc(path: &Path, track: &Track) -> io::Result<u32> {
    struct Hashing(crc32fast::Hasher);
    impl Write for Hashing {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut hashing = Hashing(crc32fast::Hasher::new());
    write_track(path, track, &mut hashing)?;
    Ok(hashing.0.finalize())
}
