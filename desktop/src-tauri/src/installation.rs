//! The tasks of the builder's Windows installer. We run each one in the
//! builder program and exit before we open a window (`main.rs`). Installing
//! adds the command line's folder to the user's Path. Uninstalling, except
//! for an update, removes everything that the builder and its games store on
//! the computer, and takes that folder off the Path again. We call both from
//! `windows/installer-hooks.nsh`.
//!
//! We edit the Path through the registry, whatever its length. NSIS, which
//! we build the installer with, has a string limit of 1,023 characters, so
//! a longer Path would come back truncated.

use crate::launch_contract::user_folder;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
    Security::Isolation::DeleteAppContainerProfile,
    System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE,
    },
    UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE},
};

/// The key below HKEY_CURRENT_USER that contains the person's environment.
pub const ENVIRONMENT: &str = "Environment";
const PATH: &str = "Path";

/// The folders of the builder and its games that we remove on uninstall.
pub struct Installation {
    /// The per-user local application data, `%LOCALAPPDATA%`.
    pub local: PathBuf,
    /// The per-user roaming application data, `%APPDATA%`.
    pub roaming: PathBuf,
    /// The builder's identifier, which is the name of its folders in both.
    pub identifier: String,
    /// The command line's folder, which installing puts on the Path.
    pub command_line: PathBuf,
    /// The key below HKEY_CURRENT_USER that contains the Path, `ENVIRONMENT`.
    pub environment: String,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// We name a game's sandbox with this and the game's identity
/// (vendor/retroarch/rominabox_launch.h). Its folder in Packages has the
/// same name, in lower case.
fn sandbox_prefix() -> &'static str {
    include_str!("../../../vendor/retroarch/rominabox_launch.h")
        .lines()
        .find_map(|line| line.strip_prefix("#define RIB_GAME_APP_ID_PREFIX \"")?.strip_suffix('"'))
        .expect("rominabox_launch.h declares RIB_GAME_APP_ID_PREFIX")
}

/// `path` with `folder` as one more entry, or None when it is already there.
/// We compare entries as paths on Windows, ignoring case.
pub fn path_with(path: &str, folder: &str) -> Option<String> {
    if path.split(';').any(|entry| entry.eq_ignore_ascii_case(folder)) {
        return None;
    }
    Some(if path.is_empty() { folder.to_string() } else { format!("{path};{folder}") })
}

/// `path` without any entry that is `folder`, or None when it has none.
pub fn path_without(path: &str, folder: &str) -> Option<String> {
    let kept: Vec<&str> = path.split(';').filter(|entry| !entry.eq_ignore_ascii_case(folder)).collect();
    (kept.len() != path.split(';').count()).then(|| kept.join(";"))
}

