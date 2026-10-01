//! The settings that a player can change in Options are in the Options of
//! every design, shipped or with a layout no shipped design has, without any
//! declaration in the design.

mod support;

use rominabox_engine::{
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
        let panel = support::panel_with_role(&composed.cfg, "options");
        fs::write(&document, showing(&composed.menu, &panel)).unwrap();
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

/// The game is not silent by the middle of the slider. In every design we
/// declare the volume for each position of its slider, from silence to
/// normal, and five steps up from silence is about -10 dB, which anyone hears.
#[test]
fn the_volume_slider_is_told_an_audible_middle() {
    let root = rominabox_scratch::Scratch::dir("rominabox-player-settings-volume-curve");
    let kit = support::kit(&root);
    let volume = player_settings::volume();
    for design in support::designs() {
        let composed = compose(&kit, &design, |_| {}, &root.join(&design));
        let told = format!("setting_values_{} = \"", volume.id);
        let values: Vec<f32> = composed
            .cfg
            .lines()
            .find_map(|line| line.strip_prefix(&told)?.strip_suffix('"'))
            .unwrap_or_else(|| {
                let said: Vec<&str> = composed
                    .cfg
                    .lines()
                    .filter(|line| line.starts_with("setting_") && line.contains("_volume"))
                    .collect();
                panic!(
                    "{design}: the volume slider is not told its value at each position:\n{}",
                    said.join("\n")
                )
            })
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert!(
            values.first() == Some(&-80.0) && values.last() == Some(&0.0),
            "{design}: the slider runs from silence to normal: {values:?}"
        );
        assert!(
            values.windows(2).all(|pair| pair[0] < pair[1]),
            "{design}: each position is louder than the one before: {values:?}"
        );
        assert!(
            values.len() > 5 && values[5] > -11.0 && values[5] < -9.5,
            "{design}: five steps up from silence is about -10 dB: {values:?}"
        );
    }
}

/// For a game without a menu sound pack, we ship the volume tick beside the
/// menu. For a game with a pack, or with no volume to change, we do not.
#[test]
fn the_volume_tick_ships_only_where_it_is_heard() {
    let root = rominabox_scratch::Scratch::dir("rominabox-player-settings-tick");
    let kit = support::kit(&root);
    let tick = rominabox_engine::volume::tick_file();
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
