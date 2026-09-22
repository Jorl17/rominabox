//! The author's defaults for the Controls screen in the player.
//!
//! We never change the bundled configuration. The player's changes are in a
//! separate override file for each game, so exporting again or relaunching
//! never discards the player's bindings or labels.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

const MAX_LABEL_BYTES: usize = 80;
const MAX_KEY_BYTES: usize = 32;
const MAX_BUTTON_BYTES: usize = 3;
const MAX_AXIS_BYTES: usize = 4;
const MAX_MOUSE_BUTTON: u32 = 5;

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
#[serde(rename_all = "camelCase")]
pub struct ControlOverride {
    pub label: Option<String>,
    pub key: Option<String>,
    pub button: Option<String>,
    pub axis: Option<String>,
    pub mouse: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlsRegistry {
    profiles: Vec<ControlProfile>,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDefinition {
    pub id: String,
    pub label: String,
    pub key: String,
    /// We draw grouped controls as one marker (the catalog's Control::group).
    #[serde(default)]
    pub group: Option<String>,
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

/// Validates author-provided overrides against the selected controller layout.
///
/// This is the check for a shipped game, with advanced emulator access off,
/// so `q` and `f` are ordinary keys. Escape stays reserved in both modes.
pub fn validate_for_system(system: &str, controls: &Controls) -> Result<ControlProfile, String> {
    validate_for_system_with_advanced_access(system, controls, false)
}

/// Same validation as [`validate_for_system`], with quit and fullscreen reserved
/// only when `advanced_emulator_access` is on.
///
/// The flag is the advanced-access opt-in, not a second recovery setting.
/// While it is on, `q` quits and `f` toggles fullscreen, so with a gameplay
/// bind on either key, one press would trigger both.
pub fn validate_for_system_with_advanced_access(
    system: &str,
    controls: &Controls,
    advanced_emulator_access: bool,
) -> Result<ControlProfile, String> {
    let profile = if let Some(id) = &controls.profile {
        let normalized = normalize_system(system);
        registry()?
            .profiles
            .into_iter()
            .find(|profile| {
                profile.id == *id
                    && (profile.id == "retropad"
                        || profile
                            .systems
                            .iter()
                            .any(|candidate| normalize_system(candidate) == normalized))
            })
            .ok_or_else(|| format!("Controller variant {id} is not available for {system}."))?
    } else {
        profile_for_system(system)?
    };
    validate_for_profile(&profile, controls, advanced_emulator_access)?;
    Ok(profile)
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

/// Write the immutable defaults for the native Controls screen.
///
/// We use this for a shipped game, without advanced emulator access, so we do
/// not reserve quit and fullscreen.
pub fn write_defaults_config(
    system: &str,
    controls: &Controls,
    destination: &Path,
) -> Result<ControlProfile, String> {
    write_defaults_config_with_advanced_access(system, controls, destination, false)
}

/// [`write_defaults_config`] for an export that may have reserved quit and
/// fullscreen. `advanced_emulator_access` is the same flag as in the hotkey policy.
pub fn write_defaults_config_with_advanced_access(
    system: &str,
    controls: &Controls,
    destination: &Path,
    advanced_emulator_access: bool,
) -> Result<ControlProfile, String> {
    let profile =
        validate_for_system_with_advanced_access(system, controls, advanced_emulator_access)?;
    let values = effective_controls(&profile, controls);
    let mut config = format!("controls_profile = \"{}\"\n", profile.id);
    // The controllers that we offer in the picker in the game. We separate
    // the ids with spaces, because a controller id never contains one.
    let offered = variants_for_system(system)?;
    config.push_str(&format!(
        "controls_variants = \"{}\"\n",
        offered
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    ));
    for entry in &offered {
        config.push_str(&format!(
            "controls_variant_name_{} = \"{}\"\n",
            entry.id,
            escape_config_value(&entry.name)
        ));
        // The emulated device for each variant, so that when the player picks
        // a pad we change the device too. We leave it out where the pad is the
        // core's default, which is most of them.
        if let Some(device) = entry.core_device {
            config.push_str(&format!(
                "controls_variant_device_{} = \"{device}\"\n",
                entry.id
            ));
        }
    }
    // We do not write the emulated device here, because
    // `input_libretro_device_p1` takes effect only in a remap file, never in
    // a config file. We write it in `packaging::stage_controller_remap`.
    for control in &profile.controls {
        let value = values
            .get(&control.id)
            .expect("profile controls are always present in effective controls");
        config.push_str(&format!(
            "rib_label_{} = \"{}\"\ninput_player1_{} = \"{}\"\n",
            control.id,
            escape_config_value(&value.label),
            control.id,
            escape_config_value(&value.key),
        ));
        if let Some(button) = &value.button {
            config.push_str(&format!(
                "input_player1_{}_btn = \"{}\"\n",
                control.id, button
            ));
        }
        if let Some(axis) = &value.axis {
            config.push_str(&format!(
                "input_player1_{}_axis = \"{}\"\n",
                control.id, axis
            ));
        }
        if let Some(mouse) = value.mouse {
            config.push_str(&format!(
                "input_player1_{}_mbtn = \"{}\"\n",
                control.id, mouse
            ));
        }
    }
    fs::write(destination, config)
        .map_err(|error| format!("write controls defaults {}: {error}", destination.display()))?;
    Ok(profile)
}

#[derive(Clone, Debug)]
struct EffectiveControl {
    label: String,
    key: String,
    button: Option<String>,
    axis: Option<String>,
    mouse: Option<u32>,
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
                    button: override_value.and_then(|value| value.button.clone()),
                    axis: override_value.and_then(|value| value.axis.clone()),
                    mouse: override_value.and_then(|value| value.mouse),
                },
            )
        })
        .collect()
}

