//! Assembly of a self-contained native game.
//!
//! Export is blocking on purpose. Desktop callers should run it with
//! `tauri::async_runtime::spawn_blocking` and use the callback for progress.

use crate::launch_contract::{app_file, plan_field, plan_mark, shipped, token};
use crate::menu::file_name;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::content;
use crate::controls;
pub use crate::export_error::{ErrorStage, ExportError};
use crate::hotkeys::isolated_hotkey_config;
use crate::icons;

/// Disc image containers whose support depends on how a core was built.
const CONTAINER_FORMATS: &[&str] = &[
    "ccd", "cdi", "chd", "cue", "gdi", "iso", "m3u", "pbp", "rvz", "toc",
];

/// What we write in an export. On macOS we write one `.app`. On Windows we
/// write the executable and, when the runtime requires one, a folder next to
/// it. We put that app in the output folder and nothing else.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportTarget {
    Macos,
    Windows,
}

/// The platform in the download list that this export is for.
///
/// For a Windows package we download the Windows core, even on a Mac. For a
/// Mac package we download the Mac entry that matches the runtime in it. We
/// build that runtime for this machine and do not cross-compile it.
pub fn core_platform(target: &ExportTarget) -> &'static str {
    match target {
        ExportTarget::Windows => "windows-x86_64",
        ExportTarget::Macos => match std::env::consts::ARCH {
            "aarch64" => "macos-arm64",
            "x86_64" => "macos-x86_64",
            _ => "unsupported",
        },
    }
}

/// The core we ship in this export, with the file name for its platform.
///
/// A Windows export made on macOS contains `flycast_libretro.dll`, not the
/// host's `flycast_libretro.dylib`. We use this for the presence check, the
/// download, the copy and the licence. What the host runs is in `current_target`.
struct ExportCore<'a> {
    platform: &'static str,
    system_name: &'a str,
    core: &'a crate::systems::Core,
    artifact_name: &'a str,
}

fn export_core(request: &ExportRequest) -> Option<ExportCore<'static>> {
    let system = crate::systems::find(&request.system)?;
    let core = system.preferred_core()?;
    let platform = core_platform(&request.target);
    Some(ExportCore {
        platform,
        system_name: &system.name,
        artifact_name: core.artifact_for(platform)?,
        core,
    })
}

impl ExportCore<'_> {
    fn artifact_relative(&self) -> PathBuf {
        Path::new("cores").join(self.artifact_name)
    }

    fn licence_relative(&self) -> PathBuf {
        Path::new("licenses").join(&self.core.license_file)
    }
}

/// The path of the core binary for this export. An explicit `request.core` is
/// a development override. Otherwise it is the file chosen in `export_core`.
fn shipped_core(request: &ExportRequest, resolved: Option<&ExportCore<'_>>) -> PathBuf {
    if let Some(explicit) = &request.core {
        return explicit.clone();
    }
    let relative = resolved
        .map(ExportCore::artifact_relative)
        .unwrap_or_else(|| PathBuf::from("cores"));
    resolve_cached(
        &request.runtime_kit,
        request.core_cache.as_deref(),
        &relative,
    )
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
    /// Keep emulating when the window does not have the focus. RetroArch's
    /// `pause_nonactive` is the opposite of this. We write it into the frozen
    /// config, as we do quit-autosave, because the player has no control for
    /// it and a per-game `controls.cfg` would otherwise replace it.
    #[serde(default)]
    pub keep_playing_in_background: bool,
    /// Save on quit and load that save the next time the player opens the
    /// game. The author makes one choice for both.
    #[serde(default)]
    pub autosave_on_quit: bool,
    /// The Options entries we offer in this game. When absent, we use the
    /// design's defaults. With an empty list, we show no Options button.
    #[serde(default)]
    pub menu_entries: Option<Vec<String>>,
    /// The shader presets we bundle into the game. Usually there are none,
    /// and then the game has no shader screen and no preset.
    #[serde(default)]
    pub shaders: crate::shaders::ShaderSelection,
    /// Include player-authenticated Casual achievements, independently of data.
    #[serde(default = "crate::achievements::default_included")]
    pub include_achievements: bool,
    pub output_dir: PathBuf,
    /// Replace an app already at the destination. Without it, when an app is
    /// already there, we export nothing and say so.
    #[serde(default)]
    pub replace: bool,
    pub target: ExportTarget,
    /// A frozen, redistributable kit. It contains `bin/retroarch`,
    /// `designs/<id>/`, `menu-assets/`, `autoconfig/`, `licenses/` and
    /// `manifest.json`, but no cores, which come from `core_cache`. The
    /// documents of a design are under its name, and we share the controller
    /// artwork between designs, because we show the same pads in every design.
    #[serde(default)]
    pub runtime_kit: PathBuf,
    /// Optional explicit core path for development and future custom kits.
    pub core: Option<PathBuf>,
    /// The cores we fetch when someone makes an app, with the same layout as a
    /// kit, `cores/` and `licenses/`. At export we copy the one core for the
    /// game from here, so the player downloads nothing when the game opens. At
    /// every export we check whether the cached nightly is still the newest.
    /// When this is absent, we fetch nothing and the core must be in the kit.
    #[serde(default)]
    pub core_cache: Option<PathBuf>,
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
    system_availability_in(runtime_kit, None, target)
}

/// Resolve availability, also looking in the cache of downloaded cores.
///
/// We search the cache first. It contains the cores downloaded when we create
/// an app, in the same `cores/` and `licenses/` layout, and it is not a
/// global RetroArch folder.
pub fn system_availability_in(
    runtime_kit: &Path,
    cache: Option<&Path>,
    target: &str,
) -> Vec<SystemAvailability> {
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
                let artifact =
                    resolve_cached(runtime_kit, cache, &Path::new("cores").join(filename));
                let licence = resolve_cached(
                    runtime_kit,
                    cache,
                    &Path::new("licenses").join(&core.license_file),
                );
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
    Complete,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub stage: ExportStage,
    pub fraction: f32,
    pub message: String,
    /// We set it only on the event for fetching cores or failing to fetch them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cores: Option<crate::export_cores::CoreActivity>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub app_path: PathBuf,
    pub installed_bytes: u64,
    pub runtime_bytes: u64,
    pub content_bytes: u64,
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
                Err(error) => return Err(ExportError::io(ErrorStage::Stage, &path, error)),
            }
        }

        Err(ExportError::new(
            ErrorStage::Stage,
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
        Err(error) => return Err(ExportError::io(ErrorStage::Cleanup, path, error)),
    };
    let result = if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)
    } else {
        // With `remove_dir_all` we remove symlinks without following them, and
        // the root is the exact directory that we created atomically above.
        fs::remove_dir_all(path)
    };
    result.map_err(|error| ExportError::io(ErrorStage::Cleanup, path, error))
}

pub fn export_game<F>(
    request: &ExportRequest,
    cancelled: &AtomicBool,
    progress: F,
) -> Result<ExportResult, ExportError>
where
    F: FnMut(ExportProgress),
{
    export_game_fetching(request, cancelled, progress, &crate::cores::UreqTransport)
}

/// `export_game` with the network as an argument. In tests we pass a transport
/// with answers from a table, and we count the requests.
pub fn export_game_fetching<F>(
    request: &ExportRequest,
    cancelled: &AtomicBool,
    mut progress: F,
    transport: &dyn crate::cores::Transport,
) -> Result<ExportResult, ExportError>
where
    F: FnMut(ExportProgress),
{
    // Before we look for the file during validation. Here we bring the core
    // into the cache, downloaded or updated, and build the game with it. We
    // fix the platform here and read `resolved` in every later step.
    // Before anything else, because the author decides about an app in the way.
    crate::publish::refuse_unless_replacing(request)?;
    let resolved = export_core(request);
    prepare_core(request, resolved.as_ref(), &mut progress, transport)?;
    emit(
        &mut progress,
        ExportStage::Validate,
        0.02,
        "Checking export inputs",
    );
    validate_request(request, resolved.as_ref())?;
    check_cancelled(cancelled)?;
    match request.target {
        ExportTarget::Macos => export_macos(request, resolved.as_ref(), cancelled, &mut progress),
        ExportTarget::Windows => Err(ExportError::new(
            ErrorStage::Refused,
            "Windows apps cannot be made with this version of ROM-in-a-Box yet.",
        )),
    }
}

/// The defaults of the player's settings that the author chose in `request`.
fn player_defaults(request: &ExportRequest) -> crate::player_settings::Defaults {
    crate::player_settings::Defaults {
        keep_playing_in_background: request.keep_playing_in_background,
    }
}

