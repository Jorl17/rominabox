use super::*;
use crate::cores::{Response, Version};
use std::cell::Cell;
use std::io::Write;

const PLAYER: &str = "d2039639402d7069fcd866e40b80dd777efc86d3";

/// A kit folder whose manifest names `platform` and `player`.
fn kit(folder: &Path, platform: &str, player: &str) -> PathBuf {
    fs::create_dir_all(folder).unwrap();
    fs::write(
        folder.join("manifest.json"),
        serde_json::json!({
            "platform": platform,
            "components": [{"name": "RetroArch", "revision": player}],
        })
        .to_string(),
    )
    .unwrap();
    folder.to_path_buf()
}

/// A zip of `files`, as a published kit is.
fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Serve one body, or fail like a missing release when there is none, and
/// record each request.
struct Serves {
    body: Vec<u8>,
    asked: Cell<usize>,
    last: std::cell::RefCell<String>,
}

impl Transport for Serves {
    fn get(&self, url: &str) -> Result<Response, ()> {
        self.asked.set(self.asked.get() + 1);
        *self.last.borrow_mut() = url.to_string();
        if self.body.is_empty() {
            return Err(());
        }
        Ok(Response { body: self.body.clone(), version: Version::default() })
    }
    fn head(&self, _url: &str) -> Result<Version, ()> {
        Err(())
    }
}

fn serves(body: Vec<u8>) -> Serves {
    Serves { body, asked: Cell::new(0), last: std::cell::RefCell::new(String::new()) }
}

fn nothing() -> Serves {
    serves(Vec::new())
}

fn windows_manifest() -> Vec<u8> {
    serde_json::json!({
        "platform": "windows",
        "components": [{"name": "RetroArch", "revision": PLAYER}],
    })
    .to_string()
    .into_bytes()
}

/// We make a game for the builder's own platform from the bundled kit,
/// without looking for or fetching another.
#[test]
fn the_builders_own_platform_uses_its_own_kit() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-own");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let transport = nothing();
    let found = for_export(&ExportTarget::Macos, &bundled, &root.join("store"), &transport).unwrap();
    assert_eq!(found, bundled);
    assert_eq!(transport.asked.get(), 0);
}

/// We make a game for the other platform from the kit in the store for the
/// builder's player, when that kit is there.
#[test]
fn the_other_platform_uses_the_stores_kit_for_this_player() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-store");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let store = root.join("store");
    let placed = kit(&folder(&store, &ExportTarget::Windows, PLAYER), "windows", PLAYER);
    let transport = nothing();
    assert_eq!(for_export(&ExportTarget::Windows, &bundled, &store, &transport).unwrap(), placed);
    assert_eq!(transport.asked.get(), 0);
}

/// We refuse a kit for another player, or no kit and nothing published,
/// with a sentence about where to put a kit and where we looked for it.
#[test]
fn a_kit_for_another_player_or_none_is_refused_saying_where_one_goes() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-refused");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let store = root.join("store");
    let place = folder(&store, &ExportTarget::Windows, PLAYER);
    let missing = for_export(&ExportTarget::Windows, &bundled, &store, &nothing()).unwrap_err();
    assert!(missing.contains("Games for Windows"), "{missing}");
    assert!(missing.contains(&place.display().to_string()), "{missing}");
    assert!(missing.contains(&release_url(&ExportTarget::Windows, PLAYER)), "{missing}");
    kit(&place, "windows", "0000000000000000000000000000000000000000");
    let other = for_export(&ExportTarget::Windows, &bundled, &store, &nothing()).unwrap_err();
    assert!(other.contains("carries player 0000") && other.contains(PLAYER), "{other}");
}

/// We unpack a downloaded kit into the store. We refuse one that lists a
/// file outside itself, and then leave nothing where we look for a kit.
#[test]
fn a_downloaded_kit_is_unpacked_safely() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-fetch");
    let place = folder(&root.join("store"), &ExportTarget::Windows, PLAYER);
    let body = archive(&[("manifest.json", &windows_manifest()), ("bin/retroarch.exe", b"MZ")]);
    fetch("https://kits.example/windows.zip", &place, &serves(body)).unwrap();
    assert_eq!(identity(&place).unwrap().platform, "windows");
    assert_eq!(fs::read(place.join("bin/retroarch.exe")).unwrap(), b"MZ");

    let elsewhere = folder(&root.join("other-store"), &ExportTarget::Windows, PLAYER);
    let escaping = archive(&[("../escaped.txt", b"x")]);
    let error = fetch("https://kits.example/windows.zip", &elsewhere, &serves(escaping)).unwrap_err();
    assert!(error.contains("outside itself"), "{error}");
    assert!(!elsewhere.exists() && !root.join("other-store/escaped.txt").exists());
}

/// With a kit published for this player, we download it from the player's
/// release into the store for the first Windows game, and use it for the
/// next one without asking again.
#[test]
fn a_published_kit_is_downloaded_from_its_release_the_first_time_and_kept() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-published");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let store = root.join("store");
    let transport = serves(archive(&[("manifest.json", &windows_manifest())]));
    let first = for_export(&ExportTarget::Windows, &bundled, &store, &transport).unwrap();
    assert_eq!(
        *transport.last.borrow(),
        "https://github.com/Jorl17/rominabox/releases/download/kit-d2039639402d/windows-d2039639402d.zip"
    );
    let second = for_export(&ExportTarget::Windows, &bundled, &store, &transport).unwrap();
    assert_eq!(first, folder(&store, &ExportTarget::Windows, PLAYER));
    assert_eq!(second, first);
    assert_eq!(transport.asked.get(), 1, "the kit was downloaded again");
}

/// We refuse a release whose archive contains the kit of another player.
#[test]
fn a_downloaded_kit_for_another_player_is_refused() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-other-player");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let other = serde_json::json!({
        "platform": "windows",
        "components": [{"name": "RetroArch", "revision": "0000000000000000000000000000000000000000"}],
    })
    .to_string();
    let transport = serves(archive(&[("manifest.json", other.as_bytes())]));
    let error = for_export(&ExportTarget::Windows, &bundled, &root.join("store"), &transport).unwrap_err();
    assert!(error.contains("not the one this builder needs"), "{error}");
}

/// For the builder's own platform we require only that the kit is for that
/// platform. With a manifest that lists no player, we still make games for
/// the builder's own platform.
#[test]
fn the_builders_own_kit_needs_no_player_named() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-unnamed");
    let bundled = root.join("bundled");
    fs::create_dir_all(&bundled).unwrap();
    fs::write(bundled.join("manifest.json"), r#"{"platform":"windows","components":[]}"#).unwrap();
    let found = for_export(&ExportTarget::Windows, &bundled, &root.join("store"), &nothing()).unwrap();
    assert_eq!(found, bundled);
}
