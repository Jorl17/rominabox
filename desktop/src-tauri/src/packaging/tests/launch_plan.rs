use super::*;
use crate::meta_binds::{isolated_meta_bind_config, META_BINDS};
use crate::packaging::launch_plan::isolated_runtime_config;

#[test]
fn launcher_quotes_hostile_content_filename_as_data() {
    let directory = rominabox_scratch::Scratch::dir("rominabox-launcher");
    let launcher = directory.join("launcher");
    let hostile = OsStr::new("content/weird'$(touch PWNED)`echo nope`.bin");
    let mut settings = request(false);
    settings.game.title = "Game's $(title) `literal`".into();
    write_launch_plan(&launcher, "identity", hostile, &settings).unwrap();
    let plan = fs::read_to_string(launcher).unwrap();

    assert!(plan.contains("content\tcontent/weird'$(touch PWNED)`echo nope`.bin\n"));
    assert!(plan.contains("title\tGame's $(title) `literal`\n"));
    assert!(!plan.contains("Resources/content/weird"));
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
    // The player can change whether the game keeps playing in the background.
    // We export only the default, which we apply in the launcher until the
    // player chooses. If we froze the setting, the choice would have no effect.
    assert_eq!(config_value(&off, "pause_nonactive"), None);
    assert!(
        plan.contains("player_setting\tbackground-play.cfg\tpause_nonactive\ttrue\n"),
        "{plan}"
    );

    settings.game.keep_playing_in_background = true;
    settings.game.autosave_on_quit = true;
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

/// We keep the game's audio running in the menu, so the player hears a
/// change of volume. With menu sounds Off we play none of the cues of the
/// pack, and with a pack we play them all.
#[test]
fn the_menu_keeps_audio_for_the_volume_and_the_pack_decides_its_cues() {
    let mut settings = request(false);
    settings.game.show_menu = true;
    let off = embedded_runtime_config(&write_test_launcher(settings.clone()));
    assert_eq!(config_value(&off, "audio_enable_menu"), Some("true"));
    for cue in ["ok", "cancel", "scroll"] {
        assert_eq!(
            config_value(&off, &format!("audio_enable_menu_{cue}")),
            Some("false"),
            "menu sounds Off plays no {cue} cue"
        );
    }
    settings.game.menu_sounds = "blip".into();
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
    settings.game.show_menu = true;
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
        "no RetroArch meta bind may keep the default Space binding:\n{config}"
    );
}

#[test]
fn advanced_emulator_access_reaches_fast_forward_without_dropping_a_bind() {
    let advanced_tier: Vec<_> = META_BINDS
        .iter()
        .filter(|bind| bind.advanced_key.is_some())
        .map(|bind| (bind.name, bind.advanced_key.unwrap()))
        .collect();
    // Quit and fullscreen are not in this tier, because q and f are gameplay
    // keys in every mode. Every other key is nul, so in a default export we
    // write nul for both fast-forward keys.
    assert_eq!(
        advanced_tier,
        vec![("toggle_fast_forward", "space"), ("hold_fast_forward", "l")],
        "the advanced tier is fast-forward; a default export writes nul for both"
    );

    let mut ordinary = request(false);
    ordinary.game.show_menu = true;
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
    // The player quits from the menu and goes fullscreen with Alt+Enter, so
    // q and f remain gameplay keys.
    assert_eq!(
        config_value(&ordinary_config, "input_exit_emulator"),
        Some("nul")
    );
    assert_eq!(
        config_value(&ordinary_config, "input_toggle_fullscreen"),
        Some("nul")
    );

    let mut advanced = request(false);
    advanced.game.show_menu = true;
    advanced.game.advanced_emulator_access = true;
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
    // The player opens the menu through MENU CONTROLS, not the RetroArch toggle.
    assert_eq!(config_value(&config, "input_menu_toggle"), Some("nul"));
    // Advanced access does not bind q or f either.
    assert_eq!(config_value(&config, "input_exit_emulator"), Some("nul"));
    assert_eq!(
        config_value(&config, "input_toggle_fullscreen"),
        Some("nul")
    );
    assert_eq!(config_value(&config, "input_rewind"), Some("nul"));
    assert_eq!(
        config_value(&config, "input_menu_toggle_gamepad_combo"),
        Some("0")
    );
    assert_eq!(config_value(&config, "input_quit_gamepad_combo"), Some("0"));
    assert!(config.contains(&isolated_meta_bind_config(true)));
}

