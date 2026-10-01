use super::*;
use crate::controls::{ControlOverride, Controls};

fn hotkeys(json: serde_json::Value) -> Result<Hotkeys, String> {
    serde_json::from_value(json).map_err(|error| error.to_string())
}

fn named(id: &str) -> Hotkey {
    Hotkey::named(id).unwrap_or_else(|| panic!("no hotkey {id}"))
}

fn texts(hotkeys: &Hotkeys, id: &str) -> Vec<String> {
    hotkeys.of(named(id)).iter().map(Binding::text).collect()
}

/// We read the hotkeys from the player's declarations, as in its build: what
/// each keeps, when the player can use each, and which two share.
#[test]
fn the_hotkeys_are_the_players_own() {
    assert_eq!(
        Hotkey::all().map(Hotkey::id).collect::<Vec<_>>(),
        ["menu", "confirm", "back", "quick-save", "quick-load", "previous-slot", "next-slot"]
    );
    assert_eq!(named("menu").keeps(), Keeps::Key);
    assert_eq!(named("confirm").keeps(), Keeps::Binding);
    assert_eq!(named("quick-save").keeps(), Keeps::Nothing);
    assert_eq!(named("menu").acts(), Acts::Both);
    assert_eq!(named("back").acts(), Acts::InMenu);
    assert_eq!(named("next-slot").acts(), Acts::InGame);
    assert!(named("menu").shares_with(named("back")) && named("back").shares_with(named("menu")));
    assert!(!named("confirm").shares_with(named("back")));
    assert!(!named("quick-save").shares_with(named("confirm")));
}

/// The builder's defaults: MENU is Escape, the pad's Home and L3+R3. CONFIRM
/// is Enter and the bottom face button (Cross). BACK is Escape and the right
/// face button (Circle). QUICK SAVE, QUICK LOAD, PREVIOUS SLOT and NEXT SLOT
/// are RetroArch's desktop keys for them, F2, F4, F6 and F7, with no pad input.
#[test]
fn the_builders_defaults_are_todays_menu_keys_and_retroarchs_state_keys() {
    let defaults = crate::builder::unstated::hotkeys();
    assert_eq!(texts(&defaults, "menu"), ["key:escape", "pad:home", "pad:l3+r3"]);
    assert_eq!(texts(&defaults, "confirm"), ["key:enter", "pad:b"]);
    assert_eq!(texts(&defaults, "back"), ["key:escape", "pad:a"]);
    assert_eq!(texts(&defaults, "quick-save"), ["key:f2"]);
    assert_eq!(texts(&defaults, "quick-load"), ["key:f4"]);
    assert_eq!(texts(&defaults, "previous-slot"), ["key:f6"]);
    assert_eq!(texts(&defaults, "next-slot"), ["key:f7"]);
    defaults.check().unwrap();
}

#[test]
fn a_request_changes_the_hotkeys_it_names_and_keeps_the_rest() {
    let chosen = hotkeys(serde_json::json!({
        "confirm": ["key:space", "pad:b"], "back": ["key:backspace", "pad:a"], "quick-save": ["key:f5", "pad:select"]
    }))
    .unwrap();
    assert_eq!(
        chosen.of(named("confirm")),
        [Binding::Key("space".into()), Binding::Pad(vec![PadInput::Position("b".into())])]
    );
    assert_eq!(texts(&chosen, "quick-save"), ["key:f5", "pad:select"]);
    let defaults = crate::builder::unstated::hotkeys();
    for kept in ["menu", "quick-load", "previous-slot", "next-slot"] {
        assert_eq!(chosen.of(named(kept)), defaults.of(named(kept)), "{kept}");
    }
    chosen.check().unwrap();
    let error = hotkeys(serde_json::json!({ "pause": ["key:p"] })).unwrap_err();
    assert!(
        error.contains("'pause' is no hotkey")
            && error.contains("menu, confirm, back, quick-save, quick-load, previous-slot, next-slot"),
        "{error}"
    );
}

