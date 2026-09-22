//! The files that an export will copy for a dropped file.
//!
//! At export we copy what `content::collect` returns, so we show that list in
//! the receipt, including the game file and any `.sbi` next to it.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::content;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traveling {
    /// The game file that we passed to `collect`. A dropped folder or a `.sbi`
    /// next to it is not that file. We must pass this path to the export, or we
    /// reject the folder in `collect` and the receipt lists a file we do not copy.
    pub entry: PathBuf,
    pub files: Vec<String>,
}

pub fn files_for(dropped: &Path) -> Result<Traveling, String> {
    let entry = content::resolve_dropped(dropped)?;
    let set = content::collect(&entry)?;
    let files = set
        .files
        .iter()
        .map(|file| {
            file.relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    Ok(Traveling { entry, files })
}
