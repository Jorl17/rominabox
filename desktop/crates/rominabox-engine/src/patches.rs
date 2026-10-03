//! Which patches belong to a game, decided without applying them.
//!
//! We include a patch found beside a dropped game when it clearly belongs,
//! as we do with a `.sbi`, and export the patched game instead of the
//! original. What "clearly" means depends on what the patch format states.
//!
//! - BPS and UPS state the size and CRC-32 of the original game, and we
//!   compare them with the game's.
//! - An xdelta patch belongs when its header contains this game's file name,
//!   or when it has the game's name.
//! - IPS states nothing, so it belongs only when it has the game's name, as
//!   in RetroArch's own convention (`Game.sfc`, `Game.ips`).
//!
//! A patch the author chose (dropped with the game, or added) belongs unless
//! what it states contradicts the game. We apply several in file-name order,
//! each to the previous result, and a BPS or UPS patch also states the game
//! it produces, so we check the next patch against that. We apply nothing
//! here. At export we apply them once (crate::patching::apply_files), and
//! refuse the export when a patch fails.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::patching::PatchFormat;

/// The file extensions of patch files.
const PATCH_EXTENSIONS: [&str; 5] = ["ips", "ups", "bps", "xdelta", "vcdiff"];

/// Whether a file is a patch, by its extension: the RetroArch extensions,
/// with the numbered forms (`.ips1` to `.ips9`).
pub fn is_patch_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    let base = extension.trim_end_matches(|character: char| character.is_ascii_digit());
    PATCH_EXTENSIONS.contains(&base) && extension.len() - base.len() <= 1
}

/// The patch files in `folder`, in name order.
pub fn in_folder(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_patch_file(path))
        .collect();
    found.sort_by_key(|path| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
    found
}

/// The contents of an xdelta patch header, in xdelta3's format: the name of
/// the patched file and the name of the original.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XdeltaNames {
    pub made: Option<String>,
    pub from: Option<String>,
    /// Whether its windows contain xdelta3's Adler-32 checksums.
    pub checked: bool,
}

/// The names and the checksum flag in an xdelta patch header. The xdelta3
/// application header is `made/…/from/…`, and each name can be followed by
/// `#` and a digest.
pub fn xdelta_names(patch: &[u8]) -> Option<XdeltaNames> {
    if PatchFormat::of(patch) != Some(PatchFormat::Xdelta) {
        return None;
    }
    let mut at = 4;
    let indicator = *patch.get(at)?;
    at += 1;
    if indicator & 0x01 != 0 {
        at += 1; // the secondary compressor's id
    }
    if indicator & 0x02 != 0 {
        return Some(XdeltaNames::default()); // a code table: not read here
    }
    let mut names = XdeltaNames::default();
    if indicator & 0x04 != 0 {
        let length = vcdiff_number(patch, &mut at)? as usize;
        let header = patch.get(at..at.checked_add(length)?)?;
        at += length;
        let text = String::from_utf8_lossy(header);
        let fields: Vec<&str> = text.split('/').collect();
        let name = |field: Option<&&str>| {
            field
                .map(|value| value.split('#').next().unwrap_or("").trim().to_string())
                .filter(|value| !value.is_empty())
        };
        names.made = name(fields.first());
        names.from = name(fields.get(2));
    }
    names.checked = patch.get(at).is_some_and(|window| window & 0x04 != 0);
    Some(names)
}

/// A VCDIFF number, in big-endian base-128.
fn vcdiff_number(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let mut value: u32 = 0;
    for _ in 0..5 {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value = value.checked_mul(128)? | u32::from(byte & 0x7F);
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Where a patch came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offered {
    /// Found beside the game.
    Beside,
    /// Dropped with the game, or added by the author.
    Chosen,
}

/// The patches that belong to a game, and what they state about the result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Belonging {
    /// The patches we apply at export, in the order we apply them.
    pub patches: Vec<PathBuf>,
    /// The file name of the patched game, when a patch states it. An xdelta
    /// header contains the name of the file it produces.
    pub made: Option<String>,
    /// Patches the author chose that, by what they state, are for another game.
    pub refused: Vec<PathBuf>,
}

/// What we know of the game we would apply a patch to: the original, or the
/// result of the patches before it.
struct Game {
    size: Option<u64>,
    crc: Crc,
    /// Its file name, which an xdelta header can contain.
    name: String,
}

enum Crc {
    /// The original's, which we read from its file when we apply a patch.
    OfFile(PathBuf),
    Known(u32),
    Unknown,
}

impl Game {
    fn crc(&mut self) -> io::Result<Option<u32>> {
        if let Crc::OfFile(path) = &self.crc {
            self.crc = Crc::Known(file_crc(path)?);
        }
        Ok(match self.crc {
            Crc::Known(crc) => Some(crc),
            _ => None,
        })
    }
}

/// The CRC-32 of a file, which we read a piece at a time.
fn file_crc(path: &Path) -> io::Result<u32> {
    let mut file = File::open(path)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        match file.read(&mut buffer)? {
            0 => return Ok(hasher.finalize()),
            count => hasher.update(&buffer[..count]),
        }
    }
}

