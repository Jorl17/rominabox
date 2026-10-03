//! A game exported with a patch beside it contains the patched game, as the
//! player sees it. The exported content is the result of the patch, and the
//! game is another game, with its own identity and storage.
#![cfg(target_os = "macos")]

mod export_fixture;
mod patch_writers;

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use export_fixture::{export_request, workspace};
use patch_writers::ips;

/// What `export_request` writes as the game, and the result of the patch.
const ORIGINAL: &[u8] = b"RIBtest";
const PATCHED: &[u8] = b"RIBpatched game";

fn bundle_identifier(app: &Path) -> String {
    let plist = fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
    let key = plist.find("<key>CFBundleIdentifier</key>").expect("a bundle identifier");
    let value = &plist[key..];
    let start = value.find("<string>").unwrap() + "<string>".len();
    let end = value.find("</string>").unwrap();
    value[start..end].to_string()
}

#[test]
fn a_patch_beside_the_game_is_applied_in_the_exported_game() {
    let root = workspace();
    let cancelled = AtomicBool::new(false);

    let mut plain = export_request(&root);
    plain.output_dir = root.join("plain");
    let unpatched = rominabox_engine::packaging::export_game(&plain, &cancelled, |_| {}).unwrap();
    let content = |app: &Path| fs::read(app.join("Contents/Resources/content/sonic.bin")).unwrap();
    assert_eq!(content(&unpatched.app_path), ORIGINAL);

    // RetroArch's convention: the patch has the game's name.
    fs::write(root.join("sonic.ips"), ips(ORIGINAL, PATCHED)).unwrap();
    let mut with_patch = export_request(&root);
    with_patch.output_dir = root.join("patched");
    let patched = rominabox_engine::packaging::export_game(&with_patch, &cancelled, |_| {}).unwrap();

    assert_eq!(content(&patched.app_path), PATCHED, "the exported game is the patched one");
    assert!(
        !patched.app_path.join("Contents/Resources/content/sonic.ips").exists(),
        "the patch itself does not travel: the game it made does"
    );
    assert_ne!(
        bundle_identifier(&patched.app_path),
        bundle_identifier(&unpatched.app_path),
        "a patched game keeps its own saves and storage"
    );
}

#[test]
fn the_details_step_names_the_patch_that_travels() {
    let root = workspace();
    let rom = root.join("sonic.bin");
    fs::write(&rom, ORIGINAL).unwrap();
    fs::write(root.join("sonic.ips"), ips(ORIGINAL, PATCHED)).unwrap();
    let traveling = rominabox_engine::traveling::files_for(&rom, Some("megadrive")).unwrap();
    assert_eq!(traveling.names(), vec!["sonic.bin".to_string()]);
    assert_eq!(traveling.patches, vec!["sonic.ips".to_string()]);
}

/// We apply a patch that the author chose when we export the game. When it
/// fails, we refuse the export and name the patch, here an xdelta patch for
/// another game, whose header contains the name of a file other than this game.
#[test]
fn a_chosen_patch_that_fails_refuses_the_export_and_names_the_patch() {
    let root = workspace();
    let cancelled = AtomicBool::new(false);
    let elsewhere = root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let patch = elsewhere.join("Director's Cut.xdelta");
    fs::write(
        &patch,
        fs::read(rominabox_engine::repo::at("desktop/crates/rominabox-engine/tests/fixtures/patches/test-game-lzma.xdelta"))
            .unwrap(),
    )
    .unwrap();
    let mut request = export_request(&root);
    request.game.files.added = vec![patch];

    let traveling = rominabox_engine::traveling::files_with(&request.game.rom, Some("megadrive"), &request.game.files).unwrap();
    assert_eq!(traveling.patches, vec!["Director's Cut.xdelta".to_string()], "nothing it states rules the game out");
    let refused = rominabox_engine::packaging::export_game(&request, &cancelled, |_| {}).unwrap_err();
    assert_eq!(refused.stage, rominabox_engine::packaging::ErrorStage::Refused);
    assert!(refused.message.contains("Director's Cut.xdelta does not apply to sonic.bin"), "{}", refused.message);
    assert!(!request.output_dir.join("Hotkey Isolation.app").exists(), "no game is left half made");
}
