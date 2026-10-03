//! Versioned, self-contained archives of authoring projects.
//!
//! A project archive is a ZIP with exactly one `manifest.json` and the assets
//! listed in the manifest. We store assets without compression, so that we can
//! copy disc images of several gigabytes with bounded streaming.

use crate::content;
use crate::controls;
use crate::game::Game;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

mod request;

const FORMAT_VERSION: u32 = 3;
const MANIFEST_PATH: &str = "manifest.json";
const CONTENT_PREFIX: &str = "content/";
const FIRMWARE_PREFIX: &str = "firmware/";
const ICON_PREFIX: &str = "assets/icon.";
const BACKGROUND_PREFIX: &str = "assets/background.";
const MAX_ARCHIVE_ENTRIES: usize = 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 =
    MAX_ASSET_BYTES + (2 * 1024 * 1024 * 1024) + MAX_MANIFEST_BYTES;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSaveRequest {
    pub archive_path: PathBuf,
    /// The game the project makes: its files and every choice.
    pub settings: Game,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpenRequest {
    pub archive_path: PathBuf,
    /// Must be a new, explicit directory. We never replace an existing path.
    pub extraction_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchiveResult {
    pub archive_path: PathBuf,
    pub archive_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProject {
    pub archive_path: PathBuf,
    pub extraction_dir: PathBuf,
    /// Contains absolute paths to extracted assets. It has no runtime kit,
    /// explicit core, or previous output directory, which are machine-local.
    pub settings: Game,
}

/// The game, with its files named after their place in the archive, and
/// the files in the archive that are in no single setting.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectManifest {
    format_version: u32,
    game: Game,
    assets: ProjectAssets,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectAssets {
    /// Every file of the game's content, its ROM (`game.rom`) among them.
    content: Vec<String>,
    /// Every file of its custom shaders, passes included.
    #[serde(default)]
    shaders: Vec<String>,
}

pub fn save_project(request: &ProjectSaveRequest) -> Result<ProjectArchiveResult, String> {
    validate_settings(&request.settings)?;
    let content = content::collect_with(
        &request.settings.rom,
        Some(&request.settings.system),
        &request.settings.files,
    )?;
    refuse_existing(&request.archive_path, "project archive")?;
    let parent = request
        .archive_path
        .parent()
        .ok_or_else(|| "project archive path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| path_error("create archive directory", parent, error))?;

    let content_assets = content
        .files
        .iter()
        .map(|file| archive_content_name(&file.relative))
        .collect::<Result<Vec<_>, _>>()?;
    // We put the game's patches beside it, under their own names.
    let patch_assets = content
        .patches()
        .iter()
        .map(|patch| archive_named_asset(CONTENT_PREFIX, patch))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_names(&[content_assets.as_slice(), &patch_assets].concat(), "content")?;
    let firmware_assets = request
        .settings
        .firmware
        .iter()
        .map(|path| archive_named_asset(FIRMWARE_PREFIX, path))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_names(&firmware_assets, "firmware")?;
    let (stored_shaders, shader_files) = crate::shaders::pack_selection(&request.settings.shaders)?;
    let shader_assets: Vec<String> = shader_files.iter().map(|(name, _)| name.clone()).collect();
    ensure_unique_names(&shader_assets, "shader")?;
    let icon_asset = request
        .settings
        .icon
        .as_deref()
        .map(|path| archive_asset_name(ICON_PREFIX, path))
        .transpose()?;
    let background_asset = request
        .settings
        .background
        .as_deref()
        .map(|path| archive_asset_name(BACKGROUND_PREFIX, path))
        .transpose()?;
    let manifest = ProjectManifest {
        format_version: FORMAT_VERSION,
        game: Game {
            // The game file as the author dropped it. We store a compressed disc
            // compressed, even when we decompress it at export.
            rom: PathBuf::from(archive_content_name(
                content.files.iter().find(|file| file.role == content::FileRole::Game).map_or(&content.entrypoint, |game| &game.relative),
            )?),
            icon: icon_asset.clone().map(PathBuf::from),
            background: background_asset.clone().map(PathBuf::from),
            firmware: firmware_assets.iter().map(PathBuf::from).collect(),
            shaders: stored_shaders,
            // We store the content as we export it: without what the author
            // left out, and with each added file as one of its files, under
            // its name in the archive. We store the original game, and its
            // patches as added files, which we apply at export.
            files: content::GameFiles {
                left_out: Vec::new(),
                added: content
                    .files
                    .iter()
                    .filter(|file| file.role == content::FileRole::Added)
                    .map(|file| archive_content_name(&file.relative))
                    .chain(patch_assets.iter().cloned().map(Ok))
                    .map(|name| name.map(PathBuf::from))
                    .collect::<Result<_, _>>()?,
                decompress: request.settings.files.decompress,
            },
            ..request.settings.clone()
        },
        assets: ProjectAssets {
            content: [content_assets, patch_assets.clone()].concat(),
            shaders: shader_assets,
        },
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize project manifest: {error}"))?;
    validate_save_sizes(&request.settings, &content, manifest_bytes.len() as u64)?;
    let temporary = unique_sibling(&request.archive_path, "save");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| path_error("create project staging archive", &temporary, error))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true);
    write_bytes(&mut writer, MANIFEST_PATH, &manifest_bytes, options)?;
    for (file, name) in content.files.iter().zip(&manifest.assets.content) {
        match &file.staging {
            content::Staging::Bytes(bytes) => write_bytes(&mut writer, name, bytes, options)?,
            content::Staging::Copy | content::Staging::Patched(_) | content::Staging::Unpacked(_) => {
                write_path(&mut writer, name, &file.source, MAX_ASSET_BYTES, options)?
            }
        }
    }
    for (patch, name) in content.patches().iter().zip(&patch_assets) {
        write_path(&mut writer, name, patch, MAX_ASSET_BYTES, options)?;
    }
    for (path, name) in request.settings.firmware.iter().zip(&firmware_assets) {
        write_path(&mut writer, name, path, MAX_ASSET_BYTES, options)?;
    }
    for (name, path) in &shader_files {
        write_path(&mut writer, name, path, MAX_ASSET_BYTES, options)?;
    }
    if let (Some(name), Some(path)) = (&icon_asset, &request.settings.icon) {
        write_path(&mut writer, name, path, MAX_IMAGE_BYTES, options)?;
    }
    if let (Some(name), Some(path)) = (&background_asset, &request.settings.background) {
        write_path(&mut writer, name, path, MAX_IMAGE_BYTES, options)?;
    }
    writer
        .finish()
        .map_err(|error| format!("finish project archive: {error}"))?;

    // On the supported filesystems, creating a hard link is an atomic
    // publication that never overwrites. If this step fails, we leave the
    // staging file in place and never overwrite the final path. After we
    // publish, the staging name is only a second name for the project.
    fs::hard_link(&temporary, &request.archive_path).map_err(|error| {
        path_error(
            "publish project archive without overwrite",
            &request.archive_path,
            error,
        )
    })?;
    fs::remove_file(&temporary)
        .map_err(|error| path_error("remove project staging archive", &temporary, error))?;
    let archive_bytes = fs::metadata(&request.archive_path)
        .map_err(|error| path_error("measure project archive", &request.archive_path, error))?
        .len();
    Ok(ProjectArchiveResult {
        archive_path: request.archive_path.clone(),
        archive_bytes,
    })
}

pub fn open_project(request: &ProjectOpenRequest) -> Result<OpenProject, String> {
    if !request.archive_path.is_file() {
        return Err(format!(
            "project archive does not exist: {}",
            request.archive_path.display()
        ));
    }
    refuse_existing(&request.extraction_dir, "project extraction directory")?;
    let file = File::open(&request.archive_path)
        .map_err(|error| path_error("open project archive", &request.archive_path, error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("read project archive: {error}"))?;
    let manifest = validate_archive(&mut archive)?;

    fs::create_dir(&request.extraction_dir).map_err(|error| {
        path_error(
            "create project extraction directory",
            &request.extraction_dir,
            error,
        )
    })?;
    let root = &request.extraction_dir;
    for name in manifest.assets.content.iter().chain(&manifest.assets.shaders) {
        extract_asset(&mut archive, name, root, MAX_ASSET_BYTES)?;
    }
    let stored = manifest.game;
    let firmware = stored
        .firmware
        .iter()
        .map(|name| extract_asset(&mut archive, stored_name(name)?, root, MAX_ASSET_BYTES))
        .collect::<Result<Vec<_>, _>>()?;
    let mut picture = |name: &Option<PathBuf>| {
        name.as_deref()
            .map(|name| extract_asset(&mut archive, stored_name(name)?, root, MAX_IMAGE_BYTES))
            .transpose()
    };
    let (icon, background) = (picture(&stored.icon)?, picture(&stored.background)?);
    let settings = Game {
        rom: root.join(&stored.rom),
        icon,
        background,
        firmware,
        shaders: crate::shaders::unpack_selection(stored.shaders.clone(), root),
        files: content::GameFiles {
            left_out: Vec::new(),
            added: stored.files.added.iter().map(|name| root.join(name)).collect(),
            decompress: stored.files.decompress,
        },
        ..stored
    };
    Ok(OpenProject {
        archive_path: request.archive_path.clone(),
        extraction_dir: request.extraction_dir.clone(),
        settings,
    })
}

fn validate_archive<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<ProjectManifest, String> {
    if archive.len() == 0 || archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "project archive must contain 1 to {MAX_ARCHIVE_ENTRIES} files"
        ));
    }
    let mut names = Vec::with_capacity(archive.len());
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("read project archive entry: {error}"))?;
        if !file.is_file() {
            return Err(format!(
                "project archive contains a directory or symlink: {}",
                file.name()
            ));
        }
        ensure_safe_archive_name(file.name())?;
        total = total
            .checked_add(file.size())
            .ok_or_else(|| "project archive size overflow".to_string())?;
        if file.size() > MAX_ASSET_BYTES || total > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(
                "project archive exceeds the supported uncompressed size limit".to_string(),
            );
        }
        names.push(file.name().to_string());
    }
    let mut manifest_file = archive
        .by_name(MANIFEST_PATH)
        .map_err(|_| "project archive is missing manifest.json".to_string())?;
    if manifest_file.size() > MAX_MANIFEST_BYTES {
        return Err("project manifest is too large".to_string());
    }
    let mut manifest_bytes =
        Vec::with_capacity(manifest_file.size().min(MAX_MANIFEST_BYTES) as usize);
    let manifest_read = manifest_file
        .by_ref()
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut manifest_bytes)
        .map_err(|error| format!("read project manifest: {error}"))?;
    if manifest_read as u64 > MAX_MANIFEST_BYTES || manifest_read as u64 != manifest_file.size() {
        return Err("project manifest size does not match its archive entry".to_string());
    }
    let manifest: ProjectManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("invalid project manifest: {error}"))?;
    validate_manifest(&manifest, &names)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &ProjectManifest, names: &[String]) -> Result<(), String> {
    if manifest.format_version != FORMAT_VERSION {
        return Err(format!(
            "unsupported project format version: {}",
            manifest.format_version
        ));
    }
    let game = &manifest.game;
    validate_choices(game)?;
    let rom = stored_name(&game.rom)?.to_string();
    let firmware = game
        .firmware
        .iter()
        .map(|name| stored_name(name).map(str::to_string))
        .collect::<Result<Vec<_>, _>>()?;
    let icon = game.icon.as_deref().map(stored_name).transpose()?;
    let background = game.background.as_deref().map(stored_name).transpose()?;
    let mut expected = vec![MANIFEST_PATH.to_string()];
    expected.extend(manifest.assets.content.iter().cloned());
    expected.extend(firmware.iter().cloned());
    expected.extend(manifest.assets.shaders.iter().cloned());
    expected.extend(icon.into_iter().chain(background).map(str::to_string));
    if names.len() != expected.len()
        || expected
            .iter()
            .any(|name| !names.iter().any(|actual| actual == name))
    {
        return Err("project archive entries do not match its manifest".to_string());
    }
    for name in &manifest.assets.content {
        validate_prefixed_path(name, CONTENT_PREFIX)?;
    }
    if !manifest.assets.content.contains(&rom) {
        return Err("project content does not include its entrypoint".to_string());
    }
    for name in &firmware {
        validate_prefixed_path(name, FIRMWARE_PREFIX)?;
    }
    for name in &manifest.assets.shaders {
        validate_prefixed_path(name, "shaders/")?;
    }
    ensure_unique_names(&expected, "project asset")?;
    if let Some(icon) = icon {
        validate_asset_name(icon, ICON_PREFIX)?;
    }
    if let Some(background) = background {
        validate_asset_name(background, BACKGROUND_PREFIX)?;
    }
    Ok(())
}

