//! Built-in ids and component classes required in a composed player menu.
//!
//! In the native player we use the string constants generated from
//! `document_contract.inc`. In this test we read the same declaration, so a
//! design cannot omit a built-in element and leave the C++ lookup returning
//! null. We check presence, and not visibility, styling or event behaviour.

mod support;

use std::collections::BTreeSet;
use std::path::PathBuf;

fn menu_document(design: &std::path::Path) -> String {
    let staged = rominabox_scratch::Scratch::dir("rominabox-design-contract");
    support::stage_theme(design, &staged, "blue")
        .expect("the design composes and stages");
    std::fs::read_to_string(staged.join("menu.rml")).unwrap()
}

fn repo_root() -> PathBuf {
    rominabox_desktop::repo::root()
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

fn classes_in(document: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = document;
    while let Some(at) = rest.find("class=\"") {
        rest = &rest[at + 7..];
        if let Some(end) = rest.find('"') {
            found.extend(rest[..end].split_whitespace().map(str::to_string));
            rest = &rest[end..];
        }
    }
    found
}

struct ContractEntry {
    name: String,
    value: String,
    scope: String,
    presence: String,
}

fn contract_source() -> String {
    read("vendor/retroarch/menu/drivers/rmlui/document_contract.inc")
        .expect("vendor/retroarch is checked out with the document contract")
}

fn contract_entries(kind: &str) -> Vec<ContractEntry> {
    let prefix = format!("RIB_{kind}(");
    let entries: Vec<ContractEntry> = contract_source()
        .lines()
        .filter_map(|line| line.trim().strip_prefix(&prefix))
        .map(|tail| {
            let declaration = tail.strip_suffix(')').expect("closed contract declaration");
            let fields: Vec<&str> = declaration.split(',').map(str::trim).collect();
            assert_eq!(fields.len(), 4, "four contract fields: {declaration}");
            ContractEntry {
                name: fields[0].to_string(),
                value: fields[1]
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .expect("quoted contract value")
                    .to_string(),
                scope: fields[2].to_string(),
                presence: fields[3].to_string(),
            }
        })
        .collect();
    assert!(!entries.is_empty(), "no RIB_{kind} declarations were read");
    entries
}

fn slot_count() -> usize {
    let source = contract_source();
    let line = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("RIB_SLOT_COUNT("))
        .expect("slot count declaration");
    let count: usize = line
        .strip_suffix(')')
        .expect("closed slot count")
        .parse()
        .expect("numeric slot count");
    assert_eq!(count, 6, "the current player contract has six slots");
    count
}

#[test]
fn built_in_document_contract_is_well_formed() {
    for kind in ["ELEMENT", "CLASS"] {
        let entries = contract_entries(kind);
        let mut names = BTreeSet::new();
        for entry in entries {
            assert!(
                names.insert(entry.name.clone()),
                "duplicate {kind} name: {}",
                entry.name
            );
            assert!(
                !entry.value.is_empty(),
                "empty {kind} value: {}",
                entry.name
            );
            assert!(matches!(entry.presence.as_str(), "Required" | "Optional"));
            assert!(matches!(
                entry.scope.as_str(),
                "Shared" | "Menu" | "Splash" | "Slots" | "Generated" | "State"
            ));
        }
    }
    slot_count();
}

#[test]
fn the_shipped_designs_provide_every_required_menu_id_and_class() {
    let elements = contract_entries("ELEMENT");
    let classes = contract_entries("CLASS");
    let slots = slot_count();
    for design in designs() {
        let document = menu_document(&design);
        let ids = ids_in(&document);
        let styled = classes_in(&document);
        for entry in elements.iter().filter(|entry| entry.presence == "Required") {
            match entry.scope.as_str() {
                "Shared" | "Menu" => assert!(
                    ids.contains(&entry.value),
                    "{} omits required id '{}'",
                    design.display(),
                    entry.value
                ),
                "Slots" => {
                    for slot in 1..=slots {
                        let id = format!("{}{slot}", entry.value);
                        assert!(
                            ids.contains(&id),
                            "{} omits required slot id '{id}'",
                            design.display()
                        );
                    }
                }
                _ => {}
            }
        }
        for entry in classes.iter().filter(|entry| entry.presence == "Required") {
            if matches!(entry.scope.as_str(), "Shared" | "Menu" | "Slots") {
                assert!(
                    styled.contains(&entry.value),
                    "{} omits required class '{}'",
                    design.display(),
                    entry.value
                );
            }
        }
    }
}

