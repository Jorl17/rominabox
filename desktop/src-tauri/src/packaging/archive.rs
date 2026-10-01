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
use std::time::UNIX_EPOCH;
use zip::write::FullFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const PROGRAM: u32 = 0o755;
const DATA: u32 = 0o644;
const FOLDER: u32 = 0o755;
/// The id of the extended timestamp field (`UT`), with times in seconds since
/// 1970, which name an instant and so take priority over the date field.
const EXTENDED_TIMESTAMP: u16 = 0x5455;
/// Its flag for "the modification time follows".
const MODIFIED: u8 = 1;

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
    let changed = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs());
    let mut options = FullFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(dos_time(changed));
    let mut stamp = vec![MODIFIED];
    stamp.extend_from_slice(&u32::try_from(changed).unwrap_or(u32::MAX).to_le_bytes());
    options
        .add_extra_data(EXTENDED_TIMESTAMP, stamp.into_boxed_slice(), false)
        .map_err(io::Error::other)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other(format!(
            "{} is a link, which a zip made here does not carry",
            path.display()
        )));
    }
    if metadata.is_dir() {
        zip.add_directory(name, options.clone().unix_permissions(FOLDER))?;
        let mut entries = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<io::Result<Vec<_>>>()?;
        entries.sort();
        for entry in entries {
            let child = entry.to_str().ok_or_else(|| {
                io::Error::other(format!("{} holds a name that is not UTF-8", path.display()))
            })?;
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

/// `seconds` after 1970 in the date field of a zip, as local time on this
/// computer, because that field is local time in every zip tool. For a time
/// before 1980, which the field cannot store, we use its first date.
fn dos_time(seconds: u64) -> DateTime {
    local_clock(seconds)
        .and_then(|(year, month, day, hour, minute, second)| {
            DateTime::from_date_and_time(year, month, day, hour, minute, second).ok()
        })
        .unwrap_or_default()
}

/// The time on this computer's clock `seconds` after 1970, in the time zone
/// of that moment: year, month, day, hour, minute and second.
#[cfg(windows)]
fn local_clock(seconds: u64) -> Option<(u16, u8, u8, u8, u8, u8)> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    /// Seconds from 1601, where a FILETIME starts, to 1970.
    const FROM_1601: u64 = 11_644_473_600;
    /// A FILETIME counts tenths of a microsecond.
    const TICKS_PER_SECOND: u64 = 10_000_000;
    let ticks = seconds
        .checked_add(FROM_1601)?
        .checked_mul(TICKS_PER_SECOND)?;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: SYSTEMTIME is plain integers, for which zero is valid.
    let (mut utc, mut local): (SYSTEMTIME, SYSTEMTIME) = unsafe { std::mem::zeroed() };
    // SAFETY: every pointer is to a local in scope, of the type that the call
    // takes, and a null zone means the computer's time zone.
    let converted = unsafe {
        FileTimeToSystemTime(&file, &mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
    };
    converted.then(|| {
        (
            local.wYear,
            local.wMonth as u8,
            local.wDay as u8,
            local.wHour as u8,
            local.wMinute as u8,
            local.wSecond as u8,
        )
    })
}

/// The time on this computer's clock `seconds` after 1970, in the time zone
/// of that moment: year, month, day, hour, minute and second.
#[cfg(unix)]
fn local_clock(seconds: u64) -> Option<(u16, u8, u8, u8, u8, u8)> {
    let time = libc::time_t::try_from(seconds).ok()?;
    // SAFETY: tm is plain integers and pointers, for which zero is valid.
    let mut clock: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both point to locals in scope, and localtime_r writes only `clock`.
    if unsafe { libc::localtime_r(&time, &mut clock) }.is_null() {
        return None;
    }
    Some((
        u16::try_from(clock.tm_year + 1900).ok()?,
        u8::try_from(clock.tm_mon + 1).ok()?,
        u8::try_from(clock.tm_mday).ok()?,
        u8::try_from(clock.tm_hour).ok()?,
        u8::try_from(clock.tm_min).ok()?,
        u8::try_from(clock.tm_sec).ok()?,
    ))
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
        let mode = |relative: &str| {
            fs::metadata(unzipped.join(relative))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("Pokémon Gold.app/Contents/MacOS/retroarch"), PROGRAM);
        assert_eq!(
            mode("Pokémon Gold.app/Contents/Resources/content/Pokémon Gold.gbc"),
            DATA
        );
        assert_eq!(mode("Pokémon Gold.app/Contents/MacOS"), FOLDER);
        let ran =
            std::process::Command::new(unzipped.join("Pokémon Gold.app/Contents/MacOS/retroarch"))
                .status()
                .unwrap();
        assert!(ran.success(), "the unpacked program does not run");
    }

    /// The time on this computer's clock `seconds` after 1970. We ask the
    /// tools of the system for it instead of working it out here.
    fn system_clock_shows(seconds: u64) -> (u16, u8, u8, u8, u8, u8) {
        use std::process::Command;
        #[cfg(unix)]
        const FIELDS: &str = "%Y %m %d %H %M %S";
        #[cfg(windows)]
        let asked = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "[DateTimeOffset]::FromUnixTimeSeconds({seconds}).ToLocalTime().ToString('yyyy MM dd HH mm ss')"
            ))
            .output();
        #[cfg(target_os = "linux")]
        let asked = Command::new("date")
            .arg("-d")
            .arg(format!("@{seconds}"))
            .arg(format!("+{FIELDS}"))
            .output();
        // On macOS and the BSDs, -r gives the instant.
        #[cfg(all(unix, not(target_os = "linux")))]
        let asked = Command::new("date")
            .arg("-r")
            .arg(seconds.to_string())
            .arg(format!("+{FIELDS}"))
            .output();
        let shown = String::from_utf8(asked.unwrap().stdout).unwrap();
        let parts: Vec<u16> = shown
            .split_whitespace()
            .map(|part| part.parse().unwrap())
            .collect();
        assert_eq!(parts.len(), 6, "{shown:?}");
        let part = |index: usize| parts[index] as u8;
        (parts[0], part(1), part(2), part(3), part(4), part(5))
    }

    /// We record the time when each file changed as an instant in the
    /// extended timestamp field, for unzip, ditto, 7-Zip and libarchive, and
    /// as local time on this computer in the date field of the zip, because
    /// that field is local time for every zip tool.
    #[test]
    fn a_file_keeps_when_it_changed() {
        let seconds = 1_790_586_896; // 2026-09-28 09:14:56 UTC
        let root = rominabox_scratch::Scratch::dir("rominabox-zip-times");
        let file = root.path().join("game.bin");
        fs::write(&file, b"cartridge").unwrap();
        File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(seconds))
            .unwrap();
        let zip = root.path().join("Game.zip");
        write_zip(&[(&file, "game.bin")], &zip).unwrap();
        let mut archive = zip::ZipArchive::new(File::open(&zip).unwrap()).unwrap();
        let entry = archive.by_name("game.bin").unwrap();
        let instant = entry.extra_data_fields().find_map(|field| match field {
            zip::extra_fields::ExtraField::ExtendedTimestamp(stamp) => stamp.mod_time(),
            _ => None,
        });
        assert_eq!(instant, Some(seconds as u32), "the instant it changed");
        let shown = entry.last_modified().unwrap();
        assert_eq!(
            (
                shown.year(),
                shown.month(),
                shown.day(),
                shown.hour(),
                shown.minute(),
                shown.second()
            ),
            system_clock_shows(seconds),
            "the date field is this computer's clock"
        );
    }

    #[test]
    fn a_time_before_1980_is_the_first_date_a_zip_holds() {
        assert_eq!(dos_time(0), DateTime::default());
    }
}
