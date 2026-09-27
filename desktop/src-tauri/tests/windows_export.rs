//! A Windows export of a stand-in game from a stand-in Windows kit: the game's
//! folder, its contents, and what we tell its player. The core is an actual
//! library that we compile, so we check the imports in an actual import
//! table. We launch nothing.
#![cfg(windows)]

mod export_fixture;

use export_fixture::{export_request_from, kit_base, workspace};
use rominabox_desktop::packaging::{ErrorStage, ExportTarget};
use std::{fs, path::Path, process::Command, sync::atomic::AtomicBool};

/// Compiles `source` into the library `output`, linked with `with`.
fn library(output: &Path, source: &str, with: &[&Path]) {
    let file = output.with_extension("c");
    fs::write(&file, source).unwrap();
    let status = Command::new("cc")
        .args(["-shared", "-O2", "-o"])
        .arg(output)
        .arg(&file)
        .args(with)
        .status()
        .expect("the toolchain's cc runs");
    assert!(status.success(), "could not compile {}", output.display());
}

/// A Windows kit: its player and launcher (stand-ins, as we only copy them),
/// one controller profile per driver folder, and the Mega Drive core.
fn windows_kit(root: &Path) -> std::path::PathBuf {
    let kit = kit_base(root);
    fs::write(kit.join("bin/retroarch.exe"), b"player").unwrap();
    fs::write(kit.join("bin/launcher.exe"), b"launcher").unwrap();
    for driver in ["xinput", "dinput"] {
        fs::create_dir_all(kit.join("autoconfig").join(driver)).unwrap();
        fs::write(kit.join("autoconfig").join(driver).join("pad.cfg"), driver).unwrap();
    }
    library(
        &kit.join("cores/genesis_plus_gx_libretro.dll"),
        "__declspec(dllexport) unsigned retro_api_version(void) { return 1; }\n",
        &[],
    );
    kit
}

fn names(directory: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn config_value<'a>(plan: &'a str, key: &str) -> Option<&'a str> {
    plan.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

#[test]
fn a_windows_game_is_a_folder_holding_its_program_resources_and_player() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.target = ExportTarget::Windows;
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();

    assert_eq!(names(&request.output_dir), ["Hotkey Isolation"]);
    let game = request.output_dir.join("Hotkey Isolation");
    assert_eq!(result.app_path, game);
    assert_eq!(
        names(&game),
        ["Hotkey Isolation.exe", "Resources", "Runtime"]
    );
    assert_eq!(
        fs::read(game.join("Hotkey Isolation.exe")).unwrap(),
        b"launcher"
    );
    assert_eq!(names(&game.join("Runtime")), ["retroarch.exe"]);
    assert_eq!(
        fs::read(game.join("Runtime/retroarch.exe")).unwrap(),
        b"player"
    );

    let resources = game.join("Resources");
    assert_eq!(
        fs::read(resources.join("game-core.dll")).unwrap(),
        fs::read(
            request
                .runtime_kit
                .join("cores/genesis_plus_gx_libretro.dll")
        )
        .unwrap()
    );
    assert!(!resources.join("game-core.dylib").exists());
    assert_eq!(
        fs::read(resources.join("content/sonic.bin")).unwrap(),
        b"RIBtest"
    );
    for driver in ["xinput", "dinput"] {
        assert!(
            resources
                .join("autoconfig")
                .join(driver)
                .join("pad.cfg")
                .is_file(),
            "{driver}"
        );
    }

    let plan = fs::read_to_string(resources.join("launch.plan")).unwrap();
    assert!(plan.contains("data_dir\t$user_data/ROM-in-a-Box/Games/"));
    assert_eq!(config_value(&plan, "audio_driver"), Some("wasapi"));
    assert_eq!(config_value(&plan, "input_joypad_driver"), Some("xinput"));
    assert!(result.runtime_bytes > 0 && result.installed_bytes > result.runtime_bytes);
}

#[test]
fn a_core_that_needs_a_library_windows_lacks_is_refused() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.target = ExportTarget::Windows;
    // In its import table, the core lists a library that no Windows machine has.
    let helper = root.join("helper.dll");
    library(
        &helper,
        "__declspec(dllexport) int helper(void) { return 7; }\n",
        &[],
    );
    library(
        &request
            .runtime_kit
            .join("cores/genesis_plus_gx_libretro.dll"),
        "__declspec(dllimport) int helper(void);\n\
         __declspec(dllexport) unsigned retro_api_version(void) { return (unsigned)helper(); }\n",
        &[&helper],
    );
    let cancelled = AtomicBool::new(false);
    let error = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {})
        .expect_err("a game whose core cannot load must not be made");

    assert_eq!(error.stage, ErrorStage::Dependencies);
    assert!(error.message.contains("helper.dll"), "{}", error.message);
    assert_eq!(names(&request.output_dir), Vec::<String>::new());
}
