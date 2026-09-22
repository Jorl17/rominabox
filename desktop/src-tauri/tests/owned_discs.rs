//! Tests against actual disc images in the local test ROM folder.
//!
//! One is a CHD with a sibling SBI, and another is a Dreamcast disc folder.
//! We read those files and write nothing next to them. We use the catalogue
//! copies already in work/identification-cache, so the tests do not use the
//! network.

use rominabox_desktop::{content, metadata, systems};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn owned(name: &str) -> PathBuf {
    PathBuf::from("/Users/mariowilde/Downloads/roms").join(name)
}

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-owned-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

/// The picture list and the one cover for this game. We work offline, so a
/// match that exists only because of a download does not count.
fn stage(cache: &Path, system_id: &str, picture: &str) {
    let system = systems::find(system_id).unwrap_or_else(|| panic!("no {system_id}"));
    let catalog = system
        .catalog
        .as_deref()
        .unwrap_or_else(|| panic!("{system_id} has no catalogue"));
    // The checkout of this run, not the one where the binary was built,
    // because several checkouts can share one cargo target. In the `reporoot`
    // tests we reject the other form.
    let dat = rominabox_desktop::repo::at("work/identification-cache/dats")
        .join(format!("{system_id}.dat"));
    let dest = cache.join("catalogs").join(format!("{catalog}.dat"));
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::copy(&dat, &dest).unwrap_or_else(|error| panic!("copy {}: {error}", dat.display()));
    let index = cache.join("artwork-index").join(format!("{catalog}.txt"));
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::write(&index, format!("{picture}\n")).unwrap();
    let png = cache
        .join("artwork")
        .join(catalog)
        .join("Named_Boxarts")
        .join(format!("{picture}.png"));
    fs::create_dir_all(png.parent().unwrap()).unwrap();
    fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
}

#[test]
fn his_ape_escape_chd_is_named_and_given_a_cover() {
    let chd = owned("Ape Escape (Europe).chd");
    assert!(chd.is_file(), "missing {}", chd.display());
    let cache = scratch();
    stage(&cache, "ps1", "Ape Escape (Europe)");

    let inspection = metadata::inspect_game(&chd, &cache, false).expect("inspect");
    assert_eq!(inspection.system, "ps1", "{:?}", inspection.warnings);
    assert!(
        inspection.matched,
        "the CHD was not identified: {:?}",
        inspection.warnings
    );
    assert_eq!(inspection.title, "Ape Escape");
    assert_eq!(
        inspection.catalog_name.as_deref(),
        Some("Ape Escape (Europe)")
    );
    assert!(
        inspection.icon_path.is_some(),
        "no cover: {:?}",
        inspection.warnings
    );
    assert!(
        inspection
            .support_files
            .iter()
            .any(|name| name == "Ape Escape (Europe).sbi"),
        "the CHD inspection did not name the sibling SBI: {:?}",
        inspection.support_files
    );
}

#[test]
fn his_ape_escape_subchannel_file_is_collected_with_the_disc() {
    let chd = owned("Ape Escape (Europe).chd");
    let sbi = owned("Ape Escape (Europe).sbi");
    assert!(chd.is_file() && sbi.is_file());

    let collected = content::collect(&chd).expect("a CHD can be collected");
    let names: Vec<String> = collected
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|name| name == "Ape Escape (Europe).sbi"),
        "dropping the CHD must bring the sibling SBI with it: {names:?}"
    );

    // When someone drops the SBI, we must identify the disc next to it. The
    // SBI is not a game, and identifying it as an unknown file is a failure.
    let cache = scratch();
    stage(&cache, "ps1", "Ape Escape (Europe)");
    let inspection = metadata::inspect_game(&sbi, &cache, false).expect("inspect sbi");
    assert!(
        inspection.matched,
        "the SBI beside the CHD was not identified as Ape Escape: {:?}",
        inspection.warnings
    );
    assert_eq!(
        inspection.catalog_name.as_deref(),
        Some("Ape Escape (Europe)")
    );
}

#[test]
fn his_sonic_adventure_2_folder_is_named_and_given_a_cover() {
    let folder = owned("Sonic Adventure 2 (Europe)");
    assert!(folder.is_dir(), "missing {}", folder.display());
    let cache = scratch();
    stage(
        &cache,
        "dreamcast",
        "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)",
    );

    let inspection = metadata::inspect_game(&folder, &cache, false).expect("inspect folder");
    assert_eq!(inspection.system, "dreamcast", "{:?}", inspection.warnings);
    assert!(
        inspection.matched,
        "Sonic Adventure 2 was not identified: {:?}",
        inspection.warnings
    );
    assert_eq!(inspection.title, "Sonic Adventure 2");
    assert_eq!(
        inspection.catalog_name.as_deref(),
        Some("Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)")
    );
    assert!(
        inspection.icon_path.is_some(),
        "no cover: {:?}",
        inspection.warnings
    );
}