/// How a patch fits the game, and the result when we apply it.
enum Fit {
    /// What it states matches this game.
    Proved(Game),
    /// It states nothing that rules this game out.
    Possible(Game),
    /// What it states matches another game.
    Disproved,
}

/// The parts of a patch file that we read for the checks: its start, with
/// the header of every format, and its last twelve bytes, with the BPS and
/// UPS checksums. A patch for a disc can be large, and we need nothing else.
struct PatchStart {
    head: Vec<u8>,
    tail: Vec<u8>,
}

const HEAD_BYTES: u64 = 64 * 1024;

fn read_start(path: &Path) -> io::Result<PatchStart> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let mut head = Vec::new();
    (&mut file).take(HEAD_BYTES).read_to_end(&mut head)?;
    let mut tail = Vec::new();
    file.seek(SeekFrom::Start(length.saturating_sub(12)))?;
    file.read_to_end(&mut tail)?;
    Ok(PatchStart { head, tail })
}

/// What a BPS or UPS patch states: the size and CRC-32 of the original game
/// and of the patched game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Declared {
    source: (u64, u32),
    target: (u64, u32),
}

/// What `head`, the start of a BPS or UPS patch, and `tail`, its last twelve
/// bytes, state about the original and the patched game.
fn declared(head: &[u8], tail: &[u8]) -> Option<Declared> {
    match PatchFormat::of(head)? {
        PatchFormat::Bps | PatchFormat::Ups => {}
        _ => return None,
    }
    let checksums: &[u8; 12] = tail.try_into().ok()?;
    let word = |at: usize| u32::from_le_bytes([checksums[at], checksums[at + 1], checksums[at + 2], checksums[at + 3]]);
    let mut at = 4;
    let source = byuu_number(head, &mut at)?;
    let target = byuu_number(head, &mut at)?;
    Some(Declared { source: (source, word(0)), target: (target, word(4)) })
}

/// byuu's number format, which BPS and UPS use for sizes: little-endian
/// base-128, plus one per continuation.
fn byuu_number(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let (mut value, mut shift) = (0u64, 0u32);
    for _ in 0..10 {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value = value.checked_add(u64::from(byte & 0x7F).checked_shl(shift)?)?;
        if byte & 0x80 != 0 {
            return Some(value);
        }
        shift += 7;
        value = value.checked_add(1u64.checked_shl(shift)?)?;
    }
    None
}

/// How `patch`, read as `start`, fits `game`. `named_for_game` is whether the
/// patch has the game's name.
fn fit(start: &PatchStart, format: PatchFormat, named_for_game: bool, game: &mut Game) -> io::Result<Fit> {
    let unknown = |name: String| Game { size: None, crc: Crc::Unknown, name };
    Ok(match format {
        PatchFormat::Bps | PatchFormat::Ups => {
            let Some(declared) = declared(&start.head, &start.tail) else {
                return Ok(Fit::Disproved);
            };
            // A UPS patch applies in either direction, as in RetroArch.
            let ways: &[((u64, u32), (u64, u32))] = match format {
                PatchFormat::Ups => &[(declared.source, declared.target), (declared.target, declared.source)],
                _ => &[(declared.source, declared.target)],
            };
            let makes = |(size, crc): (u64, u32), name: &str| Game {
                size: Some(size),
                crc: Crc::Known(crc),
                name: name.to_string(),
            };
            let sized: Vec<_> =
                ways.iter().filter(|(from, _)| game.size.is_none_or(|size| size == from.0)).collect();
            if sized.is_empty() {
                Fit::Disproved
            } else {
                match game.crc()? {
                    Some(crc) => match sized.iter().find(|(from, _)| from.1 == crc) {
                        Some((_, to)) => Fit::Proved(makes(*to, &game.name)),
                        None => Fit::Disproved,
                    },
                    None => Fit::Possible(makes(sized[0].1, &game.name)),
                }
            }
        }
        PatchFormat::Xdelta => {
            let names = xdelta_names(&start.head).unwrap_or_default();
            let next = unknown(names.made.clone().unwrap_or_else(|| game.name.clone()));
            if named_for_game || names.from.as_deref() == Some(game.name.as_str()) {
                Fit::Proved(next)
            } else {
                Fit::Possible(next)
            }
        }
        PatchFormat::Ips => {
            let next = unknown(game.name.clone());
            if named_for_game {
                Fit::Proved(next)
            } else {
                Fit::Possible(next)
            }
        }
    })
}

