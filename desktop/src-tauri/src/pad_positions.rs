//! The position of each control on the standard pad.
//!
//! Through RetroArch's controller profiles, every player's controller becomes
//! one standard pad (the RetroPad), so a control bound to a position there
//! works on any pad. We read a control from its own position unless the
//! author moved it. When the author moves a control, its key moves with it,
//! we tell the game's menu the new position, and a remap line maps the
//! control for the core. A RetroArch remap applies to a position in which
//! the keyboard and the pad are combined, so we cannot move them apart.
//!
//! A stick's directions are positions too. In RetroArch a stick axis is one
//! input, so when we remap either half, the other half gives nothing from
//! the key or the pad (we test this in `scripts/native_runtime/remap_play.c`).
//! So a direction may stay in place only while its opposite stays too.

use crate::controls::{ControlDefinition, ControlOverride, ControlProfile, PadPosition};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A control and the position we read it from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub control: String,
    pub slot: String,
}

/// The position we read `control` from, according to `chosen`: where the
/// author moved it, or its own. We check that the positions agree in `place`.
pub fn chosen<'a>(control: &'a str, chosen: &'a BTreeMap<String, ControlOverride>) -> &'a str {
    chosen.get(control).and_then(|value| value.pad.as_deref()).unwrap_or(control)
}

/// Every declared control's position: the author's choice, or its own.
///
/// No two controls may share a position, and a stick direction left where
/// it is may not lose its opposite.
pub fn place(
    declared: &[ControlDefinition],
    chosen: &BTreeMap<String, ControlOverride>,
    positions: &[PadPosition],
) -> Result<Vec<Placed>, String> {
    let name = |slot: &str| {
        positions
            .iter()
            .find(|position| position.id == slot)
            .map_or(slot.to_string(), |position| position.name.clone())
    };
    let label = |id: &str| {
        declared
            .iter()
            .find(|control| control.id == id)
            .map_or(id.to_string(), |control| control.label.clone())
    };
    let mut placed: Vec<Placed> = Vec::new();
    for control in declared {
        let slot = self::chosen(&control.id, chosen).to_string();
        if slot != control.id && !positions.iter().any(|position| position.id == slot) {
            return Err(format!("{slot} is not a position on the pad (for {})", control.id));
        }
        if let Some(other) = placed.iter().find(|other| other.slot == slot) {
            return Err(format!(
                "{} and {} are both on {}",
                label(&other.control),
                control.label,
                name(&slot)
            ));
        }
        placed.push(Placed {
            control: control.id.clone(),
            slot,
        });
    }
    for entry in placed.iter().filter(|entry| entry.slot == entry.control) {
        let Some(opposite) = positions
            .iter()
            .find(|position| position.id == entry.control)
            .and_then(|position| position.opposite.as_deref())
        else {
            continue;
        };
        let moved = placed
            .iter()
            .find(|other| other.control == opposite && other.slot != opposite);
        let taken = placed
            .iter()
            .find(|other| other.slot == opposite && other.control != opposite);
        let cause = match (moved, taken) {
            (Some(moved), _) => format!("{} is moved", label(&moved.control)),
            (None, Some(taken)) => format!("{} is on {}", label(&taken.control), name(opposite)),
            (None, None) => continue,
        };
        return Err(format!(
            "{} stops working while {cause}: a stick's opposite directions move together",
            label(&entry.control)
        ));
    }
    Ok(placed)
}

/// The remap lines that give the core each moved control, one line for each
/// position that changed. We unmap a position that one control left and no
/// other control took, so we read the control only from its new position.
/// Empty when the author moved nothing.
pub fn remap_lines(placed: &[Placed], positions: &[PadPosition]) -> Result<String, String> {
    let is_position = |id: &str| positions.iter().any(|position| position.id == id);
    let mut lines = String::new();
    for entry in placed.iter().filter(|entry| entry.slot != entry.control) {
        lines.push_str(&format!(
            "{} = \"{}\"\n",
            remap_key(&entry.slot)?,
            bind_number(&entry.control)?
        ));
    }
    for entry in placed.iter().filter(|entry| entry.slot != entry.control) {
        let left = &entry.control;
        if is_position(left) && !placed.iter().any(|other| &other.slot == left) {
            lines.push_str(&format!("{} = \"-1\"\n", remap_key(left)?));
        }
    }
    Ok(lines)
}

