use super::*;
use crate::controls::Controls;
use crate::packaging::ExportTarget;

fn fixture(name: &str) -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir(&format!("rominabox-project-{name}"))
}

#[test]
fn project_round_trip_preserves_cue_and_tracks() {
    let root = fixture("cue-round-trip");
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("track.bin"), b"track bytes").unwrap();
    fs::write(
        source.join("disc.cue"),
        "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n",
    )
    .unwrap();
    let archive_path = root.join("game.rominabox");
    save_project(&ProjectSaveRequest {
        archive_path: archive_path.clone(),
        settings: Game {
            rom: source.join("disc.cue"),
            title: "Disc game".to_string(),
            system: "segacd".to_string(),
            icon: None,
            background: None,
            show_menu: false,
            start_at_menu: false,
            theme: "native".to_string(),
            palette: "blue".to_string(),
            menu_sounds: "off".to_string(),
            controls: Controls::default(),
            menu_controls: crate::builder::unstated::menu_controls(),
            firmware: Vec::new(),
            splash: false,
            advanced_emulator_access: false,
            keep_playing_in_background: false,
            autosave_on_quit: false,
            menu_entries: None,
            shaders: crate::shaders::ShaderSelection::default(),
            include_achievements: false,
            target: ExportTarget::Macos,
            both_platforms: false,
            intel_macs: false,
        },
    })
    .unwrap();

    let opened = open_project(&ProjectOpenRequest {
        archive_path,
        extraction_dir: root.join("opened"),
    })
    .unwrap();
    assert_eq!(
        fs::read(&opened.settings.rom).unwrap(),
        fs::read(source.join("disc.cue")).unwrap()
    );
    assert_eq!(
        fs::read(opened.settings.rom.parent().unwrap().join("track.bin")).unwrap(),
        b"track bytes"
    );
    assert!(!opened.settings.advanced_emulator_access);
}

#[test]
fn project_round_trip_preserves_background_play_and_quit_autosave() {
    let root = fixture("play-settings");
    let rom = root.join("game.bin");
    fs::write(&rom, b"rom bytes").unwrap();
    let mut settings = settings(rom, false);
    settings.keep_playing_in_background = true;
    settings.autosave_on_quit = true;
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
    assert!(opened.settings.keep_playing_in_background);
    assert!(opened.settings.autosave_on_quit);
}

fn settings(rom: PathBuf, advanced_emulator_access: bool) -> Game {
    Game {
        rom,
        title: "Access game".to_string(),
        system: "megadrive".to_string(),
        icon: None,
        background: None,
        show_menu: false,
        start_at_menu: false,
        theme: "native".to_string(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls: Controls::default(),
        menu_controls: crate::builder::unstated::menu_controls(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: crate::shaders::ShaderSelection::default(),
        include_achievements: false,
        target: ExportTarget::Macos,
        both_platforms: false,
        intel_macs: false,
    }
}

#[test]
fn project_round_trip_preserves_advanced_emulator_access() {
    let root = fixture("advanced-access-true");
    let rom = root.join("game.bin");
    fs::write(&rom, b"rom bytes").unwrap();
    let archive_path = root.join("game.rominabox");
    save_project(&ProjectSaveRequest {
        archive_path: archive_path.clone(),
        settings: settings(rom, true),
    })
    .unwrap();

    let opened = open_project(&ProjectOpenRequest {
        archive_path,
        extraction_dir: root.join("opened"),
    })
    .unwrap();
    assert!(opened.settings.advanced_emulator_access);
    assert!(
        opened
            .settings
            .into_export_request(root.join("out"), root.join("kit"), None)
            .game.advanced_emulator_access
    );
}

#[test]
fn current_project_defaults_omitted_capability_on() {
    let root = fixture("current-defaults");
    let archive_path = root.join("game.rominabox");
    {
        let file = File::create(&archive_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let manifest = br#"{
  "formatVersion": 3,
  "game": {
    "rom": "content/game.bin",
    "title": "Current",
    "system": "megadrive",
    "showMenu": false,
    "startAtMenu": false,
    "theme": "native",
    "palette": "blue",
    "target": "macos"
  },
  "assets": { "content": ["content/game.bin"] }
}"#;
        write_bytes(&mut writer, MANIFEST_PATH, manifest, options).unwrap();
        write_bytes(&mut writer, "content/game.bin", b"rom", options).unwrap();
        writer.finish().unwrap();
    }

    let opened = open_project(&ProjectOpenRequest {
        archive_path,
        extraction_dir: root.join("opened"),
    })
    .unwrap();
    assert!(!opened.settings.advanced_emulator_access);
    assert!(!opened.settings.keep_playing_in_background);
    assert!(!opened.settings.autosave_on_quit);
    assert!(opened.settings.include_achievements);
}
