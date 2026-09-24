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

/// The app that an export of `request` produces. On Windows we do no
/// packaging here, so nothing can be in the way.
fn destination(request: &ExportRequest) -> Option<PathBuf> {
    match request.target {
        ExportTarget::Macos => Some(request.output_dir.join(macos_app_name(&request.title))),
        ExportTarget::Windows => None,
    }
}

pub(crate) fn macos_app_name(title: &str) -> String {
    format!("{}.app", safe_filename(title))
}

/// Before doing anything, refuse an export to the place of an existing app,
/// unless the request includes replacing it.
pub(crate) fn refuse_unless_replacing(request: &ExportRequest) -> Result<(), ExportError> {
    match destination(request) {
        Some(app) if !request.replace => refuse_existing(&app),
        _ => Ok(()),
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

/// Move the finished `app` to `destination`. We first move an app that is
/// already there into `staging`, and put it back if the new one cannot replace
/// it. Removing `staging` afterwards removes the old app.
pub(crate) fn put_in_place(
    app: &Path,
    destination: &Path,
    staging: &Path,
    replace: bool,
) -> Result<(), ExportError> {
    let saving = |error: io::Error| ExportError::io(ErrorStage::Complete, destination, error);
    if !occupied(destination) {
        return fs::rename(app, destination).map_err(saving);
    }
    if !replace {
        return refuse_existing(destination);
    }
    let previous = staging.join("previous");
    fs::rename(destination, &previous).map_err(saving)?;
    fs::rename(app, destination).map_err(|error| {
        let _ = fs::rename(&previous, destination);
        saving(error)
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// When the swap fails after we moved the old app aside, we put the old
    /// app back.
    #[test]
    fn a_new_app_that_cannot_be_moved_in_puts_the_old_one_back() {
        let root = rominabox_scratch::Scratch::dir("rominabox-publish");
        let staging = root.join("staging");
        fs::create_dir_all(&staging).unwrap();
        let destination = root.join("Game.app");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old"), b"old").unwrap();

        let error =
            put_in_place(&staging.join("missing.app"), &destination, &staging, true).unwrap_err();

        assert_eq!(error.stage, ErrorStage::Complete);
        assert_eq!(fs::read(destination.join("old")).unwrap(), b"old");
        assert!(!staging.join("previous").exists());
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

        let error = put_in_place(&app, &destination, &staging, false).unwrap_err();

        assert_eq!(error.stage, ErrorStage::Exists);
        assert_eq!(error.path.as_deref(), Some(destination.as_path()));
        assert!(destination.join("old").is_file());
        assert!(app.is_dir());
    }
}