/// The in-game menu that we ship in an export of `request`, for a game with
/// `discs` discs. We use this one mapping in the tests too, so that we test
/// exactly what an export would stage.
pub fn menu_request(request: &ExportRequest, discs: usize) -> crate::menu::MenuRequest {
    let kit = &request.runtime_kit;
    crate::menu::MenuRequest {
        palette: request.palette.clone(),
        background: request.background.clone(),
        system: request.system.clone(),
        controls: request.controls.clone(),
        show_menu: request.show_menu,
        splash: request.splash,
        include_achievements: request.include_achievements,
        menu_entries: request.menu_entries.clone(),
        shaders: request.shaders.clone(),
        discs,
        settings: player_defaults(request),
        sound_pack: request.menu_sounds != "off",
        // Controller artwork is not part of a design. We show the same pads in
        // every design, from the shared menu-assets in the kit.
        ..crate::menu::MenuRequest::new(
            crate::themes::staged_design(kit, &request.theme),
            kit.join("menu-assets"),
        )
    }
}

/// Write everything for the menu into `menu_assets`: the composed design,
/// the control defaults, and the logo of the splash when there is a splash.
/// Returns the controller profile we wrote the defaults for.
pub fn stage_menu(
    request: &ExportRequest,
    discs: usize,
    menu_assets: &Path,
) -> Result<controls::ControlProfile, ExportError> {
    crate::menu::compose_menu(&menu_request(request, discs))
        .and_then(|menu| menu.write(menu_assets))
        .map_err(|message| ExportError::new(ErrorStage::Stage, message))?;
    fs::create_dir_all(menu_assets)
        .map_err(|error| ExportError::io(ErrorStage::Stage, menu_assets, error))?;
    let profile = controls::write_defaults_config(
        &request.system,
        &request.controls,
        &menu_assets.join(file_name!(ControlsDefaults)),
    )
    .map_err(|message| ExportError::new(ErrorStage::Stage, message))?;
    if request.splash {
        copy_file(
            &request.runtime_kit.join("branding/logo.png"),
            &menu_assets.join("splash-logo.png"),
        )?;
    }
    Ok(profile)
}

fn export_macos<F>(
    request: &ExportRequest,
    resolved: Option<&ExportCore<'_>>,
    cancelled: &AtomicBool,
    progress: &mut F,
) -> Result<ExportResult, ExportError>
where
    F: FnMut(ExportProgress),
{
    if !cfg!(target_os = "macos") {
        return Err(ExportError::new(
            ErrorStage::Refused,
            "Mac apps can only be made on a Mac.",
        ));
    }

    if !Path::new("/usr/bin/codesign").is_file() {
        return Err(ExportError::new(ErrorStage::Refused, "This version of macOS does not provide the signing service required by this build. No tools were installed and no app was exported."));
    }
    let app_name = crate::publish::macos_app_name(&request.title);
    let final_app = request.output_dir.join(&app_name);
    fs::create_dir_all(&request.output_dir)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &request.output_dir, error))?;

    let staging = OwnedStaging::create(&request.output_dir)?;
    let app = staging.path().join(&app_name);
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    let frameworks = contents.join("Frameworks");
    for directory in [&macos, &resources, &frameworks] {
        fs::create_dir_all(directory)
            .map_err(|error| ExportError::io(ErrorStage::Stage, directory, error))?;
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
            ErrorStage::Validate,
            format!("unsupported system: {}", request.system),
        )
    })?;
    let selected_core = match resolved {
        Some(export_core) => export_core.core,
        None => system.preferred_core().ok_or_else(|| {
            ExportError::new(
                ErrorStage::Validate,
                format!("{} has no configured core", system.name),
            )
        })?,
    };
    let core_source = shipped_core(request, resolved);
    let core_name = OsStr::new(app_file!(Core));
    let core = resources.join(core_name);
    copy_file(&core_source, &core)?;
    let collected_content = content::collect_for(&request.rom, Some(&system.id))
        .map_err(|message| ExportError::new(ErrorStage::Validate, message))?;
    let content_directory = resources.join("content");
    for file in &collected_content.files {
        copy_content_file(file, &content_directory)?;
    }
    let rom_relative = Path::new("content").join(&collected_content.entrypoint);
    let controls_profile = stage_menu(
        request,
        collected_content.discs,
        &resources.join(app_file!(MenuAssets)),
    )?;
    let moved = controls::placement(&request.system, &request.controls)
        .and_then(|placed| crate::pad_positions::remap_lines(&placed, &controls::pad_positions()?))
        .map_err(|error| ExportError::new(ErrorStage::Stage, error))?;
    stage_controller_remap(
        &controls_profile,
        &moved,
        selected_core,
        &resources.join(shipped!(Remaps).0),
    )?;
    stage_pixel_options(selected_core, &resources.join(shipped!(CoreOptions).0))?;
    if request.show_menu {
        crate::themes::prepare_sound_assets(
            &request.runtime_kit.join("sound-packs"),
            &resources.join("assets/sounds"),
            &request.menu_sounds,
        )
        .map_err(|message| ExportError::new(ErrorStage::Stage, message))?;
    }
    stage_firmware(request, &resources.join(shipped!(Firmware).0))?;
    stage_bundled_autoconfig(
        &request.runtime_kit,
        &resources.join(shipped!(Autoconfig).0),
    )?;
    let licence = resolved
        .map(ExportCore::licence_relative)
        .unwrap_or_else(|| Path::new("licenses").join(&selected_core.license_file));
    stage_legal_materials(
        &request.runtime_kit,
        request.core_cache.as_deref(),
        &resources.join("Legal"),
        selected_core,
        &licence,
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
    install_launch_library(&macos, &runtime)?;
    mach_objects.push(macos.join("librominabox-launch.dylib"));
    write_launch_plan(
        &resources.join(app_file!(Plan)),
        &identity,
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
        "core": app_file!(Core),
        "coreSource": resolved.map(|export_core| export_core.artifact_name).unwrap_or(""),
        "content": collected_content.files.iter().map(|file| file.relative.to_string_lossy()).collect::<Vec<_>>(),
        "rom": rom_relative.to_string_lossy(),
        "firmware": request.firmware.iter().filter_map(|path| firmware_destination_name(path, system)).collect::<Vec<_>>(),
        "splash": request.splash,
        "advancedEmulatorAccess": request.advanced_emulator_access,
        "keepPlayingInBackground": request.keep_playing_in_background,
        "autosaveOnQuit": request.autosave_on_quit,
        "menuEntries": request.menu_entries,
        "includeAchievements": crate::achievements::included(request.include_achievements, request.show_menu),
    });
    fs::write(
        resources.join("game.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(|error| ExportError::io(ErrorStage::Configure, &resources.join("game.json"), error))?;
    let default_icon = icons::default_icon_path(&request.runtime_kit);
    if let Some(icon) = request.icon.as_deref().or(default_icon.as_deref()) {
        icons::create_macos_icon(icon, &resources.join("GameIcon.icns"), staging.path())?;
    }
    check_cancelled(cancelled)?;

    // A freshly built player still refers to the libraries it was linked
    // against, and the frozen kit was already rewritten. In both cases we must
    // point the game at the copies that we just staged next to it.
    relocate_dependencies(
        &mach_objects,
        "@executable_path/../Frameworks",
        Some(cancelled),
    )?;

    emit(progress, ExportStage::Sign, 0.70, "Signing the local app");
    for object in mach_objects.iter().rev() {
        run_command_cancellable(
            ErrorStage::Sign,
            Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(object),
            cancelled,
        )?;
    }
    let entitlements = staging.path().join("entitlements.plist");
    fs::write(
        &entitlements,
        sandbox_entitlements(&identity, accounts_folder(request)?.as_deref()),
    )
    .map_err(|error| ExportError::io(ErrorStage::Sign, &entitlements, error))?;
    run_command_cancellable(
        ErrorStage::Sign,
        Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-", "--entitlements"])
            .arg(&entitlements)
            .arg(&app),
        cancelled,
    )?;
    check_cancelled(cancelled)?;

    let installed_bytes = tree_size(&app)?;
    let runtime_bytes = tree_size(&runtime)?
        + tree_size(&core)?
        + tree_size(&frameworks)?
        + tree_size(&resources.join(app_file!(MenuAssets)))?
        + tree_size(&resources.join(shipped!(Autoconfig).0))?;
    let content_bytes = tree_size(&content_directory)?
        + tree_size(&resources.join(shipped!(Firmware).0))?
        + request
            .background
            .as_ref()
            .map_or(0, |path| fs::metadata(path).map(|m| m.len()).unwrap_or(0));
    check_cancelled(cancelled)?;
    crate::publish::put_in_place(&app, &final_app, request.replace)?;
    staging.cleanup()?;
    emit(progress, ExportStage::Complete, 1.0, "Export complete");
    Ok(ExportResult {
        app_path: final_app,
        installed_bytes,
        runtime_bytes,
        content_bytes,
    })
}

fn validate_request(
    request: &ExportRequest,
    resolved: Option<&ExportCore<'_>>,
) -> Result<(), ExportError> {
    if request.title.trim().is_empty() {
        return Err(ExportError::new(ErrorStage::Validate, "title is required"));
    }
    // A design is a directory, so we catch an unknown one when we resolve it.
    if let Err(message) = crate::themes::design_root(&request.theme) {
        return Err(ExportError::new(ErrorStage::Validate, message));
    }
    if request.start_at_menu && !request.show_menu {
        return Err(ExportError::new(
            ErrorStage::Validate,
            "startAtMenu requires showMenu",
        ));
    }
    crate::achievements::entries(
        &crate::themes::design_root(&request.theme)
            .map_err(|message| ExportError::new(ErrorStage::Validate, message))?,
        request.include_achievements,
        request.show_menu,
        request.menu_entries.as_deref(),
    )
    .map_err(|message| ExportError::new(ErrorStage::Validate, message))?;
    controls::validate_for_system(&request.system, &request.controls)
        .map_err(|message| ExportError::new(ErrorStage::Validate, message))?;
    if !request.shaders.is_empty() && !request.show_menu {
        return Err(ExportError::new(
            ErrorStage::Refused,
            "Shaders need the in-game menu. Turn the menu on, or leave shaders unset.",
        ));
    }
    crate::shaders::resolve(&request.shaders)
        .map_err(|message| ExportError::new(ErrorStage::Validate, message))?;
    let runtime = request.runtime_kit.join("bin/retroarch");
    for (stage, label, path) in [
        (ErrorStage::Missing, "ROM", &request.rom),
        (ErrorStage::Validate, "runtime", &runtime),
    ] {
        if !path.is_file() {
            return Err(ExportError::new(
                stage,
                format!("{label} file does not exist: {}", path.display()),
            )
            .about(path));
        }
    }
    crate::achievements::validate_runtime(
        &request.runtime_kit,
        crate::achievements::included(request.include_achievements, request.show_menu),
    )
    .map_err(|message| ExportError::new(ErrorStage::Validate, message))?;
    let system = crate::systems::find(&request.system).ok_or_else(|| {
        ExportError::new(
            ErrorStage::Validate,
            format!("unsupported system: {}", request.system),
        )
    })?;
    if system.preferred_core().is_none() {
        return Err(ExportError::new(
            ErrorStage::Validate,
            format!("{} has no configured core", system.name),
        ));
    }
    let core = shipped_core(request, resolved);
    if !core.is_file() {
        return Err(ExportError::new(
            ErrorStage::Validate,
            format!("core does not exist: {}", core.display()),
        ));
    }
    for path in request.icon.iter().chain(request.background.iter()) {
        if !path.is_file() {
            return Err(ExportError::new(
                ErrorStage::Missing,
                format!("asset does not exist: {}", path.display()),
            )
            .about(path));
        }
    }
    content::collect_for(&request.rom, Some(&system.id))
        .map_err(|message| ExportError::new(ErrorStage::Validate, message))?;
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
                ErrorStage::Refused,
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
                ErrorStage::Validate,
                format!("splash logo does not exist: {}", logo.display()),
            ));
        }
    }
    Ok(())
}

