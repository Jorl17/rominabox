use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;

use crate::{artwork, discs, dumps};

use crate::systems::{self, System};

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

    let prepared = dumps::prepare(rom).map_err(InspectionError::new)?;
    let filename = prepared.filename.clone();
    let extension = prepared.extension.clone();
    let header = dumps::identification_header(&prepared.path)?;
    let (system, warnings) = if let Some(requested) = system_override {
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
    let (system, mut warnings) = resolve_disc(system, warnings, &prepared.path, &extension);
    let mut title = filename_title(&filename);
    let mut source = MetadataSource::Filename;

    // The position of the ASCII title in the header is declared in the console
    // package, so a new console requires a package field and no new branch here.
    if let Some(window) = system.and_then(|value| value.header_title) {
        if let Some(header_title) = ascii_title(
            &header,
            window.offset as usize,
            (window.offset + window.length) as usize,
        ) {
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
    let disc = system.category == "disc";
    if system.catalog.is_some() {
        if disc {
            apply_disc_catalog(
                &mut system,
                &prepared.path,
                &extension,
                cache,
                online,
                &mut title,
                &mut source,
                &mut description,
                &mut catalog_name,
                &mut icon_path,
                &mut warnings,
            )?;
        } else {
            apply_cartridge_catalog(
                &mut system,
                &prepared.path,
                &extension,
                &header,
                cache,
                online,
                &mut title,
                &mut source,
                &mut description,
                &mut catalog_name,
                &mut icon_path,
                &mut warnings,
            )?;
        }
    } else if disc {
        warnings
            .push("This disc image was not matched. Its serial was not in the catalogue.".into());
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

fn apply_cartridge_catalog(
    system: &mut &'static systems::System,
    rom: &Path,
    extension: &str,
    header: &[u8],
    cache: &Path,
    online: bool,
    title: &mut String,
    source: &mut MetadataSource,
    description: &mut Option<String>,
    catalog_name: &mut Option<String>,
    icon_path: &mut Option<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), InspectionError> {
    let catalog_systems = if matches!(system.id.as_str(), "gb" | "gbc") {
        if extension == "gb" {
            vec![systems::find("gb").unwrap(), systems::find("gbc").unwrap()]
        } else {
            vec![systems::find("gbc").unwrap(), systems::find("gb").unwrap()]
        }
    } else {
        vec![*system]
    };
    let mut fingerprints = None;
    let mut catalogs_checked = Vec::new();
    for candidate in catalog_systems {
        let catalog = candidate.catalog.as_deref().expect("catalog candidate");
        match catalog_path(cache, catalog, online, false) {
            Ok(Some(path)) => {
                if fingerprints.is_none() {
                    fingerprints = Some(dumps::fingerprints(
                        rom,
                        extension,
                        header.starts_with(b"NES\x1a"),
                    )?);
                }
                match match_catalog(fingerprints.as_deref().unwrap(), &path) {
                    Ok(Some(entry)) => {
                        remember_match(
                            system,
                            candidate,
                            &entry,
                            catalog,
                            cache,
                            online,
                            title,
                            source,
                            description,
                            catalog_name,
                            icon_path,
                            warnings,
                        )?;
                        return Ok(());
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
    Ok(())
}

fn apply_disc_catalog(
    system: &mut &'static systems::System,
    rom: &Path,
    extension: &str,
    cache: &Path,
    online: bool,
    title: &mut String,
    source: &mut MetadataSource,
    description: &mut Option<String>,
    catalog_name: &mut Option<String>,
    icon_path: &mut Option<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), InspectionError> {
    let keys = match discs::read_disc(rom, extension) {
        discs::DiscRead::Compressed => {
            warnings.push(
                "This disc image is compressed, so its serial cannot be read. Use a cue, iso or gdi image."
                    .into(),
            );
            return Ok(());
        }
        discs::DiscRead::Unreadable(message) => {
            warnings.push(message);
            return Ok(());
        }
        discs::DiscRead::Found { keys, .. } if keys.is_empty() => {
            warnings.push("No serial was found in this disc image. Using the filename.".into());
            return Ok(());
        }
        discs::DiscRead::Found { keys, .. } => keys,
    };
    let catalog = system.catalog.as_deref().expect("disc catalogue");
    let path = match catalog_path(cache, catalog, online, true) {
        Ok(Some(path)) => path,
        Ok(None) => {
            warnings.push(format!("The {catalog} serial catalogue is not cached."));
            return Ok(());
        }
        Err(error) => {
            warnings.push(format!("Could not fetch {catalog}: {error}"));
            return Ok(());
        }
    };
    let candidate = *system;
    match match_serial(&keys, &candidate.id, &path) {
        Ok(Some(entry)) => remember_match(
            system,
            candidate,
            &entry,
            catalog,
            cache,
            online,
            title,
            source,
            description,
            catalog_name,
            icon_path,
            warnings,
        ),
        Ok(None) => {
            warnings.push("No catalogue entry has this disc's serial. Using the filename.".into());
            Ok(())
        }
        Err(error) => {
            warnings.push(error.to_string());
            Ok(())
        }
    }
}

fn remember_match(
    system: &mut &'static systems::System,
    candidate: &'static systems::System,
    entry: &CatalogEntry,
    catalog: &str,
    cache: &Path,
    online: bool,
    title: &mut String,
    source: &mut MetadataSource,
    description: &mut Option<String>,
    catalog_name: &mut Option<String>,
    icon_path: &mut Option<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), InspectionError> {
    *system = candidate;
    *title = display_title(&entry.name);
    *source = MetadataSource::Catalog;
    *description = entry
        .description
        .clone()
        .filter(|value| value != &entry.name);
    *catalog_name = Some(entry.name.clone());
    match lookup_boxart(cache, catalog, &entry.name, online) {
        Ok(Some(path)) => *icon_path = Some(path),
        Ok(None) => warnings.push("No cover is published for this game.".into()),
        Err(error) => warnings.push(format!("Box art was not available: {error}")),
    }
    Ok(())
}

fn resolve_disc(
    system: Option<&'static systems::System>,
    warnings: Vec<String>,
    path: &Path,
    extension: &str,
) -> (Option<&'static systems::System>, Vec<String>) {
    if system.is_some() {
        return (system, warnings);
    }
    let discs::DiscRead::Found {
        system_id: Some(id),
        keys,
    } = discs::read_disc(path, extension)
    else {
        return (system, warnings);
    };
    if keys.is_empty() {
        return (system, warnings);
    }
    let Some(detected) = systems::find(id) else {
        return (system, warnings);
    };
    let extension_matches = detected
        .extensions
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension));
    if !extension_matches {
        return (system, warnings);
    }
    (Some(detected), Vec::new())
}

fn match_serial(
    keys: &[String],
    system_id: &str,
    catalog_path: &Path,
) -> Result<Option<CatalogEntry>, InspectionError> {
    let dat = datary::read_file(catalog_path)
        .map_err(|error| InspectionError::new(format!("invalid DAT catalog: {error}")))?;
    let mut names_for_key: HashMap<String, HashSet<String>> = HashMap::new();
    let mut entry_for_name: HashMap<String, CatalogEntry> = HashMap::new();
    for game in dat.games {
        entry_for_name
            .entry(game.name.clone())
            .or_insert_with(|| CatalogEntry {
                name: game.name.clone(),
                description: (!game.description.is_empty()).then_some(game.description.clone()),
            });
        for rom in &game.roms {
            let Some(serial) = rom.serial.as_deref() else {
                continue;
            };
            for key in discs::keys_for_catalog_serial(system_id, serial) {
                names_for_key
                    .entry(key)
                    .or_default()
                    .insert(game.name.clone());
            }
        }
    }
    let mut matched = HashSet::new();
    for key in keys {
        let Some(names) = names_for_key.get(key) else {
            continue;
        };
        if names.len() == 1 {
            matched.extend(names.iter().cloned());
        }
    }
    if matched.len() > 1 {
        return Err(InspectionError::new(
            "This disc serial matches more than one game, so no cover was chosen.",
        ));
    }
    if matched.is_empty() {
        return Ok(None);
    }
    let name = matched.into_iter().next().expect("one name");
    Ok(entry_for_name.remove(&name))
}

fn match_catalog(
    fingerprints: &[dumps::Fingerprint],
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

fn catalog_path(
    cache: &Path,
    catalog: &str,
    online: bool,
    disc: bool,
) -> Result<Option<PathBuf>, InspectionError> {
    let path = cache.join("catalogs").join(format!("{catalog}.dat"));
    if path.is_file() {
        return Ok(Some(path));
    }
    if !online {
        return Ok(None);
    }
    let bytes = download(&checksum_catalog_url(catalog, disc), CATALOG_LIMIT)?;
    datary::from_bytes(&bytes).map_err(|error| {
        InspectionError::new(format!("downloaded DAT catalog was invalid: {error}"))
    })?;
    write_cached(&path, &bytes)?;
    Ok(Some(path))
}

pub fn checksum_catalog_url(catalog: &str, disc: bool) -> String {
    let folder = if disc { "redump" } else { "no-intro" };
    let encoded = utf8_percent_encode(catalog, NON_ALPHANUMERIC);
    format!(
        "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/{folder}/{encoded}.dat"
    )
}

fn lookup_boxart(
    cache: &Path,
    catalog: &str,
    name: &str,
    online: bool,
) -> Result<Option<PathBuf>, InspectionError> {
    let index = match artwork_index(cache, catalog, online)? {
        Some(index) => index,
        None => {
            return Ok(cached_boxart(
                cache,
                catalog,
                &artwork::scrub_filename(name),
            ))
        }
    };
    let Some(matched) = artwork::match_cover(&index, name) else {
        return Ok(None);
    };
    if let Some(path) = cached_boxart(cache, catalog, &matched.filename) {
        return Ok(Some(path));
    }
    if !online {
        return Ok(None);
    }
    let bytes = match download(
        &artwork::artwork_download_url(catalog, &matched.filename),
        ARTWORK_LIMIT,
    ) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(InspectionError::new("downloaded artwork was not a PNG"));
    }
    let path = artwork_path(cache, catalog, &matched.filename);
    write_cached(&path, &bytes)?;
    Ok(Some(path))
}

fn artwork_index(
    cache: &Path,
    catalog: &str,
    online: bool,
) -> Result<Option<artwork::ArtworkIndex>, InspectionError> {
    let path = cache.join("artwork-index").join(format!("{catalog}.txt"));
    if let Ok(text) = fs::read_to_string(&path) {
        return Ok(Some(artwork::ArtworkIndex::from_filenames(
            text.lines().filter(|line| !line.is_empty()),
        )));
    }
    if !online {
        return Ok(None);
    }
    let bytes = download(&artwork::artwork_index_url(catalog), CATALOG_LIMIT)?;
    let names = artwork::filenames_from_git_tree(&bytes).map_err(InspectionError::new)?;
    write_cached(&path, names.join("\n").as_bytes())?;
    Ok(Some(artwork::ArtworkIndex::from_filenames(names)))
}

fn cached_boxart(cache: &Path, catalog: &str, filename: &str) -> Option<PathBuf> {
    let path = artwork_path(cache, catalog, filename);
    path.is_file().then_some(path)
}

fn artwork_path(cache: &Path, catalog: &str, filename: &str) -> PathBuf {
    cache
        .join("artwork")
        .join(catalog)
        .join("Named_Boxarts")
        .join(format!("{filename}.png"))
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

/// The boot logo at 0x104 in every genuine Game Boy cartridge. A Game Boy
/// does not start without it, so we use it to tell a genuine .gb image from
/// a file that only has the extension.
const NINTENDO_LOGO: &[u8] = &[
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

fn has_valid_game_boy_header(header: &[u8]) -> bool {
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
    use std::io::Write;
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

    /// We declare in package data, for each console, where the title is in a
    /// cartridge header (the title window), and do not branch here. These
    /// fixtures fix the title we read from that window in a dropped ROM.
    mod declared_header_titles {
        use super::*;

        /// A ROM whose header contains `title` at `offset`, padded to `size`.
        fn cartridge(root: &Path, name: &str, offset: usize, title: &str, size: usize) -> PathBuf {
            let mut bytes = vec![0u8; size];
            bytes[offset..offset + title.len()].copy_from_slice(title.as_bytes());
            let rom = root.join(name);
            fs::write(&rom, bytes).unwrap();
            rom
        }

        #[test]
        fn a_mega_drive_title_is_read_from_its_declared_window() {
            let root = fixture_directory("header-md");
            // The domestic title is at 0x150 in the Mega Drive header.
            let rom = cartridge(&root, "game.md", 0x150, "SONIC THE HEDGEHOG", 0x400);
            let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
            assert_eq!(inspection.system, "megadrive");
            assert!(
                matches!(inspection.source, MetadataSource::Header),
                "the title should come from the header, not the filename"
            );
            assert_eq!(inspection.title, "SONIC THE HEDGEHOG");
        }

        #[test]
        fn a_game_boy_title_is_read_from_its_declared_window() {
            let root = fixture_directory("header-gb");
            // A .gb file can be for Game Boy or Game Boy Color, so we check for
            // the boot logo at 0x104 before we use the header.
            let mut bytes = vec![0u8; 0x200];
            bytes[0x104..0x134].copy_from_slice(NINTENDO_LOGO);
            bytes[0x134..0x134 + 6].copy_from_slice(b"TETRIS");
            // We also verify the header checksum at 0x14D, which every
            // genuine cartridge has, so the fixture has one too.
            bytes[0x14D] = bytes[0x134..=0x14C].iter().fold(0_u8, |checksum, byte| {
                checksum.wrapping_sub(*byte).wrapping_sub(1)
            });
            let rom = root.join("game.gb");
            fs::write(&rom, bytes).unwrap();
            let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
            assert_eq!(inspection.system, "gb");
            assert!(matches!(inspection.source, MetadataSource::Header));
            assert_eq!(inspection.title, "TETRIS");
        }

        #[test]
        fn a_console_declaring_no_header_window_falls_back_to_the_filename() {
            let root = fixture_directory("header-none");
            // The Master System header has no title field, so we declare none
            // in its package and use the filename.
            let rom = cartridge(&root, "Wonder Boy.sms", 0x10, "NOTATITLE", 0x200);
            let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
            assert_eq!(inspection.system, "mastersystem");
            assert!(
                matches!(inspection.source, MetadataSource::Filename),
                "nothing should be read out of a console that declares no window"
            );
            assert_eq!(inspection.title, "Wonder Boy");
        }

        /// We declare the title window of every console in package data.
        #[test]
        fn the_consoles_that_had_hardcoded_windows_still_declare_them() {
            for (id, offset, length) in [
                ("megadrive", 0x150, 0x180 - 0x150),
                ("gb", 0x134, 0x143 - 0x134),
                ("gbc", 0x134, 0x143 - 0x134),
                ("gba", 0xA0, 0xAC - 0xA0),
                ("n64", 0x20, 0x34 - 0x20),
                ("atari7800", 17, 49 - 17),
            ] {
                let window = crate::systems::find(id)
                    .expect("known console")
                    .header_title
                    .unwrap_or_else(|| panic!("{id} lost its declared header window"));
                assert_eq!((window.offset, window.length), (offset, length), "{id}");
            }
        }
    }

    #[test]
    fn a_zipped_rom_is_identified_from_the_game_inside() {
        let root = fixture_directory("zip-nes");
        let archive_path = root.join("game.zip");
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("readme.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"notes").unwrap();
        let mut bytes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        bytes.extend([1, 2, 3, 4]);
        writer
            .start_file("fixture.nes", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
        writer.finish().unwrap();

        let cache = root.join("cache");
        let catalog = cache
            .join("catalogs")
            .join("Nintendo - Nintendo Entertainment System.dat");
        std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
        std::fs::write(
            &catalog,
            r#"clrmamepro (
  name "fixture"
)
game (
  name "Tiny Adventure (USA)"
  rom ( name "fixture.nes" size 4 crc B63CFBCD sha1 12dada1fff4d4787ade3333147202c3b443e376f )
)
"#,
        )
        .unwrap();

        let inspection = inspect_game(&archive_path, &cache, false).unwrap();
        assert!(inspection.matched);
        assert_eq!(inspection.filename, "game.zip");
        assert_eq!(inspection.title, "Tiny Adventure");
    }

    #[test]
    fn an_extra_tag_on_the_cover_is_used_when_the_picture_list_has_it() {
        let root = fixture_directory("cover-tag");
        let rom = root.join("fixture.nes");
        let mut bytes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        bytes.extend([1, 2, 3, 4]);
        std::fs::write(&rom, bytes).unwrap();
        let cache = root.join("cache");
        let catalog = cache
            .join("catalogs")
            .join("Nintendo - Nintendo Entertainment System.dat");
        std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
        std::fs::write(
            &catalog,
            r#"clrmamepro (
  name "fixture"
)
game (
  name "Tiny Adventure (USA)"
  rom ( name "fixture.nes" size 4 crc B63CFBCD sha1 12dada1fff4d4787ade3333147202c3b443e376f )
)
"#,
        )
        .unwrap();
        let index = cache
            .join("artwork-index")
            .join("Nintendo - Nintendo Entertainment System.txt");
        std::fs::create_dir_all(index.parent().unwrap()).unwrap();
        std::fs::write(&index, "Tiny Adventure (USA) (Unl)\n").unwrap();
        let picture = cache
            .join("artwork")
            .join("Nintendo - Nintendo Entertainment System")
            .join("Named_Boxarts")
            .join("Tiny Adventure (USA) (Unl).png");
        std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
        std::fs::write(&picture, b"\x89PNG\r\n\x1a\n").unwrap();

        let inspection = inspect_game(&rom, &cache, false).unwrap();
        assert_eq!(inspection.icon_path.as_deref(), Some(picture.as_path()));
        assert!(inspection
            .warnings
            .iter()
            .all(|warning| warning != "No cover is published for this game."));
    }

    #[test]
    fn a_playstation_disc_is_identified_from_its_serial() {
        let root = fixture_directory("ps1-serial");
        let bin = root.join("track.bin");
        let mut image = vec![0; 64];
        image[..11].copy_from_slice(b"SLUS_012.34");
        std::fs::write(&bin, &image).unwrap();
        let cue = root.join("game.cue");
        std::fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
        let cache = root.join("cache");
        let catalog = cache.join("catalogs").join("Sony - PlayStation.dat");
        std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
        std::fs::write(
            &catalog,
            r#"clrmamepro (
  name "fixture"
)
game (
  name "Tiny Adventure (USA)"
  serial "SLUS-99999"
  rom ( name "other.bin" size 1 crc 00000000 serial "SLUS-99999" )
)
game (
  name "Crash Sample (USA)"
  serial "SLUS-01234"
  rom ( name "track.bin" size 999999 crc FFFFFFFF serial "SLUS-01234" )
)
"#,
        )
        .unwrap();

        let inspection =
            inspect_game_with_system(&cue, &cache, false, Some("PlayStation")).unwrap();
        assert_eq!(inspection.system, "ps1");
        assert!(inspection.matched, "{:?}", inspection.warnings);
        assert_eq!(inspection.title, "Crash Sample");
    }

    #[test]
    fn a_compressed_disc_is_not_checksummed() {
        let root = fixture_directory("chd");
        let rom = root.join("game.chd");
        std::fs::write(&rom, b"not a real compressed disc").unwrap();
        let inspection =
            inspect_game_with_system(&rom, &root.join("cache"), false, Some("PlayStation"))
                .unwrap();
        assert!(!inspection.matched);
        assert!(
            inspection
                .warnings
                .iter()
                .any(|warning| warning.contains("compressed")),
            "{:?}",
            inspection.warnings
        );
    }
}
