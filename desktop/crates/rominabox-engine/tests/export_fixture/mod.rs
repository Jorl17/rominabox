//! An export of a stand-in game from a stand-in kit, with, for macOS, a
//! compiled player executable that does nothing and an empty core, and the
//! licence files that are in every kit.
#![allow(dead_code)]

use rominabox_engine::packaging::{ExportRequest, ExportTarget};
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
    let output = Command::new(rominabox_engine::repo::python())
        .arg(rominabox_engine::repo::at("scripts/build_launcher.py"))
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
        game: rominabox_engine::game::Game {
            rom,
            title: "Hotkey Isolation".to_string(),
            system: "megadrive".to_string(),
            icon: None,
            background: None,
            show_menu: false,
            start_at_menu: false,
            theme: "native".to_string(),
            palette: "blue".to_string(),
            menu_sounds: "off".to_string(),
            controls: rominabox_engine::controls::Controls::default(),
            hotkeys: rominabox_engine::builder::unstated::hotkeys(),
            firmware: Vec::new(),
            splash: false,
            advanced_emulator_access: false,
            intel_macs: false,
            keep_playing_in_background: false,
            autosave_on_quit: false,
            menu_entries: None,
            shaders: rominabox_engine::shaders::ShaderSelection::default(),
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

/// The icon in the player of a Windows kit, as group 1, as in RetroArch,
/// which is a square of one colour, and the resource script that lists
/// it.
const PLAYER_ICON_COLOUR: [u8; 4] = [255, 0, 0, 255];
const PLAYER_ICON_SIZE: u32 = 32;
const PLAYER_ICON_SCRIPT: &str = "1 ICON \"retroarch.ico\"\n";
/// A Windows program that only ends: a kit's launcher, and its player.
const PROGRAM_THAT_ENDS: &str = "int main(void) { return 0; }\n";
/// The Mega Drive core of a Windows kit, a library with only its libretro
/// API version.
const STAND_IN_CORE: &str = "__declspec(dllexport) unsigned retro_api_version(void) { return 1; }\n";
/// The joypad drivers a Windows kit has a controller profile folder for.
const WINDOWS_JOYPAD_DRIVERS: [&str; 2] = ["xinput", "dinput"];

// A Windows kit, which we compile with the toolchain's cc on Windows and with
// zig on macOS and Linux, each in a separate file.
#[cfg(windows)]
mod windows;
#[cfg(windows)]
#[allow(unused_imports)]
pub use windows::{icon_of, library, program, program_from, windows_kit};
#[cfg(unix)]
mod posix;
#[cfg(unix)]
#[allow(unused_imports)]
pub use posix::windows_kit_here;

/// The next `count` bytes at `index`, which we move past them.
fn take<'a>(index: &mut &'a [u8], count: usize) -> &'a [u8] {
    let (head, rest) = index.split_at(count);
    *index = rest;
    head
}

fn number<const N: usize>(index: &mut &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..N].copy_from_slice(take(index, N));
    u64::from_le_bytes(bytes)
}

fn text(index: &mut &[u8]) -> String {
    let length = number::<2>(index) as usize;
    String::from_utf8(take(index, length).to_vec()).unwrap()
}

/// Write into `into` the contents of the one program of a Windows game, as
/// we unpack them at the first launch (`launcher/windows/unpack.c`), with the
/// launcher under its name and every packed file at its path. Returns the
/// folder under the local application data that we unpack into at launch.
pub fn unpack(program: &Path, into: &Path) -> String {
    let bytes = fs::read(program).unwrap();
    let trailer = &bytes[bytes.len() - 24..];
    assert_eq!(&trailer[16..], b"RIBTAIL1", "{} is not a packed game", program.display());
    let start = u64::from_le_bytes(trailer[..8].try_into().unwrap()) as usize;
    let mut index = &bytes[start..bytes.len() - 24];
    assert_eq!(take(&mut index, 8), b"RIBPACK1");
    let runtime = text(&mut index);
    let launcher = text(&mut index);
    take(&mut index, 32);
    for _ in 0..2 {
        let length = number::<4>(&mut index) as usize;
        take(&mut index, length);
    }
    let count = number::<4>(&mut index);
    let mut head = start;
    for _ in 0..count {
        let path = text(&mut index);
        let (offset, packed, size) = (number::<8>(&mut index), number::<8>(&mut index), number::<8>(&mut index));
        take(&mut index, 32 + 1 + 32);
        head = head.min(offset as usize);
        let frame = &bytes[offset as usize..(offset + packed) as usize];
        let mut file = Vec::new();
        std::io::Read::read_to_end(&mut ruzstd::decoding::StreamingDecoder::new(frame).unwrap(), &mut file).unwrap();
        assert_eq!(file.len() as u64, size, "{path}");
        let destination = into.join(&path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, file).unwrap();
    }
    fs::create_dir_all(into).unwrap();
    fs::write(into.join(launcher), &bytes[..head]).unwrap();
    runtime
}
