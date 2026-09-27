//! A Windows export of a stand-in game from a stand-in Windows kit: the game's
//! folder, its contents, what we tell its player, and the resources of its
//! programs. The core, the player and the launcher are actual programs that
//! we compile, so we check actual import tables and resources. The programs
//! do nothing but end.
#![cfg(windows)]

mod export_fixture;

use export_fixture::{export_request_from, kit_base, workspace};
use rominabox_desktop::packaging::{ErrorStage, ExportTarget};
use editpe::constants::{RT_GROUP_ICON, RT_ICON};
use editpe::{Image, ResourceEntryName};
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

/// An `.ico` of one `size`-pixel square of `colour`.
fn icon_of(colour: [u8; 4], size: u32) -> Vec<u8> {
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
fn program(output: &Path, script: Option<&str>) {
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
fn windows_kit(root: &Path) -> std::path::PathBuf {
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
    assert_eq!(names(&game.join("Runtime")), ["retroarch.exe"]);

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

/// The icon groups of the programs, and the icons in them.
fn icons_of(program: &Path) -> (Vec<ResourceEntryName>, usize) {
    let image = Image::parse_file(program).unwrap();
    let resources = image.resource_directory().expect("the program has resources");
    let table = |kind: u16| {
        resources
            .root()
            .get(ResourceEntryName::ID(kind as u32))
            .and_then(|entry| entry.as_table())
            .map(|table| table.entries().into_iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    (table(RT_GROUP_ICON), table(RT_ICON).len())
}

/// The first image of a program's first icon group, decoded.
fn first_icon(program: &Path) -> image::RgbaImage {
    let image = Image::parse_file(program).unwrap();
    let bytes = image.resource_directory().unwrap().get_main_icon().unwrap().unwrap().to_vec();
    image::load_from_memory(&bytes).unwrap().to_rgba8()
}

#[test]
fn windows_shows_the_games_icon_and_name_for_both_its_programs() {
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

    for program in [
        result.app_path.join("Hotkey Isolation.exe"),
        result.app_path.join("Runtime/retroarch.exe"),
    ] {
        let (groups, icons) = icons_of(&program);
        // One group, the one used for RetroArch's window. The player's own
        // icon is no longer there.
        assert_eq!(groups, [ResourceEntryName::ID(1)], "{}", program.display());
        assert_eq!(icons, 7, "every size, and nothing else: {}", program.display());
        let first = first_icon(&program);
        let middle = first.get_pixel(first.width() / 2, first.height() / 2);
        assert_eq!(middle.0, [0, 0, 255, 255], "the game's artwork: {}", program.display());

        let version = Image::parse_file(&program)
            .unwrap()
            .resource_directory()
            .unwrap()
            .get_version_info()
            .unwrap()
            .expect("the program says what it is");
        let strings = &version.strings[0].strings;
        assert_eq!(strings.get("FileDescription").map(String::as_str), Some("Hotkey Isolation"));
        assert_eq!(strings.get("ProductName").map(String::as_str), Some("Hotkey Isolation"));

        // Still a program that can run on Windows.
        let ran = Command::new(&program).status().unwrap();
        assert!(ran.success(), "{} no longer runs", program.display());
    }
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
