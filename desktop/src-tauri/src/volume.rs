//! The range of the volume, and the file with the level that the player chose.
//!
//! The range is in `vendor/retroarch/audio/volume_range.h`, which we also read
//! for the RetroArch volume hotkeys, so both have the same maximum. The volume
//! is a player setting (`player_settings`), and its file, key and control come
//! from its declaration there.

use crate::player_settings;
use std::path::Path;

const RANGE_HEADER: &str = include_str!("../../../vendor/retroarch/audio/volume_range.h");

fn defined_string(name: &str) -> &'static str {
    let needle = format!("#define {name}");
    RANGE_HEADER
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix(&needle)?.trim();
            let rest = rest.strip_prefix('"')?;
            rest.strip_suffix('"')
        })
        .unwrap_or_else(|| panic!("{name} is not a string in volume_range.h"))
}

fn defined_float(name: &str) -> f32 {
    let needle = format!("#define {name} ");
    let raw = RANGE_HEADER
        .lines()
        .find_map(|line| line.trim().strip_prefix(&needle))
        .unwrap_or_else(|| panic!("{name} is not defined in volume_range.h"));
    let number: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
        .collect();
    number
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a number in volume_range.h"))
}

/// The cue we play for a change of volume in a game with no menu sound pack,
/// by the name it has beside the menu.
pub fn tick_file() -> &'static str {
    defined_string("RIB_VOLUME_TICK_FILE")
}

pub fn min_db() -> f32 {
    defined_float("AUDIO_VOLUME_MIN_DB")
}

pub fn max_db() -> f32 {
    defined_float("AUDIO_VOLUME_MAX_DB")
}

pub fn default_db() -> f32 {
    defined_float("AUDIO_VOLUME_DEFAULT_DB")
}

pub fn position_count() -> i32 {
    defined_float("RIB_VOLUME_POSITIONS") as i32
}

/// A position is 0 at the quiet end and `position_count() - 1` at normal.
/// Normal is as loud as the control goes.
pub fn db_for_position(position: i32) -> f32 {
    let last = position_count() - 1;
    let position = position.clamp(0, last);
    if last <= 0 {
        return max_db();
    }
    min_db() + (max_db() - min_db()) * (position as f32) / (last as f32)
}

pub fn position_for_db(decibels: f32) -> i32 {
    let last = position_count() - 1;
    let decibels = decibels.clamp(min_db(), max_db());
    let span = max_db() - min_db();
    if last <= 0 || span == 0.0 {
        return last.max(0);
    }
    ((decibels - min_db()) / span * last as f32).round() as i32
}

/// The saved volume of a game. Without the file the volume is normal, the top.
/// The decibels are on the RetroArch scale, and we show the player a position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub decibels: f32,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            decibels: default_db(),
        }
    }
}

impl Level {
    pub fn clamp(self) -> Self {
        Self {
            decibels: self.decibels.clamp(min_db(), max_db()),
        }
    }
}

pub fn read(data_dir: &Path) -> Level {
    player_settings::volume()
        .chosen(data_dir)
        .map(|decibels| Level { decibels })
        .unwrap_or_default()
}

pub fn write(data_dir: &Path, level: Level) -> Result<(), String> {
    player_settings::volume().choose(data_dir, level.clamp().decibels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_is_normal_and_there_are_a_few_positions() {
        assert_eq!(min_db(), -80.0);
        assert_eq!(max_db(), 0.0);
        assert_eq!(default_db(), max_db());
        assert!(position_count() > 1);
        assert_eq!(position_count(), 10, "ten positions, the top one normal");
        assert_eq!(db_for_position(0), min_db());
        assert_eq!(db_for_position(position_count() - 1), max_db());
        assert_eq!(position_for_db(max_db()), position_count() - 1);
        for position in 0..position_count() {
            let db = db_for_position(position);
            assert_eq!(
                position_for_db(db),
                position,
                "position {position} ({db} dB) has to land back on itself"
            );
        }
        let step = defined_float("AUDIO_VOLUME_STEP_DB");
        assert_eq!(
            step * (position_count() - 1) as f32,
            max_db() - min_db(),
            "the positions have to cover the range in equal steps"
        );
    }

    #[test]
    fn a_missing_file_is_unity_gain() {
        let dir = rominabox_scratch::Scratch::reserve("rominabox-volume-missing");
        assert_eq!(read(&dir), Level::default());
    }

    #[test]
    fn a_level_roundtrips_and_a_value_past_the_ends_is_pulled_back() {
        let dir = rominabox_scratch::Scratch::dir("rominabox-volume-roundtrip");
        write(&dir, Level { decibels: -40.0 }).unwrap();
        assert_eq!(read(&dir), Level { decibels: -40.0 });
        write(&dir, Level { decibels: 40.0 }).unwrap();
        assert_eq!(read(&dir).decibels, max_db());
        write(&dir, Level { decibels: -200.0 }).unwrap();
        assert_eq!(read(&dir).decibels, min_db());
    }
}
