//! What the Options screen contains in a composed menu, for every design.
//!
//! We compose with `themes::compose_menu`, as in an export, from a kit with
//! the shipped designs and controller artwork.

use rominabox_desktop::{controls::Controls, shaders::ShaderSelection, themes};
use std::{
    fs,
    path::{Path, PathBuf},
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
            include_achievements: false,
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
