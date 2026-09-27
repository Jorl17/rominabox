//! An export of a stand-in game from a stand-in kit, with, for macOS, a
//! compiled player executable that does nothing and an empty core, and the
//! licence files that are in every kit.
#![allow(dead_code)]

use rominabox_desktop::packaging::{ExportRequest, ExportTarget};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
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

pub fn workspace() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-packaging")
}

/// A macOS kit.
pub fn fixture_kit(root: &Path) -> PathBuf {
    let kit = kit_base(root);
    fs::create_dir_all(kit.join("Frameworks")).unwrap();
    write_runtime_stub(&kit.join("bin/retroarch"));
    fs::write(kit.join("cores/genesis_plus_gx_libretro.dylib"), b"core").unwrap();
    fs::write(
        kit.join("runtime-dependencies.json"),
        r#"{"formatVersion":1,"files":[]}"#,
    )
    .unwrap();
    kit
}

/// What the kit for every platform contains besides its player and its core.
pub fn kit_base(root: &Path) -> PathBuf {
    let kit = root.join("runtime-kit");
    fs::create_dir_all(kit.join("bin")).unwrap();
    fs::create_dir_all(kit.join("cores")).unwrap();
    fs::create_dir_all(kit.join("licenses")).unwrap();
    fs::create_dir_all(kit.join("licenses/native")).unwrap();
    fs::create_dir_all(kit.join("provenance/native-rmlui")).unwrap();
    for name in [
        "RetroArch.txt",
        "NATIVE-DEPENDENCIES.txt",
        "RmlUi-MIT.txt",
        "genesis_plus_gx.txt",
    ] {
        fs::write(kit.join("licenses").join(name), name).unwrap();
    }
    fs::write(
        kit.join("manifest.json"),
        r#"{"schema_version":1,"components":[{"name":"RetroArch"},{"name":"RmlUi"},{"name":"genesis_plus_gx"}]}"#,
    )
    .unwrap();
    kit
}

/// A macOS export request.
pub fn export_request(root: &Path) -> ExportRequest {
    export_request_from(root, fixture_kit(root))
}

pub fn export_request_from(root: &Path, runtime_kit: PathBuf) -> ExportRequest {
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
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        include_achievements: false,
        output_dir: root.join("out"),
        replace: false,
        target: ExportTarget::Macos,
        runtime_kit,
        core: None,
        core_cache: None,
    }
}
