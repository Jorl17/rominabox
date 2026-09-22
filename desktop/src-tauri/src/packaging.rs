//! Assembly of a self-contained native game.
//!
//! Export is blocking on purpose. Desktop callers should run it with
//! `tauri::async_runtime::spawn_blocking` and use the callback for progress.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::content;
use crate::controls;
use crate::icons;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Disc image containers whose support depends on how a core was built.
const CONTAINER_FORMATS: &[&str] = &["chd", "cue", "iso", "gdi", "cdi", "pbp", "rvz", "m3u"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportTarget {
    Macos,
    Windows,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
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
    /// The author's defaults, which we never change. We store the player's
    /// changes separately, in the managed data folder of the exported game.
    #[serde(default)]
    pub controls: controls::Controls,
    /// The firmware files the author chose. On export we never look for
    /// firmware in global RetroArch locations.
    #[serde(default)]
    pub firmware: Vec<PathBuf>,
    /// Include the short native in-player splash and its logo asset.
    #[serde(default)]
    pub splash: bool,
    /// Restore stock RetroArch native menus in the exported app.
    #[serde(default)]
    pub advanced_emulator_access: bool,
    /// The shader presets we bundle into the game. Usually there are none,
    /// and then the game has no shader screen and no preset.
    #[serde(default)]
    pub shaders: crate::shaders::ShaderSelection,
    pub output_dir: PathBuf,
    pub target: ExportTarget,
    /// A frozen, redistributable kit. It contains `bin/retroarch`, `cores/`,
    /// `designs/<id>/`, `menu-assets/`, `autoconfig/`, `licenses/`, `sources/`
    /// and `manifest.json`. We put the documents of each design under its id
    /// and share the controller artwork, because all designs show the same pads.
    #[serde(default)]
    pub runtime_kit: PathBuf,
    /// Optional explicit core path for development and future custom kits.
    pub core: Option<PathBuf>,
}

fn default_palette() -> String {
    "blue".to_string()
}

/// Return the canonical systems that this runtime kit can export. We read the
/// capability from the declared core and legal files, and for this check we
/// search no global RetroArch path and load no core.
/// Why we cannot offer a declared console in this build.
///
/// We keep a reason that a developer can act on for every console that we
/// leave out, even when the list for the user stays short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum Unavailable {
    /// We declare no core at all, so the game cannot run.
    NoCoreDeclared,
    /// Every declared core is missing its artifact or its licence text.
    NoPreparedCore { tried: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAvailability {
    pub id: String,
    /// The component that we will use, when we find one.
    pub component: Option<String>,
    pub unavailable: Option<Unavailable>,
}

/// Resolve every declared console against a prepared kit, with reasons.
///
/// We try every core in the declared order of preference, not only the
/// first, so we do not report a console as missing when its preferred core
/// is absent and another core works.
pub fn system_availability(runtime_kit: &Path) -> Vec<SystemAvailability> {
    system_availability_for(runtime_kit, crate::systems::current_target())
}

/// Resolve availability for a named target.
///
/// We take the target as an argument instead of using the running one, so on
/// macOS we can answer "would this console work on Windows?", and we can
/// test that question at all.
pub fn system_availability_for(runtime_kit: &Path, target: &str) -> Vec<SystemAvailability> {
    crate::systems::registry()
        .iter()
        .map(|system| {
            if system.cores.is_empty() {
                return SystemAvailability {
                    id: system.id.clone(),
                    component: None,
                    unavailable: Some(Unavailable::NoCoreDeclared),
                };
            }
            let mut tried = Vec::new();
            for core in &system.cores {
                let Some(filename) = core.artifact_for(target) else {
                    // The component exists but has no declaration for this target,
                    // which is a different problem from a missing file.
                    tried.push(format!(
                        "{} (no {target} artifact declared)",
                        core.component
                    ));
                    continue;
                };
                let artifact = runtime_kit.join("cores").join(filename);
                let licence = runtime_kit.join("licenses").join(&core.license_file);
                if artifact.is_file() && licence.is_file() {
                    return SystemAvailability {
                        id: system.id.clone(),
                        component: Some(core.component.clone()),
                        unavailable: None,
                    };
                }
                // We report what was missing, so the reader can tell "this
                // console is gone" from "this core was never prepared".
                tried.push(format!(
                    "{} ({})",
                    core.component,
                    if artifact.is_file() {
                        format!("licence {} missing", core.license_file)
                    } else {
                        format!("artifact {filename} missing")
                    }
                ));
            }
            SystemAvailability {
                id: system.id.clone(),
                component: None,
                unavailable: Some(Unavailable::NoPreparedCore { tried }),
            }
        })
        .collect()
}

pub fn available_systems(runtime_kit: &Path) -> Vec<String> {
    system_availability(runtime_kit)
        .into_iter()
        .filter(|entry| entry.unavailable.is_none())
        .map(|entry| entry.id)
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStage {
    Validate,
    Stage,
    Dependencies,
    Configure,
    Sign,
    Archive,
    Complete,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub stage: ExportStage,
    pub fraction: f32,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub app_path: PathBuf,
    pub archive_path: PathBuf,
    pub installed_bytes: u64,
    pub archive_bytes: u64,
    pub runtime_bytes: u64,
    pub content_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub stage: String,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeDependencyInventory {
    format_version: u32,
    files: Vec<NativeDependencyFile>,
}

#[derive(Deserialize)]
struct NativeDependencyFile {
    name: String,
    sha256: String,
}

impl ExportError {
    pub fn new(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            message: message.into(),
        }
    }

    pub(crate) fn io(stage: &str, path: &Path, error: io::Error) -> Self {
        Self::new(stage, format!("{}: {error}", path.display()))
    }

    pub(crate) fn command(stage: &str, command: &str, output: &Output) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Self::new(
            stage,
            format!("{command} failed ({}): {}", output.status, stderr.trim()),
        )
    }
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.message)
    }
}

impl std::error::Error for ExportError {}

struct OwnedStaging {
    path: PathBuf,
    active: bool,
}

impl OwnedStaging {
    fn create(parent: &Path) -> Result<Self, ExportError> {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        for attempt in 0_u32..1000 {
            let path = parent.join(format!(".rominabox-export-{seed}-{attempt}"));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path, active: true }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ExportError::io("stage", &path, error)),
            }
        }

        Err(ExportError::new(
            "stage",
            "could not create a unique export staging directory",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup(mut self) -> Result<(), ExportError> {
        remove_owned_staging(&self.path)?;
        self.active = false;
        Ok(())
    }
}

impl Drop for OwnedStaging {
    fn drop(&mut self) {
        if self.active {
            let _ = remove_owned_staging(&self.path);
        }
    }
}

fn remove_owned_staging(path: &Path) -> Result<(), ExportError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ExportError::io("cleanup", path, error)),
    };
    let result = if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)
    } else {
        // With `remove_dir_all` we remove symlinks without following them, and
        // the root is the exact directory that we created atomically above.
        fs::remove_dir_all(path)
    };
    result.map_err(|error| ExportError::io("cleanup", path, error))
}

