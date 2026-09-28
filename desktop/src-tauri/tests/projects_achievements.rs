use rominabox_desktop::achievements;
use rominabox_desktop::controls::Controls;
use rominabox_desktop::packaging::ExportTarget;
use rominabox_desktop::game::Game;
use rominabox_desktop::projects::{
    open_project, save_project, ProjectOpenRequest, ProjectSaveRequest,
};
use rominabox_desktop::shaders::ShaderSelection;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn settings(rom: PathBuf) -> Game {
    Game {
        rom,
        title: "Achievement project".into(),
        system: "megadrive".into(),
        description: None,
        icon: None,
        background: None,
        show_menu: true,
        start_at_menu: false,
        theme: "native".into(),
        palette: "blue".into(),
        menu_sounds: "off".into(),
        controls: Controls::default(),
        menu_controls: rominabox_desktop::builder::unstated::menu_controls(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: false,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: ShaderSelection::default(),
        include_achievements: true,
        target: ExportTarget::Macos,
        both_platforms: false,
        intel_macs: false,
    }
}

fn current_archive(path: &std::path::Path, settings: serde_json::Value) {
    let file = File::create(path).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file("manifest.json", options).unwrap();
    writer
        .write_all(
            serde_json::to_string(&serde_json::json!({
                "formatVersion": 3,
                "game": settings,
                "assets": { "content": ["content/game.bin"] }
            }))
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
    writer.start_file("content/game.bin", options).unwrap();
    writer.write_all(b"rom bytes").unwrap();
    writer.finish().unwrap();
}

fn current_settings() -> serde_json::Value {
    serde_json::json!({
        "rom": "content/game.bin",
        "title": "Achievement project",
        "system": "megadrive",
        "showMenu": true,
        "startAtMenu": false,
        "theme": "native",
        "palette": "blue",
        "target": "macos"
    })
}

#[test]
fn save_rejects_both_explicit_entry_conflicts() {
    for (included, entries) in [(false, vec!["achievements"]), (true, Vec::new())] {
        let root = rominabox_scratch::Scratch::dir("rominabox-project-entry-conflict");
        let rom = root.join("game.bin");
        fs::write(&rom, b"rom bytes").unwrap();
        let mut settings = settings(rom);
        settings.include_achievements = included;
        settings.menu_entries = Some(entries.into_iter().map(str::to_string).collect());
        let result = save_project(&ProjectSaveRequest {
            archive_path: root.join("game.rominabox"),
            settings,
        });
        assert!(result.unwrap_err().contains("menuEntries"));
    }
}

#[test]
fn open_rejects_a_current_project_with_conflicting_entries() {
    let root = rominabox_scratch::Scratch::dir("rominabox-project-open-conflict");
    let archive_path = root.join("game.rominabox");
    let mut stored = current_settings();
    stored["includeAchievements"] = false.into();
    stored["menuEntries"] = serde_json::json!(["achievements"]);
    current_archive(&archive_path, stored);
    let extraction_dir = root.join("opened");
    let result = open_project(&ProjectOpenRequest {
        archive_path,
        extraction_dir: extraction_dir.clone(),
    });
    assert!(result.unwrap_err().contains("menuEntries"));
    assert!(!extraction_dir.exists());
}

#[test]
fn current_project_round_trips_included_excluded_and_no_menu() {
    for (show_menu, included) in [(true, true), (true, false), (false, true)] {
        let root = rominabox_scratch::Scratch::dir("rominabox-project-capability");
        let rom = root.join("game.bin");
        fs::write(&rom, b"rom bytes").unwrap();
        let mut settings = settings(rom);
        settings.show_menu = show_menu;
        settings.include_achievements = included;
        let archive_path = root.join("game.rominabox");
        save_project(&ProjectSaveRequest {
            archive_path: archive_path.clone(),
            settings,
        })
        .unwrap();
        let opened = open_project(&ProjectOpenRequest {
            archive_path,
            extraction_dir: root.join("opened"),
        })
        .unwrap();
        assert_eq!(opened.settings.include_achievements, included);
        assert_eq!(opened.settings.show_menu, show_menu);
        assert_eq!(
            achievements::included(
                opened.settings.include_achievements,
                opened.settings.show_menu
            ),
            included && show_menu
        );
        if !show_menu {
            let design = rominabox_desktop::themes::design_root("native").unwrap();
            assert!(
                achievements::entries(&design, included, show_menu, Some(&[]))
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn current_project_defaults_omitted_capability_on() {
    let root = rominabox_scratch::Scratch::dir("rominabox-project-current-default");
    let archive_path = root.join("game.rominabox");
    current_archive(&archive_path, current_settings());
    let opened = open_project(&ProjectOpenRequest {
        archive_path,
        extraction_dir: root.join("opened"),
    })
    .unwrap();
    assert!(opened.settings.include_achievements);
}
