//! The recipe of a game: every choice its author made, with each file in those
//! choices given by its place in the game, so that we can build the game again
//! with a newer engine and keep its identity. We export the content and the
//! firmware into the game unchanged, so in the recipe we give their places
//! there. In the recipe folder we keep a copy of each smaller file that we
//! change on the way in: the icon and the background picture, the files of
//! custom shaders, and the patches we apply while the game runs.

use super::app_files::firmware_destination_name;
use super::{ErrorStage, ExportError, ExportRequest};
use crate::content::{ContentSet, FileRole, GameFiles};
use crate::game::Game;
use crate::launch_contract::shipped;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The folder of the recipe among the game's resources.
pub(crate) const FOLDER: &str = "Recipe";
const MANIFEST: &str = "recipe.json";
const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Recipe {
    pub format_version: u32,
    pub identity: String,
    /// The game, with each file named by its path under the game's resources.
    pub game: Game,
}

/// `path` with "/" between its parts, as we write every path in a recipe.
fn portable(path: &Path) -> PathBuf {
    PathBuf::from(
        path.components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn failed(path: &Path) -> impl Fn(std::io::Error) -> ExportError + '_ {
    move |error| ExportError::io(ErrorStage::Stage, path, error)
}

/// Write the recipe of the game `request` makes, whose identity is
/// `identity`, into `resources`, where we staged its content with
/// `rom_relative` as its game file.
pub(super) fn write(
    request: &ExportRequest,
    identity: &str,
    resources: &Path,
    rom_relative: &Path,
    content: &ContentSet,
    system: &crate::systems::System,
) -> Result<(), ExportError> {
    let folder = resources.join(FOLDER);
    let copy_in = |source: &Path, name: &str| -> Result<PathBuf, ExportError> {
        let destination = folder.join(name);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(failed(parent))?;
        }
        fs::copy(source, &destination).map_err(failed(source))?;
        Ok(portable(&Path::new(FOLDER).join(name)))
    };
    let named = |stem: &str, path: &Path| match path.extension() {
        Some(extension) => format!("{stem}.{}", extension.to_string_lossy()),
        None => stem.to_string(),
    };
    let file_name = |path: &Path| path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    fs::create_dir_all(&folder).map_err(failed(&folder))?;

    let icon = request.game.icon.as_deref().map(|icon| copy_in(icon, &named("icon", icon))).transpose()?;
    let background = request
        .game
        .background
        .as_deref()
        .map(|background| copy_in(background, &named("background", background)))
        .transpose()?;
    let mut added: Vec<PathBuf> = content
        .files
        .iter()
        .filter(|file| file.role == FileRole::Added)
        .map(|file| portable(&Path::new("content").join(&file.relative)))
        .collect();
    for patch in &content.played_patches {
        added.push(copy_in(patch, &format!("patches/{}", file_name(patch)))?);
    }
    let firmware = request
        .game
        .firmware
        .iter()
        .filter_map(|path| firmware_destination_name(path, system))
        .map(|name| portable(&Path::new(shipped!(Firmware).0).join(name)))
        .collect();
    let (mut shaders, shader_files) = crate::shaders::pack_selection(&request.game.shaders)
        .map_err(|message| ExportError::new(ErrorStage::Stage, message))?;
    for (name, source) in &shader_files {
        copy_in(source, name)?;
    }
    for custom in &mut shaders.custom {
        custom.path = portable(&Path::new(FOLDER).join(&custom.path));
    }
    let recipe = Recipe {
        format_version: FORMAT_VERSION,
        identity: identity.to_string(),
        game: Game {
            rom: portable(rom_relative),
            icon,
            background,
            firmware,
            shaders,
            files: GameFiles { left_out: Vec::new(), added, decompress: false },
            both_platforms: false,
            ..request.game.clone()
        },
    };
    let path = folder.join(MANIFEST);
    fs::write(&path, serde_json::to_vec_pretty(&recipe).expect("a recipe serializes")).map_err(failed(&path))
}

/// Whether `path`, from a recipe, stays under the resources it names.
fn inside(path: &Path) -> bool {
    path.components().all(|part| matches!(part, Component::Normal(_)))
}

/// The recipe of the game whose resources are in `resources`, with each file
/// at its path there. We refuse a recipe of another format, and any path in
/// it that leaves the resources.
pub(crate) fn read(resources: &Path) -> Result<Recipe, String> {
    let path = resources.join(FOLDER).join(MANIFEST);
    let text = fs::read_to_string(&path).map_err(|_| "This game has no recipe. Export it again once.".to_string())?;
    let mut recipe: Recipe =
        serde_json::from_str(&text).map_err(|error| format!("We could not read the recipe of this game: {error}"))?;
    if recipe.format_version != FORMAT_VERSION {
        return Err(format!("This game has a recipe of another version, {}.", recipe.format_version));
    }
    let game = &mut recipe.game;
    let mut paths: Vec<&mut PathBuf> = vec![&mut game.rom];
    paths.extend(game.icon.as_mut());
    paths.extend(game.background.as_mut());
    paths.extend(game.firmware.iter_mut());
    paths.extend(game.files.added.iter_mut());
    paths.extend(game.shaders.custom.iter_mut().map(|custom| &mut custom.path));
    for path in paths {
        if !inside(path) {
            return Err(format!("The recipe of this game names a file outside it: {}", path.display()));
        }
        *path = resources.join(&*path);
    }
    Ok(recipe)
}
