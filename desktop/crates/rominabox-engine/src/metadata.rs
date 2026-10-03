use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;

use crate::{artwork, content, discs, dumps};

use crate::systems::{self, System};

const CATALOG_LIMIT: u64 = 32 * 1024 * 1024;
const ARTWORK_LIMIT: u64 = 8 * 1024 * 1024;
const NO_COVER: &str = "No cover is published for this game.";

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
/// extensions that several consoles use, such as CUE, CHD and ISO. For a
/// game with patches, we inspect the patched game (metadata/patched.rs).
pub fn inspect_game_with_system(
    rom: &Path,
    cache: &Path,
    online: bool,
    system_override: Option<&str>,
) -> Result<Inspection, InspectionError> {
    let original = inspect_file(rom, cache, online, system_override)?;
    patched::join(original, rom, cache, online)
}

/// The lookup of one game file as it is on disk.
fn inspect_file(
    rom: &Path,
    cache: &Path,
    online: bool,
    system_override: Option<&str>,
) -> Result<Inspection, InspectionError> {
    let dropped = rom.to_path_buf();
    let rom = content::resolve_dropped(rom).map_err(InspectionError::new)?;
    let metadata = rom
        .metadata()
        .map_err(|_| InspectionError::new("Choose an existing game file."))?;
    if !metadata.is_file() {
        return Err(InspectionError::new("Choose an existing game file."));
    }
    if metadata.len() == 0 {
        return Err(InspectionError::new("Choose a non-empty game file."));
    }

    let prepared = dumps::prepare(&rom).map_err(InspectionError::new)?;
    let filename = prepared.filename.clone();
    let extension = prepared.extension.clone();
    let header = dumps::identification_header(&prepared.path)?;
    let queries = disc_name_queries(&filename, &dropped);
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
    // We read once, because we need the serial both to find the console and
    // to match the catalogue, and we have to decompress a CHD to get it.
    let disc_read = if systems::candidates_for_extension(&extension)
        .iter()
        .any(|candidate| candidate.category == "disc")
    {
        Some(discs::read_disc(&prepared.path, &extension))
    } else {
        None
    };
    let (system, mut warnings) = resolve_disc(system, warnings, disc_read.as_ref(), &extension);
    let mut title = filename_title(&filename);
    let mut source = MetadataSource::Filename;

    // We declare the title window for every console with an ASCII title, and
    // prefer a filename that already spells the game. For example, a Game Boy
    // Advance title has 12 characters, so Sonic Advance is "SONIC ADVANC".
    if let Some(selected) = system {
        let cartridge =
            cartridge_for_titles(&header, &prepared.path, &extension, &selected.header_title);
        let prefix = match disc_read.as_ref() {
            Some(discs::DiscRead::Found { prefix, .. }) => Some(prefix.as_slice()),
            _ => None,
        };
        if let Some(header_title) = title_from_windows(&selected.header_title, &cartridge, prefix) {
            if header_is_better(&title, &header_title) {
                title = header_title;
                source = MetadataSource::Header;
            }
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
            icon_path: None,
            warnings,
        });
    };

    let mut catalog_name = None;
    let mut icon_path = None;
    let disc = system.category == "disc";
    if system.catalog.is_some() {
        if disc {
            let read = disc_read
                .as_ref()
                .expect("a disc console is reached through a disc extension");
            apply_disc_catalog(
                &mut system,
                read,
                &queries,
                cache,
                online,
                &mut title,
                &mut source,
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
        icon_path,
        warnings,
    })
}

