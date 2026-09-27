//! The choices we make in the builder for a game when its author does not:
//! the settings for a dropped game.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// The settings a dropped game starts with, before its author changes any.
///
/// We declare them once, in `desktop/defaults.json`. We make the builder's
/// first draft from that file, and for a setting an export request leaves
/// out, we read the value from the same file here (`unstated`).
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Defaults {
    /// Look the game up online: its catalogue entry and its cover. Without
    /// it, we use only what we cached in an earlier lookup. This is "Look up
    /// game details" in the builder.
    pub online: bool,
    pub show_menu: bool,
    pub start_at_menu: bool,
    pub include_achievements: bool,
    pub splash: bool,
    pub keep_playing_in_background: bool,
    pub autosave_on_quit: bool,
    pub advanced_emulator_access: bool,
    pub theme: String,
    pub palette: String,
    pub menu_sounds: String,
}

pub fn defaults() -> &'static Defaults {
    static DECLARED: OnceLock<Defaults> = OnceLock::new();
    DECLARED.get_or_init(|| {
        serde_json::from_str(include_str!("../../defaults.json"))
            .expect("desktop/defaults.json declares each default once and nothing else")
    })
}

/// Each default as a function, because serde's `default = "…"` names a
/// function rather than a value.
pub mod unstated {
    use super::defaults;

    pub fn online() -> bool {
        defaults().online
    }
    pub fn show_menu() -> bool {
        defaults().show_menu
    }
    pub fn start_at_menu() -> bool {
        defaults().start_at_menu
    }
    pub fn include_achievements() -> bool {
        defaults().include_achievements
    }
    pub fn splash() -> bool {
        defaults().splash
    }
    pub fn keep_playing_in_background() -> bool {
        defaults().keep_playing_in_background
    }
    pub fn autosave_on_quit() -> bool {
        defaults().autosave_on_quit
    }
    pub fn advanced_emulator_access() -> bool {
        defaults().advanced_emulator_access
    }
    pub fn theme() -> String {
        defaults().theme.clone()
    }
    pub fn palette() -> String {
        defaults().palette.clone()
    }
    pub fn menu_sounds() -> String {
        defaults().menu_sounds.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declaration names a design, a palette and a sound pack that exist.
    #[test]
    fn the_defaults_name_what_the_registry_declares() {
        let declared = defaults();
        let registry = crate::themes::registry().unwrap();
        assert!(
            registry.designs.iter().any(|design| design.id == declared.theme),
            "{}",
            declared.theme
        );
        assert!(
            registry.palettes.iter().any(|palette| palette.id == declared.palette),
            "{}",
            declared.palette
        );
        assert!(
            registry
                .sound_packs
                .iter()
                .any(|pack| pack.id == declared.menu_sounds),
            "{}",
            declared.menu_sounds
        );
    }
}
