//! File operations that Windows refuses for a moment while another program
//! has the file open.

use std::io;
use std::path::Path;

/// Rename `from` to `to`. On Windows a virus scanner keeps a file that it
/// reads open for a moment, and until it closes the file, Windows refuses to
/// move the file or a folder that holds it. So while Windows refuses for that
/// reason, we try again, for at most a few seconds. The launcher does the
/// same in `fs_move_when_free`.
pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::time::{Duration, Instant};
        const ERROR_ACCESS_DENIED: i32 = 5;
        const ERROR_SHARING_VIOLATION: i32 = 32;
        const WAIT: Duration = Duration::from_secs(15);
        const PAUSE: Duration = Duration::from_millis(50);
        let started = Instant::now();
        loop {
            match std::fs::rename(from, to) {
                Err(error)
                    if matches!(error.raw_os_error(), Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION))
                        && started.elapsed() < WAIT =>
                {
                    std::thread::sleep(PAUSE)
                }
                done => return done,
            }
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(from, to)
}

#[cfg(all(test, windows))]
mod tests {
    use std::fs;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    /// A virus scanner holds a file in a folder we have just written open,
    /// without letting anyone remove it, and closes it after a moment. The
    /// rename of the folder succeeds once the file is closed.
    #[test]
    fn a_folder_moves_once_a_scanner_closes_its_file() {
        const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
        let scratch = rominabox_scratch::Scratch::dir("rominabox-files-rename");
        let written = scratch.join("written");
        fs::create_dir_all(&written).unwrap();
        fs::write(written.join("game.exe"), b"program").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE)
            .open(written.join("game.exe"))
            .unwrap();
        let scanner = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            drop(held);
        });
        let moved = super::rename(&written, &scratch.join("in-place"));
        scanner.join().unwrap();
        moved.unwrap();
        assert!(scratch.join("in-place/game.exe").is_file());
    }
}
