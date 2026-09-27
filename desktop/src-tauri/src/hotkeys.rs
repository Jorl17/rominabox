//! The RetroArch hotkeys that we write in an exported game.

/// RetroArch meta-bind policy for exported games.
///
/// This is the only bind list, and it covers every `DECLARE_META_BIND` in
/// the pinned RetroArch `configuration.c`, which we compile and read in its
/// test (`scripts/native_runtime/meta_binds.c`). The desktop defaults in
/// `config.def.keybinds.h` and `retroarch.cfg` bind Space to
/// `toggle_fast_forward`, Escape to quit and F1 to the stock menu, so we
/// write every meta bind in an export to keep those defaults out.
///
/// The binds we keep are keyboard-only. Button, axis and mouse variants stay
/// `nul`, which is `NO_BTN`, so the user bind lists no controller button.
/// When the user joykey is `NO_BTN`, an autoconfig bind still applies in the
/// joypad poll, so a profile that contains `input_menu_toggle_btn` would
/// bind that physical button. We remove those meta lines from shipped
/// profiles at staging. `input_player1_*` gameplay keys are not declared
/// here. They come from the player's controls file, which we merge in the launcher.
///
/// We reserve only Escape in a game, for the menu, which has Quit. Quit and
/// fullscreen have no key in any mode, so `q` and `f` stay gameplay keys.
/// The fork's Alt+Enter is the fullscreen chord.
///
/// `advanced_key` is a second keyboard tier. We write it only when the
/// author set `advancedEmulatorAccess`, and it never replaces the button,
/// axis or mouse `nul`. In a normal export we write `keyboard`, which is
/// `nul` for every bind except the menu toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyBind {
    pub name: &'static str,
    pub keyboard: HotkeyKeyboard,
    /// The keyboard key we use only when advanced emulator access is on.
    /// `None` means that this bind has no advanced key.
    pub advanced_key: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyKeyboard {
    Neutral,
    MenuToggle,
}

pub const HOTKEY_BINDS: &[HotkeyBind] = &[
    HotkeyBind {
        name: "enable_hotkey",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "menu_toggle",
        keyboard: HotkeyKeyboard::MenuToggle,
        advanced_key: None,
    },
    HotkeyBind {
        name: "exit_emulator",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "close_content",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "reset",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_fast_forward",
        keyboard: HotkeyKeyboard::Neutral,
        // This is the desktop default, and we declare no gameplay key on Space.
        advanced_key: Some("space"),
    },
    HotkeyBind {
        name: "hold_fast_forward",
        keyboard: HotkeyKeyboard::Neutral,
        // `l` is the desktop default and the DualShock right-stick-right key.
        advanced_key: Some("l"),
    },
    HotkeyBind {
        name: "toggle_slowmotion",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "hold_slowmotion",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "rewind",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "pause_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "frame_advance",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "audio_mute",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "volume_up",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "volume_down",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "load_state",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "save_state",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "state_slot_increase",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "state_slot_decrease",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "play_replay",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "record_replay",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "halt_replay",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "save_replay_checkpoint",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "prev_replay_checkpoint",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "next_replay_checkpoint",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "replay_slot_increase",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "replay_slot_decrease",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_eject_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_next",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_prev",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_hold",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_next",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_prev",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_index_plus",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_index_minus",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "screenshot",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "recording_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "streaming_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "turbo_fire_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "grab_mouse_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "game_focus_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_fullscreen",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "desktop_menu_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_vrr_runloop",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "runahead_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "preempt_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "video_filter_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "fps_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_statistics",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "ai_service",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_ping_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_host_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_game_watch",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_player_chat",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_fade_chat_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "overlay_next",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
    HotkeyBind {
        name: "osk_toggle",
        keyboard: HotkeyKeyboard::Neutral,
        advanced_key: None,
    },
];

impl HotkeyBind {
    pub fn keyboard_value(self, show_menu: bool, advanced: bool) -> &'static str {
        if advanced {
            if let Some(key) = self.advanced_key {
                return key;
            }
        }
        match self.keyboard {
            HotkeyKeyboard::Neutral => "nul",
            HotkeyKeyboard::MenuToggle if show_menu => "escape",
            HotkeyKeyboard::MenuToggle => "nul",
        }
    }
}

/// Render the exported hotkey policy. Callers must not keep a second list.
///
/// With `advanced` we write `advanced_key` for the binds that have one, the
/// two fast-forward keys. We do not change button, axis or
/// mouse. When it is false, those keys stay `nul`, and we still write Escape
/// for the menu toggle when the menu is on.
pub fn isolated_hotkey_config(show_menu: bool, advanced: bool) -> String {
    let mut config = String::new();
    for bind in HOTKEY_BINDS {
        let key = bind.keyboard_value(show_menu, advanced);
        config.push_str(&format!("input_{} = \"{key}\"\n", bind.name));
        config.push_str(&format!("input_{}_btn = \"nul\"\n", bind.name));
        config.push_str(&format!("input_{}_axis = \"nul\"\n", bind.name));
        config.push_str(&format!("input_{}_mbtn = \"nul\"\n", bind.name));
    }
    let menu_combo = if show_menu { "2" } else { "0" };
    config.push_str(&format!(
        "input_menu_toggle_gamepad_combo = \"{menu_combo}\"\n"
    ));
    config.push_str("input_quit_gamepad_combo = \"0\"\n");
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retroarch_probe::Probe;

    /// Every meta bind in the table of the RetroArch fork, in order, with its
    /// default key. We get them by compiling and running
    /// `scripts/native_runtime/meta_binds.c` against configuration.c,
    /// config.def.keybinds.h and the key names of the fork.
    fn retroarch_meta_binds() -> Vec<(String, String)> {
        Probe::build(
            "meta_binds",
            &[
                "configuration.c",
                "input/input_keymaps.c",
                "input/input_driver.c",
                "libretro-common/compat/compat_strl.c",
                "libretro-common/string/stdstring.c",
                "libretro-common/encodings/encoding_utf.c",
                "libretro-common/file/file_path.c",
            ],
        )
            .lines(&[])
            .iter()
            .map(|line| {
                let (name, key) = line.split_once(' ').expect("a bind and its key");
                (name.to_string(), key.to_string())
            })
            .collect()
    }

    #[test]
    fn hotkey_policy_matches_pinned_retroarch_meta_binds() {
        let declared = retroarch_meta_binds();
        let policy: Vec<&str> = HOTKEY_BINDS.iter().map(|bind| bind.name).collect();
        assert_eq!(
            policy,
            declared.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
            "HOTKEY_BINDS must list every meta bind the fork's RetroArch declares, in its order"
        );
        // We write every one because RetroArch has defaults for these, and a
        // game would get those defaults for any bind we left out.
        let default = |name: &str| {
            declared
                .iter()
                .find(|(declared, _)| declared == name)
                .map(|(_, key)| key.as_str())
        };
        assert_eq!(default("toggle_fast_forward"), Some("space"));
        assert_eq!(default("exit_emulator"), Some("escape"));
        assert_eq!(default("menu_toggle"), Some("f1"));
    }
}
