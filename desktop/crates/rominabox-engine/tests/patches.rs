//! Which patches we export with a game: the ones beside it that clearly
//! belong to it, and the ones its author chose, applied in name order.

use std::fs;
use std::path::{Path, PathBuf};

use rominabox_engine::patches::{apply_belonging, is_patch_file, xdelta_names, Offered};
use rominabox_engine::repo;
use rominabox_scratch::Scratch;

mod patch_writers;
use patch_writers::{bps, ips};

fn original() -> Vec<u8> {
    fs::read(repo::root().join("scripts/fixtures/test-game.gbc")).unwrap()
}

/// The cartridge the xdelta fixtures make (tests/patching.rs).
fn patched() -> Vec<u8> {
    let mut game = original();
    game[0x134..0x13B].copy_from_slice(b"PATCHED");
    game.extend(b"ROM-in-a-Box patch ".iter().cycle().take(4096));
    game
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(repo::root().join("desktop/crates/rominabox-engine/tests/fixtures/patches").join(name)).unwrap()
}

/// A folder with the test cartridge as `test-game.gbc`, the name that the
/// xdelta fixtures' headers give the game they start from.
fn folder(name: &str) -> (Scratch, PathBuf) {
    let root = Scratch::dir(&format!("rominabox-patches-{name}"));
    let game = root.join("test-game.gbc");
    fs::write(&game, original()).unwrap();
    (root, game)
}

fn beside(paths: &[&Path]) -> Vec<(PathBuf, Offered)> {
    paths.iter().map(|path| (path.to_path_buf(), Offered::Beside)).collect()
}

#[test]
fn patch_files_are_known_by_retroarchs_names() {
    for name in ["a.ips", "a.IPS", "a.ips1", "a.bps9", "a.ups", "a.xdelta", "a.vcdiff"] {
        assert!(is_patch_file(Path::new(name)), "{name}");
    }
    for name in ["a.ips10", "a.gbc", "a.zip", "a", "a.bpsx"] {
        assert!(!is_patch_file(Path::new(name)), "{name}");
    }
}

#[test]
fn a_bps_beside_the_game_travels_when_it_was_made_for_it() {
    let (root, game) = folder("bps");
    let mine = root.join("Translation.bps");
    fs::write(&mine, bps(&original(), &patched())).unwrap();
    let mut other_game = original();
    other_game[0x150] ^= 0xFF;
    let theirs = root.join("Another game's patch.bps");
    fs::write(&theirs, bps(&other_game, &patched())).unwrap();

    let result = apply_belonging(&game, &beside(&[&mine, &theirs])).unwrap();
    assert_eq!(result.patches, vec![mine]);
    assert!(result.bytes == patched());
}

#[test]
fn an_ips_beside_the_game_travels_only_under_the_games_name() {
    let (root, game) = folder("ips");
    let named = root.join("test-game.ips");
    let unnamed = root.join("Some patch.ips");
    fs::write(&named, ips(&original(), &patched())).unwrap();
    fs::write(&unnamed, ips(&original(), &patched())).unwrap();

    assert!(apply_belonging(&game, &beside(&[&unnamed])).unwrap().patches.is_empty());
    let result = apply_belonging(&game, &beside(&[&named])).unwrap();
    assert_eq!(result.patches, vec![named]);
    assert!(result.bytes == patched());
}

#[test]
fn a_patch_the_author_chose_travels_when_it_applies() {
    let (root, game) = folder("chosen");
    let chosen = root.join("Some patch.ips");
    fs::write(&chosen, ips(&original(), &patched())).unwrap();
    let result = apply_belonging(&game, &[(chosen.clone(), Offered::Chosen)]).unwrap();
    assert_eq!(result.patches, vec![chosen]);
}

#[test]
fn an_xdelta_patch_beside_the_game_travels_when_it_names_or_checks_the_game() {
    let (root, game) = folder("xdelta");
    // Made by the xdelta command with its defaults, so with names and checksums.
    let checked = root.join("Director's Cut.xdelta");
    fs::write(&checked, fixture("test-game-lzma.xdelta")).unwrap();
    let result = apply_belonging(&game, &beside(&[&checked])).unwrap();
    assert!(result.bytes == patched());
    assert_eq!(result.made.as_deref(), Some("test-game-patched.gbc"), "the header names the game it makes");

    // -n -A: no checksums and no names, so nothing ties it to this game.
    let bare = root.join("Unnamed.xdelta");
    fs::write(&bare, fixture("test-game-bare.xdelta")).unwrap();
    assert!(apply_belonging(&game, &beside(&[&bare])).unwrap().patches.is_empty());
    let chosen = apply_belonging(&game, &[(bare.clone(), Offered::Chosen)]).unwrap();
    assert!(chosen.bytes == patched(), "chosen, it applies");
}

#[test]
fn an_xdelta_header_names_both_games() {
    let names = xdelta_names(&fixture("test-game-lzma.xdelta")).unwrap();
    assert_eq!(names.made.as_deref(), Some("test-game-patched.gbc"));
    assert_eq!(names.from.as_deref(), Some("test-game.gbc"));
    assert!(names.checked);
    let bare = xdelta_names(&fixture("test-game-bare.xdelta")).unwrap();
    assert_eq!((bare.made, bare.from, bare.checked), (None, None, false));
}

/// A translation and the addendum made on top of it: each applies to what
/// the one before made.
#[test]
fn several_patches_are_applied_in_name_order_each_to_the_last_ones_game() {
    let (root, game) = folder("chain");
    let middle = patched();
    let mut last = middle.clone();
    last[0x200] = 0x42;
    let first = root.join("1 Translation.bps");
    let second = root.join("2 Addendum.bps");
    fs::write(&first, bps(&original(), &middle)).unwrap();
    fs::write(&second, bps(&middle, &last)).unwrap();

    let result = apply_belonging(&game, &beside(&[&second, &first])).unwrap();
    assert_eq!(result.patches, vec![first, second]);
    assert!(result.bytes == last);
}
