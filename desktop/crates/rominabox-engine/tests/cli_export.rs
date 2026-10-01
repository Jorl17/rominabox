//! With only a game, `export` makes the same game as dropping it into the
//! builder. We identify it as in the builder and use the builder's settings
//! for everything else, and anything the request states takes precedence. We
//! print what the author sees in the details step, which is the lookup, the
//! files that go with the game, and the BIOS assessment. With only a game,
//! `project-save` saves the same project as the builder.
//!
//! These tests do not use the network. The lookup is off (`online: false`),
//! and we write the catalogue and the cover into the cache as a lookup leaves
//! them. The kit is the stand-in kit, with the files beside it that the
//! builder's settings need, which are the designs, the controller artwork,
//! the logo, and a player that reports support for achievements.
#![cfg(any(windows, target_os = "macos"))]

mod export_fixture;
mod support;

use rominabox_engine::builder::defaults;
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
        support::copy_tree(&rominabox_engine::repo::at(from), &kit.join(to));
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
    run("export", request)
}

/// Whether `command` succeeded, its last line of output, and all its output.
fn run(command: &str, request: &Value) -> (bool, Value, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg(command)
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

/// The `kind` lines printed, each as its payload.
fn events(printed: &str, kind: &str) -> Vec<Value> {
    printed
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["type"] == kind)
        .map(|line| line[kind].clone())
        .collect()
}

/// The one `kind` line printed, as its payload.
fn event(printed: &str, kind: &str) -> Value {
    match events(printed, kind).as_slice() {
        [one] => one.clone(),
        other => panic!("{} {kind} lines were printed: {printed}", other.len()),
    }
}

/// A Windows game is one program, and its files are the ones packed in it.
/// Here we unpack them into a separate folder beside it for each look.
#[cfg(windows)]
fn resources(app: &Path) -> PathBuf {
    static LOOKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let look = LOOKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let unpacked = app.with_extension(format!("unpacked-{look}"));
    export_fixture::unpack(app, &unpacked);
    unpacked.join("Resources")
}

#[cfg(target_os = "macos")]
fn resources(app: &Path) -> PathBuf {
    app.join("Contents/Resources")
}

/// Whether the game's icon is the cover, from the middle of the program's icon.
#[cfg(windows)]
fn shows_the_cover(app: &Path) -> bool {
    let image = editpe::Image::parse_file(app).unwrap();
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
    assert!(game.get("description").is_none(), "{game}");
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

    // What the author sees in the details step, the lookup and the files that
    // go with the game. A Mega Drive runs without a BIOS, so there is no assessment.
    let identified = event(&printed, "identified");
    assert_eq!(identified["title"], TITLE, "{identified}");
    assert_eq!(identified["system"], "megadrive", "{identified}");
    assert_eq!(identified["matched"], true, "{identified}");
    assert_eq!(identified["catalogName"], NAME, "{identified}");
    assert!(identified["iconPath"].is_string(), "{identified}");
    assert!(identified["warnings"].is_array(), "{identified}");
    let content = event(&printed, "content");
    assert_eq!(content["files"], json!(["cartridge.md"]), "{content}");
    assert!(events(&printed, "firmware").is_empty(), "{printed}");
}

/// We show the lookup warnings under More details in the builder, and print
/// them at export.
#[test]
fn the_lookups_warnings_are_printed() {
    let root = Scratch::dir("rominabox-cli-export-warnings");
    let mut uncached = request(root.path());
    let empty = root.path().join("nothing-cached");
    fs::create_dir_all(&empty).unwrap();
    uncached["metadataCache"] = json!(empty);
    let (exported, _, printed) = export(&uncached);
    assert!(exported, "{printed}");
    let identified = event(&printed, "identified");
    assert_eq!(identified["matched"], false, "{identified}");
    let warnings: Vec<&str> = identified["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        warnings.iter().any(|warning| warning.contains(&format!("{CATALOG} checksum catalog is not cached"))),
        "{identified}"
    );
}

/// Beside the BIOS picker in the builder we show why a file did not count, and
/// at export we print the same assessment for a console with a required BIOS.
#[test]
fn the_bios_assessment_is_printed() {
    let root = Scratch::dir("rominabox-cli-export-bios");
    let disc = root.path().join("disc.bin");
    fs::write(&disc, vec![0u8; 2352 * 16]).unwrap();
    let sheet = root.path().join("disc.cue");
    fs::write(&sheet, "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n").unwrap();
    let notes = root.path().join("notes.txt");
    fs::write(&notes, "not a BIOS").unwrap();
    let request = json!({
        "rom": sheet,
        "title": "Disc",
        "system": "ps1",
        "firmware": [notes],
        "runtimeKit": kit(root.path()),
        "coreCache": null,
        "outputDir": root.path().join("out"),
    });
    // There is no PlayStation core in the stand-in kit, so the export stops
    // at the kit check. We print the assessment before the export starts.
    let (_, _, printed) = export(&request);
    let firmware = event(&printed, "firmware");
    assert_eq!(firmware["files"][0]["name"], "notes.txt", "{firmware}");
    assert_eq!(firmware["files"][0]["counted"], false, "{firmware}");
    assert!(firmware["files"][0]["reason"].is_string(), "{firmware}");
    let content = event(&printed, "content");
    assert_eq!(content["files"], json!(["disc.cue", "disc.bin"]), "{content}");
    // The request contains the title and console, so we do not look it up.
    assert!(events(&printed, "identified").is_empty(), "{printed}");
}