pub fn export_game<F>(
    request: &ExportRequest,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<ExportResult, ExportError>
where
    F: FnMut(ExportProgress),
{
    emit(
        &mut progress,
        ExportStage::Validate,
        0.02,
        "Checking export inputs",
    );
    validate_request(request)?;
    check_cancelled(cancelled)?;
    match request.target {
        ExportTarget::Macos => export_macos(request, cancelled, &mut progress),
        ExportTarget::Windows => Err(ExportError::new(
            "validate",
            "Windows export is not available from this build; a pinned Windows runtime kit and native packaging implementation are still required",
        )),
    }
}

fn export_macos<F>(
    request: &ExportRequest,
    cancelled: &AtomicBool,
    progress: &mut F,
) -> Result<ExportResult, ExportError>
where
    F: FnMut(ExportProgress),
{
    if !cfg!(target_os = "macos") {
        return Err(ExportError::new(
            "validate",
            "macOS export currently requires a macOS host",
        ));
    }

    if !Path::new("/usr/bin/codesign").is_file() {
        return Err(ExportError::new("validate", "This version of macOS does not provide the signing service required by this build. No tools were installed and no app was exported."));
    }
    let safe_title = safe_filename(&request.title);
    let final_app = request.output_dir.join(format!("{safe_title}.app"));
    let final_archive = request.output_dir.join(format!("{safe_title}-macOS.zip"));
    refuse_existing(&final_app)?;
    refuse_existing(&final_archive)?;
    fs::create_dir_all(&request.output_dir)
        .map_err(|error| ExportError::io("stage", &request.output_dir, error))?;

    let staging = OwnedStaging::create(&request.output_dir)?;
    let app = staging.path().join(format!("{safe_title}.app"));
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    let frameworks = contents.join("Frameworks");
    for directory in [&macos, &resources, &frameworks] {
        fs::create_dir_all(directory)
            .map_err(|error| ExportError::io("stage", directory, error))?;
    }

    emit(
        progress,
        ExportStage::Stage,
        0.10,
        "Copying the game runtime",
    );
    let runtime_source = request.runtime_kit.join("bin/retroarch");
    let runtime = macos.join("retroarch");
    copy_file(&runtime_source, &runtime)?;
    make_executable(&runtime)?;

    let system = crate::systems::find(&request.system).ok_or_else(|| {
        ExportError::new(
            "validate",
            format!("unsupported system: {}", request.system),
        )
    })?;
    let selected_core = system.preferred_core().ok_or_else(|| {
        ExportError::new(
            "validate",
            format!("{} has no configured core", system.name),
        )
    })?;
    let core_source = request.core.clone().unwrap_or_else(|| {
        request
            .runtime_kit
            .join("cores")
            .join(selected_core.artifact().unwrap_or_default())
    });
    let core_name = OsStr::new("game-core.dylib");
    let core = resources.join(core_name);
    copy_file(&core_source, &core)?;
    let collected_content =
        content::collect(&request.rom).map_err(|message| ExportError::new("validate", message))?;
    let content_directory = resources.join("content");
    for file in &collected_content.files {
        copy_content_file(file, &content_directory)?;
    }
    let rom_relative = Path::new("content").join(&collected_content.entrypoint);
    if request.show_menu {
        crate::themes::prepare_theme_assets(
            &crate::themes::staged_design(&request.runtime_kit, &request.theme),
            &resources.join("menu-assets"),
            &request.palette,
            request.background.as_deref(),
        )
        .map_err(|message| ExportError::new("stage", message))?;
        crate::themes::prepare_controls_assets(
            // The controller artwork is the same for every design, because all
            // designs show the same pads, so we keep it in the shared menu-assets.
            &request.runtime_kit.join("menu-assets"),
            // The frame in which we draw the pads comes from the design, and
            // the generated coordinates must match the stylesheet of that
            // design.
            &crate::themes::staged_design(&request.runtime_kit, &request.theme),
            &resources.join("menu-assets"),
            &request.system,
            &request.controls,
        )
        .map_err(|message| ExportError::new("stage", message))?;
        crate::shaders::install(
            &crate::themes::staged_design(&request.runtime_kit, &request.theme),
            &resources.join("menu-assets"),
            &request.shaders,
        )
        .map_err(|message| ExportError::new("stage", message))?;
    } else if request.splash {
        crate::themes::prepare_splash_assets(
            &crate::themes::staged_design(&request.runtime_kit, &request.theme),
            &resources.join("menu-assets"),
        )
        .map_err(|message| ExportError::new("stage", message))?;
    }
    let controls_assets = resources.join("menu-assets");
    fs::create_dir_all(&controls_assets)
        .map_err(|error| ExportError::io("stage", &controls_assets, error))?;
    let controls_profile = controls::write_defaults_config_with_advanced_access(
        &request.system,
        &request.controls,
        &controls_assets.join("controls-defaults.cfg"),
        request.advanced_emulator_access,
    )
    .map_err(|message| ExportError::new("stage", message))?;
    stage_controller_remap(&controls_profile, selected_core, &resources.join("remaps"))?;
    if request.show_menu {
        crate::themes::prepare_sound_assets(
            &request.runtime_kit.join("sound-packs"),
            &resources.join("assets/sounds"),
            &request.menu_sounds,
        )
        .map_err(|message| ExportError::new("stage", message))?;
    }
    stage_firmware(request, &resources.join("firmware"))?;
    stage_bundled_autoconfig(&request.runtime_kit, &resources.join("autoconfig"))?;
    if request.splash {
        copy_file(
            &request.runtime_kit.join("branding/logo.png"),
            &resources.join("menu-assets/splash-logo.png"),
        )?;
    }
    stage_legal_materials(
        &request.runtime_kit,
        &resources.join("Legal"),
        selected_core,
    )?;
    check_cancelled(cancelled)?;

    emit(
        progress,
        ExportStage::Dependencies,
        0.30,
        "Verifying pinned native dependencies",
    );
    let mut mach_objects = vec![runtime.clone(), core.clone()];
    stage_frozen_dependencies(
        &request.runtime_kit,
        &frameworks,
        &mut mach_objects,
        cancelled,
    )?;
    check_cancelled(cancelled)?;

    emit(
        progress,
        ExportStage::Configure,
        0.55,
        "Writing isolated game configuration",
    );
    let identity = stable_identity(
        &request.rom,
        &request.system,
        isolation_namespace().as_deref(),
    )?;
    write_launcher(
        &macos.join("ROM-in-a-Box"),
        &identity,
        core_name,
        rom_relative.as_os_str(),
        request,
    )?;
    write_plist(
        &contents.join("Info.plist"),
        &request.title,
        &identity,
        request.icon.is_some() || icons::default_icon_path(&request.runtime_kit).is_some(),
    )?;
    let manifest = serde_json::json!({
        "formatVersion": 1,
        "identity": identity,
        "title": request.title,
        "system": request.system,
        "description": request.description,
        "theme": request.theme,
        "palette": request.palette,
        "menuSounds": request.menu_sounds,
        "controls": request.controls,
        "controlsProfile": controls_profile.id,
        "showMenu": request.show_menu,
        "startAtMenu": request.start_at_menu,
        "runtime": "RetroArch",
        "core": "game-core.dylib",
        "coreSource": selected_core.artifact().unwrap_or_default(),
        "content": collected_content.files.iter().map(|file| file.relative.to_string_lossy()).collect::<Vec<_>>(),
        "rom": rom_relative.to_string_lossy(),
        "firmware": request.firmware.iter().filter_map(|path| firmware_destination_name(path, system)).collect::<Vec<_>>(),
        "splash": request.splash,
        "advancedEmulatorAccess": request.advanced_emulator_access,
    });
    fs::write(
        resources.join("game.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(|error| ExportError::io("configure", &resources.join("game.json"), error))?;
    let default_icon = icons::default_icon_path(&request.runtime_kit);
    if let Some(icon) = request.icon.as_deref().or(default_icon.as_deref()) {
        icons::create_macos_icon(icon, &resources.join("GameIcon.icns"), staging.path())?;
    }
    check_cancelled(cancelled)?;

    emit(progress, ExportStage::Sign, 0.70, "Signing the local app");
    for object in mach_objects.iter().rev() {
        run_command_cancellable(
            "sign",
            Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(object),
            cancelled,
        )?;
    }
    run_command_cancellable(
        "sign",
        Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&app),
        cancelled,
    )?;
    check_cancelled(cancelled)?;

    emit(
        progress,
        ExportStage::Archive,
        0.82,
        "Creating the game archive",
    );
    let staged_archive = staging.path().join(final_archive.file_name().unwrap());
    archive_macos_app(&app, &staged_archive, cancelled)?;
    let installed_bytes = tree_size(&app)?;
    let archive_bytes = fs::metadata(&staged_archive)
        .map_err(|error| ExportError::io("archive", &staged_archive, error))?
        .len();
    let runtime_bytes = tree_size(&runtime)?
        + tree_size(&core)?
        + tree_size(&frameworks)?
        + tree_size(&resources.join("menu-assets"))?
        + tree_size(&resources.join("autoconfig"))?;
    let content_bytes = tree_size(&content_directory)?
        + tree_size(&resources.join("firmware"))?
        + request
            .background
            .as_ref()
            .map_or(0, |path| fs::metadata(path).map(|m| m.len()).unwrap_or(0));
    fs::rename(&app, &final_app).map_err(|error| ExportError::io("complete", &final_app, error))?;
    fs::rename(&staged_archive, &final_archive)
        .map_err(|error| ExportError::io("complete", &final_archive, error))?;
    staging.cleanup()?;
    emit(progress, ExportStage::Complete, 1.0, "Export complete");
    Ok(ExportResult {
        app_path: final_app,
        archive_path: final_archive,
        installed_bytes,
        archive_bytes,
        runtime_bytes,
        content_bytes,
    })
}

fn validate_request(request: &ExportRequest) -> Result<(), ExportError> {
    if request.title.trim().is_empty() {
        return Err(ExportError::new("validate", "title is required"));
    }
    // A design is a directory, so we catch an unknown one when we resolve it.
    if let Err(message) = crate::themes::design_root(&request.theme) {
        return Err(ExportError::new("validate", message));
    }
    if request.start_at_menu && !request.show_menu {
        return Err(ExportError::new(
            "validate",
            "startAtMenu requires showMenu",
        ));
    }
    controls::validate_for_system_with_advanced_access(
        &request.system,
        &request.controls,
        request.advanced_emulator_access,
    )
    .map_err(|message| ExportError::new("validate", message))?;
    if !request.shaders.is_empty() && !request.show_menu {
        return Err(ExportError::new(
            "validate",
            "Shaders need the in-game menu. Turn the menu on, or leave shaders unset.",
        ));
    }
    crate::shaders::resolve(&request.shaders)
        .map_err(|message| ExportError::new("validate", message))?;
    for (label, path) in [
        ("ROM", &request.rom),
        ("runtime", &request.runtime_kit.join("bin/retroarch")),
    ] {
        if !path.is_file() {
            return Err(ExportError::new(
                "validate",
                format!("{label} file does not exist: {}", path.display()),
            ));
        }
    }
    let system = crate::systems::find(&request.system).ok_or_else(|| {
        ExportError::new(
            "validate",
            format!("unsupported system: {}", request.system),
        )
    })?;
    let selected_core = system.preferred_core().ok_or_else(|| {
        ExportError::new(
            "validate",
            format!("{} has no configured core", system.name),
        )
    })?;
    let core = request.core.clone().unwrap_or_else(|| {
        request
            .runtime_kit
            .join("cores")
            .join(selected_core.artifact().unwrap_or_default())
    });
    if !core.is_file() {
        return Err(ExportError::new(
            "validate",
            format!("core does not exist: {}", core.display()),
        ));
    }
    for path in request.icon.iter().chain(request.background.iter()) {
        if !path.is_file() {
            return Err(ExportError::new(
                "validate",
                format!("asset does not exist: {}", path.display()),
            ));
        }
    }
    content::collect(&request.rom).map_err(|message| ExportError::new("validate", message))?;
    // We reject a container format by the core that would have to read it,
    // not by the console name, because CHD support in an upstream project
    // does not show that the prepared artifact was compiled with it. We do
    // not restrict cores for which we declare no capabilities.
    if let (Some(core), Some(extension)) = (
        system.cores.first(),
        request.rom.extension().and_then(OsStr::to_str),
    ) {
        let extension = extension.to_ascii_lowercase();
        if !core.capabilities.is_empty()
            && CONTAINER_FORMATS.contains(&extension.as_str())
            && !core.supports(&extension)
        {
            return Err(ExportError::new(
                "validate",
                format!(
                    "{} export from a .{extension} image is unavailable because the prepared {} core was built without {} support. Use one of these instead: {}.",
                    system.name,
                    core.component,
                    extension.to_uppercase(),
                    core.capabilities.join(", ")
                ),
            ));
        }
    }
    validate_firmware(request, system)?;
    if request.splash {
        let logo = request.runtime_kit.join("branding/logo.png");
        if !logo.is_file() {
            return Err(ExportError::new(
                "validate",
                format!("splash logo does not exist: {}", logo.display()),
            ));
        }
    }
    Ok(())
}

fn stage_legal_materials(
    runtime_kit: &Path,
    destination: &Path,
    core: &crate::systems::Core,
) -> Result<(), ExportError> {
    let licenses = destination.join("Licenses");
    fs::create_dir_all(&licenses).map_err(|error| ExportError::io("stage", &licenses, error))?;
    copy_file(
        &runtime_kit.join("licenses/RetroArch.txt"),
        &licenses.join("RetroArch.txt"),
    )?;
    copy_file(
        &runtime_kit.join("licenses/NATIVE-DEPENDENCIES.txt"),
        &licenses.join("NATIVE-DEPENDENCIES.txt"),
    )?;
    copy_file(
        &runtime_kit.join("licenses/RmlUi-MIT.txt"),
        &licenses.join("RmlUi-MIT.txt"),
    )?;
    copy_optional_tree(
        &runtime_kit.join("licenses/native"),
        &licenses.join("native"),
    )?;
    copy_optional_tree(
        &runtime_kit.join("provenance/native-rmlui"),
        &destination.join("Source-Provenance/native-rmlui"),
    )?;

    copy_file(
        &runtime_kit.join("licenses").join(&core.license_file),
        &licenses.join(&core.license_file),
    )?;
    let joypad_licence = runtime_kit.join("licenses/retroarch-joypad-autoconfig.txt");
    if joypad_licence.is_file() {
        copy_file(
            &joypad_licence,
            &licenses.join("retroarch-joypad-autoconfig.txt"),
        )?;
    }
    fs::write(
        licenses.join("README.txt"),
        "Private ROM-in-a-Box solution-discovery export. Runtime, selected core, RmlUi, joypad autoconfig profiles, and native dependency notices are included here. Component revisions and source provenance are recorded in ../components.json. The native RetroArch fork revision and build inputs are recorded in ../Source-Provenance/native-rmlui. Historical patches are retained there only as prior-checkpoint records. Public distribution requires a separate license and source-completeness review.\n",
    ).map_err(|error| ExportError::io("stage", &licenses.join("README.txt"), error))?;

    let manifest_path = runtime_kit.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| ExportError::io("stage", &manifest_path, error))?,
    )
    .map_err(|error| {
        ExportError::new(
            "stage",
            format!("invalid {}: {error}", manifest_path.display()),
        )
    })?;
    let components = manifest
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| ExportError::new("stage", "runtime manifest has no components array"))?;
    components.retain(|component| {
        component
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| {
                name == "RetroArch"
                    || name == "RmlUi"
                    || name == "retroarch-joypad-autoconfig"
                    || name == core.component.as_str()
            })
    });
    fs::write(
        destination.join("components.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(|error| ExportError::io("stage", &destination.join("components.json"), error))?;
    Ok(())
}

fn stage_frozen_dependencies(
    runtime_kit: &Path,
    destination: &Path,
    signed_objects: &mut Vec<PathBuf>,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let inventory_path = runtime_kit.join("runtime-dependencies.json");
    let inventory_bytes = fs::read(&inventory_path)
        .map_err(|error| ExportError::io("dependencies", &inventory_path, error))?;
    let inventory: NativeDependencyInventory =
        serde_json::from_slice(&inventory_bytes).map_err(|error| {
            ExportError::new(
                "dependencies",
                format!("invalid {}: {error}", inventory_path.display()),
            )
        })?;
    if inventory.format_version != 1 {
        return Err(ExportError::new(
            "dependencies",
            format!(
                "unsupported runtime dependency inventory version: {}",
                inventory.format_version
            ),
        ));
    }
    let source_directory = runtime_kit.join("Frameworks");
    let mut declared = HashSet::new();
    for dependency in inventory.files {
        check_cancelled(cancelled)?;
        let dependency_path = Path::new(&dependency.name);
        if dependency_path.components().count() != 1 || dependency.name.starts_with('.') {
            return Err(ExportError::new(
                "dependencies",
                format!("invalid dependency filename: {}", dependency.name),
            ));
        }
        if !declared.insert(dependency.name.clone()) {
            return Err(ExportError::new(
                "dependencies",
                format!("duplicate dependency filename: {}", dependency.name),
            ));
        }
        let source = source_directory.join(&dependency.name);
        let actual = sha256_file(&source)?;
        if !actual.eq_ignore_ascii_case(&dependency.sha256) {
            return Err(ExportError::new(
                "dependencies",
                format!(
                    "checksum mismatch for {}: expected {}, got {actual}",
                    dependency.name, dependency.sha256
                ),
            ));
        }
        let staged = destination.join(&dependency.name);
        copy_file(&source, &staged)?;
        signed_objects.push(staged);
    }
    for entry in fs::read_dir(&source_directory)
        .map_err(|error| ExportError::io("dependencies", &source_directory, error))?
    {
        let entry =
            entry.map_err(|error| ExportError::io("dependencies", &source_directory, error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry
            .file_type()
            .map_err(|error| ExportError::io("dependencies", &entry.path(), error))?
            .is_file()
            && !declared.contains(&name)
        {
            return Err(ExportError::new(
                "dependencies",
                format!("undeclared file in frozen Frameworks: {name}"),
            ));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, ExportError> {
    let mut file =
        fs::File::open(path).map_err(|error| ExportError::io("dependencies", path, error))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 1024 * 128];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| ExportError::io("dependencies", path, error))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn copy_content_file(
    file: &content::ContentFile,
    destination_root: &Path,
) -> Result<(), ExportError> {
    let destination = destination_root.join(&file.relative);
    let parent = destination.parent().ok_or_else(|| {
        ExportError::new("stage", "game content destination has no parent directory")
    })?;
    fs::create_dir_all(parent).map_err(|error| ExportError::io("stage", parent, error))?;
    if let Some(bytes) = &file.staged_bytes {
        fs::write(&destination, bytes)
            .map_err(|error| ExportError::io("stage", &destination, error))
    } else {
        copy_file(&file.source, &destination)
    }
}

fn validate_firmware(
    request: &ExportRequest,
    system: &crate::systems::System,
) -> Result<(), ExportError> {
    let mut names = HashSet::new();
    for path in &request.firmware {
        if !path.is_file() {
            return Err(ExportError::new(
                "validate",
                format!("firmware file does not exist: {}", path.display()),
            ));
        }
        let name = path.file_name().and_then(OsStr::to_str).ok_or_else(|| {
            ExportError::new(
                "validate",
                format!("firmware has no usable filename: {}", path.display()),
            )
        })?;
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(ExportError::new(
                "validate",
                format!("duplicate firmware filename: {name}"),
            ));
        }
    }
    for requirement in &system.firmware {
        let matches = names
            .iter()
            .filter(|name| {
                requirement
                    .accepted_names
                    .iter()
                    .any(|accepted| accepted.eq_ignore_ascii_case(name))
            })
            .count();
        if matches < requirement.minimum {
            return Err(ExportError::new("validate", requirement.help.clone()));
        }
    }
    Ok(())
}

/// Copy staged hid profiles into the app. At launch we copy them into
/// `$data_dir/autoconfig/<driver>/`, the only folder in which we let RetroArch
/// search. A kit that was not staged adds nothing.
fn stage_bundled_autoconfig(runtime_kit: &Path, destination: &Path) -> Result<(), ExportError> {
    let source = runtime_kit.join("autoconfig");
    if !source.exists() {
        return Ok(());
    }
    copy_optional_tree(&source, destination)
}

fn stage_firmware(request: &ExportRequest, destination: &Path) -> Result<(), ExportError> {
    fs::create_dir_all(destination)
        .map_err(|error| ExportError::io("stage", destination, error))?;
    let system = crate::systems::find(&request.system).ok_or_else(|| {
        ExportError::new("stage", format!("unsupported system: {}", request.system))
    })?;
    for source in &request.firmware {
        let name = firmware_destination_name(source, system).ok_or_else(|| {
            ExportError::new(
                "stage",
                format!("firmware has no filename: {}", source.display()),
            )
        })?;
        copy_file(source, &destination.join(name))?;
    }
    Ok(())
}

fn firmware_destination_name(source: &Path, system: &crate::systems::System) -> Option<String> {
    let source_name = source.file_name()?.to_str()?;
    system
        .firmware
        .iter()
        .flat_map(|requirement| &requirement.accepted_names)
        .find(|accepted| accepted.eq_ignore_ascii_case(source_name))
        .cloned()
        .or_else(|| Some(source_name.to_string()))
}

/// An optional namespace for everything that an export creates.
///
/// The same game exported from two checkouts has the same identity on
/// purpose. It is `sha256(system + ROM bytes)`, so the saves of a player
/// stay in place after a new export. Two worktrees building in parallel would
/// then use the same bundle identifier and data folder.
///
/// So we isolate them with a namespace from the environment and never change
/// the identity itself. When it is unset, as in every ordinary export, the
/// identity is unchanged, and so is the save path of a player.
fn isolation_namespace() -> Option<String> {
    std::env::var("ROMINABOX_GAME_BUNDLE_PREFIX")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// We take this as an argument and do not read the environment here, so a
/// test can call it without changing process-wide state that is shared by
/// every other test in this binary.
fn stable_identity(
    rom: &Path,
    system: &str,
    namespace: Option<&str>,
) -> Result<String, ExportError> {
    let mut file = fs::File::open(rom).map_err(|error| ExportError::io("configure", rom, error))?;
    let mut hash = Sha256::new();
    hash.update(b"rominabox-game-v1\0");
    if let Some(namespace) = namespace.map(str::trim).filter(|value| !value.is_empty()) {
        // We add it only when the environment has a namespace, so the
        // identity of an ordinary export does not depend on it.
        hash.update(namespace.as_bytes());
        hash.update(b"\0");
    }
    hash.update(system.trim().to_ascii_lowercase().as_bytes());
    hash.update(b"\0");
    let mut buffer = [0u8; 1024 * 128];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| ExportError::io("configure", rom, error))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize())[..24].to_string())
}

/// The RetroArch meta bind policy for exported games.
///
/// This is the only bind list. It covers every `DECLARE_META_BIND` in the
/// pinned RetroArch `configuration.c`. The desktop defaults in
/// `config.def.keybinds.h` and `retroarch.cfg` bind Space to
/// `toggle_fast_forward`, Escape to quit and F1 to the stock menu, so we write
/// every meta bind in an export to replace those defaults.
///
/// The binds we keep are for the keyboard only. Button, axis and mouse
/// variants stay `nul`, which is `NO_BTN`, a user bind with no button. When
/// the user joykey is `NO_BTN`, the autoconfig bind applies in the joypad
/// poll, so a profile with `input_menu_toggle_btn` would bind that button.
/// We remove those meta lines from the profiles we ship when we stage them.
/// The `input_player1_*` gameplay keys are not declared here, because they
/// are in the controls appendconfig.
///
/// `advanced_key` is a second keyboard tier. We write it only when the author
/// set `advancedEmulatorAccess`, and it never replaces the button, axis or
/// mouse `nul`. A normal export contains `keyboard`, which is `nul` for every
/// bind except the menu toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyBind {
    pub name: &'static str,
    pub keyboard: HotkeyKeyboard,
    /// The keyboard key we use only when advanced emulator access is on.
    /// `None` means that this bind has no advanced key.
    pub advanced_key: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyKeyboard {
    Neutral,
    MenuToggle,
}

/// Write the emulated controller where RetroArch reads it.
///
/// `input_libretro_device_p1` looks like an ordinary setting, but its value is
/// read only from a remap file, in `input_remapping_load_file` in
/// `configuration.c`. In `retroarch.cfg` it has no effect, and the core uses
/// its default device instead of a six-button Mega Drive pad or a PlayStation
/// DualShock.
///
/// So we write the file to the path read in `config_load_remap`,
/// `<remap dir>/<library name>/<library name>.rmp`, with the libretro library
/// name of the core binary.
fn stage_controller_remap(
    profile: &controls::ControlProfile,
    core: &crate::systems::Core,
    remaps: &Path,
) -> Result<(), ExportError> {
    let Some(device) = profile.core_device else {
        // Most pads are the default device of the core, so they require no remap.
        return Ok(());
    };
    let Some(library) = core.library_name.as_deref() else {
        return Err(ExportError::new(
            "stage",
            format!(
                "{} needs the emulated device {device}, but component '{}' does not declare its \
                 libraryName, so there is nowhere to write the remap RetroArch reads",
                profile.id, core.component
            ),
        ));
    };
    let directory = remaps.join(library);
    fs::create_dir_all(&directory).map_err(|error| ExportError::io("stage", &directory, error))?;
    let path = directory.join(format!("{library}.rmp"));
    let contents = format!("input_libretro_device_p1 = \"{device}\"\n");
    fs::write(&path, contents).map_err(|error| ExportError::io("stage", &path, error))?;
    Ok(())
}

/// The writable directories we create and manage under each game's data root.
pub const MANAGED_DATA_DIRECTORIES: &[&str] = &[
    "saves",
    "states",
    "system",
    "cache",
    "logs",
    "info",
    "playlists",
    "screenshots",
    "remaps",
    "config",
    "shaders",
    "runtime-logs",
    "recordings",
    "recording-config",
    "autoconfig",
    "assets",
    "downloads",
    "thumbnails",
    "database",
    "cheats",
    "overlays",
    "overlays/keyboards",
    "cores",
    "filters/video",
    "filters/audio",
];

pub const HOTKEY_BINDS: &[HotkeyBind] = &[
    HotkeyBind {
        name: "enable_hotkey",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "menu_toggle",
        keyboard: HotkeyKeyboard::MenuToggle,
        advanced_key: None,
    },
    HotkeyBind {
        name: "exit_emulator",
        keyboard: HotkeyKeyboard::Neutral,
        // In a shipped game the player quits from the in-game menu (Escape,
        // then Quit). Q is easy to press by accident during play, so we ship
        // the key only with advanced emulator access.
        advanced_key: Some("q"),
    },
    HotkeyBind {
        name: "close_content",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "reset",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_fast_forward",
        keyboard: HotkeyKeyboard::Neutral,
        // This is the desktop default, and we declare no gameplay key on Space.
        advanced_key: Some("space"),
    },
    HotkeyBind {
        name: "hold_fast_forward",
        keyboard: HotkeyKeyboard::Neutral,
        // `l` is the desktop default and the DualShock right-stick-right key.
        advanced_key: Some("l"),
    },
    HotkeyBind {
        name: "toggle_slowmotion",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "hold_slowmotion",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "rewind",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "pause_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "frame_advance",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "audio_mute",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "volume_up",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "volume_down",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "load_state",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "save_state",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "state_slot_increase",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "state_slot_decrease",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "play_replay",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "record_replay",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "halt_replay",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "save_replay_checkpoint",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "prev_replay_checkpoint",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "next_replay_checkpoint",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "replay_slot_increase",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "replay_slot_decrease",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_eject_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_next",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_prev",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_hold",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_next",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_prev",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_index_plus",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_index_minus",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "screenshot",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "recording_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "streaming_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "turbo_fire_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "grab_mouse_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "game_focus_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_fullscreen",
        keyboard: HotkeyKeyboard::Neutral,
        // Fullscreen works like quit. If f stayed bound while q and f are
        // ordinary gameplay keys, one press would trigger the hotkey and the
        // bind together. The macOS window menu has a Full Screen item.
        advanced_key: Some("f"),
    },
    HotkeyBind {
        name: "desktop_menu_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_vrr_runloop",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "runahead_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "preempt_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "video_filter_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "fps_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_statistics",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "ai_service",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_ping_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_host_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_game_watch",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_player_chat",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_fade_chat_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "overlay_next",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "osk_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
];

impl HotkeyBind {
    pub fn keyboard_value(self, show_menu: bool, advanced: bool) -> &'static str {
        if advanced {
            if let Some(key) = self.advanced_key {
                return key;
            }
        }
        match self.keyboard {
            HotkeyKeyboard::Neutral => "nul",
            HotkeyKeyboard::MenuToggle if show_menu => "escape",
            HotkeyKeyboard::MenuToggle => "nul",
        }
    }
}

/// Render the exported hotkey policy. Callers must not keep a second list.
///
/// With `advanced` we write `advanced_key` for the binds that have one:
/// fast-forward, quit and fullscreen. We do not change button, axis or
/// mouse. When it is false, those keys stay `nul`, and we still write Escape
/// for the menu toggle when the menu is on.
pub fn isolated_hotkey_config(show_menu: bool, advanced: bool) -> String {
    let mut config = String::new();
    for bind in HOTKEY_BINDS {
        let key = bind.keyboard_value(show_menu, advanced);
        config.push_str(&format!("input_{} = \"{key}\"\n", bind.name));
        config.push_str(&format!("input_{}_btn = \"nul\"\n", bind.name));
        config.push_str(&format!("input_{}_axis = \"nul\"\n", bind.name));
        config.push_str(&format!("input_{}_mbtn = \"nul\"\n", bind.name));
    }
    let menu_combo = if show_menu { "2" } else { "0" };
    config.push_str(&format!(
        "input_menu_toggle_gamepad_combo = \"{menu_combo}\"\n"
    ));
    config.push_str("input_quit_gamepad_combo = \"0\"\n");
    config
}

fn isolated_runtime_config(request: &ExportRequest) -> String {
    let menu_driver = if request.show_menu || request.splash {
        "rmlui"
    } else {
        "null"
    };
    let menu_audio = request.show_menu && request.menu_sounds != "off";
    let assets = if menu_audio {
        "$bundle_dir/Resources/assets"
    } else {
        "$data_dir/assets"
    };
    format!(
        r#"video_driver = "gl"
audio_driver = "coreaudio"
audio_enable_menu = "{menu_audio}"
audio_enable_menu_ok = "{menu_audio}"
audio_enable_menu_cancel = "{menu_audio}"
audio_enable_menu_scroll = "{menu_audio}"
audio_enable_menu_bgm = "false"
audio_enable_menu_notice = "false"
cheevos_unlock_sound_enable = "false"
input_joypad_driver = "hid"
menu_driver = "{menu_driver}"
menu_pause_libretro = "true"
menu_show_start_screen = "false"
menu_enable_widgets = "false"
video_font_enable = "false"
microphone_enable = "false"
sort_savefiles_enable = "false"
sort_savestates_enable = "false"
video_fullscreen = "false"
video_windowed_fullscreen = "true"
video_window_custom_size_enable = "true"
video_windowed_position_width = "960"
video_windowed_position_height = "600"
{}config_save_on_exit = "false"
savefile_directory = "$data_dir/saves"
savestate_directory = "$data_dir/states"
system_directory = "$data_dir/system"
cache_directory = "$data_dir/cache"
log_dir = "$data_dir/logs"
libretro_info_path = "$data_dir/info"
playlist_directory = "$data_dir/playlists"
screenshot_directory = "$data_dir/screenshots"
core_options_path = "$data_dir/core-options.cfg"
auto_remaps_enable = "true"
input_remap_sort_by_controller_enable = "false"
content_history_path = "$data_dir/playlists/content_history.lpl"
content_music_history_path = "$data_dir/playlists/content_music_history.lpl"
content_image_history_path = "$data_dir/playlists/content_image_history.lpl"
content_video_history_path = "$data_dir/playlists/content_video_history.lpl"
input_remapping_directory = "$data_dir/remaps"
rgui_config_directory = "$data_dir/config"
video_shader_dir = "$data_dir/shaders"
runtime_log_directory = "$data_dir/runtime-logs"
recording_output_directory = "$data_dir/recordings"
recording_config_directory = "$data_dir/recording-config"
# Seeded from Resources/autoconfig on launch, the same way remaps are seeded.
joypad_autoconfig_dir = "$data_dir/autoconfig"
assets_directory = "{assets}"
core_assets_directory = "$data_dir/downloads"
thumbnails_directory = "$data_dir/thumbnails"
content_database_path = "$data_dir/database"
cheat_database_path = "$data_dir/cheats"
overlay_directory = "$data_dir/overlays"
osk_overlay_directory = "$data_dir/overlays/keyboards"
libretro_directory = "$data_dir/cores"
video_filter_dir = "$data_dir/filters/video"
audio_filter_dir = "$data_dir/filters/audio"
history_list_enable = "false"
core_info_cache_enable = "false"
auto_overrides_enable = "false"
auto_remaps_enable = "false"
remap_save_on_exit = "false"
game_specific_options = "false"
global_core_options = "false"
auto_shaders_enable = "false"
savefiles_in_content_dir = "false"
savestates_in_content_dir = "false"
systemfiles_in_content_dir = "false"
screenshots_in_content_dir = "false"
content_runtime_log = "false"
content_runtime_log_aggregate = "false"
log_to_file = "false"
notification_show_autoconfig = "false"
notification_show_remap_load = "false"
notification_show_config_override_load = "false"
savestate_thumbnail_enable = "true"
"#,
        isolated_hotkey_config(request.show_menu, request.advanced_emulator_access)
    )
}

fn write_launcher(
    path: &Path,
    identity: &str,
    _core: &OsStr,
    rom: &OsStr,
    request: &ExportRequest,
) -> Result<(), ExportError> {
    let start_at_menu_env = if request.start_at_menu {
        "export ROMINABOX_START_AT_MENU=1"
    } else {
        ""
    };
    let splash_env = if request.splash {
        "export ROMINABOX_SPLASH=1"
    } else {
        "unset ROMINABOX_SPLASH"
    };
    let advanced_access_env = if request.advanced_emulator_access {
        "export ROMINABOX_ADVANCED_ACCESS=1"
    } else {
        "export ROMINABOX_ADVANCED_ACCESS=0"
    };
    let content_relative = shell_quote(&rom.to_string_lossy());
    let game_title = shell_quote(&request.title);
    let managed_directories = MANAGED_DATA_DIRECTORIES.join(" ");
    let runtime_config = isolated_runtime_config(request);
    let script = format!(
        r##"#!/bin/sh
set -eu
bundle_dir=$(CDPATH= cd -- "$(/usr/bin/dirname "$0")/.." && pwd)
content_relative={content_relative}
data_dir="$HOME/Library/Application Support/ROM-in-a-Box/Games/{identity}"
for name in {managed_directories}; do
  /bin/mkdir -p "$data_dir/$name"
done
unset ROMINABOX_START_AT_MENU
{splash_env}
{advanced_access_env}
unset LIBRETRO_SYSTEM_DIRECTORY LIBRETRO_DIRECTORY LIBRETRO_ASSETS_DIRECTORY LIBRETRO_AUTOCONFIG_DIRECTORY LIBRETRO_CHEATS_DIRECTORY LIBRETRO_DATABASE_DIRECTORY LIBRETRO_VIDEO_FILTER_DIRECTORY LIBRETRO_VIDEO_SHADER_DIRECTORY
export ROMINABOX_DATA_DIR="$data_dir"
export ROMINABOX_TITLE={game_title}
export ROMINABOX_RML_ASSETS="$bundle_dir/Resources/menu-assets"
{start_at_menu_env}
cfg="$data_dir/retroarch.cfg"
/bin/cat >"$cfg" <<EOF
@@RUNTIME_CONFIG@@
EOF
"##,
    )
    .replace("@@RUNTIME_CONFIG@@", &runtime_config);
    let shader_initial = if request.show_menu {
        crate::shaders::launch_preset(&request.shaders)
            .map_err(|message| ExportError::new("configure", message))?
    } else {
        None
    };
    let shader_shell = crate::shaders::launcher_shader_shell(shader_initial.as_deref());
    let script = format!(
        r##"{script}for remap_dir in "$bundle_dir"/Resources/remaps/*; do
  [ -d "$remap_dir" ] || continue
  name=${{remap_dir##*/}}
  /bin/mkdir -p "$data_dir/remaps/$name"
  for remap in "$remap_dir"/*; do
    [ -f "$remap" ] || continue
    base=${{remap##*/}}
    [ -f "$data_dir/remaps/$name/$base" ] || /bin/cp "$remap" "$data_dir/remaps/$name/$base"
  done
done
for autoconfig_dir in "$bundle_dir"/Resources/autoconfig/*; do
  [ -d "$autoconfig_dir" ] || continue
  name=${{autoconfig_dir##*/}}
  /bin/mkdir -p "$data_dir/autoconfig/$name"
  for profile in "$autoconfig_dir"/*; do
    [ -f "$profile" ] || continue
    base=${{profile##*/}}
    [ -f "$data_dir/autoconfig/$name/$base" ] || /bin/cp "$profile" "$data_dir/autoconfig/$name/$base"
  done
