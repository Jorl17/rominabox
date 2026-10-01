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

/// Serve one body and count the requests for it.
struct Serves {
    body: Vec<u8>,
    asked: Cell<usize>,
}

impl Transport for Serves {
    fn get(&self, _url: &str) -> Result<Response, ()> {
        self.asked.set(self.asked.get() + 1);
        Ok(Response { body: self.body.clone(), version: Version::default() })
    }
    fn head(&self, _url: &str) -> Result<Version, ()> {
        Err(())
    }
}

fn nothing() -> Serves {
    Serves { body: Vec::new(), asked: Cell::new(0) }
}

fn windows_manifest() -> Vec<u8> {
    serde_json::json!({
        "platform": "windows",
        "components": [{"name": "RetroArch", "revision": PLAYER}],
    })
    .to_string()
    .into_bytes()
}

fn pin(url: &str, body: &[u8]) -> Pin {
    Pin {
        platform: "windows".into(),
        player: PLAYER.into(),
        url: url.into(),
        sha256: format!("{:x}", Sha256::digest(body)),
    }
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
/// with a sentence about where to put a kit.
#[test]
fn a_kit_for_another_player_or_none_is_refused_saying_where_one_goes() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-refused");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let store = root.join("store");
    let place = folder(&store, &ExportTarget::Windows, PLAYER);
    let missing = for_export(&ExportTarget::Windows, &bundled, &store, &nothing()).unwrap_err();
    assert!(missing.contains("Games for Windows"), "{missing}");
    assert!(missing.contains(&place.display().to_string()), "{missing}");
    kit(&place, "windows", "0000000000000000000000000000000000000000");
    let other = for_export(&ExportTarget::Windows, &bundled, &store, &nothing()).unwrap_err();
    assert!(other.contains("carries player 0000") && other.contains(PLAYER), "{other}");
}

/// We download a published kit once, check it against its pin and unpack it
/// into the store. We refuse one that is not the pinned archive or that
/// lists a file outside itself, and then leave nothing where we look for a
/// kit.
#[test]
fn a_pinned_kit_is_fetched_once_checked_and_unpacked_safely() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-fetch");
    let store = root.join("store");
    let place = folder(&store, &ExportTarget::Windows, PLAYER);
    let body = archive(&[("manifest.json", &windows_manifest()), ("bin/retroarch.exe", b"MZ")]);
    let transport = Serves { body: body.clone(), asked: Cell::new(0) };
    fetch(&pin("https://kits.example/windows.zip", &body), &place, &transport).unwrap();
    assert_eq!(identity(&place).unwrap().platform, "windows");
    assert_eq!(fs::read(place.join("bin/retroarch.exe")).unwrap(), b"MZ");
    assert_eq!(transport.asked.get(), 1);

    let elsewhere = folder(&root.join("other-store"), &ExportTarget::Windows, PLAYER);
    let tampered = Serves { body: archive(&[("manifest.json", b"{}")]), asked: Cell::new(0) };
    let error = fetch(&pin("https://kits.example/windows.zip", &body), &elsewhere, &tampered).unwrap_err();
    assert!(error.contains("not the one pinned"), "{error}");
    assert!(!elsewhere.exists());

    let escaping = archive(&[("../escaped.txt", b"x")]);
    let error = fetch(
        &pin("https://kits.example/windows.zip", &escaping),
        &elsewhere,
        &Serves { body: escaping.clone(), asked: Cell::new(0) },
    )
    .unwrap_err();
    assert!(error.contains("outside itself"), "{error}");
    assert!(!elsewhere.exists() && !root.join("other-store/escaped.txt").exists());
}

/// With a kit published for this player, we download it into the store for
/// the first Windows game and use it for the next without asking again.
#[test]
fn a_published_kit_is_downloaded_the_first_time_and_kept() {
    let root = rominabox_scratch::Scratch::dir("rominabox-kits-published");
    let bundled = kit(&root.join("bundled"), "macos", PLAYER);
    let store = root.join("store");
    let body = archive(&[("manifest.json", &windows_manifest())]);
    let pins = [pin("https://kits.example/windows.zip", &body)];
    let transport = Serves { body, asked: Cell::new(0) };
    let first = resolve(&ExportTarget::Windows, &bundled, &store, &pins, &transport).unwrap();
    let second = resolve(&ExportTarget::Windows, &bundled, &store, &pins, &transport).unwrap();
    assert_eq!(first, folder(&store, &ExportTarget::Windows, PLAYER));
    assert_eq!(second, first);
    assert_eq!(transport.asked.get(), 1, "the kit was downloaded again");
}

/// We parse every pin we ship with the builder, and each contains a platform.
#[test]
fn the_shipped_pins_parse() {
    for pin in pins() {
        assert!(["macos", "windows"].contains(&pin.platform.as_str()), "{}", pin.platform);
    }
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
