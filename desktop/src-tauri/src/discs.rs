//! Serial numbers printed inside a disc, with which we identify it without
//! reading the whole image.
//!
//! The catalogues next to the cartridge lists (the redump set in
//! libretro-database) contain that serial for each game. A PlayStation disc
//! has it as `SLUS_012.34`, and the catalogue as `SLUS-01234`. We ignore
//! punctuation, and the letters and digits must match exactly. In a CHD we
//! decompress only the first sectors. We do not read cdi, pbp or rvz. This
//! way we never hash a whole DVD.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const PREFIX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscRead {
    Found {
        keys: Vec<String>,
        system_id: Option<&'static str>,
        /// The bytes we took the serial from. We read a title declared relative
        /// to the same signature from here, so we never decompress a CHD twice.
        prefix: Vec<u8>,
    },
    /// The serial is inside a container that we do not open here. We must
    /// not then hash the image instead.
    Compressed,
    Unreadable(String),
}

pub fn read_disc(path: &Path, extension: &str) -> DiscRead {
    if extension == "chd" {
        return read_chd(path);
    }
    if matches!(extension, "cdi" | "pbp" | "rvz") {
        return DiscRead::Compressed;
    }
    if extension == "m3u" {
        let nested = match m3u_first(path) {
            Ok(nested) => nested,
            Err(message) => return DiscRead::Unreadable(message),
        };
        let nested_extension = crate::dumps::extension_of(
            nested
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        );
        return read_disc(&nested, &nested_extension);
    }
    let media = match extension {
        "cue" => cue_data_file(path),
        "gdi" => gdi_data_file(path),
        _ => Ok(path.to_owned()),
    };
    let media = match media {
        Ok(media) => media,
        Err(message) => return DiscRead::Unreadable(message),
    };
    let bytes = match read_prefix(&media) {
        Ok(bytes) => bytes,
        Err(message) => return DiscRead::Unreadable(message),
    };
    let (system_id, keys) = keys_from_bytes(&bytes, extension);
    DiscRead::Found {
        keys,
        system_id,
        prefix: bytes,
    }
}

/// Every lookup key for a catalogue serial.
///
/// We also index a serial with a two-digit suffix such as `-50` without the
/// suffix, because the Dreamcast and Sega CD catalogues add a region that
/// the disc header does not have. In the caller we drop a key that belongs
/// to two games, so the shorter form never matches the wrong game.
pub fn keys_for_catalog_serial(system_id: &str, serial: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for part in serial.split(['/', ',']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        push_key(&mut keys, &normalize_serial(part));
        if let Some((body, suffix)) = part.rsplit_once('-') {
            if suffix.len() == 2 && suffix.chars().all(|character| character.is_ascii_digit()) {
                push_key(&mut keys, &normalize_serial(body));
            }
        }
        if system_id == "gamecube" {
            if let Some(code) = gamecube_code_in_serial(part) {
                push_key(&mut keys, &code);
            }
        }
    }
    keys
}

pub fn normalize_serial(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

fn push_key(keys: &mut Vec<String>, key: &str) {
    if key.len() >= 4 && !keys.iter().any(|existing| existing == key) {
        keys.push(key.to_owned());
    }
}

fn cue_data_file(path: &Path) -> Result<PathBuf, String> {
    let text =
        std::fs::read_to_string(path).map_err(|_| "The cue sheet could not be read.".to_owned())?;
    let mut current = None;
    let mut data = None;
    for line in text.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("FILE ") {
            let original = trimmed[trimmed.len() - rest.len()..].trim();
            current = Some(quoted_or_token(original));
            continue;
        }
        if upper.starts_with("TRACK ") && (upper.contains("MODE1") || upper.contains("MODE2")) {
            data = current.clone();
            break;
        }
    }
    let name = data.ok_or_else(|| "The cue sheet has no data track.".to_owned())?;
    let resolved = path.parent().unwrap_or_else(|| Path::new(".")).join(name);
    if resolved.is_file() {
        Ok(resolved)
    } else {
        Err("The cue sheet's data track is not next to it.".into())
    }
}

fn quoted_or_token(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest).to_owned();
    }
    trimmed
        .split_whitespace()
        .next()
        .unwrap_or(trimmed)
        .to_owned()
}

