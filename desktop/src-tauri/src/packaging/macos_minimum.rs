//! The oldest macOS a game runs on, the newest one its programs require.
//!
//! The load commands of each program contain it, per processor slice, in
//! `minos` of LC_BUILD_VERSION or in `version` of the older
//! LC_VERSION_MIN_MACOSX, which Intel cores built for old systems still use.
//! We write the newest of them into the Info.plist of the game, and no newer
//! one, so Intel Macs, which stop at older systems, can open the game.

use crate::export_error::{ErrorStage, ExportError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The oldest macOS each slice of `program` runs on.
fn minimums(program: &Path) -> Result<Vec<Vec<u32>>, ExportError> {
    let output = Command::new("/usr/bin/otool")
        .arg("-l")
        .arg(program)
        .output()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Configure,
                format!("could not run otool: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(ExportError::command(
            ErrorStage::Configure,
            "otool",
            &output,
        ));
    }
    let mut found = Vec::new();
    let mut version_min = false;
    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
    {
        if let Some(command) = line.strip_prefix("cmd ") {
            version_min = command == "LC_VERSION_MIN_MACOSX";
        }
        let version = match line.strip_prefix("minos ") {
            Some(version) => Some(version),
            None if version_min => line.strip_prefix("version "),
            None => None,
        };
        if let Some(version) = version {
            found.push(
                version
                    .split('.')
                    .filter_map(|part| part.parse().ok())
                    .collect(),
            );
        }
    }
    Ok(found)
}

/// The newest of the oldest systems `programs` run on, as `11.0`.
pub(super) fn newest(programs: &[PathBuf]) -> Result<String, ExportError> {
    let mut newest: Option<Vec<u32>> = None;
    for program in programs {
        for minimum in minimums(program)? {
            if newest.as_ref().is_none_or(|current| minimum > *current) {
                newest = Some(minimum);
            }
        }
    }
    let newest = newest.ok_or_else(|| {
        ExportError::new(
            ErrorStage::Configure,
            "no program in the game says which macOS it needs",
        )
    })?;
    let mut parts: Vec<String> = newest.iter().map(u32::to_string).collect();
    if parts.len() == 1 {
        parts.push("0".into());
    }
    Ok(parts.join("."))
}