fn validate_settings(settings: &Game) -> Result<(), String> {
    validate_choices(settings)?;
    for (label, path) in [
        ("ROM", Some(&settings.rom)),
        ("icon", settings.icon.as_ref()),
        ("background", settings.background.as_ref()),
    ] {
        if let Some(path) = path {
            if !path.is_file() {
                return Err(format!("{label} file does not exist: {}", path.display()));
            }
        }
    }
    ensure_unique_file_names(&settings.firmware)?;
    for path in &settings.firmware {
        if !path.is_file() {
            return Err(format!("firmware file does not exist: {}", path.display()));
        }
    }
    Ok(())
}

fn validate_save_sizes(
    settings: &Game,
    content: &content::ContentSet,
    manifest_bytes: u64,
) -> Result<(), String> {
    if manifest_bytes > MAX_MANIFEST_BYTES {
        return Err("project manifest is too large".to_string());
    }
    let mut total = manifest_bytes;
    for (label, path, limit) in [
        ("icon", settings.icon.as_ref(), MAX_IMAGE_BYTES),
        ("background", settings.background.as_ref(), MAX_IMAGE_BYTES),
    ] {
        let Some(path) = path else {
            continue;
        };
        let size = fs::metadata(path)
            .map_err(|error| path_error("measure project asset", path, error))?
            .len();
        if size > limit {
            return Err(format!(
                "{label} exceeds its supported size limit: {}",
                path.display()
            ));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| "project archive size overflow".to_string())?;
        if total > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err("project assets exceed the supported archive size limit".to_string());
        }
    }
    for path in content.files.iter().map(|file| file.source.clone()).chain(content.patches()) {
        total = add_sized_asset(total, "game content", &path, MAX_ASSET_BYTES)?;
    }
    for path in &settings.firmware {
        total = add_sized_asset(total, "firmware", path, MAX_ASSET_BYTES)?;
    }
    Ok(())
}