fn gdi_data_file(path: &Path) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| "The GD-ROM layout could not be read.".to_owned())?;
    for line in text.lines().skip(1) {
        let Some((name, track_type)) = gdi_track(line) else {
            continue;
        };
        if track_type == "0" {
            continue;
        }
        let resolved = path.parent().unwrap_or_else(|| Path::new(".")).join(name);
        if resolved.is_file() {
            return Ok(resolved);
        }
    }
    Err("The GD-ROM layout has no data track next to it.".into())
}

/// One track line: its filename, and the track type, which is data or audio.
///
/// The columns are number, start sector, type, sector size, filename, offset.
/// A filename with a space is in quotes. Redump names every track after the
/// game, so quoted names are the usual case.
fn gdi_track(line: &str) -> Option<(String, &str)> {
    let mut rest = line;
    let mut track_type = "";
    for column in 0..4 {
        let (field, tail) = next_field(rest)?;
        if column == 2 {
            track_type = field;
        }
        rest = tail;
    }
    let name = quoted_or_token(rest);
    (!name.is_empty()).then_some((name, track_type))
}

fn next_field(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    Some(match text.find(char::is_whitespace) {
        Some(end) => (&text[..end], &text[end..]),
        None => (text, ""),
    })
}

fn m3u_first(path: &Path) -> Result<PathBuf, String> {
    let text =
        std::fs::read_to_string(path).map_err(|_| "The playlist could not be read.".to_owned())?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or_else(|| "The playlist is empty.".to_owned())?;
    let resolved = path.parent().unwrap_or_else(|| Path::new(".")).join(line);
    if resolved.is_file() {
        Ok(resolved)
    } else {
        Err("The playlist's first disc is not next to it.".into())
    }
}

/// The serial is in the first sectors. We read a hunk at a time and stop
/// once we find it, so we never decompress or hash the rest of a CHD, which
/// can be hundreds of megabytes.
fn read_chd(path: &Path) -> DiscRead {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return DiscRead::Unreadable("The disc image could not be read.".into()),
    };
    let mut file = BufReader::new(file);
    let mut image = match chd::Chd::open(&mut file, None) {
        Ok(image) => image,
        Err(_) => {
            return DiscRead::Unreadable(
                "This CHD could not be opened, so its serial was not read.".into(),
            );
        }
    };
    let hunk_count = image.header().hunk_count();
    let mut output = image.get_hunksized_buffer();
    let mut compressed = Vec::new();
    let mut collected = Vec::new();
    let limit = PREFIX_BYTES as usize;
    for index in 0..hunk_count {
        if collected.len() >= limit {
            break;
        }
        let mut hunk = match image.hunk(index) {
            Ok(hunk) => hunk,
            Err(_) => {
                return DiscRead::Unreadable("This CHD could not be read.".into());
            }
        };
        if hunk.read_hunk_in(&mut compressed, &mut output).is_err() {
            return DiscRead::Unreadable("This CHD could not be read.".into());
        }
        let remaining = limit.saturating_sub(collected.len());
        collected.extend_from_slice(&output[..remaining.min(output.len())]);
        let (system_id, keys) = keys_from_bytes(&collected, "chd");
        if !keys.is_empty() {
            return DiscRead::Found {
                keys,
                system_id,
                prefix: collected,
            };
        }
    }
    let (system_id, keys) = keys_from_bytes(&collected, "chd");
    DiscRead::Found {
        keys,
        system_id,
        prefix: collected,
    }
}

fn read_prefix(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(PREFIX_BYTES).read_to_end(&mut bytes))
        .map_err(|_| "The disc image could not be read.".to_owned())?;
    if bytes.is_empty() {
        return Err("The disc image is empty.".into());
    }
    Ok(bytes)
}

fn keys_from_bytes(bytes: &[u8], extension: &str) -> (Option<&'static str>, Vec<String>) {
    if let Some(code) = gamecube_header(bytes) {
        return (Some("gamecube"), vec![code]);
    }
    if let Some(product) = dreamcast_product(bytes) {
        return (Some("dreamcast"), vec![normalize_serial(&product)]);
    }
    let sega = sega_cd_serials(bytes);
    if !sega.is_empty() {
        return (Some("segacd"), sega);
    }
    if let Some(hit) = playstation_serial(bytes) {
        let system = if hit.number >= 200 { "ps2" } else { "ps1" };
        return (Some(system), vec![hit.key]);
    }
    if extension == "gdi" {
        return (Some("dreamcast"), Vec::new());
    }
    if let Some(serial) = pc_engine_serial(bytes) {
        return (Some("pcecd"), vec![normalize_serial(&serial)]);
    }
    (None, Vec::new())
}

