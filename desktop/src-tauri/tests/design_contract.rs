//! The elements a menu design must contain for the player to work.
//!
//! In `rmlui_bridge.cpp` we look up elements in the document by id, and we
//! check every lookup for a missing element, for example
//!
//!     if (Rml::Element *hit = document->GetElementById("control-hit-" + suffix))
//!
//! So a design without an id does not fail. Part of it works without a
//! warning, and the fault appears somewhere unrelated, such as a button that
//! does nothing or a panel that never appears.
//!
//! We declare the contract here once and check it in both directions. Every
//! id that we look up in the bridge must be classified here, and every
//! classified id must be in the design for that mode.
//!
//! These tests do not check that the elements behave, are visible, are
//! styled or are connected to anything, only that they exist.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// A group of ids on one screen, and whether a design must have that screen.
///
/// We group the ids by panel and not in one flat list, because the author
/// chooses the sections of a game. A design without an optional panel must
/// stay valid, and to add a panel we add an entry here.
struct Panel {
    name: &'static str,
    required: bool,
    ids: &'static [&'static str],
}

/// The ids that the full menu document must contain.
///
/// In splash mode we stage `splash.rml` over `menu.rml`, so we always load one
/// file name in the bridge, and these are the ids we require when that file is
/// the menu.
const MENU_PANELS: &[Panel] = &[
    Panel {
        name: "shell",
        required: true,
        ids: &["body", "screen", "heading", "status", "footer-hint"],
    },
    Panel {
        name: "pause",
        required: true,
        // The player can click every one of these, and we read the table that
        // connects them in the scanner.
        ids: &[
            "pause-panel", "load", "resume", "save", "controls", "quit",
            "slot-1", "slot-2", "slot-3", "slot-4", "slot-5", "slot-6",
        ],
    },
    Panel {
        // Optional, because we generate it only when there is more than one
        // controller for a console. A design without it stays valid, and this
        // panel tests that case.
        name: "controller-picker",
        required: false,
        ids: &["controls-device", "controls-device-current", "controls-device-list"],
    },
    Panel {
        name: "controls",
        required: true,
        ids: &[
            "controls-panel", "controls-back", "controls-reset",
            "controls-cancel", "controls-status",
            // The block in which we draw the controller. We list it here because
            // we replace its content in the player when someone picks another
            // pad, and without it that change would fail without a warning.
            "controller-scene",
        ],
    },
];

fn menu_ids(required_only: bool) -> Vec<&'static str> {
    MENU_PANELS
        .iter()
        .filter(|panel| panel.required || !required_only)
        .flat_map(|panel| panel.ids.iter().copied())
        .collect()
}

/// The ids that the splash document must contain, a much smaller set.
const SPLASH_IDS: &[&str] = &["body", "screen", "splash", "splash-logo"];

