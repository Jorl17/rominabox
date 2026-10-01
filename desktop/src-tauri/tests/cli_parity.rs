//! We use the same functions in the builder's backend commands and in the
//! command line for where we put games and downloads, the hotkeys check, the
//! controller variants in the Controls step, a lookup in the builder's cache,
//! the preview in the Menu step, the refusal of a project with a palette this
//! build lacks, and the warnings on an author's filter that a Windows game may
//! not load.

use rominabox_desktop::builder::{self, defaults, Places};
use rominabox_desktop::game::Game;
use rominabox_desktop::packaging::ExportTarget;
use rominabox_desktop::projects::{save_project, ProjectSaveRequest};
use rominabox_scratch::Scratch;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A builder identifier that no builder uses, so we do not read or write the
/// folders of an installed builder or of a development checkout.
const IDENTIFIER: &str = "com.rominabox.test.cli-parity";

/// Whether the command succeeded, each line of its output, and all its output.
fn run(command: &str, request: Option<&Value>) -> (bool, Vec<Value>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg(command)
        .env("ROMINABOX_BUNDLE_ID", IDENTIFIER)
        .stdin(if request.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start rominabox-cli");
    if let Some(request) = request {
        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(request.to_string().as_bytes())
            .expect("send the request");
    }
    let output = child.wait_with_output().expect("wait for rominabox-cli");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let lines = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{command} printed {line:?}: {e}")))
        .collect();
    let printed = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
    (output.status.success(), lines, printed)
}

fn result(command: &str, request: Option<&Value>) -> Value {
    let (succeeded, lines, printed) = run(command, request);
    assert!(succeeded, "{command}: {printed}");
    let last = lines.last().cloned().unwrap_or(Value::Null);
    assert_eq!(last["type"], "result", "{command}: {printed}");
    last["result"].clone()
}

/// A Mega Drive cartridge that we can identify by its header.
fn cartridge(root: &Path) -> PathBuf {
    let mut cartridge: Vec<u8> = (0..65536u32).map(|i| (i * 7 % 251) as u8).collect();
    cartridge[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    let rom = root.join("cartridge.md");
    fs::write(&rom, &cartridge).unwrap();
    rom
}

/// `default_destination` and `export_target`, and the folders an export
/// request leaves to the builder.
#[test]
fn places_prints_where_the_builder_writes_games_and_keeps_downloads() {
    let places = Places::of(IDENTIFIER);
    let host = ExportTarget::of_host().expect("the tests run on a platform the builder makes games on");
    let core_target = host.target().expect("the host platform has a core target");
    let mut printed = result("places", None);
    // We look for the kit beside the running program, and this test is not
    // that program, so here we only check that it is a kit.
    let kit = PathBuf::from(printed["runtimeKit"].as_str().expect("a runtime kit"));
    assert!(kit.join("manifest.json").is_file(), "{}", kit.display());
    printed.as_object_mut().unwrap().remove("runtimeKit");
    assert_eq!(
        printed,
        json!({
            "outputDir": builder::destination().unwrap(),
            "target": host,
            "coreCache": places.core_cache(core_target).unwrap(),
            "metadataCache": places.metadata_cache().unwrap(),
        })
    );
}

/// `check_hotkeys` with the builder's defaults for what a request leaves
/// out, and the refusal we give at export, with its sentence, for the rules
/// of the hotkeys and, with a game, the rule its keys add.
#[test]
fn hotkeys_check_prints_the_hotkeys_or_the_refusal() {
    let declared = serde_json::to_value(&defaults().hotkeys).unwrap();
    let kept = result("hotkeys-check", Some(&json!({ "hotkeys": { "menu": ["key:f1", "pad:home"] } })));
    assert_eq!(kept["hotkeys"]["menu"], json!(["key:f1", "pad:home"]), "{kept}");
    for hotkey in ["confirm", "back", "quick-save", "quick-load", "previous-slot", "next-slot"] {
        assert_eq!(kept["hotkeys"][hotkey], declared[hotkey], "{hotkey}: {kept}");
    }

    // The pad's right face button is already bound to BACK.
    let refusal = |request: Value| {
        let (succeeded, lines, printed) = run("hotkeys-check", Some(&request));
        assert!(!succeeded, "{printed}");
        let refused = lines.last().unwrap().clone();
        assert_eq!(refused["type"], "error", "{printed}");
        refused
    };
    let shared = refusal(json!({ "hotkeys": { "confirm": ["key:enter", "pad:a"] } }));
    assert_eq!(shared["refusal"]["kind"], "shared", "{shared}");
    assert_eq!(shared["refusal"]["binding"], "pad:a", "{shared}");
    assert!(shared["message"].as_str().unwrap().contains("cannot share an input"), "{shared}");

    // F2 for QUICK SAVE, which is also C in a Mega Drive game.
    let game = json!({ "system": "megadrive", "controls": { "bindings": { "a": { "key": "f2" } } } });
    let taken = refusal(game.clone());
    assert_eq!(
        taken["refusal"],
        json!({ "kind": "gameKey", "binding": "key:f2", "hotkey": "quick-save", "control": "a", "label": "C" }),
        "{taken}"
    );
    let moved = result("hotkeys-check", Some(&json!({ "hotkeys": { "quick-save": ["key:f5"] }, "system": "megadrive", "controls": game["controls"] })));
    assert_eq!(moved["hotkeys"]["quick-save"], json!(["key:f5"]), "{moved}");
}

/// In `inspect_game` we look in the builder's cache, which the author
/// never names. We cache nothing under this identifier and keep the
/// lookup offline, so we identify the console from the header. The
/// controller variants we offer for a console in the Controls step, each
/// of which `controls` describes when the request contains it.
#[test]
fn controls_lists_the_variants_a_console_offers() {
    let variants = |answer: &Value| -> Vec<String> {
        answer["variants"].as_array().unwrap().iter().map(|variant| variant["id"].as_str().unwrap().to_owned()).collect()
    };
    let megadrive = result("controls", Some(&json!({ "system": "megadrive" })));
    assert_eq!(megadrive["id"], "megadrive");
    assert_eq!(variants(&megadrive), ["megadrive", "megadrive6"]);
    let six = result("controls", Some(&json!({ "system": "megadrive", "profile": "megadrive6" })));
    assert_eq!(six["id"], "megadrive6");
    let snes = result("controls", Some(&json!({ "system": "snes" })));
    assert_eq!(variants(&snes), [snes["id"].as_str().unwrap()]);
}

#[test]
fn inspect_without_a_cache_looks_up_in_the_builders() {
    let root = Scratch::dir("rominabox-cli-inspect-builders-cache");
    let rom = cartridge(root.path());
    let found = result("inspect", Some(&json!({ "rom": rom, "online": false })));
    assert_eq!(found["system"], "megadrive", "{found}");
    assert_eq!(found["matched"], false, "{found}");
}

fn pixels(path: &Path) -> image::RgbaImage {
    image::open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .to_rgba8()
}

/// `menu_preview`: the Menu step's picture from a design's id, with the
/// builder's design, palette, controller artwork, renderer and size for what
/// the request leaves out.
#[test]
fn preview_draws_the_builders_menu_picture_from_a_design_id() {
    let root = Scratch::dir("rominabox-cli-preview-builders");
    let drawn = |request: Value| -> PathBuf {
        let result = result("preview", Some(&request));
        PathBuf::from(result["imagePath"].as_str().expect("an image path"))
    };
    let unstated = pixels(&drawn(json!({ "outputDir": root.path().join("unstated") })));
    assert_eq!(unstated.dimensions(), (960, 600));
    let named = pixels(&drawn(json!({
        "outputDir": root.path().join("named"),
        "theme": defaults().theme,
        "palette": defaults().palette,
    })));
    assert!(unstated == named, "the unstated preview is not the builder's design and palette");
    let registry = rominabox_desktop::themes::registry().unwrap();
    let other = &registry
        .palettes
        .iter()
        .find(|palette| palette.id != defaults().palette)
        .expect("a second palette")
        .id;
    let recoloured = pixels(&drawn(json!({ "outputDir": root.path().join("other"), "palette": other })));
    assert!(recoloured != unstated, "the palette {other} drew the same picture");
}

/// `archive` with its manifest's game changed by `change`.
fn rewritten(archive: &Path, change: impl Fn(&mut Value)) {
    let mut entries = Vec::new();
    {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if entry.name() == "manifest.json" {
                let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
                change(&mut manifest["game"]);
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            entries.push((entry.name().to_string(), bytes));
        }
    }
    let mut zip = zip::ZipWriter::new(fs::File::create(archive).unwrap());
    for (name, bytes) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
}

/// We refuse to open a project with a menu palette this build does not have,
/// in the builder and in `project-open`.
#[test]
fn project_open_refuses_a_palette_this_build_lacks() {
    let root = Scratch::dir("rominabox-cli-project-palette");
    let archive = root.path().join("game.rominabox");
    let game: Game = serde_json::from_value(json!({
        "rom": cartridge(root.path()),
        "title": "Cartridge",
        "system": "megadrive",
        "target": ExportTarget::of_host().unwrap(),
    }))
    .unwrap();
    save_project(&ProjectSaveRequest {
        archive_path: archive.clone(),
        settings: game,
    })
    .unwrap();
    rewritten(&archive, |game| game["palette"] = json!("no-such-palette"));

    let (opened, lines, printed) = run(
        "project-open",
        Some(&json!({ "archivePath": archive, "extractionDir": root.path().join("opened") })),
    );
    assert!(!opened, "{printed}");
    let refused = lines.last().unwrap();
    assert_eq!(refused["type"], "error", "{printed}");
    assert!(
        refused["message"].as_str().unwrap().contains("no-such-palette"),
        "{printed}"
    );
    assert!(!root.path().join("opened").exists(), "the project was extracted");
}

/// For the same selection, we show the same warnings in the builder for an
/// author's filter that a Windows game may not load as in `shaders-check`.
#[test]
fn shaders_check_prints_the_builders_filter_warnings() {
    let root = Scratch::dir("rominabox-cli-shader-warnings");
    let deep = format!("{}/pass.glsl", ["a-folder-of-twenty-c"; 8].join("/"));
    fs::create_dir_all(root.join(&deep).parent().unwrap()).unwrap();
    fs::write(root.join(&deep), "#if defined(VERTEX)\n#elif defined(FRAGMENT)\n#endif\n").unwrap();
    fs::write(root.join("pal.glslp"), format!("shaders = 1\nshader0 = {deep}\n")).unwrap();
    let request = json!({ "custom": [{ "name": "PAL", "path": root.join("pal.glslp") }] });
    let selection: rominabox_desktop::shaders::ShaderSelection = serde_json::from_value(request.clone()).unwrap();
    let builder = rominabox_desktop::shaders::windows_warnings(&selection).unwrap();
    assert_eq!(builder.len(), 1, "the filter is deep enough to be warned about");
    assert_eq!(
        result("shaders-check", Some(&request))["warnings"],
        serde_json::to_value(&builder).unwrap()
    );
    let _ = fs::remove_dir_all(&root);
}

/// When the request does not name an author's shader, we name it after its
/// file, as we do for a file added in the Menu step of the builder.
#[test]
fn a_custom_shader_without_a_name_is_named_after_its_file() {
    let root = Scratch::dir("rominabox-cli-shader-name");
    let pass = "#if defined(VERTEX)\n#elif defined(FRAGMENT)\n#endif\n";
    fs::write(root.join("CRT Royale.glsl"), pass).unwrap();
    fs::write(root.join("pal-r57shell.GLSL"), pass).unwrap();
    let request = json!({ "custom": [
        { "path": root.join("CRT Royale.glsl") },
        { "path": root.join("pal-r57shell.GLSL") },
    ] });
    let listed = result("shaders-check", Some(&request))["shaders"].clone();
    let names: Vec<&str> = listed
        .as_array()
        .expect("the resolved shaders")
        .iter()
        .map(|shader| shader["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"CRT Royale"), "{listed}");
    assert!(names.contains(&"pal-r57shell"), "{listed}");
}
