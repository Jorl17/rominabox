//! The names of the in-game volume control, and the per-game file for its level.
//!
//! The names are in `vendor/retroarch/audio/volume_range.h`. We read that file
//! in the player, the launcher and this command, so a renamed part or a new
//! decibel limit applies to all three at once.

use std::fs;
use std::path::{Path, PathBuf};

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

pub fn slider_id() -> &'static str {
    defined_string("RIB_VOLUME_SLIDER_ID")
}

pub fn down_id() -> &'static str {
    defined_string("RIB_VOLUME_DOWN_ID")
}

pub fn up_id() -> &'static str {
    defined_string("RIB_VOLUME_UP_ID")
}

pub fn low_id() -> &'static str {
    defined_string("RIB_VOLUME_LOW_ID")
}

pub fn high_id() -> &'static str {
    defined_string("RIB_VOLUME_HIGH_ID")
}

pub fn file_name() -> &'static str {
    defined_string("RIB_VOLUME_FILE")
}

pub fn volume_key() -> &'static str {
    defined_string("RIB_VOLUME_KEY")
}

pub fn mute_key() -> &'static str {
    defined_string("RIB_VOLUME_MUTE_KEY")
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

fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(file_name())
}

/// `key = "value"` lines, the config format that RetroArch already reads.
fn parse(text: &str) -> Level {
    let mut level = Level::default();
    let mut muted = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        if key == volume_key() {
            if let Ok(decibels) = value.parse() {
                level.decibels = decibels;
            }
        } else if key == mute_key() {
            // An older file format stored muting as a button. Map it to the quiet end.
            muted = value == "true";
        }
    }
    if muted {
        level.decibels = min_db();
    }
    level.clamp()
}

fn render(level: Level) -> String {
    let level = level.clamp();
    format!("{} = \"{:.1}\"\n", volume_key(), level.decibels)
}

pub fn read(data_dir: &Path) -> Level {
    fs::read_to_string(path_in(data_dir))
        .map(|text| parse(&text))
        .unwrap_or_default()
}

pub fn write(data_dir: &Path, level: Level) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let destination = path_in(data_dir);
    let temporary = destination.with_extension("cfg.tmp");
    fs::write(&temporary, render(level)).map_err(|e| e.to_string())?;
    fs::rename(&temporary, &destination).map_err(|e| e.to_string())?;
    Ok(())
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
        assert!(position_count() < 10, "ten positions is already too many");
        assert_eq!(db_for_position(0), min_db());
        assert_eq!(db_for_position(position_count() - 1), max_db());
        assert_eq!(position_for_db(max_db()), position_count() - 1);
        assert_eq!(db_for_position(position_for_db(-40.0)), -40.0);
        let step = defined_float("AUDIO_VOLUME_STEP_DB");
        assert_eq!(
            step * (position_count() - 1) as f32,
            max_db() - min_db(),
            "the positions have to cover the range in equal steps"
        );
    }

    #[test]
    fn a_missing_file_is_unity_gain() {
        let dir = std::env::temp_dir().join("rominabox-volume-missing");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(read(&dir), Level::default());
    }

    #[test]
    fn a_level_roundtrips_and_a_value_past_the_ends_is_pulled_back() {
        let dir = std::env::temp_dir().join("rominabox-volume-roundtrip");
        let _ = fs::remove_dir_all(&dir);
        write(&dir, Level { decibels: -40.0 }).unwrap();
        assert_eq!(read(&dir), Level { decibels: -40.0 });
        let written = fs::read_to_string(path_in(&dir)).unwrap();
        assert!(
            !written.contains(mute_key()),
            "the file must not bring mute back, got {written}"
        );
        write(&dir, Level { decibels: 40.0 }).unwrap();
        assert_eq!(read(&dir).decibels, max_db());
        write(&dir, Level { decibels: -200.0 }).unwrap();
        assert_eq!(read(&dir).decibels, min_db());
    }

    #[test]
    fn an_old_mute_is_the_quiet_end() {
        let dir = std::env::temp_dir().join("rominabox-volume-old-mute");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            path_in(&dir),
            format!(
                "{} = \"0.0\"\n{} = \"true\"\n",
                volume_key(),
                mute_key()
            ),
        )
        .unwrap();
        assert_eq!(read(&dir).decibels, min_db());
    }
}
