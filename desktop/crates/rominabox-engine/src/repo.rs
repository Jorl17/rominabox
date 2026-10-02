//! The checkout whose files a test reads: the one we compiled this crate in.
//! Each checkout has its own cargo target, so that is also the checkout in
//! which its tests run.

use std::path::{Path, PathBuf};

/// The repository in which we compiled this crate, three folders above
/// `desktop/crates/rominabox-engine`.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate lives at desktop/crates/rominabox-engine inside the repository")
        .to_path_buf()
}

/// A path inside the repository of this run.
pub fn at(relative: &str) -> PathBuf {
    root().join(relative)
}

/// The builder's resources in this checkout: the runtime kit and the preview
/// renderer that we write with `scripts/build_kit.py` and bundle in the
/// builder's own package (desktop/src-tauri).
pub fn builder_resources() -> PathBuf {
    at("desktop/src-tauri/resources")
}

/// A command for the Python we run helper scripts with, followed by the
/// script and its arguments. We use the interpreter used to start
/// `scripts/test.py` (`ROMINABOX_PYTHON`), or else uv's, with the version in
/// `.python-version` and the packages in `uv.lock`, as in the README.
pub fn python() -> std::process::Command {
    match std::env::var_os("ROMINABOX_PYTHON") {
        Some(interpreter) => std::process::Command::new(interpreter),
        None => {
            let mut uv = std::process::Command::new("uv");
            uv.args(["run", "--locked", "--project"]).arg(root()).arg("python");
            uv
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Component, Path};

    fn has_parent_step(path: &Path) -> bool {
        path.components().any(|part| part == Component::ParentDir)
    }

    #[test]
    fn the_repository_paths_have_no_parent_steps() {
        let root = super::root();
        assert!(!has_parent_step(&root), "{}", root.display());
        let inside = super::at("desktop/src-tauri/resources");
        assert!(!has_parent_step(&inside), "{}", inside.display());
        assert!(root.join("AGENTS.md").is_file(), "{}", root.display());
    }
}