fn add_sized_asset(total: u64, label: &str, path: &Path, limit: u64) -> Result<u64, String> {
    let size = fs::metadata(path)
        .map_err(|error| path_error("measure project asset", path, error))?
        .len();
    if size > limit {
        return Err(format!(
            "{label} exceeds its supported size limit: {}",
            path.display()
        ));
    }
    let total = total
        .checked_add(size)
        .ok_or_else(|| "project archive size overflow".to_string())?;
    if total > MAX_TOTAL_UNCOMPRESSED_BYTES {
        return Err("project assets exceed the supported archive size limit".to_string());
    }
    Ok(total)
}

/// The rules for a game's choices, which we check when we save or open it.
fn validate_choices(settings: &Game) -> Result<(), String> {
    if settings.title.trim().is_empty() || settings.system.trim().is_empty() {
        return Err("project title and system are required".to_string());
    }
    if settings.start_at_menu && !settings.show_menu {
        return Err("startAtMenu requires showMenu".to_string());
    }
    if settings.theme.trim().is_empty() || settings.palette.trim().is_empty() {
        return Err("project theme and palette are required".to_string());
    }
    let design = crate::themes::design_root(&settings.theme)?;
    crate::themes::palette(&settings.palette)?;
    crate::achievements::entries(
        &design,
        settings.include_achievements,
        settings.show_menu,
        settings.menu_entries.as_deref(),
    )?;
    controls::validate_for_system(&settings.system, &settings.controls)?;
    settings
        .hotkeys
        .check_for(&settings.system, &settings.controls)
        .map_err(|refusal| refusal.to_string())?;
    Ok(())
}

