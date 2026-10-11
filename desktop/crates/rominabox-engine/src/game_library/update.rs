//! Updates to a game on this computer, made in place: the newest nightly of
//! its core, or the whole game built again with the builder's engine. We
//! never touch the game's data folder, which is named after its identity, so
//! its saves, states and settings stay.

use super::{Layout, Library};
use crate::cores::Transport;
use crate::packaging::{
    check_windows_core, copy_tree, export_game, finish_macos_core, macos_app_targets, pack_windows_game, recipe,
    resolve_cached, seal_macos_app, unpack_windows_game, ExportRequest, ExportTarget, UnpackedWindowsGame,
};
use crate::target::Target;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

/// The result of an update of a core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoreUpdate {
    /// We put a newer core into the game.
    Updated,
    /// The game already has the newest core.
    Current,
}

/// The core a game was made with, as its game.json records it.
pub(crate) struct GameCore {
    /// The core's file among the game's own files.
    file: String,
    core: &'static crate::systems::Core,
    system_name: &'static str,
}

impl GameCore {
    /// The core of the game whose files are in `resources`, for `target`. We
    /// refuse a game made with another core than the one its console now
    /// uses: that is a change of core, which an update of the engine makes.
    fn read(resources: &Path, target: Target) -> Result<Self, String> {
        let manifest = game_manifest(resources)?;
        let field = |name: &str| manifest[name].as_str().unwrap_or_default().to_string();
        let system = crate::systems::find(&field("system"))
            .ok_or_else(|| format!("We do not know the console of this game, {}.", field("system")))?;
        let core = system
            .preferred_core()
            .ok_or_else(|| format!("We have no core for {}.", system.name))?;
        if core.artifact_for(target) != Some(field("coreSource").as_str()) {
            return Err(format!(
                "This game was made with another core than the one we now use for {}. Update its engine to change it.",
                system.name
            ));
        }
        Ok(Self { file: field("core"), core, system_name: &system.name })
    }
}