/// The Path value in HKEY_CURRENT_USER\`key`, as written, with its
/// variables unexpanded, or empty when there is none.
fn read_path(key: &str) -> io::Result<String> {
    let mut opened: HKEY = std::ptr::null_mut();
    let name = wide(key);
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, name.as_ptr(), 0, KEY_QUERY_VALUE, &mut opened) };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(String::new());
    }
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let value = wide(PATH);
    let mut bytes = 0u32;
    let mut status = unsafe {
        RegQueryValueExW(opened, value.as_ptr(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), &mut bytes)
    };
    let mut data = vec![0u16; (bytes as usize).div_ceil(2)];
    if status == ERROR_SUCCESS {
        status = unsafe {
            RegQueryValueExW(
                opened,
                value.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
    }
    unsafe { RegCloseKey(opened) };
    match status {
        ERROR_FILE_NOT_FOUND => Ok(String::new()),
        ERROR_SUCCESS => {
            data.truncate((bytes as usize) / 2);
            while data.last() == Some(&0) {
                data.pop();
            }
            String::from_utf16(&data).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        }
        status => Err(io::Error::from_raw_os_error(status as i32)),
    }
}

fn write_path(key: &str, path: &str) -> io::Result<()> {
    let mut opened: HKEY = std::ptr::null_mut();
    let name = wide(key);
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            name.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut opened,
            std::ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let data = wide(path);
    let status = unsafe {
        RegSetValueExW(opened, wide(PATH).as_ptr(), 0, REG_EXPAND_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
    };
    unsafe { RegCloseKey(opened) };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

/// Apply `edit` to the Path in HKEY_CURRENT_USER\`key`, and write it only
/// when it changes. Return whether it changed.
fn edit_path(key: &str, edit: impl Fn(&str) -> Option<String>) -> io::Result<bool> {
    let Some(edited) = edit(&read_path(key)?) else {
        return Ok(false);
    };
    write_path(key, &edited)?;
    Ok(true)
}

/// Tell running programs that the person's environment changed, as the
/// Windows documentation requires of a program that changes it, so the next
/// terminal opened has the new Path.
pub fn announce_environment() {
    let area = wide(ENVIRONMENT);
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            std::ptr::null_mut(),
        );
    }
}

/// Remove the folder `path` and everything in it, when it exists.
fn remove_tree(path: &Path, failures: &mut Vec<String>) {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => failures.push(format!("{}: {error}", path.display())),
    }
}

impl Installation {
    /// Add the command line's folder to the Path. Return whether it changed.
    pub fn installed(&self) -> io::Result<bool> {
        let folder = self.command_line.to_string_lossy();
        edit_path(&self.environment, |path| path_with(path, &folder))
    }

    /// Remove every game's sandbox, with its registration and folder, every
    /// game's data and unpacked copies, the accounts shared by QUICK SIGN IN,
    /// and the builder's folders, and take the command line's folder off the
    /// Path. Name everything we could not remove, and continue past it. Remove
    /// nothing when a per-user folder is not absolute, or when the identifier
    /// is not the name of one folder.
    pub fn uninstalled(&self) -> (bool, Vec<String>) {
        let one_folder = matches!(
            Path::new(&self.identifier).components().collect::<Vec<_>>()[..],
            [std::path::Component::Normal(_)]
        );
        if !self.local.is_absolute() || !self.roaming.is_absolute() || !one_folder {
            return (false, vec![format!(
                "anything: {} and {} must be absolute, and {:?} one folder's name",
                self.local.display(),
                self.roaming.display(),
                self.identifier
            )]);
        }
        let mut failures = Vec::new();
        let prefix = sandbox_prefix();
        let packages = self.local.join("Packages");
        for entry in fs::read_dir(&packages).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(identity) = name
                .get(..prefix.len())
                .filter(|start| start.eq_ignore_ascii_case(prefix))
                .map(|_| &name[prefix.len()..])
            else {
                continue;
            };
            let sandbox = wide(&format!("{prefix}{identity}"));
            unsafe { DeleteAppContainerProfile(sandbox.as_ptr()) };
            remove_tree(&entry.path(), &mut failures);
        }
        for folder in [user_folder!(Games), user_folder!(Runtimes)] {
            remove_tree(&self.local.join(folder), &mut failures);
        }
        // The folder they are in, once it is empty. We install the builder
        // there unless the person chose another folder.
        if let Some(root) = Path::new(user_folder!(Games)).parent() {
            let _ = fs::remove_dir(self.local.join(root));
        }
        remove_tree(&self.local.join(crate::achievements::SHARED_ACCOUNTS), &mut failures);
        remove_tree(&self.local.join(&self.identifier), &mut failures);
        remove_tree(&self.roaming.join(&self.identifier), &mut failures);
        let folder = self.command_line.to_string_lossy();
        let changed = match edit_path(&self.environment, |path| path_without(path, &folder)) {
            Ok(changed) => changed,
            Err(error) => {
                failures.push(format!("the Path in {}: {error}", self.environment));
                false
            }
        };
        (changed, failures)
    }
}
