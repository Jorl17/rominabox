//! Updates to a game on this computer, made in place: the newest nightly of
//! its core. We never touch the game's data folder, which is named after its
//! identity, so its saves, states and settings stay.

use super::{Layout, Library};
use crate::cores::Transport;
use crate::packaging::{check_windows_core, pack_windows_game, resolve_cached, unpack_windows_game};
use crate::target::Target;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

/// What an update of a core did.
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
}

impl GameCore {
    /// The core of the game whose files are in `resources`, for `target`. We
    /// refuse a game made with another core than the one its console now
    /// uses: that is a change of core, which an update of the engine makes.
    fn read(resources: &Path, target: Target) -> Result<Self, String> {
        let path = resources.join("game.json");
        let text = fs::read_to_string(&path).map_err(|error| format!("We could not read {}: {error}", path.display()))?;
        let manifest: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| format!("We could not read {}: {error}", path.display()))?;
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
        Ok(Self { file: field("core"), core })
    }
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
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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
        let target = match self.layout {
            Layout::Windows => Target::WindowsX86_64,
            Layout::Macos => return Err("We cannot update the core of a Mac game yet.".to_string()),
        };
        self.change_core(identity, kit, cache, target, |core| {
            let artifact = core.core.artifact_for(target).expect("GameCore::read checked the artifact");
            let artifact_relative = Path::new("cores").join(artifact);
            let present = resolve_cached(kit, Some(cache), &artifact_relative).is_file();
            crate::export_cores::prepare(
                &[crate::export_cores::Wanted { component: &core.core.component, platform: target, cache, present }],
                transport,
                |_| {},
            )
            .map_err(|_| "We could not download the core. Try again later.".to_string())?;
            Ok(resolve_cached(kit, Some(cache), &artifact_relative))
        })
    }

    /// Put the core `newest` gives for the game `identity` into the game, when
    /// it differs from the core the game has.
    pub(crate) fn change_core(
        &self,
        identity: &str,
        kit: &Path,
        cache: &Path,
        target: Target,
        newest: impl FnOnce(&GameCore) -> Result<PathBuf, String>,
    ) -> Result<CoreUpdate, String> {
        let app = self.app_to_change(identity, "update")?;
        let work = Work::beside(&app)?;
        let game = work.path.join("game");
        fs::create_dir(&game).map_err(|error| format!("We could not create {}: {error}", game.display()))?;
        let unpacked = unpack_windows_game(&app, &game).map_err(|error| format!("We could not read the game: {error}"))?;
        let core = GameCore::read(&game.join("Resources"), target)?;
        let newest = newest(&core)?;
        self.put_core(&app, &work, &game, &unpacked, &core, &newest, kit, cache)
    }

    /// Put `newest` into the game unpacked into `game`, in place of its core,
    /// with its licence text, pack the game again and move it over `app`.
    #[allow(clippy::too_many_arguments)]
    fn put_core(
        &self,
        app: &Path,
        work: &Work,
        game: &Path,
        unpacked: &crate::packaging::UnpackedWindowsGame,
        core: &GameCore,
        newest: &Path,
        kit: &Path,
        cache: &Path,
    ) -> Result<CoreUpdate, String> {
        let resources = game.join("Resources");
        let installed = resources.join(&core.file);
        if digest(newest)? == digest(&installed)? {
            return Ok(CoreUpdate::Current);
        }
        check_windows_core(newest).map_err(|error| error.message)?;
        copy(newest, &installed)?;
        let licence = resolve_cached(kit, Some(cache), &Path::new("licenses").join(&core.core.license_file));
        let shipped_licence = resources.join("Legal/Licenses").join(&core.core.license_file);
        if licence.is_file() && shipped_licence.is_file() {
            copy(&licence, &shipped_licence)?;
        }
        let program = work.path.join("game.exe");
        pack_windows_game(game, &unpacked.launcher, &unpacked.runtime, &program, &AtomicBool::new(false))
            .map_err(|error| error.message)?;
        fs::rename(&program, app).map_err(|error| format!("We could not replace {}: {error}", app.display()))?;
        Ok(CoreUpdate::Updated)
    }
}