fn game_manifest(resources: &Path) -> Result<serde_json::Value, String> {
    let path = resources.join("game.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or_else(|| format!("We could not read {}.", path.display()))
}

/// A folder of our own beside a game's app, on the same drive, so that we
/// can move a new copy of the app into place. We remove it when we are done.
struct Work {
    path: PathBuf,
}

impl Work {
    fn beside(app: &Path) -> Result<Self, String> {
        let parent = app.parent().ok_or_else(|| format!("{} has no folder.", app.display()))?;
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        for attempt in 0_u32..1000 {
            let path = parent.join(format!(".rominabox-update-{seed}-{attempt}"));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("We could not create {}: {error}", path.display())),
            }
        }
        Err(format!("We could not create a folder beside {}.", app.display()))
    }

    /// Put `new` in place of `app`. We move a folder (a Mac app) aside first
    /// and back again when the new one does not go in.
    fn replace(&self, new: &Path, app: &Path) -> Result<(), String> {
        let failed = |error: std::io::Error| format!("We could not replace {}: {error}", app.display());
        if !app.is_dir() {
            return fs::rename(new, app).map_err(failed);
        }
        let previous = self.path.join("previous");
        fs::rename(app, &previous).map_err(failed)?;
        fs::rename(new, app).map_err(|error| {
            let _ = fs::rename(&previous, app);
            failed(error)
        })
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A copy of a game's app in a work folder, where we change its files.
enum Opened {
    /// A Windows game unpacked into `folder`, with what we pack it with.
    Windows { folder: PathBuf, unpacked: UnpackedWindowsGame },
    /// A copy of a Mac app.
    Macos { app: PathBuf },
}

impl Opened {
    fn open(layout: Layout, app: &Path, work: &Work) -> Result<Self, String> {
        match layout {
            Layout::Windows => {
                let folder = work.path.join("game");
                fs::create_dir(&folder).map_err(|error| format!("We could not create {}: {error}", folder.display()))?;
                let unpacked =
                    unpack_windows_game(app, &folder).map_err(|error| format!("We could not read the game: {error}"))?;
                Ok(Self::Windows { folder, unpacked })
            }
            Layout::Macos => {
                let copy = work.path.join(app.file_name().ok_or_else(|| format!("{} has no name.", app.display()))?);
                copy_tree(app, &copy).map_err(|error| error.message)?;
                Ok(Self::Macos { app: copy })
            }
        }
    }

    fn resources(&self) -> PathBuf {
        match self {
            Self::Windows { folder, .. } => folder.join("Resources"),
            Self::Macos { app } => app.join("Contents/Resources"),
        }
    }

    /// Every target the game runs on, the platform's first.
    fn targets(&self) -> Result<Vec<Target>, String> {
        match self {
            Self::Windows { .. } => Ok(vec![Target::WindowsX86_64]),
            Self::Macos { app } => macos_app_targets(app).map_err(|error| error.message),
        }
    }

    /// The core from `builds`, one for each target of the game, as we would
    /// put it into the game. We refuse a core the game could not load.
    fn finished_core(&self, builds: &[(Target, PathBuf)], work: &Work, core: &GameCore) -> Result<PathBuf, String> {
        match (self, builds) {
            (Self::Windows { .. }, [(_, only)]) => {
                check_windows_core(only).map_err(|error| error.message)?;
                Ok(only.clone())
            }
            (Self::Windows { .. }, _) => Err("A Windows game has one core.".to_string()),
            (Self::Macos { app }, _) => {
                let finished = work.path.join("core");
                finish_macos_core(builds, &finished, app, core.system_name).map_err(|error| error.message)?;
                Ok(finished)
            }
        }
    }

    /// Put this copy, with the files we changed, in place of `app`, the game
    /// `identity`.
    fn close(self, app: &Path, work: &Work, identity: &str, kit: &Path) -> Result<(), String> {
        match self {
            Self::Windows { folder, unpacked } => {
                let program = work.path.join("game.exe");
                pack_windows_game(&folder, &unpacked.launcher, &unpacked.runtime, &program, &AtomicBool::new(false))
                    .map_err(|error| error.message)?;
                work.replace(&program, app)
            }
            Self::Macos { app: copy } => {
                // We sign the app with the sandbox its recipe gives, as its export did.
                let recipe = own_recipe(&copy.join("Contents/Resources"), identity)?;
                let request = rebuild_request(recipe, ExportTarget::Macos, work, kit, None, None);
                seal_macos_app(&copy, &request, identity).map_err(|error| error.message)?;
                work.replace(&copy, app)
            }
        }
    }
}

/// The recipe in the resources `resources` of the game `identity`.
fn own_recipe(resources: &Path, identity: &str) -> Result<recipe::Recipe, String> {
    let recipe = recipe::read(resources)?;
    if recipe.identity != identity {
        return Err("The recipe in this game is the recipe of another game.".to_string());
    }
    Ok(recipe)
}

/// The export that builds the game of `recipe` again for `platform` with
/// the kit `kit`, into `work`, with its identity.
fn rebuild_request(
    recipe: recipe::Recipe,
    platform: ExportTarget,
    work: &Work,
    kit: &Path,
    core: Option<PathBuf>,
    core_cache: Option<PathBuf>,
) -> ExportRequest {
    ExportRequest {
        game: crate::game::Game { target: platform, both_platforms: false, ..recipe.game },
        zip: None,
        output_dir: work.path.join("out"),
        replace: false,
        runtime_kit: kit.to_path_buf(),
        core,
        core_cache,
        accounts_folder: None,
        identity: Some(recipe.identity),
    }
}

fn digest(path: &Path) -> Result<[u8; 32], String> {
    let bytes = fs::read(path).map_err(|error| format!("We could not read {}: {error}", path.display()))?;
    Ok(Sha256::digest(bytes).into())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    fs::copy(from, to).map(drop).map_err(|error| format!("We could not copy {} to {}: {error}", from.display(), to.display()))
}

impl Library {
    /// Bring the core of the game `identity` to the newest nightly. We first
    /// refresh the core in `cache`, the cache of the cores for this computer,
    /// from the server, as an export does, then put it into the game when it
    /// differs from the core the game has. `kit` is the runtime kit, where a
    /// core that is in no download list comes from.
    pub fn update_core(
        &self,
        identity: &str,
        kit: &Path,
        cache: &Path,
        transport: &dyn Transport,
    ) -> Result<CoreUpdate, String> {
        let own = Target::host().ok_or("We update games only on Windows and on a Mac.")?;
        self.change_core(identity, kit, cache, |core, targets| {
            let artifacts = targets
                .iter()
                .map(|&target| {
                    let artifact = core.core.artifact_for(target).ok_or_else(|| format!("We have no {target} core."))?;
                    let cache = crate::export_cores::cache_for(cache, own, target)?;
                    Ok((target, Path::new("cores").join(artifact), cache))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let wanted: Vec<_> = artifacts
                .iter()
                .map(|(target, relative, cache)| crate::export_cores::Wanted {
                    component: &core.core.component,
                    platform: *target,
                    cache,
                    present: resolve_cached(kit, Some(cache), relative).is_file(),
                })
                .collect();
            crate::export_cores::prepare(&wanted, transport, |_| {})
                .map_err(|_| "We could not download the core. Try again later.".to_string())?;
            Ok(artifacts
                .iter()
                .map(|(target, relative, cache)| (*target, resolve_cached(kit, Some(cache), relative)))
                .collect())
        })
    }

    /// Put the core `newest` gives for the game `identity` into the game,
    /// when it differs from the core the game has. `newest` gives a file for
    /// each target of the game.
    pub(crate) fn change_core(
        &self,
        identity: &str,
        kit: &Path,
        cache: &Path,
        newest: impl FnOnce(&GameCore, &[Target]) -> Result<Vec<(Target, PathBuf)>, String>,
    ) -> Result<CoreUpdate, String> {
        let app = self.app_to_change(identity, "update")?;
        let work = Work::beside(&app)?;
        let opened = Opened::open(self.layout, &app, &work)?;
        let resources = opened.resources();
        let targets = opened.targets()?;
        let core = GameCore::read(&resources, *targets.first().ok_or("The game runs on no computer we know.")?)?;
        let builds = newest(&core, &targets)?;
        let finished = opened.finished_core(&builds, &work, &core)?;
        let installed = resources.join(&core.file);
        if digest(&finished)? == digest(&installed)? {
            return Ok(CoreUpdate::Current);
        }
        copy(&finished, &installed)?;
        let licence = resolve_cached(kit, Some(cache), &Path::new("licenses").join(&core.core.license_file));
        let shipped_licence = resources.join("Legal/Licenses").join(&core.core.license_file);
        if licence.is_file() && shipped_licence.is_file() {
            copy(&licence, &shipped_licence)?;
        }
        opened.close(&app, &work, identity, kit)?;
        Ok(CoreUpdate::Updated)
    }

    /// Build the game `identity` again from its recipe with the runtime kit
    /// `kit`, with the core it has and its identity, and put the new game in
    /// place of its app. Its data folder, named after its identity, stays.
    pub fn update_engine(&self, identity: &str, kit: &Path) -> Result<(), String> {
        let platform = match self.layout {
            Layout::Windows => ExportTarget::Windows,
            Layout::Macos => ExportTarget::Macos,
        };
        let app = self.app_to_change(identity, "update")?;
        let work = Work::beside(&app)?;
        let opened = Opened::open(self.layout, &app, &work)?;
        let resources = opened.resources();
        let recipe = own_recipe(&resources, identity)?;
        let core = resources.join(game_manifest(&resources)?["core"].as_str().unwrap_or_default());
        // The licence text of the core the game has, which we keep with it.
        let cache = work.path.join("cache");
        let licences = cache.join("licenses");
        fs::create_dir_all(&licences).map_err(|error| format!("We could not create {}: {error}", licences.display()))?;
        if let Ok(entries) = fs::read_dir(resources.join("Legal/Licenses")) {
            for entry in entries.flatten().filter(|entry| entry.path().is_file()) {
                copy(&entry.path(), &licences.join(entry.file_name()))?;
            }
        }
        let request = rebuild_request(recipe, platform, &work, kit, Some(core), Some(cache));
        let made = export_game(&request, &AtomicBool::new(false), |_| {}).map_err(|error| error.message)?;
        work.replace(&made.app_path, &app)
    }
}
