//! What we decide for a game when its author does not: the settings a dropped
//! game starts with, where we keep downloads and put games, and, for a request
//! with less than the builder's draft, the rest of the game, which we fill in
//! as when someone drops the file into the builder.
//!
//! In the command line we run `export` through `complete_export` and `export`,
//! so with `rominabox-cli export game.md` we make the same game as when
//! someone drops game.md into the builder and creates the app. We run
//! `project-save` through `complete_game`, and `preview` through
//! `complete_preview`.

use crate::export_error::{ErrorStage, ExportError};
use crate::game::Game;
use crate::metadata::Inspection;
use crate::packaging::{ExportProgress, ExportRequest, ExportResult, ExportTarget};
use crate::target::Target;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
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
    /// Every connected pad is for player 1.
    pub every_pad_is_player_one: bool,
    pub advanced_emulator_access: bool,
    /// A Mac game also runs on Intel Macs, because its player, core and
    /// launcher contain Intel code beside the Apple silicon code.
    pub intel_macs: bool,
    pub theme: String,
    pub palette: String,
    /// Draw the author's background picture in the palette's screen colour, so
    /// the menu text is readable on any picture. Off, we show it as it is.
    pub tint_background: bool,
    pub menu_sounds: String,
    /// Whether the game has fast forward, with its hotkey on HOTKEYS and its
    /// speed in Options.
    pub fast_forward: bool,
    /// The fast forward speed until the player changes it, as the RetroArch
    /// `fastforward_ratio`, one of the positions declared in `settings.inc`.
    pub fast_forward_speed: f32,
    /// Whether fast forward runs only while its hotkey is held, or from one
    /// press to the next. The player can change it.
    pub fast_forward_hold: bool,
    /// The hotkeys, until the player changes them on HOTKEYS.
    #[serde(deserialize_with = "crate::hotkeys::read_defaults")]
    pub hotkeys: crate::hotkeys::Hotkeys,
}

