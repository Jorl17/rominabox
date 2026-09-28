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
    write_runtime_stub_for(path, &[]);
}

/// A player that does nothing, with code for each of `archs` (`cc -arch`
/// names), or for the host Mac alone when there are none.
pub fn write_runtime_stub_for(path: &Path, archs: &[&str]) {
    let source = path.with_extension("c");
    fs::write(
        &source,
        "int rarch_main(int c, char **v, void *d){(void)c;(void)v;(void)d;return 0;}\nint main(void){return rarch_main(0,0,0);}\n",
    )
    .unwrap();
    let mut cc = Command::new("cc");
    for arch in archs {
        cc.args(["-arch", arch]);
    }
    let status = cc
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

/// A launch library that does nothing, for both Mac processors, because a Mac
/// kit has its launch library beside its player. To run the game's launcher
/// in a test, attach the one from the tree (`attach_real_launcher`).
pub fn write_launch_library_stub(kit: &Path) {
    let library = kit.join("bin/librominabox-launch.dylib");
    let source = library.with_extension("c");
    fs::write(&source, "void rominabox_launch_stub(void) {}\n").unwrap();
    let status = Command::new("cc")
        .args(["-arch", "arm64", "-arch", "x86_64", "-dynamiclib", "-mmacosx-version-min=11.0"])
        .args(["-install_name", "@executable_path/librominabox-launch.dylib", "-o"])
        .arg(&library)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "could not compile the launch library stub");
    fs::remove_file(source).unwrap();
}

/// Put the tree's launch library in the Mac kit `kit`, attached to its
/// player as in scripts/build_kit.py, for a test that runs the exported
/// game's launcher.
pub fn attach_real_launcher(kit: &Path) {
    let output = Command::new(rominabox_desktop::repo::python())
        .arg(rominabox_desktop::repo::at("scripts/build_launcher.py"))
        .arg("--kit")
        .arg(kit)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

/// A macOS kit.
pub fn fixture_kit(root: &Path) -> PathBuf {
    let kit = kit_base(root);
    fs::create_dir_all(kit.join("Frameworks")).unwrap();
    write_runtime_stub(&kit.join("bin/retroarch"));
    write_launch_library_stub(&kit);
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
        "NATIVE-DEPENDENCIES.txt",
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
        game: rominabox_desktop::game::Game {
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
            menu_controls: rominabox_desktop::builder::unstated::menu_controls(),
            firmware: Vec::new(),
            splash: false,
            advanced_emulator_access: false,
            intel_macs: false,
            keep_playing_in_background: false,
            autosave_on_quit: false,
            menu_entries: None,
            shaders: rominabox_desktop::shaders::ShaderSelection::default(),
            include_achievements: false,
            target: ExportTarget::Macos,
            both_platforms: false,
        },
        zip: None,
        output_dir: root.join("out"),
        replace: false,
        runtime_kit,
        core: None,
        core_cache: None,
    }
}

/// Compiles `source` into the library `output`, linked with `with`.
#[cfg(windows)]
pub fn library(output: &Path, source: &str, with: &[&Path]) {
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

/// An `.ico` of one `size`-pixel square of `colour`.
#[cfg(windows)]
pub fn icon_of(colour: [u8; 4], size: u32) -> Vec<u8> {
    let pixels = image::RgbaImage::from_pixel(size, size, image::Rgba(colour));
    let frame = image::codecs::ico::IcoFrame::as_png(
        pixels.as_raw(),
        size,
        size,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    let mut bytes = Vec::new();
    image::codecs::ico::IcoEncoder::new(&mut bytes)
        .encode_images(&[frame])
        .unwrap();
    bytes
}

/// Compiles a program that only ends, with the resources of `script` when
/// there is one.
#[cfg(windows)]
pub fn program(output: &Path, script: Option<&str>) {
    let source = output.with_extension("c");
    fs::write(&source, "int main(void) { return 0; }\n").unwrap();
    let mut compile = Command::new("cc");
    compile.args(["-O2", "-o"]).arg(output).arg(&source);
    if let Some(script) = script {
        let rc = output.with_extension("rc");
        let object = output.with_extension("res.o");
        fs::write(&rc, script).unwrap();
        let status = Command::new("windres")
            .arg(&rc)
            .arg("-o")
            .arg(&object)
            .current_dir(output.parent().unwrap())
            .status()
            .expect("the toolchain's windres runs");
        assert!(status.success(), "could not compile {}", rc.display());
        compile.arg(&object);
    }
    assert!(compile.status().unwrap().success(), "could not compile {}", output.display());
}

/// A Windows kit, with a player that has an icon as group 1, as in
/// RetroArch, a launcher with no resources, one controller profile per
/// driver folder, and the Mega Drive core.
#[cfg(windows)]
pub fn windows_kit(root: &Path) -> std::path::PathBuf {
    let kit = kit_base(root);
    fs::write(kit.join("bin/retroarch.ico"), icon_of([255, 0, 0, 255], 32)).unwrap();
    program(&kit.join("bin/retroarch.exe"), Some("1 ICON \"retroarch.ico\"\n"));
    program(&kit.join("bin/launcher.exe"), None);
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

/// Compiles `source` for Windows with zig, with `extra` (a resource script, a
/// flag) beside it.
#[cfg(not(windows))]
fn for_windows(output: &Path, source: &str, extra: &[&str]) {
    let file = output.with_extension("c");
    fs::write(&file, source).unwrap();
    let status = std::process::Command::new("zig")
        .args(["cc", "-target", "x86_64-windows-gnu", "-O2", "-o"])
        .arg(output)
        .arg(&file)
        .args(extra)
        .current_dir(output.parent().unwrap())
        .status()
        .expect("zig builds Windows programs on this machine; install it (brew install zig)");
    assert!(status.success(), "could not compile {}", output.display());
}

/// A Windows kit like one from a Windows build, made on another machine. It
/// has a player with an icon as group 1, as in RetroArch, a launcher with no
/// resources, one controller profile per driver folder, and the Mega Drive
/// core, each a Windows program compiled with zig.
#[cfg(not(windows))]
pub fn windows_kit_here(root: &Path) -> std::path::PathBuf {
    let kit = kit_base(root);
    image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 0, 0, 255]))
        .save(kit.join("bin/retroarch.ico"))
        .unwrap();
    fs::write(kit.join("bin/retroarch.rc"), "1 ICON \"retroarch.ico\"\n").unwrap();
    const MAIN: &str = "int main(void) { return 0; }\n";
    for_windows(&kit.join("bin/retroarch.exe"), MAIN, &["retroarch.rc"]);
    for_windows(&kit.join("bin/launcher.exe"), MAIN, &[]);
    for driver in ["xinput", "dinput"] {
        fs::create_dir_all(kit.join("autoconfig").join(driver)).unwrap();
        fs::write(kit.join("autoconfig").join(driver).join("pad.cfg"), driver).unwrap();
    }
    for_windows(
        &kit.join("cores/genesis_plus_gx_libretro.dll"),
        "__declspec(dllexport) unsigned retro_api_version(void) { return 1; }\n",
        &["-shared"],
    );
    kit
}