/// The player still opens the menu with Escape and quits a game from it.
/// Escape is the default for MENU in MENU CONTROLS. The RetroArch menu toggle
/// and its gamepad combo are off, so only the bindings of the player open the
/// menu. Quit and fullscreen have no key, so Q cannot quit in the middle of a
/// game, and q and f stay free for gameplay.
#[test]
fn escape_stays_the_menu_and_quit_and_fullscreen_have_no_key() {
    use crate::menu_controls::{Action, Binding};
    assert!(crate::builder::unstated::menu_controls()
        .of(Action::Menu)
        .contains(&Binding::Key("escape".into())));
    let mut with_menu = request(false);
    with_menu.game.show_menu = true;
    let menu_config = embedded_runtime_config(&write_test_launcher(with_menu));
    assert_eq!(
        config_value(&menu_config, "input_menu_toggle"),
        Some("nul")
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
        Some("0")
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

#[test]
fn isolated_config_points_mutable_paths_at_the_managed_data_dir() {
    let config = isolated_runtime_config(&request(false)).unwrap();
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
fn each_platform_player_is_told_its_own_drivers() {
    for (target, audio, joypad) in [
        (ExportTarget::Macos, "coreaudio", "hid"),
        (ExportTarget::Windows, "wasapi", "xinput"),
    ] {
        let mut value = request(false);
        value.game.target = target.clone();
        let config = isolated_runtime_config(&value).unwrap();
        assert_eq!(config_value(&config, "audio_driver"), Some(audio));
        assert_eq!(config_value(&config, "input_joypad_driver"), Some(joypad));
        assert!(
            target.drivers().joypad_profiles.iter().any(|folder| folder == joypad),
            "{target:?} must ship profiles for the driver it is told to use"
        );
    }
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
    // We play the logo exactly when we staged its file.
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
    assert!(!request.game.advanced_emulator_access);
}

#[test]
fn launcher_sets_advanced_emulator_access_explicitly() {
    let off = write_test_launcher(request(false));
    assert!(off.contains("advanced\t0\n"));
    assert!(!off.contains("advanced\t1\n"));

    let mut on = request(false);
    on.game.advanced_emulator_access = true;
    let plan = write_test_launcher(on);
    assert!(plan.contains("advanced\t1\n"));
    assert!(!plan.contains("advanced\t0\n"));
}

/// We choose the video driver by the shader language of the game: glcore for
/// a slang shader, and gl for GLSL and for a game with no shaders.
#[test]
fn a_slang_game_runs_glcore_and_every_other_game_gl() {
    let folder = rominabox_scratch::Scratch::dir("rominabox-video-driver");
    let shader = |file: &str, text: &str| {
        fs::write(folder.join(file), text).unwrap();
        crate::shaders::ShaderSelection {
            custom: vec![crate::shaders::CustomShader {
                name: "CRT".into(),
                path: folder.join(file),
            }],
            ..crate::shaders::ShaderSelection::default()
        }
    };
    let slang = shader(
        "crt.slang",
        "#version 450\n#pragma stage vertex\nvoid main() {}\n#pragma stage fragment\nvoid main() {}\n",
    );
    let glsl = shader(
        "crt.glsl",
        "#if defined(VERTEX)\nvoid main() {}\n#elif defined(FRAGMENT)\nvoid main() {}\n#endif\n",
    );
    for (shaders, driver) in [
        (slang, "glcore"),
        (glsl, "gl"),
        (crate::shaders::ShaderSelection::default(), "gl"),
    ] {
        let mut value = request(false);
        value.game.show_menu = true;
        value.game.shaders = shaders;
        let plan = write_test_launcher(value);
        let config = embedded_runtime_config(&plan);
        assert_eq!(config_value(&config, "video_driver"), Some(driver));
        assert_eq!(config.matches("video_driver").count(), 1);
    }
    let _ = fs::remove_dir_all(&folder);
}

/// We name the files of a Mac game with "/" between their parts on every
/// system. On Windows, "\\" separates the parts of a path, but on a Mac it is
/// part of a name, so the launcher would not find the game's cartridge. For
/// a Windows game, we spell the paths as on the builder's system.
#[test]
fn a_mac_games_files_are_named_with_slashes_on_any_builder() {
    let disc = Path::new("content").join("disc one").join("Track 01.bin");
    assert_eq!(launch_path(&ExportTarget::Macos, &disc), "content/disc one/Track 01.bin");
    assert_eq!(launch_path(&ExportTarget::Windows, &disc), disc.to_string_lossy());
}
