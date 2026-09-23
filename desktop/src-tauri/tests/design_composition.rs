use rominabox_desktop::{controls::Controls, repo, themes};
use std::{fs, path::Path};

fn style_only_design(destination: &Path) {
    let base = repo::at("integrations/designs/native");
    let frozen_base = destination.parent().unwrap().join("native");
    fs::create_dir_all(destination).unwrap();
    fs::create_dir_all(&frozen_base).unwrap();
    for name in [
        "design.json",
        "menu.rml",
        "splash.rml",
        "heading.rml",
        "save-slots.rml",
        "screen-pause.rml",
        "screen-controls.rml",
        "footer.rml",
    ] {
        fs::copy(base.join(name), frozen_base.join(name)).unwrap();
    }
    for name in ["menu.rcss", "Silkscreen-Regular.ttf", "Silkscreen-OFL.txt"] {
        fs::copy(base.join(name), destination.join(name)).unwrap();
    }
    let mut declaration: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("design.json")).unwrap()).unwrap();
    declaration.as_object_mut().unwrap().remove("screens");
    declaration["id"] = "style-only".into();
    declaration["documents"]
        .as_object_mut()
        .unwrap()
        .remove("menu");
    fs::write(
        destination.join("design.json"),
        serde_json::to_vec(&declaration).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_style_only_design_stages_the_base_screens_with_its_styles() {
    let root = rominabox_scratch::Scratch::dir("rominabox-design-composition");
    let design = root.join("style-only");
    let staged = root.join("staged");
    style_only_design(&design);
    themes::prepare_theme_assets(&design, &staged, "amber", None).unwrap();
    themes::prepare_controls_assets(
        &repo::at("desktop/assets/controllers"),
        &design,
        &staged,
        "megadrive",
        &Controls::default(),
        None,
    )
    .unwrap();
    let menu = fs::read_to_string(staged.join("menu.rml")).unwrap();
    for id in [
        "pause-panel",
        "controls-panel",
        "options-panel",
        "controls-device-current",
    ] {
        assert!(
            menu.contains(&format!("id=\"{id}\"")),
            "missing inherited {id}"
        );
    }
    let css = fs::read_to_string(staged.join("menu.rcss")).unwrap();
    assert!(!css.contains("design("), "palette tokens must be resolved");
    assert!(
        css.contains("#bd490b"),
        "the requested Amber screen colour must be used"
    );
    let splash = root.join("splash");
    themes::prepare_splash_assets(&design, &splash, "amber").unwrap();
    assert!(fs::read_to_string(splash.join("menu.rml"))
        .unwrap()
        .contains("splash-logo"));
}

#[test]
fn disc_inherits_achievements_and_retains_its_explicit_screen_contracts() {
    let screens = themes::declared_screens(&repo::at("integrations/designs/disc")).unwrap();
    let achievements = screens
        .iter()
        .find(|screen| screen.id == "achievements")
        .expect("Disc must inherit the base achievements screen");
    assert_eq!(achievements.button, "achievements");
    assert_eq!(achievements.option_label.as_deref(), Some("ACHIEVEMENTS"));
    assert!(achievements.toggle.is_some());
    let pause = screens.iter().find(|screen| screen.id == "pause").unwrap();
    assert_eq!(pause.heading, "MEMORY CARD");
    let disc = screens.iter().find(|screen| screen.id == "disc").unwrap();
    assert_eq!(disc.images.as_deref(), Some("discs"));
    let discs = screens.iter().find(|screen| screen.id == "discs").unwrap();
    assert_eq!(
        discs.option_label, None,
        "the explicit Disc navigation must remain"
    );
}

#[test]
fn one_screen_override_keeps_the_other_base_screens() {
    let root = rominabox_scratch::Scratch::dir("rominabox-one-screen-override");
    let design = root.join("style-only");
    style_only_design(&design);
    let mut pause = fs::read_to_string(root.join("native/screen-pause.rml")).unwrap();
    pause = pause.replace("CHOOSE A SLOT", "CHOOSE A CARD");
    fs::write(design.join("screen-pause.rml"), pause).unwrap();
    let mut declaration: serde_json::Value =
        serde_json::from_slice(&fs::read(design.join("design.json")).unwrap()).unwrap();
    declaration["screens"] = serde_json::json!([{"id": "pause", "heading": "CARD PAUSED"}]);
    fs::write(
        design.join("design.json"),
        serde_json::to_vec(&declaration).unwrap(),
    )
    .unwrap();

    let staged = root.join("staged");
    themes::prepare_theme_assets(&design, &staged, "blue", None).unwrap();
    themes::prepare_controls_assets(
        &repo::at("desktop/assets/controllers"),
        &design,
        &staged,
        "megadrive",
        &Controls::default(),
        None,
    )
    .unwrap();
    let menu = fs::read_to_string(staged.join("menu.rml")).unwrap();
    assert!(menu.contains("CHOOSE A CARD"));
    assert!(!menu.contains("CHOOSE A SLOT"));
    assert!(menu.contains("id=\"controls-panel\""));
    assert!(menu.contains("id=\"options-panel\""));
    let screens = themes::declared_screens(&design).unwrap();
    assert_eq!(screens[0].heading, "CARD PAUSED");
    assert!(screens.iter().any(|screen| screen.id == "achievements"));
}

#[test]
fn a_missing_adjacent_native_package_is_not_read_from_the_repository() {
    let root = rominabox_scratch::Scratch::dir("rominabox-no-global-design-fallback");
    let design = root.join("style-only");
    fs::create_dir_all(&design).unwrap();
    fs::copy(
        repo::at("integrations/designs/native/menu.rcss"),
        design.join("menu.rcss"),
    )
    .unwrap();
    let error = themes::declared_screens(&design).unwrap_err();
    assert!(
        error.contains("Native base design is missing beside"),
        "{error}"
    );
}

#[test]
fn disc_stages_its_chrome_and_inherited_achievement_controls() {
    let design = repo::at("integrations/designs/disc");
    let root = rominabox_scratch::Scratch::dir("rominabox-disc-composition");
    themes::prepare_theme_assets(&design, &root, "blue", None).unwrap();
    let screens = themes::prepare_controls_assets(
        &repo::at("desktop/assets/controllers"),
        &design,
        &root,
        "megadrive",
        &Controls::default(),
        Some(&["controls".into(), "achievements".into()]),
    )
    .unwrap();
    let menu = fs::read_to_string(root.join("menu.rml")).unwrap();
    let option_at = menu.find("id=\"options-panel\"").unwrap();
    let disc_at = menu.find("id=\"disc-panel\"").unwrap();
    let lists_at = menu.find("<!--SCREENS-->").unwrap();
    assert!(option_at < disc_at && disc_at < lists_at);
    assert!(menu.contains("id=\"spine\""));
    assert!(menu.contains("MEMORY CARD") && menu.contains("CHOOSE A BLOCK"));
    assert!(menu.contains("id=\"unlock-row\""));
    assert!(menu.contains("id=\"achievements\""));
    assert!(!menu.contains("id=\"version\""));
    let css = fs::read_to_string(root.join("menu.rcss")).unwrap();
    assert!(css.contains("#unlock-row") && css.contains("#unlock-badge"));
    assert!(!css.contains("design("));
    assert!(screens
        .iter()
        .any(|screen| screen.id == "achievements" && screen.toggle.is_some()));
}
