use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crc32fast::Hasher;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use sha1::{Digest, Sha1};

use crate::systems::{self, System};

const HEADER_BYTES: u64 = 512;
const CATALOG_LIMIT: u64 = 32 * 1024 * 1024;
const ARTWORK_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub title: String,
    pub system: String,
    pub filename: String,
    pub size: u64,
    pub source: MetadataSource,
    pub matched: bool,
    pub catalog_name: Option<String>,
    pub description: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataSource {
    Catalog,
    Header,
    Filename,
}

#[derive(Debug)]
pub struct InspectionError(String);

impl InspectionError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for InspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for InspectionError {}

impl From<io::Error> for InspectionError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

pub fn inspect_game(rom: &Path, cache: &Path, online: bool) -> Result<Inspection, InspectionError> {
    inspect_game_with_system(rom, cache, online, None)
}

/// Inspect one game file. `system_override` is the console for container
/// extensions that several consoles use, such as CUE, CHD and ISO.
pub fn inspect_game_with_system(
    rom: &Path,
    cache: &Path,
    online: bool,
    system_override: Option<&str>,
) -> Result<Inspection, InspectionError> {
    let metadata = rom
        .metadata()
        .map_err(|_| InspectionError::new("Choose an existing game file."))?;
    if !metadata.is_file() {
        return Err(InspectionError::new("Choose an existing game file."));
    }
    if metadata.len() == 0 {
        return Err(InspectionError::new("Choose a non-empty game file."));
    }

    let filename = rom
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| InspectionError::new("Choose a game file with a valid filename."))?
        .to_owned();
    let extension = rom
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let header = read_header(rom)?;
    let (system, mut warnings) = if let Some(requested) = system_override {
        let selected = systems::find(requested)
            .ok_or_else(|| InspectionError::new(format!("Unknown console: {requested}")))?;
        if !selected
            .extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
        {
            return Err(InspectionError::new(format!(
                "{} does not support .{extension} files.",
                selected.name
            )));
        }
        (Some(selected), Vec::new())
    } else {
        identify_system(&extension, &header)
    };
    let mut title = filename_title(&filename);
    let mut source = MetadataSource::Filename;

    if system.is_some_and(|value| value.id == "megadrive") {
        if let Some(header_title) = ascii_title(&header, 0x150, 0x180) {
            title = header_title;
            source = MetadataSource::Header;
        }
    } else if system.is_some_and(|value| matches!(value.id.as_str(), "gb" | "gbc")) {
        if let Some(header_title) = ascii_title(&header, 0x134, 0x143) {
            title = header_title;
            source = MetadataSource::Header;
        }
    } else if system.is_some_and(|value| value.id == "gba") {
        if let Some(header_title) = ascii_title(&header, 0xA0, 0xAC) {
            title = header_title;
            source = MetadataSource::Header;
        }
    } else if system.is_some_and(|value| value.id == "n64") {
        if let Some(header_title) = ascii_title(&header, 0x20, 0x34) {
            title = header_title;
            source = MetadataSource::Header;
        }
    } else if system.is_some_and(|value| value.id == "atari7800") {
        if let Some(header_title) = ascii_title(&header, 17, 49) {
            title = header_title;
            source = MetadataSource::Header;
        }
    }

    let Some(mut system) = system else {
        return Ok(Inspection {
            title,
            system: String::new(),
            filename,
            size: metadata.len(),
            source,
            matched: false,
            catalog_name: None,
            description: None,
            icon_path: None,
            warnings,
        });
    };

    let mut catalog_name = None;
    let mut description = None;
    let mut icon_path = None;
    if system.catalog.is_some() {
        let catalog_systems = if matches!(system.id.as_str(), "gb" | "gbc") {
            if extension == "gb" {
                vec![systems::find("gb").unwrap(), systems::find("gbc").unwrap()]
            } else {
                vec![systems::find("gbc").unwrap(), systems::find("gb").unwrap()]
            }
        } else {
            vec![system]
        };
        let mut fingerprints = None;
        let mut catalogs_checked = Vec::new();
        for candidate in catalog_systems {
            let catalog = candidate.catalog.as_deref().expect("catalog candidate");
            match catalog_path(cache, catalog, online) {
                Ok(Some(path)) => {
                    // Compute every required fingerprint in one streaming pass,
                    // only once a catalog is available. The fingerprints include
                    // the headerless iNES form that checksum catalogs use.
                    if fingerprints.is_none() {
                        fingerprints =
                            Some(stream_fingerprints(rom, header.starts_with(b"NES\x1a"))?);
                    }
                    match match_catalog(fingerprints.as_deref().unwrap(), &path) {
                        Ok(Some(entry)) => {
                            system = candidate;
                            title = display_title(&entry.name);
                            source = MetadataSource::Catalog;
                            description = entry.description.filter(|value| value != &entry.name);
                            catalog_name = Some(entry.name.clone());
                            if online {
                                match cache_boxart(cache, catalog, &entry.name) {
                                    Ok(Some(path)) => icon_path = Some(path),
                                    Ok(None) => warnings
                                        .push("No matching Libretro box art was available.".into()),
                                    Err(error) => {
                                        warnings.push(format!("Box art was not available: {error}"))
                                    }
                                }
                            } else {
                                icon_path = cached_boxart(cache, catalog, &entry.name);
                            }
                            break;
                        }
                        Ok(None) => catalogs_checked.push(catalog),
                        Err(error) => warnings.push(format!("Could not read {catalog}: {error}")),
                    }
                }
                Ok(None) => warnings.push(format!("The {catalog} checksum catalog is not cached.")),
                Err(error) => warnings.push(format!("Could not fetch {catalog}: {error}")),
            }
        }
        if catalog_name.is_none() && !catalogs_checked.is_empty() {
            warnings.push(format!(
                "No exact checksum match was found in {}; using embedded or filename metadata.",
                catalogs_checked.join(" or ")
            ));
        }
    } else {
        warnings.push(
            "This container needs system-specific identification and was not checksum-scanned."
                .into(),
        );
    }

    Ok(Inspection {
        title,
        system: system.id.clone(),
        filename,
        size: metadata.len(),
        source,
        matched: catalog_name.is_some(),
        catalog_name,
        description,
        icon_path,
        warnings,
    })
}