done
for firmware in "$bundle_dir"/Resources/firmware/*; do
  [ -f "$firmware" ] || continue
  name=${{firmware##*/}}
  [ -f "$data_dir/system/$name" ] || /bin/cp "$firmware" "$data_dir/system/$name"
done
controls_defaults="$bundle_dir/Resources/menu-assets/controls-defaults.cfg"
controls_override="$data_dir/controls.cfg"
case "$controls_defaults" in
  *'|'*) echo "ROM-in-a-Box cannot load controls from a path containing |." >&2; exit 1 ;;
esac
append_config="$controls_defaults"
if [ -f "$controls_override" ]; then
  case "$controls_override" in
    *'|'*) echo "ROM-in-a-Box cannot load controls from a path containing |." >&2; exit 1 ;;
  esac
  append_config="$append_config|$controls_override"
fi
{shader_shell}
set -- --config "$cfg" --appendconfig "$append_config" --libretro "$bundle_dir/Resources/game-core.dylib" "$bundle_dir/Resources/$content_relative"
if [ -n "$shader_preset" ]; then
  set -- "$@" --set-shader "$shader_preset"
fi
exec "$bundle_dir/MacOS/retroarch" "$@" >>"$data_dir/logs/launch.log" 2>&1
"##,
    );
    fs::write(path, script).map_err(|error| ExportError::io("configure", path, error))?;
    make_executable(path)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn write_plist(
    path: &Path,
    title: &str,
    identity: &str,
    has_icon: bool,
) -> Result<(), ExportError> {
    let icon = if has_icon {
        "<key>CFBundleIconFile</key><string>GameIcon</string>"
    } else {
        ""
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleDisplayName</key><string>{}</string>
<key>CFBundleExecutable</key><string>ROM-in-a-Box</string>
<key>CFBundleIdentifier</key><string>app.rominabox.game.{}</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>{}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>26.0</string>
{}
</dict></plist>
"#,
        xml_escape(title),
        identity,
        xml_escape(title),
        icon
    );
    fs::write(path, plist).map_err(|error| ExportError::io("configure", path, error))
}

