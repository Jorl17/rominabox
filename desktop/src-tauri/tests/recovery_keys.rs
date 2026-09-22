//! We ship quit and fullscreen keys only with advanced emulator access.
//!
//! The quit binding and the fullscreen key are part of the same opt-in as the
//! stock RetroArch menus. Escape is the menu toggle in both modes, because
//! the player quits a shipped game through the menu.
//!
//! In these tests we read generated config and call the authoring validator.
//! We do not launch a player or open a window, and we do not prove that
//! RetroArch performs the action bound to a key.

use rominabox_desktop::{
    controls::{self, Controls},
    packaging::{isolated_hotkey_config, ExportRequest, ExportTarget},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
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

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-recovery-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
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
    let design =
        rominabox_desktop::repo::at("integrations/designs/native");
    copy_tree(&design, &kit.join("designs/native"));
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

fn export_request(root: &Path, advanced: bool) -> ExportRequest {
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
        controls: Controls::default(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: advanced,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        achievements: Default::default(),
        output_dir: root.join("out"),
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

fn exported_config(advanced: bool) -> String {
    let root = workspace();
    let request = export_request(&root, advanced);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    let plan = result.app_path.join("Contents/Resources/launch.plan");
    embedded_runtime_config(&fs::read_to_string(&plan).unwrap())
}

fn binding(key: &str) -> Controls {
    serde_json::from_value(serde_json::json!({
        "bindings": {"a": {"key": key}}
    }))
    .unwrap()
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

/// Advanced emulator access is the opt-in for stock menus and the advanced
/// hotkey tier. Quit is part of it, and not of every shipped game.
///
/// We read the launcher. This does not prove that RetroArch quits on Q.
#[test]
#[cfg(target_os = "macos")]
fn an_advanced_export_binds_quit() {
    let config = exported_config(true);
    assert_eq!(config_value(&config, "input_exit_emulator"), Some("q"));
    assert_eq!(
        config_value(&config, "input_menu_toggle"),
        Some("escape"),
        "advanced access does not take Escape away from the menu"
    );
}

/// F goes with Q. With `input_toggle_fullscreen = f` while f is a legal
/// gameplay key, one press would trigger the hotkey and the bind together. The
/// macOS window menu still has Full Screen, so a default export has nul.
///
/// We read the launcher. This does not prove that the window menu item exists.
#[test]
#[cfg(target_os = "macos")]
fn fullscreen_moves_with_quit() {
    let ordinary = exported_config(false);
    let advanced = exported_config(true);
    assert_eq!(
        config_value(&ordinary, "input_toggle_fullscreen"),
        Some("nul")
    );
    assert_eq!(
        config_value(&advanced, "input_toggle_fullscreen"),
        Some("f")
    );
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

/// Q and F are reserved only while recovery hotkeys are on. With them off,
/// both are ordinary gameplay keys, so a stick can use f again.
///
/// We call the authoring validator. This does not prove that the capture UI of
/// the builder accepts the key, because that UI reserves keys separately.
#[test]
fn a_gameplay_binding_to_q_or_f_follows_advanced_access() {
    for key in ["q", "f"] {
        let reserved =
            controls::validate_for_system_with_advanced_access("megadrive", &binding(key), true)
                .expect_err("recovery on reserves the hotkey");
        assert!(
            reserved.contains("reserved for player recovery"),
            "{key}: {reserved}"
        );
        controls::validate_for_system_with_advanced_access("megadrive", &binding(key), false)
            .unwrap_or_else(|error| panic!("{key} must be bindable when recovery is off: {error}"));
        controls::validate_for_system("megadrive", &binding(key)).unwrap_or_else(|error| {
            panic!("the shipped-game validator must accept {key}: {error}")
        });
    }
}
