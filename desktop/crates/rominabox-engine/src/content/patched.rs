//! The patches for a game that is one file (crate::patches). We apply them
//! to it when exporting, in the order of the list.

use std::path::{Path, PathBuf};

use super::{same_file, ContentFile, ContentSet, GameFiles, Staging};
use crate::patches::{self, Belonging, Offered};

/// The game file, first in `files`, staged with the patches that belong to
/// it: the ones beside it the author did not leave out, and the ones the
/// author added.
pub(super) fn patch_game_file(root: &Path, choices: &GameFiles, files: &mut [ContentFile]) -> Result<Belonging, String> {
    let Some(game) = files.first_mut() else {
        return Ok(Belonging::default());
    };
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
    let belonging = patches::belonging(&game.source, &offered)?;
    if !belonging.patches.is_empty() {
        game.staging = Staging::Patched(belonging.patches.clone());
    }
    Ok(belonging)
}

impl ContentSet {
    /// The patches we apply to the game file when exporting, in order.
    pub fn patches(&self) -> &[PathBuf] {
        match self.files.first().map(|game| &game.staging) {
            Some(Staging::Patched(patches)) => patches,
            _ => &[],
        }
    }
}
