//! Tests of the Windows installer steps in src/installation.rs. We run them
//! on per-user folders, a registry key and a sandbox that the test creates,
//! never on the person's own folders or Path.
#![cfg(windows)]

mod sandboxes;

use rominabox_engine::installation::{path_with, path_without, Installation};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[link(name = "advapi32")]
extern "system" {
    fn RegSetKeyValueW(key: isize, subkey: *const u16, value: *const u16, kind: u32, data: *const u16, bytes: u32)
        -> i32;
    fn RegGetValueW(
        key: isize,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut u16,
        bytes: *mut u32,
    ) -> i32;
    fn RegDeleteTreeW(key: isize, subkey: *const u16) -> i32;
    fn RegDeleteKeyW(key: isize, subkey: *const u16) -> i32;
}

const REG_EXPAND_SZ: u32 = 2;
const RRF_RT_REG_EXPAND_SZ: u32 = 0x4;
const RRF_NOEXPAND: u32 = 0x1000_0000;
const TESTS_KEY: &str = r"Software\ROM-in-a-Box tests";

/// A key under HKEY_CURRENT_USER, made for this test, with a Path value. We
/// remove it when the test ends, and the key above it when that is empty.
struct ScratchKey(String);

impl ScratchKey {
    fn new(name: &str) -> ScratchKey {
        ScratchKey(format!(r"{TESTS_KEY}\{name}-{}-{}", std::process::id(), unique()))
    }

    fn set(&self, path: &str) {
        let data = sandboxes::wide(path);
        let written = unsafe {
            RegSetKeyValueW(
                sandboxes::HKEY_CURRENT_USER,
                sandboxes::wide(&self.0).as_ptr(),
                sandboxes::wide("Path").as_ptr(),
                REG_EXPAND_SZ,
                data.as_ptr(),
                (data.len() * 2) as u32,
            )
        };
        assert_eq!(written, 0, "could not write the test's Path");
    }

    /// The Path value as it is written, or None when there is none.
    fn get(&self) -> Option<String> {
        let mut bytes = 0u32;
        let flags = RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let (key, value) = (sandboxes::wide(&self.0), sandboxes::wide("Path"));
        let read = |data: *mut u16, bytes: &mut u32| unsafe {
            RegGetValueW(sandboxes::HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), flags, std::ptr::null_mut(), data, bytes)
        };
        if read(std::ptr::null_mut(), &mut bytes) != 0 {
            return None;
        }
        let mut data = vec![0u16; bytes as usize / 2];
        assert_eq!(read(data.as_mut_ptr(), &mut bytes), 0);
        data.truncate(bytes as usize / 2);
        while data.last() == Some(&0) {
            data.pop();
        }
        Some(String::from_utf16(&data).unwrap())
    }
}

impl Drop for ScratchKey {
    fn drop(&mut self) {
        unsafe {
            RegDeleteTreeW(sandboxes::HKEY_CURRENT_USER, sandboxes::wide(&self.0).as_ptr());
            RegDeleteKeyW(sandboxes::HKEY_CURRENT_USER, sandboxes::wide(TESTS_KEY).as_ptr());
        }
    }
}

fn unique() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}

fn installation(root: &Path, key: &ScratchKey) -> Installation {
    Installation {
        local: root.join("Local"),
        roaming: root.join("Roaming"),
        identifier: "com.rominabox.desktop.installation-test".into(),
        command_line: PathBuf::from(r"C:\Program Files Test\ROM-in-a-Box\bin"),
        environment: key.0.clone(),
    }
}

fn file(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"kept").unwrap();
}

/// In the NSIS build we use for the installer, a string has at most 1,023
/// characters, so we would read a longer Path cut short and write it back so.
#[test]
fn a_path_longer_than_the_installer_could_read_keeps_every_entry_through_install_and_uninstall() {
    let root = rominabox_scratch::Scratch::dir("rominabox-installation");
    let key = ScratchKey::new("long-path");
    let path: String = (0..80)
        .map(|index| format!(r"C:\Tools\A Folder With A Long Name Number {index:02}"))
        .collect::<Vec<_>>()
        .join(";");
    assert!(path.len() > 3000);
    key.set(&path);
    let installation = installation(&root, &key);
    let folder = installation.command_line.to_string_lossy().into_owned();

    assert!(installation.installed().unwrap());
    assert_eq!(key.get().unwrap(), format!("{path};{folder}"));
    assert!(!installation.installed().unwrap(), "installed again, the folder is not added twice");
    assert_eq!(key.get().unwrap(), format!("{path};{folder}"));

    assert!(installation.removed_from_path().unwrap());
    assert_eq!(key.get().unwrap(), path);
}

#[test]
fn a_person_with_no_path_of_their_own_gets_the_folder_alone() {
    let root = rominabox_scratch::Scratch::dir("rominabox-installation");
    let key = ScratchKey::new("no-path");
    let installation = installation(&root, &key);
    assert_eq!(key.get(), None);
    assert!(installation.installed().unwrap());
    assert_eq!(key.get().unwrap(), installation.command_line.to_string_lossy());
}

