//! The games on this computer, found by their data folders, for the builder's
//! Game data section and the command line. We export their data to one zip,
//! import a backup into one game or a bulk backup into every game it contains,
//! and remove the data of a game whose app is gone.
//!
//! Every game runs in its own sandbox. On a Mac, each game has a container
//! folder in ~/Library/Containers named after its bundle identifier, and on
//! Windows a folder in %LOCALAPPDATA%\Packages named after its sandbox. We
//! read that one folder, keep the entries that start with our prefix, and
//! list a game only when its data folder contains a manifest, which we write
//! in the launcher on every launch. What a backup contains and every check of
//! a zip are in `game_data`.

use crate::game_data::{self, Check, Game};
use crate::launch_contract::game_file;
use crate::packaging::{bundle_identifier, game_data_folder, runtime_folder, BUNDLE_PREFIX};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Where a platform keeps the sandboxes of games.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    Macos,
    Windows,
}

impl Layout {
    /// The layout of the platform we run on, or None where no game runs.
    pub fn of_host() -> Option<Layout> {
        if cfg!(target_os = "macos") {
            Some(Layout::Macos)
        } else if cfg!(windows) {
            Some(Layout::Windows)
        } else {
            None
        }
    }

    /// The folder below the per-user root that contains every app's sandbox.
    fn sandboxes(self) -> &'static str {
        match self {
            Layout::Macos => "Library/Containers",
            Layout::Windows => "Packages",
        }
    }

    /// The name of the sandbox folder of the game `identity`.
    fn sandbox_name(self, identity: &str) -> String {
        match self {
            Layout::Macos => bundle_identifier(identity),
            Layout::Windows => format!("{}{identity}", sandbox_prefix().to_ascii_lowercase()),
        }
    }

    /// The identity of the game whose sandbox folder is `name`, or None for
    /// another app's folder.
    fn identity_of(self, name: &str) -> Option<&str> {
        match self {
            Layout::Macos => name.strip_prefix(BUNDLE_PREFIX),
            Layout::Windows => sandbox_identity(name),
        }
        .filter(|identity| is_identity(identity))
    }

    /// The per-user data folder inside a sandbox, where we put the games
    /// folder in the launcher.
    fn user_data(self) -> &'static str {
        match self {
            Layout::Macos => "Data/Library/Application Support",
            Layout::Windows => "AC",
        }
    }
}

/// We name a Windows game's sandbox with this and the game's identity
/// (vendor/retroarch/rominabox_launch.h). Its folder in Packages has the same
/// name, in lower case.
pub(crate) fn sandbox_prefix() -> &'static str {
    include_str!("../../../../vendor/retroarch/rominabox_launch.h")
        .lines()
        .find_map(|line| line.strip_prefix("#define RIB_GAME_APP_ID_PREFIX \"")?.strip_suffix('"'))
        .expect("rominabox_launch.h declares RIB_GAME_APP_ID_PREFIX")
}

/// The rest of `name`, the name of a folder in Packages, when it starts with
/// the sandbox prefix in any case.
pub(crate) fn sandbox_identity(name: &str) -> Option<&str> {
    let prefix = sandbox_prefix();
    name.get(..prefix.len()).filter(|start| start.eq_ignore_ascii_case(prefix)).map(|_| &name[prefix.len()..])
}

/// Remove the registration of the sandbox of the Windows game `identity`.
#[cfg(windows)]
pub(crate) fn forget_sandbox(identity: &str) {
    use windows_sys::Win32::Security::Isolation::DeleteAppContainerProfile;
    let name: Vec<u16> = format!("{}{identity}", sandbox_prefix()).encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { DeleteAppContainerProfile(name.as_ptr()) };
}

/// Whether `identity` has the form of a game's identity, a short run of
/// lower-case hexadecimal digits. Only such a name goes into a path we read
/// or remove.
fn is_identity(identity: &str) -> bool {
    (1..=64).contains(&identity.len()) && identity.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// A game on this computer: what its manifest says, and what we found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGame {
    #[serde(flatten)]
    pub game: Game,
    /// The game's icon beside its manifest, when we copied one there in the
    /// launcher.
    pub icon: Option<PathBuf>,
    /// Whether the app named in the manifest is still there.
    pub app_present: bool,
    /// Whether the game is running now.
    pub running: bool,
    /// The name we suggest for an export of its data alone.
    pub file_name: String,
    /// The game's data folder.
    pub data: PathBuf,
}

