//! Which patches belong to a game, and the patched game.
//!
//! We include a patch found beside a dropped game when it clearly belongs,
//! as we do with a `.sbi`, and export the patched game instead of the
//! original. What "clearly" means depends on what the format can prove.
//!
//! - BPS and UPS contain a checksum of the original game, which we check
//!   when we apply the patch with RetroArch's code, so a patch that applies
//!   was made for this game.
//! - An xdelta patch belongs when its header contains this game's file name,
//!   or when it contains xdelta3's window checksums and applies (with the
//!   wrong game, a window's checksum does not match and the patch fails).
//! - IPS proves nothing, so it belongs only when it has the game's name, as
//!   in RetroArch's own convention (`Game.sfc`, `Game.ips`).
//!
//! A patch the author chose (dropped with the game, or added) belongs when it
//! applies. We apply several in file-name order, each to the previous result.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::patching::{self, PatchFormat};

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

/// The patched game, and the patches we applied to make it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patched {
    pub patches: Vec<PathBuf>,
    #[serde(skip)]
    pub bytes: Vec<u8>,
    /// The file name of the patched game, when a patch states it. An xdelta
    /// header contains the name of the file it produces.
    pub made: Option<String>,
}

/// `game` with every patch in `offered` that belongs to it applied, in
/// file-name order, or None when no patch belongs.
pub fn apply_belonging(game: &Path, offered: &[(PathBuf, Offered)]) -> Result<Option<Patched>, String> {
    let mut ordered: Vec<&(PathBuf, Offered)> = offered.iter().collect();
    ordered.sort_by_key(|(path, _)| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
    let mut bytes = fs::read(game).map_err(|error| format!("read {}: {error}", game.display()))?;
    let game_name = game.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let game_stem = game.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let mut applied = Vec::new();
    let mut made = None;
    for (path, how) in ordered {
        let patch = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let Some(format) = PatchFormat::of(&patch) else {
            continue;
        };
        let named_for_game = path.file_stem().is_some_and(|stem| stem.to_string_lossy() == game_stem);
        let names = xdelta_names(&patch).unwrap_or_default();
        let may_try = match (how, format) {
            (Offered::Chosen, _) => true,
            (Offered::Beside, PatchFormat::Bps | PatchFormat::Ups) => true,
            (Offered::Beside, PatchFormat::Xdelta) => {
                named_for_game || names.checked || names.from.as_deref() == Some(game_name.as_str())
            }
            (Offered::Beside, PatchFormat::Ips) => named_for_game,
        };
        if !may_try {
            continue;
        }
        if let Ok(result) = patching::apply(&patch, &bytes) {
            bytes = result;
            applied.push(path.clone());
            if names.made.is_some() {
                made = names.made;
            }
        }
    }
    if applied.is_empty() {
        return Ok(None);
    }
    Ok(Some(Patched { patches: applied, bytes, made }))
}
