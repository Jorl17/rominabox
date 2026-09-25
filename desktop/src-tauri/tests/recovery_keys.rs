//! Only Escape is reserved. Q and F are ordinary gameplay keys in every mode.
//!
//! Neither Q nor F is a hotkey, because the menu opened with Escape has Quit,
//! and Alt+Enter is the fullscreen chord.
//!
//! In these tests we read generated config and call the authoring validator.
//! We do not launch a player or open a window, and we do not prove that
//! RetroArch performs the action bound to a key.

use rominabox_desktop::{
    controls::{self, Controls},
    hotkeys::{isolated_hotkey_config, HOTKEY_BINDS},
    packaging::{ExportRequest, ExportTarget},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

fn write_runtime_stub(path: &Path) {
    let source = path.with_extension("c");
    fs::write(
        &source,
        "int rarch_main(int c, char **v, void *d){(void)c;(void)v;(void)d;return 0;}\nint main(void){return rarch_main(0,0,0);}\n",
    )
    .unwrap();
    let status = Command::new("cc")
        .args(["-Oz", "-Wl,-headerpad_max_install_names", "-o"])
        .arg(path)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "could not compile the runtime stub");
}

fn workspace() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-recovery")
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap().filter_map(Result::ok) {
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn fixture_kit(root: &Path) -> PathBuf {
    let kit = root.join("runtime-kit");
    fs::create_dir_all(kit.join("bin")).unwrap();
    fs::create_dir_all(kit.join("cores")).unwrap();
    fs::create_dir_all(kit.join("Frameworks")).unwrap();
    fs::create_dir_all(kit.join("licenses")).unwrap();
    fs::create_dir_all(kit.join("licenses/native")).unwrap();
    fs::create_dir_all(kit.join("provenance/native-rmlui")).unwrap();
    write_runtime_stub(&kit.join("bin/retroarch"));
    fs::write(kit.join("cores/genesis_plus_gx_libretro.dylib"), b"core").unwrap();
    for name in [
        "RetroArch.txt",
        "NATIVE-DEPENDENCIES.txt",
        "RmlUi-MIT.txt",
        "genesis_plus_gx.txt",
    ] {
        fs::write(kit.join("licenses").join(name), name).unwrap();
    }
    fs::write(
        kit.join("runtime-dependencies.json"),
        r#"{"formatVersion":1,"files":[]}"#,
    )
    .unwrap();
    fs::write(
        kit.join("manifest.json"),
        r#"{"schema_version":1,"components":[{"name":"RetroArch"},{"name":"RmlUi"},{"name":"genesis_plus_gx"}]}"#,
    )
    .unwrap();
    let design = rominabox_desktop::repo::at("integrations/designs/native");
    copy_tree(&design, &kit.join("designs/native"));
    copy_tree(
        &rominabox_desktop::repo::at("integrations/parts"),
        &kit.join("parts"),
    );
    fs::create_dir_all(kit.join("menu-assets")).unwrap();
    // We read the scene template for the controls from the kit's shared
    // menu-assets, not from the folder of the selected design.
    fs::copy(design.join("menu.rml"), kit.join("menu-assets/menu.rml")).unwrap();
    // Every pad that the player can choose in the picker, because an export
    // contains them all. Without a drawing for each, the player could choose a
    // pad, such as the six-button Mega Drive, that we would never show.
    for entry in rominabox_desktop::controls::variants_for_system("megadrive").unwrap() {
        if !entry.image.is_empty() {
            fs::write(kit.join("menu-assets").join(&entry.image), []).unwrap();
        }
    }
    fs::write(kit.join("menu-assets/CONTROLLERS.txt"), []).unwrap();
    kit
}

fn export_request(root: &Path, advanced: bool, controls: Controls) -> ExportRequest {
    let rom = root.join("sonic.bin");
    fs::write(&rom, b"RIBtest").unwrap();
    ExportRequest {
        rom,
        title: "Recovery Keys".to_string(),
        system: "megadrive".to_string(),
        description: None,
        icon: None,
        background: None,
        show_menu: true,
        start_at_menu: false,
        theme: "native".to_string(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls,
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: advanced,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        include_achievements: false,
        output_dir: root.join("out"),
        replace: false,
        target: ExportTarget::Macos,
        runtime_kit: fixture_kit(root),
        core: None,
        core_cache: None,
    }
}

fn embedded_runtime_config(plan: &str) -> String {
    let marker = "---config---\n";
    let start = plan
        .find(marker)
        .expect("exported launch plan contains the runtime config");
    plan[start + marker.len()..].to_string()
}

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

/// The runtime config and the default controls that we write in an export.
fn export(advanced: bool, controls: Controls) -> (String, String) {
    let root = workspace();
    let request = export_request(&root, advanced, controls);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {})
        .unwrap_or_else(|error| panic!("advanced={advanced}: export refused: {error:?}"));
    let resources = result.app_path.join("Contents/Resources");
    let plan = fs::read_to_string(resources.join("launch.plan")).unwrap();
    let defaults = fs::read_to_string(resources.join("menu-assets/controls-defaults.cfg")).unwrap();
    (embedded_runtime_config(&plan), defaults)
}

fn exported_config(advanced: bool) -> String {
    export(advanced, Controls::default()).0
}

fn binding(key: &str) -> Controls {
    serde_json::from_value(serde_json::json!({
        "bindings": {"a": {"key": key}}
    }))
    .unwrap()
}

/// A on Q and B on F.
fn q_and_f() -> Controls {
    serde_json::from_value(serde_json::json!({
        "bindings": {"a": {"key": "q"}, "b": {"key": "f"}}
    }))
    .unwrap()
}

/// Every RetroArch hotkey whose keyboard key is `key`.
fn hotkeys_on<'a>(config: &'a str, key: &str) -> Vec<&'static str> {
    HOTKEY_BINDS
        .iter()
        .map(|bind| bind.name)
        .filter(|name| config_value(config, &format!("input_{name}")) == Some(key))
        .collect()
}

