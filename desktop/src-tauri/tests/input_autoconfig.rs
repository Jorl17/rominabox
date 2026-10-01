//! Joypad autoconfig for exported games, and fast-forward as an advanced key.
//!
//! We check the hotkey tier, and the hid profiles we ship at export so that we
//! can match a plugged-in pad. In these tests we read the generated config,
//! the staged kit and the pinned RetroArch sources. We do not launch a player,
//! open a window or talk to a gamepad, so these tests do not prove that a
//! DualSense moves a character.

#[cfg(target_os = "macos")]
mod export_fixture;
#[cfg(target_os = "macos")]
#[path = "input_autoconfig/macos.rs"]
mod macos;

use rominabox_desktop::{
    controls::{self, Controls},
    meta_binds::{isolated_meta_bind_config, META_BINDS},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[cfg(target_os = "macos")]
mod support;

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-autoconfig")
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
    let path = rominabox_desktop::repo::at("desktop/controls.json");
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
    let path = rominabox_desktop::repo::at("vendor/retroarch/configuration.c");
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

/// A normal export has no fast-forward key. With advanced access, we restore
/// Space and `l` for fast-forward, on the keyboard only, and nothing else.
/// Quit and fullscreen are `nul` in both, so `q` and `f` stay gameplay keys.
///
/// We check only the config we write, so this does not prove that those keys
/// do the named actions at runtime. `l` is also a declared stick key. We
/// record that overlap and do not decide whether hold should move. We do not
/// check author overrides that reuse Space.
#[test]
fn advanced_access_reaches_fast_forward_on_the_keyboard_only() {
    let ordinary = isolated_meta_bind_config(false);
    let advanced = isolated_meta_bind_config(true);

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
        "advanced access may change only the fast-forward keyboard keys"
    );
    for keys in [&ordinary_keys, &advanced_keys] {
        assert_eq!(keys.get("input_exit_emulator"), Some(&"nul"));
        assert_eq!(keys.get("input_toggle_fullscreen"), Some(&"nul"));
    }

    let gameplay = declared_gameplay_keys();
    assert!(
        gameplay.contains("l"),
        "hold fast-forward reuses `l`, which a declared stick binding also uses"
    );
    assert!(
        !gameplay.contains("space"),
        "Space is the advanced toggle and is not a declared gameplay key"
    );

    let advanced_entries: Vec<_> = META_BINDS
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
fn meta_bind_allow_list_is_unchanged_and_controller_variants_stay_nul() {
    let policy: Vec<&str> = META_BINDS.iter().map(|bind| bind.name).collect();
    assert_eq!(policy, pinned_meta_bind_names());

    let advanced = isolated_meta_bind_config(true);
    for bind in META_BINDS {
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

/// We write keyboard keys and never the button numbers of a controller,
/// because we read a player's pad through its controller profile, and a raw
/// number would fit only one pad model. When the author moves a control on
/// the pad, we move its key to the new position and tell the menu where it is.
///
/// This does not prove that an absent `_btn` line has no effect. In the pinned
/// poll, autoconfig applies when the user joykey stays `NO_BTN`.
#[test]
fn gameplay_binds_are_keys_on_pad_positions() {
    let root = scratch();
    let no_pad_numbers = |text: &str| {
        !text.lines().any(|line| {
            line.contains("_btn") || line.contains("_axis") || line.contains("_mbtn")
        })
    };
    controls::write_defaults_config(
        "megadrive",
        &Controls::default(),
        &root.join("controls.cfg"),
    )
    .unwrap();
    let text = fs::read_to_string(root.join("controls.cfg")).unwrap();
    assert!(text.contains("input_player1_a = \"c\""));
    assert!(!text.contains("rib_position_"), "nothing moved:\n{text}");
    assert!(no_pad_numbers(&text), "a default export must not invent controller buttons:\n{text}");

    // Mega Drive C is RetroPad a, B is b. Swapped, each key goes with its button.
    let swapped: Controls = serde_json::from_value(serde_json::json!({
        "bindings": { "a": { "pad": "b" }, "b": { "pad": "a" } }
    }))
    .unwrap();
    controls::write_defaults_config("megadrive", &swapped, &root.join("swapped.cfg")).unwrap();
    let swapped_text = fs::read_to_string(root.join("swapped.cfg")).unwrap();
    for line in [
        "input_player1_b = \"c\"",
        "input_player1_a = \"x\"",
        "rib_position_a = \"b\"",
        "rib_position_b = \"a\"",
    ] {
        assert!(swapped_text.contains(line), "no {line} in\n{swapped_text}");
    }
    assert!(no_pad_numbers(&swapped_text), "{swapped_text}");

    let raw = serde_json::from_value::<Controls>(serde_json::json!({
        "bindings": { "a": { "button": "0" } }
    }));
    assert!(raw.is_err(), "a pad model's button number is not a binding");
}

/// We strip at least every bind that RetroArch loads from a profile.
///
/// `input_config_set_autoconfig_binds` goes through the whole bind table
/// (`configuration.c:7529-7540`), and `turbo` and `hold` are in it without
/// being meta binds. When the user bind is NO_BTN, as ours are, the autoconfig
/// button applies, so a line such as `input_turbo_btn` left in a staged
/// profile would make the pad turbo-fire. So we use an allow-list, and this
/// test covers binds that are not meta binds.
#[test]
fn no_shipped_profile_can_bind_anything_but_gameplay() {
    let staged = staged_autoconfig_root();
    let Some(profiles) = read_profiles(&staged) else {
        eprintln!("no staged autoconfig profiles here; nothing was verified");
        return;
    };
    assert!(!profiles.is_empty(), "the kit should stage joypad profiles");

    // Names that RetroArch would load and that are not gameplay binds.
    let forbidden = [
        "menu_toggle",
        "exit_emulator",
        "turbo",
        "hold",
        "toggle_fast_forward",
        "hold_fast_forward",
        "screenshot",
        "rewind",
        "pause_toggle",
    ];
    for (name, body) in &profiles {
        for line in body.lines() {
            let line = line.trim();
            assert!(
                !line.to_ascii_lowercase().starts_with("#include"),
                "{name} carries an #include; RetroArch would follow it to a file \
                 that never passes through staging"
            );
            let Some((key, _)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if key == rominabox_desktop::hotkeys::home_button_key() {
                continue;
            }
            for bind in forbidden {
                assert!(
                    !key.starts_with(&format!("input_{bind}")),
                    "{name} binds '{key}', which RetroArch's autoconfig loader \
                     would apply over our own hotkey allow-list"
                );
            }
        }
    }
}

/// We keep gameplay labels such as `input_b_btn_label` with the allow-list.
///
/// Stripping only one suffix from `input_b_btn_label` leaves `b_btn`, which is
/// not a bind name. These labels name the physical buttons, and a profile
/// exists to give them.
#[test]
fn gameplay_button_labels_are_kept() {
    let staged = staged_autoconfig_root();
    let Some(profiles) = read_profiles(&staged) else {
        eprintln!("no staged autoconfig profiles here; nothing was verified");
        return;
    };
    let labelled = profiles
        .iter()
        .filter(|(_, body)| body.contains("_btn_label"))
        .count();
    assert!(
        labelled > 0,
        "no staged profile kept a button label; the allow-list is stripping \
         legitimate gameplay metadata"
    );
}

fn staged_autoconfig_root() -> std::path::PathBuf {
    rominabox_desktop::repo::at("desktop/src-tauri/resources/runtime/autoconfig/hid")
}

fn read_profiles(root: &std::path::Path) -> Option<Vec<(String, String)>> {
    let entries = std::fs::read_dir(root).ok()?;
    Some(
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|e| e == "cfg"))
            .filter_map(|entry| {
                let body = std::fs::read_to_string(entry.path()).ok()?;
                Some((entry.file_name().to_string_lossy().into_owned(), body))
            })
            .collect(),
    )
}
