//! What the core receives when the player presses something in an exported
//! game. We write the controls file and the remap with
//! `write_defaults_config` and `remap_file`, and apply them with the fork's
//! own input layer (`scripts/native_runtime/remap_play.c`). The pad is a
//! stand-in, mapped to the standard pad through its autoconfig profile, and
//! the keyboard is a stand-in with a list of the keys that are down.

use super::tests::moved;
use crate::controls::{self, Controls};
use crate::retroarch_probe::{Probe, CONFIGURED, INPUT_LAYER};
use rominabox_scratch::Scratch;
use std::fs;

fn probe() -> Probe {
    Probe::build_defining("remap_play", CONFIGURED, INPUT_LAYER)
}

/// A game's controls as we export them, and what the core receives for
/// each press, one press at a time.
struct Game {
    probe: Probe,
    _folder: Scratch,
    controls: String,
    remap: String,
}

impl Game {
    fn exported(system: &str, chosen: &Controls) -> Self {
        let folder = Scratch::dir("rominabox-remap-play");
        let controls = folder.path().join("controls-defaults.cfg");
        let profile = controls::write_defaults_config(system, chosen, &controls).unwrap();
        let moved = controls::placement(system, chosen)
            .and_then(|placed| super::remap_lines(&placed, &controls::pad_positions()?))
            .unwrap();
        let remap = match super::remap_file(&profile, &moved) {
            text if text.is_empty() => "none".to_string(),
            text => {
                let path = folder.path().join("game.rmp");
                fs::write(&path, text).unwrap();
                path.display().to_string()
            }
        };
        Self {
            probe: probe(),
            controls: controls.display().to_string(),
            _folder: folder,
            remap,
        }
    }

    /// The remap RetroArch would get if we skipped the placement checks.
    fn exported_unchecked(system: &str, pairs: &[(&str, &str)]) -> Self {
        let game = Self::exported(system, &Controls::default());
        let profile = controls::profile_for_system(system).unwrap();
        let placed: Vec<super::Placed> = profile
            .controls
            .iter()
            .map(|control| super::Placed {
                control: control.id.clone(),
                slot: pairs
                    .iter()
                    .find(|(id, _)| *id == control.id)
                    .map_or(control.id.clone(), |(_, slot)| slot.to_string()),
            })
            .collect();
        let lines = super::remap_lines(&placed, &controls::pad_positions().unwrap()).unwrap();
        let path = game._folder.path().join("unchecked.rmp");
        fs::write(&path, super::remap_file(&profile, &lines)).unwrap();
        Self {
            remap: path.display().to_string(),
            ..game
        }
    }

    fn reads(&self, press: &str) -> Vec<String> {
        self.probe.lines(&[&self.controls, &self.remap, press])
    }
}

fn keys(pairs: &[(&str, &str)]) -> Controls {
    serde_json::from_value(serde_json::json!({
        "bindings": pairs
            .iter()
            .map(|(control, key)| (control.to_string(), serde_json::json!({ "key": key })))
            .collect::<serde_json::Map<_, _>>()
    }))
    .unwrap()
}

/// A stick bound to keys, as we bind it when the author captures a stick.
/// Pressing a key gives its full direction, and the pad's stick still works.
#[test]
fn a_stick_bound_to_keys_is_pushed_by_them() {
    let game = Game::exported(
        "ps1",
        &keys(&[
            ("l_y_minus", "y"),
            ("l_x_plus", "m"),
            ("l_y_plus", "n"),
            ("l_x_minus", "h"),
        ]),
    );
    let remap = fs::read_to_string(&game.remap).unwrap();
    assert_eq!(remap, "input_libretro_device_p1 = \"517\"\n", "keys move nothing on the pad");
    assert_eq!(game.reads("key:y"), ["l_y_minus 32767"]);
    assert_eq!(game.reads("key:m"), ["l_x_plus 32767"]);
    assert_eq!(game.reads("key:n"), ["l_y_plus 32767"]);
    assert_eq!(game.reads("key:h"), ["l_x_minus 32767"]);
    assert_eq!(game.reads("pad:l_x_minus"), ["l_x_minus 32767"]);
    // The PlayStation Square button stays on its own key and position.
    assert_eq!(game.reads("pad:y"), ["y"]);
}