struct CatalogEntry {
    name: String,
    description: Option<String>,
}

fn match_catalog(
    fingerprints: &[Fingerprint],
    catalog_path: &Path,
) -> Result<Option<CatalogEntry>, InspectionError> {
    let dat = datary::read_file(catalog_path)
        .map_err(|error| InspectionError::new(format!("invalid DAT catalog: {error}")))?;
    let expected: Result<Vec<_>, InspectionError> = fingerprints
        .iter()
        .map(|fingerprint| {
            let crc: datary::Crc32 =
                format!("{:08X}", fingerprint.crc32)
                    .parse()
                    .map_err(|error| {
                        InspectionError::new(format!("invalid computed checksum: {error}"))
                    })?;
            let sha1: datary::Sha1 = fingerprint.sha1.parse().map_err(|error| {
                InspectionError::new(format!("invalid computed SHA-1: {error}"))
            })?;
            Ok((fingerprint.size, crc, sha1))
        })
        .collect();
    let expected = expected?;
    for game in dat.games {
        if game.roms.iter().any(|rom| {
            expected.iter().any(|(size, crc, sha1)| {
                rom.size == *size
                    && rom.crc.as_ref() == Some(crc)
                    && rom.sha1.as_ref().is_none_or(|actual| actual == sha1)
            })
        }) {
            return Ok(Some(CatalogEntry {
                name: game.name,
                description: (!game.description.is_empty()).then_some(game.description),
            }));
        }
    }
    Ok(None)
}

struct Fingerprint {
    size: u64,
    crc32: u32,
    sha1: String,
}

