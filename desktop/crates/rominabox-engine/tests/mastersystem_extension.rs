//! Master System is a console we ship that uses the core of another console.
//!
//! We declare Genesis Plus GX in the Mega Drive package, and Master System,
//! Sega CD, Game Gear and SG-1000 only name it. These tests follow the builder
//! steps of an export (system lookup, controller defaults and theme staging),
//! so a package that only parses does not pass.

mod support;

use rominabox_engine::{
    controls::{self, Controls},
    systems,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Consoles whose preferred core is the one Genesis Plus GX definition.
const GENESIS_PLUS_GX_CONSOLES: [&str; 5] =
    ["megadrive", "segacd", "mastersystem", "gamegear", "sg1000"];

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-mastersystem")
}

fn menu_assets() -> PathBuf {
    rominabox_engine::repo::at("integrations/designs/native")
}

fn controller_assets() -> PathBuf {
    rominabox_engine::repo::at("desktop/assets/controllers")
}

fn file_names(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn resolve(needle: &str) -> &'static systems::System {
    systems::find(needle).unwrap_or_else(|| panic!("'{needle}' should resolve to a console"))
}

#[test]
fn mastersystem_resolves_by_id_and_every_declared_alias() {
    // We store the id when we recognise a game, but files and people use
    // aliases. We must find a console under every name it declares, and we
    // recognise a ROM by the extension of the console before we know any of
    // those names.
    let system = resolve("mastersystem");
    assert_eq!(system.id, "mastersystem");
    assert_eq!(system.name, "Master System");
    assert_eq!(
        system.aliases,
        [
            "master system".to_string(),
            "sms".to_string(),
            "mark iii".to_string()
        ]
    );
    assert_eq!(system.extensions, ["sms".to_string()]);
    assert_eq!(system.category, "cartridge");

    let mut needles = vec![system.id.as_str()];
    needles.extend(system.aliases.iter().map(String::as_str));
    for needle in needles {
        assert_eq!(
            resolve(needle).id,
            "mastersystem",
            "'{needle}' must be this console, not a similarly named one"
        );
    }

    for extension in &system.extensions {
        let candidates: Vec<&str> = systems::candidates_for_extension(extension)
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect();
        assert_eq!(candidates, ["mastersystem"], ".{extension}");
    }
}

#[test]
fn mastersystem_selects_the_same_genesis_plus_gx_component_as_the_other_sega_consoles() {
    // Master System has no core of its own. We download one Genesis Plus GX
    // file once for all five consoles. With a private copy, or a different
    // artifact under the same id, we would download it again.
    let mastersystem = resolve("mastersystem");
    assert_eq!(mastersystem.cores.len(), 1, "no fallback core");
    let shared = mastersystem
        .preferred_core()
        .expect("Master System declares a core");
    assert_eq!(shared.component, "genesis_plus_gx");

    for id in GENESIS_PLUS_GX_CONSOLES {
        let system = resolve(id);
        assert_eq!(system.cores.len(), 1, "{id} must not carry a second core");
        let core = system.preferred_core().expect("{id} declares a core");
        assert_eq!(core.component, shared.component, "{id} component");
        assert_eq!(core.artifacts, shared.artifacts, "{id} artifact");
        assert_eq!(core.license, shared.license, "{id} licence");
        assert_eq!(core.license_file, shared.license_file, "{id} licence file");
        assert_eq!(core.capabilities, shared.capabilities, "{id} capabilities");
    }
}

