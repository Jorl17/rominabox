//! We load only complete packages, and report every reason a package is not.
//!
//! In each test we build a broken package under a temporary root and check
//! the diagnostic `code`, `package` and `field`. A diagnostic at load time
//! has the package directory, and one from a check across files has the
//! console id. We never return a `Catalog` for a broken root, so we never
//! use the built-in `retropad` in place of a missing declaration.

use rominabox_catalog::{Catalog, Diagnostic};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-catalog-invalid")
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn write_json(path: &Path, value: &Value) {
    write(path, &serde_json::to_string_pretty(value).unwrap());
}

fn package_dir(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn one(name: &str) -> (rominabox_scratch::Scratch, PathBuf) {
    let root = scratch();
    let package = package_dir(&root, name);
    (root, package)
}

/// A console valid on its own, with one extension and the built-in profile.
fn plain(id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "id": id,
        "name": id,
        "content": {
            "extensions": ["rom"],
            "category": "cartridge"
        },
        "controllers": {
            "default": "retropad",
            "variants": ["retropad"]
        }
    })
}

fn offering(id: &str, profile: &str) -> Value {
    let mut manifest = plain(id);
    manifest["controllers"] = json!({
        "default": profile,
        "variants": [profile]
    });
    manifest
}

fn write_console(package: &Path, manifest: &Value) {
    write_json(&package.join("console.json"), manifest);
}

fn profile(id: &str, presentation: Value, controls: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 1,
        "id": id,
        "name": id,
        "presentation": presentation,
        "controls": controls
    })
}

fn generic(id: &str, controls: Vec<Value>) -> Value {
    profile(id, json!({ "kind": "generic" }), controls)
}

fn illustrated(id: &str, image: &str, controls: Vec<Value>) -> Value {
    profile(
        id,
        json!({ "kind": "illustrated", "image": image }),
        controls,
    )
}

fn button(id: &str) -> Value {
    json!({ "id": id, "label": id, "key": "a" })
}

fn anchored(id: &str) -> Value {
    json!({ "id": id, "label": id, "key": "a", "x": 10, "y": 20 })
}

fn write_profile(package: &Path, value: &Value) {
    let id = value["id"].as_str().unwrap();
    write_json(&package.join("controllers").join(format!("{id}.json")), value);
}

fn render(problems: &[Diagnostic]) -> String {
    problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn reject(root: &Path) -> Vec<Diagnostic> {
    match Catalog::load(root) {
        Ok(_) => panic!("broken package produced a catalog instead of diagnostics"),
        Err(problems) => problems,
    }
}

fn assert_sole(root: &Path, code: &str, package: &str, field: &str) {
    let problems = reject(root);
    assert_eq!(
        problems.len(),
        1,
        "expected only {code}, got:\n{}",
        render(&problems)
    );
    let diagnostic = &problems[0];
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.package, package);
    assert_eq!(diagnostic.field, field);
}

fn assert_has(problems: &[Diagnostic], code: &str, package: &str, field: &str) {
    let diagnostic = problems
        .iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("no {code} in:\n{}", render(problems)));
    assert_eq!(diagnostic.package, package, "{code} package");
    assert_eq!(diagnostic.field, field, "{code} field");
}

#[test]
fn a_package_directory_without_console_json_is_rejected() {
    let root = scratch();
    let package = package_dir(&root, "orphan");
    write(&package.join("README"), "present, but not a manifest");
    assert_sole(&root, "package.no_manifest", "orphan", "console.json");
}

#[test]
fn console_json_that_is_not_valid_json_is_rejected() {
    let (root, package) = one("broken");
    write(&package.join("console.json"), "{ this is not json");
    assert_sole(&root, "parse.invalid_json", "broken", "console.json");
}

#[test]
fn a_schema_version_this_build_does_not_understand_is_rejected() {
    let (root, package) = one("future");
    let mut manifest = plain("future");
    manifest["schemaVersion"] = json!(2);
    write_console(&package, &manifest);
    assert_sole(&root, "schema.unsupported_version", "future", "schemaVersion");
}

#[test]
fn two_packages_cannot_declare_the_same_console_id() {
    let root = scratch();
    // `later` sorts after `earlier`, so we report the collision in `later`.
    // Both manifests declare the console id `shared`.
    write_console(&package_dir(&root, "earlier"), &plain("shared"));
    write_console(&package_dir(&root, "later"), &plain("shared"));
    assert_sole(&root, "id.duplicate", "later", "id");
}

#[test]
fn two_packages_cannot_declare_the_same_controller_profile_id() {
    let root = scratch();
    let earlier = package_dir(&root, "earlier");
    let later = package_dir(&root, "later");
    let pad = generic("sixbutton", vec![button("a")]);
    write_console(&earlier, &offering("earlier", "sixbutton"));
    write_console(&later, &offering("later", "sixbutton"));
    write_profile(&earlier, &pad);
    write_profile(&later, &pad);
    assert_sole(&root, "id.duplicate", "later", "controllers");
}

