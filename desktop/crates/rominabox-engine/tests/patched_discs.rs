//! Patches for a disc. We apply each to the track it was made for, export the
//! other tracks and the sheet as they are, and name the patched disc after
//! the patch, because we recognise a disc by its serial, which a patch rarely
//! changes.
//!
//! We made `fixtures/patches/tiny-disc-track-3.xdelta` with the defaults of
//! xdelta3 3.2.1 from `TRACK_THREE` to `TRACK_THREE_PATCHED`, each in a file
//! named "Tiny Disc (Track 3).bin", so its header contains the name of that track.

use std::fs;
use std::path::{Path, PathBuf};

use rominabox_engine::content::{self, ContentSet, GameFiles, Staging};
use rominabox_engine::metadata::inspect_game;
use rominabox_scratch::Scratch;

mod patch_writers;
use patch_writers::{bps, ips};

const TRACK_THREE: &[u8] = b"Tiny Disc track three, as it shipped";
const TRACK_THREE_PATCHED: &[u8] = b"Tiny Disc track three, as its director meant it";

/// A GD-ROM in `root`: a small data track, a larger audio track, and the
/// high-density data track, `TRACK_THREE`.
fn gd_rom(root: &Path) -> PathBuf {
    let mut first = vec![0u8; 0x100];
    first[..15].copy_from_slice(b"SEGA SEGAKATANA");
    first[0x40..0x4A].copy_from_slice(b"T-00001   ");
    fs::write(root.join("Tiny Disc (Track 1).bin"), &first).unwrap();
    fs::write(root.join("Tiny Disc (Track 2).bin"), vec![7u8; 4096]).unwrap();
    fs::write(root.join("Tiny Disc (Track 3).bin"), TRACK_THREE).unwrap();
    let layout = root.join("Tiny Disc.gdi");
    fs::write(
        &layout,
        "3\n\
         1 0 4 2352 \"Tiny Disc (Track 1).bin\" 0\n\
         2 450 0 2352 \"Tiny Disc (Track 2).bin\" 0\n\
         3 45000 4 2352 \"Tiny Disc (Track 3).bin\" 0\n",
    )
    .unwrap();
    layout
}

/// Each file of `set` by name, with the patches staged onto it.
fn staged(set: &ContentSet) -> Vec<(String, Vec<String>)> {
    let name = |path: &Path| path.file_name().unwrap().to_string_lossy().into_owned();
    set.files
        .iter()
        .map(|file| {
            let patches = match &file.staging {
                Staging::Patched(patches) => patches.iter().map(|patch| name(patch)).collect(),
                _ => Vec::new(),
            };
            (name(&file.source), patches)
        })
        .collect()
}

fn patches_on(set: &ContentSet, track: &str) -> Vec<String> {
    staged(set).into_iter().find(|(name, _)| name == track).map(|(_, patches)| patches).unwrap_or_default()
}

#[test]
fn an_xdelta_beside_a_disc_goes_onto_the_track_its_header_names() {
    let root = Scratch::dir("rominabox-patched-disc-xdelta");
    let layout = gd_rom(&root);
    fs::copy(
        rominabox_engine::repo::at("desktop/crates/rominabox-engine/tests/fixtures/patches/tiny-disc-track-3.xdelta"),
        root.join("Tiny Disc (Director's Cut).xdelta"),
    )
    .unwrap();

    let set = content::collect(&layout).unwrap();
    assert_eq!(patches_on(&set, "Tiny Disc (Track 3).bin"), ["Tiny Disc (Director's Cut).xdelta"], "{:?}", staged(&set));
    for other in ["Tiny Disc.gdi", "Tiny Disc (Track 1).bin", "Tiny Disc (Track 2).bin"] {
        assert!(patches_on(&set, other).is_empty(), "{other} was given a patch: {:?}", staged(&set));
    }
    assert_eq!(set.patched_name, None, "a track keeps the name its sheet gives it");

    let made = root.join("made.bin");
    let patches: Vec<PathBuf> = set.patches();
    let paths: Vec<&Path> = patches.iter().map(PathBuf::as_path).collect();
    rominabox_engine::patching::apply_files(&root.join("Tiny Disc (Track 3).bin"), &paths, &made).unwrap();
    assert_eq!(fs::read(made).unwrap(), TRACK_THREE_PATCHED, "the fixture is what this test says it is");
}

