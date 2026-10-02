//! Checks that run only on a Mac: the hid profiles in a macOS export, the
//! pinned upstream set they come from, and the copy we make at launch.

use super::{config_value, pinned_meta_bind_names, scratch};
use crate::{export_fixture, support};
use rominabox_engine::{
    controls::Controls,
    packaging::{ExportRequest, ExportTarget, MANAGED_DATA_DIRECTORIES},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

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

fn repo_root() -> PathBuf {
    rominabox_engine::repo::root()
}

fn runtime_resources() -> PathBuf {
    rominabox_engine::repo::builder_resources().join("runtime")
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

/// Return whether a config line binds a meta action. Its key is then
/// `input_<name>`, or that name with the button, axis, mouse or label suffix
/// that RetroArch reads. A player prefix or an `_altN` alternative is the
/// same bind. Comments do not count, and neither does the one meta line we
/// keep in a profile, the pad's menu button, which we read as Home in the menu.
fn is_meta_bind_assignment(line: &str, names: &[String]) -> bool {
    let body = line.trim();
    if body.is_empty() || body.starts_with('#') || !line.contains('=') {
        return false;
    }
    let key = line.split_once('=').unwrap().0.trim();
    if key == rominabox_engine::hotkeys::home_button_key() {
        return false;
    }
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
    let output = Command::new(support::python())
        .arg("-c")
        .arg(script)
        .arg(repo_root().join("scripts/prepare_runtime.py"))
        .arg(source)
        .output()
        .expect("the test Python can import the staging stripper");
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

/// The staged hid directory contains a profile with the DualSense ids.
///
/// `1356/3302` are the decimal Sony vendor id and DualSense product id, as
/// in the RetroArch log line `[Autoconf] ... not configured`. We set the hid
/// driver in the export, so the profiles come from `autoconfig/hid/`. There
/// must be exactly one staged profile with that pair. We do not check that a
/// pad is attached, that IOHID reports those ids, or that the buttons match.
#[test]
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

/// When we stage the profiles, we remove every meta and hotkey line instead
/// of trusting upstream not to set them. We keep only the pad's menu button,
/// `input_menu_toggle_btn`, which we read as Home in our menu and which the
/// RetroArch menu toggle does not use.
///
/// We add binds that the current hid set does not contain (exit, mouse, a
/// player prefix, an alternative, the label and axis of the menu button) and
/// check that we remove them and keep the menu button. Then we check every
/// staged profile, and that the DualSense menu line of the pinned archive is
/// in the staged file. We do not check the PS button on hardware.
#[test]
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
    assert!(
        stripped.lines().any(|line| line == "input_menu_toggle_btn = \"12\""),
        "the pad's own menu button survives, for Home: {stripped}"
    );
    for gone in ["input_menu_toggle_btn_label", "input_menu_toggle_axis", "input_menu_toggle_mbtn",
            "input_menu_toggle = ", "input_player1_menu_toggle_btn", "input_menu_toggle_btn_alt1"] {
        assert!(
            !stripped.lines().any(|line| line.starts_with(gone)),
            "{gone} is not the menu button and must go: {stripped}"
        );
    }
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
    // We keep the downloaded upstream archive in work/downloads, never in the kit.
    let archive = rominabox_engine::repo::at(&format!(
        "work/downloads/retroarch-joypad-autoconfig-{revision}.tar.gz"
    ));
    let upstream = archive_member(&archive, "hid/DualSense Wireless Controller (PS5).cfg");
    assert_eq!(
        profile_value(&upstream, &rominabox_engine::hotkeys::home_button_key()),
        Some("12"),
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
    assert_eq!(
        profile_value(&staged, &rominabox_engine::hotkeys::home_button_key()),
        Some("12"),
        "the staged DualSense keeps its PS button as the menu button:\n{staged}"
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
fn joypad_autoconfig_licence_and_provenance_match_the_pin() {
    let revision = pinned_autoconfig_revision();
    let resources = runtime_resources();
    let licence = fs::read_to_string(resources.join("licenses/retroarch-joypad-autoconfig.txt"))
        .expect("staged COPYING");
    assert!(licence.contains("Copyright (c) 2019 The RetroArch team"));
    assert!(licence.contains("Permission is hereby granted"));
    assert!(licence.contains("\n\nMIT License\n"));
    assert!(licence.contains(&format!("Version:  {revision}\n")));

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
    let source_url = component["source_url"].as_str().unwrap();
    assert!(source_url.contains(&revision));
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
    export_fixture::write_runtime_stub_for(&kit.join("bin/retroarch"), &[]);
    export_fixture::attach_real_launcher(&kit);
    fs::write(kit.join("cores/genesis_plus_gx_libretro.dylib"), b"core").unwrap();
    for name in [
        "NATIVE-DEPENDENCIES.txt",
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
        game: rominabox_engine::game::Game {
            show_menu: false,
            theme: "native".into(),
            splash: false,
            advanced_emulator_access: true,
            include_achievements: false,
            ..rominabox_engine::game::Game::new(rom, "Autoconfig", "megadrive", ExportTarget::Macos)
        },
        zip: None,
        output_dir: root.join("out"),
        replace: false,
        runtime_kit: kit,
        core: None,
        core_cache: None,
        accounts_folder: None,
    };
    let cancelled = AtomicBool::new(false);
    let result = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    let launcher = result.app_path.join("Contents/MacOS/retroarch");
    let plan = fs::read_to_string(result.app_path.join("Contents/Resources/launch.plan")).unwrap();
    let marker = "---config---\n";
    let start = plan
        .find(marker)
        .expect("launch plan contains the runtime config");
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
        .env("ROMINABOX_QUIET", "1")
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