#[test]
fn the_shipped_designs_provide_every_required_splash_id() {
    let elements = contract_entries("ELEMENT");
    for design in designs() {
        let document = std::fs::read_to_string(design.join("splash.rml"))
            .expect("the design has a splash document");
        let ids = ids_in(&document);
        for entry in elements.iter().filter(|entry| entry.presence == "Required") {
            if matches!(entry.scope.as_str(), "Shared" | "Splash") {
                assert!(
                    ids.contains(&entry.value),
                    "{} omits required splash id '{}'",
                    design.display(),
                    entry.value
                );
            }
        }
    }
}

/// An overlay is part of the design, so the design must contain its element.
///
/// In the player we show an overlay by the id it was declared under, and, as
/// everywhere here, without failing. With a declaration but no element, the
/// game waits in its first seconds to show nothing, and we report nothing.
#[test]
fn every_declared_overlay_has_an_element_in_the_design() {
    for design in designs() {
        let overlays =
            rominabox_desktop::menu::declared_overlays(&design).expect("a design's overlays");
        assert!(
            !overlays.is_empty(),
            "{} declares no overlays, so a player who chose it is never told \
             how to reach the pause menu",
            design.display()
        );
        let drawn =
            menu_document(&design) + &std::fs::read_to_string(design.join("splash.rml")).unwrap();
        for overlay in &overlays {
            assert!(
                drawn.contains(&format!("id=\"{}\"", overlay.id)),
                "{} declares the overlay '{}' and no document draws it",
                design.display(),
                overlay.id
            );
        }
    }
}

/// Every design package, so that a rule applies to the second as well as the
/// first. If we checked a design only once we release it, it would be too late.
fn designs() -> Vec<PathBuf> {
    let root = repo_root().join("integrations/designs");
    let found: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("designs directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("design.json").is_file())
        .collect();
    assert!(
        !found.is_empty(),
        "no design was read, so this proved nothing"
    );
    found
}

/// An overlay is not a screen, and must not open as one.
///
/// We open a screen by name, give it the heading and the footer, hide every
/// other panel, and the player reaches it from a button. None of that applies
/// to something drawn over a running game, and if a design declared one as
/// both, the player would get a button that blanks the menu.
#[test]
fn no_overlay_is_also_a_screen() {
    for design in designs() {
        let screens: Vec<String> = rominabox_desktop::menu::declared_screens(&design)
            .expect("a design's screens")
            .into_iter()
            .map(|screen| screen.id)
            .collect();
        for overlay in
            rominabox_desktop::menu::declared_overlays(&design).expect("a design's overlays")
        {
            assert!(
                !screens.contains(&overlay.id),
                "{} declares '{}' as both an overlay and a screen",
                design.display(),
                overlay.id
            );
        }
    }
}

/// We declare in one place how long an overlay takes to leave.
///
/// In the player we hide the element when that time is up, and in the design
/// the element fades out over the same time. If we wrote the time twice, the
/// two could drift, and a notice would vanish mid-fade or stay a moment after
/// it is invisible.
#[test]
fn an_overlays_leaving_time_is_declared_once_and_read_by_both_consumers() {
    for design in designs() {
        let sheet = std::fs::read_to_string(design.join("menu.rcss")).expect("a stylesheet");
        for overlay in
            rominabox_desktop::menu::declared_overlays(&design).expect("a design's overlays")
        {
            let token = format!("design(overlay-leave-{})", overlay.id);
            assert!(
                sheet.contains(&token),
                "{} never reads {token}, so whatever it animates for '{}' is a \
                 second copy of a number the design already declared",
                design.join("menu.rcss").display(),
                overlay.id
            );
        }
    }
}

/// The controller scene is also part of the contract.
///
/// A design does not write its id, so the checks above would not notice that
/// it is missing, and without it the design shows no controller at all.
#[test]
fn the_menu_document_draws_the_controller_scene() {
    for design in designs() {
        let document = menu_document(&design);
        let scene = document
            .find("id=\"controller-scene\"")
            .unwrap_or_else(|| panic!("{} has no #controller-scene", design.display()));
        assert!(
            document[scene..].contains("id=\"control-up\""),
            "{}: the generated controller scene is not in #controller-scene",
            design.display()
        );
    }
}

/// A design is a directory, and we reject an unknown one when we resolve it.
///
/// We choose the directory to stage by the design id, so a second design is
/// reachable.
#[test]
fn a_design_resolves_to_its_own_directory() {
    let native =
        rominabox_desktop::themes::design_root("native").expect("the shipped design resolves");
    assert!(
        native.ends_with("integrations/designs/native"),
        "a design lives in its own package directory, got {native:?}"
    );
    // We can resolve a design only when its directory exists, so we fail here
    // for a declared design with no package, and not later at staging.
    assert!(
        native.is_dir(),
        "resolving must confirm the package exists: {native:?}"
    );
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
    let declared: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("design.json")).expect("design.json"),
    )
    .expect("valid JSON");
    for key in ["menu", "splash", "style"] {
        let name = declared["documents"][key]
            .as_str()
            .expect("a declared document");
        assert!(
            root.join(name).exists(),
            "{key} document '{name}' is missing"
        );
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
    // Every pad we can offer in the builder, and not only one. With a check for
    // "at least one drawing", we would miss the artwork of a newly added pad,
    // and the export for that console would then fail when we prepare its
    // controller artwork.
    let controls: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("desktop/controls.json")).expect("controls.json"),
    )
    .expect("valid JSON");
    let offered: Vec<String> = controls["profiles"]
        .as_array()
        .expect("profiles")
        .iter()
        .filter_map(|profile| profile["image"].as_str())
        .filter(|image| !image.is_empty())
        .map(str::to_string)
        .collect();
    assert!(
        !offered.is_empty(),
        "controls.json offers no illustrated pad"
    );
    let missing: Vec<&String> = offered
        .iter()
        .filter(|image| !staged.join(image).is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "the kit's shared menu-assets is missing controller artwork the builder \
         offers, so exporting those consoles fails: {missing:?}. Restage with \
         sh scripts/native_runtime/build-builder-macos.sh"
    );
    let design = repo_root().join("integrations/designs/native");
    let owned: Vec<_> = std::fs::read_dir(&design)
        .expect("design package")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("controller-")
        })
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

