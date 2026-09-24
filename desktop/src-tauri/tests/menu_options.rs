//! What the Options screen contains in a composed menu, for every design.
//!
//! We compose with `themes::compose_menu`, as in an export, from a kit with
//! the shipped designs and controller artwork.

use rominabox_desktop::{controls::Controls, shaders::ShaderSelection, themes};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

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

fn kit(root: &Path) -> PathBuf {
    let kit = root.join("kit");
    copy_tree(
        &rominabox_desktop::repo::at("integrations/designs"),
        &kit.join("designs"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("desktop/assets/controllers"),
        &kit.join("menu-assets"),
    );
    kit
}

struct Composed {
    menu: String,
    cfg: String,
}

fn compose(
    kit: &Path,
    design: &str,
    entries: Option<&[String]>,
    discs: usize,
    destination: &Path,
) -> Composed {
    themes::compose_menu(
        &themes::MenuRequest {
            kit,
            design,
            palette: "blue",
            background: None,
            system: "megadrive",
            controls: &Controls::default(),
            show_menu: true,
            splash: false,
            include_achievements: entries
                .is_some_and(|entries| entries.iter().any(|entry| entry == "achievements")),
            menu_entries: entries,
            shaders: &ShaderSelection::default(),
            discs,
        },
        destination,
    )
    .unwrap_or_else(|error| panic!("{design}: {error}"));
    Composed {
        menu: fs::read_to_string(destination.join("menu.rml")).unwrap(),
        cfg: fs::read_to_string(destination.join("design.cfg")).unwrap(),
    }
}

fn designs() -> Vec<String> {
    themes::registry()
        .unwrap()
        .designs
        .into_iter()
        .map(|design| design.id)
        .collect()
}

fn declared_screens(cfg: &str) -> Vec<String> {
    cfg.lines()
        .find_map(|line| line.strip_prefix("screens = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// When an author turns every Options entry off, there is no Options screen.
/// A cartridge has one disc, so there is no disc entry either.
#[test]
fn an_empty_options_list_ships_no_options_screen() {
    let root = rominabox_scratch::Scratch::dir("rominabox-empty-options");
    let kit = kit(&root);
    for design in designs() {
        let composed = compose(&kit, &design, Some(&[]), 1, &root.join(&design));
        let screens = declared_screens(&composed.cfg);
        assert!(
            !screens.iter().any(|id| id == "options"),
            "{design}: an empty list still declares an Options screen: {screens:?}"
        );
        for id in ["options-panel", "options", "discs", "discs-panel"] {
            assert!(
                !composed.menu.contains(&format!("id=\"{id}\"")),
                "{design}: an empty Options list still ships #{id}"
            );
        }
    }
}

/// The disc entry and its list exist only when the game has more than one
/// disc. A cartridge's default Options contains its default entries and no DISC.
#[test]
fn a_one_disc_game_has_no_disc_entry() {
    let root = rominabox_scratch::Scratch::dir("rominabox-one-disc");
    let kit = kit(&root);
    for design in designs() {
        let composed = compose(&kit, &design, None, 1, &root.join(&design));
        let screens = declared_screens(&composed.cfg);
        assert!(
            !screens.iter().any(|id| id == "discs"),
            "{design}: a one-disc game declares the disc list: {screens:?}"
        );
        for id in ["discs", "discs-panel"] {
            assert!(
                !composed.menu.contains(&format!("id=\"{id}\"")),
                "{design}: a one-disc game ships #{id}"
            );
        }
    }
}

/// A game of several discs has the disc list, whatever the author chose, and
/// in Native the player reaches it from Options.
#[test]
fn a_game_of_several_discs_gets_the_disc_list() {
    let root = rominabox_scratch::Scratch::dir("rominabox-several-discs");
    let kit = kit(&root);
    for design in designs() {
        for entries in [None, Some(Vec::new())] {
            let destination = root.join(format!("{design}-{}", entries.is_some()));
            let composed = compose(&kit, &design, entries.as_deref(), 3, &destination);
            let screens = declared_screens(&composed.cfg);
            assert!(
                screens.iter().any(|id| id == "discs"),
                "{design}: three discs and no disc list: {screens:?}"
            );
            assert!(composed.menu.contains("id=\"discs-panel\""), "{design}");
            if design == "native" {
                assert!(
                    screens.iter().any(|id| id == "options"),
                    "native reaches the disc list from Options: {screens:?}"
                );
                assert!(composed.menu.contains("id=\"discs\""), "the DISC entry");
            }
        }
    }
}

/// The windowless RmlUi probe used by the `menu` tests. We build it with the
/// same recipe as those tests, so this test cannot link a different RmlUi.
fn rml_probe() -> PathBuf {
    let output = Command::new("python3")
        .current_dir(rominabox_desktop::repo::root())
        .args([
            "-c",
            "import sys; sys.path.insert(0, 'scripts'); import menu_interaction as m; m.build(); print(m.PROBE)",
        ])
        .output()
        .expect("python3 runs");
    assert!(
        output.status.success(),
        "could not build the RmlUi probe (python3 scripts/prepare_rmlui.py first): {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

/// Options entries do not overlap. In the RmlUi layout, the next entry covers
/// the lower edge of an entry that overlaps it, and navigation by layout can
/// never reach the covered one. Every entry comes from the same template, so
/// each must be under the pointer for the same height.
#[test]
fn options_entries_do_not_overlap() {
    let probe = rml_probe();
    let root = rominabox_scratch::Scratch::dir("rominabox-options-overlap");
    let kit = kit(&root);
    let entries: Vec<String> = ["controls", "shaders", "achievements"]
        .into_iter()
        .map(str::to_string)
        .collect();
    for design in designs() {
        let destination = root.join(&design);
        let composed = compose(&kit, &design, Some(&entries), 1, &destination);
        // Options is the visible screen, and Pause is hidden.
        let shown = composed
            .menu
            .replacen("<div id=\"pause-panel\">", "<div id=\"pause-panel\" style=\"display:none;\">", 1)
            .replacen(
                "id=\"options-panel\" style=\"display:none;\"",
                "id=\"options-panel\"",
                1,
            )
            .replacen(
                "id=\"options-panel\" class=\"screen-panel\" style=\"display:none;\"",
                "id=\"options-panel\" class=\"screen-panel\"",
                1,
            );
        assert_ne!(shown, composed.menu, "{design}: Options could not be shown");
        let document = destination.join("options.rml");
        fs::write(&document, shown).unwrap();
        let mut command = Command::new(&probe);
        command.arg("--document").arg(&document).args(["--size", "960x600"]);
        for y in 0..600 {
            command.args(["--step", &format!("move:480,{y}")]);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{design}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut rows: BTreeMap<String, usize> = BTreeMap::new();
        for line in String::from_utf8(output.stdout).unwrap().lines() {
            let step: serde_json::Value = serde_json::from_str(line).unwrap();
            let hover = step["hover"].as_str().unwrap_or_default();
            if entries.iter().any(|entry| entry == hover) {
                *rows.entry(hover.to_string()).or_default() += 1;
            }
        }
        assert_eq!(rows.len(), entries.len(), "{design}: entries under the pointer: {rows:?}");
        let heights: Vec<usize> = entries.iter().map(|entry| rows[entry]).collect();
        assert!(
            heights.iter().all(|height| *height == heights[0]),
            "{design}: each entry is under the pointer for a different height, so one \
             covers another: {rows:?}"
        );
    }
}