fn validate_for_profile(
    profile: &ControlProfile,
    controls: &Controls,
    advanced_emulator_access: bool,
) -> Result<(), String> {
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
            validate_key(id, key, advanced_emulator_access)?;
        }
        if let Some(button) = &value.button {
            validate_button(id, button)?;
        }
        if let Some(axis) = &value.axis {
            validate_axis(id, axis)?;
        }
        if let Some(mouse) = value.mouse {
            if mouse > MAX_MOUSE_BUTTON {
                return Err(format!(
                    "mouse button for {id} must be between 0 and {MAX_MOUSE_BUTTON}"
                ));
            }
        }
    }
    // We do not check control ids again here. In the catalog we refuse to
    // generate a registry with an unknown id, and a second copy of the list
    // in this crate could only drift from the first. We still check labels
    // and keys, because they are values and not identities.
    for control in &profile.controls {
        validate_label(&control.id, &control.label)?;
        validate_key(&control.id, &control.key, advanced_emulator_access)?;
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

fn validate_key(id: &str, value: &str, advanced_emulator_access: bool) -> Result<(), String> {
    if value.len() > MAX_KEY_BYTES || !retroarch_keys().contains(value) {
        return Err(format!("key for {id} is not an allowed RetroArch key"));
    }
    // The player opens the menu with Escape in both modes, the only way to Quit
    // when Q is not a hotkey, so Escape is never a gameplay binding.
    if value == "escape" {
        return Err(format!(
            "key for {id} toggles the menu and cannot be a gameplay binding"
        ));
    }
    // Q quits and F toggles fullscreen only while advanced access is on. With
    // a gameplay bind on the same key, one press would trigger both. In a
    // shipped game the keys are free.
    if advanced_emulator_access && matches!(value, "q" | "f") {
        return Err(format!(
            "key for {id} is reserved for player recovery and cannot be rebound"
        ));
    }
    Ok(())
}

fn validate_button(id: &str, value: &str) -> Result<(), String> {
    let valid = value.len() <= MAX_BUTTON_BYTES
        && value.bytes().all(|character| character.is_ascii_digit())
        && value
            .parse::<u8>()
            .ok()
            .filter(|number| *number <= 63 && number.to_string() == value)
            .is_some();
    if !valid {
        return Err(format!(
            "button for {id} must be a controller button from 0 to 63"
        ));
    }
    Ok(())
}

fn validate_axis(id: &str, value: &str) -> Result<(), String> {
    let valid = value.len() <= MAX_AXIS_BYTES
        && matches!(value.as_bytes().first(), Some(b'+') | Some(b'-'))
        && value[1..]
            .bytes()
            .all(|character| character.is_ascii_digit())
        && value[1..]
            .parse::<u8>()
            .ok()
            .filter(|number| *number <= 15 && format!("{}{}", &value[..1], number) == value)
            .is_some();
    if !valid {
        return Err(format!(
            "axis for {id} must be a signed controller axis from -15 to +15"
        ));
    }
    Ok(())
}

fn retroarch_keys() -> &'static HashSet<&'static str> {
    static KEYS: std::sync::OnceLock<HashSet<&'static str>> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        [
            "a",
            "b",
            "c",
            "d",
            "e",
            "f",
            "g",
            "h",
            "i",
            "j",
            "k",
            "l",
            "m",
            "n",
            "o",
            "p",
            "q",
            "r",
            "s",
            "t",
            "u",
            "v",
            "w",
            "x",
            "y",
            "z",
            "nul",
            "0",
            "1",
            "2",
            "3",
            "4",
            "5",
            "6",
            "7",
            "8",
            "9",
            "up",
            "down",
            "left",
            "right",
            "enter",
            "space",
            "tab",
            "backspace",
            "escape",
            "insert",
            "delete",
            "home",
            "end",
            "pageup",
            "pagedown",
            "lshift",
            "rshift",
            "lctrl",
            "rctrl",
            "lalt",
            "ralt",
            "comma",
            "period",
            "slash",
            "semicolon",
            "quote",
            "leftbracket",
            "rightbracket",
            "minus",
            "equals",
            "backquote",
            "kp0",
            "kp1",
            "kp2",
            "kp3",
            "kp4",
            "kp5",
            "kp6",
            "kp7",
            "kp8",
            "kp9",
            "kp_period",
            "kp_divide",
            "kp_multiply",
            "kp_minus",
            "kp_plus",
            "kp_enter",
            "f1",
            "f2",
            "f3",
            "f4",
            "f5",
            "f6",
            "f7",
            "f8",
            "f9",
            "f10",
            "f11",
            "f12",
        ]
        .into_iter()
        .collect()
    })
}

fn escape_config_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
