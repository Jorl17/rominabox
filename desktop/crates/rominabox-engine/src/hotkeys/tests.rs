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
        [
            "menu",
            "confirm",
            "back",
            "previous-page",
            "next-page",
            "quick-save",
            "quick-load",
            "previous-slot",
            "next-slot",
            "fast-forward",
            "fullscreen"
        ]
    );
    assert_eq!(named("menu").keeps(), Keeps::Key);
    assert_eq!(named("confirm").keeps(), Keeps::Binding);
    assert_eq!(named("quick-save").keeps(), Keeps::Nothing);
    assert_eq!(named("menu").acts(), Acts::Both);
    assert_eq!(named("back").acts(), Acts::InMenu);
    assert_eq!(named("next-slot").acts(), Acts::InGame);
    assert_eq!(named("fullscreen").acts(), Acts::Both);
    assert!(named("menu").shares_with(named("back")) && named("back").shares_with(named("menu")));
    assert!(!named("confirm").shares_with(named("back")));
    assert!(named("quick-save").shares_with(named("confirm")));
}

/// The builder's defaults: MENU is Escape, the pad's Home and L3+R3. CONFIRM
/// is Enter and the bottom face button (Cross). BACK is Escape and the right
/// face button (Circle). PREVIOUS PAGE and NEXT PAGE are Page Up and Page
/// Down, and L1 and R1. QUICK SAVE, QUICK LOAD, PREVIOUS SLOT and NEXT SLOT
/// are RetroArch's desktop keys for them, F2, F4, F6 and F7, with no pad input.
#[test]
fn the_builders_defaults_are_todays_menu_keys_and_retroarchs_state_keys() {
    let defaults = crate::builder::unstated::hotkeys();
    assert_eq!(texts(&defaults, "menu"), ["key:escape", "pad:home", "pad:l3+r3"]);
    assert_eq!(texts(&defaults, "confirm"), ["key:enter", "pad:b"]);
    assert_eq!(texts(&defaults, "back"), ["key:escape", "pad:a"]);
    assert_eq!(texts(&defaults, "previous-page"), ["key:pageup", "pad:l"]);
    assert_eq!(texts(&defaults, "next-page"), ["key:pagedown", "pad:r"]);
    assert_eq!(texts(&defaults, "quick-save"), ["key:f2"]);
    assert_eq!(texts(&defaults, "quick-load"), ["key:f4"]);
    assert_eq!(texts(&defaults, "previous-slot"), ["key:f6"]);
    assert_eq!(texts(&defaults, "next-slot"), ["key:f7"]);
    defaults.check().unwrap();
}

#[test]
fn a_request_changes_the_hotkeys_it_names_and_keeps_the_rest() {
    let chosen = hotkeys(serde_json::json!({
        "confirm": ["key:k", "pad:b"], "back": ["key:backspace", "pad:a"], "quick-save": ["key:f5", "pad:select"]
    }))
    .unwrap();
    assert_eq!(
        chosen.of(named("confirm")),
        [Binding::Key("k".into()), Binding::Pad(vec![PadInput::Position("b".into())])]
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
            && error.contains("menu, confirm, back, previous-page, next-page, quick-save, quick-load"),
        "{error}"
    );
}

/// In the defaults of an export, a pad input is a position of the standard
/// pad or Home, so the binding works on any pad. An input with no position,
/// as pad:13, is only ever a player's own capture in the game.
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
        ("pad:13", "no pad input"),
        ("pad:a+a", "twice"),
        ("escape", "no binding"),
    ] {
        let error = Binding::read(text).unwrap_err();
        assert!(error.contains(says), "{text}: {error}");
    }
    assert_eq!(Binding::read("pad:l3+r3").unwrap().text(), "pad:l3+r3");
}

