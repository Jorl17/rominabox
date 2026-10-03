//! Which files we will copy at export for a drop.
//!
//! At export we copy whatever `content::collect_for` returns for the game file
//! and its console, so for the Also importing line we ask exactly that. Without
//! the console, the answer can differ, because a companion file that one
//! console requires, for example the `.sub` of a PC Engine CD sheet, can be
//! optional for another, and the line would then name files we do not copy.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::content::{self, FileRole, GameFiles};

/// One file that goes with the game, and its role for the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TravelingFile {
    pub name: String,
    pub role: FileRole,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traveling {
    /// The game file we passed to `collect_for`. A dropped folder or a sibling
    /// `.sbi` is not that file. We must export with this path, because we
    /// refuse a folder in `collect_for`, and the Also importing line would
    /// then name something we do not copy.
    pub entry: PathBuf,
    /// The game file first, then the files we copy with it.
    pub files: Vec<TravelingFile>,
    /// The patches that go with the game, by file name, which we apply to it
    /// at export.
    pub patches: Vec<String>,
    /// Patches the author added that do not apply to this game.
    pub refused: Vec<String>,
    /// The files the author added, including a patch dropped on its own. This
    /// is the game's `files.added`.
    pub added: Vec<PathBuf>,
    /// When the game is a compressed disc with patches, the question whether
    /// to include them, which means decompressing it.
    pub compressed: Option<Compressed>,
}

/// Patches for a compressed disc, which go with the game only if we
/// decompress it. We ask the author, with the size for each answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compressed {
    /// The compressed game file.
    pub game: String,
    pub patches: Vec<String>,
    pub without_bytes: u64,
    pub with_bytes: u64,
    /// Whether the author chose to include them.
    pub included: bool,
}

impl Traveling {
    /// The files' names, the game file first.
    pub fn names(&self) -> Vec<String> {
        self.files.iter().map(|file| file.name.clone()).collect()
    }
}

pub fn files_for(dropped: &Path, system: Option<&str>) -> Result<Traveling, String> {
    files_with(dropped, system, &GameFiles::default())
}

/// The same, with what the author left out and added.
pub fn files_with(dropped: &Path, system: Option<&str>, choices: &GameFiles) -> Result<Traveling, String> {
    let (entry, choices) = content::dropped_game(dropped, choices)?;
    let set = content::collect_with(&entry, system.filter(|id| !id.is_empty()), &choices)?;
    let files = set
        .files
        .iter()
        .map(|file| TravelingFile {
            name: file
                .relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            role: file.role,
        })
        .collect();
    let names = |paths: &[PathBuf]| -> Vec<String> {
        paths
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
    };
    let compressed = set.compressed.as_ref().map(|compressed| Compressed {
        game: entry.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        patches: names(&compressed.patches),
        without_bytes: compressed.without_bytes,
        with_bytes: compressed.with_bytes,
        included: compressed.included,
    });
    // We also list the patches for which the author has not answered yet.
    let mut patches = names(&set.patches());
    if let Some(waiting) = compressed.as_ref().filter(|compressed| !compressed.included) {
        patches.extend(waiting.patches.iter().cloned());
    }
    Ok(Traveling {
        entry,
        files,
        patches,
        refused: names(&set.refused_patches),
        added: choices.added,
        compressed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str) -> rominabox_scratch::Scratch {
        rominabox_scratch::Scratch::dir(&format!("rominabox-traveling-{name}"))
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
    fn the_also_importing_line_names_what_this_consoles_export_copies() {
        let root = fixture("ccd");
        fs::write(root.join("game.ccd"), b"[CloneCD]\n").unwrap();
        fs::write(root.join("game.img"), b"data").unwrap();
        fs::write(root.join("game.sub"), b"sub").unwrap();
        let ccd = root.join("game.ccd");

        for system in ["ps1", "pcecd"] {
            let named = files_for(&ccd, Some(system)).unwrap().names();
            assert_eq!(
                named,
                export_copies(&ccd, system),
                "for {system} the details step names different files than export copies"
            );
        }
        fs::remove_dir_all(&root).unwrap();
    }

    // Both consoles use a CloneCD `.sub` when it is there, but only Beetle
    // PCE Fast cannot open the disc without it. When we ask without a console,
    // a companion file is required only when every console requires it, so
    // without the console we would accept a disc that we then refuse at
    // PC Engine CD export.
    #[test]
    fn the_also_importing_line_refuses_what_this_consoles_export_refuses() {
        let root = fixture("ccd-no-sub");
        fs::write(root.join("game.ccd"), b"[CloneCD]\n").unwrap();
        fs::write(root.join("game.img"), b"data").unwrap();
        let ccd = root.join("game.ccd");

        let refused = files_for(&ccd, Some("pcecd")).unwrap_err();
        assert!(refused.contains("game.sub"), "{refused}");
        fs::remove_dir_all(&root).unwrap();
    }

    /// The details step lists whatever we will copy at export. A track is
    /// the same game as the sheet that lists it, so we list the same
    /// files for either drop.
    #[test]
    fn dropping_a_track_lists_the_same_files_as_dropping_its_sheet() {
        let root = fixture("track-listed");
        fs::write(root.join("track03.bin"), b"track").unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"track03.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();

        let from_track = files_for(&root.join("track03.bin"), Some("ps1")).unwrap();
        let from_sheet = files_for(&cue, Some("ps1")).unwrap();
        assert_eq!(from_track.entry, from_sheet.entry);
        assert_eq!(from_track.files, from_sheet.files);
        fs::remove_dir_all(&root).unwrap();
    }
}
