//! Programs we start from the builder for part of its work, such as the
//! menu preview renderer. On Windows, a console program started from a
//! windowed one gets a separate console window unless we say otherwise.
//! macOS and Linux have no such window.

use std::{ffi::OsStr, process::Command};

/// `program`, ready for its arguments, set to start without a window.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW, from the Win32 process creation flags.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