#[test]
fn a_binding_is_a_key_retroarch_reads_or_pad_inputs_the_standard_pad_has() {
    assert_eq!(Binding::read("key:f1").unwrap(), Binding::Key("f1".into()));
    assert_eq!(
        Binding::read("pad:l3+r3").unwrap(),
        Binding::Pad(vec![PadInput::Position("l3".into()), PadInput::Position("r3".into())])
    );
    assert_eq!(Binding::read("pad:home").unwrap(), Binding::Pad(vec![PadInput::Home]));
    for (text, says) in [
        ("key:notakey", "no key RetroArch reads"),
        ("pad:l_x_plus", "no pad input"),
        ("pad:a+a", "twice"),
        ("escape", "no binding"),
    ] {
        let error = Binding::read(text).unwrap_err();
        assert!(error.contains(says), "{text}: {error}");
    }
    assert_eq!(Binding::read("pad:l3+r3").unwrap().text(), "pad:l3+r3");
}

/// The rules we apply in the menu to every change, and check before an
/// export: nobody can be locked out, and only two hotkeys that may share an
/// input can both have it. A hotkey that keeps nothing may have no binding.
#[test]
fn defaults_that_would_lock_a_player_out_or_hold_an_input_twice_are_refused() {
    for (json, says) in [
        (serde_json::json!({ "menu": ["pad:home"] }), "menu has no key"),
        (serde_json::json!({ "confirm": [] }), "confirm has no binding"),
        (serde_json::json!({ "confirm": ["key:enter", "pad:a"] }), "bound to both"),
        (serde_json::json!({ "back": ["key:escape", "key:escape"] }), "twice"),
        (serde_json::json!({ "quick-save": ["key:f4"] }), "bound to both quick-save and quick-load"),
        (serde_json::json!({ "next-slot": ["key:enter"] }), "bound to both confirm and next-slot"),
    ] {
        let error = hotkeys(json.clone()).unwrap().check().unwrap_err().to_string();
        assert!(error.contains(says), "{json}: {error}");
    }
    // MENU and BACK have the same effect in the menu, so they may share one.
    hotkeys(serde_json::json!({ "back": ["pad:home"] })).unwrap().check().unwrap();
    hotkeys(serde_json::json!({ "quick-save": [], "quick-load": [], "previous-slot": [], "next-slot": [] }))
        .unwrap()
        .check()
        .unwrap();
}

fn keyed(control: &str, key: &str) -> Controls {
    Controls {
        profile: None,
        bindings: [(control.to_string(), ControlOverride { key: Some(key.into()), ..Default::default() })].into(),
    }
}

/// A hotkey used during play may have none of the game's keys, or one press
/// would do both. We refuse such an export and name the control. A hotkey
/// used only in the menu may have one, because the game is paused there. For
/// example, Enter is CONFIRM and Start on the Mega Drive.
#[test]
fn a_hotkey_that_acts_while_the_game_plays_holds_none_of_the_games_keys() {
    let defaults = crate::builder::unstated::hotkeys();
    defaults.check_for("megadrive", &Controls::default()).unwrap();
    let refused = |hotkeys: &Hotkeys, controls: &Controls| hotkeys.check_for("megadrive", controls).unwrap_err();
    assert_eq!(
        refused(&defaults, &keyed("a", "f2")),
        Refusal::GameKey {
            binding: Binding::Key("f2".into()),
            hotkey: named("quick-save"),
            control: "a".into(),
            label: "C".into()
        }
    );
    // Escape is a key of MENU, and the player uses it during play.
    let escape = refused(&defaults, &keyed("b", "escape")).to_string();
    assert!(escape.contains("menu, which acts while the game plays") && escape.contains("key for b (B)"), "{escape}");
    // A hotkey with a default key of the game.
    let start = hotkeys(serde_json::json!({ "quick-load": ["key:enter"], "confirm": ["key:space"] })).unwrap();
    assert_eq!(
        refused(&start, &Controls::default()),
        Refusal::GameKey {
            binding: Binding::Key("enter".into()),
            hotkey: named("quick-load"),
            control: "start".into(),
            label: "Start".into()
        }
    );
    // Without Escape on MENU, Escape is a key like any other.
    hotkeys(serde_json::json!({ "menu": ["key:f1"], "back": ["key:backspace"] }))
        .unwrap()
        .check_for("megadrive", &keyed("b", "escape"))
        .unwrap();
}