fn prepare_core<F>(
    request: &ExportRequest,
    resolved: Option<&ExportCore<'_>>,
    progress: &mut F,
    transport: &dyn crate::cores::Transport,
) -> Result<(), ExportError>
where
    F: FnMut(ExportProgress),
{
    let (Some(resolved), Some(cache)) = (resolved, request.core_cache.as_deref()) else {
        return Ok(());
    };
    let present = [resolved.artifact_relative(), resolved.licence_relative()]
        .iter()
        .all(|relative| resolve_cached(&request.runtime_kit, Some(cache), relative).is_file());
    let wanted = [crate::export_cores::Wanted {
        component: &resolved.core.component,
        platform: resolved.platform,
        present,
    }];
    // We write the words in the builder, and the event contains the facts for
    // them. Neither contains a URL.
    crate::export_cores::prepare(cache, &wanted, transport, |activity| {
        progress(ExportProgress {
            stage: ExportStage::Validate,
            fraction: 0.04,
            message: activity.message(),
            cores: Some(activity.clone()),
        })
    })
    .map_err(|_| {
        ExportError::new(
            ErrorStage::Cores,
            format!(
                "The {} core could not be downloaded. Try again later.",
                resolved.system_name
            ),
        )
    })
}

/// A file that we ship in the export, from the cache or the kit. We look in
/// the cache first, because it contains what this builder downloaded, and we
/// put a newer nightly there, not in the kit.
fn resolve_cached(kit: &Path, cache: Option<&Path>, relative: &Path) -> PathBuf {
    if let Some(cache) = cache {
        let fetched = cache.join(relative);
        if fetched.is_file() {
            return fetched;
        }
    }
    kit.join(relative)
}

fn stage_legal_materials(
    runtime_kit: &Path,
    cache: Option<&Path>,
    destination: &Path,
    core: &crate::systems::Core,
    licence: &Path,
) -> Result<(), ExportError> {
    let licenses = destination.join("Licenses");
    fs::create_dir_all(&licenses)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &licenses, error))?;
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
        &resolve_cached(runtime_kit, cache, licence),
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
    ).map_err(|error| ExportError::io(ErrorStage::Stage, &licenses.join("README.txt"), error))?;

    let manifest_path = runtime_kit.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| ExportError::io(ErrorStage::Stage, &manifest_path, error))?,
    )
    .map_err(|error| {
        ExportError::new(
            ErrorStage::Stage,
            format!("invalid {}: {error}", manifest_path.display()),
        )
    })?;
    let components = manifest
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            ExportError::new(
                ErrorStage::Stage,
                "runtime manifest has no components array",
            )
        })?;
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
    .map_err(|error| {
        ExportError::io(
            ErrorStage::Stage,
            &destination.join("components.json"),
            error,
        )
    })?;
    Ok(())
}

/// The libraries in `frameworks` that `roots` load, following the links of
/// each dylib. `None` means that none of the roots was a Mach-O we could read,
/// for example a fixture shell script, so we ship the whole inventory, as
/// those tests expect. For a compiled player we return the closure, and we
/// leave anything outside it in the kit and do not copy it into the game.
fn framework_closure(
    roots: &[PathBuf],
    frameworks: &Path,
) -> Result<Option<HashSet<String>>, ExportError> {
    fn leaf(dependency: &str, frameworks: &Path) -> Option<String> {
        let name = Path::new(dependency).file_name()?.to_str()?.to_string();
        frameworks.join(&name).is_file().then_some(name)
    }

    let mut needed = HashSet::new();
    let mut readable = false;
    let mut queue = VecDeque::new();
    for root in roots {
        let dependencies = match macho_dependencies(root) {
            Ok(dependencies) => dependencies,
            Err(_) => continue,
        };
        readable = true;
        for dependency in dependencies {
            if let Some(name) = leaf(&dependency, frameworks) {
                queue.push_back(name);
            }
        }
    }
    if !readable {
        return Ok(None);
    }
    while let Some(name) = queue.pop_front() {
        if !needed.insert(name.clone()) {
            continue;
        }
        for dependency in macho_dependencies(&frameworks.join(&name))? {
            if let Some(next) = leaf(&dependency, frameworks) {
                queue.push_back(next);
            }
        }
    }
    Ok(Some(needed))
}

