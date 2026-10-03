//! What goes with a game when its author changes it or drops a patch. We find
//! the game for a patch dropped on its own, the author can leave out a
//! companion or a patch when the game runs without it, and an added file goes too.

use std::fs;
use std::path::PathBuf;

use rominabox_engine::content::{FileRole, GameFiles};
use rominabox_engine::traveling::files_with;
use rominabox_scratch::Scratch;

mod patch_writers;
use patch_writers::{bps, ips};

const ORIGINAL: &[u8] = b"Tiny Blast, as it shipped";
const PATCHED: &[u8] = b"Tiny Blast, as its director meant it";

fn none() -> GameFiles {
    GameFiles::default()
}

#[test]
fn a_patch_dropped_on_its_own_finds_the_game_it_is_for_and_travels_with_it() {
    let root = Scratch::dir("rominabox-files-patch-alone");
    let game = root.join("Tiny Blast (USA).md");
    fs::write(&game, ORIGINAL).unwrap();
    fs::write(root.join("Another Game (USA).md"), b"another game, another size").unwrap();
    let patch = root.join("Director's Cut.bps");
    fs::write(&patch, bps(ORIGINAL, PATCHED)).unwrap();

    let traveling = files_with(&patch, Some("megadrive"), &none()).unwrap();
    assert_eq!(traveling.entry.file_name().unwrap(), "Tiny Blast (USA).md");
    assert_eq!(traveling.patches, vec!["Director's Cut.bps".to_string()]);
    assert_eq!(traveling.added.len(), 1, "the dropped patch is the author's choice");
}

#[test]
fn an_ips_dropped_on_its_own_finds_the_game_of_its_name() {
    let root = Scratch::dir("rominabox-files-ips-alone");
    fs::write(root.join("Tiny Blast (USA).md"), ORIGINAL).unwrap();
    fs::write(root.join("Other (USA).md"), ORIGINAL).unwrap();
    let patch = root.join("Tiny Blast (USA).ips");
    fs::write(&patch, ips(ORIGINAL, PATCHED)).unwrap();

    let traveling = files_with(&patch, Some("megadrive"), &none()).unwrap();
    assert_eq!(traveling.entry.file_name().unwrap(), "Tiny Blast (USA).md");
    assert_eq!(traveling.patches, vec!["Tiny Blast (USA).ips".to_string()]);
}

#[test]
fn a_patch_for_no_game_in_its_folder_says_so() {
    let root = Scratch::dir("rominabox-files-patch-orphan");
    fs::write(root.join("Tiny Blast (USA).md"), b"not the game the patch was made for").unwrap();
    let patch = root.join("Director's Cut.bps");
    fs::write(&patch, bps(ORIGINAL, PATCHED)).unwrap();
    let refused = files_with(&patch, Some("megadrive"), &none()).unwrap_err();
    assert!(refused.contains("is for none of the games in its folder"), "{refused}");
}

#[test]
fn a_patch_beside_the_game_can_be_left_out() {
    let root = Scratch::dir("rominabox-files-patch-left-out");
    let game = root.join("Tiny Blast (USA).md");
    fs::write(&game, ORIGINAL).unwrap();
    fs::write(root.join("Director's Cut.bps"), bps(ORIGINAL, PATCHED)).unwrap();
    let left_out = GameFiles { left_out: vec!["Director's Cut.bps".into()], added: Vec::new() };
    assert!(files_with(&game, Some("megadrive"), &left_out).unwrap().patches.is_empty());
}

#[test]
fn a_companion_the_console_brings_can_be_left_out_and_one_it_needs_cannot() {
    let root = Scratch::dir("rominabox-files-companions");
    fs::write(root.join("Disc.cue"), "FILE \"Disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
    fs::write(root.join("Disc.bin"), b"track").unwrap();
    fs::write(root.join("Disc.sbi"), b"subchannel").unwrap();
    let cue = root.join("Disc.cue");

    let kept = files_with(&cue, Some("ps1"), &none()).unwrap();
    let roles: Vec<(String, FileRole)> = kept.files.iter().map(|file| (file.name.clone(), file.role)).collect();
    assert!(roles.contains(&("Disc.sbi".into(), FileRole::Companion { required: false })), "{roles:?}");
    assert!(roles.contains(&("Disc.bin".into(), FileRole::Named)), "{roles:?}");

    let without = GameFiles { left_out: vec!["Disc.sbi".into()], added: Vec::new() };
    assert!(!files_with(&cue, Some("ps1"), &without).unwrap().names().contains(&"Disc.sbi".to_string()));

    fs::write(root.join("Game.ccd"), "[CloneCD]\n").unwrap();
    fs::write(root.join("Game.img"), b"image").unwrap();
    let needed = GameFiles { left_out: vec!["Game.img".into()], added: Vec::new() };
    let refused = files_with(&root.join("Game.ccd"), Some("ps1"), &needed).unwrap_err();
    assert!(refused.contains("Game.img cannot be left out"), "{refused}");
}

#[test]
fn an_added_file_travels_and_an_added_patch_that_does_not_apply_is_named() {
    let root = Scratch::dir("rominabox-files-added");
    let game = root.join("Tiny Blast (USA).md");
    fs::write(&game, ORIGINAL).unwrap();
    let elsewhere = Scratch::dir("rominabox-files-added-elsewhere");
    let manual: PathBuf = elsewhere.join("Manual.txt");
    fs::write(&manual, b"read me").unwrap();
    let wrong = elsewhere.join("Wrong game.bps");
    fs::write(&wrong, bps(b"some other game entirely", PATCHED)).unwrap();

    let choices = GameFiles { left_out: Vec::new(), added: vec![manual, wrong] };
    let traveling = files_with(&game, Some("megadrive"), &choices).unwrap();
    let added: Vec<&str> = traveling
        .files
        .iter()
        .filter(|file| file.role == FileRole::Added)
        .map(|file| file.name.as_str())
        .collect();
    assert_eq!(added, vec!["Manual.txt"]);
    assert_eq!(traveling.refused, vec!["Wrong game.bps".to_string()]);
    assert!(traveling.patches.is_empty());
}

/// A sheet lists the game's files, and a patch does not change it. An IPS with
/// a playlist's name stays beside it, and the playlist goes unchanged.
#[test]
fn a_patch_under_a_sheets_name_does_not_change_the_sheet() {
    let root = Scratch::dir("rominabox-files-sheet-patch");
    fs::write(root.join("Disc.cue"), "FILE \"Disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
    fs::write(root.join("Disc.bin"), b"track").unwrap();
    let playlist = root.join("Game.m3u");
    fs::write(&playlist, "Disc.cue\n").unwrap();
    fs::write(root.join("Game.ips"), ips(b"Disc.cue\n", b"Patched.cue\n")).unwrap();

    let traveling = files_with(&playlist, Some("ps1"), &none()).unwrap();
    assert!(traveling.patches.is_empty(), "{:?}", traveling.patches);
}
