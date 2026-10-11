//! Updates of a Mac game. The player in the stand-in kit is a program that
//! does nothing, and each core is a small library from `cc` in the core
//! cache. We use no network: we answer downloads from no server, and the
//! cache holds the newest core.

use super::export_fixture::{export_request, mac_library};
use super::{files, install, workspace};
use rominabox_engine::cores::{Response, Transport, Version};
use rominabox_engine::game_library::{CoreUpdate, Layout, Library};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

const CORE: &str = "genesis_plus_gx_libretro.dylib";
const LICENCE: &str = "genesis_plus_gx.txt";

/// A network with no server.
struct Offline;

impl Transport for Offline {
    fn get(&self, _url: &str) -> Result<Response, ()> {
        Err(())
    }

    fn head(&self, _url: &str) -> Result<Version, ()> {
        Err(())
    }
}

/// The builder's core cache for this Mac.
fn cache(root: &Path) -> PathBuf {
    root.join("core-cache").join(rominabox_engine::target::Target::host().unwrap().key())
}

/// Put in the cache a Mega Drive core whose code contains `mark`, linked
/// with `flags`, and the licence text `licence`.
fn cache_core(root: &Path, mark: &str, flags: &[&str], licence: &str) {
    let cache = cache(root);
    let source = format!("const char *mark = \"{mark}\";\nunsigned retro_api_version(void) {{ return 1; }}\n");
    mac_library(&cache.join("cores").join(CORE), &source, flags);
    fs::create_dir_all(cache.join("licenses")).unwrap();
    fs::write(cache.join("licenses").join(LICENCE), licence).unwrap();
}

/// A Mac game exported with the core in the cache, installed in a library
/// with a save, and its identity.
fn installed_game(root: &Path) -> (Library, PathBuf, String, PathBuf) {
    let mut request = export_request(root);
    fs::remove_file(request.runtime_kit.join("cores").join(CORE)).unwrap();
    cache_core(root, "the first core", &[], "the old licence");
    request.core_cache = Some(cache(root));
    let made =
        rominabox_engine::packaging::export_game_fetching(&request, &AtomicBool::new(false), |_| {}, &Offline).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(made.app_path.join("Contents/Resources/game.json")).unwrap()).unwrap();
    let identity = manifest["identity"].as_str().unwrap().to_string();
    let library = Library::at(root.join("data"), Layout::Macos);
    install(&library, &made.app_path, &identity, "rings");
    (library, made.app_path, identity, request.runtime_kit)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle.as_bytes())
}

/// What `codesign` says of the app, which is empty when its signature holds.
fn signature_problems(app: &Path) -> String {
    let output = Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(app).output().unwrap();
    if output.status.success() {
        String::new()
    } else {
        String::from_utf8_lossy(&output.stderr).into_owned()
    }
}

fn left_work(app: &Path) -> Vec<String> {
    fs::read_dir(app.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".rominabox-update-"))
        .collect()
}

/// A Mac game built again from its recipe with the same kit is the same app,
/// file for file, and its saves stay.
#[test]
fn a_mac_game_built_again_from_its_recipe_is_the_same_game() {
    let root = workspace();
    let (library, app, identity, kit) = installed_game(&root);
    let before = files(&app);

    library.update_engine(&identity, &kit).unwrap();

    let after = files(&app);
    let differing: Vec<_> =
        before.keys().chain(after.keys()).filter(|path| before.get(*path) != after.get(*path)).collect();
    assert!(differing.is_empty(), "the game built again differs in {differing:?}");
    assert_eq!(signature_problems(&app), "");
    assert_eq!(fs::read_to_string(library.data_dir(&identity).join("saves/sonic.srm")).unwrap(), "rings");
    assert_eq!(left_work(&app), Vec::<String>::new());
}

/// We put a newer core into a Mac game, with its licence text, and sign the
/// app again, so that macOS still accepts it. Its identity and saves stay.
/// With the same core, we change nothing.
#[test]
fn a_newer_core_goes_into_a_mac_game_and_its_saves_stay() {
    let root = workspace();
    let (library, app, identity, kit) = installed_game(&root);
    let core = app.join("Contents/Resources/game-core.dylib");
    assert!(contains(&fs::read(&core).unwrap(), "the first core"));
    cache_core(&root, "the newest core", &[], "the new licence");

    assert_eq!(library.update_core(&identity, &kit, &cache(&root), &Offline), Ok(CoreUpdate::Updated));

    assert!(contains(&fs::read(&core).unwrap(), "the newest core"));
    assert_eq!(
        fs::read_to_string(app.join("Contents/Resources/Legal/Licenses").join(LICENCE)).unwrap(),
        "the new licence"
    );
    assert_eq!(signature_problems(&app), "");
    assert!(library.games().iter().any(|game| game.game.identity == identity && game.app_present));
    assert_eq!(fs::read_to_string(library.data_dir(&identity).join("saves/sonic.srm")).unwrap(), "rings");

    let before = files(&app);
    assert_eq!(library.update_core(&identity, &kit, &cache(&root), &Offline), Ok(CoreUpdate::Current));
    assert!(files(&app) == before, "the same core changed the game");
    assert_eq!(left_work(&app), Vec::<String>::new());
}

/// We refuse a core that needs a library the game does not contain, and the
/// game stays as it was.
#[test]
fn a_core_that_needs_a_missing_library_is_refused_and_the_game_stays() {
    let root = workspace();
    let (library, app, identity, kit) = installed_game(&root);
    let before = files(&app);
    let extra = root.join("extra/libextra.dylib");
    mac_library(&extra, "void extra(void) {}\n", &[]);
    let folder = format!("-L{}", extra.parent().unwrap().display());
    cache_core(&root, "a core with a library", &[&folder, "-lextra"], "the new licence");

    let refused = library.update_core(&identity, &kit, &cache(&root), &Offline);

    assert!(
        refused.as_ref().is_err_and(|message| message.contains("libextra.dylib, which this game does not include")),
        "{refused:?}"
    );
    assert!(files(&app) == before, "a refused core changed the game");
    assert_eq!(left_work(&app), Vec::<String>::new());
}
