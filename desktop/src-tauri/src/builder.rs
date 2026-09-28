//! The choices we make in the builder for a game when its author does not:
//! the settings for a dropped game, the folders for downloads and for games,
//! and, for a request with less than the builder's draft, the rest of the
//! game, filled in as when someone drops the file into the builder.
//!
//! We run the command line's `export` through `complete_export`, so
//! `rominabox-cli export game.md` makes the same game as dropping game.md
//! into the builder and creating the app.

use crate::packaging::{ExportRequest, ExportTarget};
use crate::target::Target;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::OnceLock;

/// The settings a dropped game starts with, before its author changes any.
///
/// We declare them once, in `desktop/defaults.json`. We make the builder's
/// first draft from that file, and for a setting an export request leaves
/// out, we read the value from the same file here (`unstated`).
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Defaults {
    /// Look the game up online: its catalogue entry and its cover. Without
    /// it, we use only what we cached in an earlier lookup. This is "Look up
    /// game details" in the builder.
    pub online: bool,
    pub show_menu: bool,
    pub start_at_menu: bool,
    pub include_achievements: bool,
    pub splash: bool,
    pub keep_playing_in_background: bool,
    pub autosave_on_quit: bool,
    pub advanced_emulator_access: bool,
    /// A Mac game also runs on Intel Macs, because its player, core and
    /// launcher contain Intel code beside the Apple silicon code.
    pub intel_macs: bool,
    pub theme: String,
    pub palette: String,
    pub menu_sounds: String,
    /// The inputs to open the menu, and to confirm and go back in it, until
    /// the player changes them on MENU CONTROLS.
    #[serde(deserialize_with = "crate::menu_controls::read_defaults")]
    pub menu_controls: crate::menu_controls::MenuControls,
}

pub fn defaults() -> &'static Defaults {
    static DECLARED: OnceLock<Defaults> = OnceLock::new();
    DECLARED.get_or_init(|| {
        serde_json::from_str(include_str!("../../defaults.json"))
            .expect("desktop/defaults.json declares each default once and nothing else")
    })
}

/// Each default as a function, because serde's `default = "…"` names a
/// function rather than a value.
pub mod unstated {
    use super::defaults;

    pub fn online() -> bool {
        defaults().online
    }
    pub fn show_menu() -> bool {
        defaults().show_menu
    }
    pub fn start_at_menu() -> bool {
        defaults().start_at_menu
    }
    pub fn include_achievements() -> bool {
        defaults().include_achievements
    }
    pub fn splash() -> bool {
        defaults().splash
    }
    pub fn keep_playing_in_background() -> bool {
        defaults().keep_playing_in_background
    }
    pub fn autosave_on_quit() -> bool {
        defaults().autosave_on_quit
    }
    pub fn advanced_emulator_access() -> bool {
        defaults().advanced_emulator_access
    }
    pub fn intel_macs() -> bool {
        defaults().intel_macs
    }
    pub fn theme() -> String {
        defaults().theme.clone()
    }
    pub fn palette() -> String {
        defaults().palette.clone()
    }
    pub fn menu_sounds() -> String {
        defaults().menu_sounds.clone()
    }
    pub fn menu_controls() -> crate::menu_controls::MenuControls {
        defaults().menu_controls.clone()
    }
}

/// Where we keep the downloads of the builder named `identifier`. We pass the
/// builder's own identifier in the builder, and `identifier()` in the command
/// line, which has no configuration. So both use the same folders.
///
/// These are the folders of Tauri's `app_cache_dir` and `app_local_data_dir`,
/// which also come from `dirs` in Tauri.
pub struct Places {
    identifier: String,
}

impl Places {
    pub fn of(identifier: impl Into<String>) -> Places {
        Places {
            identifier: identifier.into(),
        }
    }

    /// The per-user cache folder, for catalogues and covers from lookups.
    pub fn metadata_cache(&self) -> Result<PathBuf, String> {
        let cache = dirs::cache_dir().ok_or("this machine has no cache folder for lookups")?;
        Ok(cache.join(&self.identifier).join("metadata"))
    }

    /// Downloaded cores, a cache for this machine only: the per-user local
    /// data folder. On Windows it is Local AppData, not the Roaming folder
    /// that moves with a profile between machines, and on macOS it is
    /// Application Support.
    pub fn core_cache(&self, target: Target) -> Result<PathBuf, String> {
        let data = dirs::data_local_dir().ok_or("this machine has no local data folder for cores")?;
        Ok(crate::export_cores::cache_folder(
            &data.join(&self.identifier).join("core-cache"),
            target,
        ))
    }
}

/// Where a game is written unless its author says otherwise.
pub fn destination() -> Result<PathBuf, String> {
    let downloads = dirs::download_dir().ok_or("this machine has no Downloads folder")?;
    Ok(downloads.join("ROM-in-a-Box"))
}

