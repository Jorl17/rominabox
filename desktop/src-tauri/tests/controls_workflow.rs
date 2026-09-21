use rominabox_desktop::{
    controls::{self, Controls},
    themes,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn workspace() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "rominabox-controls-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs/native")
}
/// A source directory with artwork for every profile that declares it. We
/// take the filenames from controls.json, so adding an illustrated profile
/// cannot silently break these tests.
fn illustrated_assets() -> PathBuf {
    let root = workspace();
    fs::copy(assets().join("menu.rml"), root.join("menu.rml")).unwrap();
    let registry: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../controls.json"))
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
    themes::prepare_controls_assets(&assets(), &root, "atari2600", &options).unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(markup.contains("id=\"control-r3\""));
    assert!(!markup.contains("id=\"controller-image\""));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
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
    themes::prepare_controls_assets(&illustrated_assets(), &root, "nes", &options).unwrap();
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
        themes::prepare_controls_assets(&source, &root, system, &options).unwrap();
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
    themes::prepare_splash_assets(&assets(), &root).unwrap();
    let markup = fs::read_to_string(root.join("menu.rml")).unwrap();
    assert!(markup.contains("id=\"body\""));
    assert!(markup.contains("id=\"splash-logo\""));
    assert!(!markup.contains("pause-panel"));
    assert!(!markup.contains("controller-image"));
    assert_eq!(fs::read_dir(root).unwrap().count(), 4);
}

/// We show a picker only where there is a choice.
///
/// The Mega Drive has a three-button and a six-button pad, because some games
/// misbehave with six buttons, and the original pad had a Mode button for it.
/// For PlayStation we offer one pad, and a dropdown of one option is noise.
#[test]
fn the_controller_picker_is_offered_only_when_there_is_a_choice() {
    let root = workspace();
    let staged = |system: &str, profile_image: &str| -> String {
        let source = root.join(format!("source-{system}"));
        let destination = root.join(format!("staged-{system}"));
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::copy(assets().join("menu.rml"), source.join("menu.rml")).unwrap();
        fs::write(source.join(profile_image), []).unwrap();
        fs::write(source.join("CONTROLLERS.txt"), []).unwrap();
        rominabox_desktop::themes::prepare_controls_assets(
            &source,
            &destination,
            system,
            &Controls::default(),
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

    let playstation = staged("ps1", "controller-ps1.png");
    assert!(
        !playstation.contains("controls-device"),
        "PlayStation offers one pad; a list of one is noise, not a choice"
    );
}
