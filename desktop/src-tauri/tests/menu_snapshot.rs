//! Characterisation snapshot of the menu that we stage in an export.
//!
//! We export every design in the registry, with four menu configurations and
//! two consoles, and compare the staged menu files byte for byte with
//! `tests/fixtures/menu-snapshots/<case>/`, so any change to the composed
//! menu appears as a difference.
//!
//! Re-record with `ROMINABOX_RECORD_SNAPSHOT=1`.
#![cfg(target_os = "macos")]

use rominabox_desktop::packaging::{ExportRequest, ExportTarget};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

/// We put the product version into the footer. It changes with every release
/// and is of no use for the composition.
const VERSION_TOKEN: &str = "@VERSION@";

#[derive(Clone, Copy)]
enum Menu {
    /// The design's own default Options entries.
    Default,
    /// An empty entry list: no Options button.
    NoOptions,
    /// Every entry the design offers, with shaders and achievements.
    Everything,
    /// No menu, only the splash.
    SplashOnly,
}

impl Menu {
    const ALL: [Menu; 4] = [
        Menu::Default,
        Menu::NoOptions,
        Menu::Everything,
        Menu::SplashOnly,
    ];

    fn name(self) -> &'static str {
        match self {
            Menu::Default => "default",
            Menu::NoOptions => "no-options",
            Menu::Everything => "everything",
            Menu::SplashOnly => "splash-only",
        }
    }
}

/// There are three- and six-button pads for Mega Drive, and the digital pad
/// and the analog pad with two sticks for PlayStation.
const SYSTEMS: [&str; 2] = ["megadrive", "ps1"];

struct Case {
    design: String,
    menu: Menu,
    system: &'static str,
}

impl Case {
    fn name(&self) -> String {
        format!("{}-{}-{}", self.design, self.menu.name(), self.system)
    }
}

