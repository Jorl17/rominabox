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