#[test]
fn a_bps_beside_a_disc_goes_onto_the_track_whose_size_and_checksum_it_states() {
    let root = Scratch::dir("rominabox-patched-disc-bps");
    let layout = gd_rom(&root);
    let first = fs::read(root.join("Tiny Disc (Track 1).bin")).unwrap();
    let mut first_patched = first.clone();
    first_patched[0x80..0x89].copy_from_slice(b"TINY DISC");
    fs::write(root.join("Header.bps"), bps(&first, &first_patched)).unwrap();
    fs::write(root.join("Story.bps"), bps(TRACK_THREE, TRACK_THREE_PATCHED)).unwrap();
    // Made from neither track, because it is for another disc.
    fs::write(root.join("Other Disc.bps"), bps(b"another disc", b"another disc, patched")).unwrap();

    let set = content::collect(&layout).unwrap();
    assert_eq!(patches_on(&set, "Tiny Disc (Track 1).bin"), ["Header.bps"], "{:?}", staged(&set));
    assert_eq!(patches_on(&set, "Tiny Disc (Track 3).bin"), ["Story.bps"], "{:?}", staged(&set));
    assert_eq!(set.patches().len(), 2, "the patch for another disc stays behind: {:?}", staged(&set));
}

#[test]
fn a_patch_the_author_chose_that_names_no_track_goes_onto_the_largest_data_track() {
    let root = Scratch::dir("rominabox-patched-disc-chosen");
    let layout = gd_rom(&root);
    // The high-density track is the larger data track, as on a real GD-ROM.
    let track_three = [TRACK_THREE, &[0u8; 1024]].concat();
    fs::write(root.join("Tiny Disc (Track 3).bin"), &track_three).unwrap();
    let chosen = root.join("chosen").join("Translation.ips");
    fs::create_dir_all(chosen.parent().unwrap()).unwrap();
    fs::write(&chosen, ips(&track_three, &[TRACK_THREE_PATCHED, &[0u8; 1024]].concat())).unwrap();

    let choices = GameFiles { added: vec![chosen], ..GameFiles::default() };
    let set = content::collect_with(&layout, Some("dreamcast"), &choices).unwrap();
    // Track 2 is the largest file, but it is audio.
    assert_eq!(patches_on(&set, "Tiny Disc (Track 3).bin"), ["Translation.ips"], "{:?}", staged(&set));
    assert!(set.refused_patches.is_empty());
}

#[test]
fn a_patched_disc_is_named_after_its_patch_and_keeps_the_original_picture() {
    let root = Scratch::dir("rominabox-patched-disc-lookup");
    let mut image = vec![0u8; 64];
    image[..11].copy_from_slice(b"SLUS_012.34");
    fs::write(root.join("Tiny Disc (Europe).bin"), &image).unwrap();
    let cue = root.join("Tiny Disc (Europe).cue");
    fs::write(&cue, "FILE \"Tiny Disc (Europe).bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
    let mut patched = image.clone();
    patched[32..40].copy_from_slice(b"PATCHED!");
    fs::write(root.join("Tiny Disc (Europe) (Director's Cut).bps"), bps(&image, &patched)).unwrap();
    let cache = root.join("cache");
    let picture = stage_disc_cover(&cache, "Tiny Disc (Europe)", "SLUS-01234");

    let inspection = inspect_game(&cue, &cache, false).unwrap();
    assert_eq!(inspection.system, "ps1", "{:?}", inspection.warnings);
    assert_eq!(inspection.title, "Tiny Disc (Director's Cut)");
    assert_eq!(inspection.icon_path.as_deref(), Some(picture.as_path()));
    assert!(!cache.join("patched").exists(), "the lookup made a patched copy of the disc");
}

/// A PlayStation catalogue with `serial` as `name`, and a picture.
fn stage_disc_cover(cache: &Path, name: &str, serial: &str) -> PathBuf {
    let catalog = "Sony - PlayStation";
    let dat = cache.join("catalogs").join(format!("{catalog}.dat"));
    fs::create_dir_all(dat.parent().unwrap()).unwrap();
    fs::write(
        &dat,
        format!(
            "clrmamepro (\n  name \"fixture\"\n)\ngame (\n  name \"{name}\"\n  rom ( name \"track.bin\" size 1 crc 00000000 serial \"{serial}\" )\n)\n"
        ),
    )
    .unwrap();
    let index = cache.join("artwork-index").join(format!("{catalog}.txt"));
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::write(&index, format!("{name}\n")).unwrap();
    let png = cache.join("artwork").join(catalog).join("Named_Boxarts").join(format!("{name}.png"));
    fs::create_dir_all(png.parent().unwrap()).unwrap();
    fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
    png
}
