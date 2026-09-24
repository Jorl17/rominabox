mod support;

use rominabox_desktop::{controls::Controls, menu, repo, themes};
use std::{fs, path::Path, process::Command};

fn style_only_design(destination: &Path) {
    let base = repo::at("integrations/designs/native");
    let frozen_base = destination.parent().unwrap().join("native");
    support::copy_tree(
        &repo::at("integrations/parts"),
        &destination.parent().unwrap().parent().unwrap().join("parts"),
    );
    fs::create_dir_all(destination).unwrap();
    fs::create_dir_all(&frozen_base).unwrap();
    // The whole Native package, as it is in a kit beside every design.
    for entry in fs::read_dir(&base).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), frozen_base.join(entry.file_name())).unwrap();
        }
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
    let design = root.join("designs/style-only");
    let staged = root.join("staged");
    style_only_design(&design);
    support::stage_theme(&design, &staged, "amber").unwrap();
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
    support::stage_splash(&design, &splash, "amber").unwrap();
    assert!(fs::read_to_string(splash.join("menu.rml"))
        .unwrap()
        .contains("splash-logo"));
}

#[test]
fn disc_inherits_achievements_and_retains_its_explicit_screen_contracts() {
    let screens = rominabox_desktop::menu::declared_screens(&repo::at("integrations/designs/disc")).unwrap();
    assert_eq!(
        screens
            .iter()
            .map(|screen| screen.id.as_str())
            .collect::<Vec<_>>(),
        [
            "pause",
            "disc",
            "discs",
            "options",
            "controls",
            "shaders",
            "achievements"
        ]
    );
    let achievements = screens
        .iter()
        .find(|screen| screen.id == "achievements")
        .expect("Disc must inherit the base achievements screen");
    assert_eq!(achievements.button, "achievements");
    assert_eq!(achievements.option_label.as_deref(), Some("ACHIEVEMENTS"));
    assert!(achievements.toggle.is_none());
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
    let design = root.join("designs/style-only");
    style_only_design(&design);
    let mut pause = fs::read_to_string(root.join("designs/native/screen-pause.rml")).unwrap();
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
    support::stage_controls(
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
    let screens = rominabox_desktop::menu::declared_screens(&design).unwrap();
    assert_eq!(screens[0].heading, "CARD PAUSED");
    assert!(screens.iter().any(|screen| screen.id == "achievements"));
}

#[test]
fn a_non_pause_override_keeps_native_screen_and_entry_order() {
    let root = rominabox_scratch::Scratch::dir("rominabox-screen-order");
    let design = root.join("designs/style-only");
    style_only_design(&design);
    let mut declaration: serde_json::Value =
        serde_json::from_slice(&fs::read(design.join("design.json")).unwrap()).unwrap();
    declaration["screens"] = serde_json::json!([{"id": "achievements", "heading": "TROPHIES"}]);
    fs::write(
        design.join("design.json"),
        serde_json::to_vec(&declaration).unwrap(),
    )
    .unwrap();
    let native = rominabox_desktop::menu::declared_screens(&root.join("designs/native")).unwrap();
    let selected = rominabox_desktop::menu::declared_screens(&design).unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|screen| screen.id.as_str())
            .collect::<Vec<_>>(),
        native
            .iter()
            .map(|screen| screen.id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(selected[4].heading, "TROPHIES");
}

#[test]
fn explicit_screen_order_rejects_missing_and_repeated_ids() {
    let root = rominabox_scratch::Scratch::dir("rominabox-explicit-screen-order");
    let design = root.join("designs/style-only");
    style_only_design(&design);
    let mut declaration: serde_json::Value =
        serde_json::from_slice(&fs::read(design.join("design.json")).unwrap()).unwrap();
    for (order, expected) in [
        (
            serde_json::json!(["pause", "missing"]),
            "Unknown screen 'missing'",
        ),
        (
            serde_json::json!(["pause", "pause"]),
            "Duplicate screen 'pause'",
        ),
    ] {
        declaration["screenOrder"] = order;
        fs::write(
            design.join("design.json"),
            serde_json::to_vec(&declaration).unwrap(),
        )
        .unwrap();
        let error = rominabox_desktop::menu::declared_screens(&design).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_list_screen_override_uses_the_selected_wrapper_and_inherited_parts() {
    let root = rominabox_scratch::Scratch::dir("rominabox-list-screen-override");
    let design = root.join("designs/style-only");
    let staged = root.join("staged");
    style_only_design(&design);
    fs::write(
        design.join("screen-achievements.rml"),
        "<div id=\"PANEL-ID\" class=\"screen-panel\" style=\"display:none;\"><div id=\"custom-achievements-chrome\">TROPHIES</div><!--ROWS--><!--ACTIONS--><!--STATUS--></div>",
    )
    .unwrap();
    support::stage_controls(
        &repo::at("desktop/assets/controllers"),
        &design,
        &staged,
        "megadrive",
        &Controls::default(),
        Some(&["achievements".into()]),
    )
    .unwrap();
    let menu = fs::read_to_string(staged.join("menu.rml")).unwrap();
    assert_eq!(menu.matches("id=\"achievements-panel\"").count(), 1);
    for id in [
        "custom-achievements-chrome",
        "achievements-prototype",
        "achievements-back",
        "achievements-status",
    ] {
        assert_eq!(
            menu.matches(&format!("id=\"{id}\"")).count(),
            1,
            "missing or duplicate {id}"
        );
    }
    assert!(
        menu.find("id=\"controls-panel\"").unwrap()
            < menu.find("id=\"achievements-panel\"").unwrap()
    );
}

#[test]
fn an_options_screen_override_needs_no_separate_order_file() {
    let root = rominabox_scratch::Scratch::dir("rominabox-options-screen-override");
    let design = root.join("designs/style-only");
    let staged = root.join("staged");
    style_only_design(&design);
    fs::write(
        design.join("screen-options.rml"),
        "<div id=\"options-panel\" style=\"display:none;\"><div id=\"custom-options-chrome\">INDEX</div><div id=\"options-entries\"><!--OPTIONS--></div><button class=\"menu-action options-back\" id=\"options-back\">BACK</button></div>",
    )
    .unwrap();
    support::stage_controls(
        &repo::at("desktop/assets/controllers"),
        &design,
        &staged,
        "megadrive",
        &Controls::default(),
        None,
    )
    .unwrap();
    let menu = fs::read_to_string(staged.join("menu.rml")).unwrap();
    assert_eq!(menu.matches("id=\"custom-options-chrome\"").count(), 1);
    assert_eq!(menu.matches("id=\"options-panel\"").count(), 1);
    assert!(menu.contains("id=\"controls\""));
}

#[test]
fn a_missing_adjacent_native_package_is_not_read_from_the_repository() {
    let root = rominabox_scratch::Scratch::dir("rominabox-no-global-design-fallback");
    let design = root.join("designs/style-only");
    fs::create_dir_all(&design).unwrap();
    fs::copy(
        repo::at("integrations/designs/native/menu.rcss"),
        design.join("menu.rcss"),
    )
    .unwrap();
    let error = rominabox_desktop::menu::declared_screens(&design).unwrap_err();
    assert!(
        error.contains("Native base design is missing beside"),
        "{error}"
    );
}

#[test]
fn disc_stages_its_chrome_and_inherited_achievement_controls() {
    let design = repo::at("integrations/designs/disc");
    let root = rominabox_scratch::Scratch::dir("rominabox-disc-composition");
    support::stage_controls(
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
    let lists_at = menu.find("id=\"achievements-panel\"").unwrap();
    assert!(option_at < disc_at && disc_at < lists_at);
    assert!(menu.contains("id=\"spine\""));
    assert!(menu.contains("MEMORY CARD") && menu.contains("CHOOSE A BLOCK"));
    assert!(menu.contains("id=\"unlock-row\""));
    assert!(menu.contains("id=\"achievements\""));
    assert!(!menu.contains("id=\"version\""));
    let css = fs::read_to_string(root.join("menu.rcss")).unwrap();
    assert!(css.contains("#unlock-row") && css.contains("#unlock-badge"));
    assert!(!css.contains("design("));
    assert!(menu::declared_screens(&design)
        .unwrap()
        .iter()
        .any(|screen| screen.id == "achievements" && screen.toggle.is_none()));

    let menu = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(!menu.contains("achievement-mode"));
    assert!(menu.contains("class=\"menu-action list-back\" id=\"achievements-back\""));
    let css = fs::read_to_string(root.join("menu.rcss")).unwrap();
    assert!(
        css.contains(".list-actions { position: absolute; left: 466dp; top: 508dp; width: 476dp;")
    );
    assert!(css.contains(".list-actions .list-toggle { left: 0; width: 284dp;"));
    assert!(css.contains(".list-actions .list-back { left: 300dp; width: 176dp;"));
    assert!(css.contains(".list-toggle-label") && css.contains(".list-toggle-state"));

    let document = menu
        .replace(
            "<div id=\"pause-panel\">",
            "<div id=\"pause-panel\" style=\"display:none;\">",
        )
        .replace(
            "id=\"achievements-panel\" class=\"screen-panel\" style=\"display:none;\"",
            "id=\"achievements-panel\" class=\"screen-panel\"",
        );
    // We hit-test the staged screen with the no-window RmlUi probe when the
    // native test setup provides its binary.
    if let Ok(probe) = std::env::var("ROMINABOX_RML_PROBE") {
        fs::write(root.join("menu.rml"), &document).unwrap();
        let output = Command::new(probe)
            .arg("--document")
            .arg(root.join("menu.rml"))
            .args(["--step", "move:700,520", "--step", "move:800,520"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let hits: Vec<serde_json::Value> = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(
            hits[0]["hover"],
            "achievements-panel",
            "hits: {hits:?}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            hits[1]["hover"],
            "achievements-back",
            "hits: {hits:?}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if let Ok(preview) = std::env::var("ROMINABOX_RML_PREVIEW") {
        let start = css.find(".list-actions {").unwrap();
        let end = css[start..].find(".list-status {").unwrap() + start;
        let old_actions = concat!(
            ".list-actions { position: absolute; left: 766dp; top: 508dp; width: 176dp; height: 36dp; }\n",
            ".list-actions .menu-action { position: absolute; left: 0; width: 176dp; height: 36dp; font-family: Silkscreen; font-size: 14dp; line-height: 36dp; text-align: center; }\n",
        );
        let old_css = format!("{}{}{}", &css[..start], old_actions, &css[end..]);
        let render = |name: &str, stylesheet: &str| {
            let directory = root.join(name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("menu.rml"), &document).unwrap();
            fs::write(directory.join("menu.rcss"), stylesheet).unwrap();
            fs::copy(
                root.join("Silkscreen-Regular.ttf"),
                directory.join("Silkscreen-Regular.ttf"),
            )
            .unwrap();
            let image = directory.join("capture.png");
            let output = Command::new(&preview)
                .args([directory.join("menu.rml"), image.clone()])
                .args(["960", "600"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            image::open(image).unwrap().to_rgba8()
        };
        let old = render("old-back", &old_css);
        let actual = render("new-back", &css);
        for y in 508..544 {
            for x in 766..942 {
                assert_eq!(
                    actual.get_pixel(x, y),
                    old.get_pixel(x, y),
                    "Disc Back pixel changed at ({x},{y})"
                );
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn disc_guard_message_wraps_within_its_slot() {
    let root = rominabox_scratch::Scratch::dir("rominabox-disc-slot-guard");
    support::stage_theme(&repo::at("integrations/designs/disc"), &root, "violet")
    .unwrap();
    let base = fs::read_to_string(root.join("menu.rml")).unwrap();
    let guarded = base
        .replace(
            "id=\"slot-2\" class=\"slot empty \">",
            "id=\"slot-2\" class=\"slot empty disabled\">",
        )
        .replace(
            "id=\"slot-state-2\" class=\"slot-state\">EMPTY",
            "id=\"slot-state-2\" class=\"slot-state\">ACHIEVEMENTS ON",
        );
    assert_ne!(guarded, base, "the staged slot must accept the guard state");
    assert!(guarded.contains("id=\"slot-2\" class=\"slot empty disabled\""));
    assert!(guarded.contains("class=\"slot-state\">ACHIEVEMENTS ON"));
    let preview = repo::at("desktop/src-tauri/resources/preview/rml-preview");
    let render = |name: &str, markup: &str| {
        let document = root.join(format!("{name}.rml"));
        fs::write(&document, markup).unwrap();
        let picture = root.join(format!("{name}.png"));
        let result = Command::new(&preview)
            .args([document.as_path(), picture.as_path()])
            .args(["960", "600"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        image::open(picture).unwrap().to_rgba8()
    };
    let original = render("original", &base);
    let guard = render("guard", &guarded);

    // Keep the last glyph out of the 12dp inner inset of the slot and out of
    // the gap before the action column. A word inside the border still looks
    // crowded when it reaches into this inset.
    for y in 120..170 {
        for x in 737..766 {
            assert_eq!(
                guard.get_pixel(x, y),
                original.get_pixel(x, y),
                "guard text crowded the inner edge of slot 2 at ({x}, {y})"
            );
        }
    }
    // "ON" must appear on a second line. Clipping the long label also keeps
    // the gap clear, but hides the state that the player must read.
    let second_line_pixels = (142..170)
        .flat_map(|y| (600..748).map(move |x| (x, y)))
        .filter(|&(x, y)| guard.get_pixel(x, y) != original.get_pixel(x, y))
        .count();
    assert!(
        second_line_pixels > 0,
        "the guard state lost its second line"
    );
}

#[test]
fn live_achievements_inherit_account_form_before_any_download() {
    for name in ["native", "disc"] {
        for palette in themes::registry().unwrap().palettes {
            let design = repo::at(&format!("integrations/designs/{name}"));
            let root = rominabox_scratch::Scratch::dir("rominabox-live-achievements");
            let entries =
                rominabox_desktop::achievements::entries(&design, true, true, None).unwrap();
            menu::compose_menu(&menu::MenuRequest {
                palette: palette.id.clone(),
                include_achievements: true,
                menu_entries: Some(entries),
                ..menu::MenuRequest::new(&design, support::artwork())
            })
            .and_then(|composed| composed.write(&root))
            .unwrap();
            let menu = fs::read_to_string(root.join("menu.rml")).unwrap();
            for id in [
                "achievements",
                "achievements-panel",
                "achievements-login",
                "achievement-username",
                "achievement-password",
                "achievements-list",
                "achievements-prototype",
                "achievements-back",
            ] {
                assert_eq!(
                    menu.matches(&format!("id=\"{id}\"")).count(),
                    1,
                    "{name}: {id}"
                );
            }
            assert!(menu.contains("type=\"password\""));
            assert!(!menu.contains("achievement-mode"));
            assert!(!fs::read_to_string(root.join("menu.rcss"))
                .unwrap()
                .contains("design("));
            let captures = std::env::var("ROMINABOX_ACCOUNT_SHOTS").ok();
            if let Some(probes) = std::env::var_os("ROMINABOX_INPUT_PROBE") {
                for probe in std::env::split_paths(&probes) {
                    let mut command = Command::new(probe);
                    command.arg(root.path());
                    if captures.is_some() {
                        command.arg("--capture");
                    }
                    let output = command.output().unwrap();
                    assert!(
                        output.status.success(),
                        "{name}/{}: {}{}",
                        palette.id,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            if let Some(directory) = captures {
                let directory = Path::new(&directory);
                fs::create_dir_all(directory).unwrap();
                for state in [
                    "signed-out",
                    "sign-in",
                    "confirmation",
                    "startup",
                    "notification",
                ] {
                    let snapshot = root.join(format!("account-{state}.rml"));
                    assert!(
                        snapshot.is_file(),
                        "Captures require the account input probe"
                    );
                    let output =
                        Command::new(repo::at("desktop/src-tauri/resources/preview/rml-preview"))
                            .arg(snapshot)
                            .arg(directory.join(format!("{name}-{}-{state}.png", palette.id)))
                            .args(["960", "600"])
                            .output()
                            .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
}

#[test]
fn a_design_customizes_account_controls_without_copying_the_screen() {
    let root = rominabox_scratch::Scratch::dir("rominabox-account-style");
    let design = root.join("designs/style-only");
    let staged = root.join("staged");
    style_only_design(&design);
    fs::write(
        design.join("achievements.rcss"),
        ".account-reveal { width: 120dp; color: design(highlight); }",
    )
    .unwrap();
    menu::compose_menu(&menu::MenuRequest {
        palette: "violet".into(),
        include_achievements: true,
        menu_entries: Some(vec!["controls".into(), "achievements".into()]),
        ..menu::MenuRequest::new(&design, support::artwork())
    })
    .and_then(|composed| composed.write(&staged))
    .unwrap();
    let css = fs::read_to_string(staged.join("menu.rcss")).unwrap();
    assert!(
        css.contains(".account-input"),
        "the base controls remain styled"
    );
    let override_at = css
        .find(".account-reveal { width: 120dp; color: #ff5c9a")
        .expect("design override with its selected palette");
    assert!(
        override_at > css.find(".account-input").unwrap(),
        "selected rules follow shared rules"
    );
    assert!(!design.join("screen-achievements.rml").exists());
}