/// We tell the builder the rule and the hotkeys, so that we can word the
/// problem there. We refuse in the builder whatever the export would refuse.
#[test]
fn a_refusal_names_the_rule_and_the_hotkeys_for_the_builder() {
    let refused = |json| serde_json::to_value(hotkeys(json).unwrap().check().unwrap_err()).unwrap();
    assert_eq!(
        refused(serde_json::json!({ "back": ["key:enter"] })),
        serde_json::json!({ "kind": "shared", "binding": "key:enter", "hotkey": "confirm", "other": "back" })
    );
    assert_eq!(
        refused(serde_json::json!({ "menu": ["pad:l3+r3"] })),
        serde_json::json!({ "kind": "noKey", "hotkey": "menu" })
    );
    let game_key = crate::builder::unstated::hotkeys().check_for("megadrive", &keyed("y", "f7")).unwrap_err();
    assert_eq!(
        serde_json::to_value(game_key).unwrap(),
        serde_json::json!({ "kind": "gameKey", "binding": "key:f7", "hotkey": "next-slot", "control": "y", "label": "A" })
    );
}

#[test]
fn the_defaults_file_holds_each_list_and_every_pad_inputs_words() {
    let text = crate::builder::unstated::hotkeys().defaults_config().unwrap();
    for line in [
        "hotkey_menu = \"key:escape pad:home pad:l3+r3\"\n",
        "hotkey_confirm = \"key:enter pad:b\"\n",
        "hotkey_back = \"key:escape pad:a\"\n",
        "hotkey_quick-save = \"key:f2\"\n",
        "hotkey_quick-load = \"key:f4\"\n",
        "hotkey_previous-slot = \"key:f6\"\n",
        "hotkey_next-slot = \"key:f7\"\n",
        "pad_word_b = \"Bottom button\"\n",
        "pad_word_a = \"Right button\"\n",
        "pad_word_home = \"Home\"\n",
    ] {
        assert!(text.contains(line), "{line}in {text}");
    }
    let unbound = hotkeys(serde_json::json!({ "quick-save": [] })).unwrap().defaults_config().unwrap();
    assert!(unbound.contains("hotkey_quick-save = \"\"\n"), "{unbound}");
}

/// Whether `button`, held on a stand-in pad with a profile from the kit, is
/// Home in the menu, and which button it is otherwise. We use
/// `scripts/native_runtime/pad_home.c` with the fork's reader (pad_inputs.c),
/// over the binds loaded from the staged profile by RetroArch's loader.
fn home_on(profile: &str, button: u32) -> Vec<String> {
    let staged = crate::repo::at("desktop/src-tauri/resources/runtime/autoconfig").join(profile);
    assert!(staged.is_file(), "the kit stages {}", staged.display());
    crate::retroarch_probe::Probe::build(
        "pad_home",
        &[
            "configuration.c",
            "input/input_driver.c",
            "input/input_keymaps.c",
            "menu/drivers/rmlui/pad_inputs.c",
            "libretro-common/file/config_file.c",
            "libretro-common/file/config_file_userdata.c",
            "libretro-common/lists/string_list.c",
            "libretro-common/compat/compat_strl.c",
            "libretro-common/compat/fopen_utf8.c",
            "libretro-common/string/stdstring.c",
            "libretro-common/encodings/encoding_utf.c",
            "libretro-common/file/file_path.c",
            "libretro-common/file/file_path_io.c",
            "libretro-common/streams/file_stream.c",
            "libretro-common/vfs/vfs_implementation.c",
            "libretro-common/time/rtime.c",
        ],
    )
    .lines(&[staged.to_str().unwrap(), &button.to_string()])
}

/// Home is the menu button on each pad, as named in its RetroArch profile:
/// the PS button on a DualSense and Guide on an Xbox pad. Both count as Home
/// while someone holds them. We keep that line in every profile we ship. Any
/// other button is its position on the standard pad from its profile (an
/// Xbox pad's A is the bottom button), and never Home.
#[test]
#[cfg(target_os = "macos")]
fn a_dualsense_and_an_xbox_pad_each_have_home() {
    assert_eq!(
        home_on("hid/DualSense Wireless Controller (PS5).cfg", 12),
        ["home 12", "held 1", "captured home"]
    );
    assert_eq!(
        home_on("hid/Xbox Wireless Controller.cfg", 15),
        ["home 15", "held 1", "captured home"]
    );
    assert_eq!(
        home_on("hid/Xbox Wireless Controller.cfg", 0),
        ["home 15", "held 0", "captured b"]
    );
}

#[test]
#[cfg(windows)]
fn a_dualsense_and_an_xbox_pad_each_have_home() {
    assert_eq!(home_on("dinput/DualSense5.cfg", 12), ["home 12", "held 1", "captured home"]);
    assert_eq!(
        home_on("xinput/XBOX Series Controller.cfg", 10),
        ["home 10", "held 1", "captured home"]
    );
}