/// A design may style the shared list, but never the rows of one list.
///
/// The shader list, the achievement list and any list we add to Options have
/// the same parts, which are rows we write when we bundle the game, a pager,
/// and a state on the right. They share `.list-row` and the rules beside it.
/// If a design styled `.shader-row`, every other design would have to do the
/// same, and to add a screen we would have to edit every design.
///
/// A design MAY name the place of a composed screen. `#options` is the button
/// on the pause row and `#options-panel` is the panel it opens, in the same
/// way as `#controls`. So we accept `#actions #options`, because it places a
/// button and does not restyle a list.
///
/// We strip comments first, so a comment that explains the shared list by
/// naming the lists that use it is not a violation.
///
/// This test does NOT prove that the shared rules look right, that a design
/// has them at all, or that we give the rows of a composed screen the shared
/// class at export. For that last one we test the staged markup, and not on
/// the stylesheet.
#[test]
fn no_design_styles_one_list_by_name() {
    // Screens that we add at composition. The design defines pause and
    // controls, and must not define these. To add a screen, add a word here.
    const COMPOSED: [&str; 3] = ["shader", "achievement", "options"];
    // The parts of the shared list. It is a defect when one of these is
    // named after a particular screen in a design, and it is not one when
    // it is named after the screen's button or panel.
    const PARTS: [&str; 4] = ["row", "list", "pager", "entry"];
    let designs = rominabox_desktop::repo::at("integrations/designs");
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
                    let Some(rest) = identifier.strip_prefix(name) else {
                        continue;
                    };
                    assert!(
                        !PARTS.iter().any(|part| rest.contains(part)),
                        "{} styles the {name} list's own rows: {}\n\
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

/// Hover alone must not add another focus border beside keyboard selection.
#[test]
fn list_hover_does_not_paint_a_second_focus_border() {
    for design in ["native", "disc"] {
        let css = read(&format!("integrations/designs/{design}/menu.rcss"))
            .expect("shipped design stylesheet");
        let hover = css
            .split_once(".list-row:hover")
            .and_then(|(_, rule)| rule.split_once('}'))
            .map(|(rule, _)| rule)
            .expect("list row hover rule");
        assert!(
            !hover.contains("border-color"),
            "{design} paints a second list focus border on hover"
        );
    }
}
