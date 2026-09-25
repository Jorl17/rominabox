//! The volume control is the slider of the design, with an arrow on each side.
//!
//! It is in Options, without a screen, a number or a mute switch. When a
//! design changes the style of the slider, the volume control changes too.

use std::fs;

mod support;

fn scratch(name: &str) -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir(name)
}

/// A design without any words of its own.
fn english() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::new()
}

/// Where we draw the settings in a bare design, in English.
fn place(design: &std::path::Path) -> rominabox_desktop::menu::SettingsPlace<'_> {
    static ENGLISH: std::sync::OnceLock<std::collections::BTreeMap<String, String>> =
        std::sync::OnceLock::new();
    rominabox_desktop::menu::SettingsPlace {
        design,
        words: ENGLISH.get_or_init(english),
    }
}

/// A design package with no parts of its own, in the place it has in a kit,
/// beside the shared parts.
fn bare_design(root: &std::path::Path) -> std::path::PathBuf {
    support::copy_tree(&rominabox_desktop::repo::at("integrations/parts"), &root.join("parts"));
    let design = root.join("designs/bare");
    fs::create_dir_all(&design).unwrap();
    fs::write(design.join("design.json"), "{}").unwrap();
    design
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

    let markup = rominabox_desktop::menu::volume_control_markup(&design, &english()).unwrap();
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
    let root = scratch("rominabox-volume-builtin");
    let design = bare_design(&root);
    let markup = rominabox_desktop::menu::volume_control_markup(&design, &english()).unwrap();
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
    let error = rominabox_desktop::menu::volume_control_markup(&design, &english())
        .expect_err("a slider with none of the part's classes is not a slider");
    assert!(
        error.contains("slider"),
        "the refusal should say which part is wrong: {error}"
    );
}

/// We draw a slider even for a design without a style for it, because we link
/// the geometry of the shared part before the design's stylesheet. A design
/// that styles the slider overrides it with ordinary rules of its own.
#[test]
fn the_slider_part_is_linked_before_the_designs_stylesheet() {
    let root = scratch("rominabox-volume-part-sheet");
    let composed = rominabox_desktop::menu::compose_menu(&rominabox_desktop::menu::MenuRequest::new(
        rominabox_desktop::repo::at("integrations/designs/disc"),
        support::artwork(),
    ))
    .unwrap();
    composed.write(&root).unwrap();
    let menu = composed.text("menu.rml").unwrap();
    let part = menu
        .find("href=\"parts/slider.rcss\"")
        .expect("the slider part is linked");
    let design = menu.find("href=\"menu.rcss\"").expect("the design's stylesheet");
    assert!(part < design, "the part comes first, so the design can restyle it");
    let sheet = fs::read_to_string(root.join("parts/slider.rcss")).unwrap();
    assert!(sheet.contains(".slider-track"), "{sheet}");
    assert!(
        !composed.text("menu.rcss").unwrap().contains("part:slider"),
        "nothing is pasted after the design's own rules"
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
    let root = scratch("rominabox-volume-in-options");
    let design = bare_design(&root);
    let document = r#"<body><div id="options-panel" style="display:none;"><div id="options-entries"><button class="menu-action option-entry" id="controls">CONTROLS</button></div></div><div id="footer"></div></body>"#;
    let installed = rominabox_desktop::menu::install_settings(document, &place(&design), &[rominabox_desktop::player_settings::volume()]).unwrap();
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
    let root = scratch("rominabox-volume-nowhere");
    let design = bare_design(&root);
    let document = r#"<body><button class="menu-action" id="quit">QUIT</button><div id="footer"></div></body>"#;
    let installed = rominabox_desktop::menu::install_settings(document, &place(&design), &[rominabox_desktop::player_settings::volume()]).unwrap();
    assert!(
        !installed.contains("volume"),
        "volume lives in Options, so a menu without that screen does not grow one, got {installed}"
    );
}
