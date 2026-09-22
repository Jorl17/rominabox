//! Versioned, self-contained archives of authoring projects.
//!
//! A project archive is a ZIP with exactly one `manifest.json` and the assets
//! listed in the manifest. We store assets without compression, so that we can
//! copy disc images of several gigabytes with bounded streaming.

use crate::content;
use crate::controls::{self, Controls};
use crate::packaging::{ExportRequest, ExportTarget};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const FORMAT_VERSION: u32 = 1;
const MANIFEST_PATH: &str = "manifest.json";
const ROM_PREFIX: &str = "assets/rom.";
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
pub struct ProjectSettings {
    pub rom: PathBuf,
    pub title: String,
    pub system: String,
    pub description: Option<String>,
    pub icon: Option<PathBuf>,
    pub background: Option<PathBuf>,
    pub show_menu: bool,
    pub start_at_menu: bool,
    pub theme: String,
    #[serde(default = "default_palette")]
    pub palette: String,
    #[serde(default = "crate::themes::default_menu_sounds")]
    pub menu_sounds: String,
    #[serde(default)]
    pub controls: Controls,
    /// Explicit firmware files. We keep them with this project and later
    /// install them only in the managed storage of the exported game.
    #[serde(default)]
    pub firmware: Vec<PathBuf>,
    /// Whether we include the short native splash in the player.
    #[serde(default)]
    pub splash: bool,
    /// Restore stock RetroArch native menus in the exported app.
    #[serde(default)]
    pub advanced_emulator_access: bool,
    /// Keep emulating when the window is not focused. Per game, set at export.
    #[serde(default)]
    pub keep_playing_in_background: bool,
    /// Save on quit and resume from that save next launch. Per game.
    #[serde(default)]
    pub autosave_on_quit: bool,
    /// The Options entries we offer in this game. When absent, we use the
    /// design's defaults. With an empty list, we show no Options button.
    #[serde(default)]
    pub menu_entries: Option<Vec<String>>,
    /// Presets bundled into the game. Usually this is empty.
    #[serde(default)]
    pub shaders: crate::shaders::ShaderSelection,
    #[serde(default)]
    pub achievements: crate::achievements::AchievementSelection,
    pub target: ExportTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSaveRequest {
    pub archive_path: PathBuf,
    pub settings: ProjectSettings,
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
    pub settings: ProjectSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectManifest {
    format_version: u32,
    settings: StoredSettings,
    assets: ProjectAssets,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    title: String,
    system: String,
    description: Option<String>,
    show_menu: bool,
    start_at_menu: bool,
    theme: String,
    palette: String,
    #[serde(default = "crate::themes::default_menu_sounds")]
    menu_sounds: String,
    #[serde(default)]
    controls: Controls,
    #[serde(default)]
    splash: bool,
    #[serde(default)]
    advanced_emulator_access: bool,
    #[serde(default)]
    keep_playing_in_background: bool,
    #[serde(default)]
    autosave_on_quit: bool,
    #[serde(default)]
    menu_entries: Option<Vec<String>>,
    #[serde(default)]
    shaders: crate::shaders::ShaderSelection,
    #[serde(default)]
    achievements: crate::achievements::AchievementSelection,
    target: ExportTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectAssets {
    rom: String,
    #[serde(default)]
    content: Vec<String>,
    #[serde(default)]
    firmware: Vec<String>,
    icon: Option<String>,
    background: Option<String>,
    /// Custom shader files. Catalog presets come with the builder, so we do not store them.
    #[serde(default)]
    shaders: Vec<String>,
}

impl From<&ExportRequest> for ProjectSettings {
    fn from(request: &ExportRequest) -> Self {
        Self {
            rom: request.rom.clone(),
            title: request.title.clone(),
            system: request.system.clone(),
            description: request.description.clone(),
            icon: request.icon.clone(),
            background: request.background.clone(),
            show_menu: request.show_menu,
            start_at_menu: request.start_at_menu,
            theme: request.theme.clone(),
            palette: request.palette.clone(),
            menu_sounds: request.menu_sounds.clone(),
            controls: request.controls.clone(),
            firmware: request.firmware.clone(),
            splash: request.splash,
            advanced_emulator_access: request.advanced_emulator_access,
            keep_playing_in_background: request.keep_playing_in_background,
            autosave_on_quit: request.autosave_on_quit,
            menu_entries: request.menu_entries.clone(),
            shaders: request.shaders.clone(),
            achievements: request.achievements.clone(),
            target: request.target.clone(),
        }
    }
}

impl ProjectSettings {
    /// Add the host-local export dependencies after someone opens a project.
    pub fn into_export_request(
        self,
        output_dir: PathBuf,
        runtime_kit: PathBuf,
        core: Option<PathBuf>,
    ) -> ExportRequest {
        ExportRequest {
            rom: self.rom,
            title: self.title,
            system: self.system,
            description: self.description,
            icon: self.icon,
            background: self.background,
            show_menu: self.show_menu,
            start_at_menu: self.start_at_menu,
            theme: self.theme,
            palette: self.palette,
            menu_sounds: self.menu_sounds,
            controls: self.controls,
            firmware: self.firmware,
            splash: self.splash,
            advanced_emulator_access: self.advanced_emulator_access,
            keep_playing_in_background: self.keep_playing_in_background,
            autosave_on_quit: self.autosave_on_quit,
            menu_entries: self.menu_entries,
            shaders: self.shaders,
            achievements: self.achievements,
            output_dir,
            target: self.target,
            runtime_kit,
            core,
            core_cache: None,
        }
    }
}

pub fn save_project(request: &ProjectSaveRequest) -> Result<ProjectArchiveResult, String> {
    validate_settings(&request.settings)?;
    let content = content::collect_for(&request.settings.rom, Some(&request.settings.system))?;
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
    let rom = archive_content_name(&content.entrypoint)?;
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
    let assets = ProjectAssets {
        rom,
        content: content_assets,
        firmware: firmware_assets,
        icon: request
            .settings
            .icon
            .as_deref()
            .map(|path| archive_asset_name(ICON_PREFIX, path))
            .transpose()?,
        background: request
            .settings
            .background
            .as_deref()
            .map(|path| archive_asset_name(BACKGROUND_PREFIX, path))
            .transpose()?,
        shaders: shader_assets,
    };
    let manifest = ProjectManifest {
        format_version: FORMAT_VERSION,
        settings: StoredSettings {
            title: request.settings.title.clone(),
            system: request.settings.system.clone(),
            description: request.settings.description.clone(),
            show_menu: request.settings.show_menu,
            start_at_menu: request.settings.start_at_menu,
            theme: request.settings.theme.clone(),
            palette: request.settings.palette.clone(),
            menu_sounds: request.settings.menu_sounds.clone(),
            controls: request.settings.controls.clone(),
            splash: request.settings.splash,
            advanced_emulator_access: request.settings.advanced_emulator_access,
            keep_playing_in_background: request.settings.keep_playing_in_background,
            autosave_on_quit: request.settings.autosave_on_quit,
            menu_entries: request.settings.menu_entries.clone(),
            shaders: stored_shaders,
            achievements: request.settings.achievements.clone(),
            target: request.settings.target.clone(),
        },
        assets: assets.clone(),
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
    for (file, name) in content.files.iter().zip(&assets.content) {
        if let Some(bytes) = &file.staged_bytes {
            write_bytes(&mut writer, name, bytes, options)?;
        } else {
            write_path(&mut writer, name, &file.source, MAX_ASSET_BYTES, options)?;
        }
    }
    for (path, name) in request.settings.firmware.iter().zip(&assets.firmware) {
        write_path(&mut writer, name, path, MAX_ASSET_BYTES, options)?;
    }
    for (name, path) in &shader_files {
        write_path(&mut writer, name, path, MAX_ASSET_BYTES, options)?;
    }
    if let (Some(name), Some(path)) = (&assets.icon, &request.settings.icon) {
        write_path(&mut writer, name, path, MAX_IMAGE_BYTES, options)?;
    }
    if let (Some(name), Some(path)) = (&assets.background, &request.settings.background) {
        write_path(&mut writer, name, path, MAX_IMAGE_BYTES, options)?;
    }
    writer
        .finish()
        .map_err(|error| format!("finish project archive: {error}"))?;

    // On the supported file systems, a hard link is an atomic publication that
    // never overwrites. If this step fails we leave the staging file in place,
    // and we never overwrite the final path.
    fs::hard_link(&temporary, &request.archive_path).map_err(|error| {
        path_error(
            "publish project archive without overwrite",
            &request.archive_path,
            error,
        )
    })?;
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
    let content_names = if manifest.assets.content.is_empty() {
        vec![manifest.assets.rom.clone()]
    } else {
        manifest.assets.content.clone()
    };
    for name in &content_names {
        extract_asset(&mut archive, name, &request.extraction_dir, MAX_ASSET_BYTES)?;
    }
    let rom = request.extraction_dir.join(&manifest.assets.rom);
    let firmware = manifest
        .assets
        .firmware
        .iter()
        .map(|name| extract_asset(&mut archive, name, &request.extraction_dir, MAX_ASSET_BYTES))
        .collect::<Result<Vec<_>, _>>()?;
    for name in &manifest.assets.shaders {
        extract_asset(&mut archive, name, &request.extraction_dir, MAX_ASSET_BYTES)?;
    }
    let icon = manifest
        .assets
        .icon
        .as_deref()
        .map(|name| extract_asset(&mut archive, name, &request.extraction_dir, MAX_IMAGE_BYTES))
        .transpose()?;
    let background = manifest
        .assets
        .background
        .as_deref()
        .map(|name| extract_asset(&mut archive, name, &request.extraction_dir, MAX_IMAGE_BYTES))
        .transpose()?;
    let settings = ProjectSettings {
        rom,
        title: manifest.settings.title,
        system: manifest.settings.system,
        description: manifest.settings.description,
        icon,
        background,
        show_menu: manifest.settings.show_menu,
        start_at_menu: manifest.settings.start_at_menu,
        theme: manifest.settings.theme,
        palette: manifest.settings.palette,
        menu_sounds: manifest.settings.menu_sounds,
        controls: manifest.settings.controls,
        firmware,
        splash: manifest.settings.splash,
        advanced_emulator_access: manifest.settings.advanced_emulator_access,
        keep_playing_in_background: manifest.settings.keep_playing_in_background,
        autosave_on_quit: manifest.settings.autosave_on_quit,
        menu_entries: manifest.settings.menu_entries,
        shaders: crate::shaders::unpack_selection(
            manifest.settings.shaders,
            &request.extraction_dir,
        ),
        achievements: manifest.settings.achievements,
        target: manifest.settings.target,
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
    validate_stored_settings(&manifest.settings)?;
    let mut expected = vec![MANIFEST_PATH.to_string()];
    if manifest.assets.content.is_empty() {
        expected.push(manifest.assets.rom.clone());
    } else {
        expected.extend(manifest.assets.content.iter().cloned());
    }
    expected.extend(manifest.assets.firmware.iter().cloned());
    expected.extend(manifest.assets.shaders.iter().cloned());
    if let Some(icon) = &manifest.assets.icon {
        expected.push(icon.clone());
    }
    if let Some(background) = &manifest.assets.background {
        expected.push(background.clone());
    }
    if names.len() != expected.len()
        || expected
            .iter()
            .any(|name| !names.iter().any(|actual| actual == name))
    {
        return Err("project archive entries do not match its manifest".to_string());
    }
    if manifest.assets.content.is_empty() {
        validate_asset_name(&manifest.assets.rom, ROM_PREFIX)?;
    } else {
        for name in &manifest.assets.content {
            validate_prefixed_path(name, CONTENT_PREFIX)?;
        }
        if !manifest
            .assets
            .content
            .iter()
            .any(|name| name == &manifest.assets.rom)
        {
            return Err("project content does not include its entrypoint".to_string());
        }
    }
    for name in &manifest.assets.firmware {
        validate_prefixed_path(name, FIRMWARE_PREFIX)?;
    }
    for name in &manifest.assets.shaders {
        validate_prefixed_path(name, "shaders/")?;
    }
    ensure_unique_names(&expected, "project asset")?;
    if let Some(icon) = &manifest.assets.icon {
        validate_asset_name(icon, ICON_PREFIX)?;
    }
    if let Some(background) = &manifest.assets.background {
        validate_asset_name(background, BACKGROUND_PREFIX)?;
    }
    Ok(())
}

fn validate_settings(settings: &ProjectSettings) -> Result<(), String> {
    validate_stored_settings(&StoredSettings {
        title: settings.title.clone(),
        system: settings.system.clone(),
        description: settings.description.clone(),
        show_menu: settings.show_menu,
        start_at_menu: settings.start_at_menu,
        theme: settings.theme.clone(),
        palette: settings.palette.clone(),
        menu_sounds: settings.menu_sounds.clone(),
        controls: settings.controls.clone(),
        splash: settings.splash,
        advanced_emulator_access: settings.advanced_emulator_access,
        keep_playing_in_background: settings.keep_playing_in_background,
        autosave_on_quit: settings.autosave_on_quit,
        menu_entries: settings.menu_entries.clone(),
        shaders: settings.shaders.clone(),
        achievements: settings.achievements.clone(),
        target: settings.target.clone(),
    })?;
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
    settings: &ProjectSettings,
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
    for file in &content.files {
        total = add_sized_asset(total, "game content", &file.source, MAX_ASSET_BYTES)?;
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

fn validate_stored_settings(settings: &StoredSettings) -> Result<(), String> {
    if settings.title.trim().is_empty() || settings.system.trim().is_empty() {
        return Err("project title and system are required".to_string());
    }
    if settings.start_at_menu && !settings.show_menu {
        return Err("startAtMenu requires showMenu".to_string());
    }
    if settings.theme.trim().is_empty() || settings.palette.trim().is_empty() {
        return Err("project theme and palette are required".to_string());
    }
    controls::validate_for_system(&settings.system, &settings.controls)?;
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

fn default_palette() -> String {
    "blue".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> rominabox_scratch::Scratch {
        rominabox_scratch::Scratch::dir(&format!("rominabox-project-{name}"))
    }

    #[test]
    fn project_round_trip_preserves_cue_and_tracks() {
        let root = fixture("cue-round-trip");
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("track.bin"), b"track bytes").unwrap();
        fs::write(
            source.join("disc.cue"),
            "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n",
        )
        .unwrap();
        let archive_path = root.join("game.rominabox");
        save_project(&ProjectSaveRequest {
            archive_path: archive_path.clone(),
            settings: ProjectSettings {
                rom: source.join("disc.cue"),
                title: "Disc game".to_string(),
                system: "segacd".to_string(),
                description: None,
                icon: None,
                background: None,
                show_menu: false,
                start_at_menu: false,
                theme: "native".to_string(),
                palette: "blue".to_string(),
                menu_sounds: "off".to_string(),
                controls: Controls::default(),
                firmware: Vec::new(),
                splash: false,
                advanced_emulator_access: false,
                keep_playing_in_background: false,
                autosave_on_quit: false,
                menu_entries: None,
                shaders: crate::shaders::ShaderSelection::default(),
                achievements: Default::default(),
                target: ExportTarget::Macos,
            },
        })
        .unwrap();

        let opened = open_project(&ProjectOpenRequest {
            archive_path,
            extraction_dir: root.join("opened"),
        })
        .unwrap();
        assert_eq!(
            fs::read(&opened.settings.rom).unwrap(),
            fs::read(source.join("disc.cue")).unwrap()
        );
        assert_eq!(
            fs::read(opened.settings.rom.parent().unwrap().join("track.bin")).unwrap(),
            b"track bytes"
        );
        assert!(!opened.settings.advanced_emulator_access);
    }

    #[test]
    fn project_round_trip_preserves_background_play_and_quit_autosave() {
        let root = fixture("play-settings");
        let rom = root.join("game.bin");
        fs::write(&rom, b"rom bytes").unwrap();
        let mut settings = settings(rom, false);
        settings.keep_playing_in_background = true;
        settings.autosave_on_quit = true;
        let archive_path = root.join("game.rominabox");
        save_project(&ProjectSaveRequest {
            archive_path: archive_path.clone(),
            settings,
        })
        .unwrap();
        let opened = open_project(&ProjectOpenRequest {
            archive_path,
            extraction_dir: root.join("opened"),
        })
        .unwrap();
        assert!(opened.settings.keep_playing_in_background);
        assert!(opened.settings.autosave_on_quit);
    }

    fn settings(rom: PathBuf, advanced_emulator_access: bool) -> ProjectSettings {
        ProjectSettings {
            rom,
            title: "Access game".to_string(),
            system: "megadrive".to_string(),
            description: None,
            icon: None,
            background: None,
            show_menu: false,
            start_at_menu: false,
            theme: "native".to_string(),
            palette: "blue".to_string(),
            menu_sounds: "off".to_string(),
            controls: Controls::default(),
            firmware: Vec::new(),
            splash: false,
            advanced_emulator_access,
            keep_playing_in_background: false,
            autosave_on_quit: false,
            menu_entries: None,
            shaders: crate::shaders::ShaderSelection::default(),
            achievements: Default::default(),
            target: ExportTarget::Macos,
        }
    }

    #[test]
    fn project_round_trip_preserves_advanced_emulator_access() {
        let root = fixture("advanced-access-true");
        let rom = root.join("game.bin");
        fs::write(&rom, b"rom bytes").unwrap();
        let archive_path = root.join("game.rominabox");
        save_project(&ProjectSaveRequest {
            archive_path: archive_path.clone(),
            settings: settings(rom, true),
        })
        .unwrap();

        let opened = open_project(&ProjectOpenRequest {
            archive_path,
            extraction_dir: root.join("opened"),
        })
        .unwrap();
        assert!(opened.settings.advanced_emulator_access);
        assert!(
            opened
                .settings
                .into_export_request(root.join("out"), root.join("kit"), None)
                .advanced_emulator_access
        );
    }

    #[test]
    fn legacy_project_defaults_advanced_emulator_access_off() {
        let root = fixture("legacy-advanced-access");
        let archive_path = root.join("game.rominabox");
        {
            let file = File::create(&archive_path).unwrap();
            let mut writer = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let manifest = br#"{
  "formatVersion": 1,
  "settings": {
    "title": "Legacy",
    "system": "megadrive",
    "showMenu": false,
    "startAtMenu": false,
    "theme": "native",
    "palette": "blue",
    "target": "macos"
  },
  "assets": { "rom": "assets/rom.bin" }
}"#;
            write_bytes(&mut writer, MANIFEST_PATH, manifest, options).unwrap();
            write_bytes(&mut writer, "assets/rom.bin", b"rom", options).unwrap();
            writer.finish().unwrap();
        }

        let opened = open_project(&ProjectOpenRequest {
            archive_path,
            extraction_dir: root.join("opened"),
        })
        .unwrap();
        assert!(!opened.settings.advanced_emulator_access);
        assert!(!opened.settings.keep_playing_in_background);
        assert!(!opened.settings.autosave_on_quit);
    }
}
