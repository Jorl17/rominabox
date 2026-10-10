//! Characterisation snapshot of the menu that we stage in an export.
//!
//! We compose every design in the registry, with four menu configurations and
//! two consoles, with `menu::compose_menu`, and compare its menu files byte
//! for byte with `tests/fixtures/menu-snapshots/<case>/`. After a change to
//! the composition the files are identical, or a new recording shows what
//! changed. On macOS we also export every case and must stage exactly what
//! `compose_menu` composed.
//!
//! Record again with `ROMINABOX_RECORD_SNAPSHOT=1`, and say in the commit
//! what changed and why.

mod export_fixture;
mod support;

use rominabox_engine::packaging::{ExportRequest, ExportTarget};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
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

    /// The components that we list on the game's ABOUT screen: some for the
    /// menu with everything, none otherwise.
    fn licences(self) -> Vec<rominabox_engine::licences::Row> {
        match self {
            Menu::Everything => ["Genesis Plus GX", "RetroArch"]
                .into_iter()
                .map(|title| rominabox_engine::licences::Row {
                    group: rominabox_engine::licences::Group::Native,
                    title: title.into(),
                    version: String::new(),
                    licence: "GPL-3.0-or-later".into(),
                    file: String::new(),
                    copyright: String::new(),
                })
                .collect(),
            Menu::Default | Menu::NoOptions | Menu::SplashOnly => Vec::new(),
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
    let designs = rominabox_engine::themes::registry().unwrap().designs;
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

/// The Mac kit we make export tests from (`export_fixture::fixture_kit`),
/// with the files we read to compose the menu, the shader library, a
/// stand-in core for each console and a player with achievements.
#[cfg(target_os = "macos")]
fn export_kit(root: &Path) -> PathBuf {
    let kit = export_fixture::fixture_kit(root);
    support::with_menu_assets(&kit);
    support::with_shader_library(&kit);
    for system in SYSTEMS {
        let core = rominabox_engine::systems::find(system)
            .and_then(|system| system.preferred_core())
            .unwrap();
        fs::write(kit.join("cores").join(core.artifact().unwrap()), b"core").unwrap();
        fs::write(kit.join("licenses").join(&core.license_file), b"licence").unwrap();
    }
    fs::write(
        kit.join("manifest.json"),
        r#"{"schema_version":1,"components":[{"name":"RetroArch","capabilities":{"achievements":true}},{"name":"RmlUi"}]}"#,
    )
    .unwrap();
    kit
}

/// Generated content: a few bytes named like a cartridge, or a one-track cue
/// sheet and its image.
#[cfg(target_os = "macos")]
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

/// The export a case describes, without content, which we read only on export.
fn request(root: &Path, kit: &Path, case: &Case) -> ExportRequest {
    let design = rominabox_engine::themes::staged_design(kit, &case.design);
    let mut request = ExportRequest {
        game: rominabox_engine::game::Game {
            theme: case.design.clone(),
            splash: false,
            include_achievements: false,
            ..rominabox_engine::game::Game::new(PathBuf::new(), "Menu Snapshot", case.system, ExportTarget::Macos)
        },
        zip: None,
        output_dir: root.join(format!("out-{}", case.name())),
        replace: false,
        runtime_kit: kit.to_path_buf(),
        core: None,
        core_cache: None,
        accounts_folder: None,
    };
    match case.menu {
        Menu::Default => {}
        Menu::NoOptions => request.game.menu_entries = Some(Vec::new()),
        Menu::Everything => {
            let entries: Vec<String> = rominabox_engine::menu::declared_screens(&design)
                .unwrap()
                .into_iter()
                .filter(|screen| screen.option_label.is_some())
                .map(|screen| screen.id)
                .collect();
            request.game.include_achievements = entries
                .iter()
                .any(|entry| entry == rominabox_engine::achievements::SCREEN);
            request.game.menu_entries = Some(entries);
            let catalog = rominabox_engine::shaders::catalog().unwrap();
            request.game.shaders = rominabox_engine::shaders::ShaderSelection {
                bundled: catalog.iter().map(|entry| entry.id.clone()).collect(),
                custom: Vec::new(),
                initial: catalog.last().map(|entry| entry.id.clone()),
            };
        }
        Menu::SplashOnly => {
            request.game.show_menu = false;
            request.game.splash = true;
        }
    }
    request
}

/// The menu that an export of `request` puts in its `menu-assets`, which we
/// stage with the export step alone, without the rest of the export.
fn compose(request: &ExportRequest, menu: Menu, destination: &Path) -> BTreeMap<String, String> {
    // Every case's content is one cartridge or one disc sheet.
    rominabox_engine::packaging::stage_menu(request, 1, &menu.licences(), destination)
        .unwrap_or_else(|error| panic!("{}: staging the menu failed: {error}", request.game.theme));
    let staged = staged_menu(destination);
    assert!(
        staged.contains_key("menu.rml") && staged.contains_key("controls-defaults.cfg"),
        "{}: nothing was staged: {:?}",
        request.game.theme,
        staged.keys().collect::<Vec<_>>()
    );
    staged
}

/// The staged files of a menu. We copy the artwork, fonts and shader sources
/// instead of composing them.
fn is_snapshot_file(name: &str) -> bool {
    matches!(
        name,
        "menu.rml"
            | "menu.rcss"
            | "design.cfg"
            | "controls-defaults.cfg"
            | "shaders.cfg"
            | "hotkeys-defaults.cfg"
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
        files.insert(name, without_version(&text, version));
    }
    files
}

/// Return `text` with the product version replaced by a fixed token where it
/// stands alone, so that we still compare a longer number that contains it
/// (`10.1.0` for `0.1.0`).
fn without_version(text: &str, version: &str) -> String {
    let part_of_number = |c: char| c.is_ascii_digit() || c == '.';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(version) {
        let before = rest[..at].chars().last().or_else(|| out.chars().last());
        let after = rest[at + version.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(part_of_number) || after.is_some_and(part_of_number) {
            out.push_str(version);
        } else {
            out.push_str(VERSION_TOKEN);
        }
        rest = &rest[at + version.len()..];
    }
    out.push_str(rest);
    out
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

fn snapshot_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/menu-snapshots")
}

fn snapshot_directory(case: &str) -> PathBuf {
    snapshot_root().join(case)
}

/// Fixture directories that no case produces. When we remove a case, its old
/// snapshot must not look covered.
fn orphaned_snapshots(cases: &[Case]) -> Vec<String> {
    let names: Vec<String> = cases.iter().map(Case::name).collect();
    let mut orphans: Vec<String> = fs::read_dir(snapshot_root())
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| !names.contains(name))
                .collect()
        })
        .unwrap_or_default();
    orphans.sort();
    orphans
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
fn the_composed_menu_matches_its_snapshot() {
    let root = rominabox_scratch::Scratch::dir("rominabox-menu-snapshot");
    let kit = support::kit(&root);
    support::with_shader_library(&kit);
    let recording = std::env::var_os("ROMINABOX_RECORD_SNAPSHOT").is_some_and(|value| value == "1");
    let cases = cases();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.name();
        let staged = compose(&request(&root, &kit, case), case.menu, &root.join(&name));
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
    for orphan in orphaned_snapshots(&cases) {
        failures.push(format!(
            "{orphan}: a snapshot directory no case produces; remove it by hand if the case was meant to go"
        ));
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

/// The player shows the splash exactly when the game has the splash's logo,
/// so a game with the splash off must not have it. The card over a game that
/// waits in the background has its own copy, in every game with a menu.
#[test]
fn a_game_has_the_splash_logo_only_with_the_splash() {
    let root = rominabox_scratch::Scratch::dir("rominabox-menu-logos");
    let kit = support::kit(&root);
    support::with_shader_library(&kit);
    let mut wrong = Vec::new();
    for menu in Menu::ALL {
        let case = Case { design: "native".into(), menu, system: "megadrive" };
        let request = request(&root, &kit, &case);
        let staged = root.join(case.name());
        rominabox_engine::packaging::stage_menu(&request, 1, &menu.licences(), &staged).unwrap();
        for (file, wanted) in [("splash-logo.png", request.game.splash), ("logo.png", request.game.show_menu)] {
            if staged.join(file).is_file() != wanted {
                wrong.push(format!("{}: {file} {}", case.name(), if wanted { "missing" } else { "shipped" }));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("
"));
}

/// We trust the snapshot only as far as `compose` matches an export. So we
/// export every case and check that the menu files of the export are the
/// ones that `compose` wrote for the same request.
#[cfg(target_os = "macos")]
#[test]
fn an_export_stages_exactly_the_composed_menu() {
    let root = rominabox_scratch::Scratch::dir("rominabox-menu-export");
    let kit = export_kit(&root);
    let cases = cases();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.name();
        let mut request = request(&root, &kit, case);
        let original = request.clone();
        request.game.rom = content(&root, case.system);
        let result = rominabox_engine::packaging::export_game(
            &request,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap_or_else(|error| panic!("{name}: export failed: {error:?}"));
        // In the menu we list what is in the game's licence index.
        let index = result.app_path.join("Contents/Resources/Legal/Licenses/index.json");
        let licences: Vec<rominabox_engine::licences::Row> =
            serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
        let composed = rominabox_engine::packaging::stage_menu(
            &original,
            1,
            &licences,
            &root.join(format!("composed-{name}")),
        )
        .map(|_| staged_menu(&root.join(format!("composed-{name}"))))
        .unwrap_or_else(|error| panic!("{name}: staging the menu failed: {error}"));
        let exported = staged_menu(&result.app_path.join("Contents/Resources/menu-assets"));
        let label = format!("{name} (compose_menu as the snapshot, the export as staged)");
        if let Some(difference) = first_difference(&label, &composed, &exported) {
            failures.push(difference);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} exports stage a different menu from compose_menu:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    eprintln!("exported {} menus", cases.len());
}

#[test]
fn only_the_version_on_its_own_is_normalised() {
    assert_eq!(without_version("ROM-IN-A-BOX / 0.1.0", "0.1.0"), "ROM-IN-A-BOX / @VERSION@");
    assert_eq!(without_version("10.1.0 and 0.1.01", "0.1.0"), "10.1.0 and 0.1.01");
    assert_eq!(without_version("0.1.0<b>0.1.0</b>", "0.1.0"), "@VERSION@<b>@VERSION@</b>");
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