struct PlaystationHit {
    key: String,
    number: u32,
}

fn playstation_serial(bytes: &[u8]) -> Option<PlaystationHit> {
    if bytes.len() < 11 {
        return None;
    }
    for start in 0..=bytes.len() - 11 {
        if !bytes[start..start + 4]
            .iter()
            .all(|byte| byte.is_ascii_uppercase())
        {
            continue;
        }
        if bytes[start + 4] != b'_' && bytes[start + 4] != b'-' {
            continue;
        }
        if !bytes[start + 5..start + 8]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        if bytes[start + 8] != b'.' && bytes[start + 8] != b'-' {
            continue;
        }
        if !bytes[start + 9..start + 11]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let number = std::str::from_utf8(&bytes[start + 5..start + 8])
            .ok()?
            .parse()
            .ok()?;
        return Some(PlaystationHit {
            key: normalize_serial(&String::from_utf8_lossy(&bytes[start..start + 11])),
            number,
        });
    }
    None
}

fn gamecube_header(bytes: &[u8]) -> Option<String> {
    if bytes.get(0x1C..0x20) != Some(&[0xC2, 0x33, 0x9F, 0x3D]) {
        return None;
    }
    let code = bytes.get(..4)?;
    if !code.iter().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    Some(String::from_utf8_lossy(code).to_ascii_uppercase())
}

fn gamecube_code_in_serial(serial: &str) -> Option<String> {
    let upper = serial.to_ascii_uppercase();
    let rest = upper.split("DOL-").nth(1)?;
    let code: String = rest.chars().take(4).collect();
    if code.len() == 4
        && code
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        Some(code)
    } else {
        None
    }
}

fn dreamcast_product(bytes: &[u8]) -> Option<String> {
    let start = bytes
        .windows(15)
        .position(|window| window == b"SEGA SEGAKATANA")?;
    let product = bytes.get(start + 0x40..start + 0x4A)?;
    let text = String::from_utf8_lossy(product).trim().to_owned();
    if text
        .chars()
        .any(|character| character.is_ascii_alphanumeric())
    {
        Some(text)
    } else {
        None
    }
}

fn sega_cd_serials(bytes: &[u8]) -> Vec<String> {
    let text = ascii_text(bytes);
    let Some(start) = text.find("SEGADISC") else {
        return Vec::new();
    };
    let end = (start + 0x220).min(text.len());
    let window = &text[start..end];
    let mut keys = Vec::new();
    for token in
        window.split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
    {
        if token.is_empty() {
            continue;
        }
        let upper = token.to_ascii_uppercase();
        let recognised = upper.starts_with("T-")
            || upper.starts_with("G-")
            || upper.starts_with("MK-")
            || (upper.len() >= 4
                && upper.chars().next().unwrap().is_ascii_digit()
                && upper.contains('-'));
        if recognised {
            push_key(&mut keys, &normalize_serial(&upper));
        }
    }
    keys
}

fn pc_engine_serial(bytes: &[u8]) -> Option<String> {
    let text = ascii_text(bytes);
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let mut length = 0;
        while index + length < chars.len() && chars[index + length].is_ascii_alphanumeric() {
            length += 1;
        }
        if length >= 6 {
            let token: String = chars[index..index + length].iter().collect();
            let upper = token.to_ascii_uppercase();
            if upper.contains("CD") && upper.chars().any(|character| character.is_ascii_digit()) {
                return Some(upper);
            }
        }
        index += length.max(1);
    }
    None
}

