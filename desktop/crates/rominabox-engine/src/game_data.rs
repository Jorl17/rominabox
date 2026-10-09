//! A game's data as one portable zip, through the same C code that the player
//! and the launcher compile (`desktop/src-tauri/gamedata/game_data.h`), so the
//! builder writes and reads exactly the zips a game does. What a backup
//! contains, how we check a zip and how we rename saves for another game are
//! all decided there. Here we only pass paths and text across.

use serde::Serialize;
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::{Path, PathBuf};

/// The C side's game, which only the C code lays out.
#[repr(C)]
struct RawGame {
    _opaque: [u8; 0],
}

const ERROR_SIZE: usize = 1024;
/// The most games one zip may contain (`RIB_GAME_DATA_GAMES`).
const MOST_GAMES: usize = 512;
/// The manifest's fields, by their names in it.
const FIELDS: [&str; 7] = ["identity", "title", "system", "console", "content", "app", "made_with"];
const PLAYER_FILE: &str = "player_file";

extern "C" {
    fn rib_game_get(game: *const RawGame, name: *const c_char) -> *const c_char;
    fn rib_game_set(game: *mut RawGame, name: *const c_char, value: *const c_char) -> c_int;
    fn rib_game_player_file_count(game: *const RawGame) -> usize;
    fn rib_game_player_file(game: *const RawGame, which: usize) -> *const c_char;
    fn rib_games_new(count: usize) -> *mut RawGame;
    fn rib_games_at(games: *mut RawGame, which: usize) -> *mut RawGame;
    fn rib_games_free(games: *mut RawGame);
    fn rib_game_manifest_write(data_dir: *const c_char, game: *const RawGame) -> c_int;
    fn rib_game_manifest_read(data_dir: *const c_char, game: *mut RawGame) -> c_int;
    fn rib_game_data_export(
        data_dirs: *const *const c_char,
        count: usize,
        zip_path: *const c_char,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
    fn rib_game_data_list(
        zip_path: *const c_char,
        games: *mut RawGame,
        capacity: usize,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
    fn rib_game_data_check(
        zip_path: *const c_char,
        which: usize,
        data_dir: *const c_char,
        source: *mut RawGame,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
    fn rib_game_data_import(
        zip_path: *const c_char,
        which: usize,
        data_dir: *const c_char,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
}

/// What a game's manifest says about it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub identity: String,
    pub title: String,
    /// The console's id, as in desktop/systems.json, and its name.
    pub system: String,
    pub console: String,
    /// The name of the game's file without its extension, after which
    /// RetroArch names saves, states and screenshots.
    pub content: String,
    /// Where the app was when the game last started.
    pub app: String,
    /// The version of ROM-in-a-Box that made the game.
    pub made_with: String,
    /// The files of the settings the player changes in the menu.
    pub player_files: Vec<String>,
}

/// What checking a backup against a game found. In JSON, the case is in
/// `kind`, and the backup's game or the sentence is in `detail`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum Check {
    /// A backup of this game.
    SameGame,
    /// A backup of another game for the same console, which we can import
    /// after telling the person which game it is from.
    OtherGame(Game),
    /// We cannot import it, for the reason in the sentence.
    Refused(String),
}

/// Room for games on the C side, freed when it goes.
struct Games(*mut RawGame);

impl Games {
    fn new(count: usize) -> Games {
        Games(unsafe { rib_games_new(count) })
    }

    fn at(&self, which: usize) -> *mut RawGame {
        unsafe { rib_games_at(self.0, which) }
    }

    fn read(&self, which: usize) -> Game {
        let game = self.at(which);
        let field = |name: &str| {
            let name = CString::new(name).expect("a field name");
            text(unsafe { rib_game_get(game, name.as_ptr()) })
        };
        let count = unsafe { rib_game_player_file_count(game) };
        Game {
            identity: field("identity"),
            title: field("title"),
            system: field("system"),
            console: field("console"),
            content: field("content"),
            app: field("app"),
            made_with: field("made_with"),
            player_files: (0..count).map(|index| text(unsafe { rib_game_player_file(game, index) })).collect(),
        }
    }

    fn write(&self, which: usize, game: &Game) {
        let target = self.at(which);
        let values = [
            &game.identity,
            &game.title,
            &game.system,
            &game.console,
            &game.content,
            &game.app,
            &game.made_with,
        ];
        let named = FIELDS.iter().zip(values).chain(game.player_files.iter().map(|file| (&PLAYER_FILE, file)));
        for (name, value) in named {
            let (name, value) = (CString::new(*name).expect("a field name"), c_text(value));
            unsafe { rib_game_set(target, name.as_ptr(), value.as_ptr()) };
        }
    }
}

impl Drop for Games {
    fn drop(&mut self) {
        unsafe { rib_games_free(self.0) };
    }
}

fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
}

/// `value` for the C side, which cannot contain a NUL: we drop any.
fn c_text(value: &str) -> CString {
    CString::new(value.replace('\0', "")).expect("no NUL is left")
}

/// A path for the C side, which reads UTF-8 on every platform.
fn c_path(path: &Path) -> Result<CString, String> {
    path.to_str()
        .map(c_text)
        .ok_or_else(|| format!("{} is not a path we can write in UTF-8", path.display()))
}

/// The sentence the C side wrote, or `fallback` when it wrote none.
fn sentence(error: &[c_char; ERROR_SIZE], fallback: &str) -> String {
    let written = text(error.as_ptr());
    if written.is_empty() {
        fallback.to_string()
    } else {
        written
    }
}

/// The manifest in a game's data folder, when there is one we can read.
pub fn read_manifest(data_dir: &Path) -> Option<Game> {
    let folder = c_path(data_dir).ok()?;
    let games = Games::new(1);
    (unsafe { rib_game_manifest_read(folder.as_ptr(), games.at(0)) } == 0).then(|| games.read(0))
}

/// Write `game` as the manifest in `data_dir`, as the launcher does.
pub fn write_manifest(data_dir: &Path, game: &Game) -> Result<(), String> {
    let folder = c_path(data_dir)?;
    let games = Games::new(1);
    games.write(0, game);
    if unsafe { rib_game_manifest_write(folder.as_ptr(), games.at(0)) } == 0 {
        Ok(())
    } else {
        Err(format!("could not write the manifest in {}: {}", data_dir.display(), std::io::Error::last_os_error()))
    }
}

/// Write the data of the games in `data_dirs` to the zip at `zip`: one game
/// at its top, or each in a folder of its own.
pub fn export(data_dirs: &[PathBuf], zip: &Path) -> Result<(), String> {
    let folders = data_dirs.iter().map(|folder| c_path(folder)).collect::<Result<Vec<_>, _>>()?;
    let pointers: Vec<*const c_char> = folders.iter().map(|folder| folder.as_ptr()).collect();
    let zip = c_path(zip)?;
    let mut error = [0 as c_char; ERROR_SIZE];
    let result =
        unsafe { rib_game_data_export(pointers.as_ptr(), pointers.len(), zip.as_ptr(), error.as_mut_ptr(), ERROR_SIZE) };
    if result == 0 {
        Ok(())
    } else {
        Err(sentence(&error, "We could not write the zip."))
    }
}

/// The games whose data the zip at `zip` contains, after checking every entry.
pub fn list(zip: &Path) -> Result<Vec<Game>, String> {
    let zip = c_path(zip)?;
    let games = Games::new(MOST_GAMES);
    let mut error = [0 as c_char; ERROR_SIZE];
    let count = unsafe { rib_game_data_list(zip.as_ptr(), games.0, MOST_GAMES, error.as_mut_ptr(), ERROR_SIZE) };
    if count < 0 {
        return Err(sentence(&error, "We could not read the zip."));
    }
    Ok((0..count as usize).map(|which| games.read(which)).collect())
}

/// Whether the `which`th game in the zip can go into the game whose data is
/// in `data_dir`.
pub fn check(zip: &Path, which: usize, data_dir: &Path) -> Check {
    let (zip, folder) = match (c_path(zip), c_path(data_dir)) {
        (Ok(zip), Ok(folder)) => (zip, folder),
        (Err(error), _) | (_, Err(error)) => return Check::Refused(error),
    };
    let source = Games::new(1);
    let mut error = [0 as c_char; ERROR_SIZE];
    let result = unsafe {
        rib_game_data_check(zip.as_ptr(), which, folder.as_ptr(), source.at(0), error.as_mut_ptr(), ERROR_SIZE)
    };
    match result {
        0 => Check::SameGame,
        1 => Check::OtherGame(source.read(0)),
        _ => Check::Refused(sentence(&error, "We cannot import this zip.")),
    }
}

/// Replace the player's data in `data_dir` with that of the `which`th game in
/// the zip, renaming saves to this game's names when it is another game.
pub fn import(zip: &Path, which: usize, data_dir: &Path) -> Result<(), String> {
    let (zip, folder) = (c_path(zip)?, c_path(data_dir)?);
    let mut error = [0 as c_char; ERROR_SIZE];
    let result = unsafe { rib_game_data_import(zip.as_ptr(), which, folder.as_ptr(), error.as_mut_ptr(), ERROR_SIZE) };
    if result == 0 {
        Ok(())
    } else {
        Err(sentence(&error, "We could not import the zip."))
    }
}

#[cfg(test)]
mod tests;
