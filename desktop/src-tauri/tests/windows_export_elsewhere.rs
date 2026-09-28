//! A Windows game made on another platform, for example with the builder on
//! a Mac. It has the same folder, core, controller profiles and launch plan
//! as from a Windows builder, and we write the game's icon and name into its
//! programs. The programs are real Windows programs, compiled for Windows
//! with zig. We run them only in the tests on Windows
//! (`windows_export.rs`).
#![cfg(not(windows))]

mod export_fixture;

use editpe::constants::{RT_GROUP_ICON, RT_ICON};
use editpe::{Image, ResourceEntryName};
use export_fixture::{export_request_from, kit_base, workspace};
use rominabox_desktop::packaging::ExportTarget;
use std::{fs, path::Path, process::Command, sync::atomic::AtomicBool};

/// Compiles `source` for Windows with zig, with `extra` (a resource script, a
/// flag) beside it.
fn for_windows(output: &Path, source: &str, extra: &[&str]) {
    let file = output.with_extension("c");
    fs::write(&file, source).unwrap();
    let status = Command::new("zig")
        .args(["cc", "-target", "x86_64-windows-gnu", "-O2", "-o"])
        .arg(output)
        .arg(&file)
        .args(extra)
        .current_dir(output.parent().unwrap())
        .status()
        .expect("zig builds Windows programs on this machine; install it (brew install zig)");
    assert!(status.success(), "could not compile {}", output.display());
}

/// A Windows kit as we make it in a Windows build: a player with an icon as
/// group 1, as in RetroArch, a launcher with no resources, one controller
/// profile per driver folder, and the Mega Drive core.
fn windows_kit(root: &Path) -> std::path::PathBuf {
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

fn names(directory: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_windows_game_made_here_is_the_folder_a_windows_builder_makes() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.target = ExportTarget::Windows;
    let artwork = root.join("artwork.png");
    image::RgbaImage::from_pixel(64, 48, image::Rgba([0, 0, 255, 255]))
        .save(&artwork)
        .unwrap();
    request.icon = Some(artwork);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();

    let game = request.output_dir.join("Hotkey Isolation");
    assert_eq!(result.app_path, game);
    assert_eq!(names(&game), ["Hotkey Isolation.exe", "Resources", "Runtime"]);
    assert_eq!(names(&game.join("Runtime")), ["retroarch.exe"]);
    let resources = game.join("Resources");
    assert_eq!(
        fs::read(resources.join("game-core.dll")).unwrap(),
        fs::read(request.runtime_kit.join("cores/genesis_plus_gx_libretro.dll")).unwrap()
    );
    assert!(!resources.join("game-core.dylib").exists());
    assert_eq!(fs::read(resources.join("content/sonic.bin")).unwrap(), b"RIBtest");
    for driver in ["xinput", "dinput"] {
        assert!(resources.join("autoconfig").join(driver).join("pad.cfg").is_file(), "{driver}");
    }
    let plan = fs::read_to_string(resources.join("launch.plan")).unwrap();
    assert!(plan.contains("input_joypad_driver = \"xinput\""), "{plan}");

    for program in [game.join("Hotkey Isolation.exe"), game.join("Runtime/retroarch.exe")] {
        let image = Image::parse_file(&program).unwrap();
        let resources = image.resource_directory().expect("the program has resources");
        let table = |kind: u16| {
            resources
                .root()
                .get(ResourceEntryName::ID(kind as u32))
                .and_then(|entry| entry.as_table())
                .map(|table| table.entries().into_iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        assert_eq!(table(RT_GROUP_ICON), [ResourceEntryName::ID(1)], "{}", program.display());
        assert_eq!(table(RT_ICON).len(), 7, "every size: {}", program.display());
        let icon = resources.get_main_icon().unwrap().unwrap().to_vec();
        let first = image::load_from_memory(&icon).unwrap().to_rgba8();
        let middle = first.get_pixel(first.width() / 2, first.height() / 2);
        assert_eq!(middle.0, [0, 0, 255, 255], "the game's artwork: {}", program.display());
        let version = resources.get_version_info().unwrap().expect("the program says what it is");
        let strings = &version.strings[0].strings;
        assert_eq!(strings.get("FileDescription").map(String::as_str), Some("Hotkey Isolation"));
        assert_eq!(strings.get("ProductName").map(String::as_str), Some("Hotkey Isolation"));
    }
}