fn write_bytes<W: Write + io::Seek>(
    writer: &mut ZipWriter<W>,
    name: &str,
    bytes: &[u8],
    options: SimpleFileOptions,
) -> Result<(), String> {
    writer
        .start_file(name, options)
        .map_err(|error| format!("start project archive member {name}: {error}"))?;
    writer
        .write_all(bytes)
        .map_err(|error| format!("write project archive member {name}: {error}"))
}

fn write_path<W: Write + io::Seek>(
    writer: &mut ZipWriter<W>,
    name: &str,
    path: &Path,
    limit: u64,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let size = fs::metadata(path)
        .map_err(|error| path_error("measure project asset", path, error))?
        .len();
    if size > limit {
        return Err(format!(
            "project asset exceeds its supported size limit of {limit} bytes: {}",
            path.display()
        ));
    }
    writer
        .start_file(name, options)
        .map_err(|error| format!("start project archive member {name}: {error}"))?;
    let mut source =
        File::open(path).map_err(|error| path_error("open project asset", path, error))?;
    copy_bounded(&mut source, writer, limit)
        .map_err(|error| format!("write project asset {}: {error}", path.display()))?;
    Ok(())
}

fn extract_asset<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    extraction_dir: &Path,
    limit: u64,
) -> Result<PathBuf, String> {
    let destination = extraction_dir.join(name);
    let parent = destination
        .parent()
        .ok_or_else(|| "project asset has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| path_error("create project asset directory", parent, error))?;
    let mut source = archive
        .by_name(name)
        .map_err(|_| format!("project archive is missing asset: {name}"))?;
    let expected = source.size();
    if expected > limit {
        return Err(format!(
            "project asset exceeds its supported size limit: {name}"
        ));
    }
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| path_error("create extracted project asset", &destination, error))?;
    let copied = copy_bounded(&mut source, &mut destination_file, limit)
        .map_err(|error| format!("extract project asset {name}: {error}"))?;
    if copied != expected {
        return Err(format!(
            "project asset size does not match its archive entry: {name}"
        ));
    }
    Ok(destination)
}

