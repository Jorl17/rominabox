//! We load a console unknown to the crate from its directory alone.
//!
//! These packages are in a temporary root, under ids that appear in no
//! production source. With these tests we check that we add a new console
//! without a change to the loader. Discovery, the built-in pad, a private profile, a
//! private core and the reuse of ids from another package all come from files.

use rominabox_catalog::model::{Presentation, SCHEMA_VERSION};
use rominabox_catalog::{Catalog, Diagnostic, BUILTIN_GENERIC_PROFILE};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// A complete 1×1 PNG (signature, IHDR, IDAT, IEND). An empty file would still
/// satisfy `is_file`, and would not be the asset an illustrated profile declares.
const MINIMAL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

struct ConsoleDecl<'a> {
    id: &'a str,
    name: &'a str,
    aliases: &'a [&'a str],
    extensions: &'a [&'a str],
    category: &'a str,
    default_profile: &'a str,
    variants: &'a [&'a str],
    cores: &'a [&'a str],
}

struct AnchoredControl {
    id: &'static str,
    label: &'static str,
    key: &'static str,
    x: i32,
    y: i32,
    callout_x: i32,
    callout_y: i32,
}

/// Not the zeroes of the built-in grid, so we notice an anchor we lost.
const TESTBOX_PAD: &[AnchoredControl] = &[
    AnchoredControl {
        id: "b",
        label: "B",
        key: "z",
        x: 40,
        y: 180,
        callout_x: 16,
        callout_y: 120,
    },
    AnchoredControl {
        id: "a",
        label: "A",
        key: "x",
        x: 96,
        y: 176,
        callout_x: 160,
        callout_y: 120,
    },
];

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-catalog-extension")
}

fn write_console(root: &Path, directory: &str, console: ConsoleDecl<'_>) -> PathBuf {
    let dir = root.join(directory);
    fs::create_dir_all(&dir).expect("package directory");
    let mut body = json!({
        "schemaVersion": SCHEMA_VERSION,
        "id": console.id,
        "name": console.name,
        "aliases": console.aliases,
        "content": {
            "extensions": console.extensions,
            "category": console.category,
        },
        "controllers": {
            "default": console.default_profile,
            "variants": console.variants,
        },
    });
    if !console.cores.is_empty() {
        body["cores"] = Value::Array(
            console
                .cores
                .iter()
                .map(|component| json!({ "component": component }))
                .collect(),
        );
    }
    fs::write(
        dir.join("console.json"),
        serde_json::to_vec_pretty(&body).expect("console.json"),
    )
    .expect("write console.json");
    dir
}

/// We resolve an illustration from the directory of the profile file, so we
/// write the PNG beside the controller JSON.
fn write_illustrated_profile(package: &Path, id: &str, name: &str, image: &str) -> PathBuf {
    let controllers = package.join("controllers");
    fs::create_dir_all(&controllers).expect("controllers directory");
    let controls: Vec<Value> = TESTBOX_PAD
        .iter()
        .map(|control| {
            json!({
                "id": control.id,
                "label": control.label,
                "key": control.key,
                "x": control.x,
                "y": control.y,
                "calloutX": control.callout_x,
                "calloutY": control.callout_y,
            })
        })
        .collect();
    let body = json!({
        "schemaVersion": SCHEMA_VERSION,
        "id": id,
        "name": name,
        "presentation": { "kind": "illustrated", "image": image },
        "controls": controls,
    });
    fs::write(
        controllers.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&body).expect("controller profile"),
    )
    .expect("write controller profile");
    let png = controllers.join(image);
    fs::write(&png, MINIMAL_PNG).expect("write illustration");
    png
}

fn write_generic_profile(package: &Path, id: &str, name: &str) {
    let controllers = package.join("controllers");
    fs::create_dir_all(&controllers).expect("controllers directory");
    let body = json!({
        "schemaVersion": SCHEMA_VERSION,
        "id": id,
        "name": name,
        "presentation": { "kind": "generic" },
        "controls": [{ "id": "a", "label": "A", "key": "x" }],
    });
    fs::write(
        controllers.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&body).expect("generic profile"),
    )
    .expect("write generic profile");
}

fn write_component(package: &Path, id: &str, name: &str) {
    let components = package.join("components");
    fs::create_dir_all(&components).expect("components directory");
    let body = json!({
        "schemaVersion": SCHEMA_VERSION,
        "id": id,
        "name": name,
        "artifacts": { "macos-arm64": format!("{id}.dylib") },
        "license": { "spdx": "MIT", "file": format!("{id}.txt") },
    });
    fs::write(
        components.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&body).expect("component"),
    )
    .expect("write component");
}

