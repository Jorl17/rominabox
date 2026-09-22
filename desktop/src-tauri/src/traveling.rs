//! The files that an export will copy for a dropped file.
//!
//! At export we copy what `content::collect_for` returns for the game file and
//! its console, so we call it the same way for the receipt. Without the
//! console the result can differ, because a companion file required for one
//! console, such as the `.sub` for a PC Engine CD sheet, is optional for
//! another, and the receipt would list files that the export does not copy.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::content;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traveling {
    /// The game file that we passed to `collect_for`. A dropped folder or a
    /// `.sbi` next to it is not that file. We must pass this path to the
    /// export, or we reject the folder in `collect_for` and the receipt lists
    /// a file that we do not copy.
    pub entry: PathBuf,
    pub files: Vec<String>,
}

pub fn files_for(dropped: &Path, system: Option<&str>) -> Result<Traveling, String> {
    let entry = content::resolve_dropped(dropped)?;
    let set = content::collect_for(&entry, system.filter(|id| !id.is_empty()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rominabox-traveling-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn export_copies(entry: &Path, system: &str) -> Vec<String> {
        content::collect_for(entry, Some(system))
            .unwrap()
            .files
            .iter()
            .map(|file| file.relative.to_string_lossy().into_owned())
            .collect()
    }

    // For every console, the details step lists the files we copy at export.
    // We ask in the same way for both, so the list cannot differ by one file.
    #[test]
    fn the_receipt_names_what_this_consoles_export_copies() {
        let root = fixture("ccd");
        fs::write(root.join("game.ccd"), b"[CloneCD]\n").unwrap();
        fs::write(root.join("game.img"), b"data").unwrap();
        fs::write(root.join("game.sub"), b"sub").unwrap();
        let ccd = root.join("game.ccd");

        for system in ["ps1", "pcecd"] {
            let receipt = files_for(&ccd, Some(system)).unwrap().files;
            assert_eq!(
                receipt,
                export_copies(&ccd, system),
                "for {system} the details step names different files than export copies"
            );
        }
        fs::remove_dir_all(&root).unwrap();
    }

    // Both consoles use a CloneCD `.sub` when there is one, but only Beetle PCE
    // Fast cannot open the disc without it. Without a console, we require a
    // companion file only when every console requires it, so a receipt made
    // without a console can show a disc that fails in the PC Engine CD export.
    #[test]
    fn the_receipt_refuses_what_this_consoles_export_refuses() {
        let root = fixture("ccd-no-sub");
        fs::write(root.join("game.ccd"), b"[CloneCD]\n").unwrap();
        fs::write(root.join("game.img"), b"data").unwrap();
        let ccd = root.join("game.ccd");

        let refused = files_for(&ccd, Some("pcecd")).unwrap_err();
        assert!(refused.contains("game.sub"), "{refused}");
        fs::remove_dir_all(&root).unwrap();
    }
}
