//! Small programs in `scripts/native_runtime/` that we compile against the
//! fork's own RetroArch sources, so that a test checks RetroArch itself
//! instead of a description of it. We do not start RetroArch. In a probe we
//! read its tables, call its functions and print the results. We build the
//! probes with `scripts/retroarch_probe.py`, as for the `typing` scope.

use rominabox_scratch::Scratch;
use std::{path::PathBuf, process::Command};

pub struct Probe {
    // We remove it, with the program, when we drop the probe.
    _scratch: Scratch,
    program: PathBuf,
}

impl Probe {
    /// Compile `scripts/native_runtime/<name>.c` with the fork's `sources`,
    /// named relative to `vendor/retroarch`.
    pub fn build(name: &str, sources: &[&str]) -> Self {
        let scratch = Scratch::dir(&format!("rominabox-{name}"));
        let built = Command::new(crate::repo::python())
            .arg(crate::repo::at("scripts/retroarch_probe.py"))
            .arg(scratch.path())
            .arg(crate::repo::at(&format!("scripts/native_runtime/{name}.c")))
            .args(sources)
            .output()
            .expect("the probe builder runs");
        assert!(
            built.status.success(),
            "the {name} probe did not build:\n{}{}",
            String::from_utf8_lossy(&built.stdout),
            String::from_utf8_lossy(&built.stderr)
        );
        let program = PathBuf::from(
            String::from_utf8(built.stdout)
                .expect("the builder prints a path")
                .trim(),
        );
        Self {
            _scratch: scratch,
            program,
        }
    }

    /// The output of the probe for `arguments`, one entry per line.
    pub fn lines(&self, arguments: &[&str]) -> Vec<String> {
        let printed = Command::new(&self.program)
            .args(arguments)
            .output()
            .expect("the probe runs");
        assert!(
            printed.status.success(),
            "the probe failed:\n{}",
            String::from_utf8_lossy(&printed.stderr)
        );
        String::from_utf8(printed.stdout)
            .expect("the probe prints text")
            .lines()
            .map(str::to_string)
            .collect()
    }
}