struct CatalogEntry {
    name: String,
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
    read: &discs::DiscRead,
    names: &[String],
    cache: &Path,
    online: bool,
    title: &mut String,
    source: &mut MetadataSource,
    catalog_name: &mut Option<String>,
    icon_path: &mut Option<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), InspectionError> {
    let mut serial_note = None;
    let keys = match read {
        discs::DiscRead::Compressed => {
            serial_note = Some(
                "This disc image is compressed, so its serial cannot be read. Use a cue, iso or gdi image."
                    .to_owned(),
            );
            Vec::new()
        }
        discs::DiscRead::Unreadable(message) => {
            serial_note = Some(message.clone());
            Vec::new()
        }
        discs::DiscRead::Found { keys, .. } => keys.clone(),
    };
    let catalog = system.catalog.as_deref().expect("disc catalogue");
    let path = match catalog_path(cache, catalog, online, true) {
        Ok(Some(path)) => path,
        Ok(None) => {
            if let Some(note) = serial_note {
                warnings.push(note);
            }
            warnings.push(format!("The {catalog} serial catalogue is not cached."));
            return Ok(());
        }
        Err(error) => {
            warnings.push(format!("Could not fetch {catalog}: {error}"));
            return Ok(());
        }
    };
    let candidate = *system;
    let serial = if keys.is_empty() {
        None
    } else {
        match match_serial(&keys, &candidate.id, &path) {
            Ok(found) => found,
            // Two editions have the same serial, so we choose by filename, for
            // example to tell Sonic Adventure 2 from its beta.
            Err(_) => None,
        }
    };
    if let Some(entry) = serial {
        return remember_match(
            system,
            candidate,
            &entry,
            catalog,
            cache,
            online,
            title,
            source,
            catalog_name,
            icon_path,
            warnings,
        );
    }
    if let Some(entry) = match_by_name(names, &path)? {
        remember_match(
            system,
            candidate,
            &entry,
            catalog,
            cache,
            online,
            title,
            source,
            catalog_name,
            icon_path,
            warnings,
        )?;
        // The title is from the catalogue row, which the serial did not pick.
        *source = MetadataSource::Filename;
        return Ok(());
    }
    if let Some(note) = serial_note {
        warnings.push(note);
    } else if keys.is_empty() {
        warnings.push("No serial was found in this disc image. Using the filename.".into());
    } else {
        warnings.push("No catalogue entry has this disc's serial. Using the filename.".into());
    }
    Ok(())
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
    catalog_name: &mut Option<String>,
    icon_path: &mut Option<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), InspectionError> {
    *system = candidate;
    *title = display_title(&entry.name);
    *source = MetadataSource::Catalog;
    *catalog_name = Some(entry.name.clone());
    match lookup_boxart(cache, catalog, &entry.name, online) {
        Ok(Some(path)) => *icon_path = Some(path),
        Ok(None) => warnings.push(NO_COVER.into()),
        Err(error) => warnings.push(format!("Box art was not available: {error}")),
    }
    Ok(())
}

