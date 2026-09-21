#![cfg(target_os = "macos")]

use rominabox_desktop::packaging::{
    isolated_hotkey_config, ExportRequest, ExportStage, ExportTarget, HOTKEY_BINDS,
    MANAGED_DATA_DIRECTORIES,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-packaging-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn fixture_kit(root: &Path) -> PathBuf {
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
    kit
}

fn export_request(root: &Path) -> ExportRequest {
    let rom = root.join("sonic.bin");
    fs::write(&rom, b"RIBtest").unwrap();
    ExportRequest {
        rom,
        title: "Hotkey Isolation".to_string(),
        system: "megadrive".to_string(),
        description: None,
        icon: None,
        background: None,
        show_menu: false,
        start_at_menu: false,
        theme: "native".to_string(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls: rominabox_desktop::controls::Controls::default(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: false,
        output_dir: root.join("out"),
        target: ExportTarget::Macos,
        runtime_kit: fixture_kit(root),
        core: None,
    }
}

fn embedded_runtime_config(script: &str) -> String {
    let marker = "/bin/cat >\"$cfg\" <<EOF\n";
    let start = script
        .find(marker)
        .expect("exported launcher writes retroarch.cfg");
    let body = &script[start + marker.len()..];
    let end = body.find("\nEOF\n").expect("exported config heredoc ends");
    body[..end].to_string()
}

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

fn assert_no_export_staging(output_dir: &Path) {
    let staging_directories = fs::read_dir(output_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".rominabox-export-")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(
        staging_directories.is_empty(),
        "export must remove owned staging directories: {staging_directories:?}"
    );
}

#[test]
fn export_writes_the_reviewed_hotkey_policy_and_managed_paths() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    assert_no_export_staging(&request.output_dir);
    let launcher = result.app_path.join("Contents/MacOS/ROM-in-a-Box");
    let script = fs::read_to_string(&launcher).unwrap();
    let config = embedded_runtime_config(&script);
    let policy = isolated_hotkey_config(false, false);

    assert!(
        config.contains(&policy),
        "exported retroarch.cfg must embed the single hotkey policy"
    );
    assert_eq!(
        config_value(&config, "input_toggle_fast_forward"),
        Some("nul")
    );
    assert_eq!(
        config_value(&config, "input_hold_fast_forward"),
        Some("nul")
    );
    assert_eq!(config_value(&config, "input_menu_toggle"), Some("nul"));
    assert_eq!(config_value(&config, "input_toggle_fullscreen"), Some("f"));
    assert!(!config.contains("input_player1_"));
    assert!(script.contains(&format!(
        "for name in {}; do",
        MANAGED_DATA_DIRECTORIES.join(" ")
    )));
    assert!(script.contains("export ROMINABOX_DATA_DIR=\"$data_dir\""));
    assert!(!script.contains("export HOME="));
    assert_eq!(
        HOTKEY_BINDS
            .iter()
            .filter(|bind| bind.name == "toggle_fast_forward")
            .count(),
        1
    );
}

#[test]
fn cancelled_export_removes_its_staging_directory() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);

    let error = rominabox_desktop::packaging::export_game(&request, &cancelled, |progress| {
        if matches!(progress.stage, ExportStage::Stage) {
            cancelled.store(true, Ordering::Relaxed);
        }
    })
    .unwrap_err();

    assert_eq!(error.stage, "cancelled");
    assert_no_export_staging(&request.output_dir);
}

#[test]
fn failed_export_removes_its_staging_directory() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);
    let runtime = request.runtime_kit.join("bin/retroarch");

    let error = rominabox_desktop::packaging::export_game(&request, &cancelled, |progress| {
        if matches!(progress.stage, ExportStage::Stage) {
            fs::remove_file(&runtime).unwrap();
        }
    })
    .unwrap_err();

    assert_eq!(error.stage, "stage");
    assert_no_export_staging(&request.output_dir);
}