fn stage_frozen_dependencies(
    runtime_kit: &Path,
    destination: &Path,
    signed_objects: &mut Vec<PathBuf>,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let inventory_path = runtime_kit.join("runtime-dependencies.json");
    let inventory_bytes = fs::read(&inventory_path)
        .map_err(|error| ExportError::io(ErrorStage::Dependencies, &inventory_path, error))?;
    let inventory: NativeDependencyInventory =
        serde_json::from_slice(&inventory_bytes).map_err(|error| {
            ExportError::new(
                ErrorStage::Dependencies,
                format!("invalid {}: {error}", inventory_path.display()),
            )
        })?;
    if inventory.format_version != 1 {
        return Err(ExportError::new(
            ErrorStage::Dependencies,
            format!(
                "unsupported runtime dependency inventory version: {}",
                inventory.format_version
            ),
        ));
    }
    let source_directory = runtime_kit.join("Frameworks");
    let closure = framework_closure(signed_objects, &source_directory)?;
    let mut declared = HashSet::new();
    for dependency in inventory.files {
        check_cancelled(cancelled)?;
        let dependency_path = Path::new(&dependency.name);
        if dependency_path.components().count() != 1 || dependency.name.starts_with('.') {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!("invalid dependency filename: {}", dependency.name),
            ));
        }
        if !declared.insert(dependency.name.clone()) {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!("duplicate dependency filename: {}", dependency.name),
            ));
        }
        if closure
            .as_ref()
            .is_some_and(|needed| !needed.contains(&dependency.name))
        {
            continue;
        }
        let source = source_directory.join(&dependency.name);
        let actual = sha256_file(&source)?;
        if !actual.eq_ignore_ascii_case(&dependency.sha256) {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
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
        .map_err(|error| ExportError::io(ErrorStage::Dependencies, &source_directory, error))?
    {
        let entry = entry
            .map_err(|error| ExportError::io(ErrorStage::Dependencies, &source_directory, error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry
            .file_type()
            .map_err(|error| ExportError::io(ErrorStage::Dependencies, &entry.path(), error))?
            .is_file()
            && !declared.contains(&name)
        {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!("undeclared file in frozen Frameworks: {name}"),
            ));
        }
    }
    if let Some(needed) = &closure {
        let mut missing: Vec<_> = needed.difference(&declared).cloned().collect();
        missing.sort();
        if !missing.is_empty() {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!(
                    "player links {} which the runtime dependency inventory does not list",
                    missing.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, ExportError> {
    let mut file = fs::File::open(path)
        .map_err(|error| ExportError::io(ErrorStage::Dependencies, path, error))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 1024 * 128];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| ExportError::io(ErrorStage::Dependencies, path, error))?;
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
        ExportError::new(
            ErrorStage::Stage,
            "game content destination has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| ExportError::io(ErrorStage::Stage, parent, error))?;
    if let Some(bytes) = &file.staged_bytes {
        fs::write(&destination, bytes)
            .map_err(|error| ExportError::io(ErrorStage::Stage, &destination, error))
    } else {
        copy_file(&file.source, &destination)
    }
}

fn validate_firmware(
    request: &ExportRequest,
    system: &crate::systems::System,
) -> Result<(), ExportError> {
    for path in &request.firmware {
        if !path.is_file() {
            return Err(ExportError::new(
                ErrorStage::Missing,
                format!("firmware file does not exist: {}", path.display()),
            )
            .about(path));
        }
        if path.file_name().and_then(OsStr::to_str).is_none() {
            return Err(ExportError::new(
                ErrorStage::Validate,
                format!("firmware has no usable filename: {}", path.display()),
            ));
        }
    }
    let assessment = crate::systems::assess_firmware(system, &request.firmware);
    if !assessment.can_continue {
        return Err(ExportError::new(ErrorStage::Refused, assessment.refusal()));
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
        .map_err(|error| ExportError::io(ErrorStage::Stage, destination, error))?;
    let system = crate::systems::find(&request.system).ok_or_else(|| {
        ExportError::new(
            ErrorStage::Stage,
            format!("unsupported system: {}", request.system),
        )
    })?;
    for source in &request.firmware {
        let name = firmware_destination_name(source, system).ok_or_else(|| {
            ExportError::new(
                ErrorStage::Stage,
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
    // The author's game file. When it is gone, we tell the author that.
    let mut file = fs::File::open(rom).map_err(|error| {
        let stage = if error.kind() == io::ErrorKind::NotFound {
            ErrorStage::Missing
        } else {
            ErrorStage::Configure
        };
        ExportError::io(stage, rom, error)
    })?;
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
            .map_err(|error| ExportError::io(ErrorStage::Configure, rom, error))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize())[..24].to_string())
}

/// Write the emulated controller, and the controls that the author moved on
/// the pad, to the remap file for RetroArch.
///
/// `input_libretro_device_p1` looks like an ordinary setting, but the only
/// read of it in `configuration.c` is inside `input_remapping_load_file`,
/// not in the loading of `retroarch.cfg`. A device in the config has no
/// effect, and the default device of the core stays in use.
///
/// So we write the file to the path in `config_load_remap`,
/// `<remap dir>/<library name>/<library name>.rmp`, with the libretro library
/// name of the artifact. `moved` contains the lines that
/// `pad_positions::remap_lines` returns for the author's controls.
fn stage_controller_remap(
    profile: &controls::ControlProfile,
    moved: &str,
    core: &crate::systems::Core,
    remaps: &Path,
) -> Result<(), ExportError> {
    let device = profile
        .core_device
        .map(|device| format!("input_libretro_device_p1 = \"{device}\"\n"))
        .unwrap_or_default();
    let contents = device + moved;
    if contents.is_empty() {
        // Most pads are the core's default device, with nothing moved, and
        // need no remap at all.
        return Ok(());
    }
    let Some(library) = core.library_name.as_deref() else {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "{} needs a remap, but component '{}' does not declare its libraryName, so \
                 there is nowhere to write the remap RetroArch reads",
                profile.id, core.component
            ),
        ));
    };
    let directory = remaps.join(library);
    fs::create_dir_all(&directory)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &directory, error))?;
    let path = directory.join(format!("{library}.rmp"));
    fs::write(&path, contents).map_err(|error| ExportError::io(ErrorStage::Stage, &path, error))?;
    Ok(())
}

/// Write the options that show the pixels of this core as they are.
///
/// We write them to `<config dir>/<library name>/<library name>.opt`, the
/// path for per-core options in RetroArch, with the library name of the
/// remap. For a core with nothing declared we write no file, so the core
/// defaults apply. In the launcher, we copy the file into the game's config
/// directory on first launch, and keep a file the player already changed.
fn stage_pixel_options(core: &crate::systems::Core, destination: &Path) -> Result<(), ExportError> {
    if core.pixels.is_empty() {
        return Ok(());
    }
    let Some(library) = core.library_name.as_deref() else {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "component '{}' keeps its picture with core options, but does not declare its \
                 libraryName, so there is nowhere to write the options file RetroArch reads",
                core.component
            ),
        ));
    };
    if library.is_empty()
        || library.contains(['/', '\\'])
        || core.pixels.iter().any(|option| {
            option.key.is_empty()
                || option.value.is_empty()
                || option.key.contains([' ', '=', '"', '\n', '/'])
                || option.value.contains(['"', '\n'])
        })
    {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "component '{}' has a picture option that cannot be written as a core options line",
                core.component
            ),
        ));
    }
    let directory = destination.join(library);
    fs::create_dir_all(&directory)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &directory, error))?;
    let path = directory.join(format!("{library}.opt"));
    let mut contents = String::new();
    for option in &core.pixels {
        contents.push_str(&format!("{} = \"{}\"\n", option.key, option.value));
    }
    fs::write(&path, contents).map_err(|error| ExportError::io(ErrorStage::Stage, &path, error))?;
    Ok(())
}