#[test]
fn the_default_pad_is_illustrated_and_every_control_is_anchored() {
    // We place each hit target in the menu from the declared anchor. With the
    // origin of the generic grid as every anchor, we would draw every button
    // of an illustrated pad in the corner, and the load would look fine.
    let profile = controls::profile_for_system("mastersystem").expect("default profile");
    assert_eq!(profile.id, "mastersystem");
    assert_eq!(profile.image, "controller-mastersystem.png");

    let image = controller_assets().join(&profile.image);
    let bytes =
        fs::read(&image).unwrap_or_else(|error| panic!("read {}: {error}", image.display()));
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "the declared illustration must be the PNG on disk"
    );

    let expected = [
        ("up", 373, 172),
        ("down", 373, 232),
        ("left", 343, 202),
        ("right", 403, 202),
        ("b", 567, 230),
        ("a", 640, 230),
        ("start", 500, 138),
    ];
    assert_eq!(profile.controls.len(), expected.len());
    for (control, (id, x, y)) in profile.controls.iter().zip(expected) {
        assert_eq!(control.id, id);
        assert_eq!((control.x, control.y), (x, y), "{id} anchor");
    }
}

#[test]
fn an_author_can_pick_the_generic_retropad_and_get_an_asset_free_grid() {
    // The drawn pad is the default, and an author may pick RetroPad instead.
    // That author must still get a working grid, without the illustration
    // that this console has.
    let root = scratch();
    let options = Controls {
        profile: Some("retropad".to_string()),
        ..Controls::default()
    };
    support::stage_controls(
        &menu_assets(),
        &menu_assets(),
        &root,
        "mastersystem",
        &options,
        None,
    )
    .expect("RetroPad is offered for every console");
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(markup.contains("id=\"control-r3\""));
    assert!(!markup.contains("id=\"controller-image\""));
    // The document, the one scene we can swap to, and the sound and mark of
    // the menu, with no console artwork.
    assert_eq!(
        file_names(&root),
        BTreeSet::from([
            "menu.rml".to_string(),
            "menu.rcss".to_string(),
            "Silkscreen-Regular.ttf".to_string(),
            "Silkscreen-OFL.txt".to_string(),
            "ScienceGothic-Bold.ttf".to_string(),
            "ScienceGothic-OFL.txt".to_string(),
            rominabox_engine::volume::tick_file().to_string(),
            "parts".to_string(),
            "scene-retropad.rml".to_string(),
            // We stage the screen declarations with the menu.
            "design.cfg".to_string(),
            // The buttons that open the menu, and confirm and go back in it.
            "hotkeys-defaults.cfg".to_string(),
            // The design's own mark beside its name in the footer.
            "brand-mark.png".to_string(),
        ])
    );
    controls::write_defaults_config("mastersystem", &options, &root.join("controls.cfg")).unwrap();
    assert!(fs::read_to_string(root.join("controls.cfg"))
        .unwrap()
        .contains("input_player1_l2"));
}

#[test]
fn default_controls_config_carries_the_emulated_device_and_bindings() {
    // We set up the input of the core from this file, not from the picture of
    // the pad. The two face buttons of a Master System pad are 1 and 2, and
    // pause is Start. Without the declared device, the core gets a generic joypad.
    let root = scratch();
    let profile = controls::write_defaults_config(
        "mastersystem",
        &Controls::default(),
        &root.join("controls.cfg"),
    )
    .expect("default profile writes");
    assert_eq!(profile.id, "mastersystem");
    assert_eq!(profile.core_device, Some(769));

    let text = fs::read_to_string(root.join("controls.cfg")).unwrap();
    assert!(text.contains("controls_profile = \"mastersystem\""));
    // We write this key to a remap file, not to the config, because the key
    // works only in a remap file. See stage_controller_remap.
    assert!(!text.contains("input_libretro_device_p1"));
    for (id, label, key) in [
        ("up", "Up", "up"),
        ("down", "Down", "down"),
        ("left", "Left", "left"),
        ("right", "Right", "right"),
        ("b", "1", "z"),
        ("a", "2", "x"),
        ("start", "Pause", "enter"),
    ] {
        assert!(
            text.contains(&format!("rib_label_{id} = \"{label}\"")),
            "{id} label missing from {text}"
        );
        assert!(
            text.contains(&format!("input_player1_{id} = \"{key}\"")),
            "{id} binding missing from {text}"
        );
    }
    assert_eq!(
        text.lines().filter(|line| line.starts_with("input_player1_") && !line.ends_with("= \"nul\"")).count(),
        7,
        "Master System declares seven bindings"
    );
}