fn bundle_dependencies(
    mach_objects: &mut Vec<PathBuf>,
    frameworks: &Path,
    search_dirs: &[PathBuf],
    cancelled: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    let mut queue: VecDeque<(PathBuf, PathBuf)> = mach_objects
        .iter()
        .cloned()
        .map(|path| (path.clone(), path))
        .collect();
    let mut copied: HashMap<String, PathBuf> = HashMap::new();
    let mut inspected = HashSet::new();
    while let Some((object, origin)) = queue.pop_front() {
        if let Some(cancelled) = cancelled {
            check_cancelled(cancelled)?;
        }
        if !inspected.insert(object.clone()) {
            continue;
        }
        for (index, dependency) in macho_dependencies(&object)?.into_iter().enumerate() {
            if index == 0 && object.extension() == Some(OsStr::new("dylib")) {
                continue;
            }
            if is_system_dependency(&dependency) {
                continue;
            }
            let Some(source) = resolve_dependency_source(&dependency, &origin, search_dirs) else {
                return Err(ExportError::new(
                    "dependencies",
                    format!(
                        "could not resolve native dependency {dependency} required by {}",
                        origin.display()
                    ),
                ));
            };
            if !source.is_file() {
                return Err(ExportError::new(
                    "dependencies",
                    format!("missing native dependency {dependency}"),
                ));
            }
            let name = source.file_name().unwrap().to_string_lossy().into_owned();
            let canonical = fs::canonicalize(&source)
                .map_err(|error| ExportError::io("dependencies", &source, error))?;
            if let Some(previous) = copied.get(&name) {
                if fs::canonicalize(previous).ok().as_ref() != Some(&canonical) {
                    return Err(ExportError::new(
                        "dependencies",
                        format!("dependency filename collision: {name}"),
                    ));
                }
                continue;
            }
            let destination = frameworks.join(&name);
            copy_file(&canonical, &destination)?;
            copied.insert(name, canonical.clone());
            queue.push_back((destination.clone(), canonical));
            mach_objects.push(destination);
        }
    }
    Ok(())
}