/// The writable directories we create and manage under each game's data root.
pub const MANAGED_DATA_DIRECTORIES: &[&str] = &[
    "saves",
    "states",
    shipped!(Firmware).1,
    "cache",
    "logs",
    "info",
    "playlists",
    "screenshots",
    shipped!(Remaps).1,
    shipped!(CoreOptions).1,
    "shaders",
    "runtime-logs",
    "recordings",
    "recording-config",
    shipped!(Autoconfig).1,
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

fn isolated_runtime_config(request: &ExportRequest) -> String {
    let menu_driver = if request.show_menu || request.splash {
        "rmlui"
    } else {
        "null"
    };
    // We keep the game's audio running in the menu, so the player hears a
    // change of volume, and play pack cues only when the export has a pack.
    let menu_audio = request.show_menu;
    let menu_sounds = request.show_menu && request.menu_sounds != "off";
    // We set both halves with one option. The file is `<savestate>.auto`, not
    // a numbered pause-menu slot, so Save and Load on that row are unchanged.
    let autosave = if request.autosave_on_quit {
        "true"
    } else {
        "false"
    };
    let (data, resources) = (token!(DataDir), token!(ResourcesDir));
    let assets = if menu_sounds {
        format!("{resources}/assets")
    } else {
        format!("{data}/assets")
    };
    format!(
        r#"video_driver = "gl"
audio_driver = "coreaudio"
audio_enable_menu = "{menu_audio}"
audio_enable_menu_ok = "{menu_sounds}"
audio_enable_menu_cancel = "{menu_sounds}"
audio_enable_menu_scroll = "{menu_sounds}"
audio_enable_menu_bgm = "false"
audio_enable_menu_notice = "false"
cheevos_enable = "false"
cheevos_hardcore_mode_enable = "false"
cheevos_test_unofficial = "false"
cheevos_start_active = "false"
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
savefile_directory = "{data}/saves"
savestate_directory = "{data}/states"
savestate_auto_save = "{autosave}"
savestate_auto_load = "{autosave}"
system_directory = "{data}/{firmware}"
cache_directory = "{data}/cache"
log_dir = "{data}/logs"
libretro_info_path = "{data}/info"
playlist_directory = "{data}/playlists"
screenshot_directory = "{data}/screenshots"
core_options_path = "{data}/core-options.cfg"
auto_remaps_enable = "true"
network_cmd_enable = "false"
input_remap_sort_by_controller_enable = "false"
content_history_path = "{data}/playlists/content_history.lpl"
content_music_history_path = "{data}/playlists/content_music_history.lpl"
content_image_history_path = "{data}/playlists/content_image_history.lpl"
content_video_history_path = "{data}/playlists/content_video_history.lpl"
input_remapping_directory = "{data}/{remaps}"
rgui_config_directory = "{data}/{core_options}"
video_shader_dir = "{data}/shaders"
runtime_log_directory = "{data}/runtime-logs"
recording_output_directory = "{data}/recordings"
recording_config_directory = "{data}/recording-config"
# Seeded from Resources/autoconfig on launch, the same way remaps are seeded.
joypad_autoconfig_dir = "{data}/{autoconfig}"
assets_directory = "{assets}"
core_assets_directory = "{data}/downloads"
thumbnails_directory = "{data}/thumbnails"
content_database_path = "{data}/database"
cheat_database_path = "{data}/cheats"
overlay_directory = "{data}/overlays"
osk_overlay_directory = "{data}/overlays/keyboards"
libretro_directory = "{data}/cores"
video_filter_dir = "{data}/filters/video"
audio_filter_dir = "{data}/filters/audio"
history_list_enable = "false"
core_info_cache_enable = "false"
auto_overrides_enable = "false"
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
        isolated_hotkey_config(request.show_menu, request.advanced_emulator_access),
        firmware = shipped!(Firmware).1,
        remaps = shipped!(Remaps).1,
        core_options = shipped!(CoreOptions).1,
        autoconfig = shipped!(Autoconfig).1,
    )
}

fn game_data_template(identity: &str) -> String {
    format!(
        "{}/Library/Application Support/ROM-in-a-Box/Games/{identity}",
        token!(Home)
    )
}

/// The shared QUICK SIGN IN folder for this export, when it has achievements.
fn accounts_folder(request: &ExportRequest) -> Result<Option<String>, ExportError> {
    if !crate::achievements::included(request.include_achievements, request.show_menu) {
        return Ok(None);
    }
    let named = std::env::var("ROMINABOX_ACCOUNTS_FOLDER").ok();
    crate::achievements::accounts_folder(isolation_namespace().as_deref(), named.as_deref())
        .map(Some)
        .map_err(|message| ExportError::new(ErrorStage::Configure, message))
}

/// `accounts` is the QUICK SIGN IN folder, present exactly when the game has
/// achievements. We grant the network and that folder together.
fn sandbox_entitlements(identity: &str, accounts: Option<&str>) -> String {
    let (network, shared) = match accounts {
        Some(folder) => (
            "<key>com.apple.security.network.client</key><true/>".to_string(),
            format!(
                "<key>com.apple.security.temporary-exception.files.home-relative-path.read-write</key>\n\
                 <array><string>/Library/Application Support/{folder}/</string></array>"
            ),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.app-sandbox</key><true/>
{network}
<key>com.apple.security.device.usb</key><true/>
<key>com.apple.security.device.bluetooth</key><true/>
<key>com.apple.security.temporary-exception.files.home-relative-path.read-only</key>
<array><string>/Library/Application Support/ROM-in-a-Box/Games/{identity}/</string></array>
{shared}
</dict></plist>
"#
    )
}

/// Every file that the `.c` files among `inputs` include, with the paths from
/// the compiler, or `None` when that list is not available. The launcher
/// includes declarations from the player tree, and we must rebuild it after a
/// change there as after a change next to it.
fn included_files(inputs: &[PathBuf]) -> Option<Vec<PathBuf>> {
    let sources = inputs
        .iter()
        .filter(|input| input.extension() == Some(OsStr::new("c")));
    let listed = Command::new("cc").arg("-MM").args(sources).output().ok()?;
    if !listed.status.success() {
        return None;
    }
    // Make rules, `object: source header ...`. A line that ends in a backslash
    // continues on the next, and a space in a path has a backslash before it.
    let text = String::from_utf8_lossy(&listed.stdout)
        .replace("\\\n", " ")
        .replace("\\ ", "\0");
    Some(
        text.lines()
            .filter_map(|rule| rule.split_once(": "))
            .flat_map(|(_, files)| files.split_whitespace())
            .map(|file| PathBuf::from(file.replace('\0', " ")))
            .collect(),
    )
}

/// Build `destination` from `inputs`, every file it is made from. We compile
/// the `.c` files among them, and we rebuild it after a change to any header
/// it includes, wherever that header is.
fn compile_c(inputs: &[PathBuf], destination: &Path, extra: &[&str]) -> Result<(), ExportError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ExportError::io(ErrorStage::Configure, parent, error))?;
    }
    let built = fs::metadata(destination)
        .and_then(|meta| meta.modified())
        .ok();
    let unchanged = |input: &PathBuf| {
        fs::metadata(input)
            .and_then(|meta| meta.modified())
            .is_ok_and(|changed| Some(changed) <= built)
    };
    let current = built.is_some()
        && inputs.iter().all(unchanged)
        && included_files(inputs).is_some_and(|included| included.iter().all(unchanged));
    if current {
        return Ok(());
    }
    let parent = destination.parent().ok_or_else(|| {
        ExportError::new(
            ErrorStage::Configure,
            "compiled output needs a parent directory",
        )
    })?;
    let staging = OwnedStaging::create(parent)?;
    let temporary = staging.path().join("compiled");
    let status = Command::new("cc")
        .args(extra)
        .arg("-o")
        .arg(&temporary)
        .args(
            inputs
                .iter()
                .filter(|input| input.extension() == Some(OsStr::new("c"))),
        )
        .status()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Configure,
                format!("could not compile the launcher: {error}"),
            )
        })?;
    if !status.success() {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the launcher failed to compile",
        ));
    }
    fs::rename(&temporary, destination)
        .map_err(|error| ExportError::io(ErrorStage::Configure, destination, error))?;
    Ok(())
}

fn install_launch_library(macos: &Path, retroarch: &Path) -> Result<(), ExportError> {
    let launcher = crate::repo::at("desktop/src-tauri/launcher");
    let mut library_sources = fs::read_dir(&launcher)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| ExportError::io(ErrorStage::Configure, &launcher, error))?;
    library_sources
        .retain(|path| matches!(path.extension().and_then(OsStr::to_str), Some("c" | "h")));
    library_sources.sort();
    let injector_source = vec![crate::repo::at("scripts/native_runtime/inject_dylib.c")];
    let work = crate::repo::at("work");
    let library = work.join("librominabox-launch.dylib");
    let injector = work.join("inject-dylib");
    compile_c(
        &library_sources,
        &library,
        &[
            "-Oz",
            "-dynamiclib",
            "-Wl,-dead_strip",
            "-Wl,-install_name,@executable_path/librominabox-launch.dylib",
        ],
    )?;
    compile_c(&injector_source, &injector, &["-Oz"])?;
    let status = Command::new(&injector)
        .arg(retroarch)
        .arg("@executable_path/librominabox-launch.dylib")
        .status()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Configure,
                format!("could not attach the launcher: {error}"),
            )
        })?;
    if !status.success() {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the runtime has no room for the launcher",
        ));
    }
    copy_file(&library, &macos.join("librominabox-launch.dylib"))
}