#[test]
fn the_path_entry_is_matched_as_windows_matches_paths_and_nothing_else_moves() {
    let folder = r"C:\ROM-in-a-Box\bin";
    assert_eq!(path_with("", folder).as_deref(), Some(folder));
    assert_eq!(path_with(r"C:\a;;C:\b;", folder), Some(format!(r"C:\a;;C:\b;;{folder}")));
    assert_eq!(path_with(r"C:\a;c:\rom-in-a-box\BIN", folder), None);
    assert_eq!(path_without(r"C:\a;C:\ROM-in-a-Box\BIN;;C:\b;", folder).as_deref(), Some(r"C:\a;;C:\b;"));
    assert_eq!(path_without(&format!(r"{folder};C:\a;{folder}"), folder).as_deref(), Some(r"C:\a"));
    assert_eq!(path_without(r"C:\a;C:\ROM-in-a-Box\bin\more", folder), None);
}

/// When someone uninstalls the builder, we take the command line's folder off
/// the Path and leave every game's data, as for an uninstall without "Delete
/// the application data" ticked. With it ticked, we also remove every game's
/// sandbox with its registration and folder, the games' data and unpacked
/// copies, the QUICK SIGN IN accounts and the builder's folders, and nothing
/// else.
#[test]
fn uninstalling_removes_every_games_sandbox_data_and_copies_the_accounts_and_the_builders_folders() {
    let root = rominabox_scratch::Scratch::dir("rominabox-installation");
    let key = ScratchKey::new("uninstall");
    let installation = installation(&root, &key);
    let identity = format!("{:024x}", unique() & ((1u128 << 96) - 1));
    sandboxes::register(&identity);
    struct Registered<'a>(&'a str);
    impl Drop for Registered<'_> {
        fn drop(&mut self) {
            sandboxes::unregister(self.0);
        }
    }
    let _registered = Registered(&identity);
    // The folder of a sandbox has a lower-case name on Windows.
    let sandbox = installation.local.join("Packages").join(sandboxes::sandbox_name(&identity).to_lowercase());
    let (local, roaming) = (&installation.local, &installation.roaming);
    let removed = [
        sandbox.join(r"AC\ROM-in-a-Box\Games").join(&identity).join("retroarch.cfg"),
        local.join(r"ROM-in-a-Box\Games").join(&identity).join(r"saves\game.srm"),
        local.join(r"ROM-in-a-Box\Runtimes").join(format!("{identity}-0123abcd")).join("Game.exe"),
        local.join(r"ROM-in-a-Box Accounts\accounts.json"),
        local.join(&installation.identifier).join(r"core-cache\core.dll"),
        roaming.join(&installation.identifier).join("settings.json"),
    ];
    let kept = [
        local.join(r"Packages\Another.Program_1234\settings.dat"),
        local.join(r"ROM-in-a-Box\rominabox-desktop.exe"),
        local.join(r"ROM-in-a-Box Accounts-wt-a-worktree\accounts.json"),
        local.join(r"Another Program\settings.dat"),
        roaming.join(r"Another Program\settings.dat"),
    ];
    for path in removed.iter().chain(&kept) {
        file(path);
    }
    key.set(&format!(r"C:\a;{};C:\b", installation.command_line.display()));
    assert!(sandboxes::registered(&identity));

    assert!(installation.removed_from_path().unwrap());
    assert_eq!(key.get().unwrap(), r"C:\a;C:\b");
    for path in removed.iter().chain(&kept) {
        assert!(path.is_file(), "{} is gone without the box ticked", path.display());
    }
    assert!(sandboxes::registered(&identity), "the game's sandbox is gone without the box ticked");

    assert_eq!(installation.data_removed(), Vec::<String>::new());
    assert!(!sandboxes::registered(&identity), "the game's sandbox is still registered");
    for path in [
        sandbox,
        local.join(r"ROM-in-a-Box\Games"),
        local.join(r"ROM-in-a-Box\Runtimes"),
        local.join("ROM-in-a-Box Accounts"),
        local.join(&installation.identifier),
        roaming.join(&installation.identifier),
    ] {
        assert!(!path.exists(), "{} is still there", path.display());
    }
    for path in &kept {
        assert!(path.is_file(), "{} is gone", path.display());
    }

    // We also remove the folder with the games' data, once nothing else is
    // in it.
    fs::remove_file(local.join(r"ROM-in-a-Box\rominabox-desktop.exe")).unwrap();
    assert_eq!(installation.data_removed(), Vec::<String>::new());
    assert!(!local.join("ROM-in-a-Box").exists());
}

/// From the installer we expect the person's two absolute folders and the
/// builder's identifier, and we refuse anything else before we remove
/// anything. Otherwise an empty identifier would name the per-user folder.
#[test]
fn an_uninstall_given_a_folder_that_is_not_absolute_or_an_identifier_that_is_not_a_name_removes_nothing() {
    let root = rominabox_scratch::Scratch::dir("rominabox-installation");
    let key = ScratchKey::new("refused");
    // We remove neither folder on uninstall: one is in the per-user folder,
    // the other beside it. An identifier for a folder elsewhere is for a
    // folder that this test made.
    let canaries = [root.join(r"Local\Another Program\settings.dat"), root.join(r"Elsewhere\kept")];
    let elsewhere = root.join("Elsewhere").to_string_lossy().into_owned();
    for identifier in ["", ".", "..", r"a\b", &elsewhere] {
        canaries.iter().for_each(|canary| file(canary));
        let mut refused = installation(&root, &key);
        refused.identifier = identifier.into();
        let failures = refused.data_removed();
        for canary in &canaries {
            assert!(canary.is_file(), "{identifier:?} removed {}", canary.display());
        }
        assert!(failures.len() == 1, "{identifier:?}: {failures:?}");
    }
    let mut relative = installation(&root, &key);
    relative.local = PathBuf::from("Local");
    let failures = relative.data_removed();
    assert_eq!(failures.len(), 1, "{failures:?}");
}
