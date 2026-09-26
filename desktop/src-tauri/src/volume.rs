//! The volumes a player can choose, as in the RetroArch header.
//!
//! `vendor/retroarch/audio/volume_range.h` contains the volume at each
//! position of the game's slider, and the limits for the RetroArch hotkeys and
//! settings list. We read that list for the volume player setting
//! (`player_settings`), with which we declare it to the menu, and we find the
//! position for a value only in the menu code.

use std::collections::HashMap;
use std::sync::OnceLock;

const RANGE_HEADER: &str = include_str!("../../../vendor/retroarch/audio/volume_range.h");

/// Every `#define` in the header, by name, with its continued lines joined.
fn definitions() -> &'static HashMap<&'static str, String> {
    static DEFINED: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    DEFINED.get_or_init(|| {
        let mut defined = HashMap::new();
        let mut lines = RANGE_HEADER.lines();
        while let Some(line) = lines.next() {
            let Some(rest) = line.trim().strip_prefix("#define ") else {
                continue;
            };
            let (name, first) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let mut body = first.trim().to_string();
            while let Some(continued) = body.strip_suffix('\\') {
                body = format!("{} {}", continued.trim(), lines.next().unwrap_or("").trim());
            }
            defined.insert(name, body.trim().to_string());
        }
        defined
    })
}

fn defined(name: &str) -> &'static str {
    definitions()
        .get(name)
        .unwrap_or_else(|| panic!("{name} is not defined in volume_range.h"))
}

/// A decibel value as written in C, `(-80.0f)` or `-38.2f`, or the name of
/// another definition that is one.
fn decibels(text: &str) -> f32 {
    let text = text
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    if text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        return decibels(defined(text));
    }
    text.trim_end_matches('f')
        .parse()
        .unwrap_or_else(|_| panic!("{text} is not a number in volume_range.h"))
}

/// The cue we play for a change of volume in a game with no menu sound pack,
/// by the name it has beside the menu.
pub fn tick_file() -> &'static str {
    defined("RIB_VOLUME_TICK_FILE")
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| panic!("RIB_VOLUME_TICK_FILE is not a string in volume_range.h"))
}

/// The volume at each position of the game's slider, silence first and
/// normal last.
pub fn levels() -> &'static [f32] {
    static LEVELS: OnceLock<Vec<f32>> = OnceLock::new();
    LEVELS.get_or_init(|| {
        defined("RIB_VOLUME_LEVELS_DB")
            .split(',')
            .map(decibels)
            .collect()
    })
}

/// RetroArch's volume while the player has chosen none.
pub fn default_db() -> f32 {
    decibels(defined("AUDIO_VOLUME_DEFAULT_DB"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slider_runs_from_silence_to_normal_in_even_steps_to_the_ear() {
        let levels = levels();
        let silence = decibels(defined("AUDIO_VOLUME_MIN_DB"));
        let normal = decibels(defined("AUDIO_VOLUME_MAX_DB"));
        assert_eq!(silence, -80.0);
        assert_eq!(normal, 0.0, "the top is unity gain");
        assert_eq!(levels.len(), 10, "ten positions");
        assert_eq!(levels.first(), Some(&silence));
        assert_eq!(levels.last(), Some(&normal));
        assert_eq!(default_db(), normal, "the default is normal");
        // Above silence the amplitude at position p of 9 is (p/9) squared, so
        // its square root rises by the same amount each step. One decimal of
        // rounding moves it by less than 0.003.
        let last = (levels.len() - 1) as f32;
        for (position, &db) in levels.iter().enumerate().skip(1) {
            let root = 10f32.powf(db / 40.0);
            assert!(
                (root - position as f32 / last).abs() < 0.003,
                "position {position} is {db} dB, not an even step to the ear"
            );
        }
    }
}
