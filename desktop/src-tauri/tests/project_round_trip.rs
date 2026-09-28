//! When we save a project with every setting changed from the builder's
//! default, it opens with every setting and file as we saved it.

use rominabox_desktop::game::Game;
use rominabox_desktop::projects::{
    open_project, save_project, ProjectOpenRequest, ProjectSaveRequest,
};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn picture(path: &Path, shade: u8) {
    image::RgbaImage::from_pixel(4, 3, image::Rgba([shade, 40, 200, 255]))
        .save(path)
        .unwrap();
}

/// Every setting, none of them the builder's default (desktop/defaults.json).
fn everything(root: &Path) -> Value {
    fs::write(root.join("Game.md"), b"a Mega Drive game").unwrap();
    picture(&root.join("cover.png"), 10);
    picture(&root.join("backdrop.png"), 90);
    fs::write(root.join("bios.bin"), b"firmware bytes").unwrap();
    fs::write(
        root.join("mine.glsl"),
        "#if defined(VERTEX)\nvoid main() {}\n#elif defined(FRAGMENT)\nvoid main() {}\n#endif\n",
    )
    .unwrap();
    json!({
        "rom": root.join("Game.md"),
        "title": "Every Setting",
        "system": "megadrive",
        "icon": root.join("cover.png"),
        "background": root.join("backdrop.png"),
        "showMenu": true,
        "startAtMenu": true,
        "theme": "disc",
        "palette": "amber",
        "menuSounds": "bell",
        "controls": { "bindings": { "a": { "label": "Jump", "key": "space" } } },
        "menuControls": {
            "menu": ["key:f1", "pad:home"],
            "confirm": ["key:enter"],
            "back": ["key:backspace", "pad:b"]
        },
        "firmware": [root.join("bios.bin")],
        "splash": false,
        "includeAchievements": false,
        "advancedEmulatorAccess": true,
        "keepPlayingInBackground": true,
        "autosaveOnQuit": true,
        "menuEntries": ["controls"],
        "shaders": {
            "bundled": ["scanlines", "phosphor"],
            "custom": [{ "name": "Mine", "path": root.join("mine.glsl") }],
            "initial": "phosphor"
        },
        "target": "windows",
        "bothPlatforms": true,
        "intelMacs": true
    })
}

/// `settings` with each file as its name and bytes, not its location, because
/// we open a project's files in a folder of its own. A picture is only its
/// bytes, because we name it for its role in the archive and show no name.
fn by_contents(mut settings: Value) -> Value {
    let read = |path: &Value| -> Value {
        let path = path.as_str().expect("a path");
        json!({ "name": Path::new(path).file_name().unwrap().to_string_lossy(), "bytes": fs::read(path).unwrap() })
    };
    settings["rom"] = read(&settings["rom"]);
    for key in ["icon", "background"] {
        settings[key] = json!(fs::read(settings[key].as_str().expect("a picture")).unwrap());
    }
    settings["firmware"] = settings["firmware"].as_array().unwrap().iter().map(read).collect();
    for shader in settings["shaders"]["custom"].as_array_mut().unwrap() {
        shader["path"] = read(&shader["path"]);
    }
    settings
}

#[test]
fn a_project_opens_with_every_setting_and_file_it_was_saved_with() {
    let root = rominabox_scratch::Scratch::dir("rominabox-project-everything");
    let settings: Game = serde_json::from_value(everything(&root)).unwrap();
    let saved = serde_json::to_value(&settings).unwrap();
    assert_eq!(saved["bothPlatforms"], true, "a project takes Mac and Windows");
    save_project(&ProjectSaveRequest {
        archive_path: root.join("Every Setting.rominabox"),
        settings,
    })
    .unwrap();
    let opened = open_project(&ProjectOpenRequest {
        archive_path: root.join("Every Setting.rominabox"),
        extraction_dir: root.join("opened"),
    })
    .unwrap();
    let opened = serde_json::to_value(&opened.settings).unwrap();
    assert_eq!(by_contents(opened), by_contents(saved));
}