fn stream_fingerprints(
    path: &Path,
    normalize_ines: bool,
) -> Result<Vec<Fingerprint>, InspectionError> {
    let mut file = File::open(path)?;
    let mut crc32 = Hasher::new();
    let mut sha1 = Sha1::new();
    let mut normalized_crc32 = Hasher::new();
    let mut normalized_sha1 = Sha1::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        crc32.update(&buffer[..count]);
        sha1.update(&buffer[..count]);
        if normalize_ines {
            let skipped = 16_u64.saturating_sub(size).min(count as u64) as usize;
            normalized_crc32.update(&buffer[skipped..count]);
            normalized_sha1.update(&buffer[skipped..count]);
        }
        size += count as u64;
    }
    let sha1 = sha1.finalize();
    let mut fingerprints = vec![Fingerprint {
        size,
        crc32: crc32.finalize(),
        sha1: sha1.iter().map(|byte| format!("{byte:02x}")).collect(),
    }];
    if normalize_ines && size >= 16 {
        let normalized_sha1 = normalized_sha1.finalize();
        fingerprints.push(Fingerprint {
            size: size - 16,
            crc32: normalized_crc32.finalize(),
            sha1: normalized_sha1
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        });
    }
    Ok(fingerprints)
}

fn catalog_path(
    cache: &Path,
    catalog: &str,
    online: bool,
) -> Result<Option<PathBuf>, InspectionError> {
    let path = cache.join("catalogs").join(format!("{catalog}.dat"));
    if path.is_file() {
        return Ok(Some(path));
    }
    if !online {
        return Ok(None);
    }
    let encoded = utf8_percent_encode(catalog, NON_ALPHANUMERIC);
    let url = format!(
        "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro/{encoded}.dat"
    );
    let bytes = download(&url, CATALOG_LIMIT)?;
    datary::from_bytes(&bytes).map_err(|error| {
        InspectionError::new(format!("downloaded DAT catalog was invalid: {error}"))
    })?;
    write_cached(&path, &bytes)?;
    Ok(Some(path))
}

fn cache_boxart(
    cache: &Path,
    catalog: &str,
    name: &str,
) -> Result<Option<PathBuf>, InspectionError> {
    if let Some(path) = cached_boxart(cache, catalog, name) {
        return Ok(Some(path));
    }
    let filename = thumbnail_name(name);
    let repository = catalog.replace(' ', "_");
    let encoded_catalog = utf8_percent_encode(&repository, NON_ALPHANUMERIC);
    let encoded_filename = utf8_percent_encode(&filename, NON_ALPHANUMERIC);
    let url = format!(
        "https://raw.githubusercontent.com/libretro-thumbnails/{encoded_catalog}/master/Named_Boxarts/{encoded_filename}.png"
    );
    let bytes = match download(&url, ARTWORK_LIMIT) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(InspectionError::new("downloaded artwork was not a PNG"));
    }
    let path = artwork_path(cache, catalog, name);
    write_cached(&path, &bytes)?;
    Ok(Some(path))
}

fn cached_boxart(cache: &Path, catalog: &str, name: &str) -> Option<PathBuf> {
    let path = artwork_path(cache, catalog, name);
    path.is_file().then_some(path)
}

fn artwork_path(cache: &Path, catalog: &str, name: &str) -> PathBuf {
    cache
        .join("artwork")
        .join(catalog)
        .join("Named_Boxarts")
        .join(format!("{}.png", thumbnail_name(name)))
}

fn thumbnail_name(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            '&' | '*' | '/' | ':' | '`' | '<' | '>' | '?' | '\\' | '|' | '"' => '_',
            other => other,
        })
        .collect()
}

fn download(url: &str, limit: u64) -> Result<Vec<u8>, InspectionError> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(5))
        .call()
        .map_err(|error| InspectionError::new(error.to_string()))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(InspectionError::new("download exceeded the size limit"));
    }
    Ok(bytes)
}

