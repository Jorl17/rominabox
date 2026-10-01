//! The author's defaults for the Controls screen in the player.
//!
//! We never change the bundled configuration. The player's changes are in a
//! separate override file for each game, so exporting again or relaunching
//! never discards the player's bindings or labels.

use crate::menu::key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

const MAX_LABEL_BYTES: usize = 80;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Controls {
    /// The controller variant. Without one, we use the console's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub bindings: BTreeMap<String, ControlOverride>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlOverride {
    pub label: Option<String>,
    pub key: Option<String>,
    /// The pad position we read the control from, when the author moved it.
    /// It is one of the `padPositions` in `controls.json`.
    pub pad: Option<String>,
    /// The mouse button that also presses the control, as named in a
    /// controls file. It is one of the values in `mouse_buttons.inc`.
    pub mouse: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlsRegistry {
    pad_positions: Vec<PadPosition>,
    profiles: Vec<ControlProfile>,
}

/// A position on the standard pad that we can read a control from, and the
/// words we show for it in the builder. We declare them in the catalog.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PadPosition {
    pub id: String,
    pub name: String,
    /// The opposite direction on the same axis of a stick, which moves with it.
    #[serde(default)]
    pub opposite: Option<String>,
}

impl PadPosition {
    /// A button (a face button, shoulder, trigger, d-pad direction, Start,
    /// Select or a stick's click), not one direction of a stick's axis.
    pub fn is_button(&self) -> bool {
        self.opposite.is_none()
    }
}

