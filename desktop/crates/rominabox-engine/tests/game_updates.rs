//! Updates of a game on this computer, on games we export from a stand-in kit
//! and install in a library of our own.

mod export_fixture;
mod support;

use export_fixture::{export_request_from, unpack, windows_kit, workspace};
use rominabox_engine::game_data::{write_manifest, Game};
use rominabox_engine::game_library::{Layout, Library};
use rominabox_engine::packaging::ExportTarget;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Every file under `folder`, by its path under it, with its bytes.
fn files(folder: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut waiting = vec![folder.to_path_buf()];
    while let Some(directory) = waiting.pop() {
        for entry in fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                waiting.push(path);
            } else {
                found.insert(path.strip_prefix(folder).unwrap().to_path_buf(), fs::read(&path).unwrap());
            }
        }
    }
    found
}

/// The game at `app`, installed in `library` as its launcher records it on
/// its first launch, with a save that says `save`.
fn install(library: &Library, app: &Path, identity: &str, save: &str) {
    let data = library.data_dir(identity);
    fs::create_dir_all(data.join("saves")).unwrap();
    write_manifest(
        &data,
        &Game {
            identity: identity.into(),
            title: "Hotkey Isolation".into(),
            system: "megadrive".into(),
            console: "Mega Drive / Genesis".into(),
            content: "sonic".into(),
            app: app.to_string_lossy().into_owned(),
            made_with: "0.4.3".into(),
            player_files: Vec::new(),
        },
    )
    .unwrap();
    fs::write(data.join("saves/sonic.srm"), save).unwrap();
}

/// A Windows game contains its recipe. When we build it again from that
/// recipe with the same kit, we make the same game, file for file, with the
/// same identity, and its saves stay.
#[test]
fn a_windows_game_built_again_from_its_recipe_is_the_same_game() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.game.target = ExportTarget::Windows;
    let made = rominabox_engine::packaging::export_game(&request, &AtomicBool::new(false), |_| {}).unwrap();
    let before = root.join("before");
    let runtime_before = unpack(&made.app_path, &before);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(before.join("Resources/game.json")).unwrap()).unwrap();
    let identity = manifest["identity"].as_str().unwrap().to_string();
    assert!(before.join("Resources/Recipe/recipe.json").is_file(), "the game has no recipe");
    let library = Library::at(root.join("data"), Layout::Windows);
    install(&library, &made.app_path, &identity, "rings");

    library.update_engine(&identity, &request.runtime_kit).unwrap();

    let after = root.join("after");
    let runtime_after = unpack(&made.app_path, &after);
    assert_eq!(runtime_after, runtime_before);
    let (old, new) = (files(&before), files(&after));
    let differing: Vec<_> = old
        .keys()
        .chain(new.keys())
        .filter(|path| old.get(*path) != new.get(*path))
        .collect();
    assert!(differing.is_empty(), "the game built again differs in {differing:?}");
    assert_eq!(fs::read_to_string(library.data_dir(&identity).join("saves/sonic.srm")).unwrap(), "rings");
}

/// With a newer engine in the kit, the game built again from its recipe has
/// the kit's new files, and the same content, settings, identity and saves.
#[test]
fn a_windows_game_built_again_with_a_newer_engine_has_the_kits_new_files() {
    let root = workspace();
    let mut request = export_request_from(&root, windows_kit(&root));
    request.game.target = ExportTarget::Windows;
    let made = rominabox_engine::packaging::export_game(&request, &AtomicBool::new(false), |_| {}).unwrap();
    let before = root.join("before");
    unpack(&made.app_path, &before);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(before.join("Resources/game.json")).unwrap()).unwrap();
    let identity = manifest["identity"].as_str().unwrap().to_string();
    let library = Library::at(root.join("data"), Layout::Windows);
    install(&library, &made.app_path, &identity, "rings");
    let profiles: Vec<PathBuf> = files(&request.runtime_kit)
        .into_keys()
        .filter(|path| path.file_name().is_some_and(|name| name == "pad.cfg"))
        .map(|path| request.runtime_kit.join(path))
        .collect();
    assert!(!profiles.is_empty(), "the kit has no controller profile");
    for profile in &profiles {
        fs::write(profile, "input_device = \"a newer engine\"
").unwrap();
    }

    library.update_engine(&identity, &request.runtime_kit).unwrap();

    let after = root.join("after");
    unpack(&made.app_path, &after);
    for driver in ["xinput", "dinput"] {
        let profile = after.join("Resources/autoconfig").join(driver).join("pad.cfg");
        assert_eq!(fs::read_to_string(&profile).unwrap(), "input_device = \"a newer engine\"
", "{driver}");
    }
    for kept in ["Resources/game.json", "Resources/content/sonic.bin", "Resources/game-core.dll", "Resources/Recipe/recipe.json"] {
        assert_eq!(fs::read(after.join(kept)).unwrap(), fs::read(before.join(kept)).unwrap(), "{kept}");
    }
    assert!(library.games().iter().any(|game| game.game.identity == identity && game.app_present));
    assert_eq!(fs::read_to_string(library.data_dir(&identity).join("saves/sonic.srm")).unwrap(), "rings");
}
