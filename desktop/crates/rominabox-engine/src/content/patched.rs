//! The patches for a game (crate::patches), and the file we apply each one
//! to: the game file when the game is one file, otherwise the track given in
//! the patch as its target. We apply them to that file when exporting, in
//! the order of the list.
//!
//! For a compressed disc (CHD), we read its tracks (crate::chd_disc). We
//! apply its patches only when the author chose to include them, because we
//! then export the disc decompressed, as a sheet and its tracks.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{extension_of, is_playlist, is_sheet, same_file, ContentFile, ContentSet, GameFiles, Staging};
use crate::chd_disc::{self, Disc, TrackKind};
use crate::patches::{self, Belonging, Offered};
use crate::systems::SheetParser;

/// A compressed disc written out as tracks, with the patches for each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unpacked {
    pub disc: Disc,
    /// For each of `disc.tracks`, the patches applied to it, in order.
    pub patches: Vec<Vec<PathBuf>>,
}

/// Patches for a compressed disc: what including them costs, and whether
/// the author chose to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressedPatches {
    pub patches: Vec<PathBuf>,
    /// The compressed disc's size.
    pub without_bytes: u64,
    /// The size of its tracks and sheet, decompressed.
    pub with_bytes: u64,
    pub included: bool,
}

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
) -> Result<(Belonging, Option<CompressedPatches>), String> {
    let offered = offered(root, choices);
    if offered.is_empty() || files.is_empty() {
        return Ok((Belonging::default(), None));
    }
    if sheet.is_none() && extension_of(&files[0].source) == "chd" {
        return patch_compressed(&offered, choices.decompress, &mut files[0]);
    }

    let targets = patchable(files, sheet.is_some());
    let fallback = match sheet {
        None => targets.first().copied(),
        Some(parser) => data_track(root, entrypoint, parser, files),
    };
    let sources: Vec<PathBuf> = targets.iter().map(|&index| files[index].source.clone()).collect();
    let fallback = fallback.and_then(|index| targets.iter().position(|&target| target == index));
    let (each, mut found) = assign(sources.len(), fallback, offered, |at, offered| {
        patches::belonging(&sources[at], offered)
    })?;
    for (&index, belonging) in targets.iter().zip(each) {
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
    Ok((found, None))
}

/// The patches we offer, in file-name order: the ones beside the game that the
/// author did not leave out, then the ones the author added.
fn offered(root: &Path, choices: &GameFiles) -> Vec<(PathBuf, Offered)> {
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
    offered
}

/// Which of `count` targets each patch belongs to. We give each patch to
/// the first target in it, and any other to `fallback`, the game file or a
/// disc's data track. There we call `belongs` in order, because each patch
/// follows the ones before it. We refuse a patch that the author chose
/// when it has no target.
fn assign(
    count: usize,
    fallback: Option<usize>,
    offered: Vec<(PathBuf, Offered)>,
    belongs: impl Fn(usize, &[(PathBuf, Offered)]) -> Result<Belonging, String>,
) -> Result<(Vec<Belonging>, Belonging), String> {
    let mut assigned: Vec<Vec<(PathBuf, Offered)>> = vec![Vec::new(); count];
    let mut found = Belonging::default();
    for (patch, how) in offered {
        let proved = (0..count).find(|&at| {
            belongs(at, &[(patch.clone(), Offered::Beside)]).is_ok_and(|belonging| !belonging.patches.is_empty())
        });
        match (proved.or(fallback), how) {
            (Some(at), _) => assigned[at].push((patch, how)),
            (None, Offered::Chosen) => found.refused.push(patch),
            (None, Offered::Beside) => {}
        }
    }
    let each = assigned
        .iter()
        .enumerate()
        .map(|(at, offered)| if offered.is_empty() { Ok(Belonging::default()) } else { belongs(at, offered) })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((each, found))
}

/// The patches for a compressed disc, by its tracks. We stage them only
/// when the author chose to include them (`decompress`).
fn patch_compressed(
    offered: &[(PathBuf, Offered)],
    decompress: bool,
    game: &mut ContentFile,
) -> Result<(Belonging, Option<CompressedPatches>), String> {
    let chosen = || offered.iter().filter(|(_, how)| *how == Offered::Chosen).map(|(path, _)| path.clone()).collect();
    let stem = game.source.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    // We apply no patch to a disc that we cannot read as tracks.
    let Ok(disc) = chd_disc::read(&game.source, &stem) else {
        return Ok((Belonging { refused: chosen(), ..Belonging::default() }, None));
    };
    let fallback = (0..disc.tracks.len())
        .filter(|&at| disc.tracks[at].kind != TrackKind::Audio)
        .max_by_key(|&at| disc.tracks[at].bytes());
    let (each, mut found) = assign(disc.tracks.len(), fallback, offered.to_vec(), |at, offered| {
        patches::belonging_to_track(&game.source, &disc.tracks[at], offered)
    })?;
    let mut patches = Vec::with_capacity(each.len());
    for belonging in each {
        found.patches.extend(belonging.patches.iter().cloned());
        found.refused.extend(belonging.refused);
        patches.push(belonging.patches);
    }
    if found.patches.is_empty() {
        return Ok((found, None));
    }
    let compressed = CompressedPatches {
        patches: found.patches.clone(),
        without_bytes: fs::metadata(&game.source).map_err(|error| format!("read {}: {error}", game.source.display()))?.len(),
        with_bytes: disc.bytes() + disc.sheet().len() as u64,
        included: decompress,
    };
    if decompress {
        game.staging = Staging::Unpacked(Box::new(Unpacked { disc, patches }));
    }
    Ok((found, Some(compressed)))
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

impl ContentFile {
    /// Where we write the file, relative to the content folder. For a
    /// compressed disc written out as tracks, that is its sheet and tracks.
    pub fn written(&self) -> Vec<PathBuf> {
        match &self.staging {
            Staging::Unpacked(unpacked) => std::iter::once(&unpacked.disc.sheet_name)
                .chain(unpacked.disc.tracks.iter().map(|track| &track.name))
                .map(|name| self.relative.with_file_name(name))
                .collect(),
            _ => vec![self.relative.clone()],
        }
    }
}

impl ContentSet {
    /// The patches we apply when exporting, in the order of their target files.
    pub fn patches(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .flat_map(|file| match &file.staging {
                Staging::Patched(patches) => patches.clone(),
                Staging::Unpacked(unpacked) => unpacked.patches.concat(),
                Staging::Copy | Staging::Bytes(_) => Vec::new(),
            })
            .collect()
    }
}
