//! The checkout whose files a test reads: the one we compiled this crate in.
//! Each checkout has its own cargo target, so that is also the checkout in
//! which its tests run.

use std::path::PathBuf;

/// The repository in which we compiled this crate.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A path inside the repository of this run.
pub fn at(relative: &str) -> PathBuf {
    root().join(relative)
}

/// The Python we run helper scripts with: the one used to start
/// `scripts/test.py` (`ROMINABOX_PYTHON`), or else the platform's usual name.
/// On Windows that is `python`, because there `python3` is the Microsoft
/// Store stub. On macOS, Linux and other POSIX systems it is `python3`.
pub fn python() -> String {
    std::env::var("ROMINABOX_PYTHON").unwrap_or_else(|_| {
        if cfg!(windows) { "python" } else { "python3" }.to_string()
    })
}