/// The left stick captured on the d-pad. We read the stick from the d-pad
/// and the d-pad from the stick, and every key moved with its control.
#[test]
fn a_stick_moved_onto_the_d_pad_is_read_from_it() {
    let mut chosen = Controls::default();
    chosen.bindings = moved(&[
        ("l_y_minus", "up"),
        ("up", "l_y_minus"),
        ("l_x_plus", "right"),
        ("right", "l_x_plus"),
        ("l_y_plus", "down"),
        ("down", "l_y_plus"),
        ("l_x_minus", "left"),
        ("left", "l_x_minus"),
    ]);
    let game = Game::exported("ps1", &chosen);
    assert_eq!(game.reads("pad:up"), ["l_y_minus 32767"]);
    assert_eq!(game.reads("pad:left"), ["l_x_minus 32767"]);
    assert_eq!(game.reads("pad:right"), ["l_x_plus 32767"]);
    assert_eq!(game.reads("pad:l_y_minus"), ["up"]);
    assert_eq!(game.reads("pad:l_x_plus"), ["right"]);
    // The stick's default keys are T, B, G and V. The d-pad's are the arrows.
    assert_eq!(game.reads("key:t"), ["l_y_minus 32767"]);
    assert_eq!(game.reads("key:v"), ["l_x_minus 32767"]);
    assert_eq!(game.reads("key:up"), ["up"]);
    assert_eq!(game.reads("key:left"), ["left"]);
}

/// Why the author may not move half an axis. In RetroArch the half left in
/// place then gives nothing, from the pad or from its key. The other axis
/// still works.
#[test]
fn retroarch_drops_the_half_of_an_axis_left_in_place() {
    let game = Game::exported_unchecked("ps1", &[("l_x_minus", "b"), ("b", "l_x_minus")]);
    assert_eq!(game.reads("pad:b"), ["l_x_minus 32767"]);
    assert_eq!(game.reads("pad:l_x_minus"), ["b"]);
    assert_eq!(game.reads("pad:l_x_plus"), Vec::<String>::new());
    assert_eq!(game.reads("key:b"), Vec::<String>::new(), "Left stick right's key");
    assert_eq!(game.reads("pad:l_y_minus"), ["l_y_minus 32767"]);
}

/// When the author moves a button onto a stick direction, the player presses
/// it by moving the stick, and its key moves with it.
#[test]
fn a_button_moved_onto_a_whole_axis_is_pressed_by_the_stick() {
    let mut chosen = Controls::default();
    chosen.bindings = moved(&[
        ("b", "l_x_minus"),
        ("l_x_minus", "b"),
        ("y", "l_x_plus"),
        ("l_x_plus", "y"),
    ]);
    let game = Game::exported("ps1", &chosen);
    assert_eq!(game.reads("pad:l_x_minus"), ["b"]);
    assert_eq!(game.reads("pad:l_x_plus"), ["y"]);
    assert_eq!(game.reads("pad:b"), ["l_x_minus 32767"]);
    // The key for Cross is Z, and the key for Square is A.
    assert_eq!(game.reads("key:z"), ["b"]);
    assert_eq!(game.reads("key:a"), ["y"]);
}

/// The Mega Drive C button swapped onto the bottom button. The core gets C
/// from that button, and the key for C moves with it.
#[test]
fn swapped_buttons_are_read_where_they_moved() {
    let mut chosen = Controls::default();
    chosen.bindings = moved(&[("a", "b"), ("b", "a")]);
    let game = Game::exported("megadrive", &chosen);
    assert_eq!(game.reads("pad:b"), ["a"]);
    assert_eq!(game.reads("pad:a"), ["b"]);
    assert_eq!(game.reads("key:c"), ["a"]);
    assert_eq!(game.reads("key:x"), ["b"]);
}
