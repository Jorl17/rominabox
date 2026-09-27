//! Where an export goes, and moving it there.
//!
//! We never touch an app already at the destination unless the request
//! includes replacing it. We build a replacement beside the old app and swap
//! it in only once it is complete, so after a failed export the old app is
//! as it was.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::export_error::{ErrorStage, ExportError};
use crate::packaging::{ExportRequest, ExportTarget};

/// The app that an export of `request` produces.
fn destination(request: &ExportRequest) -> PathBuf {
    request.output_dir.join(app_name(&request.target, &request.title))
}

/// The name of the app for a game called `title` on `target`, a bundle on
/// macOS or a folder on Windows, which contains the program of the game with
/// the same name.
pub(crate) fn app_name(target: &ExportTarget, title: &str) -> String {
    match target {
        ExportTarget::Macos => format!("{}.app", safe_filename(title)),
        ExportTarget::Windows => windows_filename(title),
    }
}

/// Before doing anything, refuse an export to the place of an existing app,
/// unless the request includes replacing it.
pub(crate) fn refuse_unless_replacing(request: &ExportRequest) -> Result<(), ExportError> {
    if request.replace {
        Ok(())
    } else {
        refuse_existing(&destination(request))
    }
}

pub(crate) fn refuse_existing(path: &Path) -> Result<(), ExportError> {
    if occupied(path) {
        Err(ExportError::new(
            ErrorStage::Exists,
            format!("{} already exists", path.display()),
        )
        .about(path))
    } else {
        Ok(())
    }
}

/// Move the finished `app` to `destination`. We first set aside an app
/// already there, beside it and never in `staging`, which we remove
/// afterwards. If we can neither put the new app in place nor move the old
/// one back, we leave the old one aside and name its place in the error.
pub(crate) fn put_in_place(app: &Path, destination: &Path, replace: bool) -> Result<(), ExportError> {
    let saving = |error: io::Error| ExportError::io(ErrorStage::Complete, destination, error);
    if !occupied(destination) {
        return fs::rename(app, destination).map_err(saving);
    }
    if !replace {
        return refuse_existing(destination);
    }
    let aside = set_aside_name(destination);
    fs::rename(destination, &aside).map_err(saving)?;
    if let Err(error) = fs::rename(app, destination) {
        return Err(put_back(&aside, destination, error));
    }
    remove_set_aside(&aside)
}

/// Move the old app back where it was. When something else is there by now,
/// we leave the old app aside and name its place in the error.
fn put_back(aside: &Path, destination: &Path, error: io::Error) -> ExportError {
    match fs::rename(aside, destination) {
        Ok(()) => ExportError::io(ErrorStage::Complete, destination, error),
        Err(_) => ExportError::io(ErrorStage::Replace, aside, error),
    }
}

/// A free name beside `destination`: `Game (replaced).app`, then
/// `Game (replaced 2).app`, and so on.
fn set_aside_name(destination: &Path) -> PathBuf {
    let stem = destination
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Game".into());
    let extension = destination
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|number| {
            let suffix = if number == 1 { String::new() } else { format!(" {number}") };
            destination.with_file_name(format!("{stem} (replaced{suffix}){extension}"))
        })
        .find(|candidate| !occupied(candidate))
        .expect("an unused name exists")
}

