//! The runtime kit we make an export from.
//!
//! We bundle the kit for the builder's own platform with the builder. For a
//! game for the other platform we use that platform's kit in the kit store.
//! Someone placed it there by hand, or we fetched it on first use from the
//! archive listed in `desktop/kits.json` for the builder's player and checked
//! its SHA-256. Its player must be the one in the bundled kit, because we
//! build both kits from one source and the game's menu is part of the player.

use crate::cores::Transport;
use crate::packaging::ExportTarget;
use serde::Deserialize;
use sha2::{Digest, Sha256};
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

fn identity(kit: &Path) -> Option<Identity> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(kit.join("manifest.json")).ok()?).ok()?;
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

/// A kit published for download: the archive of one platform's kit for one
/// player.
#[derive(Deserialize)]
struct Pin {
    platform: String,
    player: String,
    url: String,
    sha256: String,
}

#[derive(Deserialize)]
struct Pins {
    kits: Vec<Pin>,
}

fn pins() -> Vec<Pin> {
    serde_json::from_str::<Pins>(include_str!("../../kits.json"))
        .expect("desktop/kits.json lists kits")
        .kits
}

/// Where we keep `platform`'s kit for `player` in the store. We keep each
/// player's kit in a separate folder, so we never replace an older one.
fn folder(store: &Path, platform: &ExportTarget, player: &str) -> PathBuf {
    let short: String = player.chars().take(12).collect();
    store.join(format!("{}-{short}", word(platform)))
}

/// The kit we make a game for `platform` from: `bundled` for the builder's
/// own platform, otherwise the kit in the store, which we fetch when missing.
pub fn for_export(
    platform: &ExportTarget,
    bundled: &Path,
    store: &Path,
    transport: &dyn Transport,
) -> Result<PathBuf, String> {
    resolve(platform, bundled, store, &pins(), transport)
}

fn resolve(
    platform: &ExportTarget,
    bundled: &Path,
    store: &Path,
    pins: &[Pin],
    transport: &dyn Transport,
) -> Result<PathBuf, String> {
    let own = identity(bundled)
        .ok_or_else(|| format!("{} is not a runtime kit: it has no manifest naming its player", bundled.display()))?;
    if own.platform == word(platform) {
        return Ok(bundled.to_path_buf());
    }
    let wanted = Identity {
        platform: word(platform).to_string(),
        player: own.player.clone(),
    };
    let place = folder(store, platform, &own.player);
    match identity(&place) {
        Some(found) if found == wanted => return Ok(place),
        Some(found) => {
            return Err(format!(
                "The {} runtime kit in {} carries player {}, and this builder's is {}. Put the kit made with this version of ROM-in-a-Box there instead.",
                called(platform),
                place.display(),
                found.player,
                own.player
            ))
        }
        None => {}
    }
    let Some(pin) = pins
        .iter()
        .find(|pin| pin.platform == wanted.platform && pin.player == wanted.player)
    else {
        return Err(format!(
            "Games for {} are made with ROM-in-a-Box's {} player, which this builder downloads the first time it is needed, and it is not available to download yet. To make one now, put the {} runtime kit from this version of ROM-in-a-Box in {}.",
            called(platform),
            called(platform),
            called(platform),
            place.display()
        ));
    };
    fetch(pin, &place, transport)?;
    match identity(&place) {
        Some(found) if found == wanted => Ok(place),
        _ => Err(format!(
            "The {} runtime kit downloaded from {} is not the one this builder needs.",
            called(platform),
            pin.url
        )),
    }
}

/// Download the archive in `pin`, check that it is the pinned one, and unpack
/// it into `place` through a folder beside it, so that after a failed download
/// no partial kit is in `place`.
fn fetch(pin: &Pin, place: &Path, transport: &dyn Transport) -> Result<(), String> {
    let body = transport
        .get(&pin.url)
        .map_err(|()| format!("Could not download the runtime kit from {}.", pin.url))?
        .body;
    let digest = format!("{:x}", Sha256::digest(&body));
    if digest != pin.sha256 {
        return Err(format!(
            "The runtime kit downloaded from {} is not the one pinned for this builder.",
            pin.url
        ));
    }
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
    fs::rename(&unpacking, place).map_err(|error| format!("Could not place the kit in {}: {error}", place.display()))
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