/// A game whose data a backup contains, and whether that game is on this
/// computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupGame {
    #[serde(flatten)]
    pub game: Game,
    pub here: bool,
}

/// A game in a bulk backup that we did not import, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub game: Game,
    pub reason: String,
}

/// What importing a bulk backup did with each of its games.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImport {
    pub imported: Vec<Game>,
    /// The games whose data folder is not on this computer.
    pub not_here: Vec<Game>,
    pub refused: Vec<Skipped>,
}

/// The games of one person on one computer.
pub struct Library {
    /// The person's home folder on a Mac, or `%LOCALAPPDATA%` on Windows.
    root: PathBuf,
    layout: Layout,
}

impl Library {
    /// The games of the person running this program.
    pub fn here() -> Result<Library, String> {
        let layout = Layout::of_host().ok_or("Games do not run on this platform.")?;
        let root = match layout {
            Layout::Macos => dirs::home_dir(),
            Layout::Windows => dirs::data_local_dir(),
        };
        let root = root.ok_or("We could not find your user folder.")?;
        Ok(Library::at(root, layout))
    }

    /// The games whose sandboxes are below `root`, laid out as on `layout`.
    pub fn at(root: PathBuf, layout: Layout) -> Library {
        Library { root, layout }
    }

    fn sandboxes(&self) -> PathBuf {
        self.root.join(self.layout.sandboxes())
    }

    fn sandbox(&self, identity: &str) -> PathBuf {
        self.sandboxes().join(self.layout.sandbox_name(identity))
    }

    /// The data folder of the game `identity`.
    pub fn data_dir(&self, identity: &str) -> PathBuf {
        self.sandbox(identity).join(self.layout.user_data()).join(game_data_folder(identity))
    }

    /// The game `identity`, when its data folder contains its manifest.
    fn installed(&self, identity: &str) -> Option<InstalledGame> {
        if !is_identity(identity) {
            return None;
        }
        let data = self.data_dir(identity);
        let game = game_data::read_manifest(&data).filter(|game| game.identity == identity)?;
        let icon = Some(data.join(game_file!(Icon))).filter(|icon| icon.is_file());
        let app = Path::new(&game.app);
        let app_present = !game.app.is_empty() && app.exists();
        let running = app_present && running(app);
        let file_name = game_data::file_name(&game);
        Some(InstalledGame { game, icon, app_present, running, file_name, data })
    }

