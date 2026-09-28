//! The oldest macOS for a game, which is the newest minimum of
//! any of its programs.
//!
//! The load commands of each program give it per processor slice, as `minos`
//! of LC_BUILD_VERSION, or `version` of the older LC_VERSION_MIN_MACOSX in
//! Intel cores built for old systems (`crate::mach_o::minimum_systems`). We
//! put the newest of them in the game's Info.plist, so it requires no newer
//! system than necessary, and it opens on Intel Macs with older systems.

use crate::export_error::{ErrorStage, ExportError};
use std::fs;
use std::path::PathBuf;

/// The newest of the oldest systems for `programs`, as `11.0`. We skip a
/// file that is not a program (a stand-in core in a test).
pub(super) fn newest(programs: &[PathBuf]) -> Result<String, ExportError> {
    let mut newest: Option<Vec<u32>> = None;
    for program in programs {
        let bytes = fs::read(program).map_err(|error| ExportError::io(ErrorStage::Configure, program, error))?;
        if !crate::mach_o::is_mach_o(&bytes) {
            continue;
        }
        let minimums = crate::mach_o::minimum_systems(&bytes)
            .map_err(|error| ExportError::new(ErrorStage::Configure, format!("{}: {error}", program.display())))?;
        for minimum in minimums {
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
