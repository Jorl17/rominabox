//! We leave joypad autoconfig out on purpose. Fast-forward is an advanced key.
//!
//! These tests check the two facts that decide whether a plugged-in pad can
//! play an exported game, and the hotkey tier beside them. We read generated
//! config and the pinned RetroArch sources. We do not launch a player, open a
//! window, or communicate with a gamepad, so this does not prove that a
//! DualSense moves a character.

use rominabox_desktop::{
    controls::{self, Controls},
    packaging::{
        isolated_hotkey_config, ExportRequest, ExportTarget, HOTKEY_BINDS, MANAGED_DATA_DIRECTORIES,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-autoconfig-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

fn keyboard_assignments(config: &str) -> BTreeMap<&str, &str> {
    config
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            if !name.starts_with("input_")
                || name.ends_with("_btn")
                || name.ends_with("_axis")
                || name.ends_with("_mbtn")
            {
                return None;
            }
            Some((name, value.trim_matches('"')))
        })
        .collect()
}

fn declared_gameplay_keys() -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../controls.json");
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut keys = BTreeSet::new();
    for profile in registry["profiles"].as_array().unwrap() {
        for control in profile["controls"].as_array().unwrap() {
            if let Some(key) = control["key"].as_str() {
                keys.insert(key.to_string());
            }
        }
    }
    keys
}

fn pinned_meta_bind_names() -> Vec<String> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/retroarch/configuration.c");
    let text = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "pinned RetroArch configuration.c is required to check the allow-list: {}",
            path.display()
        )
    });
    let mut names = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("DECLARE_META_BIND(") else {
            continue;
        };
        let name = rest
            .split(',')
            .nth(1)
            .map(str::trim)
            .expect("DECLARE_META_BIND has a bind name")
            .to_string();
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }
    names
}

