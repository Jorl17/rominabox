//! Joypad autoconfig for exported games, and fast-forward as an advanced key.
//!
//! We check the hotkey tier, and the hid profiles we ship at export so that we
//! can match a plugged-in pad. In these tests we read the generated config,
//! the staged kit and the pinned RetroArch sources. We do not launch a player,
//! open a window or talk to a gamepad, so these tests do not prove that a
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
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

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

struct ContainerGuard(PathBuf);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let Some(name) = self.0.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        if name.starts_with("app.rominabox.game.") && self.0.is_dir() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

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
    let path =
        rominabox_desktop::repo::at("vendor/retroarch/configuration.c");
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

fn repo_root() -> PathBuf {
    rominabox_desktop::repo::root()
}

fn runtime_resources() -> PathBuf {
    rominabox_desktop::repo::at("desktop/src-tauri/resources/runtime")
}

fn pinned_autoconfig_revision() -> String {
    let path = repo_root().join("scripts/prepare_runtime.py");
    let text = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "prepare_runtime.py is required to read the autoconfig pin: {}",
            path.display()
        )
    });
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("JOYPAD_AUTOCONFIG_REVISION = \"") else {
            continue;
        };
        let revision = rest.trim_end_matches('"').to_string();
        assert_eq!(
            revision.len(),
            40,
            "autoconfig pin must be a full commit sha"
        );
        return revision;
    }
    panic!("JOYPAD_AUTOCONFIG_REVISION is missing from prepare_runtime.py");
}

fn strip_player_prefix(rest: &str) -> &str {
    let Some(after_player) = rest.strip_prefix("player") else {
        return rest;
    };
    let Some(underscore) = after_player.find('_') else {
        return rest;
    };
    let digits = &after_player[..underscore];
    if !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()) {
        &after_player[underscore + 1..]
    } else {
        rest
    }
}

fn strip_alt_suffix(rest: &str) -> &str {
    let Some(index) = rest.rfind("_alt") else {
        return rest;
    };
    let digits = &rest[index + 4..];
    if !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()) {
        &rest[..index]
    } else {
        rest
    }
}

/// Whether a config assignment binds a meta action: its key is `input_<name>`
/// or that name plus one of the button, axis, mouse or label suffixes in
/// RetroArch. Player prefixes and `_altN` alternatives count as the same
/// bind. Comments are not assignments.
fn is_meta_bind_assignment(line: &str, names: &[String]) -> bool {
    let body = line.trim();
    if body.is_empty() || body.starts_with('#') || !line.contains('=') {
        return false;
    }
    let key = line.split_once('=').unwrap().0.trim();
    let Some(rest) = key.strip_prefix("input_") else {
        return false;
    };
    let rest = strip_alt_suffix(strip_player_prefix(rest));
    names.iter().any(|name| {
        rest == name
            || rest == format!("{name}_btn")
            || rest == format!("{name}_axis")
            || rest == format!("{name}_mbtn")
            || rest == format!("{name}_btn_label")
            || rest == format!("{name}_axis_label")
    })
}

fn autoconfig_profiles(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk_files(&root.join("autoconfig"), &mut found);
    found.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("cfg"));
    found.sort();
    found
}

fn profile_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name.trim() == key).then(|| value.trim().trim_matches('"'))
    })
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

