//! The runtime kit we make an export from.
//!
//! We bundle the kit for the builder's own platform with the builder. For a
//! game for the other platform we use that platform's kit in the kit store.
//! Someone copied it there by hand, or we downloaded it on first use from the
//! release of the builder's player commit (`release_url`). Its player must
//! be the one in the bundled kit, because we build both kits from one source
//! and the game's menu is part of the player.

use crate::cores::Transport;
use crate::packaging::ExportTarget;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Which platform a kit is for and which player it contains, from its
/// `manifest.json`.
#[derive(Debug, PartialEq, Eq)]
struct Identity {
    platform: String,
    player: String,
}

fn manifest(kit: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&fs::read(kit.join("manifest.json")).ok()?).ok()
}

/// The platform a kit is for. That is all we need from the bundled kit to
/// make games for the builder's own platform.
fn platform_of_kit(kit: &Path) -> Option<String> {
    Some(manifest(kit)?["platform"].as_str()?.to_string())
}

fn identity(kit: &Path) -> Option<Identity> {
    let manifest = manifest(kit)?;
    let player = manifest["components"]
        .as_array()?
        .iter()
        .find(|component| component["name"] == "RetroArch")?["revision"]
        .as_str()?
        .to_string();
    Some(Identity {
        platform: manifest["platform"].as_str()?.to_string(),
        player,
    })
}

/// The word for `platform` in a kit's manifest.
fn word(platform: &ExportTarget) -> &'static str {
    match platform {
        ExportTarget::Macos => "macos",
        ExportTarget::Windows => "windows",
    }
}

/// What a person calls `platform` in a sentence.
fn called(platform: &ExportTarget) -> &'static str {
    match platform {
        ExportTarget::Macos => "Mac",
        ExportTarget::Windows => "Windows",
    }
}

/// The first 12 characters of a player's commit, as in the names of its kits.
fn short(player: &str) -> String {
    player.chars().take(12).collect()
}

/// Where we keep `platform`'s kit for `player` in the store. We keep each
/// player's kit in a separate folder, so we never replace an older one.
fn folder(store: &Path, platform: &ExportTarget, player: &str) -> PathBuf {
    store.join(format!("{}-{}", word(platform), short(player)))
}

/// The URL of `platform`'s kit for `player`: the asset
/// `<platform>-<player>.zip` of the release `kit-<player>` in the repository,
/// where we upload it with scripts/publish_kit.py.
pub fn release_url(platform: &ExportTarget, player: &str) -> String {
    let short = short(player);
    format!(
        "{}/releases/download/kit-{short}/{}-{short}.zip",
        env!("CARGO_PKG_REPOSITORY"),
        word(platform)
    )
}

/// The kit we make a game for `platform` from: `bundled` for the builder's
/// own platform, otherwise the kit in the store, which we download if missing.
pub fn for_export(
    platform: &ExportTarget,
    bundled: &Path,
    store: &Path,
    transport: &dyn Transport,
) -> Result<PathBuf, String> {
    if platform_of_kit(bundled).as_deref() == Some(word(platform)) {
        return Ok(bundled.to_path_buf());
    }
    let own = identity(bundled)
        .ok_or_else(|| format!("{} is not a runtime kit: it has no manifest naming its player", bundled.display()))?;
    let wanted = Identity {
        platform: word(platform).to_string(),
        player: own.player.clone(),
    };
    let place = folder(store, platform, &own.player);
    match identity(&place) {
        Some(found) if found == wanted => return Ok(place),
        Some(found) => {
            return Err(format!(
                "The {} runtime kit in {} contains player {}, and this builder's player is {}. Copy the kit made with this version of ROM-in-a-Box there instead.",
                called(platform),
                place.display(),
                found.player,
                own.player
            ))
        }
        None => {}
    }
    let url = release_url(platform, &own.player);
    fetch(&url, &place, transport)
        .map_err(|_| format!("The {} runtime kit could not be downloaded. Try again later.", called(platform)))?;
    match identity(&place) {
        Some(found) if found == wanted => Ok(place),
        _ => Err(format!(
            "The {} runtime kit downloaded from {url} is not the one this builder needs.",
            called(platform)
        )),
    }
}

/// Download the archive at `url` and unpack it into `place` through a folder
/// beside it, so that after a failed download no partial kit is in `place`.
fn fetch(url: &str, place: &Path, transport: &dyn Transport) -> Result<(), String> {
    let body = transport
        .get(url)
        .map_err(|()| "It could not be downloaded.".to_string())?
        .body;
    let parent = place.parent().ok_or("the kit store has no folder")?;
    fs::create_dir_all(parent).map_err(|error| format!("Could not make {}: {error}", parent.display()))?;
    // A name unique to this download, never that of an interrupted one.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let unpacking = parent.join(format!(
        ".{}-unpacking-{}-{stamp}",
        place.file_name().and_then(|name| name.to_str()).unwrap_or("kit"),
        std::process::id()
    ));
    unpack(&body, &unpacking)?;
    crate::files::rename(&unpacking, place).map_err(|error| format!("Could not place the kit in {}: {error}", place.display()))
}

/// Write every entry of the zip `archive` under `destination`. An entry whose
/// name points outside `destination` is an error.
fn unpack(archive: &[u8], destination: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
        .map_err(|error| format!("The runtime kit is not a zip archive: {error}"))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("The runtime kit names a file outside itself: {}", entry.name()))?;
        let path = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
        fs::write(&path, bytes).map_err(|error| format!("Could not write {}: {error}", path.display()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o777)).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