#[test]
fn a_package_cannot_redeclare_the_builtin_retropad_profile() {
    let (root, package) = one("handheld");
    write_console(&package, &plain("handheld"));
    write_profile(&package, &generic("retropad", vec![button("a")]));
    assert_sole(&root, "id.duplicate", "handheld", "controllers");
}

#[test]
fn an_alias_cannot_collide_with_another_consoles_id() {
    let root = scratch();
    write_console(&package_dir(&root, "alpha"), &plain("alpha"));
    let mut beta = plain("beta");
    beta["aliases"] = json!(["alpha"]);
    write_console(&package_dir(&root, "beta"), &beta);
    assert_sole(&root, "alias.duplicate", "beta", "aliases");
}

#[test]
fn an_alias_cannot_collide_with_another_consoles_alias() {
    let root = scratch();
    let mut alpha = plain("alpha");
    alpha["aliases"] = json!(["pocket"]);
    let mut beta = plain("beta");
    beta["aliases"] = json!(["pocket"]);
    write_console(&package_dir(&root, "alpha"), &alpha);
    write_console(&package_dir(&root, "beta"), &beta);
    assert_sole(&root, "alias.duplicate", "beta", "aliases");
}

#[test]
fn a_missing_controller_profile_is_not_replaced_with_retropad() {
    // The manifest lists `no-such-pad`. If we used the built-in profile instead,
    // we would return Ok(catalog). The load must fail with no catalog at all.
    let (root, package) = one("handy");
    write_console(&package, &offering("handy", "no-such-pad"));
    assert_sole(&root, "reference.missing_profile", "handy", "controllers");
}

#[test]
fn a_console_referencing_an_undeclared_component_is_rejected() {
    let (root, package) = one("disc");
    let mut manifest = plain("disc");
    manifest["cores"] = json!([{ "component": "no_such_core" }]);
    write_console(&package, &manifest);
    assert_sole(&root, "reference.missing_component", "disc", "cores");
}

/// A core comes from a build of ours or from libretro's nightly. Any other
/// origin is a typo, which we would have taken for a download in preparation.
#[test]
fn a_component_from_an_unknown_origin_is_rejected() {
    let (root, package) = one("cartridge");
    write_console(&package, &plain("cartridge"));
    write_json(
        &package.join("components/core.json"),
        &json!({
            "schemaVersion": 1,
            "id": "core",
            "name": "core",
            "artifacts": { "windows-x86_64": "core_libretro.dll" },
            "license": { "spdx": "GPL-2.0", "file": "core.txt" },
            "capabilities": [],
            "provenance": {
                "origin": "buildbot",
                "repository": "libretro/core",
                "revision": "0000000000000000000000000000000000000000",
                "branch": "master",
                "licenseCandidates": ["COPYING"],
                "correspondsToArtifact": false
            }
        }),
    );
    assert_sole(&root, "parse.invalid_json", "cartridge", "components/core.json");
}

#[test]
fn a_default_profile_must_be_among_the_declared_variants() {
    let (root, package) = one("md");
    let mut manifest = plain("md");
    manifest["controllers"] = json!({
        "default": "retropad",
        "variants": ["sixbutton"]
    });
    write_console(&package, &manifest);
    write_profile(&package, &generic("sixbutton", vec![button("a")]));
    assert_sole(
        &root,
        "controller.default_not_offered",
        "md",
        "controllers.default",
    );
}

#[test]
fn an_illustrated_profile_whose_png_does_not_exist_is_rejected() {
    let (root, package) = one("nes");
    write_console(&package, &offering("nes", "nes"));
    write_profile(
        &package,
        &illustrated("nes", "nes.png", vec![anchored("a")]),
    );
    assert_sole(
        &root,
        "controller.illustration_missing",
        "nes",
        "nes.presentation.image",
    );
}

#[test]
fn an_illustrated_control_missing_anchors_is_rejected() {
    let (root, package) = one("snes");
    write_console(&package, &offering("snes", "snes"));
    write_profile(&package, &illustrated("snes", "snes.png", vec![button("a")]));
    write(&package.join("controllers").join("snes.png"), "");
    assert_sole(
        &root,
        "controller.anchor_missing",
        "snes",
        "snes.controls.a",
    );
}

#[test]
fn a_control_id_outside_the_retropad_vocabulary_is_rejected() {
    let (root, package) = one("arcade");
    write_console(&package, &offering("arcade", "stick"));
    write_profile(&package, &generic("stick", vec![button("coin")]));
    assert_sole(&root, "control.unknown_id", "arcade", "stick.controls");
}