fn resolve_disc(
    system: Option<&'static systems::System>,
    warnings: Vec<String>,
    read: Option<&discs::DiscRead>,
    extension: &str,
) -> (Option<&'static systems::System>, Vec<String>) {
    if system.is_some() {
        return (system, warnings);
    }
    let Some(discs::DiscRead::Found {
        system_id: Some(id),
        keys,
        ..
    }) = read
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

fn disc_name_queries(filename: &str, dropped: &Path) -> Vec<String> {
    let mut queries = Vec::new();
    let mut push = |value: &str| {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !queries.iter().any(|existing| existing == trimmed) {
            queries.push(trimmed.to_owned());
        }
    };
    if let Some(stem) = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
    {
        push(stem);
    }
    if dropped.is_dir() {
        if let Some(name) = dropped.file_name().and_then(|name| name.to_str()) {
            push(name);
        }
    }
    queries
}

/// We use the filename only when the serial is missing or two catalogue rows
/// have it. We compare names with the code of the cover matcher, so we do
/// not parse a name in a second way.
fn match_by_name(
    names: &[String],
    catalog_path: &Path,
) -> Result<Option<CatalogEntry>, InspectionError> {
    let dat = datary::read_file(catalog_path)
        .map_err(|error| InspectionError::new(format!("invalid DAT catalog: {error}")))?;
    let mut entry_for_name: HashMap<String, CatalogEntry> = HashMap::new();
    let mut catalog_names = Vec::new();
    for game in dat.games {
        if entry_for_name.contains_key(&game.name) {
            continue;
        }
        catalog_names.push(game.name.clone());
        entry_for_name.insert(
            game.name.clone(),
            CatalogEntry {
                name: game.name.clone(),
            },
        );
    }
    let index = artwork::ArtworkIndex::from_filenames(&catalog_names);
    for name in names {
        let Some(found) = artwork::match_cover(&index, name) else {
            continue;
        };
        if let Some(entry) = entry_for_name.remove(&found.filename) {
            return Ok(Some(entry));
        }
    }
    Ok(None)
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
    let bytes = download(&checksum_catalog_url(catalog, disc), CATALOG_LIMIT, WAIT)?;
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
        WAIT,
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
    let bytes = download(
        &artwork::artwork_index_url(catalog),
        CATALOG_LIMIT,
        PICTURE_LIST_WAIT,
    )?;
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

/// The longest time we allow for a lookup download, from start to end.
const WAIT: Duration = Duration::from_secs(5);
/// On GitHub, building the list of pictures for a console takes several
/// seconds when nobody has asked for that list recently, and about a second
/// otherwise.
const PICTURE_LIST_WAIT: Duration = Duration::from_secs(60);

fn download(url: &str, limit: u64, wait: Duration) -> Result<Vec<u8>, InspectionError> {
    if crate::cores::offline() {
        panic!(
            "{} is set and a lookup was asked of the network",
            crate::cores::OFFLINE_VARIABLE
        );
    }
    let response = ureq::get(url)
        .timeout(wait)
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

fn cartridge_for_titles(
    header: &[u8],
    path: &Path,
    extension: &str,
    windows: &[systems::HeaderTitle],
) -> Vec<u8> {
    let needed = windows
        .iter()
        .filter(|window| window.anchor.is_none())
        .map(|window| {
            let end = window.offset + window.length;
            let complement = window
                .complement_at
                .map(|at| window.offset + at + 4)
                .unwrap_or(0);
            end.max(complement) as usize
        })
        .max()
        .unwrap_or(0);
    if needed <= header.len() {
        return header.to_vec();
    }
    dumps::image_prefix(path, extension, needed).unwrap_or_else(|_| header.to_vec())
}

fn title_from_windows(
    windows: &[systems::HeaderTitle],
    cartridge: &[u8],
    disc_prefix: Option<&[u8]>,
) -> Option<String> {
    for window in windows {
        let Some(bytes) = window_bytes(window, cartridge, disc_prefix) else {
            continue;
        };
        let start = match &window.anchor {
            Some(anchor) => {
                let needle = anchor.as_bytes();
                if needle.is_empty() {
                    continue;
                }
                let Some(at) = bytes
                    .windows(needle.len())
                    .position(|found| found == needle)
                else {
                    continue;
                };
                at + window.offset as usize
            }
            None => window.offset as usize,
        };
        if let Some(magic) = &window.magic {
            if !magic_matches(bytes, magic) {
                continue;
            }
        }
        if let Some(at) = window.complement_at {
            if !complement_matches(bytes, start + at as usize) {
                continue;
            }
        }
        if let Some(title) = ascii_title(bytes, start, start + window.length as usize) {
            return Some(title);
        }
    }
    None
}

fn window_bytes<'a>(
    window: &systems::HeaderTitle,
    cartridge: &'a [u8],
    disc_prefix: Option<&'a [u8]>,
) -> Option<&'a [u8]> {
    if window.anchor.is_some() {
        disc_prefix
    } else {
        Some(cartridge)
    }
}

fn magic_matches(bytes: &[u8], magic: &systems::HeaderMagic) -> bool {
    let offset = magic.offset as usize;
    if let Some(text) = &magic.text {
        let needle = text.as_bytes();
        if bytes.get(offset..offset + needle.len()) != Some(needle) {
            return false;
        }
    }
    if let Some(hex) = &magic.hex {
        let Some(needle) = decode_hex(hex) else {
            return false;
        };
        if bytes.get(offset..offset + needle.len()) != Some(needle.as_slice()) {
            return false;
        }
    }
    magic.text.is_some() || magic.hex.is_some()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || text.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let raw = text.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        let hi = hex_value(raw[index])?;
        let lo = hex_value(raw[index + 1])?;
        out.push((hi << 4) | lo);
        index += 2;
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The two little-endian halves of a Super Nintendo header checksum.
fn complement_matches(bytes: &[u8], at: usize) -> bool {
    let Some(pair) = bytes.get(at..at + 4) else {
        return false;
    };
    let checksum = u16::from_le_bytes([pair[0], pair[1]]);
    let complement = u16::from_le_bytes([pair[2], pair[3]]);
    checksum.wrapping_add(complement) == 0xFFFF
}

mod patched;
mod titles;
use titles::*;

#[cfg(test)]
mod tests;
