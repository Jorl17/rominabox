//! We bundle no cores and no source archives in the builder. At export we
//! download the core we need. The kit contains the player and its files.

use std::path::{Path, PathBuf};

fn files(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[test]
fn the_builders_kit_carries_no_core_and_no_archive() {
    let kit = rominabox_engine::repo::builder_resources().join("runtime");
    assert!(kit.is_dir(), "no prepared runtime kit at {}", kit.display());
    let mut found = Vec::new();
    files(&kit, &mut found);
    let carried: Vec<_> = found
        .iter()
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            name.contains("_libretro.")
                || [".tar.gz", ".tgz", ".tar", ".zip"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
        })
        .collect();
    assert!(
        carried.is_empty(),
        "the builder would bundle these: {carried:#?}"
    );
    for directory in ["cores", "sources"] {
        assert!(
            !kit.join(directory).exists(),
            "the kit still has {directory}/"
        );
    }
}