/// Ids that we make in the bridge by appending a control id or a slot number.
/// They cannot be literally in the design, because we generate the controls
/// into `<!--CONTROLS-->` from the console package, and the slots are numbered.
const GENERATED_PREFIXES: &[&str] = &[
    "control-",
    "control-binding-",
    "control-hit-",
    "control-label-",
    "slot-",
    "slot-image-",
    "slot-label-",
    "slot-state-",
    "controls-device-option-",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read(relative: &str) -> Option<String> {
    std::fs::read_to_string(repo_root().join(relative)).ok()
}

fn ids_in(document: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = document;
    while let Some(at) = rest.find("id=\"") {
        rest = &rest[at + 4..];
        if let Some(end) = rest.find('"') {
            found.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    found
}

fn ids_the_bridge_looks_up(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find("GetElementById(\"") {
        rest = &rest[at + 16..];
        if let Some(end) = rest.find('"') {
            found.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    // In the scan above we miss ids looked up through a variable, and those are
    // the ids that a player clicks. We call `GetElementById(binding.id)` for
    // each entry in a table of {"resume", ACTION}, {"save", ACTION} and the rest.
    // If we scanned only literal call sites, a design without Continue, Save,
    // Controls and Quit would count as valid.
    found.extend(ids_in_binding_table(source));
    found
}

/// The `{"id", RIB_RMLUI_ACTION_...}` pairs in the table of the bridge.
fn ids_in_binding_table(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find("{\"") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find('"') else { break };
        let id = &rest[..end];
        let after = &rest[end..];
        // Only a pair whose second element is an action; other braced literals
        // in this file are not element ids.
        if after
            .split(',')
            .nth(1)
            .is_some_and(|value| value.trim_start().starts_with("RIB_RMLUI_ACTION_"))
        {
            found.insert(id.to_string());
        }
        rest = after;
    }
    found
}

/// We may look up only ids that this contract classifies.
///
/// Without this check the contract would go stale. A new `GetElementById` in
/// the bridge would add a requirement for every design, and nobody would know
/// about it until something stopped working without a warning.
#[test]
fn every_id_the_bridge_reaches_for_is_classified() {
    let Some(bridge) = read("vendor/retroarch/menu/drivers/rmlui_bridge.cpp") else {
        // This must fail. A contract check that checks nothing without a
        // warning when the submodule is missing is worse than no check at all.
        panic!(
            "vendor/retroarch is not checked out, so the contract cannot be \
             checked. Run `git submodule update --init` before trusting this."
        );
    };
    let classified: BTreeSet<&str> = menu_ids(false)
        .into_iter()
        .chain(SPLASH_IDS.iter().copied())
        .chain(GENERATED_PREFIXES.iter().copied())
        .collect();

    // We may also look up a generated id literally. For example, in the bridge
    // we measure `slot-image-1` to get the picture aspect. So we classify an id
    // that begins with a generated prefix by that prefix.
    let unclassified: Vec<String> = ids_the_bridge_looks_up(&bridge)
        .into_iter()
        .filter(|id| {
            !classified.contains(id.as_str())
                && !GENERATED_PREFIXES
                    .iter()
                    .any(|prefix| id.starts_with(prefix) && id.len() > prefix.len())
        })
        .collect();
    assert!(
        unclassified.is_empty(),
        "the bridge looks up ids this contract does not classify: {unclassified:?}. \
         Add each to MENU_IDS, SPLASH_IDS or GENERATED_PREFIXES, so every design \
         knows it has to provide them."
    );
}

#[test]
fn the_shipped_design_provides_every_menu_id() {
    let Some(document) = read("integrations/designs/native/menu.rml") else {
        panic!("the shipped design has no menu.rml");
    };
    let present = ids_in(&document);
    for panel in MENU_PANELS.iter().filter(|panel| panel.required) {
        let missing: Vec<&&str> = panel.ids.iter().filter(|id| !present.contains(**id)).collect();
        assert!(
            missing.is_empty(),
            "menu.rml omits {missing:?} from the required '{}' panel. The bridge \
             looks these up defensively, so whatever they belong to does nothing \
             and reports nothing.",
            panel.name
        );
    }
}

#[test]
fn the_shipped_design_provides_every_splash_id() {
    let Some(document) = read("integrations/designs/native/splash.rml") else {
        panic!("the shipped design has no splash.rml");
    };
    let present = ids_in(&document);
    let missing: Vec<&&str> = SPLASH_IDS
        .iter()
        .filter(|id| !present.contains(**id))
        .collect();
    assert!(missing.is_empty(), "splash.rml is missing {missing:?}");
}

/// The controls placeholder is part of the contract too.
///
/// It is not an id, so the checks above would miss it, and without it there
/// is no controller in the menu of a design.
#[test]
fn the_menu_document_has_somewhere_to_put_the_controls() {
    let Some(document) = read("integrations/designs/native/menu.rml") else {
        panic!("the shipped design has no menu.rml");
    };
    assert!(
        document.contains("<!--CONTROLS-->"),
        "menu.rml has no <!--CONTROLS--> placeholder; the generated controller \
         scene has nowhere to go"
    );
}

/// A design is a directory, and we reject an unknown one when we resolve it.
///
/// We choose the directory to stage by the design id, so a second design is
/// reachable.
#[test]
fn a_design_resolves_to_its_own_directory() {
    let native = rominabox_desktop::themes::design_root("native")
        .expect("the shipped design resolves");
    assert!(
        native.ends_with("integrations/designs/native"),
        "a design lives in its own package directory, got {native:?}"
    );
    // We can resolve a design only when its directory exists, so we fail here
    // for a declared design with no package, and not later at staging.
    assert!(native.is_dir(), "resolving must confirm the package exists: {native:?}");
    assert!(
        native.is_absolute(),
        "the builder does not run from the repository root, so a relative \
         answer is one nobody can act on: {native:?}"
    );
    let refusal = rominabox_desktop::themes::design_root("no-such-design")
        .expect_err("an undeclared design must be refused");
    assert!(
        refusal.contains("no-such-design"),
        "the refusal should name what was asked for: {refusal}"
    );
}

/// Every document and font the design declares is actually there.
#[test]
fn the_shipped_design_package_is_complete() {
    let root = repo_root().join("integrations/designs/native");
    let declared: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("design.json")).expect("design.json"))
            .expect("valid JSON");
    for key in ["menu", "splash", "style"] {
        let name = declared["documents"][key].as_str().expect("a declared document");
        assert!(root.join(name).exists(), "{key} document '{name}' is missing");
    }
    for font in declared["fonts"].as_array().expect("fonts") {
        for key in ["file", "license"] {
            let name = font[key].as_str().expect("a declared font file");
            // The licence goes with the font, because we redistribute the font
            // in every exported game.
            assert!(root.join(name).exists(), "font {key} '{name}' is missing");
        }
    }
}

/// At export we stage the requested design, from its directory in the kit.
///
/// This checks that a second design is reachable, and not only that we can
/// declare it.
#[test]
fn a_kit_keeps_each_design_under_its_own_name() {
    let kit = PathBuf::from("/some/kit");
    let native = rominabox_desktop::themes::staged_design(&kit, "native");
    assert_eq!(native, kit.join("designs").join("native"));

    let other = rominabox_desktop::themes::staged_design(&kit, "ps1-era");
    assert_ne!(
        native, other,
        "two designs must not resolve to the same staged directory"
    );
}

/// Controller artwork is shared and does not belong to a design.
///
/// Every design shows the same pads, so moving the artwork under a design would
/// duplicate ten PNGs per design, and to add a design we would have to stage
/// them all again.
#[test]
fn controller_artwork_is_not_a_designs_to_own() {
    let staged = repo_root().join("desktop/src-tauri/resources/runtime/menu-assets");
    if !staged.exists() {
        eprintln!("the runtime kit is not prepared here; nothing was verified");
        return;
    }
    let art: Vec<_> = std::fs::read_dir(&staged)
        .expect("kit menu-assets")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("controller-"))
        .collect();
    assert!(
        !art.is_empty(),
        "controller artwork should stay in the kit's shared menu-assets"
    );
    let design = repo_root().join("integrations/designs/native");
    let owned: Vec<_> = std::fs::read_dir(&design)
        .expect("design package")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("controller-"))
        .collect();
    assert!(
        owned.is_empty(),
        "a design package must not carry controller artwork: {owned:?}"
    );
}

