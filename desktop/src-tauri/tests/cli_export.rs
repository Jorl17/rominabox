//! With only a game, `export` makes the same game as dropping it into the
//! builder, identified in the same way and with the builder settings for
//! everything else. A value in the request replaces the builder setting.
//!
//! We make no network request here. The lookup is off (`online: false`), and
//! we write the catalogue and the cover into the cache as a lookup would. The
//! kit is the stand-in kit with the files required for the builder settings,
//! which are the designs, the controller artwork, the logo, and a player with
//! achievements support.
#![cfg(any(windows, target_os = "macos"))]

mod export_fixture;
mod support;

use rominabox_desktop::builder::defaults;
use rominabox_scratch::Scratch;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CATALOG: &str = "Sega - Mega Drive - Genesis";
const NAME: &str = "ROM-in-a-Box Test Cartridge (World)";
/// The title the builder shows for that catalogue entry.
const TITLE: &str = "ROM-in-a-Box Test Cartridge";
const DESCRIPTION: &str = "A cartridge made for this test";
/// The cover's colour, which the game's icon has when the cover is its icon.
const COVER: [u8; 4] = [0, 200, 80, 255];

/// A generated cartridge, and a lookup cache with its catalogue entry and
/// its cover.
fn catalogued(root: &Path) -> (PathBuf, PathBuf) {
    // The Mega Drive header's console name, then a pattern no real dump has.
    let mut cartridge: Vec<u8> = (0..65536u32).map(|i| (i * 7 % 251) as u8).collect();
    cartridge[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    let rom = root.join("cartridge.md");
    fs::write(&rom, &cartridge).unwrap();

    let cache = root.join("lookup");
    fs::create_dir_all(cache.join("catalogs")).unwrap();
    fs::write(
        cache.join("catalogs").join(format!("{CATALOG}.dat")),
        format!(
            "clrmamepro (\n\tname \"{CATALOG}\"\n)\n\ngame (\n\tname \"{NAME}\"\n\tdescription \"{DESCRIPTION}\"\n\trom ( name \"{NAME}.md\" size {} crc {:08X} )\n)\n",
            cartridge.len(),
            crc32fast::hash(&cartridge),
        ),
    )
    .unwrap();
    fs::create_dir_all(cache.join("artwork-index")).unwrap();
    fs::write(cache.join("artwork-index").join(format!("{CATALOG}.txt")), NAME).unwrap();
    let covers = cache.join("artwork").join(CATALOG).join("Named_Boxarts");
    fs::create_dir_all(&covers).unwrap();
    image::RgbaImage::from_pixel(64, 64, image::Rgba(COVER))
        .save(covers.join(format!("{NAME}.png")))
        .unwrap();
    (rom, cache)
}

/// The stand-in kit, with the files beside it that the builder's settings need.
fn kit(root: &Path) -> PathBuf {
    #[cfg(windows)]
    let kit = export_fixture::windows_kit(root);
    #[cfg(target_os = "macos")]
    let kit = export_fixture::fixture_kit(root);
    for (from, to) in [
        ("integrations/designs", "designs"),
        ("integrations/parts", "parts"),
        ("desktop/assets/controllers", "menu-assets"),
        ("desktop/assets/branding", "branding"),
    ] {
        support::copy_tree(&rominabox_desktop::repo::at(from), &kit.join(to));
    }
    let manifest = kit.join("manifest.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["components"][0]["capabilities"] = json!({ "achievements": true });
    fs::write(manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    kit
}

/// A request with only the game and what a test must name, which is the
/// lookup cache, offline lookup, the stand-in kit, the core in the kit
/// instead of the builder's cache, and a folder for the test.
fn request(root: &Path) -> Value {
    let (rom, cache) = catalogued(root);
    json!({
        "rom": rom,
        "online": false,
        "metadataCache": cache,
        "runtimeKit": kit(root),
        "coreCache": null,
        "outputDir": root.join("out"),
    })
}

/// Whether the export succeeded, its last line of output, and all its output.
fn export(request: &Value) -> (bool, Value, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg("export")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start rominabox-cli");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(request.to_string().as_bytes())
        .expect("send the request");
    let output = child.wait_with_output().expect("wait for rominabox-cli");
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let last = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or(Value::Null);
    (output.status.success(), last, printed)
}

#[cfg(windows)]
fn resources(app: &Path) -> PathBuf {
    app.join("Resources")
}

#[cfg(target_os = "macos")]
fn resources(app: &Path) -> PathBuf {
    app.join("Contents/Resources")
}

/// Whether the game icon is the cover, judged by the middle of the program icon.
#[cfg(windows)]
fn shows_the_cover(app: &Path) -> bool {
    let program = fs::read_dir(app)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|extension| extension == "exe"))
        .expect("the game's program");
    let image = editpe::Image::parse_file(&program).unwrap();
    let Some(icon) = image
        .resource_directory()
        .and_then(|resources| resources.get_main_icon().unwrap())
    else {
        return false;
    };
    let icon = image::load_from_memory(icon).unwrap().to_rgba8();
    icon.get_pixel(icon.width() / 2, icon.height() / 2).0 == COVER
}

/// Whether the game's icon is the cover. There is no icon in the stand-in
/// kit, so an app has an icon only when the game has one.
#[cfg(target_os = "macos")]
fn shows_the_cover(app: &Path) -> bool {
    resources(app).join("GameIcon.icns").is_file()
}

/// The identity recorded in the exported game.
fn game(result: &Value) -> Value {
    let app = PathBuf::from(result["result"]["appPath"].as_str().expect("an app path"));
    serde_json::from_slice(&fs::read(resources(&app).join("game.json")).unwrap()).unwrap()
}

#[test]
fn a_game_alone_is_exported_as_the_builder_makes_it() {
    let root = Scratch::dir("rominabox-cli-export-alone");
    let (exported, last, printed) = export(&request(root.path()));
    assert!(exported, "{printed}");
    let game = game(&last);

    assert_eq!(game["title"], TITLE, "{game}");
    assert_eq!(game["system"], "megadrive", "{game}");
    assert_eq!(game["description"], DESCRIPTION, "{game}");
    let app = PathBuf::from(last["result"]["appPath"].as_str().unwrap());
    assert!(shows_the_cover(&app), "the cover is not the game's icon");

    let builder = defaults();
    assert_eq!(game["showMenu"], builder.show_menu, "{game}");
    assert_eq!(game["startAtMenu"], builder.start_at_menu, "{game}");
    assert_eq!(game["splash"], builder.splash, "{game}");
    assert_eq!(game["includeAchievements"], builder.include_achievements, "{game}");
    assert_eq!(game["keepPlayingInBackground"], builder.keep_playing_in_background, "{game}");
    assert_eq!(game["autosaveOnQuit"], builder.autosave_on_quit, "{game}");
    assert_eq!(game["advancedEmulatorAccess"], builder.advanced_emulator_access, "{game}");
    assert_eq!(game["theme"], builder.theme, "{game}");
    assert_eq!(game["palette"], builder.palette, "{game}");
    assert_eq!(game["menuSounds"], builder.menu_sounds, "{game}");
}

#[test]
fn what_a_request_states_wins_over_the_lookup_and_the_defaults() {
    let root = Scratch::dir("rominabox-cli-export-stated");
    let mut stated = request(root.path());
    let builder = defaults();
    stated["title"] = json!("Named By Hand");
    stated["description"] = json!("Written by hand");
    stated["icon"] = Value::Null;
    stated["splash"] = json!(!builder.splash);
    stated["showMenu"] = json!(!builder.show_menu);
    let (exported, last, printed) = export(&stated);
    assert!(exported, "{printed}");
    let game = game(&last);

    assert_eq!(game["title"], "Named By Hand", "{game}");
    assert_eq!(game["description"], "Written by hand", "{game}");
    // The request does not state it, so we still look it up.
    assert_eq!(game["system"], "megadrive", "{game}");
    let app = PathBuf::from(last["result"]["appPath"].as_str().unwrap());
    assert!(!shows_the_cover(&app), "a request without a cover got the cover");
    assert_eq!(game["splash"], !builder.splash, "{game}");
    assert_eq!(game["showMenu"], !builder.show_menu, "{game}");
    // The request does not state it, so we use the builder's.
    assert_eq!(game["palette"], builder.palette, "{game}");
}

/// `export GAME` is the whole request. When stdin is a pipe nobody writes to,
/// the command still ends, here on the game we gave it, which does not exist.
#[test]
fn a_game_named_on_the_command_line_is_the_request_and_stdin_is_not_read() {
    let root = Scratch::dir("rominabox-cli-export-named");
    let missing = root.path().join("no-such-game.md");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg("export")
        .arg(&missing)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start rominabox-cli");
    let held = child.stdin.take();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for rominabox-cli") {
            break status;
        }
        if started.elapsed() > Duration::from_secs(10) {
            child.kill().ok();
            child.wait().ok();
            panic!("export GAME waited on stdin");
        }
        thread::sleep(Duration::from_millis(20));
    };
    drop(held);
    let mut printed = String::new();
    child
        .stdout
        .take()
        .expect("stdout is piped")
        .read_to_string(&mut printed)
        .unwrap();
    assert!(!status.success(), "{printed}");
    assert!(printed.contains("no-such-game.md"), "{printed}");
}
