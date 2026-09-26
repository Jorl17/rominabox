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
/// Escape is the only key a gameplay binding may not use, in every mode.
pub fn validate_for_system(system: &str, controls: &Controls) -> Result<ControlProfile, String> {
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
    validate_for_profile(&profile, controls)?;
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

/// Write the defaults for the Controls screen of the player.
pub fn write_defaults_config(
    system: &str,
    controls: &Controls,
    destination: &Path,
) -> Result<ControlProfile, String> {
    let profile = validate_for_system(system, controls)?;
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
        // Which controls belong to this pad. In the game we find the controls
        // by going through every declared label. Without this list, the player
        // could focus the extra buttons of a six-button pad on a three-button
        // pad, whose scene has no element for them.
        config.push_str(&format!(
            "controls_variant_controls_{} = \"{}\"\n",
            entry.id,
            entry
                .controls
                .iter()
                .map(|control| control.id.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    // We do not write the emulated device here.
    // `input_libretro_device_p1` works only in a remap file, never in a config
    // file, so we write it to the remap file in
    // `packaging::stage_controller_remap`.
    // We write every pad that the player can choose, not only the chosen one,
    // so that a player who switches pad has the new pad's labels and keys in
    // the exported game. We write the chosen pad first, so that the author's
    // own labels take precedence where two pads have the same control
    // id.
    let mut declared: Vec<crate::controls::ControlDefinition> = profile.controls.clone();
    for entry in &offered {
        for control in &entry.controls {
            if !declared.iter().any(|seen| seen.id == control.id) {
                declared.push(control.clone());
            }
        }
    }
    for control in &declared {
        let fallback = EffectiveControl {
            label: control.label.clone(),
            key: control.key.clone(),
            button: None,
            axis: None,
            mouse: None,
        };
        let value = values.get(&control.id).unwrap_or(&fallback);
        config.push_str(&format!(
            "rib_label_{} = \"{}\"\ninput_player1_{} = \"{}\"\n",
            control.id,
            escape_config_value(&value.label),
            control.id,
            escape_config_value(&value.key),
        ));
        if let Some(group) = &control.group {
            config.push_str(&format!(
                "rib_group_{} = \"{}\"\n",
                control.id,
                escape_config_value(group),
            ));
        }
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
    if !retroarch_keys().contains(value) {
        return Err(format!("key for {id} is not an allowed RetroArch key"));
    }
    // The player opens the menu, where Quit is, with Escape, so Escape is
    // never a gameplay binding. We reserve no other key.
    if value == "escape" {
        return Err(format!(
            "key for {id} toggles the menu and cannot be a gameplay binding"
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

/// Every name for a keyboard key in RetroArch, one per key, in RetroArch's own
/// spelling. We generate it with `scripts/native_runtime/key_names.c` from the
/// fork's own key table and parser. See
/// `tests::the_checked_in_key_names_are_what_the_fork_reads`.
fn retroarch_keys() -> &'static HashSet<String> {
    static KEYS: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        serde_json::from_str::<RetroArchKeys>(include_str!("../../retroarch-keys.json"))
            .expect("desktop/retroarch-keys.json must be valid")
            .keys
            .into_iter()
            .collect()
    })
}

#[derive(Deserialize, Serialize)]
struct RetroArchKeys {
    keys: Vec<String>,
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
                "libretro-common/string/stdstring.c",
            ],
        )
    }

    /// `desktop/retroarch-keys.json` from the fork's key table and parser.
    fn generated_key_names() -> String {
        let keys = RetroArchKeys {
            keys: key_names_probe().lines(&[]),
        };
        serde_json::to_string_pretty(&keys).unwrap() + "\n"
    }

    const KEY_NAMES: &str = "desktop/retroarch-keys.json";

    /// The check against drift. If it fails, the fork's key names changed. Run
    ///
    ///     cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib \
    ///         controls::tests::write_retroarch_key_names -- --ignored
    ///
    /// and commit the written file.
    #[test]
    fn the_checked_in_key_names_are_what_the_fork_reads() {
        let checked_in = fs::read_to_string(crate::repo::at(KEY_NAMES)).expect("readable");
        assert_eq!(
            checked_in,
            generated_key_names(),
            "{KEY_NAMES} is not what the fork's RetroArch reads; regenerate it with write_retroarch_key_names"
        );
    }

    #[test]
    #[ignore = "writes desktop/retroarch-keys.json from the fork; run it when the fork's key names change"]
    fn write_retroarch_key_names() {
        fs::write(crate::repo::at(KEY_NAMES), generated_key_names()).expect("writable");
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

    /// The builder's key declaration: the name for each browser key that we
    /// capture, and the names that have words in the builder.
    fn builder_keyboard() -> (Vec<(String, String)>, Vec<String>) {
        let text = fs::read_to_string(crate::repo::at("desktop/keyboard.json"))
            .expect("desktop/keyboard.json is readable");
        let keyboard: serde_json::Value =
            serde_json::from_str(&text).expect("desktop/keyboard.json is JSON");
        let capture = keyboard["capture"]
            .as_object()
            .expect("keyboard.json declares capture")
            .iter()
            .map(|(code, name)| (code.clone(), name.as_str().unwrap().to_string()))
            .collect();
        let labelled = keyboard["labels"]
            .as_object()
            .expect("keyboard.json declares labels")
            .keys()
            .cloned()
            .collect();
        (capture, labelled)
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

    // We store a key in the builder as it is written in the game's config. A
    // name that is no key in RetroArch exports without error and has no effect.
    #[test]
    fn the_builder_captures_each_key_by_a_name_retroarch_reads_as_that_key() {
        let (capture, labelled) = builder_keyboard();
        let names: Vec<&str> = capture.iter().map(|(_, name)| name.as_str()).collect();
        let mut wrong: Vec<String> = capture
            .iter()
            .zip(retroarch_reads(&names))
            .filter(|((_, name), read)| name != read)
            .map(|((code, name), read)| {
                format!("{code} is captured as {name}, which RetroArch reads as {read}")
            })
            .collect();
        let labelled_names: Vec<&str> = labelled.iter().map(String::as_str).collect();
        wrong.extend(
            labelled
                .iter()
                .zip(retroarch_reads(&labelled_names))
                .filter(|(name, read)| *name != read)
                .map(|(name, read)| {
                    format!("the label for {name} names what RetroArch reads as {read}")
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

    // Pressing Escape in the builder cancels a capture, so we never store it.
    #[test]
    fn the_exporter_accepts_every_key_the_builder_captures() {
        let (capture, _) = builder_keyboard();
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