fn walk_files(root: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// A normal export has no fast-forward key. With advanced access we bind
/// Space to the toggle and `l` to hold, and never a controller button.
///
/// This does not prove that RetroArch treats those keys as fast-forward at
/// runtime. It checks the config we write. `l` is also a declared stick
/// key. We record that overlap here and leave open whether hold should
/// move. We do not check author overrides that reuse Space.
#[test]
fn advanced_access_reaches_fast_forward_on_space_and_nowhere_else() {
    let ordinary = isolated_hotkey_config(true, false);
    let advanced = isolated_hotkey_config(true, true);

    assert_eq!(
        config_value(&ordinary, "input_toggle_fast_forward"),
        Some("nul")
    );
    assert_eq!(
        config_value(&advanced, "input_toggle_fast_forward"),
        Some("space")
    );
    for suffix in ["_btn", "_axis", "_mbtn"] {
        assert_eq!(
            config_value(&advanced, &format!("input_toggle_fast_forward{suffix}")),
            Some("nul"),
            "advanced fast-forward must stay keyboard-only"
        );
    }
    assert_eq!(
        config_value(&ordinary, "input_hold_fast_forward"),
        Some("nul")
    );
    assert_eq!(
        config_value(&advanced, "input_hold_fast_forward"),
        Some("l")
    );

    let mut ordinary_keys = keyboard_assignments(&ordinary);
    let mut advanced_keys = keyboard_assignments(&advanced);
    assert_eq!(
        ordinary_keys.remove("input_toggle_fast_forward"),
        Some("nul")
    );
    assert_eq!(ordinary_keys.remove("input_hold_fast_forward"), Some("nul"));
    assert_eq!(
        advanced_keys.remove("input_toggle_fast_forward"),
        Some("space")
    );
    assert_eq!(advanced_keys.remove("input_hold_fast_forward"), Some("l"));
    assert_eq!(
        ordinary_keys, advanced_keys,
        "advanced access may change only the two fast-forward keyboard keys"
    );

    let gameplay = declared_gameplay_keys();
    assert!(
        gameplay.contains("l"),
        "hold fast-forward reuses `l`, which a declared stick binding also uses"
    );
    assert!(
        !gameplay.contains("space"),
        "Space is the advanced toggle and is not a declared gameplay key"
    );

    let advanced_entries: Vec<_> = HOTKEY_BINDS
        .iter()
        .filter(|bind| bind.advanced_key.is_some())
        .map(|bind| (bind.name, bind.advanced_key))
        .collect();
    assert_eq!(
        advanced_entries,
        vec![
            ("toggle_fast_forward", Some("space")),
            ("hold_fast_forward", Some("l")),
        ]
    );
}

/// The allow-list is the whole `DECLARE_META_BIND` table. It contains every
/// name in that table, and every button, axis and mouse variant stays `nul`
/// even when advanced access is on.
///
/// We compare source text, with both sides of the Lakka `exit_emulator`
/// ifdef, after we remove duplicate names. We do not preprocess the macOS
/// build, and this does not prove that no pad can reach a hotkey.
#[test]
fn hotkey_allow_list_is_unchanged_and_controller_variants_stay_nul() {
    let policy: Vec<&str> = HOTKEY_BINDS.iter().map(|bind| bind.name).collect();
    assert_eq!(policy, pinned_meta_bind_names());

    let advanced = isolated_hotkey_config(false, true);
    for bind in HOTKEY_BINDS {
        for suffix in ["_btn", "_axis", "_mbtn"] {
            assert_eq!(
                config_value(&advanced, &format!("input_{}{suffix}", bind.name)),
                Some("nul"),
                "{}{suffix} was removed or given a controller binding",
                bind.name
            );
        }
    }
}

/// By default we write keyboard keys and leave joypad buttons unset. We still
/// write a button that the author supplies, so the omission is a default and
/// not missing code.
///
/// This does not prove that RetroArch ignores an absent `_btn` line. In the
/// pinned poll code, the autoconfig bind applies while the joykey is `NO_BTN`.
#[test]
fn default_gameplay_binds_are_keyboard_only() {
    let root = scratch();
    controls::write_defaults_config(
        "megadrive",
        &Controls::default(),
        &root.join("controls.cfg"),
    )
    .unwrap();
    let text = fs::read_to_string(root.join("controls.cfg")).unwrap();
    assert!(text.contains("input_player1_a = \"c\""));
    assert!(
        !text.lines().any(|line| {
            line.contains("_btn") || line.contains("_axis") || line.contains("_mbtn")
        }),
        "a default export must not invent controller buttons:\n{text}"
    );

    let authored: Controls = serde_json::from_value(serde_json::json!({
        "bindings": { "a": { "button": "0" } }
    }))
    .unwrap();
    controls::write_defaults_config("megadrive", &authored, &root.join("authored.cfg")).unwrap();
    let authored_text = fs::read_to_string(root.join("authored.cfg")).unwrap();
    assert!(authored_text.contains("input_player1_a_btn = \"0\""));
    assert!(authored_text.contains("input_player1_a = \"c\""));
}

/// The runtime kit and an exported app contain no joypad profile. In the
/// launcher we set `joypad_autoconfig_dir` to a managed directory that we
/// only create, and with advanced access we still ship no profiles.
///
/// This does not prove that the directory is empty after a player has run,
/// and it does not look for compiled-in profiles in the RetroArch executable.
#[test]
#[cfg(target_os = "macos")]
fn export_points_at_an_empty_autoconfig_directory_and_ships_no_profile() {
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime");
    assert!(
        resources.is_dir(),
        "runtime resources are part of the kit this test checks"
    );
    let mut shipped = Vec::new();
    walk_files(&resources, &mut shipped);
    let profile = shipped.iter().find(|path| {
        path.components()
            .any(|component| component.as_os_str() == "autoconfig")
            || fs::read_to_string(path)
                .ok()
                .is_some_and(|text| text.contains("input_vendor_id"))
    });
    assert!(
        profile.is_none(),
        "runtime kit must not carry a joypad profile: {profile:?}"
    );

    let root = scratch();
    let kit = root.join("runtime-kit");
    fs::create_dir_all(kit.join("bin")).unwrap();
    fs::create_dir_all(kit.join("cores")).unwrap();
    fs::create_dir_all(kit.join("Frameworks")).unwrap();
    fs::create_dir_all(kit.join("licenses")).unwrap();
    fs::create_dir_all(kit.join("licenses/native")).unwrap();
    fs::create_dir_all(kit.join("provenance/native-rmlui")).unwrap();
    fs::write(kit.join("bin/retroarch"), b"#!/bin/sh\n").unwrap();
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
    let rom = root.join("sonic.bin");
    fs::write(&rom, b"RIBtest").unwrap();
    let request = ExportRequest {
        rom,
        title: "Autoconfig".to_string(),
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
        advanced_emulator_access: true,
        output_dir: root.join("out"),
        target: ExportTarget::Macos,
        runtime_kit: kit,
        core: None,
    };
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    let launcher = result.app_path.join("Contents/MacOS/ROM-in-a-Box");
    let script = fs::read_to_string(&launcher).unwrap();
    let marker = "/bin/cat >\"$cfg\" <<EOF\n";
    let start = script.find(marker).expect("launcher writes retroarch.cfg");
    let body = &script[start + marker.len()..];
    let config = &body[..body.find("\nEOF\n").expect("config heredoc ends")];

    assert_eq!(
        config_value(config, "joypad_autoconfig_dir"),
        Some("$data_dir/autoconfig")
    );
    assert_eq!(config_value(config, "input_joypad_driver"), Some("hid"));
    assert!(MANAGED_DATA_DIRECTORIES.contains(&"autoconfig"));
    assert!(script.contains(&format!(
        "for name in {}; do",
        MANAGED_DATA_DIRECTORIES.join(" ")
    )));
    assert!(
        !script
            .lines()
            .any(|line| line.contains("cp ") && line.contains("autoconfig")),
        "the launcher must not copy a profile into the autoconfig directory"
    );
    assert_eq!(
        config_value(config, "input_toggle_fast_forward"),
        Some("space"),
        "advanced access has to reach the exported launcher, not only the helper"
    );
    assert_eq!(config_value(config, "input_hold_fast_forward"), Some("l"));
    assert_eq!(
        config_value(config, "input_toggle_fast_forward_btn"),
        Some("nul")
    );
    assert_eq!(
        config_value(config, "input_hold_fast_forward_btn"),
        Some("nul")
    );

    let mut bundled = Vec::new();
    walk_files(&result.app_path, &mut bundled);
    let shipped_profile = bundled.iter().find(|path| {
        path.components()
            .any(|component| component.as_os_str() == "autoconfig")
            || fs::read_to_string(path)
                .ok()
                .is_some_and(|text| text.contains("input_vendor_id"))
    });
    assert!(
        shipped_profile.is_none(),
        "exported app must not contain a joypad profile: {shipped_profile:?}"
    );
}
