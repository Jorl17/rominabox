//! The RetroArch meta binds that we write for an exported game. These are
//! the RetroArch hotkeys, and we turn every one of them off.

/// The RetroArch meta binds that we write for exported games.
///
/// This is the only list of binds. It contains every `DECLARE_META_BIND` in
/// the pinned RetroArch `configuration.c`, and in its test we compile that
/// file and read the binds (`scripts/native_runtime/meta_binds.c`). The
/// desktop defaults in `config.def.keybinds.h` and `retroarch.cfg` bind Space
/// to `toggle_fast_forward`, Escape to quit and F1 to the stock menu, so we
/// write every meta bind in an export to keep those defaults out.
///
/// Every bind is `nul` on the keyboard, the buttons, the axes and the mouse.
/// The player opens the menu with HOTKEYS (`crate::hotkeys`), by default
/// Escape, the Home button of a pad and L3+R3, so we turn off the RetroArch
/// menu toggle and its gamepad combo. A `nul` user joykey still falls back to
/// an autoconfig bind. The profiles we ship keep one meta line,
/// `input_menu_toggle_btn`, from which we read the Home button of each pad in
/// the menu, and we clear the RetroArch toggle bit that it sets before we
/// handle input in the menu. The player saves and loads a state and changes
/// its slot with QUICK SAVE, QUICK LOAD, PREVIOUS SLOT and NEXT SLOT in
/// HOTKEYS, on the current slot in the menu, by default with F2, F4, F6 and
/// F7. We do not declare the `input_player1_*` gameplay keys here. They come
/// from the player's controls file, which we merge in the launcher.
///
/// Quit and fullscreen have no key in any mode, so `q` and `f` remain
/// gameplay keys. In the fork, the player presses Alt+Enter for fullscreen.
///
/// `advanced_key` is a second keyboard key. We write it only when the author
/// set `advancedEmulatorAccess`, and we still write `nul` for the button,
/// axis and mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetaBind {
    pub name: &'static str,
    /// The keyboard key we use only when advanced emulator access is on.
    /// `None` means that this bind has no advanced key.
    pub advanced_key: Option<&'static str>,
}

pub const META_BINDS: &[MetaBind] = &[
    MetaBind {
        name: "enable_hotkey",
        advanced_key: None,
    },
    MetaBind {
        name: "menu_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "exit_emulator",
        advanced_key: None,
    },
    MetaBind {
        name: "close_content",
        advanced_key: None,
    },
    MetaBind {
        name: "reset",
        advanced_key: None,
    },
    MetaBind {
        name: "toggle_fast_forward",
        // This is the desktop default, and we declare no gameplay key on Space.
        advanced_key: Some("space"),
    },
    MetaBind {
        name: "hold_fast_forward",
        // `l` is the desktop default and the DualShock right-stick-right key.
        advanced_key: Some("l"),
    },
    MetaBind {
        name: "toggle_slowmotion",
        advanced_key: None,
    },
    MetaBind {
        name: "hold_slowmotion",
        advanced_key: None,
    },
    MetaBind {
        name: "rewind",
        advanced_key: None,
    },
    MetaBind {
        name: "pause_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "frame_advance",
        advanced_key: None,
    },
    MetaBind {
        name: "audio_mute",
        advanced_key: None,
    },
    MetaBind {
        name: "volume_up",
        advanced_key: None,
    },
    MetaBind {
        name: "volume_down",
        advanced_key: None,
    },
    MetaBind {
        name: "load_state",
        advanced_key: None,
    },
    MetaBind {
        name: "save_state",
        advanced_key: None,
    },
    MetaBind {
        name: "state_slot_increase",
        advanced_key: None,
    },
    MetaBind {
        name: "state_slot_decrease",
        advanced_key: None,
    },
    MetaBind {
        name: "play_replay",
        advanced_key: None,
    },
    MetaBind {
        name: "record_replay",
        advanced_key: None,
    },
    MetaBind {
        name: "halt_replay",
        advanced_key: None,
    },
    MetaBind {
        name: "save_replay_checkpoint",
        advanced_key: None,
    },
    MetaBind {
        name: "prev_replay_checkpoint",
        advanced_key: None,
    },
    MetaBind {
        name: "next_replay_checkpoint",
        advanced_key: None,
    },
    MetaBind {
        name: "replay_slot_increase",
        advanced_key: None,
    },
    MetaBind {
        name: "replay_slot_decrease",
        advanced_key: None,
    },
    MetaBind {
        name: "disk_eject_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "disk_next",
        advanced_key: None,
    },
    MetaBind {
        name: "disk_prev",
        advanced_key: None,
    },
    MetaBind {
        name: "shader_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "shader_hold",
        advanced_key: None,
    },
    MetaBind {
        name: "shader_next",
        advanced_key: None,
    },
    MetaBind {
        name: "shader_prev",
        advanced_key: None,
    },
    MetaBind {
        name: "cheat_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "cheat_index_plus",
        advanced_key: None,
    },
    MetaBind {
        name: "cheat_index_minus",
        advanced_key: None,
    },
    MetaBind {
        name: "screenshot",
        advanced_key: None,
    },
    MetaBind {
        name: "recording_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "streaming_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "turbo_fire_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "grab_mouse_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "game_focus_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "toggle_fullscreen",
        advanced_key: None,
    },
    MetaBind {
        name: "desktop_menu_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "toggle_vrr_runloop",
        advanced_key: None,
    },
    MetaBind {
        name: "runahead_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "preempt_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "video_filter_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "fps_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "toggle_statistics",
        advanced_key: None,
    },
    MetaBind {
        name: "ai_service",
        advanced_key: None,
    },
    MetaBind {
        name: "netplay_ping_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "netplay_host_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "netplay_game_watch",
        advanced_key: None,
    },
    MetaBind {
        name: "netplay_player_chat",
        advanced_key: None,
    },
    MetaBind {
        name: "netplay_fade_chat_toggle",
        advanced_key: None,
    },
    MetaBind {
        name: "overlay_next",
        advanced_key: None,
    },
    MetaBind {
        name: "osk_toggle",
        advanced_key: None,
    },
];

impl MetaBind {
    pub fn keyboard_value(self, advanced: bool) -> &'static str {
        match self.advanced_key {
            Some(key) if advanced => key,
            _ => "nul",
        }
    }
}

/// Produce the meta binds for an export. Keep no second list of them.
///
/// With `advanced`, we write `advanced_key` for the binds that have one, the
/// two fast-forward keys, and leave the button, axis and mouse unchanged.
/// The RetroArch menu toggle has no key and its gamepad combo is off,
/// because the player opens the menu with the keys set in HOTKEYS.
pub fn isolated_meta_bind_config(advanced: bool) -> String {
    let mut config = String::new();
    for bind in META_BINDS {
        let key = bind.keyboard_value(advanced);
        config.push_str(&format!("input_{} = \"{key}\"\n", bind.name));
        config.push_str(&format!("input_{}_btn = \"nul\"\n", bind.name));
        config.push_str(&format!("input_{}_axis = \"nul\"\n", bind.name));
        config.push_str(&format!("input_{}_mbtn = \"nul\"\n", bind.name));
    }
    config.push_str("input_menu_toggle_gamepad_combo = \"0\"\n");
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
        let policy: Vec<&str> = META_BINDS.iter().map(|bind| bind.name).collect();
        assert_eq!(
            policy,
            declared.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
            "META_BINDS must list every meta bind the fork's RetroArch declares, in its order"
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
