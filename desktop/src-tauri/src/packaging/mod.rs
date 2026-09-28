//! Assembly of a self-contained native game.
//!
//! Export is blocking on purpose. Desktop callers should run it with
//! `tauri::async_runtime::spawn_blocking` and use the callback for progress.
//!
//! The export itself is here. What we write alike for every platform is in
//! `app_files` and `launch_plan`, and each packager is in a file named after
//! its platform (`macos`, `windows`). The other files contain the consoles a
//! kit can export (`availability`), the core files a game ships and their
//! source (`export_core`), and the processors of a Mac app (`slices`).

mod app_files;
mod availability;
mod export_core;
mod launch_plan;
mod macos;
mod macos_minimum;
mod slices;
mod windows;
#[cfg(test)]
mod tests;

use crate::target::Target;
use crate::launch_contract::{app_file, shipped};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::content;
use crate::controls;
pub use crate::export_error::{ErrorStage, ExportError};

use app_files::{
    copy_content_file, firmware_destination_name, stage_bundled_autoconfig,
    stage_controller_remap, stage_firmware, stage_legal_materials, stage_pixel_options, tree_size,
};
pub use app_files::{menu_request, stage_menu};
pub use availability::{
    available_systems, system_availability, system_availability_for, system_availability_in,
    SystemAvailability, Unavailable,
};
use export_core::{export_core, prepare_core, resolve_cached, shipped_cores, ExportCore};
use launch_plan::{isolation_namespace, stable_identity, write_launch_plan};
pub use launch_plan::MANAGED_DATA_DIRECTORIES;
use macos::MacosPackager;
pub use macos::freeze_macos_executable;
use windows::WindowsPackager;

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

impl ExportTarget {
    /// The platform for exports from a builder on the host, or `None` on a
    /// host that we do not build for.
    pub fn of_host() -> Option<ExportTarget> {
        Target::host().map(ExportTarget::of_target)
    }

    /// The platform a target is on.
    pub fn of_target(target: Target) -> ExportTarget {
        match target {
            Target::MacosArm64 | Target::MacosX86_64 => ExportTarget::Macos,
            Target::WindowsX86_64 => ExportTarget::Windows,
        }
    }

    /// The target this export is for, or `None` when there is none.
    ///
    /// For a Windows package we download the Windows core, even on a Mac. For
    /// a Mac package we download the Mac entry that matches the runtime in it.
    /// We build that runtime for this machine and do not cross-compile it.
    pub fn target(&self) -> Option<Target> {
        match self {
            ExportTarget::Windows => Some(Target::WindowsX86_64),
            ExportTarget::Macos => match std::env::consts::ARCH {
                "aarch64" => Some(Target::MacosArm64),
                "x86_64" => Some(Target::MacosX86_64),
                _ => None,
            },
        }
    }

    /// Every target that we build an app for on this platform: the platform's
    /// target (`target`), and Intel Macs too when a Mac game also runs on them.
    /// The author chooses `intel_macs` for a Mac game. A Windows game has one.
    pub fn targets(&self, intel_macs: bool) -> Option<Vec<Target>> {
        let own = self.target()?;
        Some(match (self, own) {
            (ExportTarget::Macos, Target::MacosArm64) if intel_macs => {
                vec![own, Target::MacosX86_64]
            }
            (ExportTarget::Macos | ExportTarget::Windows, _) => vec![own],
        })
    }

    /// The drivers that we tell the player to use on this platform, which we
    /// declare with the player's build.
    pub fn drivers(&self) -> Drivers {
        let platform = serde_json::to_value(self).expect("a platform names itself");
        let declared = &player_recipe()["drivers"][platform.as_str().expect("a platform is a word")];
        serde_json::from_value(declared.clone())
            .unwrap_or_else(|error| panic!("the player recipe declares no drivers for {platform}: {error}"))
    }
}

/// How we build and run the player, per target and platform, as declared in
/// `scripts/native_runtime/player-recipe.json`.
fn player_recipe() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../scripts/native_runtime/player-recipe.json"))
        .expect("the player recipe parses")
}

/// The menu preview renderer in the builder's resources on `target`, with the
/// program name from the player recipe, or an error for a target for which
/// we build none.
pub fn preview_renderer(target: Target) -> Result<String, String> {
    player_recipe()["preview"][target.key()]["output"]
        .as_str()
        .map(|name| format!("preview/{name}"))
        .ok_or_else(|| format!("No menu preview is built for {target}."))
}

/// Where the file with `role` (`player`, `launcher`) is in `target`'s kit.
fn kit_file(target: Target, role: &str) -> PathBuf {
    let declared = &player_recipe()["kit"][target.key()]["files"][role]["at"];
    PathBuf::from(
        declared
            .as_str()
            .unwrap_or_else(|| panic!("the player recipe puts no {role} in the {target} kit")),
    )
}

