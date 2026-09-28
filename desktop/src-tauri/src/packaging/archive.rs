//! A zip of apps with the Unix modes required on a Mac for their files.
//!
//! The programs in a Mac app must be executable after unpacking. On Windows
//! the files of the builder cannot keep that mode, and a kit unpacked there
//! has lost it, so we decide the mode of a file by its kind and never read it
//! from the file system. A program (a Mach-O file or a script) is executable,
//! any other file is not, and every folder can be entered. When someone
//! unpacks with Archive Utility or `ditto -x -k`, each file gets that mode.

use super::{ErrorStage, ExportError};
use crate::mach_o;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const PROGRAM: u32 = 0o755;
const DATA: u32 = 0o644;
const FOLDER: u32 = 0o755;

/// Write `destination`, a zip that contains each of `roots`, a folder with
/// everything in it or a file, under the given name, such as `("…/Game.app",
/// "Game.app")`. We join names inside the zip with "/" on every host.
pub fn write_zip(roots: &[(&Path, &str)], destination: &Path) -> Result<(), ExportError> {
    let failed = |path: &Path, error: io::Error| ExportError::io(ErrorStage::Complete, path, error);
    let file = File::create(destination).map_err(|error| failed(destination, error))?;
    let mut zip = ZipWriter::new(io::BufWriter::new(file));
    for (root, name) in roots {
        add(&mut zip, root, name).map_err(|error| failed(root, error))?;
    }
    zip.finish()
        .and_then(|mut written| written.flush().map_err(Into::into))
        .map_err(|error| failed(destination, io::Error::other(error)))
}

/// Add `path` to the zip as `name`, and for a folder, everything in it, in
/// name order.
fn add<W: Write + io::Seek>(zip: &mut ZipWriter<W>, path: &Path, name: &str) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(dos_time(metadata.modified().unwrap_or(UNIX_EPOCH)));
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other(format!("{} is a link, which a zip made here does not carry", path.display())));
    }
    if metadata.is_dir() {
        zip.add_directory(name, options.unix_permissions(FOLDER))?;
        let mut entries = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<io::Result<Vec<_>>>()?;
        entries.sort();
        for entry in entries {
            let child = entry
                .to_str()
                .ok_or_else(|| io::Error::other(format!("{} holds a name that is not UTF-8", path.display())))?;
            add(zip, &path.join(child), &format!("{name}/{child}"))?;
        }
        return Ok(());
    }
    let mut file = File::open(path)?;
    let mut head = Vec::with_capacity(8);
    (&mut file).take(8).read_to_end(&mut head)?;
    zip.start_file(name, options.unix_permissions(mode_of(&head)))?;
    zip.write_all(&head)?;
    io::copy(&mut file, zip)?;
    Ok(())
}

/// The mode of a file, from its first bytes. A program is executable.
fn mode_of(head: &[u8]) -> u32 {
    if mach_o::is_mach_o(head) || head.starts_with(b"#!") {
        PROGRAM
    } else {
        DATA
    }
}

/// `time` in the form of a zip date, in whole seconds of UTC. For a time
/// before 1980, which a zip cannot store, we use the first zip date.
fn dos_time(time: SystemTime) -> DateTime {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    // Days since 1970 to a civil date (Howard Hinnant's algorithm).
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let clock = seconds % 86_400;
    u16::try_from(year)
        .ok()
        .and_then(|year| {
            DateTime::from_date_and_time(
                year,
                month as u8,
                day as u8,
                (clock / 3600) as u8,
                (clock % 3600 / 60) as u8,
                (clock % 60) as u8,
            )
            .ok()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_executable_and_anything_else_is_not() {
        assert_eq!(mode_of(&[0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1]), PROGRAM);
        assert_eq!(mode_of(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2]), PROGRAM);
        assert_eq!(mode_of(b"#!/bin/sh\n"), PROGRAM);
        // A Java class file starts like a universal file, and its version is
        // in the place of the slice count of a universal file.
        assert_eq!(mode_of(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 52]), DATA);
        assert_eq!(mode_of(b"<?xml ver"), DATA);
        assert_eq!(mode_of(b""), DATA);
    }

    /// A zip made where files have no modes. Here the program on disk is
    /// not executable and the data is, and after unpacking as on a Mac
    /// (`ditto -x -k`, as with Archive Utility), each has its correct mode.
    #[test]
    #[cfg(target_os = "macos")]
    fn modes_come_from_what_each_file_is_not_from_the_disk() {
        use std::os::unix::fs::PermissionsExt;
        let root = rominabox_scratch::Scratch::dir("rominabox-zip-modes");
        let app = root.path().join("staged");
        let program = app.join("Contents/MacOS/retroarch");
        let data = app.join("Contents/Resources/content/Pokémon Gold.gbc");
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        fs::create_dir_all(data.parent().unwrap()).unwrap();
        fs::copy("/usr/bin/true", &program).unwrap();
        fs::write(&data, b"cartridge").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&data, fs::Permissions::from_mode(0o755)).unwrap();

        let zip = root.path().join("Game.zip");
        write_zip(&[(&app, "Pokémon Gold.app")], &zip).unwrap();
        let unzipped = root.path().join("unzipped");
        let status = std::process::Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(&zip)
            .arg(&unzipped)
            .status()
            .unwrap();
        assert!(status.success());
        let mode = |relative: &str| fs::metadata(unzipped.join(relative)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode("Pokémon Gold.app/Contents/MacOS/retroarch"), PROGRAM);
        assert_eq!(mode("Pokémon Gold.app/Contents/Resources/content/Pokémon Gold.gbc"), DATA);
        assert_eq!(mode("Pokémon Gold.app/Contents/MacOS"), FOLDER);
        let ran = std::process::Command::new(unzipped.join("Pokémon Gold.app/Contents/MacOS/retroarch")).status().unwrap();
        assert!(ran.success(), "the unpacked program does not run");
    }

    #[test]
    fn a_date_is_the_same_date_in_a_zip() {
        let time = UNIX_EPOCH + std::time::Duration::from_secs(1_790_586_896); // 2026-09-28 09:14:56 UTC
        let written = dos_time(time);
        assert_eq!(
            (written.year(), written.month(), written.day(), written.hour(), written.minute(), written.second()),
            (2026, 9, 28, 9, 14, 56)
        );
        assert_eq!(dos_time(UNIX_EPOCH), DateTime::default());
    }
}