#[test]
fn the_same_control_id_cannot_be_declared_twice_in_one_profile() {
    let (root, package) = one("pce");
    write_console(&package, &offering("pce", "pce"));
    write_profile(&package, &generic("pce", vec![button("b"), button("b")]));
    assert_sole(&root, "control.duplicate_id", "pce", "pce.controls");
}

fn stick_member(id: &str, direction: Option<&str>) -> Value {
    let mut member = json!({ "id": id, "label": id, "key": "a", "group": "l_stick" });
    if let Some(direction) = direction {
        member["direction"] = json!(direction);
    }
    member
}

/// A generic pad of `members`, with a title for its stick.
fn stick_pad(members: Vec<Value>) -> Value {
    let mut pad = generic("pad", members);
    pad["groups"] = json!({ "l_stick": { "title": "Stick" } });
    pad
}

#[test]
fn a_stick_member_without_a_direction_is_rejected() {
    let (root, package) = one("pad");
    write_console(&package, &offering("pad", "pad"));
    write_profile(
        &package,
        &stick_pad(vec![stick_member("l_y_minus", Some("up")), stick_member("l_x_plus", None)]),
    );
    assert_sole(&root, "control.stick_direction_missing", "pad", "pad.controls[group=l_stick]");
}

#[test]
fn a_stick_declares_its_directions_once_in_capture_order() {
    let (root, package) = one("pad");
    write_console(&package, &offering("pad", "pad"));
    write_profile(
        &package,
        &stick_pad(vec![stick_member("l_y_plus", Some("down")), stick_member("l_y_minus", Some("up"))]),
    );
    assert_sole(&root, "control.stick_direction_order", "pad", "pad.controls[group=l_stick]");
}

/// We show a stick's title in its box and in the builder's table, so
/// every stick must have one, and a blank title counts as none.
#[test]
fn a_stick_without_a_title_is_rejected() {
    let members = || vec![stick_member("l_y_minus", Some("up")), stick_member("l_x_plus", Some("right"))];
    for groups in [json!(null), json!({ "l_stick": { "title": "  " } })] {
        let (root, package) = one("pad");
        write_console(&package, &offering("pad", "pad"));
        let mut pad = generic("pad", members());
        if !groups.is_null() {
            pad["groups"] = groups;
        }
        write_profile(&package, &pad);
        assert_sole(&root, "controller.group_untitled", "pad", "pad.groups.l_stick");
    }
}

#[test]
fn a_title_for_a_stick_the_pad_does_not_have_is_rejected() {
    let (root, package) = one("pad");
    write_console(&package, &offering("pad", "pad"));
    let mut pad = stick_pad(vec![stick_member("l_y_minus", Some("up"))]);
    pad["groups"]["r_stick"] = json!({ "title": "C-stick" });
    write_profile(&package, &pad);
    assert_sole(&root, "controller.group_unused", "pad", "pad.groups.r_stick");
}

#[test]
fn only_a_stick_member_has_a_direction() {
    let (root, package) = one("pad");
    write_console(&package, &offering("pad", "pad"));
    let mut lone = button("up");
    lone["direction"] = json!("up");
    write_profile(&package, &generic("pad", vec![lone]));
    assert_sole(&root, "control.direction_outside_stick", "pad", "pad.controls.up");
}

#[test]
fn a_console_declaring_no_content_extensions_is_rejected() {
    let (root, package) = one("empty");
    let mut manifest = plain("empty");
    manifest["content"]["extensions"] = json!([]);
    write_console(&package, &manifest);
    assert_sole(&root, "content.no_extensions", "empty", "content.extensions");
}

#[test]
fn an_asset_path_cannot_escape_its_package() {
    let outside = rominabox_scratch::Scratch::reserve("rominabox-catalog-outside");
    let absolute = outside.to_str().expect("temp dir is utf-8");
    for image in ["../outside.png", absolute] {
        let (root, package) = one("art");
        write_console(&package, &offering("art", "art"));
        write_profile(&package, &illustrated("art", image, vec![anchored("a")]));
        assert_sole(
            &root,
            "asset.escapes_package",
            "art",
            "art.presentation.image",
        );
    }
}

#[test]
fn one_package_reports_every_fault_in_a_single_load() {
    let (root, package) = one("half");
    let mut manifest = plain("half");
    manifest["content"]["extensions"] = json!([]);
    manifest["controllers"] = json!({
        "default": "missing-pad",
        "variants": ["missing-pad"]
    });
    write_console(&package, &manifest);

    let problems = reject(&root);
    assert_eq!(
        problems.len(),
        2,
        "both faults should be reported together:\n{}",
        render(&problems)
    );
    assert_has(&problems, "content.no_extensions", "half", "content.extensions");
    assert_has(&problems, "reference.missing_profile", "half", "controllers");
}
