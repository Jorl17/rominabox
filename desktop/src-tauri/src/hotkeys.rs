//! The RetroArch hotkeys that we write in an exported game.

/// RetroArch meta-bind policy for exported games.
///
/// This is the only bind list, and it covers every `DECLARE_META_BIND` in
/// the pinned RetroArch `configuration.c`. The desktop defaults in
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
        // In a shipped game the player quits from the in-game menu (Escape,
        // then Quit). Q is easy to press by accident during play, so we ship
        // the key only with advanced emulator access.
        advanced_key: Some("q"),
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
        // Fullscreen works like quit. If f stayed bound while q and f are
        // ordinary gameplay keys, one press would trigger the hotkey and the
        // bind together. The macOS window menu has a Full Screen item.
        advanced_key: Some("f"),
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
/// With `advanced` we write `advanced_key` for the binds that have one:
/// fast-forward, quit and fullscreen. We do not change button, axis or
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
    use std::fs;

    #[test]
    fn hotkey_policy_matches_pinned_retroarch_meta_binds() {
        // Read the fork's own source. A copy elsewhere can be missing, and then
        // this test would return early and pass without checking anything.
        let source = crate::repo::at("vendor/retroarch/configuration.c");
        assert!(source.is_file(), "missing {}", source.display());
        let text = fs::read_to_string(&source).unwrap();
        let mut pinned = Vec::new();
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("DECLARE_META_BIND(") else {
                continue;
            };
            let name = rest
                .split(',')
                .nth(1)
                .map(str::trim)
                .expect("DECLARE_META_BIND has a bind name");
            if !pinned.iter().any(|existing| existing == name) {
                pinned.push(name.to_string());
            }
        }
        let policy: Vec<&str> = HOTKEY_BINDS.iter().map(|bind| bind.name).collect();
        assert_eq!(
            policy,
            pinned.iter().map(String::as_str).collect::<Vec<_>>(),
            "HOTKEY_BINDS must stay exhaustive against pinned DECLARE_META_BIND"
        );

        let commented_defaults =
            fs::read_to_string(source.parent().unwrap().join("retroarch.cfg")).unwrap();
        assert!(commented_defaults.contains("input_toggle_fast_forward = space"));
        assert!(commented_defaults.contains("input_exit_emulator = escape"));
        assert!(commented_defaults.contains("input_menu_toggle = f1"));
    }
}
