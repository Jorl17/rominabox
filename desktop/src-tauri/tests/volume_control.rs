//! The volume control is a slider and a mute toggle.
//!
//! We draw those parts as the design styles them, and the control contains
//! one of each. When a design restyles the slider, volume changes with it,
//! and a design without any rule for volume still has the control.

use std::fs;
use std::path::Path;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn volume_is_built_from_the_designs_slider_and_toggle() {
    let design = scratch("rominabox-volume-parts");
    fs::create_dir(design.join("parts")).unwrap();
    fs::write(
        design.join("parts/slider.rml"),
        r#"<div id="PART-ID" class="slider owned-slider"><div class="slider-track"><div class="slider-fill"></div><div class="slider-thumb"></div></div><div class="slider-readout"></div></div>"#,
    )
    .unwrap();
    fs::write(
        design.join("parts/toggle.rml"),
        r#"<button id="PART-ID" class="toggle owned-toggle"><span class="toggle-knob"></span><span class="toggle-label">LABEL</span></button>"#,
    )
    .unwrap();

    let markup = rominabox_desktop::themes::volume_control_markup(&design).unwrap();
    assert!(
        markup.contains("owned-slider"),
        "a design's own slider has to be the one volume uses, got {markup}"
    );
    assert!(
        markup.contains("owned-toggle"),
        "mute has to be the design's toggle, got {markup}"
    );
    assert!(markup.contains("id=\"volume-level\""));
    assert!(markup.contains("MUTE"));
    assert!(
        !markup.contains("PART-ID"),
        "the holes have to be filled, got {markup}"
    );
}

#[test]
fn a_design_with_no_parts_still_gets_a_slider() {
    let design = scratch("rominabox-volume-builtin");
    let markup = rominabox_desktop::themes::volume_control_markup(&design).unwrap();
    for class in ["slider", "slider-track", "slider-fill", "slider-thumb", "slider-readout", "toggle"] {
        assert!(
            markup.contains(class),
            "the built-in volume control is missing {class}: {markup}"
        );
    }
}

#[test]
fn a_part_that_cannot_be_found_is_refused() {
    let design = scratch("rominabox-volume-broken-part");
    fs::create_dir(design.join("parts")).unwrap();
    fs::write(
        design.join("parts/slider.rml"),
        r#"<div id="PART-ID" class="pretty"></div>"#,
    )
    .unwrap();
    let error = rominabox_desktop::themes::volume_control_markup(&design)
        .expect_err("a slider with none of the part's classes is not a slider");
    assert!(
        error.contains("slider"),
        "the refusal should say which part is wrong: {error}"
    );
}

#[test]
fn the_builtin_slider_style_is_only_for_a_design_that_has_none() {
    let bare = rominabox_desktop::themes::builtin_part_rules("body { color: #fff; }");
    assert!(
        bare.contains("part:slider"),
        "a design that never styled a slider still has to be given one"
    );
    let styled = rominabox_desktop::themes::builtin_part_rules(".slider { height: 12dp; }");
    assert!(
        styled.is_empty(),
        "a design that styled .slider must not have a second slider pasted after it"
    );
}

#[test]
fn the_shipped_design_styles_the_slider_rather_than_volume() {
    let css = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs/native/menu.rcss"),
    )
    .unwrap();
    assert!(
        css.contains(".slider"),
        "the design has to say what a slider looks like"
    );
    assert!(
        css.contains(".toggle"),
        "the design has to say what a toggle looks like"
    );
    assert!(
        !css.contains("#volume-level"),
        "volume-specific styling is the slider failing to be a part"
    );
}

#[test]
fn a_design_that_never_mentions_volume_still_has_the_screen() {
    let design = scratch("rominabox-volume-omitted");
    fs::write(
        design.join("design.json"),
        r#"{"screens":[{"id":"pause","panel":"pause-panel","heading":"PAUSED","footer":"ESC","button":"back"}]}"#,
    )
    .unwrap();
    let screens = rominabox_desktop::themes::declared_screens(&design).unwrap();
    assert!(
        screens.iter().any(|screen| screen.id == "volume"),
        "omitting volume from the declaration must not omit the screen"
    );
    let pause = screens.iter().find(|screen| screen.id == "pause").unwrap();
    assert!(
        pause.button.split_whitespace().any(|button| button == "volume-back"),
        "pause has to be reachable from the volume screen, buttons are {}",
        pause.button
    );
    assert_eq!(pause.heading, "PAUSED", "ensuring volume must not rewrite the design's words");

    let document = r#"<body><button class="menu-action" id="quit">QUIT</button><div id="footer"></div></body>"#;
    let installed = rominabox_desktop::themes::install_volume_control(document, &design).unwrap();
    assert!(installed.contains("id=\"volume-panel\""));
    assert!(installed.contains("class=\"slider\""));
    assert!(installed.contains("id=\"volume\""));
}
