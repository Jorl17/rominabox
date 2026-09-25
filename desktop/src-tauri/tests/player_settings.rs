//! The settings that a player can change in Options are in the Options of
//! every design, shipped or with a layout no shipped design has, without any
//! declaration in the design.

mod support;

use rominabox_desktop::{
    menu,
    player_settings::{self, Defaults, Kind, PlayerSetting},
    themes,
};
use std::{fs, path::Path};
use support::{boxes, showing};

struct Composed {
    menu: String,
    cfg: String,
    files: Vec<String>,
}

fn compose(
    kit: &Path,
    design: &str,
    request: impl FnOnce(&mut menu::MenuRequest),
    to: &Path,
) -> Composed {
    let mut wanted =
        menu::MenuRequest::new(themes::staged_design(kit, design), kit.join("menu-assets"));
    request(&mut wanted);
    let composition =
        menu::compose_menu(&wanted).unwrap_or_else(|error| panic!("{design}: {error}"));
    composition.write(to).unwrap();
    Composed {
        menu: composition.text("menu.rml").unwrap().to_string(),
        cfg: composition.text("design.cfg").unwrap().to_string(),
        files: composition
            .names()
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect(),
    }
}

fn every_design() -> Vec<String> {
    let mut designs = support::designs();
    designs.extend(support::hypothetical_designs());
    designs
}

fn state(menu: &str, setting: &PlayerSetting) -> String {
    let marker = format!("id=\"{}-state\"", setting.control());
    let at = menu.find(&marker).expect("the switch shows its state");
    let text = &menu[at..];
    let open = text.find('>').unwrap() + 1;
    let close = text.find('<').unwrap();
    text[open..close].to_string()
}

fn opening_tag<'a>(menu: &'a str, id: &str) -> &'a str {
    let at = menu
        .find(&format!("id=\"{id}\""))
        .expect("the element is drawn");
    let start = menu[..at].rfind('<').unwrap();
    let end = at + menu[at..].find('>').unwrap();
    &menu[start..end]
}

/// In every design, we draw each setting in Options, where a player can reach
/// it, and declare it to the player with the key it sets.
#[test]
fn every_design_offers_every_player_setting() {
    let root = rominabox_scratch::Scratch::dir("rominabox-player-settings-every-design");
    let kit = support::kit_with_hypothetical(&root);
    let settings = player_settings::declared(Defaults::default());
    for design in every_design() {
        let destination = root.join(&design);
        let composed = compose(&kit, &design, |_| {}, &destination);
        let document = destination.join("options.rml");
        fs::write(&document, showing(&composed.menu, "options-panel")).unwrap();
        let controls: Vec<String> = settings.iter().map(PlayerSetting::control).collect();
        let ids: Vec<&str> = controls.iter().map(String::as_str).collect();
        for (control, laid_out) in ids.iter().zip(boxes(&document, (960, 600), &ids)) {
            let [x, y, width, height] =
                laid_out.unwrap_or_else(|| panic!("{design}: Options does not draw {control}"));
            assert!(
                width > 0.0
                    && height > 0.0
                    && x >= 0.0
                    && y >= 0.0
                    && x + width <= 960.0
                    && y + height <= 600.0,
                "{design}: {control} is not on screen in Options: {x},{y} {width}x{height}"
            );
        }
        let declared: Vec<&str> = settings.iter().map(|setting| setting.id).collect();
        assert!(
            composed
                .cfg
                .contains(&format!("settings = \"{}\"", declared.join(" "))),
            "{design}: the player is not told about every setting:\n{}",
            composed.cfg
        );
        for setting in &settings {
            assert!(
                composed.cfg.contains(&format!(
                    "setting_key_{} = \"{}\"",
                    setting.id,
                    setting.key.name()
                )) && composed.cfg.contains(&format!(
                    "setting_file_{} = \"{}\"",
                    setting.id,
                    setting.file()
                )),
                "{design}: {} is declared without its key or file",
                setting.id
            );
        }
    }
}