fn expect_catalog(root: &Path) -> Catalog {
    Catalog::load(root).unwrap_or_else(|problems| {
        panic!(
            "synthetic catalog should load:\n{}",
            problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

fn entry_names(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .expect("read package directory")
        .map(|entry| entry.expect("directory entry").file_name().to_string_lossy().into_owned())
        .collect()
}

fn variant_ids(catalog: &Catalog, console: &str) -> Vec<String> {
    catalog
        .controller_variants(console)
        .iter()
        .map(|profile| profile.id.clone())
        .collect()
}

fn illustrated_testbox(root: &Path) -> (PathBuf, PathBuf) {
    let dir = write_console(
        root,
        "testbox",
        ConsoleDecl {
            id: "testbox",
            name: "Test Box",
            aliases: &["Test Box"],
            extensions: &["tbx"],
            category: "cartridge",
            default_profile: "testboxpad",
            variants: &["testboxpad"],
            cores: &[],
        },
    );
    let png = write_illustrated_profile(&dir, "testboxpad", "Test Box Pad", "testboxpad.png");
    (dir, png)
}

#[test]
fn a_dropped_in_console_is_discovered_by_id_and_every_alias() {
    // We store the console id after recognition, but files and players use
    // aliases. If we cannot find a package under each name it declares, then
    // adding it did not work.
    let root = scratch();
    write_console(
        &root,
        "testbox",
        ConsoleDecl {
            id: "testbox",
            name: "Test Box",
            aliases: &["Test Box", "T-Box"],
            extensions: &["tbx", "tbox"],
            category: "disc",
            default_profile: BUILTIN_GENERIC_PROFILE,
            variants: &[BUILTIN_GENERIC_PROFILE],
            cores: &[],
        },
    );

    let catalog = expect_catalog(&root);
    let console = &catalog
        .console("testbox")
        .expect("console id is a catalog key")
        .console;
    assert_eq!(console.name, "Test Box");
    assert_eq!(
        console.aliases,
        vec!["Test Box".to_string(), "T-Box".to_string()]
    );
    assert_eq!(
        console.content.extensions,
        vec!["tbx".to_string(), "tbox".to_string()]
    );
    assert_eq!(console.content.category, "disc");
    assert_eq!(
        variant_ids(&catalog, "testbox"),
        vec![BUILTIN_GENERIC_PROFILE.to_string()]
    );

    for needle in ["testbox", "Test Box", "T-Box"] {
        let found = catalog
            .find(needle)
            .unwrap_or_else(|| panic!("'{needle}' should resolve to the dropped-in console"));
        assert_eq!(found.console.id, "testbox", "'{needle}'");
    }
}

#[test]
fn a_single_console_json_is_usable_through_the_builtin_retropad() {
    // Most consoles need neither a drawn pad nor a private core. We put
    // RetroPad into every catalog so that this package can be one file. If we
    // required a controllers directory in the loader, we would reject it.
    let root = scratch();
    let dir = write_console(
        &root,
        "testbox",
        ConsoleDecl {
            id: "testbox",
            name: "Test Box",
            aliases: &[],
            extensions: &["tbx"],
            category: "cartridge",
            default_profile: BUILTIN_GENERIC_PROFILE,
            variants: &[BUILTIN_GENERIC_PROFILE],
            cores: &[],
        },
    );
    assert_eq!(
        entry_names(&dir),
        BTreeSet::from(["console.json".to_string()])
    );

    let catalog = expect_catalog(&root);
    let offered = catalog.controller_variants("testbox");
    assert_eq!(offered.len(), 1, "the one file must offer a usable pad");
    let profile = offered[0];
    assert_eq!(profile.id, BUILTIN_GENERIC_PROFILE);
    assert_eq!(profile.name, "RetroPad");
    assert_eq!(profile.presentation, Presentation::Generic);
    assert!(
        profile.controls.iter().any(|control| control.id == "start"),
        "the built-in pad must expose bindable controls"
    );
    assert_eq!(
        catalog
            .find("testbox")
            .map(|entry| entry.console.id.as_str()),
        Some("testbox")
    );
}

#[test]
fn a_package_can_introduce_its_own_illustrated_controller() {
    // Illustrated pads are package data, not a set compiled into the crate.
    // We must read the anchors back unchanged, or we draw every button in the
    // menu in the wrong place.
    let root = scratch();
    let (_dir, png) = illustrated_testbox(&root);
    let bytes = fs::read(&png).expect("illustration readable");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "the declared illustration must be a PNG"
    );

    let catalog = expect_catalog(&root);
    let profile = catalog
        .profile("testboxpad")
        .expect("the package's own profile is addressable by id");
    assert_eq!(
        profile.presentation,
        Presentation::Illustrated {
            image: "testboxpad.png".into(),
        }
    );
    assert_eq!(variant_ids(&catalog, "testbox"), vec!["testboxpad".to_string()]);
    assert_eq!(profile.controls.len(), TESTBOX_PAD.len());
    for (got, expected) in profile.controls.iter().zip(TESTBOX_PAD) {
        assert_eq!(got.id, expected.id);
        assert_eq!(
            (got.x, got.y),
            (Some(expected.x), Some(expected.y)),
            "{} button anchor",
            expected.id
        );
        assert_eq!(
            (got.callout_x, got.callout_y),
            (Some(expected.callout_x), Some(expected.callout_y)),
            "{} callout",
            expected.id
        );
    }
}

#[test]
fn a_package_can_introduce_its_own_core_component() {
    // The core for a console is a binding to an id, and we resolve that id
    // through the component file. A core unknown to the crate must need no
    // change to the code.
    let root = scratch();
    let dir = write_console(
        &root,
        "testbox",
        ConsoleDecl {
            id: "testbox",
            name: "Test Box",
            aliases: &[],
            extensions: &["tbx"],
            category: "cartridge",
            default_profile: BUILTIN_GENERIC_PROFILE,
            variants: &[BUILTIN_GENERIC_PROFILE],
            cores: &["testbox_core"],
        },
    );
    write_component(&dir, "testbox_core", "Test Box Core");

    let catalog = expect_catalog(&root);
    let console = &catalog.console("testbox").expect("testbox").console;
    assert_eq!(console.cores.len(), 1);
    let resolved = catalog
        .component(&console.cores[0].component)
        .expect("the cores binding resolves to the component this package introduced");
    assert_eq!(resolved.id, "testbox_core");
    assert_eq!(resolved.name, "Test Box Core");
    assert_eq!(
        resolved.artifacts.get("macos-arm64").map(String::as_str),
        Some("testbox_core.dylib")
    );
}

#[test]
fn deleting_a_declared_illustration_is_a_hard_failure() {
    // A profile that lists an illustration without its file is a broken
    // package. If we loaded it as a generic pad, we would ship the console
    // without its declared artwork and say nothing.
    let root = scratch();
    let (dir, png) = illustrated_testbox(&root);
    let catalog = expect_catalog(&root);
    match &catalog.profile("testboxpad").expect("profile").presentation {
        Presentation::Illustrated { image } => assert_eq!(image, "testboxpad.png"),
        other => panic!("expected an illustrated profile before the delete, got {other:?}"),
    }

    fs::remove_file(&png).expect("remove the declared illustration");
    assert!(
        !png.exists(),
        "the failure has to be caused by removing the declared asset"
    );

    let problems = Catalog::load(&root).expect_err("a missing illustration is not a usable package");
    assert_eq!(
        problems.len(),
        1,
        "expected only the missing illustration, got {problems:?}"
    );
    let diagnostic: &Diagnostic = &problems[0];
    assert_eq!(diagnostic.code, "controller.illustration_missing");
    assert_eq!(
        diagnostic.package,
        dir.file_name().unwrap().to_str().unwrap(),
        "the diagnostic names the package that lost the asset"
    );
    assert_eq!(diagnostic.field, "testboxpad.presentation.image");
    assert!(
        diagnostic.message.contains("testboxpad.png"),
        "{}",
        diagnostic.message
    );
}

#[test]
fn a_package_can_reuse_another_packages_profile_and_component() {
    // The guest package contains neither the profile nor the component, and
    // the donor package does not mention the guest. We load both consoles
    // because we resolve ids across the root, without changes to the donor.
    let root = scratch();
    let donor = write_console(
        &root,
        "donor",
        ConsoleDecl {
            id: "donor",
            name: "Donor",
            aliases: &[],
            extensions: &["dnr"],
            category: "cartridge",
            default_profile: "donorpad",
            variants: &["donorpad"],
            cores: &["donorcore"],
        },
    );
    write_generic_profile(&donor, "donorpad", "Donor Pad");
    write_component(&donor, "donorcore", "Donor Core");
    let guest = write_console(
        &root,
        "testbox",
        ConsoleDecl {
            id: "testbox",
            name: "Test Box",
            aliases: &["tbx"],
            extensions: &["tbx"],
            category: "disc",
            default_profile: "donorpad",
            variants: &["donorpad"],
            cores: &["donorcore"],
        },
    );
    assert_eq!(
        entry_names(&guest),
        BTreeSet::from(["console.json".to_string()]),
        "reuse must not copy the donor's files into the guest"
    );

    let catalog = expect_catalog(&root);
    assert!(catalog.console("donor").is_some(), "the owner still loads");
    assert!(catalog.console("testbox").is_some(), "the guest loads beside it");
    assert_eq!(catalog.consoles().count(), 2);

    let guest_console = &catalog.console("testbox").expect("guest").console;
    let profile = catalog
        .profile(&guest_console.controllers.default)
        .expect("the guest's controller id resolves to the donor's profile");
    assert_eq!(profile.id, "donorpad");
    assert_eq!(profile.name, "Donor Pad");
    assert_eq!(variant_ids(&catalog, "testbox"), vec!["donorpad".to_string()]);

    let component = catalog
        .component(&guest_console.cores[0].component)
        .expect("the guest's core binding resolves to the donor's component");
    assert_eq!(component.id, "donorcore");
    assert_eq!(component.name, "Donor Core");
    assert_eq!(
        component.artifacts.get("macos-arm64").map(String::as_str),
        Some("donorcore.dylib")
    );
}
