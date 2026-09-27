//! The position of each control on the standard pad.
//!
//! With the RetroArch controller profiles, the controller of every player
//! becomes one standard pad (the RetroPad), so a control bound to a position
//! there works on any pad. We read a control from its default position unless
//! the author moved it. We move the key of a moved control with it, write its
//! position for the menu, and add a remap line so the core gets
//! the right control. A RetroArch remap applies to positions after the keyboard and
//! the pad are merged, so the key and the pad cannot move separately.

use crate::controls::{ControlDefinition, ControlOverride, PadPosition};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A control and the position we read it from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub control: String,
    pub slot: String,
}

/// The position of every declared control, from the author or by default.
///
/// The directions and the press of a stick stay where they are, and no two
/// controls may share a position.
pub fn place(
    declared: &[ControlDefinition],
    chosen: &BTreeMap<String, ControlOverride>,
    positions: &[PadPosition],
) -> Result<Vec<Placed>, String> {
    let mut placed: Vec<Placed> = Vec::new();
    for control in declared {
        let slot = match chosen
            .get(&control.id)
            .and_then(|value| value.pad.as_deref())
        {
            None => control.id.clone(),
            Some(pad) => {
                let Some(position) = positions.iter().find(|position| position.id == pad) else {
                    return Err(format!(
                        "{pad} is not a position on the pad (for {})",
                        control.id
                    ));
                };
                if control.group.is_some() && pad != control.id {
                    return Err(format!(
                        "{} belongs to a stick, which stays where it is",
                        control.label
                    ));
                }
                position.id.clone()
            }
        };
        if let Some(other) = placed.iter().find(|other| other.slot == slot) {
            let name = positions
                .iter()
                .find(|position| position.id == slot)
                .map_or(slot.as_str(), |position| position.name.as_str());
            let label = |id: &str| {
                declared
                    .iter()
                    .find(|control| control.id == id)
                    .map_or(id.to_string(), |control| control.label.clone())
            };
            return Err(format!(
                "{} and {} are both on {name}",
                label(&other.control),
                control.label
            ));
        }
        placed.push(Placed {
            control: control.id.clone(),
            slot,
        });
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
            "input_player1_btn_{} = \"{}\"\n",
            entry.slot,
            joypad_id(&entry.control)?
        ));
    }
    for entry in placed.iter().filter(|entry| entry.slot != entry.control) {
        let left = &entry.control;
        if is_position(left) && !placed.iter().any(|other| &other.slot == left) {
            lines.push_str(&format!("input_player1_btn_{left} = \"-1\"\n"));
        }
    }
    Ok(lines)
}

/// The libretro number for a pad position, as in a remap. We read it from
/// `libretro.h` in the fork, which declares `RETRO_DEVICE_ID_JOYPAD_<POSITION>`.
fn joypad_id(position: &str) -> Result<u32, String> {
    const LIBRETRO: &str =
        include_str!("../../../vendor/retroarch/libretro-common/include/libretro.h");
    static IDS: OnceLock<BTreeMap<String, u32>> = OnceLock::new();
    let ids = IDS.get_or_init(|| {
        LIBRETRO
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let name = words.nth(1)?.strip_prefix("RETRO_DEVICE_ID_JOYPAD_")?;
                (words.next()?.parse::<u32>().ok()).map(|id| (name.to_ascii_lowercase(), id))
            })
            .collect()
    });
    ids.get(position)
        .copied()
        .ok_or_else(|| format!("libretro.h declares no joypad id for {position}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::{pad_positions, profile_for_system};

    fn moved(pairs: &[(&str, &str)]) -> BTreeMap<String, ControlOverride> {
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

    #[test]
    fn the_numbers_a_remap_names_are_libretros() {
        assert_eq!(joypad_id("b"), Ok(0));
        assert_eq!(joypad_id("a"), Ok(8));
        assert_eq!(joypad_id("r3"), Ok(15));
        for position in pad_positions().unwrap() {
            assert!(joypad_id(&position.id).is_ok(), "{}", position.id);
        }
    }

    /// Mega Drive C is RetroPad a. When the author swaps it with B, each key
    /// stays with its control and the core gets C from the bottom button.
    #[test]
    fn swapped_controls_hand_the_core_what_it_expects() {
        let profile = profile_for_system("megadrive").unwrap();
        let positions = pad_positions().unwrap();
        let placed = place(
            &profile.controls,
            &moved(&[("a", "b"), ("b", "a")]),
            &positions,
        )
        .unwrap();
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
        let profile = profile_for_system("megadrive").unwrap();
        let positions = pad_positions().unwrap();
        let placed = place(&profile.controls, &moved(&[("a", "x")]), &positions).unwrap();
        assert_eq!(
            remap_lines(&placed, &positions).unwrap(),
            "input_player1_btn_x = \"8\"\ninput_player1_btn_a = \"-1\"\n"
        );
    }

    #[test]
    fn nothing_moved_writes_no_remap() {
        let profile = profile_for_system("megadrive").unwrap();
        let positions = pad_positions().unwrap();
        let placed = place(&profile.controls, &BTreeMap::new(), &positions).unwrap();
        assert!(placed.iter().all(|entry| entry.slot == entry.control));
        assert_eq!(remap_lines(&placed, &positions).unwrap(), "");
    }

    #[test]
    fn two_controls_on_one_position_are_refused() {
        let profile = profile_for_system("megadrive").unwrap();
        let error = place(
            &profile.controls,
            &moved(&[("a", "b")]),
            &pad_positions().unwrap(),
        )
        .unwrap_err();
        assert!(error.contains("both on Bottom button"), "{error}");
    }

    #[test]
    fn a_stick_stays_where_it_is() {
        let profile = profile_for_system("ps1").unwrap();
        let error = place(
            &profile.controls,
            &moved(&[("l_x_minus", "b")]),
            &pad_positions().unwrap(),
        )
        .unwrap_err();
        assert!(error.contains("belongs to a stick"), "{error}");
        let error = place(
            &profile.controls,
            &moved(&[("a", "north")]),
            &pad_positions().unwrap(),
        )
        .unwrap_err();
        assert!(error.contains("not a position"), "{error}");
    }
}
