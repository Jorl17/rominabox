//! A Windows export of a stand-in game from a stand-in Windows kit. We check
//! the game's one program, what we unpack from it, what we tell its player,
//! and the details about its programs that Windows shows. The core, the
//! player and the launcher are real programs compiled with the toolchain, so
//! we check real import tables and resources. The programs only exit.
#![cfg(windows)]

mod export_fixture;
mod sandboxes;
mod support;

use export_fixture::{export_request_from, library, unpack, windows_kit, workspace};
use rominabox_engine::packaging::{ErrorStage, ExportTarget, LONGEST_LOCAL_APP_DATA, LONGEST_PATH};
use editpe::constants::{RT_GROUP_ICON, RT_ICON};
use editpe::{Image, ResourceEntryName};
use std::{fs, path::Path, process::Command, sync::atomic::AtomicBool};

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
fn a_windows_game_is_one_program_holding_its_resources_and_player() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.game.target = ExportTarget::Windows;
    let cancelled = AtomicBool::new(false);
    let result = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {}).unwrap();

    assert_eq!(names(&request.output_dir), ["Hotkey Isolation.exe"]);
    assert_eq!(result.app_path, request.output_dir.join("Hotkey Isolation.exe"));
    let game = root.join("unpacked");
    let runtime = unpack(&result.app_path, &game);
    assert!(runtime.starts_with("ROM-in-a-Box/Runtimes/"), "{runtime}");
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
    request.game.target = ExportTarget::Windows;
    let artwork = root.join("artwork.png");
    image::RgbaImage::from_pixel(64, 48, image::Rgba([0, 0, 255, 255]))
        .save(&artwork)
        .unwrap();
    request.game.icon = Some(artwork);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    let game = root.join("unpacked");
    unpack(&result.app_path, &game);

    // The program that the person opens, and the player unpacked from it.
    for program in [result.app_path.clone(), game.join("Runtime/retroarch.exe")] {
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

/// Libraries that come with Windows, as listed in the imports of cores in the
/// catalog (Flycast, PCSX2 and Dolphin, as published on the buildbot). We
/// export a core that requires them.
#[test]
fn a_core_that_needs_only_what_windows_carries_is_exported() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.game.target = ExportTarget::Windows;
    library(
        &request
            .runtime_kit
            .join("cores/genesis_plus_gx_libretro.dll"),
        "#include <winsock2.h>\n#include <mswsock.h>\n#include <d3dcompiler.h>\n#include <dxgi.h>\n\
         #define SECURITY_WIN32\n#include <security.h>\n\
         __declspec(dllexport) void *windows_parts[] = {\n\
             (void *)AcceptEx, (void *)D3DCompile, (void *)CreateDXGIFactory, (void *)GetUserNameExW};\n",
        &[
            Path::new("-lmswsock"),
            Path::new("-ld3dcompiler_47"),
            Path::new("-ldxgi"),
            Path::new("-lsecur32"),
        ],
    );
    let cancelled = AtomicBool::new(false);
    let exported = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {});
    assert!(exported.is_ok(), "{:?}", exported.err().map(|error| error.message));
}

#[test]
fn a_core_that_needs_a_library_windows_lacks_is_refused() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.game.target = ExportTarget::Windows;
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
    let error = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {})
        .expect_err("a game whose core cannot load must not be made");

    // The fault is in the downloaded core, not in the builder, so we tell the
    // author the library, and do not ask them to reinstall ROM-in-a-Box.
    assert_eq!(error.stage, ErrorStage::CoreLibraries);
    assert_eq!(
        error.sentence(),
        "The emulator for this console needs helper.dll, which Windows does not include, \
         so the game would not start. Try again later: a newer version of the emulator may not need it."
    );
    assert_eq!(names(&request.output_dir), Vec::<String>::new());
}

