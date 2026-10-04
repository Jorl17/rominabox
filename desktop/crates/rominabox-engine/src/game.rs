//! A game as described in the builder: its files and every choice made for
//! it. An export request, a project and a project's archive each contain one,
//! so we declare each setting once, here, with the default for a request that
//! leaves it out, which is the builder's (`crate::builder::defaults`).

use crate::controls;
use crate::packaging::ExportTarget;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub rom: PathBuf,
    pub title: String,
    pub system: String,
    pub icon: Option<PathBuf>,
    pub background: Option<PathBuf>,
    /// Draw the background picture in the palette's screen colour, so that the
    /// menu's text is readable on any picture. Without it, we show the picture
    /// as the author made it.
    #[serde(default = "crate::builder::unstated::tint_background")]
    pub tint_background: bool,
    #[serde(default = "crate::builder::unstated::show_menu")]
    pub show_menu: bool,
    #[serde(default = "crate::builder::unstated::start_at_menu")]
    pub start_at_menu: bool,
    #[serde(default = "crate::builder::unstated::theme")]
    pub theme: String,
    #[serde(default = "crate::builder::unstated::palette")]
    pub palette: String,
    #[serde(default = "crate::builder::unstated::menu_sounds")]
    pub menu_sounds: String,
    /// The author's defaults, which we never change. We store the player's
    /// changes separately, in the managed data folder of the exported game.
    #[serde(default)]
    pub controls: controls::Controls,
    /// The hotkeys to open the menu, confirm and go back in it, and save, load
    /// and change the slot during play, until the player changes them on
    /// HOTKEYS. For a hotkey left out we use the builder's default. We accept
    /// `menuControls` as an alias for this field in saved projects.
    #[serde(default = "crate::builder::unstated::hotkeys", alias = "menuControls")]
    pub hotkeys: crate::hotkeys::Hotkeys,
    /// The firmware files the author chose. On export we never look for
    /// firmware in global RetroArch locations.
    #[serde(default)]
    pub firmware: Vec<PathBuf>,
    /// Include the short native in-player splash and its logo asset.
    #[serde(default = "crate::builder::unstated::splash")]
    pub splash: bool,
    /// Restore stock RetroArch native menus in the exported app.
    #[serde(default = "crate::builder::unstated::advanced_emulator_access")]
    pub advanced_emulator_access: bool,
    /// Keep emulating when the window does not have the focus. RetroArch's
    /// `pause_nonactive` is the opposite of this. We write it into the frozen
    /// config, as we do quit-autosave, because the player has no control for
    /// it and a per-game `controls.cfg` would otherwise replace it.
    #[serde(default = "crate::builder::unstated::keep_playing_in_background")]
    pub keep_playing_in_background: bool,
    /// Whether the game has fast forward, with its hotkey on HOTKEYS and its
    /// speed in Options. When it is off, the game has neither.
    #[serde(default = "crate::builder::unstated::fast_forward")]
    pub fast_forward: bool,
    /// The fast forward speed, as the RetroArch `fastforward_ratio`, until the
    /// player changes it.
    #[serde(default = "crate::builder::unstated::fast_forward_speed")]
    pub fast_forward_speed: f32,
    /// Whether fast forward runs only while its hotkey is held, or from one
    /// press to the next, until the player changes it.
    #[serde(default = "crate::builder::unstated::fast_forward_hold")]
    pub fast_forward_hold: bool,
    /// Save on quit and load that save the next time the player opens the
    /// game. The author makes one choice for both.
    #[serde(default = "crate::builder::unstated::autosave_on_quit")]
    pub autosave_on_quit: bool,
    /// Every connected pad is player 1 from the moment the game starts, and
    /// can open and use the menu. We send the rumble to the last pad on which
    /// someone pressed a button. When off, each pad is a separate player, for
    /// a game with a second player.
    #[serde(default = "crate::builder::unstated::every_pad_is_player_one")]
    pub every_pad_is_player_one: bool,
    /// The Options entries we offer in this game. When absent, we use the
    /// design's defaults. With an empty list, we show no Options button.
    #[serde(default, deserialize_with = "menu_entries")]
    pub menu_entries: Option<Vec<String>>,
    /// The shader presets we bundle into the game. Usually there are none,
    /// and then the game has no shader screen and no preset.
    #[serde(default)]
    pub shaders: crate::shaders::ShaderSelection,
    /// Include player-authenticated Casual achievements, independently of data.
    #[serde(default = "crate::builder::unstated::include_achievements")]
    pub include_achievements: bool,
    pub target: ExportTarget,
    /// The game for Mac and for Windows, each from its platform's kit, in one
    /// `<title>.zip` containing `Mac/<title>.app` and `Windows/<title>`.
    #[serde(default)]
    pub both_platforms: bool,
    /// A Mac game also runs on Intel Macs. Ignored for a Windows game.
    #[serde(default = "crate::builder::unstated::intel_macs")]
    pub intel_macs: bool,
    /// What the author left out of, and added to, the files that go with
    /// the game: companions, patches and anything else.
    #[serde(default)]
    pub files: crate::content::GameFiles,
}

impl Game {
    /// The hotkeys of the game. Fast forward is one only when fast forward is
    /// on, so we neither check nor write a binding kept for it while it is off.
    pub fn offered_hotkeys(&self) -> crate::hotkeys::Hotkeys {
        match crate::hotkeys::Hotkey::named(crate::hotkeys::FAST_FORWARD) {
            Some(fast_forward) if !self.fast_forward => self.hotkeys.without(fast_forward),
            _ => self.hotkeys.clone(),
        }
    }

    /// The game of `rom`, with the builder's default for every setting,
    /// as for a request with only these, from the same declaration.
    pub fn new(rom: impl Into<PathBuf>, title: &str, system: &str, target: ExportTarget) -> Game {
        serde_json::from_value(serde_json::json!({
            "rom": rom.into(),
            "title": title,
            "system": system,
            "target": target,
        }))
        .expect("a game states its rom, title, system and target; the rest is declared")
    }
}

/// Options entries by their screens' ids. We accept `menu-controls` as an
/// alias for the HOTKEYS entry in saved projects.
fn menu_entries<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<String>>, D::Error> {
    const RENAMED: [(&str, &str); 1] = [("menu-controls", "hotkeys")];
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.map(|entries| {
        entries
            .into_iter()
            .map(|entry| {
                RENAMED
                    .iter()
                    .find(|(old, _)| *old == entry)
                    .map_or(entry.clone(), |(_, new)| new.to_string())
            })
            .collect()
    }))
}
