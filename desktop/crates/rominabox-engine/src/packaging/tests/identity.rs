//! We store the saves of every exported game under
//! `Games/{stable_identity(rom, [], system, title)}`. We hash the system
//! string as given, trimmed and lowercased but NOT canonicalised to a console
//! id, so the value that callers pass is part of the save path of a player
//! forever. These cases test that behaviour on purpose.
//!
//! Do not resolve aliases first to tidy this. Aliases are valid in
//! `systems::find`, so canonicalising here would move the saves of every
//! existing player to a new directory and leave the old one without warning.

use super::*;

fn rom_with(bytes: &[u8]) -> (rominabox_scratch::Scratch, PathBuf) {
    let dir = rominabox_scratch::Scratch::dir("rominabox-identity");
    let rom = dir.join("game.bin");
    fs::write(&rom, bytes).unwrap();
    (dir, rom)
}

/// When a game file disappears while we read its identity, we do not report
/// a full save folder.
#[test]
fn a_game_file_that_has_gone_is_named() {
    let root = rominabox_scratch::Scratch::dir("rominabox-identity");
    let error = stable_identity(&root.join("Sonic.md"), &[], "megadrive", "Sonic", None).unwrap_err();
    assert_eq!(error.stage, ErrorStage::Missing);
    assert!(error.sentence().contains("\u{201c}Sonic.md\u{201d}"), "{}", error.sentence());
}

#[test]
fn identity_is_stable_for_the_same_rom_and_system() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let first = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    let second = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.len(),
        24,
        "save directory names must stay 24 hex chars"
    );
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
}

/// Without a namespace, nothing changes.
///
/// Every ordinary export has no namespace, so the saves of every existing
/// player stay where they are. A failure here means that save directories
/// have moved in ordinary exports.
#[test]
fn an_absent_namespace_leaves_the_identity_exactly_as_it_was() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    // We compare with a fixed value instead of with itself, because if we
    // computed both sides, a change of hash would go unnoticed.
    let identity = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    assert_eq!(identity.len(), 24);
    assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(identity, "00fa33a8ceceb6c271e9b04f");
}

/// Two checkouts do not share a game's data directory.
///
/// With a shared directory, the launcher from one checkout could rewrite
/// retroarch.cfg under a running game from the other, and both would append
/// to one launch.log.
#[test]
fn a_namespace_gives_the_same_game_a_separate_home() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let shared = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    let first =
        stable_identity(&rom, &[], "megadrive", "Sonic", Some("app.rominabox.game.wt-a")).unwrap();
    let second =
        stable_identity(&rom, &[], "megadrive", "Sonic", Some("app.rominabox.game.wt-b")).unwrap();

    assert_ne!(
        first, shared,
        "a namespaced export must not land on the shared home"
    );
    assert_ne!(
        first, second,
        "two worktrees must not share one game's home"
    );
    assert_eq!(
        stable_identity(&rom, &[], "megadrive", "Sonic", Some("  ")).unwrap(),
        shared,
        "an empty namespace is no namespace, not a third directory"
    );
}

/// The bundle identifier comes from the identity, so one namespace separates
/// both.
///
/// It is `app.rominabox.game.{identity}`, the key for the app in
/// LaunchServices. When two checkouts share one identifier, opening the game
/// from one checkout brings up the running game from the other, and an old
/// game still running with the identifier stops a rebuilt game from launching.
#[test]
fn the_bundle_identifier_is_namespaced_with_the_identity() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let shared = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    let isolated =
        stable_identity(&rom, &[], "megadrive", "Sonic", Some("app.rominabox.game.wt-a")).unwrap();
    let identifier = |identity: &str| format!("app.rominabox.game.{identity}");
    assert_ne!(
        identifier(&shared),
        identifier(&isolated),
        "two checkouts of the same game must not claim one bundle identifier"
    );
}

#[test]
fn identity_ignores_surrounding_space_and_letter_case() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let canonical = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    assert_eq!(
        stable_identity(&rom, &[], "  MegaDrive  ", "Sonic", None).unwrap(),
        canonical
    );
    assert_eq!(stable_identity(&rom, &[], "MEGADRIVE", "Sonic", None).unwrap(), canonical);
}

/// A deliberate difference. `gb` and its alias `Game Boy` both resolve to the
/// same console, but give DIFFERENT save directories for the same ROM bytes,
/// so callers must pass the canonical id. With this test, nobody can remove
/// the difference without also moving the data of existing players.
#[test]
fn an_alias_does_not_share_a_save_directory_with_its_canonical_id() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let canonical = crate::systems::find("gb").expect("gb is a known system");
    let via_alias = crate::systems::find("Game Boy").expect("alias resolves");
    assert_eq!(
        canonical.id, via_alias.id,
        "both spellings must resolve to one console"
    );
    assert_ne!(
        stable_identity(&rom, &[], "gb", "Sonic", None).unwrap(),
        stable_identity(&rom, &[], "Game Boy", "Sonic", None).unwrap(),
        "identity hashes the supplied string, not the resolved console id"
    );
}

#[test]
fn a_different_system_or_different_bytes_changes_the_identity() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let (_other, other_rom) = rom_with(b"rominabox-identity-fixture-2");
    let base = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    assert_ne!(stable_identity(&rom, &[], "nes", "Sonic", None).unwrap(), base);
    assert_ne!(
        stable_identity(&other_rom, &[], "megadrive", "Sonic", None).unwrap(),
        base
    );
}

/// The same ROM under another title is another game, with a separate data
/// folder, sandbox and app ID. With the same title it is the same game, so a
/// player keeps the saves when the author exports it again.
#[test]
fn another_title_is_another_game() {
    let (_dir, rom) = rom_with(b"rominabox-identity-fixture");
    let game = stable_identity(&rom, &[], "megadrive", "Sonic", None).unwrap();
    assert_ne!(stable_identity(&rom, &[], "megadrive", "Sonic CRT", None).unwrap(), game);
    assert_eq!(stable_identity(&rom, &[], "megadrive", "  Sonic  ", None).unwrap(), game);
}

/// A patched game is a separate game. We make its identity from the bytes of
/// the original followed by those of each patch, in the order we apply them,
/// wherever the patches are, without making the patched game.
#[test]
fn a_patched_game_is_identified_by_the_original_and_its_patches() {
    let (dir, rom) = rom_with(b"rominabox-identity-fixture");
    let (first, second) = (dir.join("1.ips"), dir.join("2.ips"));
    fs::write(&first, b"PATCH first EOF").unwrap();
    fs::write(&second, b"PATCH second EOF").unwrap();
    let (_elsewhere, moved) = rom_with(b"PATCH first EOF");
    let identity = |patches: &[PathBuf]| stable_identity(&rom, patches, "megadrive", "Sonic", None).unwrap();

    assert_ne!(identity(&[first.clone()]), identity(&[]));
    assert_eq!(identity(&[first.clone()]), identity(&[moved]), "the patch's bytes, not where it lies");
    assert_ne!(identity(&[first.clone(), second.clone()]), identity(&[second, first]));
}