/// In a default export we do not bind Q to quit, because a player could press
/// it by accident and lose the session. Quit is in the menu that Escape opens.
///
/// We read the launcher that an export writes. This does not prove that Quit
/// in the menu works, or that RetroArch receives a keypress.
#[test]
#[cfg(target_os = "macos")]
fn a_default_export_binds_no_exit_key() {
    let config = exported_config(false);
    assert_eq!(
        config_value(&config, "input_exit_emulator"),
        Some("nul"),
        "a shipped game must not quit on Q"
    );
    assert_eq!(
        config_value(&config, "input_menu_toggle"),
        Some("escape"),
        "Escape is how the player reaches Quit"
    );
}

/// Q and F are gameplay keys in every mode. With advanced emulator access on
/// or off, we accept an export with A on Q and B on F, its controls contain
/// both keys, and no RetroArch hotkey is bound to either, so pressing either
/// key only acts in the game.
///
/// If we bound Q to quit and F to fullscreen with advanced access, a player of
/// an advanced export could not use them, or would quit the game mid-play.
///
/// We read what an export writes. This does not prove that RetroArch passes a
/// keypress to the core.
#[test]
#[cfg(target_os = "macos")]
fn q_and_f_are_gameplay_keys_and_no_hotkey_in_every_mode() {
    for advanced in [false, true] {
        let (config, defaults) = export(advanced, q_and_f());
        assert_eq!(
            config_value(&defaults, "input_player1_a"),
            Some("q"),
            "advanced={advanced}"
        );
        assert_eq!(
            config_value(&defaults, "input_player1_b"),
            Some("f"),
            "advanced={advanced}"
        );
        for key in ["q", "f"] {
            assert_eq!(
                hotkeys_on(&config, key),
                Vec::<&str>::new(),
                "advanced={advanced}: {key} is a gameplay key and no hotkey"
            );
        }
        assert_eq!(config_value(&config, "input_exit_emulator"), Some("nul"));
        assert_eq!(
            config_value(&config, "input_toggle_fullscreen"),
            Some("nul")
        );
        assert_eq!(
            config_value(&config, "input_menu_toggle"),
            Some("escape"),
            "advanced={advanced}: Escape opens the menu, which has Quit"
        );
    }
}

/// Escape toggles the menu whether or not advanced access is on. We reject it
/// as a gameplay binding in both modes for the same reason: it is the only way
/// into that menu.
///
/// The keyboard lines come from the hotkey policy in the launcher. This does
/// not prove that a keypress opens the menu. In the export tests above we read
/// a written launcher.
#[test]
fn escape_toggles_the_menu_in_both_modes_and_is_never_a_gameplay_key() {
    for advanced in [false, true] {
        let config = isolated_hotkey_config(true, advanced);
        assert_eq!(
            config_value(&config, "input_menu_toggle"),
            Some("escape"),
            "advanced={advanced}"
        );
        let error = controls::validate_for_system_with_advanced_access(
            "megadrive",
            &binding("escape"),
            advanced,
        )
        .expect_err("escape stays reserved");
        assert!(
            error.contains("toggles the menu"),
            "advanced={advanced}: {error}"
        );
    }
}
