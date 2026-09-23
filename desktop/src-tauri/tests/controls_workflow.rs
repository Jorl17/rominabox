use rominabox_desktop::{
    controls::{self, Controls},
    themes,
};
use std::{fs, path::PathBuf};

fn workspace() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-controls")
}
fn assets() -> PathBuf {
    rominabox_desktop::repo::at("integrations/designs/native")
}
/// A source directory with artwork for every profile that declares it. We
/// take the filenames from controls.json, so adding an illustrated profile
/// cannot silently break these tests.
fn illustrated_assets() -> rominabox_scratch::Scratch {
    let root = workspace();
    fs::copy(assets().join("menu.rml"), root.join("menu.rml")).unwrap();
    let registry: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(rominabox_desktop::repo::at("desktop/controls.json"))
            .unwrap(),
    )
    .unwrap();
    for profile in registry["profiles"].as_array().unwrap() {
        match profile["image"].as_str() {
            Some(image) if !image.is_empty() => fs::write(root.join(image), []).unwrap(),
            _ => {}
        }
    }
    fs::write(root.join("CONTROLLERS.txt"), []).unwrap();
    root
}
#[test]
fn six_button_authoring_configures_the_emulated_device_and_labels() {
    let root = workspace();
    let options:Controls=serde_json::from_value(serde_json::json!({"profile":"megadrive6","bindings":{"r":{"label":"Special","key":"space"}}})).unwrap();
    let profile =
        controls::write_defaults_config("megadrive", &options, &root.join("controls.cfg")).unwrap();
    assert_eq!(profile.controls.len(), 12);
    let text = fs::read_to_string(root.join("controls.cfg")).unwrap();
    // The emulated device must NOT be here. `input_libretro_device_p1` has an
    // effect only in a remap file (in `configuration.c` the key appears only in
    // the remap loader and saver), so in a config file it has no effect.
    assert!(
        !text.contains("input_libretro_device_p1"),
        "the emulated device belongs in a remap file, not the controls config"
    );
    assert_eq!(
        profile.core_device,
        Some(513),
        "the profile still declares the six-button device for the remap to use"
    );
    assert!(text.contains("rib_label_r = \"Special\""));
    assert!(text.contains("rib_label_select = \"Mode\""));
    assert!(text.contains("input_player1_r = \"space\""));
    let saved = serde_json::to_vec(&options).unwrap();
    let reopened: Controls = serde_json::from_slice(&saved).unwrap();
    assert_eq!(reopened.profile.as_deref(), Some("megadrive6"));
}
#[test]
fn a_missing_controller_illustration_uses_a_working_asset_free_grid() {
    let root = workspace();
    let options = Controls::default();
    themes::prepare_controls_assets(&assets(), &assets(), &root, "atari2600", &options, None).unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(!markup.contains("id=\"controller-image\""));

    // Every control, and not a sample of one. A console with no drawing has
    // the same controls as one with a drawing. For a core with no controller
    // drawing, we still show its controls, drawn as squares or rectangles.
    let declared = controls::profile_for_system("atari2600").expect("a profile");
    let missing: Vec<&str> = declared
        .controls
        .iter()
        .map(|control| control.id.as_str())
        // Either form is valid. In the player we listen on `control-<id>` and on
        // `control-hit-<id>`, whichever the document contains. An illustrated
        // pad has an invisible hit circle over the drawn button, and in the
        // grid the box itself is the target. If we required both, we would
        // reject the grid, which is valid.
        .filter(|id| {
            !markup.contains(&format!("id=\"control-{id}\""))
                && !markup.contains(&format!("id=\"control-hit-{id}\""))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "the asset-free grid is missing {} of {} controls, so they cannot be \
         clicked, hovered or rebound: {missing:?}",
        missing.len(),
        declared.controls.len()
    );
    // Only the document and the scene that the player can switch to. For a
    // console with no illustration, no artwork may go into the export.
    let written: Vec<String> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        written.iter().all(|name| name.ends_with(".rml") || name == "design.cfg"),
        "an asset-free console staged something that is not the menu: {written:?}"
    );
    controls::write_defaults_config("atari2600", &options, &root.join("controls.cfg")).unwrap();
    assert!(fs::read_to_string(root.join("controls.cfg"))
        .unwrap()
        .contains("input_player1_l2"));
}
#[test]
fn custom_labels_are_escaped_without_changing_control_identity() {
    let root = workspace();
    let options: Controls =
        serde_json::from_value(serde_json::json!({"bindings":{"a":{"label":"Jump <go> & fly"}}}))
            .unwrap();
    themes::prepare_controls_assets(&illustrated_assets(), &assets(), &root, "nes", &options, None).unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(markup.contains("Jump &lt;go&gt; &amp; fly"));
    assert!(markup.contains("id=\"control-a\""));
}
#[test]
fn default_callout_labels_occur_once_and_custom_labels_keep_console_identity() {
    let options: Controls = serde_json::from_value(serde_json::json!({
        "bindings": {
            "up": {"label": "   "},
            "down": {"label": "Down"},
            "a": {"label": "Jump <go> & fly"}
        }
    }))
    .unwrap();
    for (system, source, console_identity) in [
        ("megadrive", illustrated_assets(), "C"),
        ("nes", illustrated_assets(), "A"),
    ] {
        let root = workspace();
        themes::prepare_controls_assets(&source, &assets(), &root, system, &options, None).unwrap();
        let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
        for label in ["Up", "Down", "Left", "Start"] {
            assert_eq!(
                markup.matches(&format!(">{label}<")).count(),
                1,
                "{system} default label {label} should appear once"
            );
        }
        assert!(
            !markup.contains(r#"class="control-original">Up"#)
                && !markup.contains(r#"class="control-original">Down"#),
            "{system} should omit console identity when the author label is not meaningfully different"
        );
        assert!(markup.contains("Jump &lt;go&gt; &amp; fly"));
        assert!(markup.contains(&format!(
            r#"<span class="control-original">{console_identity}</span><span id="control-binding-a">"#
        )));
        assert_eq!(markup.matches(r#"class="control-original""#).count(), 1);
        assert!(markup.contains(r#"id="control-binding-down""#));
        assert!(markup.contains(r#"id="control-down""#));
    }
}
#[test]
fn controller_variants_cannot_be_applied_to_an_unrelated_system() {
    let options: Controls =
        serde_json::from_value(serde_json::json!({"profile":"megadrive6"})).unwrap();
    assert!(controls::validate_for_system("gbc", &options).is_err());
}
#[test]
fn a_three_button_profile_rejects_six_button_only_overrides() {
    let options: Controls =
        serde_json::from_value(serde_json::json!({"bindings":{"l":{"key":"space"}}})).unwrap();
    assert!(controls::validate_for_system("megadrive", &options).is_err());
}

#[test]
fn splash_only_document_has_no_pause_controls_and_can_make_its_background_transparent() {
    let root = workspace();
    themes::prepare_splash_assets(&assets(), &root, "blue").unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(markup.contains("id=\"body\""));
    assert!(markup.contains("id=\"splash-logo\""));
    assert!(!markup.contains("pause-panel"));
    assert!(!markup.contains("controller-image"));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 5);
}

/// We give a game with a logo and no menu a stylesheet that RmlUi can read.
///
/// A stylesheet copied verbatim from the design would still contain
/// `design(background)`. Such a declaration has no effect in RmlUi and gives
/// no error, and the logo would appear over whatever was left.
#[test]
fn a_logo_only_export_gets_the_palette_in_its_stylesheet() {
    let root = workspace();
    themes::prepare_splash_assets(&assets(), &root, "blue").unwrap();
    let css = fs::read_to_string(root.join("menu.rcss")).unwrap();
    assert!(
        !css.contains("design("),
        "the staged stylesheet still asks for tokens nothing resolved"
    );
}

/// We declare what is in the document, and not the design's whole catalogue.
///
/// A logo-only export has one element. If we declared every screen and every
/// overlay of the design, in the player we would wait for the notice's timer
/// to show an element that is not in that document, and answer a request for a
/// screen with a panel that does not exist.
#[test]
fn a_document_is_only_told_about_what_it_draws() {
    let full = workspace();
    themes::prepare_theme_assets(&assets(), &full, "blue", None).unwrap();
    let declared = fs::read_to_string(full.join("design.cfg")).unwrap();
    assert!(declared.contains("overlays = \"splash notice\""), "{declared}");
    assert!(declared.contains("screens = \"pause options controls\""), "{declared}");
    assert!(
        declared.contains("overlay_needs_splash = \"splash-logo.png\""),
        "{declared}"
    );
    // We wait for the logo before the notice, not for a delay long enough on
    // one machine, because a slow start delays both, so they cannot overlap.
    assert!(
        declared.contains("overlay_follows_notice = \"splash\""),
        "{declared}"
    );

    let logo_only = workspace();
    themes::prepare_splash_assets(&assets(), &logo_only, "blue").unwrap();
    let declared = fs::read_to_string(logo_only.join("design.cfg")).unwrap();
    assert!(declared.contains("overlays = \"splash\""), "{declared}");
    assert!(declared.contains("screens = \"\""), "{declared}");
}

/// An overlay may wait only for one declared before it.
///
/// If two overlays waited for each other, both would wait for ever, and the
/// game would spend its first seconds drawing a menu document nobody asked
/// for. Because of the order in which we declare them, that cannot happen.
#[test]
fn an_overlay_cannot_wait_for_one_that_comes_after_it() {
    let design = workspace();
    fs::copy(assets().join("menu.rml"), design.join("menu.rml")).unwrap();
    let mut declared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(assets().join("design.json")).unwrap()).unwrap();
    declared["overlays"] = serde_json::json!([
        { "id": "notice", "follows": "splash", "afterMs": 0, "holdMs": 10, "leaveMs": 0 },
        { "id": "splash", "follows": "notice", "afterMs": 0, "holdMs": 10, "leaveMs": 0 },
    ]);
    fs::write(
        design.join("design.json"),
        serde_json::to_string(&declared).unwrap(),
    )
    .unwrap();
    let refusal = match themes::declared_overlays(&design) {
        Err(message) => message,
        Ok(_) => panic!("a wait with no end was accepted"),
    };
    assert!(refusal.contains("notice"), "{refusal}");
    assert!(refusal.contains("splash"), "{refusal}");
}

/// We show a picker only where there is a choice.
///
/// The Mega Drive has a three-button and a six-button pad, because some games
/// misbehave with six buttons, and the original pad had a Mode button for it.
/// For PlayStation we offer one pad, and a dropdown of one option is noise.
#[test]
fn the_controller_picker_is_offered_only_when_there_is_a_choice() {
    let root = workspace();
    let staged = |system: &str, _profile_image: &str| -> String {
        let source = root.join(format!("source-{system}"));
        let destination = root.join(format!("staged-{system}"));
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::copy(assets().join("menu.rml"), source.join("menu.rml")).unwrap();
        // Every pad for the console, and not only the chosen one. We stage
        // them all at export, so we can show another one when a player picks it.
        for entry in controls::variants_for_system(system).unwrap() {
            if !entry.image.is_empty() {
                fs::write(source.join(&entry.image), []).unwrap();
            }
        }
        fs::write(source.join("CONTROLLERS.txt"), []).unwrap();
        rominabox_desktop::themes::prepare_controls_assets(&source, &assets(), &destination,
            system,
            &Controls::default(),
            None,
        )
        .expect("the scene markup is generated");
        fs::read_to_string(destination.join("menu.rml")).unwrap()
    };

    let megadrive = staged("megadrive", "controller-megadrive.png");
    assert!(
        megadrive.contains("controls-device-option-megadrive6"),
        "Mega Drive offers a six-button pad, so the picker must list it"
    );
    assert!(
        megadrive.contains(r#"id="controls-device-list""#),
        "the picker's list element must exist for the bridge to open"
    );
    assert!(
        megadrive.contains("display:none"),
        "the list starts closed"
    );

    // For PlayStation we declare a DualShock and an analogue pad, and offer
    // both.
    let playstation = staged("ps1", "controller-ps1.png");
    assert!(
        playstation.contains("controls-device-option-ps1-analog"),
        "PlayStation distinguishes an analogue pad from a DualShock, and a game \
         that wants one will not accept the other; the picker must list both"
    );

    // We show a picker only when there is a choice. The Super Nintendo has
    // one pad, and a list of one is noise.
    let snes = staged("snes", "controller-snes.png");
    assert!(
        !snes.contains("controls-device"),
        "a console with one pad gets no picker"
    );
}

/// We must be able to apply a picked controller, and not only name it.
///
/// A variant's id and display name are enough to draw a list and record a
/// choice, but to apply it we need the emulated device, which is what we
/// pass to the core.
#[test]
fn each_offered_controller_carries_the_device_it_means() {
    let root = workspace();
    let options = Controls::default();
    controls::write_defaults_config("megadrive", &options, &root.join("controls.cfg")).unwrap();
    let text = fs::read_to_string(root.join("controls.cfg")).unwrap();

    assert!(
        text.contains("controls_variants = \"megadrive megadrive6\""),
        "the player needs the list before it can offer one: {text}"
    );
    // The two Mega Drive pads are 3-button and 6-button, which are different
    // emulated devices, and the picker exists to choose between them.
    assert!(text.contains("controls_variant_device_megadrive = \"257\""));
    assert!(text.contains("controls_variant_device_megadrive6 = \"513\""));
    assert!(
        text.contains("controls_variant_name_megadrive6 = "),
        "a picker shows names, not ids"
    );
}

/// Every input on a control is a row of the shared list, which we write when
/// we bundle the game.
///
/// A callout can show one assignment, but a control can have a key, a pad
/// button, an axis and a mouse button, and a stick is several of those. At
/// export we write a row for each input the bundled pads can have, plus one
/// page more than fits, so we can show that there is a next page. In the
/// player we fill those rows, and we cannot add rows there.
#[test]
fn every_bind_is_a_row_of_the_shared_list() {
    let root = workspace();
    let options = Controls::default();
    themes::prepare_controls_assets(&illustrated_assets(), &assets(), &root, "ps1", &options, None)
        .unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    let declared = fs::read_to_string(root.join("design.cfg")).unwrap();
    controls::write_defaults_config("ps1", &options, &root.join("controls.cfg")).unwrap();
    let defaults = fs::read_to_string(root.join("controls.cfg")).unwrap();

    assert!(
        markup.contains("id=\"control-binds\" class=\"list\" style=\"display:none;\""),
        "the controls screen has no shared list for the binds"
    );
    let rows = markup.matches("class=\"list-row ").count();
    assert!(
        rows > 4,
        "a PlayStation stick is five directions and a page holds four, but the list has {rows} rows"
    );
    assert!(
        markup.contains("id=\"binds-pager\""),
        "more rows than fit, and no pager"
    );
    assert!(
        !markup.contains("bind-row"),
        "the binds grew their own row"
    );
    assert!(
        declared.contains("binds_after = \""),
        "when the list appears is not declared: {declared}"
    );
    assert!(
        defaults.contains("rib_group_l_y_minus = \"l_stick\""),
        "a stick's directions are not one control: {defaults}"
    );
}

/// The callout is the list on one line. One binding stays that binding, and
/// three bindings are the three names, never a count instead of them.
#[test]
fn a_callout_names_every_binding_on_the_control() {
    let root = workspace();
    let mut options = Controls::default();
    options.bindings.insert(
        "up".into(),
        controls::ControlOverride {
            button: Some("0".into()),
            axis: Some("+0".into()),
            ..Default::default()
        },
    );
    themes::prepare_controls_assets(
        &illustrated_assets(),
        &assets(),
        &root,
        "megadrive",
        &options,
        None,
    )
    .unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    let binding = markup
        .split("id=\"control-binding-up\">")
        .nth(1)
        .and_then(|rest| rest.split('<').next())
        .unwrap_or("");
    assert_eq!(binding, "up, Button 0, Axis +0");
    assert!(
        !binding.contains("bind"),
        "a count is not a binding: {binding}"
    );
    let alone = markup
        .split("id=\"control-binding-a\">")
        .nth(1)
        .and_then(|rest| rest.split('<').next())
        .unwrap_or("");
    assert_eq!(alone, "c", "one binding is that binding, not a count: {alone}");
}
