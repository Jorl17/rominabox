//! When we generate the registries again with nothing changed, we leave the
//! files as they were, down to their times. We compile them into the builder,
//! and a new time alone would make us compile the builder again every build.

use rominabox_catalog::{compatibility_registries, Catalog, PACKAGE_ROOT};
use std::process::Command;
use std::time::SystemTime;

#[test]
fn generating_again_leaves_unchanged_registries_untouched() {
    let root = rominabox_catalog::repo_root().canonicalize().expect("repository root");
    let catalog = Catalog::load(&root.join(PACKAGE_ROOT)).unwrap_or_else(|_| panic!("the packages do not validate"));
    let paths: Vec<_> = compatibility_registries(&catalog)
        .expect("the registries")
        .into_iter()
        .map(|(name, _)| root.join("desktop").join(name))
        .collect();
    let times = || -> Vec<SystemTime> {
        paths.iter().map(|path| std::fs::metadata(path).unwrap().modified().unwrap()).collect()
    };
    let before = times();

    let ran = Command::new(env!("CARGO_BIN_EXE_rominabox-catalog"))
        .arg("generate")
        .current_dir(&root)
        .output()
        .unwrap();

    assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
    assert_eq!(times(), before, "{paths:?}");
}
