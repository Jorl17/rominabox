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
/// Every bind is `nul` on the keyboard, the buttons, the axes and the mouse.
/// The player opens the menu with the inputs on MENU CONTROLS
/// (`crate::menu_controls`), by default Escape, the pad's Home and L3+R3. We
/// keep RetroArch's own menu toggle and its gamepad combo off, so that they
/// cannot open the menu. With a `nul` user joykey, an autoconfig bind still
/// applies. Shipped profiles contain one meta line, `input_menu_toggle_btn`,
/// from which we find each pad's Home in the menu, and we drop the RetroArch
/// toggle bit from it before we handle input in the menu. `input_player1_*`
/// gameplay keys are not declared here. They come from the player's controls
/// file, which we merge in the launcher.
///
/// Quit and fullscreen have no key in any mode, so `q` and `f` stay gameplay
/// keys. The fork's Alt+Enter is the fullscreen chord.
///
/// `advanced_key` is a second keyboard tier. We write it only when the
/// author set `advancedEmulatorAccess`, and it never replaces the button,
/// axis or mouse `nul`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyBind {
    pub name: &'static str,
    /// The keyboard key we use only when advanced emulator access is on.
    /// `None` means that this bind has no advanced key.
    pub advanced_key: Option<&'static str>,
}

pub const HOTKEY_BINDS: &[HotkeyBind] = &[
    HotkeyBind {
        name: "enable_hotkey",
        advanced_key: None,
    },
    HotkeyBind {
        name: "menu_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "exit_emulator",
        advanced_key: None,
    },
    HotkeyBind {
        name: "close_content",
        advanced_key: None,
    },
    HotkeyBind {
        name: "reset",
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_fast_forward",
        // This is the desktop default, and we declare no gameplay key on Space.
        advanced_key: Some("space"),
    },
    HotkeyBind {
        name: "hold_fast_forward",
        // `l` is the desktop default and the DualShock right-stick-right key.
        advanced_key: Some("l"),
    },
    HotkeyBind {
        name: "toggle_slowmotion",
        advanced_key: None,
    },
    HotkeyBind {
        name: "hold_slowmotion",
        advanced_key: None,
    },
    HotkeyBind {
        name: "rewind",
        advanced_key: None,
    },
    HotkeyBind {
        name: "pause_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "frame_advance",
        advanced_key: None,
    },
    HotkeyBind {
        name: "audio_mute",
        advanced_key: None,
    },
    HotkeyBind {
        name: "volume_up",
        advanced_key: None,
    },
    HotkeyBind {
        name: "volume_down",
        advanced_key: None,
    },
    HotkeyBind {
        name: "load_state",
        advanced_key: None,
    },
    HotkeyBind {
        name: "save_state",
        advanced_key: None,
    },
    HotkeyBind {
        name: "state_slot_increase",
        advanced_key: None,
    },
    HotkeyBind {
        name: "state_slot_decrease",
        advanced_key: None,
    },
    HotkeyBind {
        name: "play_replay",
        advanced_key: None,
    },
    HotkeyBind {
        name: "record_replay",
        advanced_key: None,
    },
    HotkeyBind {
        name: "halt_replay",
        advanced_key: None,
    },
    HotkeyBind {
        name: "save_replay_checkpoint",
        advanced_key: None,
    },
    HotkeyBind {
        name: "prev_replay_checkpoint",
        advanced_key: None,
    },
    HotkeyBind {
        name: "next_replay_checkpoint",
        advanced_key: None,
    },
    HotkeyBind {
        name: "replay_slot_increase",
        advanced_key: None,
    },
    HotkeyBind {
        name: "replay_slot_decrease",
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_eject_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_next",
        advanced_key: None,
    },
    HotkeyBind {
        name: "disk_prev",
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_hold",
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_next",
        advanced_key: None,
    },
    HotkeyBind {
        name: "shader_prev",
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_index_plus",
        advanced_key: None,
    },
    HotkeyBind {
        name: "cheat_index_minus",
        advanced_key: None,
    },
    HotkeyBind {
        name: "screenshot",
        advanced_key: None,
    },
    HotkeyBind {
        name: "recording_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "streaming_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "turbo_fire_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "grab_mouse_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "game_focus_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_fullscreen",
        advanced_key: None,
    },
    HotkeyBind {
        name: "desktop_menu_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_vrr_runloop",
        advanced_key: None,
    },
    HotkeyBind {
        name: "runahead_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "preempt_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "video_filter_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "fps_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "toggle_statistics",
        advanced_key: None,
    },
    HotkeyBind {
        name: "ai_service",
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_ping_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_host_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_game_watch",
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_player_chat",
        advanced_key: None,
    },
    HotkeyBind {
        name: "netplay_fade_chat_toggle",
        advanced_key: None,
    },
    HotkeyBind {
        name: "overlay_next",
        advanced_key: None,
    },
    HotkeyBind {
        name: "osk_toggle",
        advanced_key: None,
    },
];

impl HotkeyBind {
    pub fn keyboard_value(self, advanced: bool) -> &'static str {
        match self.advanced_key {
            Some(key) if advanced => key,
            _ => "nul",
        }
    }
}

/// Render the exported hotkey policy. Callers must not keep a second list.
///
/// With `advanced` we write `advanced_key` for the binds that have one, the
/// two fast-forward keys. We do not change button, axis or mouse. RetroArch's
/// menu toggle has no key and its gamepad combo is off, because the inputs on
/// MENU CONTROLS open the menu.
pub fn isolated_hotkey_config(advanced: bool) -> String {
    let mut config = String::new();
    for bind in HOTKEY_BINDS {
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