pub fn defaults() -> &'static Defaults {
    static DECLARED: OnceLock<Defaults> = OnceLock::new();
    DECLARED.get_or_init(|| {
        serde_json::from_str(include_str!("../../../defaults.json"))
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
    pub fn fast_forward() -> bool {
        defaults().fast_forward
    }
    pub fn fast_forward_speed() -> f32 {
        defaults().fast_forward_speed
    }
    pub fn fast_forward_hold() -> bool {
        defaults().fast_forward_hold
    }
    pub fn every_pad_is_player_one() -> bool {
        defaults().every_pad_is_player_one
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
    pub fn tint_background() -> bool {
        defaults().tint_background
    }
    pub fn menu_sounds() -> String {
        defaults().menu_sounds.clone()
    }
    pub fn hotkeys() -> crate::hotkeys::Hotkeys {
        defaults().hotkeys.clone()
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

    /// Runtime kits for the other platform, beside the downloaded cores: a
    /// game for Windows made on a Mac, or for a Mac on Windows (`kits`).
    pub fn kit_store(&self) -> Result<PathBuf, String> {
        let data = dirs::data_local_dir().ok_or("this machine has no local data folder for kits")?;
        Ok(data.join(&self.identifier).join("kits"))
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
    let config: Value = serde_json::from_str(include_str!("../../../src-tauri/tauri.conf.json"))
        .expect("tauri.conf.json is JSON");
    config["identifier"]
        .as_str()
        .expect("tauri.conf.json names the builder's identifier")
        .to_string()
}

/// Where we find the builder's resources for the command line: beside the
/// command in a built builder, where `bin/` is beside `runtime/` and
/// `preview/`, else in this checkout, from `scripts/build_kit.py`.
fn resource_folders() -> impl Iterator<Item = PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|program| Some(program.parent()?.parent()?.to_path_buf()));
    beside
        .into_iter()
        .chain([crate::repo::builder_resources()])
}

/// One of the builder's resources, by its path inside them, for the command
/// line. In the builder we find them through Tauri.
pub fn resource(relative: &str) -> Result<PathBuf, String> {
    resource_folders()
        .map(|folder| folder.join(relative))
        .find(|path| path.exists())
        .ok_or_else(|| format!("no {relative} is beside this command or in this checkout"))
}

/// The runtime kit inside the builder, for the command line.
pub fn runtime_kit() -> Option<PathBuf> {
    resource_folders()
        .map(|folder| folder.join("runtime"))
        .find(|kit| kit.join("manifest.json").is_file())
}

/// A request completed as in the builder, and what we found for it in the
/// builder's lookup, as we print it in `inspect`. It is `None` for a request
/// that named its title and console, because we do not look such a request up.
pub struct Completed<T> {
    pub request: T,
    pub identified: Option<Inspection>,
}

/// `request` completed as in the builder for a dropped game.
///
/// We complete the game as in `complete_game`. Where the game goes, and the
/// kit and core cache we make it from, are the builder's. For every setting
/// a request leaves out, we use the builder's default, through the serde
/// defaults of `ExportRequest`. We keep what the request states, `null`
/// included. `"icon": null` is a game without a cover, and with
/// `"coreCache": null` we take the core from the kit alone.
pub fn complete_export(mut request: Map<String, Value>) -> Result<Completed<ExportRequest>, String> {
    let places = Places::of(identifier());
    let (identified, target) = fill_game(&mut request, &places)?;
    if !request.contains_key("outputDir") {
        request.insert("outputDir".into(), json!(destination()?));
    }
    if !request.contains_key("runtimeKit") {
        let own = runtime_kit().ok_or(
            "no runtime kit is beside this command or in this checkout; name one as runtimeKit",
        )?;
        request.insert("runtimeKit".into(), json!(kit_for(&target, &own, &places)?));
    }
    if !request.contains_key("coreCache") {
        request.insert("coreCache".into(), json!(places.core_cache(target.target())?));
    }
    let request = serde_json::from_value(Value::Object(request))
        .map_err(|error| format!("invalid export request: {error}"))?;
    Ok(Completed { request, identified })
}

/// `game` completed as in the builder for a dropped game: the game we save
/// in `project-save` for a request with only its file.
///
/// For a game without its title or console, we use the builder's own
/// identification to fill in the title, console and cover it leaves out. We
/// do not look up a game with both, and its cover is the one it states, or
/// none. A lookup would also check the console against the file, as when
/// someone chooses a console in the builder, and we would reject games we
/// can export. The platform is the one the command runs on.
///
/// Two fields are for the lookup and not part of the game: `online`, and
/// `metadataCache`, where we cache lookups. Both default to the builder's.
pub fn complete_game(mut game: Map<String, Value>) -> Result<Completed<Game>, String> {
    let (identified, _) = fill_game(&mut game, &Places::of(identifier()))?;
    let request = serde_json::from_value(Value::Object(game))
        .map_err(|error| format!("invalid game: {error}"))?;
    Ok(Completed { request, identified })
}

/// `request` with the game's fields that we fill in `complete_game`, the
/// lookup when there was one, and the platform the game is for.
fn fill_game(
    request: &mut Map<String, Value>,
    places: &Places,
) -> Result<(Option<Inspection>, ExportTarget), String> {
    let rom: PathBuf = match request.get("rom") {
        Some(Value::String(rom)) => rom.into(),
        _ => return Err("a request names its game as rom".into()),
    };
    // The file that someone means by a drop: the sheet that lists a dropped
    // track, the game in a dropped folder, or the game a dropped patch is
    // for, which we then include with the patch.
    let stated_files: crate::content::GameFiles = match request.get("files") {
        Some(files) => serde_json::from_value(files.clone()).map_err(|error| format!("files: {error}"))?,
        None => Default::default(),
    };
    let (rom, files) = crate::content::dropped_game(&rom, &stated_files)?;
    request.insert("rom".into(), json!(rom));
    request.insert("files".into(), json!(files));
    let online = match request.remove("online") {
        None => defaults().online,
        Some(Value::Bool(online)) => online,
        Some(other) => return Err(format!("online is true or false, not {other}")),
    };
    let metadata_cache = request.remove("metadataCache");

    let mut identified = None;
    if !request.contains_key("title") || !request.contains_key("system") {
        let cache = match metadata_cache {
            Some(Value::String(cache)) => PathBuf::from(cache),
            Some(other) => return Err(format!("metadataCache is a folder, not {other}")),
            None => places.metadata_cache()?,
        };
        let stated = request.get("system").and_then(Value::as_str);
        let found = crate::metadata::inspect_game_with_files(&rom, &cache, online, stated, &files)
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
            ("icon", json!(found.icon_path)),
        ] {
            request.entry(field).or_insert(value);
        }
        identified = Some(found);
    }
    // We give each hotkey missing from the request the default in the builder
    // for its console.
    crate::hotkeys::complete(request)?;

    let target: ExportTarget = match request.get("target") {
        Some(stated) => serde_json::from_value(stated.clone())
            .map_err(|error| format!("invalid target: {error}"))?,
        None => {
            let host = ExportTarget::of_host()
                .ok_or("the builder does not make games on this machine; name a target")?;
            request.insert("target".into(), json!(host));
            host
        }
    };
    Ok((identified, target))
}

/// The kit we make a game for `platform` from. For the builder's platform it
/// is `own`, the builder's kit. For the other platform it is that platform's
/// kit, which we download to the builder's kit store at first use (`kits`).
pub fn kit_for(platform: &ExportTarget, own: &Path, places: &Places) -> Result<PathBuf, String> {
    crate::kits::for_export(platform, own, &places.kit_store()?, &crate::cores::UreqTransport)
}

/// Make the game `request` describes, as with Create app in the builder: from
/// the request's kit, or, for both platforms, each platform's game from its
/// own kit (`kit_for`, beside `own_kit`) and core cache, in one zip.
pub fn export(
    request: &ExportRequest,
    own_kit: Option<&Path>,
    places: &Places,
    cancelled: &AtomicBool,
    report: impl FnMut(ExportProgress),
) -> Result<ExportResult, ExportError> {
    if !request.game.both_platforms {
        return crate::packaging::export_game(request, cancelled, report);
    }
    let own_kit = own_kit.ok_or_else(|| {
        ExportError::new(
            ErrorStage::Refused,
            "no runtime kit is beside this command or in this checkout",
        )
    })?;
    let kit_for = |platform: &ExportTarget| kit_for(platform, own_kit, places);
    let core_cache_for = |target| places.core_cache(target).ok();
    crate::packaging::export_for_both(request, &kit_for, &core_cache_for, cancelled, report)
}

/// The size at which we draw the preview in the builder's Menu step.
const PREVIEW_SIZE: (u32, u32) = (960, 600);

/// `request` completed as for the preview in the builder's Menu step: the
/// design in `theme`, else the builder's, from the kit's designs, the
/// builder's palette, the kit's controller artwork, the builder's preview
/// renderer, and 960 by 600. With `resource` we find one of the builder's
/// resources by its path inside them. What the request states comes first,
/// so with `design`, a design's folder, we draw a design the kit lacks.
pub fn complete_preview(
    mut request: Map<String, Value>,
    resource: &dyn Fn(&str) -> Result<PathBuf, String>,
) -> Result<crate::menu::PreviewRequest, String> {
    let theme = match request.remove("theme") {
        None | Some(Value::Null) => unstated::theme(),
        Some(Value::String(theme)) => theme,
        Some(other) => return Err(format!("theme is a design's id, not {other}")),
    };
    let renderer = || -> Result<PathBuf, String> {
        let host = Target::host().ok_or("this machine is not one the builder builds for")?;
        resource(&crate::packaging::preview_renderer(host)?)
    };
    let unstated: [(&str, &dyn Fn() -> Result<Value, String>); 6] = [
        ("design", &|| Ok(json!(resource(&format!("runtime/designs/{theme}"))?))),
        ("assets", &|| Ok(json!(resource("runtime/menu-assets")?))),
        ("renderer", &|| Ok(json!(renderer()?))),
        ("palette", &|| Ok(json!(unstated::palette()))),
        ("width", &|| Ok(json!(PREVIEW_SIZE.0))),
        ("height", &|| Ok(json!(PREVIEW_SIZE.1))),
    ];
    for (field, value) in unstated {
        if !request.contains_key(field) {
            request.insert(field.into(), value()?);
        }
    }
    serde_json::from_value(Value::Object(request))
        .map_err(|error| format!("invalid preview request: {error}"))
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
