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
            "achievements",
            "accounts"
        ]
    );
    // The QUICK SIGN IN accounts also come from Native, and BACK leads to the
    // achievements screen, from which the player opens them.
    let accounts = screens
        .iter()
        .find(|screen| screen.id == "accounts")
        .expect("Disc must inherit the saved accounts screen");
    assert_eq!(accounts.button, "achievements-quick");
    assert_eq!(accounts.opener.as_deref(), Some("achievements"));
    let achievements = screens
        .iter()
        .find(|screen| screen.id == "achievements")
        .expect("Disc must inherit the base achievements screen");
    assert_eq!(achievements.button, "achievements");
    assert_eq!(achievements.option_label.as_deref(), Some("ACHIEVEMENTS"));
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
        design.join("screen-shaders.rml"),
        "<div id=\"PANEL-ID\" class=\"screen-panel\" style=\"display:none;\"><div id=\"custom-shaders-chrome\">FILTERS</div><!--ROWS--><!--ACTIONS--><!--STATUS--></div>",
    )
    .unwrap();
    support::stage_controls(
        &repo::at("desktop/assets/controllers"),
        &design,
        &staged,
        "megadrive",
        &Controls::default(),
        Some(&["shaders".into()]),
    )
    .unwrap();
    let menu = fs::read_to_string(staged.join("menu.rml")).unwrap();
    assert_eq!(menu.matches("id=\"shaders-panel\"").count(), 1);
    for id in [
        "custom-shaders-chrome",
        "scanlines",
        "shaders-back",
        "shaders-status",
    ] {
        assert_eq!(
            menu.matches(&format!("id=\"{id}\"")).count(),
            1,
            "missing or duplicate {id}"
        );
    }
    assert!(
        menu.find("id=\"controls-panel\"").unwrap() < menu.find("id=\"shaders-panel\"").unwrap()
    );
}

/// We refuse at composition a design that replaces the achievements screen
/// and leaves out the sign-in button, and name the design, its file and the
/// id, so we never export a game in which players cannot sign in.
#[test]
fn an_achievements_override_without_the_sign_in_button_is_rejected() {
    let root = rominabox_scratch::Scratch::dir("rominabox-achievements-contract");
    let design = root.join("designs/style-only");
    style_only_design(&design);
    let screen = fs::read_to_string(repo::at("integrations/designs/native/screen-achievements.rml"))
        .unwrap();
    let start = screen.find("<button id=\"achievements-login\"").unwrap();
    let end = screen[start..].find("</button>").unwrap() + start + "</button>".len();
    let without = format!("{}{}", &screen[..start], &screen[end..]);
    fs::write(design.join("screen-achievements.rml"), &without).unwrap();
    let error = support::stage_controls(
        &support::artwork(),
        &design,
        &root.join("staged"),
        "megadrive",
        &Controls::default(),
        Some(&["controls".into(), "achievements".into()]),
    )
    .unwrap_err();
    for part in [
        "screen-achievements.rml",
        "design 'style-only'",
        "#achievements-login",
        "achievements screen",
    ] {
        assert!(error.contains(part), "the refusal should name {part}: {error}");
    }
    assert!(!root.join("staged").exists(), "nothing is written for a refused menu");

    // With the button back, we compose the same file.
    fs::write(design.join("screen-achievements.rml"), &screen).unwrap();
    support::stage_controls(
        &support::artwork(),
        &design,
        &root.join("staged"),
        "megadrive",
        &Controls::default(),
        Some(&["controls".into(), "achievements".into()]),
    )
    .unwrap();
}

/// We would ignore a file that replaces a screen the menu does not have, so
/// we refuse it by name.
#[test]
fn an_override_for_an_unknown_screen_is_rejected() {
    let root = rominabox_scratch::Scratch::dir("rominabox-unknown-screen");
    let design = root.join("designs/style-only");
    style_only_design(&design);
    fs::write(
        design.join("screen-trophies.rml"),
        "<div id=\"trophies-panel\" class=\"screen-panel\" style=\"display:none;\"></div>",
    )
    .unwrap();
    let error = support::stage_theme(&design, &root.join("staged"), "blue").unwrap_err();
    assert!(error.contains("screen-trophies.rml"), "{error}");
    assert!(error.contains("design 'style-only'"), "{error}");
}

