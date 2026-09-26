//! Small programs in `scripts/native_runtime/` compiled against the RetroArch
//! sources of the fork, so that a test checks RetroArch itself and not a
//! description of it. RetroArch does not start. In a probe we call some of its
//! functions, read some of its tables and print the results.

use rominabox_scratch::Scratch;
use std::{fs, path::PathBuf, process::Command};

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
        let retroarch = crate::repo::at("vendor/retroarch");
        // input_driver.h includes "../config.h", which is written by the
        // RetroArch configure script. The code in a probe does not depend on it,
        // so we put an empty one, one folder above a separate include path.
        let configured = scratch.join("configured");
        fs::create_dir_all(configured.join("include")).unwrap();
        fs::write(configured.join("config.h"), "").unwrap();
        let program = scratch.join(name);
        // A probe uses only a few functions and tables, so we leave the rest of
        // each source out of the program instead of adding its dependencies.
        let unused = if cfg!(target_os = "macos") {
            "-Wl,-dead_strip"
        } else {
            "-Wl,--gc-sections"
        };
        let built = Command::new("cc")
            .args([
                "-std=gnu99",
                "-w",
                "-ffunction-sections",
                "-fdata-sections",
                unused,
            ])
            .arg(format!("-I{}", configured.join("include").display()))
            .arg(format!("-I{}", retroarch.display()))
            .arg(format!(
                "-I{}",
                retroarch.join("libretro-common/include").display()
            ))
            .arg(format!("-I{}", retroarch.join("deps").display()))
            .arg(crate::repo::at(&format!("scripts/native_runtime/{name}.c")))
            .args(sources.iter().map(|source| retroarch.join(source)))
            .arg("-o")
            .arg(&program)
            .output()
            .expect("cc runs");
        assert!(
            built.status.success(),
            "the {name} probe did not build:\n{}",
            String::from_utf8_lossy(&built.stderr)
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
        assert!(printed.status.success(), "the probe failed");
        String::from_utf8(printed.stdout)
            .expect("the probe prints text")
            .lines()
            .map(str::to_string)
            .collect()
    }
}