/// The scene geometry comes from one place.
///
/// We declare the scene size, marker diameter and callout size once, and read
/// them in the design's stylesheet and in scripts/render_control_overlays.py,
/// the renderer we use for review before a build, so the frame is the same.
#[test]
fn the_scene_geometry_is_declared_once_and_read_by_both_consumers() {
    let design = repo_root().join("integrations/designs/native/design.json");
    let declared: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&design).expect("design.json"))
            .expect("valid JSON");
    let metrics = &declared["metrics"];
    for (group, key) in [
        ("scene", "width"),
        ("scene", "height"),
        ("marker", "diameter"),
        ("callout", "width"),
        ("callout", "height"),
        ("group", "width"),
        ("group", "height"),
    ] {
        assert!(
            metrics[group][key].as_i64().is_some(),
            "the design must declare metrics.{group}.{key}"
        );
    }

    // We must read them in the renderer and not repeat them there. A literal
    // scene size in the renderer could silently drift from the design.
    let renderer = std::fs::read_to_string(repo_root().join("scripts/render_control_overlays.py"))
        .expect("the overlay renderer");
    assert!(
        renderer.contains("design.json"),
        "the overlay renderer must read the design's declared metrics"
    );
    assert!(
        !renderer.contains("SCENE = (960, 380)"),
        "the overlay renderer has a hardcoded scene size again"
    );
}

/// A design styles the shared list, never one particular list.
///
/// The shader list, the achievement list and every other Options list have
/// the same layout and the `.list-row` rules. A rule for one list, such as
/// `.shader-row`, would require the same rule in every other design.
///
/// We remove comments first, so a comment with the names of the lists that
/// use the shared rules does not make the check fail.
///
/// This does not check that the shared rules look right, or that a design
/// contains them at all.
#[test]
fn no_design_styles_one_list_by_name() {
    // Screens that we add at composition. The design defines pause and
    // controls, and must not define these. To add a screen, add a word here.
    const COMPOSED: [&str; 3] = ["shader", "achievement", "options"];
    let designs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs");
    let mut read = 0;
    for entry in std::fs::read_dir(&designs).expect("designs directory") {
        let sheet = entry.expect("design entry").path().join("menu.rcss");
        if !sheet.is_file() {
            continue;
        }
        read += 1;
        let text = std::fs::read_to_string(&sheet).expect("stylesheet");
        for selector in selectors(&text) {
            for identifier in named(&selector) {
                for name in COMPOSED {
                    assert!(
                        !identifier.starts_with(name),
                        "{} styles the {name} list by name: {}\n\
                         Those rows are `.list-row`; a design that needs a \
                         different list changes the shared rules.",
                        sheet.display(),
                        selector.trim()
                    );
                }
            }
        }
    }
    assert!(read > 0, "no stylesheet was read, so this proved nothing");
}

/// Every selector in a stylesheet: the text before each rule body, with
/// comments removed first.
fn selectors(stylesheet: &str) -> Vec<String> {
    let mut plain = String::with_capacity(stylesheet.len());
    let mut rest = stylesheet;
    while let Some(open) = rest.find("/*") {
        plain.push_str(&rest[..open]);
        rest = match rest[open..].find("*/") {
            Some(close) => &rest[open + close + 2..],
            None => "",
        };
    }
    plain.push_str(rest);
    plain
        .split('}')
        .filter_map(|block| block.split('{').next())
        .map(|selector| selector.trim().to_string())
        .filter(|selector| !selector.is_empty())
        .collect()
}

/// Every class and id named in a selector, lowercased.
///
/// Matching the whole selector text would catch too much. The design defines
/// `.control-picker-option`, the controller picker, and it contains the word
/// used for the options screen. A name matches when the identifier starts
/// with it.
fn named(selector: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = selector;
    while let Some(at) = rest.find(['.', '#']) {
        let after = &rest[at + 1..];
        let end = after
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .unwrap_or(after.len());
        if end > 0 {
            found.push(after[..end].to_ascii_lowercase());
        }
        rest = &after[end..];
    }
    found
}
