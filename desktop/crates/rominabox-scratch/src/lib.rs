//! A path under the system temp directory that we remove on drop, so that
//! no directories remain after a run of the test suite.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Create the directory. We remove it when this value is dropped, also
    /// when the test panics.
    pub fn dir(prefix: &str) -> Self {
        let scratch = Self::reserve(prefix);
        std::fs::create_dir_all(&scratch.path).unwrap_or_else(|error| {
            panic!("could not create {}: {error}", scratch.path.display());
        });
        scratch
    }

    /// A unique path that does not exist yet. We remove it on drop if it exists
    /// by then. Use this rather than `dir` in a test that asserts nothing was
    /// created, because `dir` creates the path.
    pub fn reserve(prefix: &str) -> Self {
        if prefix.is_empty()
            || prefix.contains(['/', '\\'])
            || prefix.contains("..")
        {
            panic!("scratch prefix must be one path component, got {prefix:?}");
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // We stamp this in the test suite and report only names with the stamp,
        // so we do not count tests of another checkout, which use the same $TMPDIR.
        let run = std::env::var("ROMINABOX_SCRATCH_RUN").unwrap_or_else(|_| "direct".to_owned());
        if run.is_empty() || run.contains(['/', '\\']) || run.contains("..") {
            panic!("ROMINABOX_SCRATCH_RUN must be one path component, got {run:?}");
        }
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{run}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // A panic here while a test is already unwinding aborts the process and
        // hides the failure that left the directory. In the test suite we list
        // whatever we could not remove here.
        let removed = if self.path.is_dir() {
            std::fs::remove_dir_all(&self.path)
        } else if self.path.is_file() {
            std::fs::remove_file(&self.path)
        } else {
            Ok(())
        };
        if let Err(error) = removed {
            eprintln!(
                "scratch cleanup failed for {}: {error}",
                self.path.display()
            );
        }
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