/// The oldest system on which the player for `platform`, and what we build
/// next to it at export, run, as declared in the player recipe.
fn deployment_target(platform: ExportTarget) -> String {
    let name = serde_json::to_value(platform).expect("a platform names itself");
    player_recipe()["deploymentTarget"][name.as_str().expect("a platform is a word")]
        .as_str()
        .unwrap_or_else(|| panic!("the player recipe declares no deployment target for {name}"))
        .to_string()
}

/// The libraries every machine of `target`'s platform has, in lower case.
fn system_libraries(target: Target) -> Vec<String> {
    let platform = serde_json::to_value(
        ExportTarget::of_target(target),
    )
    .expect("a platform names itself");
    let declared = &player_recipe()["systemLibraries"][platform.as_str().expect("a platform is a word")];
    serde_json::from_value(declared.clone())
        .unwrap_or_else(|error| panic!("the player recipe lists no system libraries for {target}: {error}"))
}

/// What we tell the player to use on a platform: its audio and controller
/// drivers, and the controller profile folders for them in RetroArch.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drivers {
    pub audio: String,
    pub joypad: String,
    pub joypad_profiles: Vec<String>,
}

/// For a setting missing from a request we use the builder default
/// (`crate::builder::defaults`), the same one that a first draft starts from.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub rom: PathBuf,
    pub title: String,
    pub system: String,
    pub description: Option<String>,
    pub icon: Option<PathBuf>,
    pub background: Option<PathBuf>,
    #[serde(default = "crate::builder::unstated::show_menu")]
    pub show_menu: bool,
    #[serde(default = "crate::builder::unstated::start_at_menu")]
    pub start_at_menu: bool,
    #[serde(default = "crate::builder::unstated::theme")]
    pub theme: String,
    #[serde(default = "crate::builder::unstated::palette")]
    pub palette: String,
    #[serde(default = "crate::builder::unstated::menu_sounds")]
    pub menu_sounds: String,
    /// The author's defaults, which we never change. We store the player's
    /// changes separately, in the managed data folder of the exported game.
    #[serde(default)]
    pub controls: controls::Controls,
    /// The inputs to open the menu, and to confirm and go back in it, until
    /// the player changes them on MENU CONTROLS. For an action left out, we
    /// use the builder's default.
    #[serde(default = "crate::builder::unstated::menu_controls")]
    pub menu_controls: crate::menu_controls::MenuControls,
    /// The firmware files the author chose. On export we never look for
    /// firmware in global RetroArch locations.
    #[serde(default)]
    pub firmware: Vec<PathBuf>,
    /// Include the short native in-player splash and its logo asset.
    #[serde(default = "crate::builder::unstated::splash")]
    pub splash: bool,
    /// Restore stock RetroArch native menus in the exported app.
    #[serde(default = "crate::builder::unstated::advanced_emulator_access")]
    pub advanced_emulator_access: bool,
    /// A Mac game also runs on Intel Macs. Ignored for a Windows game.
    #[serde(default = "crate::builder::unstated::intel_macs")]
    pub intel_macs: bool,
    /// Keep emulating when the window does not have the focus. RetroArch's
    /// `pause_nonactive` is the opposite of this. We write it into the frozen
    /// config, as we do quit-autosave, because the player has no control for
    /// it and a per-game `controls.cfg` would otherwise replace it.
    #[serde(default = "crate::builder::unstated::keep_playing_in_background")]
    pub keep_playing_in_background: bool,
    /// Save on quit and load that save the next time the player opens the
    /// game. The author makes one choice for both.
    #[serde(default = "crate::builder::unstated::autosave_on_quit")]
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
    #[serde(default = "crate::builder::unstated::include_achievements")]
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
    let targets = request.target.targets(request.intel_macs).ok_or_else(|| {
        ExportError::new(
            ErrorStage::Refused,
            "The builder does not make games on this kind of computer.",
        )
    })?;
    let resolved = export_core(request, &targets)?;
    prepare_core(request, resolved.as_ref(), &mut progress, transport)?;
    emit(
        &mut progress,
        ExportStage::Validate,
        0.02,
        "Checking export inputs",
    );
    let mut packager = packager_for(&request.target, &targets);
    validate_request(request, resolved.as_ref(), &targets, &*packager)?;
    check_cancelled(cancelled)?;
    export_app(
        request,
        resolved.as_ref(),
        &targets,
        cancelled,
        &mut progress,
        &mut *packager,
    )
}

/// The packager for an app for `target` that runs on `targets`.
fn packager_for(target: &ExportTarget, targets: &[Target]) -> Box<dyn Packager> {
    match target {
        ExportTarget::Macos => Box::new(MacosPackager::for_targets(targets)),
        ExportTarget::Windows => Box::new(WindowsPackager::default()),
    }
}

