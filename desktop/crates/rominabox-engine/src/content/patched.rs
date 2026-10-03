//! The patches for a game (crate::patches), and the file we apply each one
//! to: the game file when the game is one file, otherwise the track given in
//! the patch as its target. We apply them to that file when exporting, in
//! the order of the list.

use std::fs;
use std::path::{Path, PathBuf};

use super::{extension_of, is_playlist, is_sheet, same_file, ContentFile, ContentSet, GameFiles, Staging};
use crate::patches::{self, Belonging, Offered};
use crate::systems::SheetParser;

/// The game's files, staged with the patches that belong to them: the ones
/// beside the game the author did not leave out, and the ones the author
/// added. `sheet` is the entrypoint's parser when the game is a sheet, and
/// `entrypoint` its file.
pub(super) fn patch_game_files(
    root: &Path,
    entrypoint: &Path,
    sheet: Option<SheetParser>,
    choices: &GameFiles,
    files: &mut [ContentFile],
) -> Result<Belonging, String> {
    let left_out = |path: &Path| {
        path.file_name()
            .is_some_and(|name| choices.left_out.iter().any(|left| left.as_str() == name.to_string_lossy()))
    };
    let chosen: Vec<PathBuf> = choices.added.iter().filter(|path| patches::is_patch_file(path)).cloned().collect();
    let mut offered: Vec<_> = patches::in_folder(root)
        .into_iter()
        .filter(|path| !left_out(path) && !chosen.iter().any(|added| same_file(added, path)))
        .map(|path| (path, Offered::Beside))
        .collect();
    offered.extend(chosen.into_iter().map(|path| (path, Offered::Chosen)));
    offered.sort_by_key(|(path, _)| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
    if offered.is_empty() {
        return Ok(Belonging::default());
    }

    let targets = patchable(files, sheet.is_some());
    let fallback = match sheet {
        None => targets.first().copied(),
        Some(parser) => data_track(root, entrypoint, parser, files),
    };
    // We give each patch to the first file given in it as its target, and
    // any other patch to the game file or a disc's data track. There we call
    // `belonging` in order, because each patch follows the ones before it.
    let mut assigned: Vec<Vec<(PathBuf, Offered)>> = vec![Vec::new(); files.len()];
    let mut found = Belonging::default();
    for (patch, how) in offered {
        let proved = targets.iter().copied().find(|&index| {
            patches::belonging(&files[index].source, &[(patch.clone(), Offered::Beside)])
                .is_ok_and(|belonging| !belonging.patches.is_empty())
        });
        match (proved.or(fallback), how) {
            (Some(index), _) => assigned[index].push((patch, how)),
            (None, Offered::Chosen) => found.refused.push(patch),
            (None, Offered::Beside) => {}
        }
    }
    for (index, offered) in assigned.into_iter().enumerate().filter(|(_, offered)| !offered.is_empty()) {
        let belonging = patches::belonging(&files[index].source, &offered)?;
        if !belonging.patches.is_empty() {
            files[index].staging = Staging::Patched(belonging.patches.clone());
        }
        found.patches.extend(belonging.patches);
        found.refused.extend(belonging.refused);
        // We keep the track names from the sheet.
        if sheet.is_none() {
            found.made = belonging.made;
        }
    }
    Ok(found)
}

/// The files a patch can be for. That is the game file when the game is one
/// file, and otherwise every file except the sheets and playlists, which only
/// name the game's files.
fn patchable(files: &[ContentFile], is_sheet_game: bool) -> Vec<usize> {
    if !is_sheet_game {
        return if files.is_empty() { Vec::new() } else { vec![0] };
    }
    (0..files.len())
        .filter(|&index| {
            let extension = extension_of(&files[index].source);
            !is_sheet(&extension) && !is_playlist(&extension) && !patches::is_patch_file(&files[index].source)
        })
        .collect()
}

/// A disc's largest data track, among `files`: where a game's program and
/// data are (a GD-ROM's high-density track, a CD's first track).
fn data_track(root: &Path, entrypoint: &Path, parser: SheetParser, files: &[ContentFile]) -> Option<usize> {
    let text = fs::read_to_string(entrypoint).ok()?;
    let data: Vec<PathBuf> = crate::discs::data_tracks(parser, &text).into_iter().map(|name| root.join(name)).collect();
    (0..files.len())
        .filter(|&index| data.iter().any(|track| same_file(track, &files[index].source)))
        .max_by_key(|&index| fs::metadata(&files[index].source).map(|meta| meta.len()).unwrap_or(0))
}

impl ContentSet {
    /// The patches we apply when exporting, in the order of their target files.
    pub fn patches(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter_map(|file| match &file.staging {
                Staging::Patched(patches) => Some(patches.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}