fn ascii_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' || *byte == b'-' {
                *byte as char
            } else {
                '\n'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playstation_serial_matches_the_catalogue_form() {
        let mut image = vec![0; 64];
        image[16..27].copy_from_slice(b"SLUS_012.34");
        let (system, keys) = keys_from_bytes(&image, "iso");
        assert_eq!(system, Some("ps1"));
        assert_eq!(keys, vec!["SLUS01234".to_owned()]);
        assert!(keys_for_catalog_serial("ps1", "SLUS-01234").contains(&"SLUS01234".to_owned()));
    }

    #[test]
    fn a_playstation_2_serial_is_not_called_a_playstation_1_game() {
        let mut image = vec![0; 32];
        image[..11].copy_from_slice(b"SLUS_200.01");
        let (system, keys) = keys_from_bytes(&image, "iso");
        assert_eq!(system, Some("ps2"));
        assert_eq!(keys, vec!["SLUS20001".to_owned()]);
    }

    #[test]
    fn a_gamecube_header_uses_the_four_character_code() {
        let mut image = vec![0; 32];
        image[..4].copy_from_slice(b"GW7P");
        image[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
        let (system, keys) = keys_from_bytes(&image, "iso");
        assert_eq!(system, Some("gamecube"));
        assert_eq!(keys, vec!["GW7P".to_owned()]);
        assert!(keys_for_catalog_serial("gamecube", "DL-DOL-GW7P-EUR").contains(&"GW7P".into()));
    }

    #[test]
    fn a_dreamcast_product_number_drops_the_catalogue_region_suffix() {
        let mut image = vec![0; 0x50];
        image[..15].copy_from_slice(b"SEGA SEGAKATANA");
        image[0x40..0x48].copy_from_slice(b"T-9708N ");
        let (system, keys) = keys_from_bytes(&image, "gdi");
        assert_eq!(system, Some("dreamcast"));
        assert_eq!(keys, vec!["T9708N".to_owned()]);
        let catalog = keys_for_catalog_serial("dreamcast", "T-9708N-50");
        assert!(catalog.contains(&"T9708N".to_owned()));
    }

    #[test]
    fn a_sega_cd_header_keeps_the_product_number() {
        let mut image = b"SEGADISCSYSTEM  ".to_vec();
        image.extend(std::iter::repeat(b' ').take(32));
        image.extend(b"T-93175");
        let (system, keys) = keys_from_bytes(&image, "iso");
        assert_eq!(system, Some("segacd"));
        assert_eq!(keys, vec!["T93175".to_owned()]);
        assert!(keys_for_catalog_serial("segacd", "T-93175").contains(&"T93175".into()));
    }

    /// Redump names every GD-ROM track after the game, so the names contain
    /// spaces and are in quotes in the layout. The quotes are not part of the
    /// filename.
    #[test]
    fn a_gd_rom_layout_points_at_a_quoted_data_track() {
        let root = std::env::temp_dir().join(format!("rominabox-gdi-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut image = vec![0; 0x50];
        image[..15].copy_from_slice(b"SEGA SEGAKATANA");
        image[0x40..0x48].copy_from_slice(b"T-9708N ");
        std::fs::write(root.join("Space Game (Europe) (Track 1).bin"), &image).unwrap();
        std::fs::write(root.join("Space Game (Europe) (Track 2).bin"), b"audio").unwrap();
        let layout = root.join("Space Game (Europe).gdi");
        std::fs::write(
            &layout,
            "2\n\
             1 0 4 2352 \"Space Game (Europe) (Track 1).bin\" 0\n\
             2 5424 0 2352 \"Space Game (Europe) (Track 2).bin\" 0\n",
        )
        .unwrap();

        match read_disc(&layout, "gdi") {
            DiscRead::Found {
                keys, system_id, ..
            } => {
                assert_eq!(system_id, Some("dreamcast"));
                assert_eq!(keys, vec!["T9708N".to_owned()]);
            }
            other => panic!("expected the disc's serial, got {other:?}"),
        }
    }

    #[test]
    fn a_cue_sheet_points_at_its_data_track() {
        let root = std::env::temp_dir().join(format!("rominabox-cue-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bin = root.join("track.bin");
        let mut image = vec![0; 64];
        image[..11].copy_from_slice(b"SLUS_012.34");
        std::fs::write(&bin, &image).unwrap();
        let cue = root.join("game.cue");
        std::fs::write(
            &cue,
            "FILE \"track.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
        )
        .unwrap();
        match read_disc(&cue, "cue") {
            DiscRead::Found {
                keys, system_id, ..
            } => {
                assert_eq!(system_id, Some("ps1"));
                assert_eq!(keys, vec!["SLUS01234".to_owned()]);
            }
            other => panic!("expected a serial, got {other:?}"),
        }
    }
}
