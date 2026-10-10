//! Small programs in `scripts/native_runtime/` that we compile against the
//! fork's own RetroArch sources, so that a test checks RetroArch itself
//! instead of a description of it. We do not start RetroArch. In a probe we
//! read its tables, call its functions and print the results. We build the
//! probes with `scripts/retroarch_probe.py`, as for the `typing` scope.

use rominabox_scratch::Scratch;
use std::{path::PathBuf, process::Command};

/// The fork's input layer with its configuration reader, as in the player's
/// build (`HAVE_CONFIGFILE`): the remap loader, the bind parsers and the
/// input poll. For use with `build_defining(name, CONFIGURED, INPUT_LAYER)`.
pub const INPUT_LAYER: &[&str] = &[
    "configuration.c",
    "input/input_driver.c",
    "input/input_keymaps.c",
    "libretro-common/file/config_file.c",
    "libretro-common/file/config_file_io.c",
    "libretro-common/string/rstrtod.c",
    "libretro-common/file/file_path_io.c",
    "libretro-common/streams/file_stream.c",
    "libretro-common/vfs/vfs_implementation.c",
    "libretro-common/compat/compat_strl.c",
    "libretro-common/string/stdstring.c",
    "libretro-common/encodings/encoding_utf.c",
    "libretro-common/file/file_path.c",
    // RetroArch's file opening on Windows. Other systems need nothing.
    "libretro-common/compat/fopen_utf8.c",
];
pub const CONFIGURED: &[&str] = &["HAVE_CONFIGFILE"];

/// The pad inputs of the menu in the fork, with what they need beyond the
/// input layer. For use after `INPUT_LAYER`.
pub const PAD_INPUTS: &[&str] = &[
    "menu/drivers/rmlui/pad_inputs.c",
    "libretro-common/file/config_file_userdata.c",
    "libretro-common/lists/string_list.c",
    "libretro-common/time/rtime.c",
];

pub struct Probe {
    // We remove it, with the program, when we drop the probe.
    _scratch: Scratch,
    program: PathBuf,
}

impl Probe {
    /// Compile `scripts/native_runtime/<name>.c` with the fork's `sources`,
    /// named relative to `vendor/retroarch`.
    pub fn build(name: &str, sources: &[&str]) -> Self {
        Self::build_defining(name, &[], sources)
    }

    /// As `build`, with each of `defined` defined (`HAVE_CONFIGFILE`) in
    /// every source, as in the player's build.
    pub fn build_defining(name: &str, defined: &[&str], sources: &[&str]) -> Self {
        let scratch = Scratch::dir(&format!("rominabox-{name}"));
        let built = crate::repo::python()
            .arg(crate::repo::at("scripts/retroarch_probe.py"))
            .arg(scratch.path())
            .arg(crate::repo::at(&format!("scripts/native_runtime/{name}.c")))
            .args(sources)
            .args(defined.iter().map(|name| format!("-D{name}")))
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