fn copy_bounded(reader: &mut impl Read, writer: &mut impl Write, limit: u64) -> io::Result<u64> {
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(total);
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::other("size overflow"))?;
        if total > limit {
            return Err(io::Error::other("size limit exceeded"));
        }
        writer.write_all(&buffer[..read])?;
    }
}

fn archive_asset_name(prefix: &str, path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("bin");
    if !extension
        .chars()
        .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(format!(
            "asset has an unsupported file extension: {}",
            path.display()
        ));
    }
    Ok(format!("{prefix}{extension}"))
}

fn archive_content_name(relative: &Path) -> Result<String, String> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "invalid relative game content path: {}",
            relative.display()
        ));
    }
    let parts = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            format!(
                "game content filename is not valid Unicode: {}",
                relative.display()
            )
        })?;
    Ok(format!("{CONTENT_PREFIX}{}", parts.join("/")))
}

fn archive_named_asset(prefix: &str, path: &Path) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("asset has no usable filename: {}", path.display()))?;
    let archive_name = format!("{prefix}{name}");
    validate_prefixed_path(&archive_name, prefix)?;
    Ok(archive_name)
}

/// A file the archived game names, as the archive names it.
fn stored_name(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("invalid project asset reference: {}", path.display()))
}

fn validate_prefixed_path(name: &str, prefix: &str) -> Result<(), String> {
    ensure_safe_archive_name(name)?;
    if !name.starts_with(prefix) || name.len() == prefix.len() {
        return Err(format!("invalid project asset reference: {name}"));
    }
    Ok(())
}

fn ensure_unique_names(names: &[String], label: &str) -> Result<(), String> {
    let mut unique = std::collections::HashSet::new();
    if let Some(duplicate) = names.iter().find(|name| !unique.insert(name.as_str())) {
        return Err(format!("duplicate {label} path: {duplicate}"));
    }
    Ok(())
}

fn ensure_unique_file_names(paths: &[PathBuf]) -> Result<(), String> {
    let names = paths
        .iter()
        .map(|path| archive_named_asset(FIRMWARE_PREFIX, path))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_names(&names, "firmware")
}

fn validate_asset_name(name: &str, prefix: &str) -> Result<(), String> {
    ensure_safe_archive_name(name)?;
    if !name.starts_with(prefix)
        || name[prefix.len()..].is_empty()
        || !name[prefix.len()..]
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(format!("invalid project asset reference: {name}"));
    }
    Ok(())
}

fn ensure_safe_archive_name(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe project archive entry: {name}"));
    }
    Ok(())
}

fn unique_sibling(destination: &Path, purpose: &str) -> PathBuf {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let stem = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{stem}.{purpose}-{}-{nonce}", std::process::id()))
}

fn refuse_existing(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() {
        Err(format!(
            "refusing to overwrite existing {label}: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

fn path_error(action: &str, path: &Path, error: io::Error) -> String {
    format!("{action} {}: {error}", path.display())
}

#[cfg(test)]
mod tests;