fn strip_planted_profile(source: &str) -> String {
    let script = r#"
import importlib.util
import sys
path, planted = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_file_location("prepare_runtime", path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
stripped, removed = module.strip_meta_bind_lines(planted, set(module.meta_bind_names()))
if removed < 1:
    raise SystemExit("stripper removed nothing")
sys.stdout.write(stripped)
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(repo_root().join("scripts/prepare_runtime.py"))
        .arg(source)
        .output()
        .expect("python3 can import the staging stripper");
    assert!(
        output.status.success(),
        "stripper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stripped profile is utf-8")
}

fn archive_member(archive: &Path, suffix: &str) -> String {
    let listing = Command::new("tar")
        .args(["-tzf"])
        .arg(archive)
        .output()
        .expect("tar can list the pinned autoconfig archive");
    assert!(
        listing.status.success(),
        "tar listing failed: {}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let listing = String::from_utf8(listing.stdout).expect("tar listing is utf-8");
    let member = listing
        .lines()
        .find(|line| line.ends_with(suffix))
        .unwrap_or_else(|| panic!("archive has no member ending in {suffix}"))
        .to_string();
    let extracted = Command::new("tar")
        .args(["-xOf"])
        .arg(archive)
        .arg(&member)
        .output()
        .expect("tar can read one autoconfig member");
    assert!(
        extracted.status.success(),
        "tar extract failed: {}",
        String::from_utf8_lossy(&extracted.stderr)
    );
    String::from_utf8(extracted.stdout).expect("upstream profile is utf-8")
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

/// A normal export has no fast-forward, quit or fullscreen key. With
/// advanced access we bind Space, `l`, `q` and `f`, on the keyboard only.
///
/// Quit and fullscreen are in this tier so that a shipped game does not quit
/// on Q and `f` can be a gameplay key. Turning on advanced access changes
/// those four keyboard keys and no other setting.
///
/// This does not prove that RetroArch treats those keys as the named actions
/// at runtime. It checks the config we write. `l` is also a declared stick
/// key. We record that overlap here and leave open whether hold should
/// move. We do not check author overrides that reuse Space.
#[test]
fn advanced_access_reaches_fast_forward_quit_and_fullscreen_on_the_keyboard_only() {
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
    // Quit and fullscreen differ between the two maps. In a default export
    // they must be nul, so Q does not quit and f stays a gameplay key.
    assert_eq!(ordinary_keys.remove("input_exit_emulator"), Some("nul"));
    assert_eq!(ordinary_keys.remove("input_toggle_fullscreen"), Some("nul"));
    assert_eq!(
        advanced_keys.remove("input_toggle_fast_forward"),
        Some("space")
    );
    assert_eq!(advanced_keys.remove("input_hold_fast_forward"), Some("l"));
    assert_eq!(advanced_keys.remove("input_exit_emulator"), Some("q"));
    assert_eq!(advanced_keys.remove("input_toggle_fullscreen"), Some("f"));
    assert_eq!(
        ordinary_keys, advanced_keys,
        "advanced access may change only the fast-forward, quit and fullscreen keyboard keys"
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
    // Quit and fullscreen are in this list because we bind neither q nor f
    // in a shipped game.
    assert_eq!(
        advanced_entries,
        vec![
            ("exit_emulator", Some("q")),
            ("toggle_fast_forward", Some("space")),
            ("hold_fast_forward", Some("l")),
            ("toggle_fullscreen", Some("f")),
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

/// The staged hid directory contains the DualSense vendor and product ids.
///
/// `1356/3302` is the decimal Sony vendor id and DualSense product id, as in
/// the RetroArch log line `[Autoconf] ... not configured`. In the export we
/// set the hid driver, with its profiles in `autoconfig/hid/`. We look for
/// the one staged profile with that pair. This does not prove that a pad is
/// attached, that IOHID reports those ids, or that the button numbers match.
#[test]
#[cfg(target_os = "macos")]
fn staged_hid_profile_matches_the_logged_dualsense_ids() {
    let resources = runtime_resources();
    assert!(
        resources.join("autoconfig/hid").is_dir(),
        "stage joypad autoconfig into the runtime kit before this test"
    );
    let mut matches = Vec::new();
    for path in autoconfig_profiles(&resources) {
        let text = fs::read_to_string(&path).unwrap();
        if profile_value(&text, "input_vendor_id") == Some("1356")
            && profile_value(&text, "input_product_id") == Some("3302")
        {
            matches.push(path);
        }
    }
    assert_eq!(
        matches.len(),
        1,
        "one hid profile must own 1356/3302; scan order would otherwise pick a tie: {matches:?}"
    );
    let text = fs::read_to_string(&matches[0]).unwrap();
    assert_eq!(profile_value(&text, "input_driver"), Some("hid"));
    assert_eq!(profile_value(&text, "input_b_btn"), Some("1"));
    assert!(
        matches[0]
            .components()
            .any(|component| component.as_os_str() == "hid"),
        "the profile has to sit in the hid driver directory RetroArch scans"
    );
}

/// We remove meta and hotkey lines in staging instead of trusting upstream.
///
/// The public DualSense profile binds `input_menu_toggle_btn`, and a `nul`
/// user joykey still falls back to that autoconfig bind. We add binds that
/// the current hid set does not contain (exit, mouse, player prefix, alt),
/// check that we strip them, check every staged profile, and show that the
/// pinned archive still has the DualSense menu line that we removed. This
/// does not prove that the PS button does nothing on hardware. A comment
/// that mentions a meta key is not an assignment.
#[test]
#[cfg(target_os = "macos")]
fn shipped_profiles_strip_meta_binds_including_ones_upstream_hid_lacks() {
    let names = pinned_meta_bind_names();
    let planted = "\
input_driver = \"hid\"
input_b_btn = \"1\"
input_b = \"c\"
input_r_btn = \"5\"
input_a_btn_label = \"Circle\"
input_l_x_plus_axis = \"+0\"
input_device_alt1 = \"DualSense Wireless Controller\"
input_vendor_id_alt1 = \"1356\"
input_product_id_alt1 = \"3302\"
input_phys = \"usb-1\"
# input_menu_toggle_btn = \"99\"
input_menu_toggle_btn = \"12\"
input_menu_toggle_btn_label = \"PS\"
input_menu_toggle_axis = \"-2\"
input_menu_toggle_mbtn = \"1\"
input_menu_toggle = \"f1\"
input_exit_emulator_btn = \"9\"
input_hold_fast_forward_axis = \"+1\"
input_toggle_fast_forward_btn_label = \"Unused\"
input_player1_menu_toggle_btn = \"4\"
input_menu_toggle_btn_alt1 = \"5\"
input_reset_btn = \"3\"
";
    let stripped = strip_planted_profile(planted);
    assert!(
        stripped.contains("# input_menu_toggle_btn = \"99\""),
        "a comment is not a bind and must survive: {stripped}"
    );
    for kept in [
        "input_b_btn = \"1\"",
        "input_b = \"c\"",
        "input_r_btn = \"5\"",
        "input_a_btn_label = \"Circle\"",
        "input_l_x_plus_axis = \"+0\"",
        "input_device_alt1 = \"DualSense Wireless Controller\"",
        "input_vendor_id_alt1 = \"1356\"",
        "input_product_id_alt1 = \"3302\"",
        "input_phys = \"usb-1\"",
    ] {
        assert!(
            stripped.contains(kept),
            "a recognition or gameplay line was stripped with the meta binds: {kept}\n{stripped}"
        );
    }
    let surviving: Vec<_> = stripped
        .lines()
        .filter(|line| is_meta_bind_assignment(line, &names))
        .collect();
    assert!(
        surviving.is_empty(),
        "planted meta binds still assigned after staging: {surviving:?}"
    );

    let resources = runtime_resources();
    let profiles = autoconfig_profiles(&resources);
    assert!(!profiles.is_empty(), "staged hid profiles are required");
    for path in &profiles {
        let text = fs::read_to_string(path).unwrap();
        let hits: Vec<_> = text
            .lines()
            .filter(|line| is_meta_bind_assignment(line, &names))
            .collect();
        assert!(
            hits.is_empty(),
            "{} still binds a meta action: {hits:?}",
            path.display()
        );
    }

    let revision = pinned_autoconfig_revision();
    let archive = resources.join(format!(
        "sources/retroarch-joypad-autoconfig-{revision}.tar.gz"
    ));
    let upstream = archive_member(&archive, "hid/DualSense Wireless Controller (PS5).cfg");
    assert!(
        is_meta_bind_assignment(
            upstream
                .lines()
                .find(|line| line.contains("input_menu_toggle_btn ="))
                .expect("upstream DualSense binds menu toggle"),
            &names
        ),
        "the pin no longer contains the menu bind this test uses as proof"
    );
    let staged = fs::read_to_string(
        resources.join("autoconfig/hid/DualSense Wireless Controller (PS5).cfg"),
    )
    .unwrap();
    assert!(
        !staged
            .lines()
            .any(|line| is_meta_bind_assignment(line, &names)),
        "staged DualSense still has a meta bind:\n{staged}"
    );
    assert_eq!(profile_value(&staged, "input_b_btn"), Some("1"));
    assert!(upstream.contains("input_b_btn = \"1\""));
}

/// We ship the profiles with the upstream MIT notice and the pinned revision.
///
/// `docs/dependencies-and-licensing.md` requires the revision, the licence
/// text and the source before we ship a component. We read the staged
/// COPYING and the kit manifest. We do not choose a licence for the project
/// here, and we do not check that the source archive went into a binary,
/// because the profiles are data files.
#[test]
#[cfg(target_os = "macos")]
fn joypad_autoconfig_licence_and_provenance_match_the_pin() {
    let revision = pinned_autoconfig_revision();
    let resources = runtime_resources();
    let licence = fs::read_to_string(resources.join("licenses/retroarch-joypad-autoconfig.txt"))
        .expect("staged COPYING");
    assert!(licence.contains("Copyright (c) 2019 The RetroArch team"));
    assert!(licence.contains("Permission is hereby granted"));
    assert!(licence.starts_with("MIT License"));

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(resources.join("manifest.json")).unwrap())
            .unwrap();
    let component = manifest["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "retroarch-joypad-autoconfig")
        .expect("manifest records retroarch-joypad-autoconfig");
    assert_eq!(component["revision"], revision);
    assert_eq!(component["license"], "MIT");
    assert_eq!(component["license_file"], "retroarch-joypad-autoconfig.txt");
    assert_eq!(
        component["source_archive"],
        format!("retroarch-joypad-autoconfig-{revision}.tar.gz")
    );
    let source_url = component["source_url"].as_str().unwrap();
    assert!(source_url.contains(&revision));
    assert!(resources
        .join("sources")
        .join(format!("retroarch-joypad-autoconfig-{revision}.tar.gz"))
        .is_file());
    let licensing =
        fs::read_to_string(repo_root().join("docs/dependencies-and-licensing.md")).unwrap();
    assert!(
        licensing.contains(&revision),
        "the licensing record must name the pinned autoconfig commit"
    );
}

/// An export contains the staged profiles, which we copy into the game's
/// data on the first launch.
///
/// `joypad_autoconfig_dir` is the per-game data directory. We copy a profile
/// there only when the destination does not exist yet, as with firmware and
/// remaps. In the fixture, retroarch is an empty shell script, so the game
/// exits right after the copy. We do not check that a later launch keeps an
/// edited profile, or that a pad matches a profile.
#[test]
#[cfg(target_os = "macos")]
fn export_ships_hid_profiles_and_the_launcher_seeds_them() {
    let resources = runtime_resources();
    let root = scratch();
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
    copy_tree(&resources.join("autoconfig"), &kit.join("autoconfig"));
    fs::copy(
        resources.join("licenses/retroarch-joypad-autoconfig.txt"),
        kit.join("licenses/retroarch-joypad-autoconfig.txt"),
    )
    .unwrap();
    let real_manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(resources.join("manifest.json")).unwrap())
            .unwrap();
    let joypad = real_manifest["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "retroarch-joypad-autoconfig")
        .expect("staged kit records joypad autoconfig")
        .clone();
    fs::write(
        kit.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "components": [
                {"name": "RetroArch"},
                {"name": "RmlUi"},
                {"name": "genesis_plus_gx"},
                joypad,
            ]
        }))
        .unwrap(),
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
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        achievements: Default::default(),
        output_dir: root.join("out"),
        target: ExportTarget::Macos,
        runtime_kit: kit,
        core: None,
        core_cache: None,
    };
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    let launcher = result.app_path.join("Contents/MacOS/retroarch");
    let plan = fs::read_to_string(result.app_path.join("Contents/Resources/launch.plan")).unwrap();
    let marker = "---config---\n";
    let start = plan.find(marker).expect("launch plan contains the runtime config");
    let config = &plan[start + marker.len()..];

    assert_eq!(
        config_value(config, "joypad_autoconfig_dir"),
        Some("$data_dir/autoconfig")
    );
    assert_eq!(config_value(config, "input_joypad_driver"), Some("hid"));
    assert!(MANAGED_DATA_DIRECTORIES.contains(&"autoconfig"));
    assert!(plan.contains("managed\tautoconfig\n"));
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

    let bundled_profiles = autoconfig_profiles(&result.app_path.join("Contents/Resources"));
    let staged_profiles = autoconfig_profiles(&resources);
    assert_eq!(bundled_profiles.len(), staged_profiles.len());
    assert!(bundled_profiles.len() > 1);
    let bundled_dualsense = bundled_profiles
        .iter()
        .find(|path| path.ends_with("hid/DualSense Wireless Controller (PS5).cfg"))
        .expect("export contains the DualSense profile");
    assert_eq!(
        fs::read(bundled_dualsense).unwrap(),
        fs::read(resources.join("autoconfig/hid/DualSense Wireless Controller (PS5).cfg")).unwrap()
    );
    let names = pinned_meta_bind_names();
    for path in &bundled_profiles {
        let text = fs::read_to_string(path).unwrap();
        assert!(
            text.lines()
                .all(|line| !is_meta_bind_assignment(line, &names)),
            "{} shipped a meta bind",
            path.display()
        );
    }
    let exported_licence = fs::read(
        result
            .app_path
            .join("Contents/Resources/Legal/Licenses/retroarch-joypad-autoconfig.txt"),
    )
    .unwrap();
    assert_eq!(
        exported_licence,
        fs::read(resources.join("licenses/retroarch-joypad-autoconfig.txt")).unwrap()
    );
    let exported_manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(
            result
                .app_path
                .join("Contents/Resources/Legal/components.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let exported_joypad = exported_manifest["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "retroarch-joypad-autoconfig")
        .expect("export provenance keeps the joypad component");
    assert_eq!(exported_joypad["revision"], pinned_autoconfig_revision());

    // Run the exported game. The fixture runtime returns at once, so no
    // window opens. In the launcher we copy the profiles to the folder that
    // RetroArch scans, and in the sandbox HOME points into the container.
    let game: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(result.app_path.join("Contents/Resources/game.json")).unwrap(),
    )
    .unwrap();
    let identity = game["identity"].as_str().unwrap();
    let bundle_id = format!("app.rominabox.game.{identity}");
    let container = PathBuf::from(std::env::var("HOME").unwrap())
        .join("Library/Containers")
        .join(&bundle_id);
    let _container = ContainerGuard(container.clone());
    let launched = Command::new(&launcher)
        .output()
        .expect("the launcher can be executed");
    assert!(
        launched.status.success(),
        "launcher seed failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&launched.stdout),
        String::from_utf8_lossy(&launched.stderr)
    );
    let game_dir = container
        .join("Data/Library/Application Support/ROM-in-a-Box/Games")
        .join(identity);
    let kit_hid = resources.join("autoconfig/hid");
    let seeded_hid = game_dir.join("autoconfig/hid");
    let mut kit_profiles: Vec<_> = fs::read_dir(&kit_hid)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("cfg"))
        .collect();
    kit_profiles.sort();
    assert!(
        !kit_profiles.is_empty(),
        "the kit has to ship hid profiles for the seed to copy"
    );
    for kit_profile in &kit_profiles {
        let name = kit_profile.file_name().unwrap();
        let seeded = seeded_hid.join(name);
        assert_eq!(
            fs::read(&seeded).unwrap_or_else(|error| panic!("{}: {error}", seeded.display())),
            fs::read(kit_profile).unwrap(),
            "the launcher has to copy every hid profile into the directory the config names"
        );
    }
    let written = fs::read_to_string(game_dir.join("retroarch.cfg")).unwrap();
    let autoconfig_dir = seeded_hid.parent().unwrap();
    assert_eq!(
        config_value(&written, "joypad_autoconfig_dir"),
        Some(autoconfig_dir.to_str().unwrap()),
        "the config must name the directory the seed just filled, not the literal $data_dir"
    );
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