#[test]
fn export_stages_only_the_mastersystem_illustration() {
    // The source directory contains the artwork of every console, which we
    // would ship by copying the whole controllers folder. We may stage the
    // PNG of this console and the shared notice, and must leave out the
    // controller PNG of every other console.
    let profile = controls::profile_for_system("mastersystem").expect("default profile");
    assert_eq!(profile.image, "controller-mastersystem.png");

    let source = scratch();
    fs::copy(menu_assets().join("menu.rml"), source.join("menu.rml")).unwrap();
    for name in file_names(&controller_assets()) {
        fs::copy(controller_assets().join(&name), source.join(&name)).unwrap();
    }
    let unrelated_pngs: BTreeSet<String> = file_names(&source)
        .into_iter()
        .filter(|name| name.ends_with(".png") && name != &profile.image)
        .collect();
    assert!(
        unrelated_pngs.contains("controller-megadrive.png"),
        "the fixture has to offer another console's artwork, or absence proves nothing"
    );

    let destination = scratch();
    support::stage_controls(
        &source,
        &menu_assets(),
        &destination,
        "mastersystem",
        &Controls::default(),
        None,
    )
    .expect("stage the default pad");

    assert_eq!(
        fs::read(destination.join(&profile.image)).unwrap(),
        fs::read(source.join(&profile.image)).unwrap(),
        "the staged illustration must be the declared file"
    );
    assert_eq!(
        fs::read(destination.join("CONTROLLERS.txt")).unwrap(),
        fs::read(source.join("CONTROLLERS.txt")).unwrap()
    );
    // We generate the menu document in place to embed the selected pad in
    // the scene. It is not artwork of another console.
    assert_eq!(
        file_names(&destination),
        BTreeSet::from([
            profile.image.clone(),
            "CONTROLLERS.txt".to_string(),
            "menu.rml".to_string(),
            "menu.rcss".to_string(),
            "Silkscreen-Regular.ttf".to_string(),
            "Silkscreen-OFL.txt".to_string(),
            "ScienceGothic-Bold.ttf".to_string(),
            "ScienceGothic-OFL.txt".to_string(),
            rominabox_engine::volume::tick_file().to_string(),
            "parts".to_string(),
            // We stage the screen declarations with the menu.
            "design.cfg".to_string(),
            // The buttons that open the menu, and confirm and go back in it.
            "hotkeys-defaults.cfg".to_string(),
            // The design's own mark beside its name in the footer.
            "brand-mark.png".to_string(),
            // The scene we read in the player when someone picks another pad.
            // There is one pad for Master System, so there is exactly one.
            format!("scene-{}.rml", profile.id),
        ])
    );
    for name in &unrelated_pngs {
        assert!(
            !destination.join(name).exists(),
            "{name} belongs to another console and must not be exported"
        );
    }
    let markup = fs::read_to_string(destination.join("menu.rml")).unwrap();
    assert!(markup.contains(&format!("src=\"{}\"", profile.image)));
    for name in &unrelated_pngs {
        assert!(
            !markup.contains(name),
            "{name} must not be referenced by the staged menu"
        );
    }
}

#[test]
fn an_override_for_a_control_the_pad_does_not_have_is_rejected() {
    // The Master System pad has a d-pad, 1, 2 and pause, and Select is only in
    // other profiles. If we accepted it, we would store a binding that we never
    // show for this pad, and an override for the wrong console would go unnoticed.
    let mut options = Controls::default();
    options.bindings.insert(
        "select".to_string(),
        controls::ControlOverride {
            key: Some("space".to_string()),
            ..controls::ControlOverride::default()
        },
    );
    let error = controls::validate_for_system("mastersystem", &options)
        .expect_err("select is not on the Master System pad");
    assert!(
        error.contains("select") && error.contains("mastersystem"),
        "{error}"
    );
}