/// The builder's identifier, for a program that is not the builder: the one
/// we give a worktree's builder in `scripts/tauri.mjs`, else the one in
/// `tauri.conf.json`.
pub fn identifier() -> String {
    if let Ok(named) = std::env::var("ROMINABOX_BUNDLE_ID") {
        if !named.trim().is_empty() {
            return named.trim().to_string();
        }
    }
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json"))
        .expect("tauri.conf.json is JSON");
    config["identifier"]
        .as_str()
        .expect("tauri.conf.json names the builder's identifier")
        .to_string()
}

/// The runtime kit in the builder, for the command line. In a built builder
/// it is beside the command, where `bin/` and `runtime/` are both resources.
/// Otherwise it is this checkout's, written by `scripts/build_kit.py`.
pub fn runtime_kit() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|program| Some(program.parent()?.parent()?.join("runtime")));
    beside
        .into_iter()
        .chain([crate::repo::at("desktop/src-tauri/resources/runtime")])
        .find(|kit| kit.join("manifest.json").is_file())
}

/// `request`, completed as we complete a dropped game in the builder.
///
/// When a request leaves out the game's title or console, we identify it with
/// the builder's own identification, and take the title, console, description
/// and cover that it leaves out from that. We treat a request with both as
/// already identified and do not look it up, so its description and cover are
/// what it states. A lookup would also check the console against the file, as
/// when someone chooses a console in the builder, and would reject games that
/// we accept in the export.
///
/// The game's folder, its platform, and the kit and core cache we make it
/// from are the builder's. For every setting a request leaves out we use the
/// builder's default, through `ExportRequest`'s serde defaults. We keep what
/// it states, `null` included: `"icon": null` is a game without a cover, and
/// with `"coreCache": null` we take the core from the kit alone.
///
/// Two fields control the lookup and are not part of the game: `online`, and
/// `metadataCache`, where we cache lookups. Both default to the builder's.
pub fn complete_export(mut request: Map<String, Value>) -> Result<ExportRequest, String> {
    let rom: PathBuf = match request.get("rom") {
        Some(Value::String(rom)) => rom.into(),
        _ => return Err("an export request names its game as rom".into()),
    };
    // The file that someone means by a drop: the sheet that lists a dropped
    // track, or the game in a dropped folder.
    let rom = crate::content::resolve_dropped(&rom)?;
    request.insert("rom".into(), json!(rom));
    let online = match request.remove("online") {
        None => defaults().online,
        Some(Value::Bool(online)) => online,
        Some(other) => return Err(format!("online is true or false, not {other}")),
    };
    let metadata_cache = request.remove("metadataCache");
    let places = Places::of(identifier());

    if !request.contains_key("title") || !request.contains_key("system") {
        let cache = match metadata_cache {
            Some(Value::String(cache)) => PathBuf::from(cache),
            Some(other) => return Err(format!("metadataCache is a folder, not {other}")),
            None => places.metadata_cache()?,
        };
        let stated = request.get("system").and_then(Value::as_str);
        let found = crate::metadata::inspect_game_with_system(&rom, &cache, online, stated)
            .map_err(|error| error.to_string())?;
        if found.system.is_empty() && stated.is_none() {
            return Err(format!(
                "{} was not recognised as a game for any console. Name its console as system.",
                found.filename
            ));
        }
        for (field, value) in [
            ("title", json!(found.title)),
            ("system", json!(found.system)),
            ("description", json!(found.description)),
            ("icon", json!(found.icon_path)),
        ] {
            request.entry(field).or_insert(value);
        }
    }

    let target: ExportTarget = match request.get("target") {
        Some(stated) => serde_json::from_value(stated.clone())
            .map_err(|error| format!("invalid export request: target: {error}"))?,
        None => {
            let host = ExportTarget::of_host()
                .ok_or("the builder does not make games on this machine; name a target")?;
            request.insert("target".into(), json!(host));
            host
        }
    };
    if !request.contains_key("outputDir") {
        request.insert("outputDir".into(), json!(destination()?));
    }
    if !request.contains_key("runtimeKit") {
        let kit = runtime_kit().ok_or(
            "no runtime kit is beside this command or in this checkout; name one as runtimeKit",
        )?;
        request.insert("runtimeKit".into(), json!(kit));
    }
    if !request.contains_key("coreCache") {
        if let Some(platform) = target.target() {
            request.insert("coreCache".into(), json!(places.core_cache(platform)?));
        }
    }
    serde_json::from_value(Value::Object(request))
        .map_err(|error| format!("invalid export request: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declaration names a design, a palette and a sound pack that exist.
    #[test]
    fn the_defaults_name_what_the_registry_declares() {
        let declared = defaults();
        let registry = crate::themes::registry().unwrap();
        assert!(
            registry.designs.iter().any(|design| design.id == declared.theme),
            "{}",
            declared.theme
        );
        assert!(
            registry.palettes.iter().any(|palette| palette.id == declared.palette),
            "{}",
            declared.palette
        );
        assert!(
            registry
                .sound_packs
                .iter()
                .any(|pack| pack.id == declared.menu_sounds),
            "{}",
            declared.menu_sounds
        );
    }
}
