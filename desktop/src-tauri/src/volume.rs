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

pub fn toggle_id() -> &'static str {
    defined_string("RIB_VOLUME_TOGGLE_ID")
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

/// The saved volume of a game. Without the file we use the RetroArch default,
/// unity gain and not muted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub decibels: f32,
    pub muted: bool,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            decibels: default_db(),
            muted: false,
        }
    }
}

impl Level {
    pub fn clamp(self) -> Self {
        Self {
            decibels: self.decibels.clamp(min_db(), max_db()),
            muted: self.muted,
        }
    }
}

fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(file_name())
}

/// `key = "value"` lines, the config format that RetroArch already reads.
fn parse(text: &str) -> Level {
    let mut level = Level::default();
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
            level.muted = value == "true";
        }
    }
    level.clamp()
}

fn render(level: Level) -> String {
    let level = level.clamp();
    format!(
        "{} = \"{:.1}\"\n{} = \"{}\"\n",
        volume_key(),
        level.decibels,
        mute_key(),
        if level.muted { "true" } else { "false" }
    )
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
    fn the_range_is_retroarchs_decibel_scale() {
        assert_eq!(min_db(), -80.0);
        assert_eq!(max_db(), 12.0);
        assert_eq!(default_db(), 0.0);
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
        write(
            &dir,
            Level {
                decibels: -6.0,
                muted: true,
            },
        )
        .unwrap();
        assert_eq!(
            read(&dir),
            Level {
                decibels: -6.0,
                muted: true
            }
        );
        write(
            &dir,
            Level {
                decibels: 40.0,
                muted: false,
            },
        )
        .unwrap();
        assert_eq!(read(&dir).decibels, max_db());
        write(
            &dir,
            Level {
                decibels: -200.0,
                muted: false,
            },
        )
        .unwrap();
        assert_eq!(read(&dir).decibels, min_db());
    }
}