fn write_launch_plan(
    path: &Path,
    identity: &str,
    rom: &OsStr,
    request: &ExportRequest,
) -> Result<(), ExportError> {
    if identity.contains(['\n', '\t', '/']) {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the game identity cannot be written into the launch plan",
        ));
    }
    let content = rom.to_string_lossy();
    if content.contains(['\n', '\t'])
        || content.starts_with('/')
        || content.split('/').any(|part| part == "..")
    {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the game's content path cannot be launched",
        ));
    }
    if request.title.contains(['\n', '\t']) {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the game title cannot be written into the launch plan",
        ));
    }
    let shader_initial = if request.show_menu {
        crate::shaders::launch_preset(&request.shaders)
            .map_err(|message| ExportError::new(ErrorStage::Configure, message))?
            .unwrap_or_default()
    } else {
        String::new()
    };
    if shader_initial.contains(['\n', '\t']) {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the starting shader cannot be written into the launch plan",
        ));
    }
    // On each line, a field name from the launcher, a tab and the value.
    let line = |field: &str, value: &str| format!("{field}\t{value}\n");
    let flag = |on: bool| if on { "1" } else { "0" };
    let achievements =
        crate::achievements::included(request.include_achievements, request.show_menu);
    let mut plan = String::from("rominabox-launch\t1\n");
    plan += &line(plan_field!(Identity), identity);
    plan += &line(plan_field!(Content), &content);
    plan += &line(plan_field!(Title), &request.title);
    plan += &line(plan_field!(StartAtMenu), flag(request.start_at_menu));
    plan += &line(
        plan_field!(Advanced),
        flag(request.advanced_emulator_access),
    );
    plan += &line(plan_field!(Achievements), flag(achievements));
    for setting in crate::player_settings::declared(player_defaults(request)).iter() {
        plan += &setting.launch_line();
    }
    plan += &line(plan_field!(ShaderInitial), &shader_initial);
    plan += &line(plan_field!(DataDir), &game_data_template(identity));
    if let Some(folder) = accounts_folder(request)? {
        plan += &line(plan_field!(AccountsDir), &folder);
    }
    for name in MANAGED_DATA_DIRECTORIES {
        plan += &line(plan_field!(Managed), name);
    }
    plan += plan_mark!(Config);
    plan += "\n";
    plan += &isolated_runtime_config(request);
    fs::write(path, plan).map_err(|error| ExportError::io(ErrorStage::Configure, path, error))
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
<key>CFBundleExecutable</key><string>retroarch</string>
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
    fs::write(path, plist).map_err(|error| ExportError::io(ErrorStage::Configure, path, error))
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
                    ErrorStage::Dependencies,
                    format!(
                        "could not resolve native dependency {dependency} required by {}",
                        origin.display()
                    ),
                ));
            };
            if !source.is_file() {
                return Err(ExportError::new(
                    ErrorStage::Dependencies,
                    format!("missing native dependency {dependency}"),
                ));
            }
            let name = source.file_name().unwrap().to_string_lossy().into_owned();
            let canonical = dunce::canonicalize(&source)
                .map_err(|error| ExportError::io(ErrorStage::Dependencies, &source, error))?;
            if let Some(previous) = copied.get(&name) {
                if dunce::canonicalize(previous).ok().as_ref() != Some(&canonical) {
                    return Err(ExportError::new(
                        ErrorStage::Dependencies,
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

/// The library that we build and sign next to the player at export. Its
/// install name is `@executable_path/` on purpose.
const LAUNCH_LIBRARY: &str = "librominabox-launch.dylib";

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
                // The launch library is next to the executable, not in
                // Frameworks, because we sign it separately, without the
                // entitlements of the bundle, and its install name is
                // @executable_path. We do not rewrite its path to ../Frameworks,
                // because the file is not there and dyld would fail before main.
                || Path::new(&dependency).file_name()
                    == Some(OsStr::new(LAUNCH_LIBRARY))
            {
                continue;
            }
            let name = Path::new(&dependency).file_name().ok_or_else(|| {
                ExportError::new(
                    ErrorStage::Dependencies,
                    format!("invalid dependency: {dependency}"),
                )
            })?;
            let replacement = format!("{framework_prefix}/{}", name.to_string_lossy());
            let mut command = Command::new("/usr/bin/install_name_tool");
            command
                .args(["-change", &dependency, &replacement])
                .arg(object);
            if let Some(cancelled) = cancelled {
                run_command_cancellable(ErrorStage::Dependencies, &mut command, cancelled)?;
            } else {
                run_command(ErrorStage::Dependencies, &mut command)?;
            }
        }
        if object.parent().and_then(Path::file_name) == Some(OsStr::new("Frameworks")) {
            let name = object.file_name().unwrap().to_string_lossy();
            let mut command = Command::new("/usr/bin/install_name_tool");
            command.args(["-id", &format!("@rpath/{name}")]).arg(object);
            if let Some(cancelled) = cancelled {
                run_command_cancellable(ErrorStage::Dependencies, &mut command, cancelled)?;
            } else {
                run_command(ErrorStage::Dependencies, &mut command)?;
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
            ErrorStage::Freeze,
            "macOS helper freezing requires a macOS host",
        ));
    }
    crate::publish::refuse_existing(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::new(ErrorStage::Freeze, "helper destination has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|error| ExportError::io(ErrorStage::Freeze, parent, error))?;
    copy_file(source, destination)?;
    make_executable(destination)?;
    let frameworks = parent.join("Frameworks");
    fs::create_dir_all(&frameworks)
        .map_err(|error| ExportError::io(ErrorStage::Freeze, &frameworks, error))?;
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
            ErrorStage::Freeze,
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
            ExportError::new(
                ErrorStage::Dependencies,
                format!("could not run otool: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(ExportError::command(
            ErrorStage::Dependencies,
            "otool",
            &output,
        ));
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

fn run_command(stage: ErrorStage, command: &mut Command) -> Result<(), ExportError> {
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
    stage: ErrorStage,
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
                ErrorStage::Cancelled,
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
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!("refusing to stage non-regular file: {}", source.display()),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ExportError::io(ErrorStage::Stage, parent, error))?;
    }
    fs::copy(source, destination)
        .map_err(|error| ExportError::io(ErrorStage::Stage, destination, error))?;
    Ok(())
}

fn copy_optional_tree(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?;
    if source_metadata.file_type().is_symlink() {
        return Err(ExportError::new(
            ErrorStage::Stage,
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
        .map_err(|error| ExportError::io(ErrorStage::Stage, destination, error))?;
    for entry in
        fs::read_dir(source).map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?
    {
        let entry = entry.map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?;
        let target = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| ExportError::io(ErrorStage::Stage, &entry.path(), error))?;
        if file_type.is_symlink() {
            return Err(ExportError::new(
                ErrorStage::Stage,
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
            .map_err(|error| ExportError::io(ErrorStage::Stage, path, error))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| ExportError::io(ErrorStage::Stage, path, error))?;
    }
    // Windows has no executable bit. A file runs because of its extension.
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

fn tree_size(path: &Path) -> Result<u64, ExportError> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ExportError::io(ErrorStage::Measure, path, error))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0;
    for entry in
        fs::read_dir(path).map_err(|error| ExportError::io(ErrorStage::Measure, path, error))?
    {
        let entry = entry.map_err(|error| ExportError::io(ErrorStage::Measure, path, error))?;
        total += tree_size(&entry.path())?;
    }
    Ok(total)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), ExportError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ExportError::new(ErrorStage::Cancelled, "export cancelled"))
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
        cores: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::{HotkeyKeyboard, HOTKEY_BINDS};

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
            keep_playing_in_background: false,
            autosave_on_quit: false,
            menu_entries: None,
            shaders: crate::shaders::ShaderSelection::default(),
            include_achievements: false,
            output_dir: PathBuf::from("output"),
            replace: false,
            target: ExportTarget::Macos,
            runtime_kit: PathBuf::from("runtime"),
            core: None,
            core_cache: None,
        }
    }

    #[test]
    fn concurrent_launcher_compiles_publish_without_sharing_temporary_files() {
        let root = rominabox_scratch::Scratch::dir("rominabox-compile-concurrency");
        let source = root.join("fixture.c");
        let output = root.join("fixture.o");
        std::fs::write(&source, "int fixture(void) { return 42; }\n").unwrap();
        let barrier = std::sync::Barrier::new(4);
        std::thread::scope(|threads| {
            let jobs: Vec<_> = (0..4)
                .map(|_| {
                    threads.spawn(|| {
                        barrier.wait();
                        compile_c(std::slice::from_ref(&source), &output, &["-c"])
                    })
                })
                .collect();
            for job in jobs {
                job.join()
                    .unwrap()
                    .expect("each concurrent compile publishes safely");
            }
        });
        assert!(output.is_file());
    }

    /// The launcher includes declarations from the player tree. We rebuild it
    /// after a change to one of them, though that file is not among its inputs.
    #[test]
    fn a_header_outside_the_inputs_rebuilds_what_includes_it() {
        let root = rominabox_scratch::Scratch::dir("rominabox-compile-includes");
        let (folder, shared) = (root.join("launcher"), root.join("shared"));
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&shared).unwrap();
        let (source, header) = (folder.join("fixture.c"), shared.join("value.h"));
        let output = root.join("fixture.o");
        fs::write(&header, "#define VALUE 1\n").unwrap();
        fs::write(
            &source,
            "#include \"../shared/value.h\"\nint fixture(void) { return VALUE; }\n",
        )
        .unwrap();
        compile_c(std::slice::from_ref(&source), &output, &["-c"]).unwrap();
        let first = fs::read(&output).unwrap();

        fs::write(&header, "#define VALUE 2\n").unwrap();
        let later =
            fs::metadata(&output).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
        fs::File::options()
            .write(true)
            .open(&header)
            .unwrap()
            .set_modified(later)
            .unwrap();
        compile_c(std::slice::from_ref(&source), &output, &["-c"]).unwrap();
        assert_ne!(
            fs::read(&output).unwrap(),
            first,
            "the object holds the new value"
        );
    }

    #[test]
    fn export_refuses_a_missing_required_bios_with_the_builders_explanation() {
        let system = crate::systems::find("pcecd").unwrap();
        let mut settings = request(false);
        settings.system = "pcecd".into();
        let error = validate_firmware(&settings, system).unwrap_err();
        let assessment = crate::systems::assess_firmware(system, &[]);
        assert!(!assessment.can_continue);
        assert_eq!(error.message, assessment.refusal());
    }

    #[test]
    fn export_allows_a_console_whose_bios_is_optional() {
        let system = crate::systems::find("ps1").unwrap();
        let mut settings = request(false);
        settings.system = "ps1".into();
        assert!(validate_firmware(&settings, system).is_ok());
    }

    #[test]
    fn launcher_quotes_hostile_content_filename_as_data() {
        let directory = rominabox_scratch::Scratch::dir("rominabox-launcher");
        let launcher = directory.join("launcher");
        let hostile = OsStr::new("content/weird'$(touch PWNED)`echo nope`.bin");
        let mut settings = request(false);
        settings.title = "Game's $(title) `literal`".into();
        write_launch_plan(&launcher, "identity", hostile, &settings).unwrap();
        let plan = fs::read_to_string(launcher).unwrap();

        assert!(plan.contains("content\tcontent/weird'$(touch PWNED)`echo nope`.bin\n"));
        assert!(plan.contains("title\tGame's $(title) `literal`\n"));
        assert!(!plan.contains("Resources/content/weird"));
    }

    fn write_test_launcher(settings: ExportRequest) -> String {
        let directory = rominabox_scratch::Scratch::dir("rominabox-hotkey");
        let launcher = directory.join("launcher");
        write_launch_plan(
            &launcher,
            "identity",
            OsStr::new("content/game.bin"),
            &settings,
        )
        .unwrap();
        fs::read_to_string(launcher).unwrap()
    }

    fn embedded_runtime_config(plan: &str) -> String {
        let marker = "---config---\n";
        let start = plan
            .find(marker)
            .expect("launch plan contains the runtime config");
        plan[start + marker.len()..].to_string()
    }

    fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
        config.lines().find_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            (name == key).then(|| value.trim_matches('"'))
        })
    }

    #[test]
    fn export_records_background_play_and_quit_autosave() {
        let mut settings = request(false);
        let plan = write_test_launcher(settings.clone());
        let off = embedded_runtime_config(&plan);
        assert_eq!(
            config_value(&off, "savestate_auto_save"),
            Some("false"),
            "quit autosave is left at RetroArch's default instead of being written"
        );
        assert_eq!(config_value(&off, "savestate_auto_load"), Some("false"));
        // The player can change background play. The export contains only the
        // default, which we apply at launch until the player chooses. In the
        // frozen config, the choice of the player could not replace it.
        assert_eq!(config_value(&off, "pause_nonactive"), None);
        assert!(
            plan.contains("player_setting\tbackground-play.cfg\tpause_nonactive\ttrue\n"),
            "{plan}"
        );

        settings.keep_playing_in_background = true;
        settings.autosave_on_quit = true;
        let plan = write_test_launcher(settings.clone());
        let on = embedded_runtime_config(&plan);
        assert_eq!(config_value(&on, "savestate_auto_save"), Some("true"));
        assert_eq!(config_value(&on, "savestate_auto_load"), Some("true"));
        assert_eq!(config_value(&on, "pause_nonactive"), None);
        assert!(
            plan.contains("player_setting\tbackground-play.cfg\tpause_nonactive\tfalse\n"),
            "{plan}"
        );
    }

    /// We keep the game audio running in the menu, so the player can hear a
    /// change of volume. With menu sounds Off we play none of the cues of the
    /// pack, and with a pack we play them all.
    #[test]
    fn the_menu_keeps_audio_for_the_volume_and_the_pack_decides_its_cues() {
        let mut settings = request(false);
        settings.show_menu = true;
        let off = embedded_runtime_config(&write_test_launcher(settings.clone()));
        assert_eq!(config_value(&off, "audio_enable_menu"), Some("true"));
        for cue in ["ok", "cancel", "scroll"] {
            assert_eq!(
                config_value(&off, &format!("audio_enable_menu_{cue}")),
                Some("false"),
                "menu sounds Off plays no {cue} cue"
            );
        }
        settings.menu_sounds = "blip".into();
        let pack = embedded_runtime_config(&write_test_launcher(settings));
        for cue in ["ok", "cancel", "scroll"] {
            assert_eq!(
                config_value(&pack, &format!("audio_enable_menu_{cue}")),
                Some("true"),
                "a pack plays its {cue} cue"
            );
        }
        assert_eq!(
            config_value(&pack, "assets_directory"),
            Some("$resources_dir/assets")
        );
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
        // Quit and fullscreen are not in this tier, because q and f are
        // gameplay keys in every mode. The base keyboard stays Neutral, so a
        // default export contains nul for both fast-forward keys.
        assert_eq!(
            advanced_tier,
            vec![
                ("toggle_fast_forward", "space", HotkeyKeyboard::Neutral),
                ("hold_fast_forward", "l", HotkeyKeyboard::Neutral),
            ],
            "the advanced tier is fast-forward; a default export writes nul for both"
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
        // The player quits from the menu and switches to fullscreen with
        // Alt+Enter, so q and f stay gameplay keys.
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
        // Advanced access does not bind q or f either.
        assert_eq!(config_value(&config, "input_exit_emulator"), Some("nul"));
        assert_eq!(
            config_value(&config, "input_toggle_fullscreen"),
            Some("nul")
        );
        assert_eq!(config_value(&config, "input_rewind"), Some("nul"));
        assert_eq!(
            config_value(&config, "input_menu_toggle_gamepad_combo"),
            Some("2")
        );
        assert_eq!(config_value(&config, "input_quit_gamepad_combo"), Some("0"));
        assert!(config.contains(&isolated_hotkey_config(true, true)));
    }

    /// The player still opens the menu with Escape and quits a game from it.
    /// Quit and fullscreen have no key, so Q cannot quit in the middle of a
    /// game, and q and f stay free for gameplay.
    #[test]
    fn escape_stays_the_menu_toggle_and_quit_and_fullscreen_have_no_key() {
        let mut with_menu = request(false);
        with_menu.show_menu = true;
        let menu_config = embedded_runtime_config(&write_test_launcher(with_menu));
        assert_eq!(
            config_value(&menu_config, "input_menu_toggle"),
            Some("escape")
        );
        assert_eq!(
            config_value(&menu_config, "input_toggle_fullscreen"),
            Some("nul")
        );
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
        assert_eq!(
            config_value(&splash_config, "input_toggle_fullscreen"),
            Some("nul")
        );
    }

    /// A unique empty directory, following the pattern of the other tests.
    fn scratch_dir() -> rominabox_scratch::Scratch {
        rominabox_scratch::Scratch::dir("rominabox-remap")
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
            pixels: Vec::new(),
        };
        let remaps = root.join("remaps");
        stage_controller_remap(&profile, "", &core, &remaps).expect("a remap is written");

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
            pixels: Vec::new(),
        };
        let remaps = root.join("remaps");
        stage_controller_remap(&profile, "", &core, &remaps)
            .expect("nothing to do is not an error");
        assert!(!remaps.exists(), "no remap directory should be created");
    }

    /// A control the author moved on the pad is a remap even on a pad that is
    /// the default device of the core.
    #[test]
    fn a_moved_control_is_written_into_the_remap() {
        let root = scratch_dir();
        let profile = controls::profile_for_system("megadrive").unwrap();
        assert_eq!(
            profile.core_device.map(|_| ()),
            Some(()),
            "the Mega Drive pad names its device"
        );
        let default_device = controls::ControlProfile {
            core_device: None,
            ..profile
        };
        let core = crate::systems::Core {
            artifacts: Default::default(),
            component: "genesis_plus_gx".into(),
            license: String::new(),
            license_file: String::new(),
            capabilities: Vec::new(),
            library_name: Some("Genesis Plus GX".into()),
            pixels: Vec::new(),
        };
        let remaps = root.join("remaps");
        let moved = "input_player1_btn_b = \"8\"\ninput_player1_btn_a = \"0\"\n";
        stage_controller_remap(&default_device, moved, &core, &remaps).expect("a remap is written");
        let text = fs::read_to_string(remaps.join("Genesis Plus GX/Genesis Plus GX.rmp")).unwrap();
        assert_eq!(text, moved);
    }

    /// We write picture options where RetroArch reads per-core options.
    ///
    /// The folder name is the library name of the core, as for the remap.
    /// For a core with no declared options we write no file, because the
    /// defaults of the core already leave the pixels unchanged.
    #[test]
    fn picture_options_are_written_where_retroarch_reads_them() {
        let root = scratch_dir();
        let core = crate::systems::Core {
            artifacts: Default::default(),
            component: "nestopia".into(),
            license: "GPL-2.0".into(),
            license_file: "nestopia.txt".into(),
            capabilities: Vec::new(),
            library_name: Some("Nestopia".into()),
            pixels: vec![crate::systems::PixelOption {
                key: "nestopia_blargg_ntsc_filter".into(),
                value: "disabled".into(),
            }],
        };
        let destination = root.join("core-options");
        stage_pixel_options(&core, &destination).expect("options are written");
        let text = fs::read_to_string(destination.join("Nestopia/Nestopia.opt"))
            .expect("options exist at the path RetroArch reads");
        assert_eq!(text, "nestopia_blargg_ntsc_filter = \"disabled\"\n");

        let untouched = crate::systems::Core {
            pixels: Vec::new(),
            library_name: None,
            ..core
        };
        let empty = root.join("empty");
        stage_pixel_options(&untouched, &empty).expect("nothing to write is not an error");
        assert!(!empty.exists(), "no options directory should be created");

        let nameless = crate::systems::Core {
            library_name: None,
            pixels: vec![crate::systems::PixelOption {
                key: "nestopia_blargg_ntsc_filter".into(),
                value: "disabled".into(),
            }],
            ..untouched
        };
        let error = stage_pixel_options(&nameless, &root.join("missing"))
            .expect_err("options with no library name have nowhere to go");
        assert!(error.message.contains("libraryName"), "{}", error.message);

        let launcher = write_test_launcher(request(false));
        assert!(
            launcher.contains("managed\tconfig\n"),
            "the launcher has to be told about the directory core options are copied into"
        );
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
            pixels: Vec::new(),
        };
        let error = stage_controller_remap(&profile, "", &core, &root.join("remaps"))
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
        assert_eq!(config.matches("auto_remaps_enable").count(), 1);
        assert_eq!(config_value(&config, "auto_remaps_enable"), Some("true"));
        assert_eq!(config_value(&config, "network_cmd_enable"), Some("false"));
    }

    #[test]
    fn splash_without_menu_uses_rmlui_but_disables_menu_shortcuts() {
        let directory = rominabox_scratch::Scratch::dir("rominabox-splash-launcher");
        let launcher = directory.join("launcher");
        write_launch_plan(
            &launcher,
            "identity",
            OsStr::new("content/game.bin"),
            &request(true),
        )
        .unwrap();
        let script = fs::read_to_string(launcher).unwrap();
        // We show the logo only when we staged its file. There is no separate
        // splash flag in the launcher.
        assert!(!script.contains("ROMINABOX_SPLASH"));
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
        assert!(off.contains("advanced\t0\n"));
        assert!(!off.contains("advanced\t1\n"));

        let mut on = request(false);
        on.advanced_emulator_access = true;
        let plan = write_test_launcher(on);
        assert!(plan.contains("advanced\t1\n"));
        assert!(!plan.contains("advanced\t0\n"));
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

        fn rom_with(bytes: &[u8]) -> (rominabox_scratch::Scratch, PathBuf) {
            let dir = rominabox_scratch::Scratch::dir("rominabox-identity");
            let rom = dir.join("game.bin");
            fs::write(&rom, bytes).unwrap();
            (dir, rom)
        }

        /// When a game file disappears while we read its identity, we do not
    /// report a full save folder.
    #[test]
    fn a_game_file_that_has_gone_is_named() {
        let root = rominabox_scratch::Scratch::dir("rominabox-identity");
        let error = stable_identity(&root.join("Sonic.md"), "megadrive", None).unwrap_err();
        assert_eq!(error.stage, ErrorStage::Missing);
        assert!(error.sentence().contains("\u{201c}Sonic.md\u{201d}"), "{}", error.sentence());
    }

    #[test]
        fn identity_is_stable_for_the_same_rom_and_system() {
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
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
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
            // We compare with a fixed value and not with a second computation,
            // because with both sides computed, a changed hash would go unnoticed.
            let identity = stable_identity(&rom, "megadrive", None).unwrap();
            assert_eq!(identity.len(), 24);
            assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
            assert_eq!(identity, "0d17b49ea458b50bd16ea900");
        }

        /// Two worktrees must not use the same data folder for a game.
        ///
        /// With a shared folder, a launch from one worktree rewrites
        /// retroarch.cfg under the running game of the other, both write to
        /// one launch.log, and a screenshot can show the wrong build.
        #[test]
        fn a_namespace_gives_the_same_game_a_separate_home() {
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
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
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
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
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
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
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
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
            let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
            let (_other, other_rom) = rom_with(b"rominabox-identity-fixture-2");
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

        /// A kit containing exactly the named cores and licence texts.
        /// A kit prepared on the machine running the tests.
        fn kit(cores: &[(&str, bool, bool)]) -> rominabox_scratch::Scratch {
            kit_for(crate::systems::current_target(), cores)
        }

        /// A kit prepared for `target`, whatever machine runs the tests.
        fn kit_for(target: &str, cores: &[(&str, bool, bool)]) -> rominabox_scratch::Scratch {
            let root = rominabox_scratch::Scratch::dir("rominabox-availability");
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
                            .join(core.artifact_for(target).expect("an artifact for this target")),
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
        fn a_core_in_the_first_boot_cache_is_enough() {
            let kit_root = kit(&[]);
            let cache = kit(&[("megadrive", true, true)]);
            let megadrive =
                system_availability_in(&kit_root, Some(&cache), crate::systems::current_target())
                    .into_iter()
                    .find(|entry| entry.id == "megadrive")
                    .expect("every declared console is reported");
            assert_eq!(megadrive.unavailable, None);
            assert_eq!(megadrive.component.as_deref(), Some("genesis_plus_gx"));
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

        /// A declared target whose file is not in this kit is a missing file.
        /// A target missing from the declaration of the component is another
        /// problem, and the report must make clear which of the two it is.
        #[test]
        fn a_console_with_no_artifact_for_a_target_says_exactly_that() {
            let root = kit_for("macos-arm64", &[("megadrive", true, true)]);
            let windows = system_availability_for(&root, "windows-x86_64")
                .into_iter()
                .find(|entry| entry.id == "megadrive")
                .expect("every console is reported for every target");
            match windows.unavailable {
                Some(Unavailable::NoPreparedCore { ref tried }) => assert!(
                    tried[0].contains("artifact genesis_plus_gx_libretro.dll missing"),
                    "windows is declared, so a macOS kit is missing the file: {tried:?}"
                ),
                other => panic!("expected a missing windows artifact, got {other:?}"),
            }
            let undeclared = system_availability_for(&root, "linux-arm64")
                .into_iter()
                .find(|entry| entry.id == "megadrive")
                .expect("every console is reported for every target");
            match undeclared.unavailable {
                Some(Unavailable::NoPreparedCore { ref tried }) => assert!(
                    tried[0].contains("no linux-arm64 artifact declared"),
                    "a target with nothing declared is a different problem from a missing file: {tried:?}"
                ),
                other => panic!("expected an undeclared target, got {other:?}"),
            }
        }

        #[test]
        fn the_same_kit_resolves_for_the_target_it_was_built_for() {
            let root = kit_for("macos-arm64", &[("megadrive", true, true)]);
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