/// A game with every shader in the catalogue, in either language. Every file
/// we unpack from it stays within the longest path that Windows can open, at
/// the place it will be read from, under the longest per-user data folder.
#[test]
fn every_file_of_a_game_with_every_shader_fits_windows_path_limit() {
    let root = workspace();
    let kit = windows_kit(&root);
    for (from, to) in [
        ("integrations/designs", "designs"),
        ("integrations/parts", "parts"),
        ("desktop/assets/controllers", "menu-assets"),
    ] {
        support::copy_tree(&rominabox_engine::repo::at(from), &kit.join(to));
    }
    support::with_shader_library(&kit);
    let every: Vec<String> = rominabox_engine::shaders::catalog()
        .unwrap()
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    // Without the shader that libretro has only in slang, we export the game with GLSL.
    let glsl: Vec<String> = every.iter().filter(|id| *id != "crt-guest-advanced").cloned().collect();
    for (language, bundled) in [("slang", every.clone()), ("glsl", glsl)] {
        let mut request = export_request_from(&root, kit.clone());
        request.game.target = ExportTarget::Windows;
        request.game.show_menu = true;
        request.game.shaders.bundled = bundled;
        request.output_dir = root.join(language);
        let cancelled = AtomicBool::new(false);
        let result = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
        let game = root.join(format!("unpacked-{language}"));
        let runtime = unpack(&result.app_path, &game);
        let mut folders = vec![game.clone()];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    folders.push(path);
                    continue;
                }
                let inside = path.strip_prefix(&game).unwrap().to_string_lossy().into_owned();
                let read_from = format!("{LONGEST_LOCAL_APP_DATA}\\{}\\{inside}", runtime.replace('/', "\\"));
                assert!(read_from.len() <= LONGEST_PATH, "{} characters: {read_from}", read_from.len());
            }
        }
    }
}

/// When a game's program is damaged, we show an error and stop, and remove
/// everything we unpacked in the per-user folder. Otherwise every launch would
/// leave another partly unpacked copy there. The launcher is the real one,
/// built from this tree, and the per-user folder is the test's own.
#[test]
fn a_damaged_game_leaves_nothing_of_its_unpack_behind() {
    let root = workspace();
    let kit = windows_kit(&root);
    let built = Command::new(rominabox_engine::repo::python())
        .arg(rominabox_engine::repo::at("scripts/build_launcher.py"))
        .arg(root.join("launcher"))
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    // The output contains the path of the launcher from the last build.
    let launcher = String::from_utf8(built.stdout).unwrap();
    fs::copy(launcher.lines().last().unwrap().trim(), kit.join("bin/launcher.exe")).unwrap();
    let mut request = export_request_from(&root, kit);
    request.game.target = ExportTarget::Windows;
    let cancelled = AtomicBool::new(false);
    let result = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    let runtime = unpack(&result.app_path, &root.join("unpacked"));

    // The last byte of the last packed file, which ends where the index starts.
    let mut bytes = fs::read(&result.app_path).unwrap();
    let trailer = bytes.len() - 24;
    let index = u64::from_le_bytes(bytes[trailer..trailer + 8].try_into().unwrap()) as usize;
    bytes[index - 1] ^= 0xFF;
    fs::write(&result.app_path, &bytes).unwrap();

    let user_data = root.join("user-data");
    fs::create_dir_all(&user_data).unwrap();
    let ran = Command::new(&result.app_path)
        .env(sandboxes::declared("RIB_ENV_TEST_USER_DATA"), &user_data)
        .env("ROMINABOX_QUIET", "1")
        .env("ROMINABOX_PLAN_ONLY", "1")
        .output()
        .unwrap();
    let runtimes = user_data.join(&runtime).parent().unwrap().to_path_buf();
    assert_eq!(ran.status.code(), Some(1));
    // We unpacked the game into the test's folder, where the new folders remain.
    assert!(runtimes.is_dir(), "{} was not made", runtimes.display());
    let left: Vec<_> = fs::read_dir(&runtimes).unwrap().map(|entry| entry.unwrap().path()).collect();
    assert!(String::from_utf8_lossy(&ran.stderr).contains("damaged"), "{}", String::from_utf8_lossy(&ran.stderr));
    assert_eq!(left, Vec::<std::path::PathBuf>::new());
}