/// Remove the replaced app by the exact path we just renamed it to. We remove
/// a link itself and never follow it.
fn remove_set_aside(aside: &Path) -> Result<(), ExportError> {
    let metadata = fs::symlink_metadata(aside)
        .map_err(|error| ExportError::io(ErrorStage::Cleanup, aside, error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(aside)
    } else {
        fs::remove_file(aside)
    }
    .map_err(|error| ExportError::io(ErrorStage::Cleanup, aside, error))
}

/// Anything at `path`, including a link that points nowhere.
fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn safe_filename(title: &str) -> String {
    let value: String = title
        .trim()
        .chars()
        .map(|character| match character {
            '/' | ':' | '\0' => '-',
            _ => character,
        })
        .collect();
    if value.is_empty() || value == "." || value == ".." {
        "Game".to_string()
    } else {
        value
    }
}

/// Names reserved for devices on Windows, with or without an extension.
const WINDOWS_DEVICES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `title` as a Windows file name, without characters invalid on Windows,
/// without a trailing dot or space, which would be lost, and never a device name.
fn windows_filename(title: &str) -> String {
    let value: String = title
        .trim()
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            character if character < ' ' => '-',
            character => character,
        })
        .collect();
    let value = value.trim_end_matches(['.', ' ']);
    if value.is_empty() {
        return "Game".to_string();
    }
    let stem = value.split('.').next().unwrap_or(value).trim_end();
    if WINDOWS_DEVICES.iter().any(|device| device.eq_ignore_ascii_case(stem)) {
        format!("{stem}-{}", &value[stem.len()..])
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When the swap itself fails after we set the old app aside, we put the
    /// old app back.
    #[test]
    fn a_new_app_that_cannot_be_moved_in_puts_the_old_one_back() {
        let root = rominabox_scratch::Scratch::dir("rominabox-publish");
        let destination = root.join("Game.app");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old"), b"old").unwrap();

        let error = put_in_place(&root.join("missing.app"), &destination, true).unwrap_err();

        assert_eq!(error.stage, ErrorStage::Complete);
        assert_eq!(fs::read(destination.join("old")).unwrap(), b"old");
        assert!(!root.join("Game (replaced).app").exists());
    }

    /// When we cannot move the old app back, we do not leave it in the staging
    /// folder, where we would delete it. In this test, an app from another
    /// export is already where the old one would go back.
    #[test]
    fn an_old_app_that_cannot_go_back_is_kept_beside_it() {
        let root = rominabox_scratch::Scratch::dir("rominabox-publish");
        let destination = root.join("Game.app");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old"), b"old").unwrap();
        let aside = set_aside_name(&destination);
        fs::rename(&destination, &aside).unwrap();
        fs::create_dir_all(destination.join("Contents")).unwrap();

        let error = put_back(&aside, &destination, io::Error::other("the new app did not move"));

        assert_eq!(error.stage, ErrorStage::Replace);
        assert_eq!(error.path.as_deref(), Some(aside.as_path()));
        assert!(error.sentence().contains("Game (replaced).app"), "{}", error.sentence());
        assert_eq!(fs::read(aside.join("old")).unwrap(), b"old");
    }

    #[test]
    fn a_replaced_app_is_gone_and_the_new_one_is_in_place() {
        let root = rominabox_scratch::Scratch::dir("rominabox-publish");
        let app = root.join("new.app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("new"), b"new").unwrap();
        let destination = root.join("Game.app");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old"), b"old").unwrap();

        put_in_place(&app, &destination, true).unwrap();

        assert_eq!(fs::read(destination.join("new")).unwrap(), b"new");
        assert!(!destination.join("old").exists());
        assert!(!root.join("Game (replaced).app").exists());
    }

    #[test]
    fn without_replace_an_app_in_the_way_is_left_alone() {
        let root = rominabox_scratch::Scratch::dir("rominabox-publish");
        let staging = root.join("staging");
        let app = staging.join("Game.app");
        fs::create_dir_all(&app).unwrap();
        let destination = root.join("Game.app");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old"), b"old").unwrap();

        let error = put_in_place(&app, &destination, false).unwrap_err();

        assert_eq!(error.stage, ErrorStage::Exists);
        assert_eq!(error.path.as_deref(), Some(destination.as_path()));
        assert!(destination.join("old").is_file());
        assert!(app.is_dir());
    }

    #[test]
    fn a_windows_game_is_named_as_windows_allows() {
        for (title, name) in [
            ("Sonic 3 & Knuckles", "Sonic 3 & Knuckles"),
            ("Pokémon Test", "Pokémon Test"),
            ("What? A \"Game\": <1/2>", "What- A -Game-- -1-2-"),
            ("Back\\slash|pipe*star", "Back-slash-pipe-star"),
            ("Tab\there", "Tab-here"),
            ("Ends with dots... ", "Ends with dots"),
            ("  ", "Game"),
            ("...", "Game"),
            ("con", "con-"),
            ("COM1.bin", "COM1-.bin"),
            ("Nul .x", "Nul- .x"),
            ("Console", "Console"),
        ] {
            assert_eq!(app_name(&ExportTarget::Windows, title), name, "{title:?}");
        }
        assert_eq!(app_name(&ExportTarget::Macos, "A/B: C"), "A-B- C.app");
    }
}
