//! Settings that the player changes in the Options of the game.
//!
//! We declare each one once here, with what the player sees, the RetroArch
//! key it controls and the default for an export. We draw every declared
//! setting in the Options of every design (`menu::document`) and list it in
//! `design.cfg` for the player program. In the menu, a change applies at once
//! and we write the file of the setting in the game's data folder. At the next
//! launch we apply that file (`launcher/player_settings.c`), from the list in
//! the launch plan.
//!
//! The export default is only a default. We never write it into the file of
//! the player, so an export with another default does not replace a choice
//! the player made, and the default applies only until the player chooses.

use std::fs;
use std::path::Path;

/// The RetroArch settings that a player setting can control. We apply each
/// of these in the player while the game runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    AudioVolume,
    PauseNonactive,
}

impl Key {
    pub fn name(self) -> &'static str {
        match self {
            Key::AudioVolume => "audio_volume",
            Key::PauseNonactive => "pause_nonactive",
        }
    }
}

/// How we show a setting, and its values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A slider of `positions` evenly spaced values, `low` to `high` in the
    /// key's own units, both ends included, with a word at each end.
    Level {
        low: f32,
        high: f32,
        positions: i32,
        low_word: &'static str,
        high_word: &'static str,
    },
    /// On or off. `inverted` when on is the key's `false`.
    Switch {
        on: &'static str,
        off: &'static str,
        inverted: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSetting {
    pub id: &'static str,
    pub label: &'static str,
    pub key: Key,
    pub kind: Kind,
    /// The key's value when the player has not chosen one.
    pub default: f32,
}

impl PlayerSetting {
    /// The setting's file in the game's data, which contains the one line for
    /// its RetroArch key.
    pub fn file(&self) -> String {
        format!("{}.cfg", self.id)
    }

    /// The element the player uses: a level's slider, a switch's button.
    pub fn control(&self) -> String {
        match self.kind {
            Kind::Level { .. } => format!("{}-level", self.id),
            Kind::Switch { .. } => self.id.to_string(),
        }
    }

    /// A value in RetroArch's text form for this key.
    pub fn text(&self, value: f32) -> String {
        match self.kind {
            Kind::Level { .. } => format!("{value:.1}"),
            Kind::Switch { .. } => (value != 0.0).to_string(),
        }
    }

    /// The value in RetroArch's text, or `None` for text that is not one of
    /// this setting's values.
    pub fn parse(&self, text: &str) -> Option<f32> {
        match self.kind {
            Kind::Level { low, high, .. } => text
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| value.clamp(low.min(high), low.max(high))),
            Kind::Switch { .. } => match text {
                "true" => Some(1.0),
                "false" => Some(0.0),
                _ => None,
            },
        }
    }

    /// Whether we show the value of a switch as on.
    pub fn is_on(&self, value: f32) -> bool {
        match self.kind {
            Kind::Switch { inverted, .. } => (value != 0.0) != inverted,
            Kind::Level { .. } => false,
        }
    }

    /// The value the player has chosen, from their file in `data_dir`, or
    /// `None` when they have not chosen one.
    pub fn chosen(&self, data_dir: &Path) -> Option<f32> {
        let text = fs::read_to_string(data_dir.join(self.file())).ok()?;
        text.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != self.key.name() {
                return None;
            }
            self.parse(value.trim().trim_matches('"'))
        })
    }

    /// The line for this setting in the launch plan: the file from which we
    /// read the player's choice at launch, the key, and the export's default,
    /// which we use while the player has chosen nothing.
    pub fn launch_line(&self) -> String {
        format!(
            "player_setting\t{}\t{}\t{}\n",
            self.file(),
            self.key.name(),
            self.text(self.default)
        )
    }

    /// Write the player's choice, replacing the file in one step.
    pub fn choose(&self, data_dir: &Path, value: f32) -> Result<(), String> {
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let destination = data_dir.join(self.file());
        let temporary = destination.with_extension("cfg.tmp");
        let line = format!("{} = \"{}\"\n", self.key.name(), self.text(value));
        fs::write(&temporary, line).map_err(|e| e.to_string())?;
        fs::rename(&temporary, &destination).map_err(|e| e.to_string())
    }
}

/// The author's choices for the player's settings, which are only defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Defaults {
    /// The author's choice for keeping the game running while its window is
    /// in the background. RetroArch's `pause_nonactive` is its opposite.
    pub keep_playing_in_background: bool,
}

/// Every setting the player of a game can change, in the order of the
/// Options screen.
pub fn declared(defaults: Defaults) -> Vec<PlayerSetting> {
    vec![
        PlayerSetting {
            id: "volume",
            label: "VOLUME",
            key: Key::AudioVolume,
            kind: Kind::Level {
                low: crate::volume::min_db(),
                high: crate::volume::max_db(),
                positions: crate::volume::position_count(),
                low_word: "LOW",
                high_word: "HIGH",
            },
            default: crate::volume::default_db(),
        },
        PlayerSetting {
            id: "background-play",
            label: "PLAY IN BACKGROUND",
            key: Key::PauseNonactive,
            kind: Kind::Switch {
                on: "ON",
                off: "OFF",
                inverted: true,
            },
            default: if defaults.keep_playing_in_background {
                0.0
            } else {
                1.0
            },
        },
    ]
}

/// The volume setting, whatever defaults the author chose.
pub fn volume() -> PlayerSetting {
    declared(Defaults::default())
        .into_iter()
        .find(|setting| setting.key == Key::AudioVolume)
        .expect("volume is declared")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn background(keep_playing: bool) -> PlayerSetting {
        declared(Defaults {
            keep_playing_in_background: keep_playing,
        })
        .into_iter()
        .find(|setting| setting.key == Key::PauseNonactive)
        .unwrap()
    }

    #[test]
    fn keeping_the_game_running_is_retroarchs_pause_nonactive_turned_off() {
        let on = background(true);
        assert_eq!(on.text(on.default), "false");
        assert!(on.is_on(on.default), "the author's on is shown as on");
        let off = background(false);
        assert_eq!(off.text(off.default), "true");
        assert!(!off.is_on(off.default));
    }

    #[test]
    fn a_choice_roundtrips_and_an_absent_file_is_no_choice() {
        let dir = rominabox_scratch::Scratch::dir("rominabox-player-setting-roundtrip");
        let setting = background(false);
        assert_eq!(setting.chosen(&dir), None);
        setting.choose(&dir, 0.0).unwrap();
        assert_eq!(setting.chosen(&dir), Some(0.0));
        assert_eq!(
            fs::read_to_string(dir.join("background-play.cfg")).unwrap(),
            "pause_nonactive = \"false\"\n"
        );
        let level = volume();
        level.choose(&dir, -200.0).unwrap();
        assert_eq!(level.chosen(&dir), Some(crate::volume::min_db()));
    }

    #[test]
    fn text_that_is_not_a_value_is_no_choice() {
        let dir = rominabox_scratch::Scratch::dir("rominabox-player-setting-garbage");
        let setting = background(true);
        fs::write(dir.join(setting.file()), "pause_nonactive = \"maybe\"\n").unwrap();
        assert_eq!(setting.chosen(&dir), None);
        fs::write(dir.join(setting.file()), "audio_volume = \"true\"\n").unwrap();
        assert_eq!(setting.chosen(&dir), None, "only its own key counts");
    }
}