/// The remap file we put in an export: the emulated device, when the pad
/// profile has one, then `moved`, the lines from `remap_lines`. Empty when
/// there is neither.
pub fn remap_file(profile: &ControlProfile, moved: &str) -> String {
    let device = profile
        .core_device
        .map(|device| format!("input_libretro_device_p1 = \"{device}\"\n"))
        .unwrap_or_default();
    device + moved
}

/// The key for a position in a remap file, spelled as in RetroArch's remap
/// loader (`input_remapping_load_file`, configuration.c). A button's key is
/// `input_player1_btn_<position>` and a stick direction's key is
/// `input_player1_stk_<stick>_<axis><sign>`.
fn remap_key(position: &str) -> Result<String, String> {
    let stick = |sign: &str, symbol: &str| {
        position
            .strip_suffix(sign)
            .map(|axis| format!("input_player1_stk_{axis}{symbol}"))
    };
    match stick("_plus", "+").or_else(|| stick("_minus", "-")) {
        Some(key) => Ok(key),
        None if joypad_ids().contains_key(position) => Ok(format!("input_player1_btn_{position}")),
        None => Err(format!("RetroArch's remap has no position {position}")),
    }
}

/// The number of a control in a remap: libretro's joypad id for a button
/// (`RETRO_DEVICE_ID_JOYPAD_<POSITION>`, libretro.h), or RetroArch's analog
/// bind for a stick direction (`RARCH_ANALOG_<STICK>_<AXIS>_<SIGN>`,
/// input_defines.h).
fn bind_number(position: &str) -> Result<u32, String> {
    if let Some(id) = joypad_ids().get(position) {
        return Ok(*id);
    }
    let words: Vec<&str> = position.split('_').collect();
    let stick = match words.first() {
        Some(&"l") => "LEFT",
        Some(&"r") => "RIGHT",
        _ => "",
    };
    let constant = format!(
        "RARCH_ANALOG_{stick}_{}",
        words.get(1..).unwrap_or_default().join("_").to_ascii_uppercase()
    );
    analog_ids()
        .get(&constant)
        .copied()
        .ok_or_else(|| format!("RetroArch declares no bind number for {position}"))
}

/// The libretro joypad ids by position, as in the fork's `libretro.h`.
fn joypad_ids() -> &'static BTreeMap<String, u32> {
    const LIBRETRO: &str =
        include_str!("../../../vendor/retroarch/libretro-common/include/libretro.h");
    static IDS: OnceLock<BTreeMap<String, u32>> = OnceLock::new();
    IDS.get_or_init(|| {
        LIBRETRO
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let name = words.nth(1)?.strip_prefix("RETRO_DEVICE_ID_JOYPAD_")?;
                (words.next()?.parse::<u32>().ok()).map(|id| (name.to_ascii_lowercase(), id))
            })
            .collect()
    })
}

