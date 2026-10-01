//! The packages must describe exactly what the checked-in registries
//! describe.
//!
//! We read `desktop/systems.json` and `desktop/controls.json` in code that
//! does not read the catalog, so any difference between them and the
//! packages is a defect, whichever side is wrong.

use rominabox_catalog::{model::Presentation, Catalog, BUILTIN_GENERIC_PROFILE, PACKAGE_ROOT};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // The rule is in the crate, so its binary and its tests cannot drift apart.
    rominabox_catalog::repo_root()
        .canonicalize()
        .expect("repository root")
}

fn catalog() -> Catalog {
    let root = repo_root().join(PACKAGE_ROOT);
    match Catalog::load(&root) {
        Ok(catalog) => catalog,
        Err(problems) => {
            let report: Vec<String> = problems.iter().map(ToString::to_string).collect();
            panic!(
                "the shipped packages do not validate:\n{}",
                report.join("\n")
            );
        }
    }
}

fn legacy(name: &str) -> Value {
    let path = repo_root().join("desktop").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("registry is readable"))
        .expect("registry is valid JSON")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_packages_describe_the_same_consoles_as_systems_json() {
    let catalog = catalog();
    let systems = legacy("systems.json");
    let systems = systems["systems"].as_array().expect("systems array");

    let package_ids: BTreeSet<String> = catalog.consoles().map(|(id, _)| id.clone()).collect();
    let legacy_ids: BTreeSet<String> = systems
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(package_ids, legacy_ids, "console ids must match exactly");

    for system in systems {
        let id = system["id"].as_str().unwrap();
        let console = &catalog.console(id).expect("ported console").console;

        assert_eq!(console.name, system["name"].as_str().unwrap(), "{id} name");
        assert_eq!(console.aliases, strings(&system["aliases"]), "{id} aliases");
        assert_eq!(
            console.content.extensions,
            strings(&system["extensions"]),
            "{id} extensions"
        );
        assert_eq!(
            console.content.category,
            system["category"].as_str().unwrap_or("cartridge"),
            "{id} category"
        );

        // We identify a dropped ROM through its checksum catalogue, so losing
        // or renaming a catalogue would cost every game its metadata unnoticed.
        let declared = console.metadata.catalog.as_ref().map(|c| c.name.as_str());
        assert_eq!(
            declared,
            system["catalog"].as_str(),
            "{id} checksum catalogue"
        );

        let legacy_components: Vec<String> = system["cores"]
            .as_array()
            .map(|cores| {
                cores
                    .iter()
                    .map(|c| c["component"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let ported: Vec<String> = console.cores.iter().map(|c| c.component.clone()).collect();
        assert_eq!(ported, legacy_components, "{id} cores, in preference order");

        let expected_profile = system["controllerProfile"]
            .as_str()
            .unwrap_or(BUILTIN_GENERIC_PROFILE);
        assert_eq!(
            console.controllers.default, expected_profile,
            "{id} default controller profile"
        );
    }
}

#[test]
fn every_core_declared_by_a_console_is_owned_by_exactly_one_package() {
    let catalog = catalog();
    let systems = legacy("systems.json");

    let mut legacy_components: BTreeMap<String, (Value, String)> = BTreeMap::new();
    for system in systems["systems"].as_array().unwrap() {
        for core in system["cores"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            legacy_components.insert(
                core["component"].as_str().unwrap().to_string(),
                (
                    // The registry has every declared target, so we compare the
                    // whole map and not the filename for one platform.
                    core["artifacts"].clone(),
                    core["license"].as_str().unwrap().to_string(),
                ),
            );
        }
    }

    for (id, (artifacts, license)) in &legacy_components {
        let component = catalog
            .component(id)
            .unwrap_or_else(|| panic!("component '{id}' should be declared by some package"));
        assert_eq!(
            &serde_json::to_value(&component.artifacts).unwrap(),
            artifacts,
            "{id} artifacts, for every declared target"
        );
        // Several cores are non-commercial, which limits what people may build
        // from them, so the packages must keep that term.
        assert_eq!(&component.license.spdx, license, "{id} licence");
    }

    let ported: BTreeSet<String> = catalog.components().map(|(id, _)| id.clone()).collect();
    let expected: BTreeSet<String> = legacy_components.keys().cloned().collect();
    assert_eq!(ported, expected, "no component may be invented or dropped");
}

#[test]
fn every_controller_profile_survives_with_its_exact_layout() {
    let catalog = catalog();
    let controls = legacy("controls.json");
    let profiles = controls["profiles"].as_array().expect("profiles array");

    for legacy_profile in profiles {
        let id = legacy_profile["id"].as_str().unwrap();
        let ported = catalog
            .profile(id)
            .unwrap_or_else(|| panic!("controller profile '{id}' should exist in the catalog"));

        assert_eq!(
            ported.name,
            legacy_profile["name"].as_str().unwrap(),
            "{id} name"
        );
        assert_eq!(
            ported.core_device,
            legacy_profile["coreDevice"].as_u64().map(|v| v as u32),
            "{id} emulated device"
        );

        let image = legacy_profile["image"].as_str().unwrap_or_default();
        match (&ported.presentation, image.is_empty()) {
            (
                Presentation::Illustrated {
                    image: ported_image,
                },
                false,
            ) => {
                assert_eq!(ported_image, image, "{id} illustration")
            }
            (Presentation::Generic, true) => {}
            (presentation, _) => panic!("{id} presentation changed: {presentation:?} vs '{image}'"),
        }

        let legacy_controls = legacy_profile["controls"].as_array().unwrap();
        assert_eq!(
            ported.controls.len(),
            legacy_controls.len(),
            "{id} control count"
        );
        for (ported_control, legacy_control) in ported.controls.iter().zip(legacy_controls) {
            let control_id = legacy_control["id"].as_str().unwrap();
            assert_eq!(ported_control.id, control_id, "{id} control order");
            assert_eq!(
                ported_control.label,
                legacy_control["label"].as_str().unwrap(),
                "{id}.{control_id} label"
            );
            assert_eq!(
                ported_control.key,
                legacy_control["key"].as_str().unwrap(),
                "{id}.{control_id} default binding"
            );
            // Anchors mean something only for an illustrated pad. We never draw
            // them in the generic grid, and the registry has 0,0 there.
            //
            // A grouped control has no anchor of its own either. A stick's four
            // directions and its click share the anchor of the group, because
            // we draw one marker per stick in the scene, not five. We write
            // that absence as 0 in the registry, so a comparison here would
            // check the placeholder and not the declaration.
            let grouped = ported_control.group.is_some();
            if !image.is_empty() && !grouped {
                assert_eq!(
                    (ported_control.x, ported_control.y),
                    (
                        legacy_control["x"].as_i64().map(|v| v as i32),
                        legacy_control["y"].as_i64().map(|v| v as i32)
                    ),
                    "{id}.{control_id} button anchor"
                );
                assert_eq!(
                    (ported_control.callout_x, ported_control.callout_y),
                    (
                        legacy_control["calloutX"].as_i64().map(|v| v as i32),
                        legacy_control["calloutY"].as_i64().map(|v| v as i32)
                    ),
                    "{id}.{control_id} callout position"
                );
            }
        }
    }

    let ported: BTreeSet<String> = catalog.profiles().map(|(id, _)| id.clone()).collect();
    let expected: BTreeSet<String> = profiles
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ported, expected, "no profile may be invented or dropped");
}

#[test]
fn a_console_offers_the_same_controller_variants_it_did_before() {
    let catalog = catalog();
    let controls = legacy("controls.json");

    for (id, _) in catalog.consoles() {
        let mut expected: Vec<String> = controls["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| {
                p["systems"]
                    .as_array()
                    .map(|s| s.iter().any(|v| v.as_str() == Some(id.as_str())))
                    .unwrap_or(false)
            })
            .map(|p| p["id"].as_str().unwrap().to_string())
            .collect();
        if expected.is_empty() {
            // For consoles with no pad of their own we choose the generic
            // profile on purpose, and the field is not merely missing.
            expected.push(BUILTIN_GENERIC_PROFILE.to_string());
        }

        let offered: BTreeSet<String> = catalog
            .controller_variants(id)
            .iter()
            .map(|p| p.id.clone())
            .collect();
        assert_eq!(
            offered,
            expected.into_iter().collect::<BTreeSet<_>>(),
            "{id} controller variants"
        );
    }
}

#[test]
fn an_alias_resolves_to_the_same_console_it_always_did() {
    let catalog = catalog();
    let systems = legacy("systems.json");

    for system in systems["systems"].as_array().unwrap() {
        let id = system["id"].as_str().unwrap();
        for alias in strings(&system["aliases"])
            .iter()
            .chain(std::iter::once(&id.to_string()))
        {
            let resolved = catalog
                .find(alias)
                .unwrap_or_else(|| panic!("'{alias}' should resolve"));
            assert_eq!(resolved.console.id, id, "'{alias}' resolves to {id}");
        }
    }
}

const SHIPPED_TARGETS: &[&str] = &[
    "linux-x86_64",
    "macos-arm64",
    "macos-x86_64",
    "windows-x86_64",
];

/// We generate the builder's download list from the packages.
///
/// With a second copy that could drift, we could fetch a core under a hash
/// that is no longer in the package.
#[test]
fn the_core_pins_are_the_packages() {
    let catalog = catalog();
    let pins: serde_json::Value = serde_json::from_str(include_str!("../../../core-pins.json"))
        .expect("desktop/core-pins.json parses");
    let listed: Vec<&serde_json::Value> = pins["cores"].as_array().unwrap().iter().collect();
    let mut expected = 0;
    for (id, component) in catalog.components() {
        let provenance = component
            .provenance
            .as_ref()
            .unwrap_or_else(|| panic!("{id} has no provenance"));
        expected += 1;
        let entry = listed
            .iter()
            .find(|item| item["component"] == *id)
            .unwrap_or_else(|| panic!("{id} is missing from core-pins.json"));
        assert_eq!(entry["repository"], provenance.repository);
        assert_eq!(entry["licenseFile"], component.license.file);
        assert_eq!(entry["licensePath"], provenance.license_candidates[0]);
        assert!(entry.get("revision").is_none());
        assert!(entry.get("licenseSha256").is_none());
        assert_eq!(
            entry["licenseRef"],
            provenance.branch.as_deref().unwrap(),
            "{id} licence ref"
        );
        let license_ref = entry["licenseRef"].as_str().unwrap();
        assert!(
            license_ref.len() < 40,
            "{id} licence ref is a commit, not the current branch"
        );
        for target in SHIPPED_TARGETS {
            let artifact = &entry["artifacts"][target];
            assert_eq!(artifact["filename"], component.artifacts[*target]);
            assert!(artifact.get("archiveSha256").is_none());
            assert!(artifact.get("binarySha256").is_none());
        }
    }
    assert_eq!(listed.len(), expected);
    assert!(pins["coreMirrors"].as_array().unwrap().len() >= 2);
}

/// We cannot fetch a component with no provenance, and a component must name
/// every target and not only macOS.
#[test]
fn every_component_can_be_obtained_for_every_shipped_target() {
    let catalog = catalog();
    let mut problems = Vec::new();
    for (id, component) in catalog.components() {
        match &component.provenance {
            None => problems.push(format!("{id}: no provenance")),
            Some(provenance) => match provenance.branch.as_deref() {
                Some(branch) if branch != provenance.revision && branch.len() < 40 => {}
                _ => problems.push(format!(
                    "{id}: licence is still a fixed commit, not the current branch"
                )),
            },
        }
        for target in SHIPPED_TARGETS {
            if !component.artifacts.contains_key(*target) {
                problems.push(format!("{id}: no {target} artifact"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "cores that cannot be obtained:\n{}",
        problems.join("\n")
    );
}

/// The checked-in registries must match what we generate from the catalog.
///
/// We generate `systems.json` and `controls.json`, so do not edit them by
/// hand. After a failure here, either generate them again with
///
///     cargo run --bin rominabox-catalog -- generate
///
/// or make the change in the package where it belongs.
#[test]
fn the_checked_in_registries_are_what_the_catalog_generates() {
    let catalog = catalog();
    let generated =
        rominabox_catalog::compatibility_registries(&catalog).expect("registries render");

    for (name, expected) in generated {
        let path = repo_root().join("desktop").join(name);
        let actual = std::fs::read_to_string(&path).expect("registry is readable");
        assert_eq!(
            actual, expected,
            "{name} has drifted from the packages; regenerate it instead of editing it by hand"
        );
    }
}
