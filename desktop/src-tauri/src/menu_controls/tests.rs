use super::*;

fn controls(json: serde_json::Value) -> Result<MenuControls, String> {
    serde_json::from_value(json).map_err(|error| error.to_string())
}

/// Every action declared in the player is an action in the exporter, and the
/// other way round, because we read `Action` from `menu_controls.inc` and
/// keep no copy next to it.
#[test]
fn the_actions_are_the_players_own() {
    let declared: Vec<String> = declared("RIB_MENU_ACTION")
        .map(|fields| fields[0].to_string())
        .collect();
    let named: Vec<String> = Action::ALL.iter().map(|action| format!("{action:?}")).collect();
    assert_eq!(declared, named);
    assert_eq!(Action::ALL.map(Action::id), ["menu", "confirm", "back"]);
    assert_eq!(Action::Menu.keeps(), Keeps::Key);
    assert_eq!(Action::Confirm.keeps(), Keeps::Binding);
    assert!(Action::Menu.shares_with(Action::Back) && Action::Back.shares_with(Action::Menu));
    assert!(!Action::Confirm.shares_with(Action::Back));
}

/// The builder defaults. MENU is Escape, Home on the pad and L3+R3. CONFIRM
/// is Enter and the right face button. BACK is Escape and the bottom face
/// button.
#[test]
fn the_builders_defaults_are_todays_menu_keys() {
    let defaults = crate::builder::unstated::menu_controls();
    let texts = |action| defaults.of(action).iter().map(Binding::text).collect::<Vec<_>>();
    assert_eq!(texts(Action::Menu), ["key:escape", "pad:home", "pad:l3+r3"]);
    assert_eq!(texts(Action::Confirm), ["key:enter", "pad:a"]);
    assert_eq!(texts(Action::Back), ["key:escape", "pad:b"]);
    defaults.check().unwrap();
}

#[test]
fn a_request_changes_the_actions_it_names_and_keeps_the_rest() {
    let chosen = controls(serde_json::json!({ "confirm": ["key:space", "pad:b"], "back": ["key:backspace", "pad:a"] })).unwrap();
    assert_eq!(chosen.of(Action::Confirm), [Binding::Key("space".into()), Binding::Pad(vec![PadInput::Position("b".into())])]);
    assert_eq!(chosen.of(Action::Menu), crate::builder::unstated::menu_controls().of(Action::Menu));
    chosen.check().unwrap();
    let error = controls(serde_json::json!({ "pause": ["key:p"] })).unwrap_err();
    assert!(error.contains("'pause' is no menu action") && error.contains("menu, confirm, back"), "{error}");
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

/// We check the rules for menu changes before an export. No one is locked
/// out, and an input belongs to one action unless two actions share it.
#[test]
fn defaults_that_would_lock_a_player_out_or_hold_an_input_twice_are_refused() {
    for (json, says) in [
        (serde_json::json!({ "menu": ["pad:home"] }), "menu has no key"),
        (serde_json::json!({ "confirm": [] }), "confirm has no binding"),
        (serde_json::json!({ "confirm": ["key:enter", "pad:b"] }), "bound to both"),
        (serde_json::json!({ "back": ["key:escape", "key:escape"] }), "twice"),
    ] {
        let error = controls(json.clone()).unwrap().check().unwrap_err().to_string();
        assert!(error.contains(says), "{json}: {error}");
    }
    // MENU and BACK have the same effect in the menu, so they may share one.
    controls(serde_json::json!({ "back": ["pad:home"] })).unwrap().check().unwrap();
}

/// We report the rule and the actions, so that we can word the problem in
/// the builder and show it to the author before an export fails.
#[test]
fn a_refusal_names_the_rule_and_the_actions_for_the_builder() {
    let refused = |json| serde_json::to_value(controls(json).unwrap().check().unwrap_err()).unwrap();
    assert_eq!(
        refused(serde_json::json!({ "back": ["key:enter"] })),
        serde_json::json!({ "kind": "shared", "binding": "key:enter", "action": "confirm", "other": "back" })
    );
    assert_eq!(
        refused(serde_json::json!({ "menu": ["pad:l3+r3"] })),
        serde_json::json!({ "kind": "noKey", "action": "menu" })
    );
}

#[test]
fn the_defaults_file_holds_each_list_and_every_pad_inputs_words() {
    let text = crate::builder::unstated::menu_controls().defaults_config().unwrap();
    assert!(text.contains("menu_control_menu = \"key:escape pad:home pad:l3+r3\"\n"), "{text}");
    assert!(text.contains("menu_control_confirm = \"key:enter pad:a\"\n"), "{text}");
    assert!(text.contains("menu_control_back = \"key:escape pad:b\"\n"), "{text}");
    assert!(text.contains("pad_word_b = \"Bottom button\"\n"), "{text}");
    assert!(text.contains("pad_word_a = \"Right button\"\n"), "{text}");
    assert!(text.contains("pad_word_home = \"Home\"\n"), "{text}");
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
