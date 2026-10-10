use super::*;

fn package(root: &Path, name: &str, json: &str) -> PathBuf {
    let design = root.join(name);
    fs::create_dir_all(&design).unwrap();
    fs::write(design.join("design.json"), json).unwrap();
    design
}

fn with_native(root: &Path) {
    let native = root.join("native");
    fs::create_dir_all(&native).unwrap();
    fs::copy(
        crate::repo::at("integrations/designs/native/design.json"),
        native.join("design.json"),
    )
    .unwrap();
}

#[test]
fn an_unknown_key_is_refused_with_the_file_that_holds_it() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-unknown");
    with_native(&root);
    let design = package(&root, "typo", r#"{"screnes": []}"#);
    let error = Manifest::load(&design).unwrap_err();
    assert!(error.contains("screnes"), "{error}");
    assert!(error.contains("typo"), "{error}");
}

#[test]
fn a_design_cannot_assign_rows() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-rows");
    with_native(&root);
    let design = package(
        &root,
        "own",
        r#"{"screens": [{"id": "credits", "panel": "credits-panel", "heading": "C", "footer": "F", "rows": "licences"}]}"#,
    );
    let error = Manifest::load(&design).err().expect("a design assigned rows");
    assert!(error.contains("declares rows"), "{error}");
}

#[test]
fn a_design_inherits_every_field_it_does_not_declare() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-inherit");
    with_native(&root);
    let design = package(
        &root,
        "bare",
        r#"{"metrics": {"marker": {"diameter": 30}}}"#,
    );
    let bare = Manifest::load(&design).unwrap();
    let native = Manifest::load(&root.join("native")).unwrap();
    assert_eq!(bare.scene.marker, 30);
    assert_eq!(bare.scene.scene_width, native.scene.scene_width);
    assert_eq!(bare.binds.hover_after_ms, native.binds.hover_after_ms);
    assert_eq!(bare.screens.len(), native.screens.len());
    assert_eq!(bare.fonts.len(), native.fonts.len());
    assert_eq!(bare.documents.style, "menu.rcss");
}

#[test]
fn a_null_option_removes_the_inherited_entry() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-null");
    with_native(&root);
    let design = package(
        &root,
        "nulled",
        r#"{"screens": [{"id": "discs", "option": null}]}"#,
    );
    let discs = declared_screens(&design)
        .unwrap()
        .into_iter()
        .find(|screen| screen.id == "discs")
        .unwrap();
    assert_eq!(discs.option_label, None);
    assert_eq!(discs.role, Some(ScreenRole::Discs), "the role is inherited");
}

#[test]
fn a_design_words_only_what_the_player_writes() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-words");
    with_native(&root);
    let worded = package(&root, "worded", r#"{"words": {"slot": "BLOCK {slot}"}}"#);
    assert_eq!(Manifest::load(&worded).unwrap().words["slot"], "BLOCK {slot}");
    let misworded = package(&root, "misworded", r#"{"words": {"slots": "BLOCK"}}"#);
    let error = Manifest::load(&misworded).unwrap_err();
    assert!(error.contains("'slots'") && error.contains("misworded"), "{error}");
}

/// We draw every word in the player with the fonts in a design, and do not
/// start a menu without fonts. An export of such a design would contain a
/// game with no menu at all.
#[test]
fn a_design_that_lists_no_font_is_refused() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-fontless");
    with_native(&root);
    let design = package(&root, "fontless", r#"{"fonts": []}"#);
    let error = Manifest::load(&design).expect_err("a design with no font was accepted");
    assert!(error.contains("fontless") && error.contains("font"), "{error}");
}

#[test]
fn only_native_assigns_roles() {
    let root = rominabox_scratch::Scratch::dir("rominabox-manifest-role");
    with_native(&root);
    let design = package(
        &root,
        "claims",
        r#"{"screens": [{"id": "mine", "role": "pause", "panel": "mine-panel", "heading": "M", "footer": "F"}]}"#,
    );
    let error = Manifest::load(&design).unwrap_err();
    assert!(error.contains("only Native assigns roles"), "{error}");
}
