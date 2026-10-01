//! What we tell the launcher of an exported game: the launch plan, its
//! isolated runtime config, and the game identity, for its data folder.

use super::app_files::player_defaults;
use super::{Drivers, ErrorStage, ExportError, ExportRequest};
use crate::hotkeys::isolated_hotkey_config;
use crate::launch_contract::{plan_field, plan_mark, shipped, token, user_folder};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// An optional namespace for everything that belongs to an export.
///
/// The same game built in two checkouts has the same identity on purpose. It
/// is `sha256(system + title + ROM bytes)`, so a player keeps the saves when
/// the author exports the game again. Two development checkouts that build
/// in parallel would then share a bundle identifier and a data directory.
///
/// So for isolation, we take a namespace from the environment and never
/// change the identity itself. When there is none, as in every published
/// export, the identity is exactly the hash above and the save path stays.
pub(super) fn isolation_namespace() -> Option<String> {
    std::env::var("ROMINABOX_GAME_BUNDLE_PREFIX")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The identity of a game, from which we name its data folder, its sandbox and its
/// app ID: the console, the title and the ROM's bytes. When the author
/// exports the same game again with the same title, the player keeps the
/// saves, and the same ROM under another title is a separate game.
///
/// We take the namespace as an argument instead of reading the environment
/// here, so we can test it without changing process-wide state that every
/// other test in this binary shares.
pub(super) fn stable_identity(
    rom: &Path,
    system: &str,
    title: &str,
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
    hash.update(title.trim().as_bytes());
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

pub(super) fn isolated_runtime_config(request: &ExportRequest) -> Result<String, ExportError> {
    // We choose the video driver by the shader language of the game.
    let video = crate::shaders::video_driver(&request.game.shaders)
        .map_err(|message| ExportError::new(ErrorStage::Configure, message))?
        .name();
    let menu_driver = if request.game.show_menu || request.game.splash {
        "rmlui"
    } else {
        "null"
    };
    // We keep the game's audio running in the menu, so the player hears a
    // change of volume, and play pack cues only when the export has a pack.
    let menu_audio = request.game.show_menu;
    let menu_sounds = request.game.show_menu && request.game.menu_sounds != "off";
    // We set both halves with one option. The file is `<savestate>.auto`, not
    // a numbered pause-menu slot, so Save and Load on that row are unchanged.
    let autosave = if request.game.autosave_on_quit {
        "true"
    } else {
        "false"
    };
    let (data, resources) = (token!(DataDir), token!(ResourcesDir));
    let Drivers { audio, joypad, .. } = request.game.target.drivers();
    let assets = if menu_sounds {
        format!("{resources}/assets")
    } else {
        format!("{data}/assets")
    };
    Ok(format!(
        r#"video_driver = "{video}"
audio_driver = "{audio}"
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
input_joypad_driver = "{joypad}"
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
        isolated_hotkey_config(request.game.advanced_emulator_access),
        firmware = shipped!(Firmware).1,
        remaps = shipped!(Remaps).1,
        core_options = shipped!(CoreOptions).1,
        autoconfig = shipped!(Autoconfig).1,
    ))
}

/// The game's data folder, one of the folders in the games folder, under
/// the per-user data folder.
fn game_data_template(identity: &str) -> String {
    format!("{}/{}", token!(UserData), game_data_folder(identity))
}

/// The game's data folder, below the per-user data folder.
pub(super) fn game_data_folder(identity: &str) -> String {
    format!("{}/{identity}", user_folder!(Games))
}

/// Where we unpack a game made into one program (a Windows game,
/// `windows_pack`), under the per-user data folder.
pub(super) fn runtime_folder(identity: &str) -> String {
    format!("{}/{identity}", user_folder!(Runtimes))
}

/// The shared QUICK SIGN IN folder for this export, when it has achievements.
pub(super) fn accounts_folder(request: &ExportRequest) -> Result<Option<String>, ExportError> {
    if !crate::achievements::included(request.game.include_achievements, request.game.show_menu) {
        return Ok(None);
    }
    let named = std::env::var("ROMINABOX_ACCOUNTS_FOLDER").ok();
    crate::achievements::accounts_folder(isolation_namespace().as_deref(), named.as_deref())
        .map(Some)
        .map_err(|message| ExportError::new(ErrorStage::Configure, message))
}

pub(super) fn write_launch_plan(
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
    if request.game.title.contains(['\n', '\t']) {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the game title cannot be written into the launch plan",
        ));
    }
    let shader_initial = if request.game.show_menu {
        crate::shaders::launch_preset(&request.game.shaders)
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
        crate::achievements::included(request.game.include_achievements, request.game.show_menu);
    let mut plan = String::from("rominabox-launch\t1\n");
    plan += &line(plan_field!(Identity), identity);
    plan += &line(plan_field!(Content), &content);
    plan += &line(plan_field!(Title), &request.game.title);
    plan += &line(plan_field!(StartAtMenu), flag(request.game.start_at_menu));
    plan += &line(
        plan_field!(Advanced),
        flag(request.game.advanced_emulator_access),
    );
    plan += &line(plan_field!(Achievements), flag(achievements));
    plan += &line(plan_field!(Sandbox), flag(true));
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
    plan += &isolated_runtime_config(request)?;
    fs::write(path, plan).map_err(|error| ExportError::io(ErrorStage::Configure, path, error))
}
