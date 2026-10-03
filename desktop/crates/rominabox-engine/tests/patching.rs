//! When we apply a patch in the engine, we get the game its author made, with
//! the RetroArch code (src/patching.rs) for every format it supports, and we
//! refuse a patch made for another game.
//!
//! We write the IPS, UPS and BPS patches here from the published layouts of
//! the formats. We made the xdelta ones in tests/fixtures/patches from the
//! same two cartridges with the xdelta3 command (3.2.1): `xdelta3 -e -S none`,
//! `-S djw` and `-S lzma`. Without -S, the command compresses with LZMA.

use rominabox_engine::patching::{apply, PatchError, PatchFormat, XdeltaCompression};
use rominabox_engine::repo;

mod patch_writers;
use patch_writers::{bps, ips, ups};

fn original() -> Vec<u8> {
    std::fs::read(repo::root().join("scripts/fixtures/test-game.gbc")).unwrap()
}

/// The test cartridge as every patch here changes it: a new title in its
/// header, and 4 KiB of new data past its end, enough for xdelta to compress.
fn patched() -> Vec<u8> {
    let mut game = original();
    game[0x134..0x13B].copy_from_slice(b"PATCHED");
    game.extend(b"ROM-in-a-Box patch ".iter().cycle().take(4096));
    game
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(repo::root().join("desktop/crates/rominabox-engine/tests/fixtures/patches").join(name)).unwrap()
}

#[test]
fn every_format_turns_the_test_cartridge_into_the_patched_one() {
    let (source, target) = (original(), patched());
    let patches = [
        ("IPS", ips(&source, &target), PatchFormat::Ips),
        ("UPS", ups(&source, &target), PatchFormat::Ups),
        ("BPS", bps(&source, &target), PatchFormat::Bps),
        ("xdelta", fixture("test-game-none.xdelta"), PatchFormat::Xdelta),
        ("xdelta with LZMA", fixture("test-game-lzma.xdelta"), PatchFormat::Xdelta),
    ];
    for (name, patch, format) in patches {
        assert_eq!(PatchFormat::of(&patch), Some(format), "{name} is read as {format:?}");
        let result = apply(&patch, &source).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(result == target, "{name} gave {} bytes, not the patched cartridge", result.len());
    }
}

#[test]
fn an_xdelta_patch_says_how_it_is_compressed() {
    assert_eq!(XdeltaCompression::of(&fixture("test-game-none.xdelta")), Some(XdeltaCompression::None));
    assert_eq!(XdeltaCompression::of(&fixture("test-game-djw.xdelta")), Some(XdeltaCompression::Djw));
    assert_eq!(XdeltaCompression::of(&fixture("test-game-lzma.xdelta")), Some(XdeltaCompression::Lzma));
    assert_eq!(XdeltaCompression::of(&original()), None);
}

/// DJW is the xdelta Huffman coder, which a patch author must ask for.
/// RetroArch cannot read it, and we name the compression when we refuse it.
#[test]
fn an_xdelta_patch_compressed_with_djw_is_refused_and_says_so() {
    assert_eq!(
        apply(&fixture("test-game-djw.xdelta"), &original()),
        Err(PatchError::DoesNotApply { compression: Some(XdeltaCompression::Djw) })
    );
}

#[test]
fn a_patch_with_checks_refuses_another_game() {
    let (source, target) = (original(), patched());
    let mut other = source.clone();
    other[0x150] ^= 0xFF;
    let refused = PatchError::DoesNotApply { compression: None };
    assert_eq!(apply(&bps(&source, &target), &other), Err(refused));
    assert_eq!(apply(&ups(&source, &target), &other), Err(refused));
    for (name, compression) in [("test-game-none.xdelta", XdeltaCompression::None), ("test-game-lzma.xdelta", XdeltaCompression::Lzma)] {
        assert_eq!(
            apply(&fixture(name), &other),
            Err(PatchError::DoesNotApply { compression: Some(compression) }),
            "{name} refuses a game it was not made for"
        );
    }
}

#[test]
fn a_file_that_is_no_patch_is_refused() {
    let game = original();
    assert_eq!(PatchFormat::of(&game), None);
    assert_eq!(apply(&game, &game), Err(PatchError::NotAPatch));
    assert_eq!(apply(b"", &game), Err(PatchError::NotAPatch));
}
