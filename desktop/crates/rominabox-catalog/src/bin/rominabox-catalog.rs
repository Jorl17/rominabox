//! Developer tool for the console catalog.
//!
//!   rominabox-catalog validate [ROOT]   report every problem, exit non-zero if any
//!   rominabox-catalog list     [ROOT]   what is declared, and what each build claims
//!   rominabox-catalog generate [ROOT]   write the compatibility registries
//!
//! With `generate` we write `desktop/systems.json` and `desktop/controls.json`
//! from the packages. The output must be byte for byte the files checked in,
//! and we check this in the parity test.

use rominabox_catalog::{model::Presentation, Catalog, BUILTIN_GENERIC_PROFILE, PACKAGE_ROOT};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "validate".to_string());
    let root = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| repository_root().join(PACKAGE_ROOT));

    let catalog = match Catalog::load(&root) {
        Ok(catalog) => catalog,
        Err(problems) => {
            eprintln!("{} problem(s) in {}:\n", problems.len(), root.display());
            for problem in &problems {
                eprintln!("  {problem}");
            }
            return ExitCode::FAILURE;
        }
    };

    match command.as_str() {
        "validate" => {
            println!(
                "{} consoles, {} controller profiles, {} components — all valid",
                catalog.consoles().count(),
                catalog.profiles().count(),
                catalog.components().count()
            );
            ExitCode::SUCCESS
        }
        "list" => {
            for (id, entry) in catalog.consoles() {
                let intent = entry
                    .console
                    .support
                    .iter()
                    .map(|(target, intent)| format!("{target}={intent:?}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "{id:<20}{:<34}{intent}",
                    entry.console.controllers.default
                );
            }
            ExitCode::SUCCESS
        }
        "generate" => match generate(&catalog, &repository_root()) {
            Ok(written) => {
                for path in written {
                    println!("wrote {}", path.display());
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("could not generate: {error}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("unknown command '{other}'; expected validate, list or generate");
            ExitCode::FAILURE
        }
    }
}

/// Generate the registries from the packages.
///
/// Keep the field order and formatting of the checked-in files exactly, so a
/// diff after generating them again shows a change in content, not noise.
fn generate(catalog: &Catalog, repository: &Path) -> Result<Vec<PathBuf>, String> {
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
        // For a console with no checksum catalogue we write no field at all.
        if let Some(reference) = &console.metadata.catalog {
            system.insert("catalog".into(), json!(reference.name));
        }
        let cores: Vec<Value> = console
            .cores
            .iter()
            .filter_map(|binding| {
                let component = catalog.component(&binding.component)?;
                Some(json!({
                    "filename": component.artifacts.get("macos-arm64")?,
                    "component": component.id,
                    "license": component.license.spdx,
                    "licenseFile": component.license.file,
                }))
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
                    (control.callout_x.unwrap_or(0), control.callout_y.unwrap_or(0))
                };
                json!({
                    "id": control.id,
                    "label": control.label,
                    "key": control.key,
                    "x": control.x.unwrap_or(0),
                    "y": control.y.unwrap_or(0),
                    "calloutX": callout_x,
                    "calloutY": callout_y,
                })
            })
            .collect();
        entry.insert("controls".into(), Value::Array(controls));
        if let Some(device) = profile.core_device {
            entry.insert("coreDevice".into(), json!(device));
        }
        profiles.push(Value::Object(entry));
    }

    let mut written = Vec::new();
    for (name, value) in [
        ("systems.json", json!({ "version": 1, "systems": systems })),
        ("controls.json", json!({ "profiles": profiles })),
    ] {
        let path = repository.join("desktop").join(name);
        let mut text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        text.push('\n');
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        written.push(path);
    }
    Ok(written)
}
