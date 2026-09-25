//! Settings the player changes in the game's own Options.
//!
//! We declare each setting once here, with its word, the RetroArch key it controls and the
//! export's default. Its words are in the player program's `words.inc`, so every design words
//! them like its other text. When we compose a menu, we draw every declared setting in the
//! design's Options (`menu::document`) and list it for the player program in `design.cfg`. In
//! the menu we apply a change at once and write the setting's file in the game's data. At the
//! next launch, we apply that file from the list in the launch plan
//! (`launcher/player_settings.c`).
//!
//! The export's default is only a default. We never write it into the player's file, so a new
//! default in a later export does not replace a choice the player already made, and applies
//! only to a player who has not.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

/// The player program's list of settable keys and of the kinds that show one.
const SOURCE: &str =
    include_str!("../../../vendor/retroarch/menu/drivers/rmlui/settings.inc");

/// Every key and kind declared in the player, with its macro, name and word.
fn declarations() -> &'static [(&'static str, String, String)] {
    static DECLARED: OnceLock<Vec<(&'static str, String, String)>> = OnceLock::new();
    DECLARED.get_or_init(|| {
        ["RIB_SETTING_KEY", "RIB_SETTING_KIND"]
            .into_iter()
            .flat_map(|macro_name| {
                crate::menu::inc::declarations(SOURCE, macro_name).map(move |fields| {
                    match &fields[..] {
                        [name, word] => (macro_name, name.clone(), word.clone()),
                        _ => panic!("settings.inc: {macro_name}({}) is not (name, \"word\")", fields.join(", ")),
                    }
                })
            })
            .collect()
    })
}

/// The word in a `macro_name(name, ...)` line of the player program. An
/// undeclared word would have no effect, so we refuse to export with one.
fn player_word(macro_name: &str, name: &str) -> &'static str {
    declarations()
        .iter()
        .find(|(declared, declared_name, _)| *declared == macro_name && declared_name == name)
        .map(|(_, _, word)| word.as_str())
        .unwrap_or_else(|| {
            panic!("settings.inc declares no {macro_name}({name}, ...), so the player could not apply it")
        })
}

/// The RetroArch settings that a player setting can control. We apply each
/// of them while the game runs, and declare each one once, by the variant's
/// name, in the player program's `settings.inc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    AudioVolume,
    PauseNonactive,
}

impl Key {
    /// Its RetroArch config key, as in the player program's declaration.
    pub fn name(self) -> &'static str {
        player_word("RIB_SETTING_KEY", &format!("{self:?}"))
    }
}

/// How we show a setting, and its values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A slider of `positions` evenly spaced values, `low` to `high` in the
    /// key's own units, both ends included. Its ends are the words
    /// `level-low` and `level-high`.
    Level { low: f32, high: f32, positions: i32 },
    /// On or off, which we show with the words `switch-on` and `switch-off`.
    /// `inverted` when on is the key's `false`.
    Switch { inverted: bool },
}

impl Kind {
    /// This kind's word in design.cfg, from the player program's declaration.
    pub fn word(&self) -> &'static str {
        player_word(
            "RIB_SETTING_KIND",
            match self {
                Kind::Level { .. } => "Level",
                Kind::Switch { .. } => "Switch",
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSetting {
    pub id: &'static str,
    /// The word that names it, by its id in `words.inc`.
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
            label: "volume",
            key: Key::AudioVolume,
            kind: Kind::Level {
                low: crate::volume::min_db(),
                high: crate::volume::max_db(),
                positions: crate::volume::position_count(),
            },
            default: crate::volume::default_db(),
        },
        PlayerSetting {
            id: "background-play",
            label: "play-in-background",
            key: Key::PauseNonactive,
            kind: Kind::Switch { inverted: true },
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
    fn every_key_and_kind_is_the_players_own() {
        for setting in declared(Defaults::default()) {
            assert!(!setting.key.name().is_empty() && !setting.kind.word().is_empty());
        }
        assert_eq!(Key::AudioVolume.name(), "audio_volume");
        assert_eq!(Key::PauseNonactive.name(), "pause_nonactive");
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