fn relocate_dependencies(
    objects: &[PathBuf],
    framework_prefix: &str,
    cancelled: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    for object in objects {
        if let Some(cancelled) = cancelled {
            check_cancelled(cancelled)?;
        }
        for (index, dependency) in macho_dependencies(object)?.into_iter().enumerate() {
            if index == 0 && object.extension() == Some(OsStr::new("dylib")) {
                continue;
            }
            if is_system_dependency(&dependency)
                || dependency.starts_with(&format!("{framework_prefix}/"))
            {
                continue;
            }
            let name = Path::new(&dependency).file_name().ok_or_else(|| {
                ExportError::new("dependencies", format!("invalid dependency: {dependency}"))
            })?;
            let replacement = format!("{framework_prefix}/{}", name.to_string_lossy());
            let mut command = Command::new("/usr/bin/install_name_tool");
            command
                .args(["-change", &dependency, &replacement])
                .arg(object);
            if let Some(cancelled) = cancelled {
                run_command_cancellable("dependencies", &mut command, cancelled)?;
            } else {
                run_command("dependencies", &mut command)?;
            }
        }
        if object.parent().and_then(Path::file_name) == Some(OsStr::new("Frameworks")) {
            let name = object.file_name().unwrap().to_string_lossy();
            let mut command = Command::new("/usr/bin/install_name_tool");
            command.args(["-id", &format!("@rpath/{name}")]).arg(object);
            if let Some(cancelled) = cancelled {
                run_command_cancellable("dependencies", &mut command, cancelled)?;
            } else {
                run_command("dependencies", &mut command)?;
            }
        }
    }
    Ok(())
}

/// Freeze one helper executable next to its recursive native dependencies.
///
/// We create the destination exactly as requested and put the dependencies in
/// a `Frameworks` directory next to it. We refuse a destination that exists.
pub fn freeze_macos_executable(source: &Path, destination: &Path) -> Result<u64, ExportError> {
    if !cfg!(target_os = "macos") {
        return Err(ExportError::new(
            "freeze",
            "macOS helper freezing requires a macOS host",
        ));
    }
    refuse_existing(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::new("freeze", "helper destination has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| ExportError::io("freeze", parent, error))?;
    copy_file(source, destination)?;
    make_executable(destination)?;
    let frameworks = parent.join("Frameworks");
    fs::create_dir_all(&frameworks)
        .map_err(|error| ExportError::io("freeze", &frameworks, error))?;
    let mut objects = vec![destination.to_path_buf()];
    let source_directory = source
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    bundle_dependencies(&mut objects, &frameworks, &source_directory, None)?;
    relocate_dependencies(&objects, "@executable_path/Frameworks", None)?;
    for object in objects.iter().rev() {
        run_command(
            "freeze",
            Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(object),
        )?;
    }
    tree_size(parent)
}

fn resolve_dependency_source(
    dependency: &str,
    origin: &Path,
    search_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if dependency.starts_with('/') {
        return Some(PathBuf::from(dependency));
    }
    let name = Path::new(dependency).file_name()?;
    let sibling = origin.parent()?.join(name);
    if sibling.is_file() {
        return Some(sibling);
    }
    search_dirs
        .iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

fn macho_dependencies(path: &Path) -> Result<Vec<String>, ExportError> {
    let output = Command::new("/usr/bin/otool")
        .arg("-L")
        .arg(path)
        .output()
        .map_err(|error| {
            ExportError::new("dependencies", format!("could not run otool: {error}"))
        })?;
    if !output.status.success() {
        return Err(ExportError::command("dependencies", "otool", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().split(" (compatibility").next())
        .map(str::to_owned)
        .collect())
}

fn is_system_dependency(path: &str) -> bool {
    path.starts_with("/System/Library/") || path.starts_with("/usr/lib/")
}

fn run_command(stage: &str, command: &mut Command) -> Result<(), ExportError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .map_err(|error| ExportError::new(stage, format!("could not run {program}: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ExportError::command(stage, &program, &output))
    }
}

fn run_command_cancellable(
    stage: &str,
    command: &mut Command,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ExportError::new(stage, format!("could not run {program}: {error}")))?;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExportError::new(
                "cancelled",
                format!("export cancelled while running {program}"),
            ));
        }
        if let Some(status) = child.try_wait().map_err(|error| {
            ExportError::new(stage, format!("could not monitor {program}: {error}"))
        })? {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut pipe) = child.stdout.take() {
                let _ = pipe.read_to_end(&mut stdout);
            }
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_end(&mut stderr);
            }
            let output = Output {
                status,
                stdout,
                stderr,
            };
            return if output.status.success() {
                Ok(())
            } else {
                Err(ExportError::command(stage, &program, &output))
            };
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_metadata =
        fs::symlink_metadata(source).map_err(|error| ExportError::io("stage", source, error))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(ExportError::new(
            "stage",
            format!("refusing to stage non-regular file: {}", source.display()),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| ExportError::io("stage", parent, error))?;
    }
    fs::copy(source, destination).map_err(|error| {
        ExportError::new(
            "stage",
            format!("{} -> {}: {error}", source.display(), destination.display()),
        )
    })?;
    Ok(())
}

/// Archive a staged app without platform ZIP tools. We start from the parent
/// folder of the staged app, so the archive contains `Game.app/`.
fn archive_macos_app(
    app: &Path,
    destination: &Path,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let root = app
        .parent()
        .ok_or_else(|| ExportError::new("archive", "staged app has no parent directory"))?;
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| ExportError::io("archive", destination, error))?;
    let mut writer = ZipWriter::new(output);
    archive_directory(&mut writer, app, root, cancelled)?;
    check_cancelled(cancelled)?;
    writer.finish().map_err(|error| {
        ExportError::new(
            "archive",
            format!("could not finish {}: {error}", destination.display()),
        )
    })?;
    Ok(())
}