#[test]
fn an_options_screen_override_needs_no_separate_order_file() {
    let root = rominabox_scratch::Scratch::dir("rominabox-options-screen-override");
    let design = root.join("designs/style-only");
    let staged = root.join("staged");
    style_only_design(&design);
    fs::write(
        design.join("screen-options.rml"),
        "<div id=\"options-panel\" class=\"screen-panel\" style=\"display:none;\"><div id=\"custom-options-chrome\">INDEX</div><div id=\"options-entries\"><!--OPTIONS--></div><button class=\"menu-action options-back\" id=\"options-back\">BACK</button></div>",
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
        .any(|screen| screen.id == "achievements"));

    let menu = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(!menu.contains("achievement-mode"));
    assert!(menu.contains("class=\"menu-action list-back\" id=\"achievements-back\""));
    // We measure the place of the list actions in Disc from element boxes in
    // tests/disc_layout.rs.

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
        // Left of BACK, with the signed-in actions hidden, there is nothing
        // to press, because the pointer is over the field itself.
        assert_eq!(
            hits[0]["hover"],
            "screen",
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
}

#[cfg(target_os = "macos")]
/// We put the design's words for the text we write in the player into
/// design.cfg. When a design gives none, we keep the English. We refuse at
/// export, by name, a word that we do not write in the player.
#[test]
fn a_designs_words_reach_the_player_and_an_unknown_one_is_refused() {
    let root = rominabox_scratch::Scratch::dir("rominabox-design-words");
    let kit = support::kit_with_hypothetical(&root);
    let worded = support::compose(&kit, "wording", None, 1, &root.join("wording"));
    assert!(
        worded.cfg.contains("word_slot = \"BLOCK {slot}\"")
            && worded.cfg.contains("word_page-count = \"PAGE {page} OF {pages}\""),
        "{}",
        worded.cfg
    );
    let native = support::compose(&kit, "native", None, 1, &root.join("native"));
    assert!(!native.cfg.contains("word_"), "{}", native.cfg);

    let misworded = kit.join("designs/misworded");
    fs::create_dir_all(&misworded).unwrap();
    fs::write(
        misworded.join("design.json"),
        r#"{"schemaVersion": 1, "id": "misworded", "words": {"slots": "BLOCK"}}"#,
    )
    .unwrap();
    let error = menu::compose_menu(&menu::MenuRequest::new(
        &misworded,
        kit.join("menu-assets"),
    ))
    .unwrap_err();
    assert!(
        error.contains("'slots'") && error.contains("design 'misworded'"),
        "{error}"
    );
}

/// A design can also word the text that we write at composition, which is a
/// player setting's name, the ends of a level, the state of a switch and the
/// mark on the running filter. The words of a design appear in the composed
/// menu and in design.cfg, where we keep them current in the player. Without
/// them we use the English.
#[test]
fn a_designs_words_name_the_settings_and_mark_the_lists() {
    let root = rominabox_scratch::Scratch::dir("rominabox-design-setting-words");
    let kit = support::kit_with_hypothetical(&root);
    let entries = vec!["controls".to_string(), "shaders".to_string()];
    let worded = support::compose(&kit, "wording", Some(&entries), 2, &root.join("wording"));
    let native = support::compose(&kit, "native", Some(&entries), 2, &root.join("native"));
    for (menu, words) in [
        (
            &worded,
            ["LOUDNESS", "SOFT", "LOUD", "KEEP PLAYING", "NO", "LIT"],
        ),
        (
            &native,
            ["VOLUME", "LOW", "HIGH", "PLAY IN BACKGROUND", "OFF", "ON"],
        ),
    ] {
        let [name, low, high, background, off, mark] = words;
        for (what, text) in [
            ("the volume's name", format!("class=\"volume-name\">{name}<")),
            ("the volume's low end", format!("id=\"volume-low\" class=\"volume-end\">{low}<")),
            ("the volume's high end", format!("id=\"volume-high\" class=\"volume-end\">{high}<")),
            ("PLAY IN BACKGROUND's name", format!(">{background} <span")),
            ("the switch's state", format!("id=\"background-play-state\" class=\"setting-state\">{off}<")),
            ("the running filter's mark", format!("class=\"list-row-state\">{mark}<")),
        ] {
            assert!(menu.menu.contains(&text), "{what}: no {text} in\n{}", menu.menu);
        }
    }
    for word in ["switch-on", "switch-off", "shader-mark", "disc-mark"] {
        assert!(
            worded.cfg.contains(&format!("word_{word} = ")),
            "the player keeps {word} current, in the design's words: {}",
            worded.cfg
        );
    }
}

/// The Options entry of the disc list starts hidden and disabled, which we
/// write at composition on the design's entry template. When the template
/// has a style, we keep it and add the hiding to it, because an element with
/// two style attributes gets only one of them.
#[test]
fn a_hidden_entry_joins_the_style_its_template_has() {
    let root = rominabox_scratch::Scratch::dir("rominabox-design-styled-entry");
    let kit = support::kit(&root);
    let design = kit.join("designs/styled-entries");
    fs::create_dir_all(&design).unwrap();
    fs::write(
        design.join("design.json"),
        r#"{"schemaVersion": 1, "id": "styled-entries"}"#,
    )
    .unwrap();
    fs::write(
        design.join("option-entry.rml"),
        r#"<button class="menu-action option-entry" id="BUTTON" style="text-align: left;"><span class="option-label">LABEL</span></button>"#,
    )
    .unwrap();
    let composed = support::compose(&kit, "styled-entries", None, 2, &root.join("composed"));
    let at = composed.menu.find("id=\"discs\"").expect("the disc list has its entry");
    let start = composed.menu[..at].rfind('<').unwrap();
    let end = at + composed.menu[at..].find('>').unwrap();
    let tag = &composed.menu[start..end];
    assert_eq!(tag.matches("style=").count(), 1, "{tag}");
    assert!(
        tag.contains("text-align: left;") && tag.contains("display: none;"),
        "{tag}"
    );
}

/// The Pause heading comes from its `screens` entry, which we write in the
/// player when Pause opens. When a design words `paused-heading`, the player
/// sees that word there, or we refuse it at export, because a word we accept
/// and never show is of no use to its author.
#[test]
fn a_design_that_words_the_pause_heading_sees_it_or_is_refused() {
    let root = rominabox_scratch::Scratch::dir("rominabox-design-pause-word");
    let kit = support::kit_with_hypothetical(&root);
    let design = kit.join("designs/pause-worded");
    fs::create_dir_all(&design).unwrap();
    fs::write(
        design.join("design.json"),
        r#"{"schemaVersion": 1, "id": "pause-worded", "words": {"paused-heading": "HALTED"}}"#,
    )
    .unwrap();
    match menu::compose_menu(&menu::MenuRequest::new(&design, kit.join("menu-assets"))) {
        Err(error) => assert!(
            error.contains("'paused-heading'") && error.contains("design 'pause-worded'"),
            "{error}"
        ),
        Ok(composition) => {
            let composed = root.join("composed");
            composition.write(&composed).unwrap();
            let cfg = fs::read_to_string(composed.join("design.cfg")).unwrap();
            let value = |key: &str| {
                cfg.lines()
                    .find_map(|line| line.strip_prefix(&format!("{key} = \"")))
                    .and_then(|rest| rest.strip_suffix('"'))
                    .map(str::to_owned)
            };
            let pause = cfg
                .lines()
                .find_map(|line| {
                    line.strip_prefix("screen_role_")
                        .and_then(|rest| rest.strip_suffix(" = \"pause\""))
                })
                .expect("the composed menu declares Pause");
            let heading = value(&format!("screen_heading_{pause}")).unwrap_or_default();
            assert_eq!(
                heading, "HALTED",
                "the export accepted paused-heading = HALTED, and the menu shows Pause's heading, {heading}"
            );
        }
    }
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