/// A hotkey of the menu and a hotkey of play never act at the same time, so
/// both may have one input: in the menu it does the first, during play the
/// second. A hotkey that acts in both places shares with neither, and two
/// hotkeys of the same place share only when hotkeys.inc says so.
#[test]
fn a_hotkey_of_the_menu_and_one_of_play_may_have_the_same_input() {
    assert!(named("back").shares_with(named("quick-save")) && named("quick-save").shares_with(named("back")));
    assert!(named("confirm").shares_with(named("next-slot")));
    assert!(!named("fullscreen").shares_with(named("quick-save")) && !named("fullscreen").shares_with(named("back")));
    assert!(!named("confirm").shares_with(named("back")) && !named("quick-save").shares_with(named("quick-load")));
    hotkeys(serde_json::json!({ "back": ["key:escape", "pad:l2"], "quick-save": ["key:f2", "pad:l2"] }))
        .unwrap()
        .check()
        .unwrap();
    let error = hotkeys(serde_json::json!({ "fullscreen": ["pad:l2"], "quick-save": ["key:f2", "pad:l2"] }))
        .unwrap()
        .check()
        .unwrap_err()
        .to_string();
    assert!(error.contains("bound to both"), "{error}");
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
        (serde_json::json!({ "fullscreen": ["key:f7"] }), "bound to both next-slot and fullscreen"),
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
        Refusal::GameInput {
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
    let start = hotkeys(serde_json::json!({ "quick-load": ["key:enter"], "confirm": ["key:k"] })).unwrap();
    assert_eq!(
        refused(&start, &Controls::default()),
        Refusal::GameInput {
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

/// We refuse a pad button as we refuse a key. A hotkey used during play may
/// have no position that the game uses, after the author moved its controls,
/// on any pad in the game. A chord is not one position, so MENU's L3+R3 is
/// allowed, and Home is no position at all.
#[test]
fn a_hotkey_that_acts_while_the_game_plays_holds_none_of_the_games_pad_buttons() {
    let refused = |json, controls: &Controls| {
        hotkeys(json).unwrap().check_for("megadrive", controls).unwrap_err().to_string()
    };
    // We read Mega Drive A from the left button.
    let left = refused(serde_json::json!({ "quick-save": ["key:f2", "pad:y"] }), &Controls::default());
    assert!(left.contains("pad:y is bound to quick-save") && left.contains("for y (A)"), "{left}");
    // The six-button pad's Mode is Select, which the three-button pad lacks.
    let mode = refused(serde_json::json!({ "next-slot": ["pad:select"] }), &Controls::default());
    assert!(mode.contains("pad:select is bound to next-slot") && mode.contains("for select (Mode)"), "{mode}");
    // A moved from the left button to L2, so L2 is the game's and left is free.
    let moved = Controls {
        profile: None,
        bindings: [("y".to_string(), ControlOverride { pad: Some("l2".into()), ..Default::default() })].into(),
    };
    let l2 = refused(serde_json::json!({ "quick-load": ["pad:l2"] }), &moved);
    assert!(l2.contains("pad:l2 is bound to quick-load") && l2.contains("for y (A)"), "{l2}");
    hotkeys(serde_json::json!({ "quick-load": ["pad:y"] })).unwrap().check_for("megadrive", &moved).unwrap();
    // The defaults are Home, and L3 and R3 held together, unused by any Mega Drive pad.
    crate::builder::unstated::hotkeys().check_for("megadrive", &Controls::default()).unwrap();
    hotkeys(serde_json::json!({ "quick-save": ["pad:l3+r3"], "menu": ["key:escape", "pad:home"] }))
        .unwrap()
        .check_for("megadrive", &Controls::default())
        .unwrap();
}

/// In the Controls step of the builder, the author binds a stick direction by
/// direction, and we check the hotkeys before each. Between two directions,
/// half an axis has moved, which we refuse on export for another reason. Even
/// then, we report only the conflicts of hotkeys with the game's inputs.
#[test]
fn the_games_inputs_are_checked_while_a_stick_is_half_moved() {
    let half = Controls {
        profile: None,
        bindings: [("l_x_minus", "b"), ("b", "l_x_minus")]
            .map(|(control, pad)| (control.to_string(), ControlOverride { pad: Some(pad.into()), ..Default::default() }))
            .into(),
    };
    assert!(crate::controls::validate_for_system("ps1", &half).is_err());
    crate::builder::unstated::hotkeys().check_for("ps1", &half).unwrap();
    assert_eq!(
        hotkeys(serde_json::json!({ "quick-save": ["key:z"] })).unwrap().check_for("ps1", &half).unwrap_err(),
        Refusal::GameInput {
            binding: Binding::Key("z".into()),
            hotkey: named("quick-save"),
            control: "b".into(),
            label: "Cross".into()
        }
    );
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
        serde_json::json!({ "kind": "gameInput", "binding": "key:f7", "hotkey": "next-slot", "control": "y", "label": "A" })
    );
}

#[test]
fn the_defaults_file_holds_each_list_and_every_pad_inputs_words() {
    let text = crate::builder::unstated::hotkeys().defaults_config(&crate::hotkeys::GameHotkeys::default()).unwrap();
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
    let unbound = hotkeys(serde_json::json!({ "quick-save": [] })).unwrap().defaults_config(&crate::hotkeys::GameHotkeys::default()).unwrap();
    assert!(unbound.contains("hotkey_quick-save = \"\"\n"), "{unbound}");
}

/// Whether `button`, held on a stand-in pad with a profile from the kit, is
/// Home in the menu, and which button it is otherwise. We use
/// `scripts/native_runtime/pad_home.c` with the fork's reader (pad_inputs.c),
/// over the binds loaded from the staged profile by RetroArch's loader.
fn home_on(profile: &str, button: u32) -> Vec<String> {
    let staged = crate::repo::builder_resources().join("runtime/autoconfig").join(profile);
    assert!(staged.is_file(), "the kit stages {}", staged.display());
    pad_home().lines(&[staged.to_str().unwrap(), &button.to_string()])
}

/// As `home_on`, with `button` held on pad `pad` (from 1) of a Mega Drive
/// game, with the remap we write on export, with or without every pad as
/// player 1, loaded with the RetroArch remap loader.
fn home_on_pad(profile: &str, button: u32, pad: u32, every_pad_is_player_one: bool) -> Vec<String> {
    let staged = crate::repo::builder_resources().join("runtime/autoconfig").join(profile);
    assert!(staged.is_file(), "the kit stages {}", staged.display());
    let folder = rominabox_scratch::Scratch::dir("rominabox-pad-home-remap");
    let remap = folder.path().join("Genesis Plus GX.rmp");
    let written = crate::pad_positions::remap_file(
        &crate::controls::profile_for_system("megadrive").unwrap(),
        &[],
        &crate::controls::unused_positions("megadrive", &crate::controls::Controls::default()).unwrap(),
        &crate::controls::unused_positions("megadrive", &crate::controls::Controls::default()).unwrap(),
        every_pad_is_player_one,
    )
    .unwrap();
    std::fs::write(&remap, written).unwrap();
    pad_home().lines(&[
        staged.to_str().unwrap(),
        &button.to_string(),
        &pad.to_string(),
        remap.to_str().unwrap(),
    ])
}

/// The remap loader is the one in the player, which we build with
/// `HAVE_CONFIGFILE`, along with the input layer and the menu's pad input.
fn pad_home() -> crate::retroarch_probe::Probe {
    use crate::retroarch_probe::{Probe, CONFIGURED, INPUT_LAYER, PAD_INPUTS};
    let sources = [INPUT_LAYER, PAD_INPUTS].concat();
    Probe::build_defining("pad_home", CONFIGURED, &sources)
}

/// Home is the menu button on each pad, as named in its RetroArch profile:
/// the PS button on a DualSense and Guide on an Xbox pad. Both count as Home
/// while someone holds them. We keep that line in every profile we ship, and
/// upstream's SDL profiles for Xbox pads name none. On macOS, Guide is 5 in
/// SDL on both pads. Any other button is its position on the standard pad
/// from its profile (an Xbox pad's A is the bottom button), and never Home.
#[test]
#[cfg(target_os = "macos")]
fn a_dualsense_and_an_xbox_pad_each_have_home() {
    assert_eq!(
        home_on("sdl2/PS5 Controller.cfg", 5),
        ["home 5", "held 1", "captured home"]
    );
    assert_eq!(
        home_on("sdl2/Xbox Series X Controller.cfg", 5),
        ["home 5", "held 1", "captured home"]
    );
    assert_eq!(
        home_on("sdl2/Xbox Series X Controller.cfg", 0),
        ["home 5", "held 0", "captured b"]
    );
}

/// The profile of an Xbox pad as staged on this platform, and its Home
/// button.
#[cfg(target_os = "macos")]
const XBOX_HOME: (&str, u32) = ("sdl2/Xbox Series X Controller.cfg", 5);
#[cfg(windows)]
const XBOX_HOME: (&str, u32) = ("xinput/XBOX Series Controller.cfg", 10);

/// Every pad is player 1 in a game unless the author turns that off. When it
/// is on, Home on the second pad, or on the last pad in RetroArch, opens the
/// menu as Home on the first pad does. When it is off, the second pad is
/// player 2, and its Home does not open the menu.
#[test]
#[cfg(any(target_os = "macos", windows))]
fn home_on_any_pad_that_plays_as_player_one_is_the_menus() {
    let (profile, home) = XBOX_HOME;
    let held = |pad: u32, every_pad: bool| home_on_pad(profile, home, pad, every_pad)[1].clone();
    assert_eq!(held(2, true), "held 1");
    assert_eq!(held(crate::pad_positions::PADS, true), "held 1");
    assert_eq!(held(1, true), "held 1");
    assert_eq!(held(2, false), "held 0");
    assert_eq!(held(1, false), "held 1");
}

/// The pad input we capture from stand-in pads with a profile from the kit,
/// with the pads in each frame as listed in `frames`. The frames and the
/// `options` are as in `scripts/native_runtime/pad_capture.c`. We run
/// RetroArch's own capture in the menu (menu_driver.c) with the fork's
/// reader.
fn captured_on(probe: &crate::retroarch_probe::Probe, options: &[&str], profile: &str, frames: &[&str]) -> String {
    let staged = crate::repo::builder_resources().join("runtime/autoconfig").join(profile);
    assert!(staged.is_file(), "the kit stages {}", staged.display());
    let mut arguments = options.to_vec();
    arguments.push(staged.to_str().unwrap());
    arguments.extend_from_slice(frames);
    probe.lines(&arguments).join("\n")
}

fn pad_capture() -> crate::retroarch_probe::Probe {
    use crate::retroarch_probe::{Probe, INPUT_LAYER, PAD_INPUTS};
    let sources = [INPUT_LAYER, &["menu/menu_driver.c"], PAD_INPUTS].concat();
    Probe::build_defining("pad_capture", &["HAVE_CONFIGFILE", "HAVE_MENU"], &sources)
}

/// Through DirectInput each trigger of a DualSense is a button, down at the
/// lightest touch, and an axis, which rests at its negative end. In its
/// profile, L2 and R2 are the axes only. On HOTKEYS and on CONTROLS, we
/// capture a trigger pressed at all as L2 or R2, pulled all the way or only
/// touched, as we capture R1 as R1.
#[test]
#[cfg(windows)]
fn a_dualsense_trigger_pressed_at_all_is_l2_or_r2() {
    let probe = pad_capture();
    let dualsense = "dinput/DualSense5.cfg";
    let rest = "a3:-32768 a4:-32768";
    for screen in [&[][..], &["--controls", "start"][..]] {
        let pressed = |button: &str, held: &str, frames: &[&str]| {
            let mut all = vec![rest.to_string(), rest.to_string()];
            all.extend(frames.iter().map(|frame| format!("{button} {held} {frame}")));
            all.push(rest.to_string());
            captured_on(&probe, screen, dualsense, &all.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let on = |what: &str| format!("captured {what} on pad 1, with {screen:?}");
        let said = |text: String| format!("{text}, with {screen:?}");
        assert_eq!(said(pressed("b6", "a4:-32768", &["a3:-20000", "a3:32767"])), on("l2"));
        assert_eq!(said(pressed("b7", "a3:-32768", &["a4:-20000", "a4:32767"])), on("r2"));
        assert_eq!(said(pressed("b6", "a4:-32768", &["a3:-30000"])), on("l2"), "a touch of L2");
        assert_eq!(said(pressed("b5", rest, &[""])), on("r"));
    }
}

/// The touchpad of a DualSense is button 13 through DirectInput, and has no
/// position in its profile. On HOTKEYS and on CONTROLS, we capture it when
/// the player releases it.
#[test]
#[cfg(windows)]
fn the_touchpad_is_captured_as_itself() {
    let probe = pad_capture();
    let rest = "a3:-32768 a4:-32768";
    for screen in [&[][..], &["--controls", "start"][..]] {
        assert_eq!(
            captured_on(&probe, screen, "dinput/DualSense5.cfg", &[rest, rest, &format!("b13 {rest}"), rest]),
            "captured button 13 on pad 1",
            "{screen:?}"
        );
    }
}

/// On CONTROLS we write a pad input in the form of RetroArch's config, and in
/// the menu we name it by its position in the profile of the first pad: on a
/// DualSense through DirectInput, the bottom button is Cross and the left
/// trigger is L2. The touchpad has no name there.
#[test]
#[cfg(windows)]
fn the_profile_of_a_dualsense_names_its_inputs() {
    let probe = pad_capture();
    let rest = "a3:-32768 a4:-32768";
    let named = |frame: &str| {
        captured_on(&probe, &["--named"], "dinput/DualSense5.cfg", &[rest, rest, &format!("{frame} {rest}"), rest])
    };
    assert_eq!(named("b1"), "captured b on pad 1\nvalue 1\nnamed Cross\nread 1");
    assert_eq!(
        captured_on(&probe, &["--named"], "dinput/DualSense5.cfg", &[rest, rest, "a3:32767 a4:-32768"]),
        "captured l2 on pad 1\nvalue +3\nnamed L2\nread +3"
    );
    assert_eq!(named("b13"), "captured button 13 on pad 1\nvalue 13\nnamed none\nread 13");
}

/// With every pad playing as player 1, we capture a press on any of them,
/// through the profile of the pad pressed.
#[test]
#[cfg(windows)]
fn a_capture_takes_any_pad_that_plays_as_player_one() {
    let probe = pad_capture();
    let rest = "a3:-32768 a4:-32768 2:a3:-32768 2:a4:-32768";
    for screen in [&["--pads", "2"][..], &["--pads", "2", "--controls", "start"][..]] {
        assert_eq!(
            captured_on(&probe, screen, "dinput/DualSense5.cfg", &[rest, rest, &format!("2:b5 {rest}"), rest]),
            "captured r on pad 2",
            "{screen:?}"
        );
    }
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

/// We give the pad defaults by the shoulder buttons with a meaning on the
/// pads of a console, counting every pad in its picker, and we accept them in
/// an export.
#[test]
fn the_pad_defaults_follow_the_shoulder_buttons_the_pads_use() {
    let all_four = [("quick-save", "pad:l"), ("quick-load", "pad:r"), ("previous-slot", "pad:l2"), ("next-slot", "pad:r2")];
    let save_and_load = [("quick-save", "pad:l2"), ("quick-load", "pad:r2")];
    let cases: [(&[&str], &[(&str, &str)]); 3] = [
        (&["gb", "gbc", "nes", "mastersystem", "sg1000", "gamegear"], &all_four),
        (&["snes", "gba", "dreamcast", "megadrive", "segacd"], &save_and_load),
        (&["n64", "gamecube", "ps1", "ps2", "atari2600", "pce", "pcecd", "atari7800"], &[]),
    ];
    let keys = crate::builder::unstated::hotkeys();
    for (systems, pads) in cases {
        for system in systems {
            let defaults = defaults_for(system).unwrap();
            for hotkey in Hotkey::all() {
                let mut expected = texts(&keys, hotkey.id());
                expected.extend(pads.iter().filter(|(id, _)| *id == hotkey.id()).map(|(_, pad)| pad.to_string()));
                assert_eq!(texts(&defaults, hotkey.id()), expected, "{system}, {}", hotkey.id());
            }
            defaults
                .check_for(system, &Controls::default())
                .unwrap_or_else(|refusal| panic!("{system}: {refusal}"));
        }
    }
}

/// We keep the hotkeys in a request and give it the defaults of its console
/// for the others. Without a console we leave it as it is.
#[test]
fn a_request_gets_the_defaults_of_its_console_for_what_it_leaves_out() {
    let mut request = serde_json::json!({ "system": "gb", "hotkeys": { "quick-save": ["key:f5"] } })
        .as_object()
        .unwrap()
        .clone();
    complete(&mut request).unwrap();
    let completed: Hotkeys = serde_json::from_value(request["hotkeys"].clone()).unwrap();
    assert_eq!(texts(&completed, "quick-save"), ["key:f5"]);
    assert_eq!(texts(&completed, "quick-load"), ["key:f4", "pad:r"]);
    assert_eq!(texts(&completed, "next-slot"), ["key:f7", "pad:r2"]);
    let mut without = serde_json::json!({ "title": "No console" }).as_object().unwrap().clone();
    complete(&mut without).unwrap();
    assert!(!without.contains_key("hotkeys"), "{without:?}");
}