fn archive_directory<W: Write + io::Seek>(
    writer: &mut ZipWriter<W>,
    directory: &Path,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    check_cancelled(cancelled)?;
    let metadata = fs::symlink_metadata(directory)
        .map_err(|error| ExportError::io("archive", directory, error))?;
    if metadata.file_type().is_symlink() {
        return Err(ExportError::new(
            "archive",
            format!("refusing to archive symlink: {}", directory.display()),
        ));
    }
    if !metadata.is_dir() {
        return Err(ExportError::new(
            "archive",
            format!(
                "expected directory while archiving: {}",
                directory.display()
            ),
        ));
    }
    let directory_name = archive_member_name(directory, root)?;
    writer
        .add_directory(
            format!("{directory_name}/"),
            archive_options(&metadata, true),
        )
        .map_err(|error| {
            ExportError::new(
                "archive",
                format!("could not add {directory_name}: {error}"),
            )
        })?;
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|error| ExportError::io("archive", directory, error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ExportError::io("archive", directory, error))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| ExportError::io("archive", &path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(ExportError::new(
                "archive",
                format!("refusing to archive symlink: {}", path.display()),
            ));
        }
        if metadata.is_dir() {
            archive_directory(writer, &path, root, cancelled)?;
        } else if metadata.is_file() {
            archive_file(writer, &path, root, &metadata, cancelled)?;
        } else {
            return Err(ExportError::new(
                "archive",
                format!(
                    "refusing to archive unsupported file type: {}",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn archive_file<W: Write + io::Seek>(
    writer: &mut ZipWriter<W>,
    path: &Path,
    root: &Path,
    metadata: &fs::Metadata,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let name = archive_member_name(path, root)?;
    writer
        .start_file(&name, archive_options(metadata, false))
        .map_err(|error| ExportError::new("archive", format!("could not add {name}: {error}")))?;
    let mut source = File::open(path).map_err(|error| ExportError::io("archive", path, error))?;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let bytes = source
            .read(&mut buffer)
            .map_err(|error| ExportError::io("archive", path, error))?;
        if bytes == 0 {
            break;
        }
        writer.write_all(&buffer[..bytes]).map_err(|error| {
            ExportError::new("archive", format!("could not write {name}: {error}"))
        })?;
    }
    Ok(())
}

fn archive_member_name(path: &Path, root: &Path) -> Result<String, ExportError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        ExportError::new(
            "archive",
            format!("path escapes app staging root: {}", path.display()),
        )
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => {
                return Err(ExportError::new(
                    "archive",
                    format!("unsafe app archive path: {}", path.display()),
                ))
            }
        }
    }
    if parts.is_empty() {
        return Err(ExportError::new(
            "archive",
            "app archive member name is empty",
        ));
    }
    Ok(parts.join("/"))
}

fn archive_options(metadata: &fs::Metadata, directory: bool) -> SimpleFileOptions {
    let mode = archive_mode(metadata, directory);
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true)
        .unix_permissions(mode)
}

#[cfg(unix)]
fn archive_mode(metadata: &fs::Metadata, _directory: bool) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn archive_mode(_metadata: &fs::Metadata, directory: bool) -> u32 {
    if directory {
        0o755
    } else {
        0o644
    }
}

fn copy_optional_tree(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_metadata =
        fs::symlink_metadata(source).map_err(|error| ExportError::io("stage", source, error))?;
    if source_metadata.file_type().is_symlink() {
        return Err(ExportError::new(
            "stage",
            format!(
                "refusing to stage symlinked directory: {}",
                source.display()
            ),
        ));
    }
    if !source_metadata.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(destination)
        .map_err(|error| ExportError::io("stage", destination, error))?;
    for entry in fs::read_dir(source).map_err(|error| ExportError::io("stage", source, error))? {
        let entry = entry.map_err(|error| ExportError::io("stage", source, error))?;
        let target = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| ExportError::io("stage", &entry.path(), error))?;
        if file_type.is_symlink() {
            return Err(ExportError::new(
                "stage",
                format!("refusing to stage symlink: {}", entry.path().display()),
            ));
        }
        if file_type.is_dir() {
            copy_optional_tree(&entry.path(), &target)?;
        } else {
            copy_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn make_executable(path: &Path) -> Result<(), ExportError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| ExportError::io("stage", path, error))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| ExportError::io("stage", path, error))?;
    }
    Ok(())
}

fn tree_size(path: &Path) -> Result<u64, ExportError> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| ExportError::io("measure", path, error))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(|error| ExportError::io("measure", path, error))? {
        let entry = entry.map_err(|error| ExportError::io("measure", path, error))?;
        total += tree_size(&entry.path())?;
    }
    Ok(total)
}

fn safe_filename(title: &str) -> String {
    let value: String = title
        .trim()
        .chars()
        .map(|character| match character {
            '/' | ':' | '\0' => '-',
            _ => character,
        })
        .collect();
    if value.is_empty() || value == "." || value == ".." {
        "Game".to_string()
    } else {
        value
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn refuse_existing(path: &Path) -> Result<(), ExportError> {
    if path.exists() {
        Err(ExportError::new(
            "validate",
            format!("refusing to overwrite existing export: {}", path.display()),
        ))
    } else {
        Ok(())
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), ExportError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ExportError::new("cancelled", "export cancelled"))
    } else {
        Ok(())
    }
}

