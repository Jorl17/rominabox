//! The registries we generate from the packages for the engine and the
//! builder: systems.json, controls.json and core-pins.json.

use crate::model::{self, Presentation, PAD_POSITIONS};
use crate::{Catalog, BUILTIN_GENERIC_PROFILE};
use std::collections::BTreeMap;

fn header_title_json(header: &model::HeaderTitle) -> serde_json::Value {
    use serde_json::json;
    let mut window = json!({ "offset": header.offset, "length": header.length });
    if let Some(anchor) = &header.anchor {
        window["anchor"] = json!(anchor);
    }
    if let Some(at) = header.complement_at {
        window["complementAt"] = json!(at);
    }
    if let Some(magic) = &header.magic {
        let mut rendered = json!({ "offset": magic.offset });
        if let Some(text) = &magic.text {
            rendered["text"] = json!(text);
        }
        if let Some(hex) = &magic.hex {
            rendered["hex"] = json!(hex);
        }
        window["magic"] = rendered;
    }
    window
}

pub fn compatibility_registries(catalog: &Catalog) -> Result<Vec<(&'static str, String)>, String> {
    use serde_json::{json, Map, Value};

    // systems.json has one entry per console, in declaration order.
    let mut systems = Vec::new();
    for (id, entry) in catalog.consoles_in_presentation_order() {
        let console = &entry.console;
        let mut system = Map::new();
        system.insert("id".into(), json!(id));
        system.insert("name".into(), json!(console.name));
        system.insert("aliases".into(), json!(console.aliases));
        system.insert("extensions".into(), json!(console.content.extensions));
        if !console.content.sheets.is_empty() {
            system.insert("sheets".into(), json!(console.content.sheets));
        }
        if !console.content.companions.is_empty() {
            system.insert("companions".into(), json!(console.content.companions));
        }
        if !console.content.recognize_only.is_empty() {
            system.insert(
                "recognizeOnly".into(),
                json!(console.content.recognize_only),
            );
        }
        // For a console with no checksum catalogue we write no field at all.
        if let Some(reference) = &console.metadata.catalog {
            system.insert("catalog".into(), json!(reference.name));
        }
        let cores: Vec<Value> = console
            .cores
            .iter()
            .filter_map(|binding| {
                let component = catalog.component(&binding.component)?;
                Some({
                    let mut core = json!({
                        // We write every declared target, so the registry does not
                        // depend on the machine where we generated it. We pick the
                        // target to run or build for when we read the registry.
                        "artifacts": component.artifacts,
                        "component": component.id,
                        "license": component.license.spdx,
                        "licenseFile": component.license.file,
                        "capabilities": component.capabilities,
                        // The paths of per-core files in RetroArch contain this
                        // exact string: the controller remap, and the options for
                        // an unfiltered picture. So we need it in the player too.
                        "libraryName": component.library_name,
                    });
                    component.write_picture(&mut core);
                    core
                })
            })
            .collect();
        system.insert("cores".into(), Value::Array(cores));
        if !console.firmware.is_empty() {
            let firmware: Vec<Value> = console
                .firmware
                .iter()
                .map(|group| {
                    json!({
                        "id": group.id,
                        "acceptedNames": group.accepted_names,
                        "minimum": group.minimum,
                        "help": group.help,
                    })
                })
                .collect();
            system.insert("firmware".into(), Value::Array(firmware));
        }
        system.insert(
            "controllerProfile".into(),
            json!(console.controllers.default),
        );
        system.insert("category".into(), json!(console.content.category));
        // We treat a bounded offset and a length as data, and a list of them
        // when the title is not at one address. A branch per console is code.
        if let Some(size) = console.recognition.copier_header {
            system.insert("copierHeader".into(), json!(size));
        }
        if !console.recognition.header_titles.is_empty() {
            let windows: Vec<Value> = console
                .recognition
                .header_titles
                .iter()
                .map(header_title_json)
                .collect();
            let value = if windows.len() == 1 {
                windows.into_iter().next().expect("one window")
            } else {
                Value::Array(windows)
            };
            system.insert("headerTitle".into(), value);
        }
        systems.push(Value::Object(system));
    }

    // controls.json has the profiles. We derive their system lists from the
    // consoles, so the two always match.
    let mut used_by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, entry) in catalog.consoles_in_presentation_order() {
        let mut offered: Vec<&str> = entry
            .console
            .controllers
            .variants
            .iter()
            .map(String::as_str)
            .collect();
        if offered.is_empty() {
            offered.push(entry.console.controllers.default.as_str());
        }
        for profile in offered {
            used_by.entry(profile).or_default().push(id.as_str());
        }
    }

    let mut profiles = Vec::new();
    for (id, profile) in catalog.profiles_in_presentation_order() {
        // We offer the built-in generic pad for every console. It is in no
        // console's package, so its system list is empty.
        let systems_for_profile: Vec<&str> = if id == BUILTIN_GENERIC_PROFILE {
            Vec::new()
        } else {
            used_by.get(id.as_str()).cloned().unwrap_or_default()
        };
        let mut entry = Map::new();
        entry.insert("id".into(), json!(id));
        entry.insert("name".into(), json!(profile.name));
        entry.insert("systems".into(), json!(systems_for_profile));
        entry.insert(
            "image".into(),
            match &profile.presentation {
                Presentation::Illustrated { image } => json!(image),
                Presentation::Generic => json!(""),
            },
        );
        let generic = matches!(profile.presentation, Presentation::Generic);
        let controls: Vec<Value> = profile
            .controls
            .iter()
            .enumerate()
            .map(|(index, control)| {
                let (callout_x, callout_y) = if generic {
                    Catalog::generic_callout(index)
                } else {
                    (
                        control.callout_x.unwrap_or(0),
                        control.callout_y.unwrap_or(0),
                    )
                };
                let mut rendered = json!({
                    "id": control.id,
                    "label": control.label,
                    "key": control.key,
                    "x": control.x.unwrap_or(0),
                    "y": control.y.unwrap_or(0),
                    "calloutX": callout_x,
                    "calloutY": callout_y,
                });
                // We leave the field out when it is absent, so the registry
                // entry of a profile without groups has no group field.
                if let Some(group) = &control.group {
                    rendered["group"] = json!(group);
                }
                if let Some(direction) = control.direction {
                    rendered["direction"] = json!(direction);
                }
                rendered
            })
            .collect();
        entry.insert("controls".into(), Value::Array(controls));
        // We leave it out when empty, so a profile without a stick has no field.
        if !profile.groups.is_empty() {
            entry.insert("groups".into(), json!(profile.groups));
        }
        if let Some(device) = profile.core_device {
            entry.insert("coreDevice".into(), json!(device));
        }
        profiles.push(Value::Object(entry));
    }

    let mut rendered = Vec::new();
    for (name, value) in [
        ("systems.json", json!({ "version": 1, "systems": systems })),
        (
            "controls.json",
            json!({
                "padPositions": PAD_POSITIONS,
                "profiles": profiles,
            }),
        ),
    ] {
        let mut text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        text.push('\n');
        rendered.push((name, text));
    }
    rendered.push(("core-pins.json", core_pins(catalog)?));
    Ok(rendered)
}

