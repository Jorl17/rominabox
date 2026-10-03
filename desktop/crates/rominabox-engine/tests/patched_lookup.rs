//! The lookup of a game with patches: we look up the patched game first,
//! use the picture of the original when the patched game has none, and the
//! name of the patch when the patched game is not in the catalogue.

use std::fs;
use std::path::{Path, PathBuf};

use rominabox_engine::metadata::inspect_game;
use rominabox_scratch::Scratch;

mod patch_writers;
use patch_writers::bps;

const CATALOG: &str = "Sega - Mega Drive - Genesis";
const ORIGINAL: &[u8] = b"Tiny Blast, as it shipped";
const PATCHED: &[u8] = b"Tiny Blast, as its director meant it";

/// A catalogue with `games`, by name and contents, and pictures for the names
/// in `pictured`.
fn stage_catalog(cache: &Path, games: &[(&str, &[u8])], pictured: &[&str]) -> Vec<PathBuf> {
    let mut dat = String::from("clrmamepro (\n  name \"fixture\"\n)\n");
    for (name, bytes) in games {
        dat.push_str(&format!(
            "game (\n  name \"{name}\"\n  rom ( name \"{name}.md\" size {} crc {:08X} )\n)\n",
            bytes.len(),
            crc32fast::hash(bytes)
        ));
    }
    let catalog = cache.join("catalogs").join(format!("{CATALOG}.dat"));
    fs::create_dir_all(catalog.parent().unwrap()).unwrap();
    fs::write(catalog, dat).unwrap();
    let index = cache.join("artwork-index").join(format!("{CATALOG}.txt"));
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::write(index, pictured.iter().map(|name| format!("{name}\n")).collect::<String>()).unwrap();
    pictured
        .iter()
        .map(|name| {
            let picture = cache.join("artwork").join(CATALOG).join("Named_Boxarts").join(format!("{name}.png"));
            fs::create_dir_all(picture.parent().unwrap()).unwrap();
            fs::write(&picture, b"\x89PNG\r\n\x1a\n").unwrap();
            picture
        })
        .collect()
}

/// The original game, with `patch` beside it.
fn game_with_patch(root: &Path, patch: &str) -> PathBuf {
    let rom = root.join("Tiny Blast (USA).md");
    fs::write(&rom, ORIGINAL).unwrap();
    fs::write(root.join(patch), bps(ORIGINAL, PATCHED)).unwrap();
    rom
}

#[test]
fn a_patched_game_the_catalogue_knows_is_named_and_pictured_as_itself() {
    let root = Scratch::dir("rominabox-patched-lookup-known");
    let rom = game_with_patch(&root, "Director's Cut.bps");
    let cache = root.join("cache");
    let pictures = stage_catalog(
        &cache,
        &[("Tiny Blast (USA)", ORIGINAL), ("Tiny Blast - Director's Cut (World)", PATCHED)],
        &["Tiny Blast (USA)", "Tiny Blast - Director's Cut (World)"],
    );

    let inspection = inspect_game(&rom, &cache, false).unwrap();
    assert_eq!(inspection.title, "Tiny Blast - Director's Cut");
    assert!(inspection.matched);
    assert_eq!(inspection.icon_path.as_deref(), Some(pictures[1].as_path()));
    assert_eq!(inspection.filename, "Tiny Blast (USA).md", "the file dropped stays the one named");
}

#[test]
fn a_patched_game_without_a_picture_of_its_own_has_the_originals() {
    let root = Scratch::dir("rominabox-patched-lookup-unpictured");
    let rom = game_with_patch(&root, "Director's Cut.bps");
    let cache = root.join("cache");
    let pictures = stage_catalog(
        &cache,
        &[("Tiny Blast (USA)", ORIGINAL), ("Tiny Blast - Director's Cut (World)", PATCHED)],
        &["Tiny Blast (USA)"],
    );

    let inspection = inspect_game(&rom, &cache, false).unwrap();
    assert_eq!(inspection.title, "Tiny Blast - Director's Cut");
    assert_eq!(inspection.icon_path.as_deref(), Some(pictures[0].as_path()));
    assert!(inspection.warnings.is_empty(), "the cover is the original's: {:?}", inspection.warnings);
}

#[test]
fn a_patched_game_the_catalogue_does_not_know_takes_the_patchs_name_and_the_originals_picture() {
    let root = Scratch::dir("rominabox-patched-lookup-unknown");
    let rom = game_with_patch(&root, "Tiny Blast Translation.bps");
    let cache = root.join("cache");
    let pictures = stage_catalog(&cache, &[("Tiny Blast (USA)", ORIGINAL)], &["Tiny Blast (USA)"]);

    let inspection = inspect_game(&rom, &cache, false).unwrap();
    assert_eq!(inspection.title, "Tiny Blast Translation");
    assert!(!inspection.matched);
    assert_eq!(inspection.icon_path.as_deref(), Some(pictures[0].as_path()));
}

/// We make the patched game once, into the lookup cache, and read it from
/// there in the next lookup.
#[test]
fn the_patched_game_is_made_once_into_the_cache() {
    let root = Scratch::dir("rominabox-patched-lookup-cached");
    let rom = game_with_patch(&root, "Director's Cut.bps");
    let cache = root.join("cache");
    stage_catalog(&cache, &[("Tiny Blast - Director's Cut (World)", PATCHED)], &[]);

    let made = |cache: &Path| -> Vec<PathBuf> {
        let Ok(folders) = fs::read_dir(cache.join("patched")) else { return Vec::new() };
        folders.flat_map(|folder| fs::read_dir(folder.unwrap().path()).unwrap()).map(|file| file.unwrap().path()).collect()
    };
    assert_eq!(inspect_game(&rom, &cache, false).unwrap().title, "Tiny Blast - Director's Cut");
    let files = made(&cache);
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(fs::read(&files[0]).unwrap(), PATCHED);
    let first = fs::metadata(&files[0]).unwrap().modified().unwrap();

    assert_eq!(inspect_game(&rom, &cache, false).unwrap().title, "Tiny Blast - Director's Cut");
    assert_eq!(made(&cache), files);
    assert_eq!(fs::metadata(&files[0]).unwrap().modified().unwrap(), first, "read, not made again");
}
