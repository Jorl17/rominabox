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

/// The player program's list of settable keys and of the kinds that show one.
const SOURCE: &str = include_str!("../../../vendor/retroarch/menu/drivers/rmlui/settings.inc");

/// The word in a `macro_name(name, ...)` line of the player program. An
/// undeclared word would have no effect, so we refuse to export with one.
fn player_word(macro_name: &str, name: &str) -> &'static str {
    crate::menu::inc::find(SOURCE, &[macro_name], name)
        .and_then(|declaration| declaration.field(1))
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
    InputRumbleEnable,
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
    /// A slider with one of `values` at each position, in the key's own
    /// units, from its low end to its high end. In the menu we find the
    /// position for a value, and label its ends `level-low` and `level-high`.
    Level { values: &'static [f32] },
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
            Kind::Level { values } => {
                let lowest = values.iter().copied().fold(f32::INFINITY, f32::min);
                let highest = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                text.parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| value.clamp(lowest, highest))
            }
            Kind::Switch { .. } => match text {
                "true" => Some(1.0),
                "false" => Some(0.0),
                _ => None,
            },
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
            "{}\t{}\t{}\t{}\n",
            crate::launch_contract::plan_field!(PlayerSetting),
            self.file(),
            self.key.name(),
            self.text(self.default)
        )
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
                values: crate::volume::levels(),
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
        // Every game has it. We disable it in a game whose core has no rumble,
        // and whether to show it then is up to the design.
        PlayerSetting {
            id: "rumble",
            label: "rumble",
            key: Key::InputRumbleEnable,
            kind: Kind::Switch { inverted: false },
            default: 1.0,
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
        assert_eq!(Key::InputRumbleEnable.name(), "input_rumble_enable");
    }

    /// What each joypad driver receives for a rumble on the first pad, with
    /// RetroArch's rumble switch at `switch`. We compile
    /// `scripts/native_runtime/rumble_gate.c` against the fork's own input
    /// layer and run it to find out.
    fn rumble_reaching_pads(switch: &str) -> Vec<String> {
        crate::retroarch_probe::Probe::build(
            "rumble_gate",
            &[
                "input/input_driver.c",
                "input/input_keymaps.c",
                "libretro-common/compat/compat_strl.c",
                "libretro-common/string/stdstring.c",
                "libretro-common/encodings/encoding_utf.c",
                "libretro-common/file/file_path.c",
            ],
        )
        .lines(&[switch])
    }

    // Off means off for any installed joypad drivers. We test three stand-ins:
    // the primary, the secondary paired with it in a build with MFi, and one
    // that scales the strength itself, to which RetroArch's rumble gain would
    // otherwise give the whole strength. We do not run the actual drivers (the
    // Mac's HID, XInput, DirectInput and so the sandbox's relay) here. In the
    // code, every call reaches them through the function linked here.
    #[test]
    fn rumble_switched_off_reaches_every_pad_as_none() {
        let expected = |strength: u32| {
            ["primary", "secondary", "scaling"]
                .map(|driver| format!("{driver} {strength}"))
                .to_vec()
        };
        assert_eq!(rumble_reaching_pads("on"), expected(30000));
        assert_eq!(rumble_reaching_pads("off"), expected(0));
    }

    #[test]
    fn keeping_the_game_running_is_retroarchs_pause_nonactive_turned_off() {
        let on = background(true);
        assert_eq!(on.text(on.default), "false");
        let off = background(false);
        assert_eq!(off.text(off.default), "true");
    }

    #[test]
    fn a_choice_is_read_and_an_absent_file_is_no_choice() {
        let dir = rominabox_scratch::Scratch::dir("rominabox-player-setting-roundtrip");
        let setting = background(false);
        assert_eq!(setting.chosen(&dir), None);
        // The line we write from the menu when the player chooses.
        fs::write(
            dir.join("background-play.cfg"),
            "pause_nonactive = \"false\"\n",
        )
        .unwrap();
        assert_eq!(setting.chosen(&dir), Some(0.0));
        let level = volume();
        fs::write(dir.join("volume.cfg"), "audio_volume = \"-200.0\"\n").unwrap();
        assert_eq!(level.chosen(&dir), Some(crate::volume::levels()[0]));
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