/// The patches in `offered` that belong to `game`, in file-name order. A
/// patch found beside the game belongs when what it states proves that it is
/// for this game. A patch the author chose belongs unless it states otherwise.
pub fn belonging(game: &Path, offered: &[(PathBuf, Offered)]) -> Result<Belonging, String> {
    let mut ordered: Vec<&(PathBuf, Offered)> = offered.iter().collect();
    ordered.sort_by_key(|(path, _)| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
    let size = fs::metadata(game).map_err(|error| format!("read {}: {error}", game.display()))?.len();
    let game_stem = game.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let mut current = Game {
        size: Some(size),
        crc: Crc::OfFile(game.to_path_buf()),
        name: game.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    let mut found = Belonging::default();
    for (path, how) in ordered {
        let start = read_start(path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let named_for_game = path.file_stem().is_some_and(|stem| stem.to_string_lossy() == game_stem);
        let fit = match PatchFormat::of(&start.head) {
            Some(format) => fit(&start, format, named_for_game, &mut current)
                .map_err(|error| format!("read {}: {error}", game.display()))?,
            None => Fit::Disproved,
        };
        let next = match (fit, how) {
            (Fit::Proved(next), _) | (Fit::Possible(next), Offered::Chosen) => next,
            (_, Offered::Chosen) => {
                found.refused.push(path.clone());
                continue;
            }
            (_, Offered::Beside) => continue,
        };
        if PatchFormat::of(&start.head) == Some(PatchFormat::Xdelta) {
            found.made = xdelta_names(&start.head).and_then(|names| names.made).or(found.made);
        }
        found.patches.push(path.clone());
        current = next;
    }
    Ok(found)
}

/// When someone drops a patch on its own, the game in `patch`'s folder that
/// it is for: the one with the patch's name, the one in an xdelta header, or
/// the one with the size and CRC-32 that a BPS or UPS patch states. A patch
/// that states neither is for the only game in the folder. `games` are the
/// game files in the folder.
pub fn game_for(patch: &Path, games: &[PathBuf]) -> Result<PathBuf, String> {
    let start = read_start(patch).map_err(|error| format!("read {}: {error}", patch.display()))?;
    let named = |wanted: &str| {
        games.iter().find(|game| game.file_stem().is_some_and(|stem| stem.to_string_lossy() == wanted))
    };
    if let Some(game) = patch.file_stem().and_then(|stem| named(&stem.to_string_lossy())) {
        return Ok(game.clone());
    }
    if let Some(from) = xdelta_names(&start.head).and_then(|names| names.from) {
        if let Some(game) = games.iter().find(|game| game.file_name().is_some_and(|name| name.to_string_lossy() == from)) {
            return Ok(game.clone());
        }
    }
    let fits: Vec<&PathBuf> = games
        .iter()
        .filter(|game| {
            belonging(game, &[(patch.to_path_buf(), Offered::Chosen)])
                .is_ok_and(|belonging| !belonging.patches.is_empty())
        })
        .collect();
    match fits.as_slice() {
        [game] => Ok((*game).clone()),
        [] => Err(format!(
            "{} is for none of the games in its folder. Drop the game with it.",
            patch.file_name().unwrap_or_default().to_string_lossy()
        )),
        _ => Err(format!(
            "{} could be for more than one game in its folder. Drop the game with it.",
            patch.file_name().unwrap_or_default().to_string_lossy()
        )),
    }
}

/// Feed `hash` the game's bytes followed by each patch's bytes, in order,
/// which identifies the patched game without making it. When we cannot read
/// a file, we return it with its error.
pub fn feed_patched(
    hash: &mut impl sha2::Digest,
    game: &Path,
    patches: &[PathBuf],
) -> Result<(), (PathBuf, io::Error)> {
    let mut buffer = vec![0u8; 1 << 20];
    for path in std::iter::once(game).chain(patches.iter().map(PathBuf::as_path)) {
        let failed = |error| (path.to_path_buf(), error);
        let mut file = File::open(path).map_err(failed)?;
        loop {
            match file.read(&mut buffer).map_err(failed)? {
                0 => break,
                count => hash.update(&buffer[..count]),
            }
        }
    }
    Ok(())
}