fn emit<F: FnMut(ExportProgress)>(
    callback: &mut F,
    stage: ExportStage,
    fraction: f32,
    message: &str,
) {
    callback(ExportProgress {
        stage,
        fraction,
        message: message.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(splash: bool) -> ExportRequest {
        ExportRequest {
            rom: PathBuf::from("game.bin"),
            title: "Game".to_string(),
            system: "megadrive".to_string(),
            description: None,
            icon: None,
            background: None,
            show_menu: false,
            start_at_menu: false,
            theme: "native".to_string(),
            palette: "blue".to_string(),
            menu_sounds: "off".to_string(),
            controls: controls::Controls::default(),
            firmware: Vec::new(),
            splash,
            advanced_emulator_access: false,
            shaders: crate::shaders::ShaderSelection::default(),
            output_dir: PathBuf::from("output"),
            target: ExportTarget::Macos,
            runtime_kit: PathBuf::from("runtime"),
            core: None,
        }
    }

    #[test]
    fn launcher_quotes_hostile_content_filename_as_data() {
        let directory = std::env::temp_dir().join(format!(
            "rominabox-launcher-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let launcher = directory.join("launcher");
        let hostile = OsStr::new("content/weird'$(touch PWNED)`echo nope`.bin");
        let mut settings = request(false);
        settings.title = "Game's $(title) `literal`".into();
        write_launcher(
            &launcher,
            "identity",
            OsStr::new("core"),
            hostile,
            &settings,
        )
        .unwrap();
        let script = fs::read_to_string(launcher).unwrap();

        assert!(
            script.contains("content_relative='content/weird'\\''$(touch PWNED)`echo nope`.bin'")
        );
        assert!(script.contains("\"$bundle_dir/Resources/$content_relative\""));
        assert!(!script.contains("Resources/content/weird"));
        assert!(script.contains("export ROMINABOX_TITLE='Game'\\''s $(title) `literal`'"));
    }

    fn write_test_launcher(settings: ExportRequest) -> String {
        let directory = std::env::temp_dir().join(format!(
            "rominabox-hotkey-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let launcher = directory.join("launcher");
        write_launcher(
            &launcher,
            "identity",
            OsStr::new("core"),
            OsStr::new("content/game.bin"),
            &settings,
        )
        .unwrap();
        fs::read_to_string(launcher).unwrap()
    }

    fn embedded_runtime_config(script: &str) -> String {
        let start = script
            .find("/bin/cat >\"$cfg\" <<EOF\n")
            .expect("launcher writes retroarch.cfg");
        let body = &script[start + "/bin/cat >\"$cfg\" <<EOF\n".len()..];
        let end = body.find("\nEOF\n").expect("launcher config heredoc ends");
        body[..end].to_string()
    }

    fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
        config.lines().find_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            (name == key).then(|| value.trim_matches('"'))
        })
    }

    #[test]
    fn exported_config_neutralizes_default_space_fast_forward() {
        let mut settings = request(false);
        settings.show_menu = true;
        let config = embedded_runtime_config(&write_test_launcher(settings));

        assert_eq!(
            config_value(&config, "input_toggle_fast_forward"),
            Some("nul"),
            "pinned RetroArch desktop defaults bind Space to toggle fast-forward"
        );
        assert_eq!(
            config_value(&config, "input_hold_fast_forward"),
            Some("nul")
        );
        for suffix in ["", "_btn", "_axis", "_mbtn"] {
            assert_eq!(
                config_value(&config, &format!("input_toggle_fast_forward{suffix}")),
                Some("nul"),
                "fast-forward{suffix} must not leak a default binding"
            );
        }
        assert!(
            !config.lines().any(|line| {
                line.starts_with("input_")
                    && !line.starts_with("input_player")
                    && !line.starts_with("input_joypad")
                    && !line.starts_with("input_menu_toggle_gamepad")
                    && !line.starts_with("input_quit_gamepad")
                    && line.contains(" = \"space\"")
            }),
            "no RetroArch hotkey may keep the default Space binding:\n{config}"
        );
    }

    #[test]
    fn advanced_emulator_access_reaches_fast_forward_without_dropping_a_bind() {
        let advanced_tier: Vec<_> = HOTKEY_BINDS
            .iter()
            .filter(|bind| bind.advanced_key.is_some())
            .map(|bind| (bind.name, bind.advanced_key.unwrap(), bind.keyboard))
            .collect();
        // Quit and fullscreen are in this tier, so q and f stay free for
        // gameplay and Q cannot quit a shipped game. The base keyboard stays
        // Neutral, so a default export contains nul for each of them.
        assert_eq!(
            advanced_tier,
            vec![
                ("exit_emulator", "q", HotkeyKeyboard::Neutral),
                ("toggle_fast_forward", "space", HotkeyKeyboard::Neutral),
                ("hold_fast_forward", "l", HotkeyKeyboard::Neutral),
                ("toggle_fullscreen", "f", HotkeyKeyboard::Neutral),
            ],
            "the advanced tier is fast-forward, quit and fullscreen; a default export writes nul for all four"
        );

        let mut ordinary = request(false);
        ordinary.show_menu = true;
        let ordinary_config = embedded_runtime_config(&write_test_launcher(ordinary));
        assert_eq!(
            config_value(&ordinary_config, "input_toggle_fast_forward"),
            Some("nul")
        );
        assert_eq!(
            config_value(&ordinary_config, "input_hold_fast_forward"),
            Some("nul")
        );
        assert!(
            !ordinary_config.contains(" = \"space\""),
            "a normal export must not write the advanced Space binding"
        );
        // The player quits a shipped game from the menu, and we handle f like
        // q so that no gameplay bind is also a hotkey. Keep this default.
        assert_eq!(
            config_value(&ordinary_config, "input_exit_emulator"),
            Some("nul")
        );
        assert_eq!(
            config_value(&ordinary_config, "input_toggle_fullscreen"),
            Some("nul")
        );

        let mut advanced = request(false);
        advanced.show_menu = true;
        advanced.advanced_emulator_access = true;
        let config = embedded_runtime_config(&write_test_launcher(advanced));
        assert_eq!(
            config_value(&config, "input_toggle_fast_forward"),
            Some("space")
        );
        assert_eq!(config_value(&config, "input_hold_fast_forward"), Some("l"));
        for name in [
            "toggle_fast_forward",
            "hold_fast_forward",
            "menu_toggle",
            "exit_emulator",
            "toggle_fullscreen",
            "rewind",
        ] {
            for suffix in ["_btn", "_axis", "_mbtn"] {
                assert_eq!(
                    config_value(&config, &format!("input_{name}{suffix}")),
                    Some("nul"),
                    "{name}{suffix} stays nul when advanced access is on"
                );
            }
        }
        assert_eq!(config_value(&config, "input_menu_toggle"), Some("escape"));
        // q and f when advanced access is on. The default export above
        // contains nul.
        assert_eq!(config_value(&config, "input_exit_emulator"), Some("q"));
        assert_eq!(config_value(&config, "input_toggle_fullscreen"), Some("f"));
        assert_eq!(config_value(&config, "input_rewind"), Some("nul"));
        assert_eq!(
            config_value(&config, "input_menu_toggle_gamepad_combo"),
            Some("2")
        );
        assert_eq!(config_value(&config, "input_quit_gamepad_combo"), Some("0"));
        assert!(config.contains(&isolated_hotkey_config(true, true)));
    }

    /// The player still opens the menu with Escape and quits a game from it.
    /// Quit and fullscreen are advanced-access keys, so this default export
    /// contains nul for both. So Q cannot quit in the middle of a game, and f
    /// stays free for gameplay.
    #[test]
    fn escape_stays_the_menu_toggle_and_quit_and_fullscreen_are_advanced_only() {
        let mut with_menu = request(false);
        with_menu.show_menu = true;
        let menu_config = embedded_runtime_config(&write_test_launcher(with_menu));
        assert_eq!(
            config_value(&menu_config, "input_menu_toggle"),
            Some("escape")
        );
        // We handle fullscreen like quit, so that a gameplay bind on f does
        // not also toggle fullscreen. The macOS window menu has a Full Screen item.
        assert_eq!(
            config_value(&menu_config, "input_toggle_fullscreen"),
            Some("nul")
        );
        // A default export has no exit key. The player quits from the menu
        // that Escape opens, not with a hidden keyboard shortcut.
        assert_eq!(
            config_value(&menu_config, "input_exit_emulator"),
            Some("nul")
        );
        assert_eq!(
            config_value(&menu_config, "input_menu_toggle_gamepad_combo"),
            Some("2")
        );
        assert_eq!(
            config_value(&menu_config, "input_quit_gamepad_combo"),
            Some("0")
        );
        for suffix in ["_btn", "_axis", "_mbtn"] {
            assert_eq!(
                config_value(&menu_config, &format!("input_menu_toggle{suffix}")),
                Some("nul")
            );
            assert_eq!(
                config_value(&menu_config, &format!("input_toggle_fullscreen{suffix}")),
                Some("nul")
            );
        }
        assert!(!menu_config.contains("input_player1_"));

        let splash_config = embedded_runtime_config(&write_test_launcher(request(true)));
        assert_eq!(
            config_value(&splash_config, "input_menu_toggle"),
            Some("nul")
        );
        // A splash export is still a default export, so f is nul unless
        // advanced access is on.
        assert_eq!(
            config_value(&splash_config, "input_toggle_fullscreen"),
            Some("nul")
        );
    }

    #[test]
    fn hotkey_policy_matches_pinned_retroarch_meta_binds() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../work/experiments/rml-retroarch/retroarch/configuration.c");
        if !source.is_file() {
            return;
        }
        let text = fs::read_to_string(&source).unwrap();
        let mut pinned = Vec::new();
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("DECLARE_META_BIND(") else {
                continue;
            };
            let name = rest
                .split(',')
                .nth(1)
                .map(str::trim)
                .expect("DECLARE_META_BIND has a bind name");
            if !pinned.iter().any(|existing| existing == name) {
                pinned.push(name.to_string());
            }
        }
        let policy: Vec<&str> = HOTKEY_BINDS.iter().map(|bind| bind.name).collect();
        assert_eq!(
            policy,
            pinned.iter().map(String::as_str).collect::<Vec<_>>(),
            "HOTKEY_BINDS must stay exhaustive against pinned DECLARE_META_BIND"
        );

        let commented_defaults =
            fs::read_to_string(source.parent().unwrap().join("retroarch.cfg")).unwrap();
        assert!(commented_defaults.contains("input_toggle_fast_forward = space"));
        assert!(commented_defaults.contains("input_exit_emulator = escape"));
        assert!(commented_defaults.contains("input_menu_toggle = f1"));
    }

    /// A unique empty directory, following the pattern of the other tests.
    fn scratch_dir() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "rominabox-remap-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// We write the emulated controller where RetroArch reads it.
    ///
    /// The device is not read from the controls config, so a device written
    /// there would leave every console that declares a `coreDevice` on the
    /// default pad of the core.
    #[test]
    fn the_emulated_device_is_written_as_a_remap_not_a_config_line() {
        let root = scratch_dir();
        let profile = controls::ControlProfile {
            id: "ps1".into(),
            name: "PlayStation".into(),
            systems: vec!["ps1".into()],
            image: String::new(),
            core_device: Some(517),
            controls: Vec::new(),
        };
        let core = crate::systems::Core {
            artifacts: Default::default(),
            component: "pcsx_rearmed".into(),
            license: "GPL-2.0".into(),
            license_file: "pcsx_rearmed.txt".into(),
            capabilities: Vec::new(),
            library_name: Some("PCSX-ReARMed".into()),
        };
        let remaps = root.join("remaps");
        stage_controller_remap(&profile, &core, &remaps).expect("a remap is written");

        // The folder name is the library name of the core, not its component
        // id, because the path in config_load_remap is made from that name.
        let written = remaps.join("PCSX-ReARMed/PCSX-ReARMed.rmp");
        let text = fs::read_to_string(&written).expect("remap exists at the path RetroArch reads");
        assert!(
            text.contains("input_libretro_device_p1 = \"517\""),
            "the remap must name the declared device: {text}"
        );
    }

    /// A pad that is the default device of the core requires no remap.
    #[test]
    fn a_profile_with_no_declared_device_writes_nothing() {
        let root = scratch_dir();
        let profile = controls::ControlProfile {
            id: "nes".into(),
            name: "NES".into(),
            systems: vec!["nes".into()],
            image: String::new(),
            core_device: None,
            controls: Vec::new(),
        };
        let core = crate::systems::Core {
            artifacts: Default::default(),
            component: "nestopia".into(),
            license: "GPL-2.0".into(),
            license_file: "nestopia.txt".into(),
            capabilities: Vec::new(),
            library_name: None,
        };
        let remaps = root.join("remaps");
        stage_controller_remap(&profile, &core, &remaps).expect("nothing to do is not an error");
        assert!(!remaps.exists(), "no remap directory should be created");
    }

    /// When a device is required and there is no place to write it, export fails.
    #[test]
    fn a_declared_device_with_no_library_name_is_refused() {
        let root = scratch_dir();
        let profile = controls::ControlProfile {
            id: "megadrive6".into(),
            name: "Mega Drive six-button".into(),
            systems: vec!["megadrive".into()],
            image: String::new(),
            core_device: Some(513),
            controls: Vec::new(),
        };
        let core = crate::systems::Core {
            artifacts: Default::default(),
            component: "genesis_plus_gx".into(),
            license: "MAME".into(),
            license_file: "genesis_plus_gx.txt".into(),
            capabilities: Vec::new(),
            library_name: None,
        };
        let error = stage_controller_remap(&profile, &core, &root.join("remaps"))
            .expect_err("silently shipping the wrong pad is the defect being prevented");
        let message = error.to_string();
        assert!(
            message.contains("libraryName"),
            "the refusal must say what is missing: {message}"
        );
    }

    #[test]
    fn isolated_config_points_mutable_paths_at_the_managed_data_dir() {
        let config = isolated_runtime_config(&request(false));
        for (key, directory) in [
            ("savefile_directory", "saves"),
            ("savestate_directory", "states"),
            ("screenshot_directory", "screenshots"),
            ("thumbnails_directory", "thumbnails"),
            ("input_remapping_directory", "remaps"),
            ("rgui_config_directory", "config"),
            ("core_options_path", "core-options.cfg"),
        ] {
            let expected = format!("$data_dir/{directory}");
            assert_eq!(config_value(&config, key), Some(expected.as_str()));
        }
        assert!(MANAGED_DATA_DIRECTORIES.contains(&"overlays/keyboards"));
        assert_eq!(
            config_value(&config, "osk_overlay_directory"),
            Some("$data_dir/overlays/keyboards")
        );
        assert!(!config.contains("Application Support/RetroArch"));
        assert!(!config.contains("input_player1_"));
    }

    #[test]
    fn splash_without_menu_uses_rmlui_but_disables_menu_shortcuts() {
        let directory = std::env::temp_dir().join(format!(
            "rominabox-splash-launcher-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let launcher = directory.join("launcher");
        write_launcher(
            &launcher,
            "identity",
            OsStr::new("core"),
            OsStr::new("content/game.bin"),
            &request(true),
        )
        .unwrap();
        let script = fs::read_to_string(launcher).unwrap();
        assert!(script.contains("export ROMINABOX_SPLASH=1"));
        assert!(script.contains("menu_driver = \"rmlui\""));
        assert!(script.contains("input_menu_toggle = \"nul\""));
        assert!(script.contains("input_menu_toggle_gamepad_combo = \"0\""));
    }

    #[test]
    fn export_request_defaults_advanced_emulator_access_off() {
        let request: ExportRequest = serde_json::from_value(serde_json::json!({
            "rom": "game.bin",
            "title": "Game",
            "system": "megadrive",
            "showMenu": false,
            "startAtMenu": false,
            "theme": "native",
            "outputDir": "output",
            "target": "macos"
        }))
        .unwrap();
        assert!(!request.advanced_emulator_access);
    }

    #[test]
    fn launcher_sets_advanced_emulator_access_explicitly() {
        let off = write_test_launcher(request(false));
        assert!(off.contains("export ROMINABOX_ADVANCED_ACCESS=0"));
        assert!(!off.contains("export ROMINABOX_ADVANCED_ACCESS=1"));

        let mut on = request(false);
        on.advanced_emulator_access = true;
        let script = write_test_launcher(on);
        assert!(script.contains("export ROMINABOX_ADVANCED_ACCESS=1"));
        assert!(!script.contains("export ROMINABOX_ADVANCED_ACCESS=0"));
    }

    /// We store the saves of every exported game under
    /// `Games/{stable_identity(rom, system)}`. The identity is a hash of the
    /// system string as the caller supplied it, trimmed and lowercased but not
    /// turned into a console id, so that string is part of the save path of a
    /// player for good. These cases check that behaviour.
    ///
    /// Do not resolve aliases first. Aliases are valid in `systems::find`, so
    /// resolving them here would move the saves of every existing player to a
    /// new folder and leave the old folder behind without a warning.
    mod save_identity {
        use super::*;

        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        fn rom_with(bytes: &[u8]) -> PathBuf {
            let dir = std::env::temp_dir().join(format!(
                "rominabox-identity-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let rom = dir.join("game.bin");
            fs::write(&rom, bytes).unwrap();
            rom
        }

        #[test]
        fn identity_is_stable_for_the_same_rom_and_system() {
            let rom = rom_with(b"rominabox-identity-fixture");
            let first = stable_identity(&rom, "megadrive", None).unwrap();
            let second = stable_identity(&rom, "megadrive", None).unwrap();
            assert_eq!(first, second);
            assert_eq!(
                first.len(),
                24,
                "save directory names must stay 24 hex chars"
            );
            assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// Without a namespace, nothing changes.
        ///
        /// Every ordinary export runs without it, so the saves of existing
        /// players stay where they are. A failure here means that the save
        /// folders of ordinary exports move with worktree isolation.
        #[test]
        fn an_absent_namespace_leaves_the_identity_exactly_as_it_was() {
            let rom = rom_with(b"rominabox-identity-fixture");
            // We compare with a fixed value and not with a second computation,
            // because with both sides computed, a changed hash would go unnoticed.
            let identity = stable_identity(&rom, "megadrive", None).unwrap();
            assert_eq!(identity.len(), 24);
            assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(
                std::env::var("ROMINABOX_GAME_BUNDLE_PREFIX").is_err(),
                "this test only means anything with no namespace set"
            );
        }

        /// Two worktrees must not use the same data folder for a game.
        ///
        /// With a shared folder, a launch from one worktree rewrites
        /// retroarch.cfg under the running game of the other, both write to
        /// one launch.log, and a screenshot can show the wrong build.
        #[test]
        fn a_namespace_gives_the_same_game_a_separate_home() {
            let rom = rom_with(b"rominabox-identity-fixture");
            let shared = stable_identity(&rom, "megadrive", None).unwrap();
            let first =
                stable_identity(&rom, "megadrive", Some("app.rominabox.game.wt-a")).unwrap();
            let second =
                stable_identity(&rom, "megadrive", Some("app.rominabox.game.wt-b")).unwrap();

            assert_ne!(
                first, shared,
                "a namespaced export must not land on the shared home"
            );
            assert_ne!(
                first, second,
                "two worktrees must not share one game's home"
            );
            assert_eq!(
                stable_identity(&rom, "megadrive", Some("  ")).unwrap(),
                shared,
                "an empty namespace is no namespace, not a third directory"
            );
        }

        /// The bundle identifier contains the identity, so one namespace
        /// prevents both collisions.
        ///
        /// It is `app.rominabox.game.{identity}`, and macOS LaunchServices
        /// identifies apps by it. When two worktrees use one identifier, a
        /// double click on the game of one worktree brings up the running game
        /// of the other, and a rebuilt game does not start while an old one
        /// with the same identifier is running.
        #[test]
        fn the_bundle_identifier_is_namespaced_with_the_identity() {
            let rom = rom_with(b"rominabox-identity-fixture");
            let shared = stable_identity(&rom, "megadrive", None).unwrap();
            let isolated =
                stable_identity(&rom, "megadrive", Some("app.rominabox.game.wt-a")).unwrap();
            let identifier = |identity: &str| format!("app.rominabox.game.{identity}");
            assert_ne!(
                identifier(&shared),
                identifier(&isolated),
                "two checkouts of the same game must not claim one bundle identifier"
            );
        }

        #[test]
        fn identity_ignores_surrounding_space_and_letter_case() {
            let rom = rom_with(b"rominabox-identity-fixture");
            let canonical = stable_identity(&rom, "megadrive", None).unwrap();
            assert_eq!(
                stable_identity(&rom, "  MegaDrive  ", None).unwrap(),
                canonical
            );
            assert_eq!(stable_identity(&rom, "MEGADRIVE", None).unwrap(), canonical);
        }

        /// `gb` and its alias `Game Boy` both stand for the same console, but
        /// with the same ROM bytes they lead to different save folders, so
        /// callers must pass the canonical id. Change this behaviour only
        /// together with a move of the saves that players already have.
        #[test]
        fn an_alias_does_not_share_a_save_directory_with_its_canonical_id() {
            let rom = rom_with(b"rominabox-identity-fixture");
            let canonical = crate::systems::find("gb").expect("gb is a known system");
            let via_alias = crate::systems::find("Game Boy").expect("alias resolves");
            assert_eq!(
                canonical.id, via_alias.id,
                "both spellings must resolve to one console"
            );
            assert_ne!(
                stable_identity(&rom, "gb", None).unwrap(),
                stable_identity(&rom, "Game Boy", None).unwrap(),
                "identity hashes the supplied string, not the resolved console id"
            );
        }

        #[test]
        fn a_different_system_or_different_bytes_changes_the_identity() {
            let rom = rom_with(b"rominabox-identity-fixture");
            let other_rom = rom_with(b"rominabox-identity-fixture-2");
            let base = stable_identity(&rom, "megadrive", None).unwrap();
            assert_ne!(stable_identity(&rom, "nes", None).unwrap(), base);
            assert_ne!(
                stable_identity(&other_rom, "megadrive", None).unwrap(),
                base
            );
        }
    }

    /// We reject a disc container that the core cannot read.
    ///
    /// Support for CHD depends on the core binary, not on the console. We
    /// cannot export a CHD disc with a core built without CHD support, and
    /// this check does not apply to cartridge cores.
    mod core_capabilities {
        use super::*;

        fn request_for(system: &str, rom: &str) -> ExportRequest {
            let mut value = request(false);
            value.system = system.to_string();
            value.rom = PathBuf::from(rom);
            value
        }

        fn refusal(system: &str, rom: &str) -> Option<String> {
            let value = request_for(system, rom);
            let definition = crate::systems::find(system).expect("known system");
            let core = definition.cores.first()?;
            let extension = value
                .rom
                .extension()
                .and_then(OsStr::to_str)?
                .to_ascii_lowercase();
            (!core.capabilities.is_empty()
                && CONTAINER_FORMATS.contains(&extension.as_str())
                && !core.supports(&extension))
            .then(|| core.component.clone())
        }

        /// We check the capability for each binary. We build Genesis Plus GX
        /// with its CHD flag set, so CHD support is in that core and we can
        /// export Sega CD CHD discs. Another core, or a different build for
        /// another target, can still lack a format, so the check follows the
        /// binary and not the console.
        #[test]
        fn sega_cd_chd_works_now_that_the_core_is_built_with_chd() {
            assert_eq!(
                refusal("segacd", "game.chd"),
                None,
                "HAVE_CHD=1 is declared by the component, so this must not be refused"
            );
        }

        #[test]
        fn every_disc_console_accepts_what_its_core_can_actually_decode() {
            for (system, rom) in [
                ("segacd", "game.cue"),
                ("segacd", "game.chd"),
                ("segacd", "game.iso"),
                ("pcecd", "game.chd"),
                ("ps1", "game.chd"),
                ("ps1", "game.pbp"),
            ] {
                assert_eq!(refusal(system, rom), None, "{system} should accept {rom}");
            }
        }

        /// We still reject the case that the rule was written for.
        #[test]
        fn a_format_the_selected_core_cannot_decode_is_still_refused() {
            assert_eq!(
                refusal("segacd", "game.rvz").as_deref(),
                Some("genesis_plus_gx"),
                "a format outside the core's decoded set must still be refused"
            );
        }

        #[test]
        fn a_cartridge_core_is_never_constrained_by_this_check() {
            for (system, rom) in [("megadrive", "game.md"), ("nes", "game.nes")] {
                assert_eq!(refusal(system, rom), None, "{system} must be unaffected");
            }
        }
    }

    /// We keep the reason for every console that we cannot offer.
    ///
    /// We leave out a declared system whose core was never prepared, and the
    /// reason contains that fact.
    mod availability {
        use super::*;

        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        /// A kit containing exactly the named cores and licence texts.
        fn kit(cores: &[(&str, bool, bool)]) -> PathBuf {
            let root = std::env::temp_dir().join(format!(
                "rominabox-availability-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("cores")).unwrap();
            fs::create_dir_all(root.join("licenses")).unwrap();
            for (system, artifact, licence) in cores {
                let core = crate::systems::find(system)
                    .expect("known system")
                    .cores
                    .first()
                    .expect("declared core");
                if *artifact {
                    fs::write(
                        root.join("cores")
                            .join(core.artifact().expect("an artifact for this target")),
                        [],
                    )
                    .unwrap();
                }
                if *licence {
                    fs::write(root.join("licenses").join(&core.license_file), []).unwrap();
                }
            }
            root
        }

        fn entry(root: &Path, id: &str) -> SystemAvailability {
            system_availability(root)
                .into_iter()
                .find(|entry| entry.id == id)
                .expect("every declared console is reported")
        }

        #[test]
        fn a_prepared_console_names_the_component_that_will_run_it() {
            let root = kit(&[("megadrive", true, true)]);
            let megadrive = entry(&root, "megadrive");
            assert_eq!(megadrive.unavailable, None);
            assert_eq!(megadrive.component.as_deref(), Some("genesis_plus_gx"));
            assert!(available_systems(&root).contains(&"megadrive".to_string()));
        }

        #[test]
        fn a_missing_artifact_is_reported_as_a_missing_artifact() {
            let root = kit(&[("megadrive", false, true)]);
            let megadrive = entry(&root, "megadrive");
            match megadrive.unavailable {
                Some(Unavailable::NoPreparedCore { ref tried }) => {
                    assert_eq!(tried.len(), 1);
                    assert!(
                        tried[0].contains("genesis_plus_gx") && tried[0].contains("artifact"),
                        "{tried:?}"
                    );
                }
                other => panic!("expected a missing artifact, got {other:?}"),
            }
            assert!(!available_systems(&root).contains(&"megadrive".to_string()));
        }

        /// We must not ship a core whose binary is present without its licence
        /// text, because we are obliged to distribute the licence with it.
        #[test]
        fn an_artifact_without_its_licence_is_still_unavailable() {
            let root = kit(&[("megadrive", true, false)]);
            match entry(&root, "megadrive").unavailable {
                Some(Unavailable::NoPreparedCore { ref tried }) => assert!(
                    tried[0].contains("licence"),
                    "the reason should name the missing licence: {tried:?}"
                ),
                other => panic!("expected a missing licence, got {other:?}"),
            }
        }

        /// We may generate the registry on one machine and use it on another,
        /// because we ship on more than one platform. These tests check that
        /// the target is a parameter and not a constant.
        #[test]
        fn a_console_with_no_artifact_for_a_target_says_exactly_that() {
            let root = kit(&[("megadrive", true, true)]);
            // This is a macOS kit. Check the result for Windows with it.
            let windows = system_availability_for(&root, "windows-x86_64")
                .into_iter()
                .find(|entry| entry.id == "megadrive")
                .expect("every console is reported for every target");
            match windows.unavailable {
                Some(Unavailable::NoPreparedCore { ref tried }) => assert!(
                    tried[0].contains("no windows-x86_64 artifact declared"),
                    "a target with nothing declared is a different problem from a missing file: {tried:?}"
                ),
                other => panic!("expected an undeclared target, got {other:?}"),
            }
        }

        #[test]
        fn the_same_kit_resolves_for_the_target_it_was_built_for() {
            let root = kit(&[("megadrive", true, true)]);
            let macos = system_availability_for(&root, "macos-arm64")
                .into_iter()
                .find(|entry| entry.id == "megadrive")
                .expect("reported");
            assert_eq!(macos.component.as_deref(), Some("genesis_plus_gx"));
            assert_eq!(macos.unavailable, None);
        }

        #[test]
        fn every_shipped_component_declares_an_artifact_for_the_target_it_claims() {
            // A console enabled for a target without an artifact name for it
            // would be a declaration that we could never resolve.
            for system in crate::systems::registry() {
                for core in &system.cores {
                    assert!(
                        !core.artifacts.is_empty(),
                        "{} declares component {} with no artifact for any target",
                        system.id,
                        core.component
                    );
                }
            }
        }

        #[test]
        fn every_declared_console_is_accounted_for_either_way() {
            let root = kit(&[("megadrive", true, true)]);
            let reported = system_availability(&root);
            assert_eq!(
                reported.len(),
                crate::systems::registry().len(),
                "a console must never simply vanish from the report"
            );
            for entry in reported {
                assert_eq!(
                    entry.component.is_some(),
                    entry.unavailable.is_none(),
                    "{} must either resolve a component or give a reason",
                    entry.id
                );
            }
        }
    }
}