    /// Every game on this computer, by title.
    pub fn games(&self) -> Vec<InstalledGame> {
        let mut games: Vec<InstalledGame> = fs::read_dir(self.sandboxes())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                self.installed(self.layout.identity_of(&name)?)
            })
            .collect();
        games.sort_by(|a, b| {
            (a.game.title.to_lowercase(), &a.game.identity).cmp(&(b.game.title.to_lowercase(), &b.game.identity))
        });
        games
    }

    fn find(&self, identity: &str) -> Result<InstalledGame, String> {
        self.installed(identity).ok_or_else(|| format!("There is no game {identity} on this computer."))
    }

    /// Write the data of the games `identities`, or of every game, to `zip`,
    /// and return those games.
    pub fn export(&self, identities: Option<&[String]>, zip: &Path) -> Result<Vec<Game>, String> {
        let games = match identities {
            None => self.games(),
            Some(identities) => identities.iter().map(|identity| self.find(identity)).collect::<Result<_, _>>()?,
        };
        if games.is_empty() {
            return Err("There is no game to export.".into());
        }
        let folders: Vec<PathBuf> = games.iter().map(|game| game.data.clone()).collect();
        game_data::export(&folders, zip)?;
        Ok(games.into_iter().map(|game| game.game).collect())
    }

    /// The games whose data `zip` contains, in its order, and which of them
    /// are on this computer.
    pub fn open(&self, zip: &Path) -> Result<Vec<BackupGame>, String> {
        Ok(game_data::list(zip)?
            .into_iter()
            .map(|game| BackupGame { here: self.installed(&game.identity).is_some(), game })
            .collect())
    }

    /// Whether the `which`th game in `zip` can go into the game `identity`.
    pub fn check(&self, zip: &Path, which: usize, identity: &str) -> Check {
        match self.ready(identity) {
            Ok(target) => game_data::check(zip, which, &target.data),
            Err(reason) => Check::Refused(reason),
        }
    }

    /// Replace the data of the game `identity` with that of the `which`th
    /// game in `zip`.
    pub fn import(&self, zip: &Path, which: usize, identity: &str) -> Result<(), String> {
        game_data::import(zip, which, &self.ready(identity)?.data)
    }

    /// Import each game in `zip` into the game with the same identity on
    /// this computer.
    pub fn import_all(&self, zip: &Path) -> Result<BulkImport, String> {
        let mut report = BulkImport::default();
        for (which, game) in game_data::list(zip)?.into_iter().enumerate() {
            if self.installed(&game.identity).is_none() {
                report.not_here.push(game);
                continue;
            }
            let imported = match self.check(zip, which, &game.identity) {
                Check::Refused(reason) => Err(reason),
                _ => self.import(zip, which, &game.identity),
            };
            match imported {
                Ok(()) => report.imported.push(game),
                Err(reason) => report.refused.push(Skipped { game, reason }),
            }
        }
        Ok(report)
    }

    /// The game `identity`, when it is here and not running, so we may
    /// change its data.
    fn ready(&self, identity: &str) -> Result<InstalledGame, String> {
        let game = self.find(identity)?;
        if game.running {
            return Err(format!("\u{201c}{}\u{201d} is open. Quit it, then try again.", game.game.title));
        }
        Ok(game)
    }

    /// Remove everything stored for the game `identity`, whose app is gone:
    /// its sandbox folder, with its data, and on Windows the registration of
    /// its sandbox and its unpacked copy. We remove only a folder whose name
    /// we made from a checked identity and found to be a folder, not a link.
    pub fn remove(&self, identity: &str) -> Result<(), String> {
        let game = self.ready(identity)?;
        if game.app_present {
            return Err(format!("\u{201c}{}\u{201d} is still at {}.", game.game.title, game.game.app));
        }
        if !self.root.is_absolute() {
            return Err(format!("{} is not an absolute path.", self.root.display()));
        }
        let mut folders = vec![self.sandbox(identity)];
        if self.layout == Layout::Windows {
            #[cfg(windows)]
            forget_sandbox(identity);
            folders.push(self.root.join(runtime_folder(identity)));
        }
        for folder in folders {
            remove_folder(&folder).map_err(|error| format!("We could not remove {}: {error}", folder.display()))?;
        }
        Ok(())
    }
}

/// Remove the folder at `path` and everything in it, when it is there. We
/// refuse a link or a file at that path.
fn remove_folder(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other("it is not a folder"));
    }
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Whether a program in the app at `app` is running. On a Mac, the launcher
/// and the player are both programs inside the app bundle, so we look for a
/// process whose program is there.
#[cfg(target_os = "macos")]
fn running(app: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(app) = app.canonicalize() else {
        return false;
    };
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return false;
    }
    // Room for processes that start between the two calls.
    let mut pids = vec![0 as libc::c_int; count as usize + 64];
    let bytes = (pids.len() * std::mem::size_of::<libc::c_int>()) as libc::c_int;
    let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(listed.max(0) as usize);
    pids.into_iter().any(|pid| {
        let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let length = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        length > 0 && Path::new(std::ffi::OsStr::from_bytes(&path[..length as usize])).starts_with(&app)
    })
}

/// On Windows, the app is the program the person opened, which runs until
/// the game ends.
#[cfg(windows)]
fn running(app: &Path) -> bool {
    crate::publish::running(app)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn running(_app: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests;