/// With only a game, `project-save` saves the same as dropping the game into
/// the builder and saving the project, which is the title, console and cover
/// from the lookup, the builder's settings, and the host's platform.
#[test]
fn a_game_alone_is_saved_as_the_builder_saves_it() {
    let root = Scratch::dir("rominabox-cli-project-alone");
    let (rom, cache) = catalogued(root.path());
    let archive = root.path().join("game.rominabox");
    let (saved, _, printed) = run(
        "project-save",
        &json!({
            "archivePath": archive,
            "settings": { "rom": rom, "online": false, "metadataCache": cache },
        }),
    );
    assert!(saved, "{printed}");
    assert_eq!(event(&printed, "identified")["title"], TITLE, "{printed}");

    let opened = rominabox_engine::projects::open_project(&rominabox_engine::projects::ProjectOpenRequest {
        archive_path: archive,
        extraction_dir: root.path().join("opened"),
    })
    .unwrap();
    let game = serde_json::to_value(&opened.settings).unwrap();
    assert_eq!(game["title"], TITLE, "{game}");
    assert_eq!(game["system"], "megadrive", "{game}");
    assert!(game["icon"].is_string(), "the cover was not saved: {game}");
    assert_eq!(
        game["target"],
        json!(rominabox_engine::packaging::ExportTarget::of_host().unwrap()),
        "{game}"
    );
    let builder = defaults();
    assert_eq!(game["splash"], builder.splash, "{game}");
    assert_eq!(game["theme"], builder.theme, "{game}");
    assert_eq!(game["palette"], builder.palette, "{game}");
}

#[test]
fn what_a_request_states_wins_over_the_lookup_and_the_defaults() {
    let root = Scratch::dir("rominabox-cli-export-stated");
    let mut stated = request(root.path());
    let builder = defaults();
    stated["title"] = json!("Named By Hand");
    stated["icon"] = Value::Null;
    stated["splash"] = json!(!builder.splash);
    stated["showMenu"] = json!(!builder.show_menu);
    let (exported, last, printed) = export(&stated);
    assert!(exported, "{printed}");
    let game = game(&last);

    assert_eq!(game["title"], "Named By Hand", "{game}");
    // The request does not state it, so we still look it up.
    assert_eq!(game["system"], "megadrive", "{game}");
    let app = PathBuf::from(last["result"]["appPath"].as_str().unwrap());
    assert!(!shows_the_cover(&app), "a request without a cover got the cover");
    assert_eq!(game["splash"], !builder.splash, "{game}");
    assert_eq!(game["showMenu"], !builder.show_menu, "{game}");
    // The request does not state it, so we use the builder's.
    assert_eq!(game["palette"], builder.palette, "{game}");
}

/// We do not look up a game when the request contains its title and console.
/// In the size tests we export a seven-byte stand-in named `.bin` as a Mega
/// Drive game. We would reject it at lookup, where Mega Drive files are not
/// `.bin`, although we accept it at export.
#[test]
fn a_request_that_names_its_game_is_not_looked_up() {
    let root = Scratch::dir("rominabox-cli-export-named-game");
    let rom = root.path().join("stand-in.bin");
    fs::write(&rom, b"RIBsize").unwrap();
    let lookup = root.path().join("lookup");
    fs::create_dir_all(&lookup).unwrap();
    let request = json!({
        "rom": rom,
        "title": "Stand-in",
        "system": "megadrive",
        // If we did look it up, the lookup would stay offline, with nothing cached.
        "online": false,
        "metadataCache": lookup,
        "runtimeKit": kit(root.path()),
        "coreCache": null,
        "outputDir": root.path().join("out"),
    });
    let (exported, last, printed) = export(&request);
    assert!(exported, "{printed}");
    let game = game(&last);
    assert_eq!(game["title"], "Stand-in", "{game}");
    assert_eq!(game["system"], "megadrive", "{game}");
    assert_eq!(game["splash"], defaults().splash, "{game}");
    assert!(events(&printed, "identified").is_empty(), "{printed}");
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