/// Every position on the standard pad, in the catalog's order.
pub fn pad_positions() -> Result<Vec<PadPosition>, String> {
    Ok(registry()?.pad_positions)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlProfile {
    pub id: String,
    pub name: String,
    pub systems: Vec<String>,
    pub image: String,
    /// The libretro device subclass for the core, absent for the standard joypad.
    pub core_device: Option<u32>,
    pub controls: Vec<ControlDefinition>,
}

/// The direction of a stick member, or its click, as we declare and check it
/// in the catalog and write it to `controls.json`. We list a stick's members
/// in this order, the order in which the player binds them in the menu.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StickDirection {
    Up,
    Right,
    Down,
    Left,
    Press,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDefinition {
    pub id: String,
    pub label: String,
    pub key: String,
    /// We draw grouped controls as one marker (the catalog's Control::group).
    #[serde(default)]
    pub group: Option<String>,
    /// The direction of a stick member, or `None` for any other control.
    #[serde(default)]
    pub direction: Option<StickDirection>,
    pub x: i32,
    pub y: i32,
    pub callout_x: i32,
    pub callout_y: i32,
}

/// Returns the declared controller profile for an emulated system.
pub fn profile_for_system(system: &str) -> Result<ControlProfile, String> {
    let default_profile = crate::systems::find(system)
        .map(|entry| entry.controller_profile.as_str())
        .unwrap_or("retropad");
    let profiles = registry()?.profiles;
    profiles
        .iter()
        .find(|profile| profile.id == default_profile)
        .or_else(|| profiles.iter().find(|profile| profile.id == "retropad"))
        .cloned()
        .ok_or_else(|| "The generic controller profile is missing.".to_string())
}

/// Validate the author's overrides against the selected controller layout.
///
/// A control may not use a key bound to a hotkey used during play. We refuse
/// that in `crate::hotkeys`, where we declare which keys those are.
pub fn validate_for_system(system: &str, controls: &Controls) -> Result<ControlProfile, String> {
    let profile = if let Some(id) = &controls.profile {
        let mut offered = variants_for_system(system)?;
        offered.extend(registry()?.profiles.into_iter().filter(|profile| profile.id == "retropad"));
        offered
            .into_iter()
            .find(|profile| profile.id == *id)
            .ok_or_else(|| format!("Controller variant {id} is not available for {system}."))?
    } else {
        profile_for_system(system)?
    };
    validate_for_profile(&profile, controls)?;
    crate::pad_positions::place(
        &declared_controls(system, &profile)?,
        &controls.bindings,
        &pad_positions()?,
    )?;
    Ok(profile)
}

/// The position on the standard pad of every control the game can show.
pub fn placement(
    system: &str,
    controls: &Controls,
) -> Result<Vec<crate::pad_positions::Placed>, String> {
    let profile = validate_for_system(system, controls)?;
    crate::pad_positions::place(
        &declared_controls(system, &profile)?,
        &controls.bindings,
        &pad_positions()?,
    )
}

/// Every control of every pad in the picker, with the chosen pad first. A
/// player who switches pad needs the labels and keys of the new pad at once,
/// because nobody can supply them in an exported game. The chosen pad comes
/// first so that the author's labels apply where two pads share a control.
fn declared_controls(
    system: &str,
    profile: &ControlProfile,
) -> Result<Vec<ControlDefinition>, String> {
    let mut declared = profile.controls.clone();
    for entry in &variants_for_system(system)? {
        for control in &entry.controls {
            if !declared.iter().any(|seen| seen.id == control.id) {
                declared.push(control.clone());
            }
        }
    }
    Ok(declared)
}

/// Every controller we offer for a console, in the order we show them.
///
/// We write them into the exported configuration, so that the controller
/// picker in the game can list them.
pub fn variants_for_system(system: &str) -> Result<Vec<ControlProfile>, String> {
    let normalized = normalize_system(system);
    let mut offered: Vec<ControlProfile> = registry()?
        .profiles
        .into_iter()
        .filter(|profile| {
            profile
                .systems
                .iter()
                .any(|candidate| normalize_system(candidate) == normalized)
        })
        .collect();
    // For a console with no dedicated pad we chose the generic profile on
    // purpose, so it is not a missing entry.
    if offered.is_empty() {
        offered = registry()?
            .profiles
            .into_iter()
            .filter(|profile| profile.id == "retropad")
            .collect();
    }
    Ok(offered)
}

/// Write the defaults for the Controls screen of the player.
pub fn write_defaults_config(
    system: &str,
    controls: &Controls,
    destination: &Path,
) -> Result<ControlProfile, String> {
    let profile = validate_for_system(system, controls)?;
    let mut config = format!("{} = \"{}\"\n", key!(ControlsProfile), profile.id);
    // The controllers that we offer in the picker in the game. We separate
    // the ids with spaces, because a controller id never contains one.
    let offered = variants_for_system(system)?;
    config.push_str(&format!(
        "{} = \"{}\"\n",
        key!(ControlsVariants),
        offered
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    ));
    for entry in &offered {
        config.push_str(&format!(
            "{} = \"{}\"\n",
            key!(ControlsVariantName, &entry.id),
            escape_config_value(&entry.name)
        ));
        // The emulated device for each variant, so that when the player picks
        // a pad we change the device too. We leave it out where the pad is the
        // core's default, which is most of them.
        if let Some(device) = entry.core_device {
            config.push_str(&format!(
                "{} = \"{device}\"\n",
                key!(ControlsVariantDevice, &entry.id)
            ));
        }
        // Which controls belong to this pad. In the game we find the controls
        // by going through every declared label. Without this list, the player
        // could focus the extra buttons of a six-button pad on a three-button
        // pad, whose scene has no element for them.
        config.push_str(&format!(
            "{} = \"{}\"\n",
            key!(ControlsVariantControls, &entry.id),
            entry
                .controls
                .iter()
                .map(|control| control.id.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    // We do not write the emulated device here, because
    // `input_libretro_device_p1` takes effect only in a remap file, never in
    // a config file. We write it in `packaging::stage_controller_remap`.
    let declared = declared_controls(system, &profile)?;
    let placed = crate::pad_positions::place(&declared, &controls.bindings, &pad_positions()?)?;
    for (control, value) in &every_control(&declared, &profile, controls) {
        // We bind a control's key and mouse button where we read the control.
        let slot = placed
            .iter()
            .find(|entry| entry.control == control.id)
            .map_or(control.id.as_str(), |entry| entry.slot.as_str());
        config.push_str(&format!(
            "{} = \"{}\"\ninput_player1_{slot} = \"{}\"\n",
            key!(ControlLabel, &control.id),
            escape_config_value(&value.label),
            escape_config_value(&value.key),
        ));
        if slot != control.id {
            config.push_str(&format!(
                "{} = \"{slot}\"\n",
                key!(ControlPosition, &control.id)
            ));
        }
        if let Some(group) = &control.group {
            config.push_str(&format!(
                "{} = \"{}\"\n",
                key!(ControlGroup, &control.id),
                escape_config_value(group),
            ));
        }
        if let Some(mouse) = &value.mouse {
            config.push_str(&format!("input_player1_{slot}_mbtn = \"{mouse}\"\n"));
        }
    }
    fs::write(destination, config)
        .map_err(|error| format!("write controls defaults {}: {error}", destination.display()))?;
    Ok(profile)
}

/// Every control of every pad in the picker, for this game: the chosen pad's
/// with the changes in `controls`, and the others as we declare them.
fn every_control(
    declared: &[ControlDefinition],
    profile: &ControlProfile,
    controls: &Controls,
) -> Vec<(ControlDefinition, EffectiveControl)> {
    let values = effective_controls(profile, controls);
    declared
        .iter()
        .map(|control| {
            let value = values.get(&control.id).cloned().unwrap_or_else(|| EffectiveControl {
                label: control.label.clone(),
                key: control.key.clone(),
                mouse: None,
            });
            (control.clone(), value)
        })
        .collect()
}

/// A key for a control in the game, with the control's id and its label,
/// on any pad in the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplayKey {
    pub control: String,
    pub label: String,
    pub key: String,
}

/// Every key for a control of the game `system` with `controls`, on every
/// pad in its picker. A hotkey used during play may not have any of these
/// keys (`crate::hotkeys`).
pub fn gameplay_keys(system: &str, controls: &Controls) -> Result<Vec<GameplayKey>, String> {
    let profile = validate_for_system(system, controls)?;
    Ok(every_control(&declared_controls(system, &profile)?, &profile, controls)
        .into_iter()
        .map(|(control, value)| GameplayKey { control: control.id, label: value.label, key: value.key })
        .collect())
}

#[derive(Clone, Debug)]
struct EffectiveControl {
    label: String,
    key: String,
    mouse: Option<String>,
}

fn effective_controls(
    profile: &ControlProfile,
    overrides: &Controls,
) -> BTreeMap<String, EffectiveControl> {
    profile
        .controls
        .iter()
        .map(|control| {
            let override_value = overrides.bindings.get(&control.id);
            (
                control.id.clone(),
                EffectiveControl {
                    label: override_value
                        .and_then(|value| value.label.clone())
                        .filter(|label| !label.trim().is_empty())
                        .unwrap_or_else(|| control.label.clone()),
                    key: override_value
                        .and_then(|value| value.key.clone())
                        .unwrap_or_else(|| control.key.clone()),
                    mouse: override_value.and_then(|value| value.mouse.clone()),
                },
            )
        })
        .collect()
}

fn validate_for_profile(profile: &ControlProfile, controls: &Controls) -> Result<(), String> {
    let valid_ids: HashSet<&str> = profile
        .controls
        .iter()
        .map(|control| control.id.as_str())
        .collect();
    for (id, value) in &controls.bindings {
        if !valid_ids.contains(id.as_str()) {
            return Err(format!(
                "control {id} is not part of the {} controller profile",
                profile.id
            ));
        }
        if let Some(label) = &value.label {
            validate_label(id, label)?;
        }
        if let Some(key) = &value.key {
            validate_key(id, key)?;
        }
        if let Some(mouse) = &value.mouse {
            if !crate::menu::words::mouse_buttons()
                .iter()
                .any(|button| &button.value == mouse)
            {
                return Err(format!("{mouse} is not a mouse button (for {id})"));
            }
        }
    }
    // We do not check control ids again here. In the catalog we refuse to
    // generate a registry with an unknown id, and a second copy of the list
    // in this crate could only drift from the first. We still check labels
    // and keys, because they are values and not identities.
    for control in &profile.controls {
        validate_label(&control.id, &control.label)?;
        validate_key(&control.id, &control.key)?;
    }
    Ok(())
}

fn registry() -> Result<ControlsRegistry, String> {
    serde_json::from_str(include_str!("../../controls.json"))
        .map_err(|error| format!("invalid bundled controls declaration: {error}"))
}

fn normalize_system(value: &str) -> String {
    crate::systems::find(value)
        .map(|system| system.id.clone())
        .unwrap_or_else(|| value.trim().to_ascii_lowercase())
}

fn validate_label(id: &str, value: &str) -> Result<(), String> {
    // When the author clears a custom label, we store an empty string, which
    // means "use the console label" and never a blank callout.
    if value.trim().is_empty() {
        return Ok(());
    }
    if value.len() > MAX_LABEL_BYTES {
        return Err(format!(
            "label for {id} must contain 1 to {MAX_LABEL_BYTES} bytes"
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() || character == '\u{7f}')
    {
        return Err(format!(
            "label for {id} contains an unsafe control character"
        ));
    }
    Ok(())
}

fn validate_key(id: &str, value: &str) -> Result<(), String> {
    if retroarch_key(value).is_none() {
        return Err(format!("key for {id} is not an allowed RetroArch key"));
    }
    Ok(())
}

/// The table of key names for the config parser in RetroArch.
const KEY_NAMES: &str = include_str!("../../../vendor/retroarch/input/input_key_names.inc");

/// Every `RIB_KEY_NAME("name", RETROK_key)` declaration in the fork, in order.
fn key_names() -> &'static [(String, String)] {
    static NAMES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let names: Vec<(String, String)> = crate::menu::inc::declarations(KEY_NAMES)
            .filter(|line| line.macro_name() == "RIB_KEY_NAME")
            .map(|line| match line.fields()[..] {
                [name, key] => (name.to_string(), key.to_string()),
                ref other => panic!(
                    "input_key_names.inc: RIB_KEY_NAME({}) is not (\"name\", RETROK_key)",
                    other.join(", ")
                ),
            })
            .collect();
        assert!(!names.is_empty(), "input_key_names.inc declares no keys");
        names
    })
}

/// The `RETROK_` name of the key for `name` in RetroArch, or `None` for a
/// name that is no key. We follow `input_config_translate_str_to_rk`. A
/// single letter is the key for that letter, and we look up any other name
/// in the key table, ignoring case. There `nul` means no key.
pub(crate) fn retroarch_key(name: &str) -> Option<String> {
    if let [letter] = name.as_bytes() {
        if letter.is_ascii_alphabetic() {
            return Some(format!("RETROK_{}", letter.to_ascii_lowercase() as char));
        }
    }
    key_names()
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map(|(_, key)| key.clone())
        .filter(|key| key != "RETROK_UNKNOWN")
}

fn escape_config_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retroarch_probe::Probe;

    fn key_names_probe() -> Probe {
        Probe::build(
            "key_names",
            &[
                "input/input_keymaps.c",
                "input/input_driver.c",
                "libretro-common/compat/compat_strl.c",
                "libretro-common/string/stdstring.c",
                "libretro-common/encodings/encoding_utf.c",
                "libretro-common/file/file_path.c",
            ],
        )
    }

    /// The key that `input_config_translate_str_to_rk`, the config parser in
    /// RetroArch, returns for each name, or `nul` for a name that is no key.
    /// We get it from `scripts/native_runtime/key_names.c`, compiled against
    /// the fork.
    fn retroarch_reads(names: &[&str]) -> Vec<String> {
        let read = key_names_probe().lines(names);
        assert_eq!(read.len(), names.len(), "one reading per name");
        read
    }

    /// The name we store for each browser key captured in the builder.
    fn builder_capture() -> Vec<(String, String)> {
        let text = fs::read_to_string(crate::repo::at("desktop/keyboard.json"))
            .expect("desktop/keyboard.json is readable");
        let keyboard: serde_json::Value =
            serde_json::from_str(&text).expect("desktop/keyboard.json is JSON");
        keyboard["capture"]
            .as_object()
            .expect("keyboard.json declares capture")
            .iter()
            .map(|(code, name)| (code.clone(), name.as_str().unwrap().to_string()))
            .collect()
    }

    /// The names given words in key_words.inc, for the game and the builder.
    fn worded_keys() -> Vec<String> {
        crate::menu::words::key_words()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn binding(key: &str) -> (String, Controls) {
        let control = profile_for_system("megadrive").unwrap().controls[0]
            .id
            .clone();
        let controls = serde_json::from_value(serde_json::json!({
            "bindings": { &control: { "key": key } }
        }))
        .unwrap();
        (control, controls)
    }

    // In the builder we store a key in the same form as the game's config, and
    // a word is for a key that the config can name. A name that is no key in
    // RetroArch exports without error and has no effect.
    #[test]
    fn every_key_the_builder_captures_or_words_is_a_name_retroarch_reads_as_that_key() {
        let capture = builder_capture();
        let worded = worded_keys();
        let names: Vec<&str> = capture.iter().map(|(_, name)| name.as_str()).collect();
        let mut wrong: Vec<String> = capture
            .iter()
            .zip(retroarch_reads(&names))
            .filter(|((_, name), read)| name != read)
            .map(|((code, name), read)| {
                format!("{code} is captured as {name}, which RetroArch reads as {read}")
            })
            .collect();
        let worded_names: Vec<&str> = worded.iter().map(String::as_str).collect();
        wrong.extend(
            worded
                .iter()
                .zip(retroarch_reads(&worded_names))
                .filter(|(name, read)| *name != read)
                .map(|(name, read)| {
                    format!("key_words.inc words {name}, which RetroArch reads as {read}")
                }),
        );
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn every_controller_default_is_a_name_retroarch_reads_as_that_key() {
        let profiles = registry().unwrap().profiles;
        let defaults: Vec<(String, &str)> = profiles
            .iter()
            .flat_map(|profile| {
                profile.controls.iter().map(move |control| {
                    (
                        format!("{} {}", profile.id, control.id),
                        control.key.as_str(),
                    )
                })
            })
            .collect();
        let names: Vec<&str> = defaults.iter().map(|(_, key)| *key).collect();
        let wrong: Vec<String> = defaults
            .iter()
            .zip(retroarch_reads(&names))
            .filter(|((_, key), read)| key != read)
            .map(|((control, key), read)| {
                format!("{control} defaults to {key}, which RetroArch reads as {read}")
            })
            .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn the_exporter_refuses_a_name_retroarch_reads_as_no_key() {
        let unknown = [
            "lshift",
            "lctrl",
            "lalt",
            "0",
            "9",
            "kp0",
            "kp_divide",
            "kp_multiply",
            "delete",
        ];
        assert_eq!(
            retroarch_reads(&unknown),
            vec!["nul"; unknown.len()],
            "RetroArch reads none of these as a key"
        );
        let accepted: Vec<&str> = unknown
            .into_iter()
            .filter(|name| validate_for_system("megadrive", &binding(name).1).is_ok())
            .collect();
        assert!(
            accepted.is_empty(),
            "the exporter accepts names RetroArch reads as no key: {}",
            accepted.join(", ")
        );
    }

    // Some keys have more than one name in RetroArch, and each name works.
    #[test]
    fn the_exporter_accepts_every_name_retroarch_reads_for_a_key() {
        let aliases = [
            "add",
            "kp_plus",
            "subtract",
            "kp_minus",
            "tilde",
            "backquote",
        ];
        assert_eq!(
            retroarch_reads(&aliases),
            vec!["add", "add", "subtract", "subtract", "tilde", "tilde"],
            "RetroArch reads each pair as one key"
        );
        let refused: Vec<String> = aliases
            .into_iter()
            .filter_map(|name| {
                validate_for_system("megadrive", &binding(name).1)
                    .err()
                    .map(|error| format!("{name}: {error}"))
            })
            .collect();
        assert!(
            refused.is_empty(),
            "the exporter refuses names RetroArch reads as a key:\n{}",
            refused.join("\n")
        );
    }

    // In RetroArch a name matches in any case, and one letter is that letter's
    // key. nul is no key. Escape is the default key of MENU in any spelling,
    // and we refuse it as a key for the game in the hotkey checks.
    #[test]
    fn the_exporter_reads_a_name_as_retroarch_does() {
        let names = ["Shift", "KP_PLUS", "Q", "q", "NUL", "Escape", "ESCAPE"];
        assert_eq!(
            retroarch_reads(&names),
            vec!["shift", "add", "q", "q", "nul", "escape", "escape"]
        );
        let hotkeys = crate::builder::unstated::hotkeys();
        let accepted: Vec<&str> = names
            .into_iter()
            .filter(|name| {
                let controls = binding(name).1;
                validate_for_system("megadrive", &controls).is_ok()
                    && hotkeys.check_for("megadrive", &controls).is_ok()
            })
            .collect();
        assert_eq!(accepted, vec!["Shift", "KP_PLUS", "Q", "q"]);
    }

    /// For every mouse button that we declare for the game and offer in the
    /// builder, the RetroArch parser returns the declared button. We check it
    /// with `scripts/native_runtime/mouse_buttons.c`, compiled against the fork.
    #[test]
    fn every_declared_mouse_button_is_read_by_retroarch_as_declared() {
        use crate::retroarch_probe::{CONFIGURED, INPUT_LAYER};
        let read = Probe::build_defining("mouse_buttons", CONFIGURED, INPUT_LAYER).lines(&[]);
        let declared = crate::menu::words::mouse_buttons();
        assert_eq!(declared.len(), 9, "five buttons and four wheel directions");
        assert_eq!(
            read,
            declared
                .iter()
                .map(|button| format!("{} ok", button.value))
                .collect::<Vec<_>>()
        );
    }

    /// A player can bind a wheel in the game, so the author can set one too,
    /// in the same form. We refuse a value with no button declared for it.
    #[test]
    fn the_exporter_writes_a_mouse_wheel_as_the_game_does() {
        let (control, wheel) = binding("x");
        let mut wheel = wheel;
        wheel.bindings.get_mut(&control).unwrap().mouse = Some("wu".into());
        let folder = rominabox_scratch::Scratch::dir("rominabox-mouse");
        let path = folder.path().join("controls.cfg");
        write_defaults_config("megadrive", &wheel, &path).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(
            written.lines().any(|line| line == format!("input_player1_{control}_mbtn = \"wu\"")),
            "{written}"
        );
        wheel.bindings.get_mut(&control).unwrap().mouse = Some("6".into());
        let error = validate_for_system("megadrive", &wheel).unwrap_err();
        assert!(error.contains("6 is not a mouse button"), "{error}");
    }

    // Pressing Escape in the builder cancels a capture, so we never store it.
    #[test]
    fn the_exporter_accepts_every_key_the_builder_captures() {
        let capture = builder_capture();
        let refused: Vec<String> = capture
            .iter()
            .filter_map(|(code, name)| {
                validate_for_system("megadrive", &binding(name).1)
                    .err()
                    .map(|error| format!("{code} is captured as {name}: {error}"))
            })
            .collect();
        assert!(refused.is_empty(), "{}", refused.join("\n"));
    }
}