/// RetroArch's analog binds by constant, as in the fork's
/// `input_defines.h`: an enum that starts at `RARCH_FIRST_CUSTOM_BIND`.
fn analog_ids() -> &'static BTreeMap<String, u32> {
    const DEFINES: &str = include_str!("../../../vendor/retroarch/input/input_defines.h");
    static IDS: OnceLock<BTreeMap<String, u32>> = OnceLock::new();
    IDS.get_or_init(|| {
        let first = DEFINES
            .lines()
            .find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some("#define") && words.next() == Some("RARCH_FIRST_CUSTOM_BIND"))
                    .then(|| words.next()?.parse::<u32>().ok())
                    .flatten()
            })
            .expect("input_defines.h defines RARCH_FIRST_CUSTOM_BIND");
        let start = DEFINES
            .find("RARCH_ANALOG_LEFT_X_PLUS = RARCH_FIRST_CUSTOM_BIND,")
            .expect("input_defines.h counts the analog binds from RARCH_FIRST_CUSTOM_BIND");
        DEFINES[start..]
            .lines()
            .map(|line| line.trim().split(['=', ',']).next().unwrap_or("").trim())
            .take_while(|name| name.starts_with("RARCH_ANALOG_") && *name != "RARCH_ANALOG_BIND_LIST_END")
            .zip(first..)
            .map(|(name, id)| (name.to_string(), id))
            .collect()
    })
}