/// The switch starts at the value chosen in the export, and a design can
/// style that value through `on`.
#[test]
fn a_switch_starts_at_the_exports_default() {
    let root = rominabox_scratch::Scratch::dir("rominabox-player-settings-default");
    let kit = support::kit(&root);
    for keep_playing in [false, true] {
        let defaults = Defaults {
            keep_playing_in_background: keep_playing,
        };
        let background = player_settings::declared(defaults)
            .into_iter()
            .find(|setting| matches!(setting.kind, Kind::Switch { .. }))
            .unwrap();
        for design in support::designs() {
            let composed = compose(
                &kit,
                &design,
                |request| request.settings = defaults,
                &root.join(format!("{design}-{keep_playing}")),
            );
            assert_eq!(
                state(&composed.menu, &background),
                if keep_playing { "ON" } else { "OFF" },
                "{design}: the switch does not start at the export's choice"
            );
            let tag = opening_tag(&composed.menu, &background.control());
            let classes = tag
                .split("class=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap_or_default();
            assert_eq!(
                classes.split_whitespace().any(|class| class == "on"),
                keep_playing,
                "{design}: `on` does not say what the export chose: {}",
                opening_tag(&composed.menu, &background.control())
            );
        }
    }
}

/// When a design places a switch itself, with `<!--SETTING:id-->`, we put its
/// toggle there instead of an Options entry.
#[test]
fn a_design_can_place_a_switch_itself() {
    let root = rominabox_scratch::Scratch::dir("rominabox-player-settings-slot");
    let kit = support::kit_with_hypothetical(&root);
    let background = player_settings::declared(Defaults::default())
        .into_iter()
        .find(|setting| matches!(setting.kind, Kind::Switch { .. }))
        .unwrap();
    let options = kit.join("designs/options-columns/screen-options.rml");
    let own = fs::read_to_string(&options).unwrap();
    let slot = menu::setting_slot(&background);
    fs::write(
        &options,
        own.replacen("<!--OPTIONS-->", &format!("<!--OPTIONS-->{slot}"), 1),
    )
    .unwrap();
    let composed = compose(&kit, "options-columns", |_| {}, &root.join("placed"));
    let tag = opening_tag(&composed.menu, &background.control());
    assert!(
        tag.contains("class=\"toggle") && !tag.contains("option-entry"),
        "the placed switch is the design's toggle, not an entry: {tag}"
    );
    assert_eq!(
        composed
            .menu
            .matches(&format!("id=\"{}\"", background.control()))
            .count(),
        1,
        "the switch is drawn once"
    );
    assert!(!composed.menu.contains(&slot), "the slot is filled");
}

/// For a game without a menu sound pack, we ship the volume tick beside the
/// menu. For a game with a pack, or with no volume to change, we do not.
#[test]
fn the_volume_tick_ships_only_where_it_is_heard() {
    let root = rominabox_scratch::Scratch::dir("rominabox-player-settings-tick");
    let kit = support::kit(&root);
    let tick = rominabox_desktop::volume::tick_file();
    let off = compose(
        &kit,
        "native",
        |request| request.sound_pack = false,
        &root.join("off"),
    );
    assert!(off.files.iter().any(|name| name == tick), "{:?}", off.files);
    let copied = fs::read(root.join("off").join(tick)).unwrap();
    assert!(copied.starts_with(b"RIFF"), "the tick is a WAV file");
    let pack = compose(
        &kit,
        "native",
        |request| request.sound_pack = true,
        &root.join("pack"),
    );
    assert!(
        !pack.files.iter().any(|name| name == tick),
        "{:?}",
        pack.files
    );
    let no_options = compose(
        &kit,
        "native",
        |request| {
            request.sound_pack = false;
            request.menu_entries = Some(Vec::new());
        },
        &root.join("no-options"),
    );
    assert!(
        !no_options.menu.contains("volume-level")
            && !no_options.files.iter().any(|name| name == tick),
        "a game with no Options has no volume to hear"
    );
}
