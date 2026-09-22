//! The volume control is the slider of the design, with an arrow on each side.
//!
//! It is in Options, without a screen, a number or a mute switch. When a
//! design changes the style of the slider, the volume control changes too.

use std::fs;
use std::path::Path;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn volume_is_built_from_the_designs_slider() {
    let design = scratch("rominabox-volume-parts");
    fs::create_dir(design.join("parts")).unwrap();
    fs::write(
        design.join("parts/slider.rml"),
        r#"<div id="PART-ID" class="slider owned-slider"><div class="slider-track"><div class="slider-fill"></div><div class="slider-thumb"></div></div><div class="slider-readout"></div></div>"#,
    )
    .unwrap();

    let markup = rominabox_desktop::themes::volume_control_markup(&design).unwrap();
    assert!(
        markup.contains("owned-slider"),
        "a design's own slider has to be the one volume uses, got {markup}"
    );
    assert!(markup.contains("id=\"volume-level\""));
    assert!(markup.contains("id=\"volume-down\""));
    assert!(markup.contains("id=\"volume-up\""));
    assert!(markup.contains("id=\"volume-low\""));
    assert!(markup.contains("id=\"volume-high\""));
    assert!(
        markup.contains(">LOW<") && markup.contains(">HIGH<"),
        "the ends say low and high, got {markup}"
    );
    assert!(
        !markup.contains("MUTE") && !markup.contains("dB") && !markup.contains("toggle"),
        "there is no mute and no decibel readout, got {markup}"
    );
    assert!(
        !markup.contains("PART-ID"),
        "the holes have to be filled, got {markup}"
    );
}

#[test]
fn a_design_with_no_parts_still_gets_a_slider() {
    let design = scratch("rominabox-volume-builtin");
    let markup = rominabox_desktop::themes::volume_control_markup(&design).unwrap();
    for class in ["slider", "slider-track", "slider-fill", "slider-thumb", "slider-readout", "volume-arrow"] {
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
        rominabox_desktop::repo::at("integrations/designs/native/menu.rcss"),
    )
    .unwrap();
    assert!(
        css.contains(".slider"),
        "the design has to say what a slider looks like"
    );
    assert!(
        css.contains(".volume-arrow"),
        "the design has to say what the arrows look like"
    );
    assert!(
        css.contains(".volume-name"),
        "the bar has to be named in the design"
    );
    let rule = |selector: &str| {
        let at = css
            .find(selector)
            .unwrap_or_else(|| panic!("{selector} is not in the design"));
        let rest = &css[at + selector.len()..];
        let end = rest.find('}').unwrap_or(rest.len());
        rest[..end].to_string()
    };
    let volume = rule("#volume-control {");
    let entries = rule("#options-entries {");
    assert!(
        volume.contains("left: 56dp;")
            && volume.contains("width: 840dp;")
            && entries.contains("left: 56dp;")
            && entries.contains("width: 840dp;"),
        "volume and the entry under it have to share a column.\nvolume: {volume}\nentries: {entries}"
    );
    assert!(
        !css.contains("#volume-level"),
        "volume-specific styling is the slider failing to be a part"
    );
    let literal = css.match_indices('#').any(|(at, _)| {
        let rest = &css[at + 1..];
        let token: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if !matches!(token.len(), 3 | 6 | 8) {
            return false;
        }
        // #volume-control starts with three hex digits and then continues.
        // A colour token ends after its digits.
        let next = rest[token.len()..].chars().next();
        !matches!(next, Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    assert!(
        !literal,
        "a colour in this design is a token, written design(name)"
    );
}

#[test]
fn volume_drops_into_options_and_does_not_open_a_screen() {
    let design = scratch("rominabox-volume-in-options");
    fs::write(design.join("design.json"), "{}").unwrap();
    let document = r#"<body><div id="options-panel" style="display:none;"><div id="options-entries"><button class="menu-action option-entry" id="controls">CONTROLS</button></div></div><div id="footer"></div></body>"#;
    let installed = rominabox_desktop::themes::install_volume_control(document, &design).unwrap();
    let panel = installed.find("id=\"options-panel\"").expect("options panel");
    let control = installed.find("id=\"volume-control\"").expect("the control");
    let entries = installed.find("id=\"options-entries\"").expect("the links");
    assert!(
        panel < control && control < entries,
        "volume sits in the options panel, ahead of the links, got {installed}"
    );
    assert!(installed.contains("id=\"controls\""), "the links stay, got {installed}");
    assert!(!installed.contains("id=\"volume-panel\""));
    assert!(!installed.contains("id=\"volume-mute\""));
    assert!(
        installed.contains(">VOLUME<"),
        "the control has to say what it is, got {installed}"
    );
}

#[test]
fn a_menu_with_no_options_screen_has_no_volume_control() {
    let design = scratch("rominabox-volume-nowhere");
    fs::write(design.join("design.json"), "{}").unwrap();
    let document = r#"<body><button class="menu-action" id="quit">QUIT</button><div id="footer"></div></body>"#;
    let installed = rominabox_desktop::themes::install_volume_control(document, &design).unwrap();
    assert!(
        !installed.contains("volume"),
        "volume lives in Options, so a menu without that screen does not grow one, got {installed}"
    );
}
