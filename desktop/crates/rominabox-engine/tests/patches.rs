//! Which patches we export with a game: the ones beside it that clearly
//! belong to it, and the ones its author chose, applied in name order.
//! We decide from what each patch states whether it belongs, and the result
//! is the game that we export from the patches that belong.

use std::fs;
use std::path::{Path, PathBuf};

use rominabox_engine::patches::{belonging, is_patch_file, xdelta_names, Offered};
use rominabox_engine::{patching, repo};
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

/// The game that we export from `game` with `patches`, made as in an export.
fn made(game: &Path, patches: &[PathBuf]) -> Vec<u8> {
    let made = game.with_file_name(format!("made-{}", patches.len()));
    let paths: Vec<&Path> = patches.iter().map(PathBuf::as_path).collect();
    patching::apply_files(game, &paths, &made).unwrap_or_else(|(index, failure)| panic!("patch {index}: {failure:?}"));
    let bytes = fs::read(&made).unwrap();
    fs::remove_file(&made).unwrap();
    bytes
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

    let result = belonging(&game, &beside(&[&mine, &theirs])).unwrap();
    assert_eq!(result.patches, vec![mine]);
    assert!(made(&game, &result.patches) == patched());
}

#[test]
fn an_ips_beside_the_game_travels_only_under_the_games_name() {
    let (root, game) = folder("ips");
    let named = root.join("test-game.ips");
    let unnamed = root.join("Some patch.ips");
    fs::write(&named, ips(&original(), &patched())).unwrap();
    fs::write(&unnamed, ips(&original(), &patched())).unwrap();

    assert!(belonging(&game, &beside(&[&unnamed])).unwrap().patches.is_empty());
    let result = belonging(&game, &beside(&[&named])).unwrap();
    assert_eq!(result.patches, vec![named]);
    assert!(made(&game, &result.patches) == patched());
}

#[test]
fn a_patch_the_author_chose_travels_when_nothing_it_states_rules_the_game_out() {
    let (root, game) = folder("chosen");
    let chosen = root.join("Some patch.ips");
    fs::write(&chosen, ips(&original(), &patched())).unwrap();
    let result = belonging(&game, &[(chosen.clone(), Offered::Chosen)]).unwrap();
    assert_eq!(result.patches, vec![chosen]);
}

#[test]
fn an_xdelta_patch_beside_the_game_travels_when_its_header_names_the_game() {
    let (root, game) = folder("xdelta");
    // Made by the xdelta command with its defaults, so with names and checksums.
    let named = root.join("Director's Cut.xdelta");
    fs::write(&named, fixture("test-game-lzma.xdelta")).unwrap();
    let result = belonging(&game, &beside(&[&named])).unwrap();
    assert!(made(&game, &result.patches) == patched());
    assert_eq!(result.made.as_deref(), Some("test-game-patched.gbc"), "the header names the game it makes");

    // -n -A: no checksums and no names, so nothing ties it to this game.
    let bare = root.join("Unnamed.xdelta");
    fs::write(&bare, fixture("test-game-bare.xdelta")).unwrap();
    assert!(belonging(&game, &beside(&[&bare])).unwrap().patches.is_empty());
    let chosen = belonging(&game, &[(bare.clone(), Offered::Chosen)]).unwrap();
    assert!(made(&game, &chosen.patches) == patched(), "chosen, it applies");
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

    let result = belonging(&game, &beside(&[&second, &first])).unwrap();
    assert_eq!(result.patches, vec![first, second]);
    assert!(made(&game, &result.patches) == last);
}