/// The parts of an export that differ for each platform. In the export itself
/// (`export_app`) we stage the files of the app the same way for every
/// platform. In a packager we lay out the app, add the player and its
/// dependencies, install the launcher, describe the app to its system and
/// make it runnable.
trait Packager {
    /// Refuse, before staging anything, an app that we cannot make here.
    fn check_host(&self) -> Result<(), ExportError>;
    /// Where the player is in the runtime kit.
    fn player_in_kit(&self) -> PathBuf;
    /// Make the app's folders, and return the folder for the app's files.
    fn lay_out(&mut self, app: &Path) -> Result<PathBuf, ExportError>;
    /// The player, from the runtime kit.
    fn place_player(&mut self, runtime_kit: &Path) -> Result<(), ExportError>;
    /// The core's name among the app's own files.
    fn core_file(&self) -> &'static str;
    /// Put the core at `destination`, from `builds`, the file for each target
    /// of the app. We use `system_name` to name the core to the author.
    fn place_core(
        &mut self,
        builds: &[(Target, PathBuf)],
        destination: &Path,
        system_name: &str,
    ) -> Result<(), ExportError>;
    /// The libraries the player and the core need beside them.
    fn stage_dependencies(
        &mut self,
        runtime_kit: &Path,
        core: &Path,
        cancelled: &AtomicBool,
    ) -> Result<(), ExportError>;
    /// The first program that starts when the game opens, before the player.
    fn install_launcher(&mut self, runtime_kit: &Path) -> Result<(), ExportError>;
    /// The app's description for the platform: its name, identity and icon.
    fn describe(
        &mut self,
        request: &ExportRequest,
        identity: &str,
        staging: &Path,
    ) -> Result<(), ExportError>;
    /// What we tell the author while `finish` runs, when it does anything.
    fn finishing(&self) -> Option<&'static str>;
    /// The last steps to make the app runnable, such as signing.
    fn finish(
        &mut self,
        request: &ExportRequest,
        identity: &str,
        staging: &Path,
        cancelled: &AtomicBool,
    ) -> Result<(), ExportError>;
    /// The bytes of the app that are the runtime rather than the game.
    fn runtime_bytes(&self) -> Result<u64, ExportError>;
}

/// An export into an app, the same on every platform except for the steps
/// in `packager`.
fn export_app<F>(
    request: &ExportRequest,
    resolved: Option<&ExportCore<'_>>,
    targets: &[Target],
    cancelled: &AtomicBool,
    progress: &mut F,
    packager: &mut dyn Packager,
) -> Result<ExportResult, ExportError>
where
    F: FnMut(ExportProgress),
{
    packager.check_host()?;
    let app_name = crate::publish::app_name(&request.target, &request.title);
    let final_app = request.output_dir.join(&app_name);
    fs::create_dir_all(&request.output_dir)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &request.output_dir, error))?;

    let staging = OwnedStaging::create(&request.output_dir)?;
    let app = staging.path().join(&app_name);
    let resources = packager.lay_out(&app)?;

    emit(
        progress,
        ExportStage::Stage,
        0.10,
        "Copying the game runtime",
    );
    packager.place_player(&request.runtime_kit)?;

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
    let core_name = OsStr::new(packager.core_file());
    let core = resources.join(core_name);
    packager.place_core(&shipped_cores(request, resolved, targets), &core, &system.name)?;
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
    packager.stage_dependencies(&request.runtime_kit, &core, cancelled)?;
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
    packager.install_launcher(&request.runtime_kit)?;
    write_launch_plan(
        &resources.join(app_file!(Plan)),
        &identity,
        rom_relative.as_os_str(),
        request,
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
        "core": packager.core_file(),
        "coreSource": resolved.and_then(|export_core| export_core.builds.first()).map_or("", |build| build.artifact_name),
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
    packager.describe(request, &identity, staging.path())?;
    check_cancelled(cancelled)?;

    if let Some(finishing) = packager.finishing() {
        emit(progress, ExportStage::Sign, 0.70, finishing);
    }
    packager.finish(request, &identity, staging.path(), cancelled)?;
    check_cancelled(cancelled)?;

    let installed_bytes = tree_size(&app)?;
    let runtime_bytes = packager.runtime_bytes()?;
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
    targets: &[Target],
    packager: &dyn Packager,
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
    let runtime = request.runtime_kit.join(packager.player_in_kit());
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
    for (_, core) in shipped_cores(request, resolved, targets) {
        if !core.is_file() {
            return Err(ExportError::new(
                ErrorStage::Validate,
                format!("core does not exist: {}", core.display()),
            ));
        }
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