#[cfg(test)]
mod played;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::{pad_positions, profile_for_system};

    pub(super) fn moved(pairs: &[(&str, &str)]) -> BTreeMap<String, ControlOverride> {
        pairs
            .iter()
            .map(|(control, pad)| {
                (
                    control.to_string(),
                    ControlOverride {
                        pad: Some(pad.to_string()),
                        ..ControlOverride::default()
                    },
                )
            })
            .collect()
    }

    fn placed(system: &str, pairs: &[(&str, &str)]) -> Result<Vec<Placed>, String> {
        place(
            &profile_for_system(system).unwrap().controls,
            &moved(pairs),
            &pad_positions().unwrap(),
        )
    }

    #[test]
    fn the_numbers_and_keys_a_remap_names_are_retroarchs() {
        assert_eq!(bind_number("b"), Ok(0));
        assert_eq!(bind_number("a"), Ok(8));
        assert_eq!(bind_number("r3"), Ok(15));
        assert_eq!(bind_number("l_x_plus"), Ok(16));
        assert_eq!(bind_number("l_y_minus"), Ok(19));
        assert_eq!(bind_number("r_y_minus"), Ok(23));
        assert_eq!(remap_key("b").as_deref(), Ok("input_player1_btn_b"));
        assert_eq!(remap_key("l_x_minus").as_deref(), Ok("input_player1_stk_l_x-"));
        assert_eq!(remap_key("r_y_plus").as_deref(), Ok("input_player1_stk_r_y+"));
        for position in pad_positions().unwrap() {
            assert!(bind_number(&position.id).is_ok(), "{}", position.id);
            assert!(remap_key(&position.id).is_ok(), "{}", position.id);
        }
    }

    /// Mega Drive C is RetroPad a. When the author swaps it with B, each key
    /// stays with its control and the core gets C from the bottom button.
    #[test]
    fn swapped_controls_hand_the_core_what_it_expects() {
        let positions = pad_positions().unwrap();
        let placed = placed("megadrive", &[("a", "b"), ("b", "a")]).unwrap();
        let slot = |control: &str| {
            placed
                .iter()
                .find(|entry| entry.control == control)
                .unwrap()
                .slot
                .clone()
        };
        assert_eq!(
            (slot("a"), slot("b"), slot("y")),
            ("b".into(), "a".into(), "y".into())
        );
        assert_eq!(
            remap_lines(&placed, &positions).unwrap(),
            "input_player1_btn_a = \"0\"\ninput_player1_btn_b = \"8\"\n"
        );
    }

    #[test]
    fn a_position_left_empty_is_unmapped() {
        let positions = pad_positions().unwrap();
        let placed = placed("megadrive", &[("a", "x")]).unwrap();
        assert_eq!(
            remap_lines(&placed, &positions).unwrap(),
            "input_player1_btn_x = \"8\"\ninput_player1_btn_a = \"-1\"\n"
        );
    }

    #[test]
    fn nothing_moved_writes_no_remap() {
        let positions = pad_positions().unwrap();
        let placed = placed("megadrive", &[]).unwrap();
        assert!(placed.iter().all(|entry| entry.slot == entry.control));
        assert_eq!(remap_lines(&placed, &positions).unwrap(), "");
    }

    #[test]
    fn two_controls_on_one_position_are_refused() {
        let error = placed("megadrive", &[("a", "b")]).unwrap_err();
        assert!(error.contains("both on Bottom button"), "{error}");
        let error = placed("ps1", &[("a", "north")]).unwrap_err();
        assert!(error.contains("not a position"), "{error}");
    }

    /// The left stick captured on the d-pad, one direction at a time. We swap
    /// each direction with the d-pad button that was in its position, and
    /// with the remap the core gets the stick from the d-pad and the d-pad
    /// from the stick.
    #[test]
    fn a_stick_moves_onto_the_d_pad_whole() {
        let positions = pad_positions().unwrap();
        let placed = placed(
            "ps1",
            &[
                ("l_y_minus", "up"),
                ("up", "l_y_minus"),
                ("l_x_plus", "right"),
                ("right", "l_x_plus"),
                ("l_y_plus", "down"),
                ("down", "l_y_plus"),
                ("l_x_minus", "left"),
                ("left", "l_x_minus"),
            ],
        )
        .unwrap();
        let lines = remap_lines(&placed, &positions).unwrap();
        for line in [
            "input_player1_btn_up = \"19\"",
            "input_player1_stk_l_y- = \"4\"",
            "input_player1_btn_right = \"16\"",
            "input_player1_stk_l_x+ = \"7\"",
            "input_player1_btn_down = \"18\"",
            "input_player1_stk_l_y+ = \"5\"",
            "input_player1_btn_left = \"17\"",
            "input_player1_stk_l_x- = \"6\"",
        ] {
            assert!(lines.lines().any(|written| written == line), "no {line} in\n{lines}");
        }
        assert_eq!(lines.lines().count(), 8, "{lines}");
    }

    /// In RetroArch an axis is one input, so if the author moved one direction
    /// and left its opposite, the opposite would stop working. We refuse this
    /// and name that direction, however the axis lost its other half.
    #[test]
    fn half_an_axis_moved_is_refused() {
        let error = placed("ps1", &[("l_x_minus", "b"), ("b", "l_x_minus")]).unwrap_err();
        assert_eq!(
            error,
            "Left stick right stops working while Left stick left is moved: a stick's \
             opposite directions move together"
        );
        let error = placed("ps1", &[("r_y_minus", "r3"), ("r3", "r_y_minus")]).unwrap_err();
        assert!(error.starts_with("Right stick down stops working"), "{error}");
        // The N64 pad has no right stick, so the author may move a button onto
        // one of its directions without leaving a control of that axis behind.
        assert!(placed("n64", &[("a", "r_x_minus")]).is_ok());
        let error = placed("n64", &[("a", "l_x_minus"), ("l_x_minus", "a")]).unwrap_err();
        assert!(error.starts_with("Stick right stops working"), "{error}");
    }

    /// The author may swap a whole axis with the other stick's axis.
    #[test]
    fn a_whole_axis_may_move_to_the_other_stick() {
        let positions = pad_positions().unwrap();
        let placed = placed(
            "ps1",
            &[
                ("l_x_minus", "r_x_minus"),
                ("r_x_minus", "l_x_minus"),
                ("l_x_plus", "r_x_plus"),
                ("r_x_plus", "l_x_plus"),
            ],
        )
        .unwrap();
        let lines = remap_lines(&placed, &positions).unwrap();
        for line in [
            "input_player1_stk_r_x- = \"17\"",
            "input_player1_stk_l_x- = \"21\"",
            "input_player1_stk_r_x+ = \"16\"",
            "input_player1_stk_l_x+ = \"20\"",
        ] {
            assert!(lines.lines().any(|written| written == line), "no {line} in\n{lines}");
        }
    }
}