/// Where we download a core from, per platform.
///
/// libretro replaces `latest` on the buildbot in place, so this list contains
/// the file and the branch we read the licence from, and no hash. We compile
/// Gambatte and Genesis Plus GX locally on macOS when the recipe matches. In
/// an export we fetch the listed file when the kit lacks that compiled file.
fn core_pins(catalog: &Catalog) -> Result<String, String> {
    use serde_json::{json, Map, Value};

    let mut cores = Vec::new();
    let mut components: Vec<_> = catalog.components().collect();
    components.sort_by(|left, right| left.0.cmp(right.0));
    for (id, component) in components {
        let Some(provenance) = &component.provenance else {
            continue;
        };
        let artifacts: Map<String, Value> = component
            .artifacts
            .iter()
            .map(|(target, filename)| (target.clone(), json!({ "filename": filename })))
            .collect();
        cores.push(json!({
            "component": id,
            "repository": provenance.repository,
            "licenseFile": component.license.file,
            "licensePath": provenance.license_candidates.first(),
            "licenseRef": provenance.branch,
            "artifacts": Value::Object(artifacts),
        }));
    }
    let value = json!({
        "schemaVersion": 2,
        "coreMirrors": [
            "https://buildbot.libretro.com/nightly",
            "https://bot.libretro.com/nightly"
        ],
        "targets": {
            "linux-x86_64": "linux/x86_64",
            "macos-arm64": "apple/osx/arm64",
            "macos-x86_64": "apple/osx/x86_64",
            "windows-x86_64": "windows/x86_64"
        },
        "licenseMirrors": [
            "https://raw.githubusercontent.com/{repository}/{ref}/{path}",
            "https://cdn.jsdelivr.net/gh/{repository}@{ref}/{path}"
        ],
        "cores": cores
    });
    let mut text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    text.push('\n');
    Ok(text)
}
