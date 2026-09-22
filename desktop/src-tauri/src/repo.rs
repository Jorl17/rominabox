//! The checkout from which a test reads its files.
//!
//! Cargo identifies a build by the source fingerprint. Two checkouts at the
//! same commit have the same sources but different paths, and
//! `env!("CARGO_MANIFEST_DIR")` puts the path into the binary. With a shared
//! target folder, one checkout can run a test binary built in another, and
//! the tests in it then read `integrations/` from the other checkout.
//!
//! So we take the path from where the tests run, not from where they were
//! compiled, and we set `ROMINABOX_REPO` in `scripts/test.py`. The compiled-in
//! path is the fallback for a `cargo test` run by hand, where the two are equal.

use std::path::PathBuf;

/// The repository this run belongs to.
pub fn root() -> PathBuf {
    match std::env::var("ROMINABOX_REPO") {
        Ok(declared) if !declared.is_empty() => PathBuf::from(declared),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
    }
}

/// A path inside the repository of this run.
pub fn at(relative: &str) -> PathBuf {
    root().join(relative)
}