fn cases() -> Vec<Case> {
    let designs = rominabox_desktop::themes::registry().unwrap().designs;
    assert!(!designs.is_empty(), "the design registry lists no designs");
    let mut cases = Vec::new();
    for design in designs {
        for menu in Menu::ALL {
            for system in SYSTEMS {
                cases.push(Case {
                    design: design.id.clone(),
                    menu,
                    system,
                });
            }
        }
    }
    cases
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

/// A kit with the actual designs, controller artwork and splash logo, and
/// stand-ins for everything that is not the menu: a runtime that only
/// returns, and cores of a few bytes.
fn fixture_kit(root: &Path) -> PathBuf {
    let kit = root.join("runtime-kit");
    for directory in [
        "bin",
        "cores",
        "Frameworks",
        "licenses/native",
        "provenance/native-rmlui",
        "branding",
    ] {
        fs::create_dir_all(kit.join(directory)).unwrap();
    }
    let source = root.join("retroarch.c");
    fs::write(
        &source,
        "int rarch_main(int c, char **v, void *d){(void)c;(void)v;(void)d;return 0;}\nint main(void){return rarch_main(0,0,0);}\n",
    )
    .unwrap();
    let status = Command::new("cc")
        .args(["-Oz", "-Wl,-headerpad_max_install_names", "-o"])
        .arg(kit.join("bin/retroarch"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "could not compile the runtime stub");
    for name in ["RetroArch.txt", "NATIVE-DEPENDENCIES.txt", "RmlUi-MIT.txt"] {
        fs::write(kit.join("licenses").join(name), name).unwrap();
    }
    for system in SYSTEMS {
        let core = rominabox_desktop::systems::find(system)
            .and_then(|system| system.preferred_core())
            .unwrap();
        fs::write(kit.join("cores").join(core.artifact().unwrap()), b"core").unwrap();
        fs::write(kit.join("licenses").join(&core.license_file), b"licence").unwrap();
    }
    fs::write(
        kit.join("runtime-dependencies.json"),
        r#"{"formatVersion":1,"files":[]}"#,
    )
    .unwrap();
    fs::write(
        kit.join("manifest.json"),
        r#"{"schema_version":1,"components":[{"name":"RetroArch","capabilities":{"achievements":true}},{"name":"RmlUi"}]}"#,
    )
    .unwrap();
    copy_tree(
        &rominabox_desktop::repo::at("integrations/designs"),
        &kit.join("designs"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("desktop/assets/controllers"),
        &kit.join("menu-assets"),
    );
    fs::copy(
        rominabox_desktop::repo::at("desktop/assets/branding/logo.png"),
        kit.join("branding/logo.png"),
    )
    .unwrap();
    kit
}

/// Generated content: a few bytes named like a cartridge, or a one-track cue
/// sheet and its image.
fn content(root: &Path, system: &str) -> PathBuf {
    let directory = root.join(format!("content-{system}"));
    fs::create_dir_all(&directory).unwrap();
    match system {
        "ps1" => {
            fs::write(directory.join("game.bin"), vec![0u8; 2352]).unwrap();
            fs::write(
                directory.join("game.cue"),
                "FILE \"game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
            )
            .unwrap();
            directory.join("game.cue")
        }
        _ => {
            fs::write(directory.join("game.bin"), b"RIBtest").unwrap();
            directory.join("game.bin")
        }
    }
}

fn request(root: &Path, kit: &Path, case: &Case) -> ExportRequest {
    let design = rominabox_desktop::themes::staged_design(kit, &case.design);
    let mut request = ExportRequest {
        rom: content(root, case.system),
        title: "Menu Snapshot".to_string(),
        system: case.system.to_string(),
        description: None,
        icon: None,
        background: None,
        show_menu: true,
        start_at_menu: false,
        theme: case.design.clone(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls: rominabox_desktop::controls::Controls::default(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: false,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        include_achievements: false,
        output_dir: root.join(format!("out-{}", case.name())),
        target: ExportTarget::Macos,
        runtime_kit: kit.to_path_buf(),
        core: None,
        core_cache: None,
    };
    match case.menu {
        Menu::Default => {}
        Menu::NoOptions => request.menu_entries = Some(Vec::new()),
        Menu::Everything => {
            let entries: Vec<String> = rominabox_desktop::themes::declared_screens(&design)
                .unwrap()
                .into_iter()
                .filter(|screen| screen.option_label.is_some())
                .map(|screen| screen.id)
                .collect();
            request.include_achievements = entries
                .iter()
                .any(|entry| entry == rominabox_desktop::achievements::SCREEN);
            request.menu_entries = Some(entries);
            let catalog = rominabox_desktop::shaders::catalog().unwrap();
            request.shaders = rominabox_desktop::shaders::ShaderSelection {
                bundled: catalog.iter().map(|entry| entry.id.clone()).collect(),
                custom: Vec::new(),
                initial: catalog.last().map(|entry| entry.id.clone()),
            };
        }
        Menu::SplashOnly => {
            request.show_menu = false;
            request.splash = true;
        }
    }
    request
}

/// The staged files of a menu. We copy the artwork, fonts and shader sources
/// instead of composing them.
fn is_snapshot_file(name: &str) -> bool {
    matches!(
        name,
        "menu.rml" | "menu.rcss" | "design.cfg" | "controls-defaults.cfg" | "shaders.cfg"
    ) || (name.starts_with("scene-") && name.ends_with(".rml"))
}

fn staged_menu(menu_assets: &Path) -> BTreeMap<String, String> {
    let version = env!("CARGO_PKG_VERSION");
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(menu_assets).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_snapshot_file(&name) {
            continue;
        }
        let text = fs::read_to_string(entry.path())
            .unwrap_or_else(|error| panic!("{name} is not text: {error}"));
        files.insert(name, text.replace(version, VERSION_TOKEN));
    }
    files
}

/// The first difference between two sets of files, named by case, file and
/// line, or `None` when they are identical.
fn first_difference(
    case: &str,
    expected: &BTreeMap<String, String>,
    actual: &BTreeMap<String, String>,
) -> Option<String> {
    for name in expected.keys() {
        if !actual.contains_key(name) {
            return Some(format!(
                "{case}: {name} is in the snapshot but was not staged"
            ));
        }
    }
    for (name, text) in actual {
        let Some(recorded) = expected.get(name) else {
            return Some(format!(
                "{case}: {name} was staged but is not in the snapshot"
            ));
        };
        if recorded == text {
            continue;
        }
        let mut recorded_lines = recorded.split('\n');
        let mut staged_lines = text.split('\n');
        let mut line = 1;
        loop {
            match (recorded_lines.next(), staged_lines.next()) {
                (Some(want), Some(got)) if want == got => line += 1,
                (want, got) => {
                    return Some(format!(
                        "{case}: {name} differs at line {line}\n  snapshot: {}\n  staged:   {}",
                        want.map_or("<end of file>".to_string(), |text| format!("{text:?}")),
                        got.map_or("<end of file>".to_string(), |text| format!("{text:?}")),
                    ));
                }
            }
        }
    }
    None
}

fn snapshot_directory(case: &str) -> PathBuf {
    rominabox_desktop::repo::at("desktop/src-tauri/tests/fixtures/menu-snapshots").join(case)
}

fn recorded(directory: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return files;
    };
    for entry in entries {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_snapshot_file(&name) {
            files.insert(name, fs::read_to_string(entry.path()).unwrap());
        }
    }
    files
}

fn record(directory: &Path, staged: &BTreeMap<String, String>) {
    fs::create_dir_all(directory).unwrap();
    for name in recorded(directory).keys() {
        if !staged.contains_key(name) {
            fs::remove_file(directory.join(name)).unwrap();
        }
    }
    for (name, text) in staged {
        fs::write(directory.join(name), text).unwrap();
    }
}

#[test]
fn the_staged_menu_matches_its_snapshot() {
    let root = rominabox_scratch::Scratch::dir("rominabox-menu-snapshot");
    let kit = fixture_kit(&root);
    let recording = std::env::var_os("ROMINABOX_RECORD_SNAPSHOT").is_some_and(|value| value == "1");
    let cases = cases();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.name();
        let request = request(&root, &kit, case);
        let result =
            rominabox_desktop::packaging::export_game(&request, &AtomicBool::new(false), |_| {})
                .unwrap_or_else(|error| panic!("{name}: export failed: {error:?}"));
        let staged = staged_menu(&result.app_path.join("Contents/Resources/menu-assets"));
        assert!(
            staged.contains_key("menu.rml") && staged.contains_key("controls-defaults.cfg"),
            "{name}: the export staged no menu: {:?}",
            staged.keys().collect::<Vec<_>>()
        );
        for (file, text) in &staged {
            assert!(
                !text.contains(root.path().to_string_lossy().as_ref()),
                "{name}: {file} contains a path from this machine"
            );
        }
        let directory = snapshot_directory(&name);
        if recording {
            record(&directory, &staged);
            continue;
        }
        let expected = recorded(&directory);
        if expected.is_empty() {
            failures.push(format!(
                "{name}: no snapshot at {}; record one with ROMINABOX_RECORD_SNAPSHOT=1",
                directory.display()
            ));
        } else if let Some(difference) = first_difference(&name, &expected, &staged) {
            failures.push(difference);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} menu snapshots differ (re-record with ROMINABOX_RECORD_SNAPSHOT=1 only for an intended change):\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    eprintln!("compared {} menu snapshots", cases.len());
}

#[test]
fn a_difference_names_the_case_the_file_and_the_first_differing_line() {
    let files = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect()
    };
    let snapshot = files(&[
        ("design.cfg", "a = 1\n"),
        ("menu.rml", "<rml>\n<body/>\n</rml>\n"),
    ]);

    assert_eq!(first_difference("case", &snapshot, &snapshot.clone()), None);

    let changed = files(&[
        ("design.cfg", "a = 1\n"),
        ("menu.rml", "<rml>\n<body id=\"x\"/>\n</rml>\n"),
    ]);
    assert_eq!(
        first_difference("native-default-megadrive", &snapshot, &changed).as_deref(),
        Some(
            "native-default-megadrive: menu.rml differs at line 2\n  snapshot: \"<body/>\"\n  staged:   \"<body id=\\\"x\\\"/>\""
        )
    );

    let longer = files(&[
        ("design.cfg", "a = 1\nb = 2\n"),
        ("menu.rml", "<rml>\n<body/>\n</rml>\n"),
    ]);
    assert_eq!(
        first_difference("case", &snapshot, &longer).as_deref(),
        Some("case: design.cfg differs at line 2\n  snapshot: \"\"\n  staged:   \"b = 2\"")
    );

    let missing = files(&[("menu.rml", "<rml>\n<body/>\n</rml>\n")]);
    assert_eq!(
        first_difference("case", &snapshot, &missing).as_deref(),
        Some("case: design.cfg is in the snapshot but was not staged")
    );

    let extra = files(&[
        ("design.cfg", "a = 1\n"),
        ("menu.rml", "<rml>\n<body/>\n</rml>\n"),
        ("shaders.cfg", "shader_ids = \"none\"\n"),
    ]);
    assert_eq!(
        first_difference("case", &snapshot, &extra).as_deref(),
        Some("case: shaders.cfg was staged but is not in the snapshot")
    );
}