fn write_cached(path: &Path, bytes: &[u8]) -> Result<(), InspectionError> {
    let parent = path
        .parent()
        .ok_or_else(|| InspectionError::new("cache path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn read_header(path: &Path) -> Result<Vec<u8>, InspectionError> {
    let mut header = Vec::with_capacity(HEADER_BYTES as usize);
    File::open(path)?
        .take(HEADER_BYTES)
        .read_to_end(&mut header)?;
    Ok(header)
}

fn identify_system(extension: &str, header: &[u8]) -> (Option<&'static System>, Vec<String>) {
    if header.get(0x100..0x104) == Some(b"SEGA") {
        return (systems::find("megadrive"), Vec::new());
    }
    if header.starts_with(b"NES\x1a") {
        return (systems::find("nes"), Vec::new());
    }
    if matches!(
        header.get(..4),
        Some([0x80, 0x37, 0x12, 0x40])
            | Some([0x37, 0x80, 0x40, 0x12])
            | Some([0x40, 0x12, 0x37, 0x80])
    ) {
        return (systems::find("n64"), Vec::new());
    }
    if header.starts_with(b"LYNX") {
        return (systems::find("lynx"), Vec::new());
    }
    if header.get(1..10) == Some(b"ATARI7800") {
        return (systems::find("atari7800"), Vec::new());
    }
    if header.get(4..8) == Some(&[0x24, 0xff, 0xae, 0x51]) && header.get(0xB2) == Some(&0x96) {
        return (systems::find("gba"), Vec::new());
    }
    if has_valid_game_boy_header(header) {
        let id = if matches!(header.get(0x143), Some(0x80) | Some(0xC0)) {
            "gbc"
        } else {
            "gb"
        };
        return (systems::find(id), Vec::new());
    }
    if matches!(extension, "gb" | "gbc") {
        return (
            None,
            vec!["The file extension suggests Game Boy, but its cartridge header signature or checksum is invalid. Choose the console explicitly only if the file is known to be valid.".into()],
        );
    }
    let candidates = systems::candidates_for_extension(extension);
    if candidates.len() == 1 {
        (candidates.first().copied(), Vec::new())
    } else if candidates.is_empty() {
        (
            None,
            vec!["This file type is not supported for automatic identification.".into()],
        )
    } else {
        (
            None,
            vec!["This container is used by multiple systems; choose the console before metadata lookup.".into()],
        )
    }
}

fn has_valid_game_boy_header(header: &[u8]) -> bool {
    const NINTENDO_LOGO: &[u8] = &[
        0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00,
        0x0D, 0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD,
        0xD9, 0x99, 0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB,
        0xB9, 0x33, 0x3E,
    ];
    if header.get(0x104..0x134) != Some(NINTENDO_LOGO) {
        return false;
    }
    let Some(stored_checksum) = header.get(0x14D) else {
        return false;
    };
    let Some(checksummed) = header.get(0x134..=0x14C) else {
        return false;
    };
    let computed = checksummed.iter().fold(0_u8, |checksum, byte| {
        checksum.wrapping_sub(*byte).wrapping_sub(1)
    });
    computed == *stored_checksum
}

fn ascii_title(header: &[u8], start: usize, end: usize) -> Option<String> {
    let bytes = header.get(start..header.len().min(end))?;
    let title: String = bytes
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                ' '
            }
        })
        .collect();
    let clean = clean_title(&title);
    (!clean.is_empty()).then_some(clean)
}

fn filename_title(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let title = clean_title(stem);
    if title.is_empty() {
        "Untitled game".into()
    } else {
        title
    }
}

fn display_title(catalog_name: &str) -> String {
    let mut groups = Vec::new();
    let mut base_end = catalog_name.len();
    while catalog_name[..base_end].ends_with(')') {
        let Some(open) = catalog_name[..base_end].rfind(" (") else {
            break;
        };
        groups.push(&catalog_name[open + 2..base_end - 1]);
        base_end = open;
    }
    groups.reverse();
    let mut title = catalog_name[..base_end].to_owned();
    for group in groups {
        if !is_catalog_tag(group) {
            title.push_str(" (");
            title.push_str(group);
            title.push(')');
        }
    }
    clean_title(&title)
}

fn is_catalog_tag(group: &str) -> bool {
    const REGIONS: &[&str] = &[
        "Australia",
        "Brazil",
        "Canada",
        "China",
        "Europe",
        "France",
        "Germany",
        "Italy",
        "Japan",
        "Korea",
        "Netherlands",
        "Russia",
        "Spain",
        "Sweden",
        "Taiwan",
        "USA",
        "World",
    ];
    let parts: Vec<_> = group.split(',').map(str::trim).collect();
    if !parts.is_empty() && parts.iter().all(|part| REGIONS.contains(part)) {
        return true;
    }
    let lower = group.to_ascii_lowercase();
    lower.starts_with("rev ")
        || lower.starts_with("revision ")
        || lower.starts_with("beta")
        || lower.starts_with("proto")
        || lower.starts_with("demo")
        || lower.ends_with(" enhanced")
        || matches!(
            lower.as_str(),
            "gb compatible" | "virtual console" | "aftermarket" | "unl"
        )
}

fn clean_title(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([' ', '.'])
        .chars()
        .take(100)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn fixture_directory(label: &str) -> PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rominabox-metadata-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cue_requires_an_explicit_system_and_accepts_a_valid_override() {
        let root = fixture_directory("cue");
        let rom = root.join("game.cue");
        fs::write(&rom, b"FILE \"track.bin\" BINARY\n").unwrap();

        let unresolved = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(unresolved.system, "");
        assert!(unresolved.warnings[0].contains("multiple systems"));

        let selected =
            inspect_game_with_system(&rom, &root.join("cache"), false, Some("Sega CD")).unwrap();
        assert_eq!(selected.system, "segacd");
    }

    #[test]
    fn public_inspection_matches_headerless_nes_catalog_data() {
        let root = fixture_directory("ines");
        let rom = root.join("fixture.nes");
        let mut bytes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        bytes.extend([1, 2, 3, 4]);
        fs::write(&rom, bytes).unwrap();

        let cache = root.join("cache");
        let catalog = cache
            .join("catalogs")
            .join("Nintendo - Nintendo Entertainment System.dat");
        fs::create_dir_all(catalog.parent().unwrap()).unwrap();
        fs::write(
            catalog,
            r#"clrmamepro (
  name "fixture"
  description "fixture"
)
game (
  name "Tiny Adventure (USA)"
  description "A generated test fixture"
  rom ( name "fixture.nes" size 4 crc B63CFBCD sha1 12dada1fff4d4787ade3333147202c3b443e376f )
)
"#,
        )
        .unwrap();

        let inspection = inspect_game(&rom, &cache, false).unwrap();
        assert_eq!(inspection.system, "nes");
        assert!(inspection.matched);
        assert_eq!(inspection.title, "Tiny Adventure");
        assert_eq!(
            inspection.catalog_name.as_deref(),
            Some("Tiny Adventure (USA)")
        );
    }

    #[test]
    fn known_headers_can_identify_generic_binary_files() {
        let root = fixture_directory("headers");
        let rom = root.join("upload.bin");
        let mut bytes = vec![0; 512];
        bytes[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        bytes[0x20..0x2B].copy_from_slice(b"MARIO TEST ");
        fs::write(&rom, bytes).unwrap();

        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "n64");
        assert_eq!(inspection.title, "MARIO TEST");

        let fake_game_boy = root.join("not-a-rom.gb");
        fs::write(&fake_game_boy, vec![0; 512]).unwrap();
        let inspection = inspect_game(&fake_game_boy, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "");
        assert!(inspection.warnings[0].contains("checksum is invalid"));
    }
}
