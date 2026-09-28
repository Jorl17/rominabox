//! A Mac game that also runs on Intel Macs, and one that does not.
//!
//! The player in the kit is a universal stand-in, as in a kit built for both
//! processors, and each core is a small library for one processor, as in the
//! download list. We read the processors of each file in an app with
//! `lipo -archs`. We launch nothing and use no network: we fill the core
//! cache first, or answer downloads from a table.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod export_fixture;

use export_fixture::{export_request, workspace, write_runtime_stub_for};
use rominabox_desktop::builder::defaults;
use rominabox_desktop::cores::{Response, Transport, Version};
use rominabox_desktop::packaging::{export_game_fetching, ErrorStage, ExportRequest};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

const CORE: &str = "genesis_plus_gx_libretro.dylib";
const LICENCE: &str = "genesis_plus_gx.txt";

/// Return the sorted processors that `path` has code for, from `lipo`.
fn listed(path: &Path) -> Vec<String> {
    let output = Command::new("lipo").arg("-archs").arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut archs: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    archs.sort();
    archs
}

/// Return the processors of the app's player, launch library and core.
fn carried(app: &Path) -> [Vec<String>; 3] {
    [
        listed(&app.join("Contents/MacOS/retroarch")),
        listed(&app.join("Contents/MacOS/librominabox-launch.dylib")),
        listed(&app.join("Contents/Resources/game-core.dylib")),
    ]
}

/// A core library built for `arch` alone.
fn core_for(path: &Path, arch: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = path.with_extension("c");
    fs::write(&source, "unsigned retro_api_version(void) { return 1; }\n").unwrap();
    let status = Command::new("cc")
        .args(["-arch", arch, "-dynamiclib", "-o"])
        .arg(path)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    fs::remove_file(source).unwrap();
}

/// The builder's core cache for `target`: `root/core-cache/<target>`.
fn cache(root: &Path, target: &str) -> PathBuf {
    root.join("core-cache").join(target)
}

/// A cache for `target` with the Mega Drive core, built for `arch`.
fn cached_core(root: &Path, target: &str, arch: &str) -> PathBuf {
    let cache = cache(root, target);
    core_for(&cache.join("cores").join(CORE), arch);
    fs::create_dir_all(cache.join("licenses")).unwrap();
    fs::write(cache.join("licenses").join(LICENCE), "licence").unwrap();
    cache
}

/// A Mega Drive export from a kit with a universal player, and the core for
/// the host Mac in the cache.
fn universal_request(root: &Path) -> ExportRequest {
    let mut request = export_request(root);
    write_runtime_stub_for(&request.runtime_kit.join("bin/retroarch"), &["arm64", "x86_64"]);
    // The core in the stand-in kit is not a library, so we use the cached one.
    fs::remove_file(request.runtime_kit.join("cores").join(CORE)).unwrap();
    request.core_cache = Some(cached_core(root, "macos-arm64", "arm64"));
    request
}

/// A stand-in for the network. We answer from a table of URL endings, fail
/// any other URL like an unreachable server, and record every download.
#[derive(Default)]
struct Table {
    files: Vec<(String, Vec<u8>)>,
    downloads: RefCell<Vec<String>>,
}

impl Transport for Table {
    fn get(&self, url: &str) -> Result<Response, ()> {
        self.downloads.borrow_mut().push(url.to_string());
        let (_, body) = self.files.iter().find(|(end, _)| url.ends_with(end.as_str())).ok_or(())?;
        Ok(Response {
            body: body.clone(),
            version: Version::default(),
        })
    }

    fn head(&self, _url: &str) -> Result<Version, ()> {
        Err(())
    }
}

fn export(request: &ExportRequest, table: &Table) -> Result<PathBuf, rominabox_desktop::packaging::ExportError> {
    export_game_fetching(request, &AtomicBool::new(false), |_| {}, table).map(|result| result.app_path)
}

#[test]
fn a_game_runs_on_this_mac_alone_unless_its_author_says_otherwise() {
    assert!(!defaults().intel_macs);
    let request: ExportRequest = serde_json::from_value(json!({
        "rom": "game.md", "title": "Game", "system": "megadrive",
        "outputDir": "out", "target": "macos"
    }))
    .unwrap();
    assert!(!request.intel_macs);
}

#[test]
fn an_ordinary_game_carries_this_macs_code_alone() {
    let root = workspace();
    let request = universal_request(&root);
    let app = export(&request, &Table::default()).unwrap();
    assert_eq!(carried(&app), [["arm64"], ["arm64"], ["arm64"]]);
    assert_eq!(
        listed(&request.runtime_kit.join("bin/retroarch")),
        ["arm64", "x86_64"],
        "the kit's player was changed"
    );
}

#[test]
fn a_game_that_also_runs_on_intel_macs_carries_both_in_its_player_library_and_core() {
    let root = workspace();
    let mut request = universal_request(&root);
    cached_core(&root, "macos-x86_64", "x86_64");
    request.intel_macs = true;
    let app = export(&request, &Table::default()).unwrap();
    let both = vec!["arm64".to_string(), "x86_64".to_string()];
    assert_eq!(carried(&app), [both.clone(), both.clone(), both]);
    let verified = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
}

/// We download the Intel core into its own folder beside the one for the
/// host Mac, under the name it has for Intel Macs in the download list.
#[test]
fn the_intel_core_is_fetched_into_the_cache_beside_this_macs() {
    let root = workspace();
    let mut request = universal_request(&root);
    request.intel_macs = true;
    let built = root.join("served").join(CORE);
    core_for(&built, "x86_64");
    let table = Table {
        files: vec![
            (format!("/apple/osx/x86_64/latest/{CORE}.zip"), zip_of(CORE, &fs::read(&built).unwrap())),
            ("/LICENSE.txt".into(), b"licence".to_vec()),
        ],
        ..Table::default()
    };
    let app = export(&request, &table).unwrap();

    let fetched = cache(&root, "macos-x86_64").join("cores").join(CORE);
    assert_eq!(listed(&fetched), ["x86_64"]);
    assert!(
        table.downloads.borrow().iter().all(|url| !url.contains("/arm64/")),
        "this Mac's cached core was downloaded again: {:?}",
        table.downloads.borrow()
    );
    assert_eq!(listed(&app.join("Contents/Resources/game-core.dylib")), ["arm64", "x86_64"]);
}

#[test]
fn a_runtime_without_an_intel_player_is_refused_in_the_authors_words() {
    let root = workspace();
    let mut request = universal_request(&root);
    write_runtime_stub_for(&request.runtime_kit.join("bin/retroarch"), &["arm64"]);
    cached_core(&root, "macos-x86_64", "x86_64");
    request.intel_macs = true;
    let error = export(&request, &Table::default()).unwrap_err();
    assert_eq!(error.stage, ErrorStage::Refused);
    assert_eq!(
        error.sentence(),
        "This builder's runtime has no Intel version, so the game cannot run on Intel Macs."
    );
    assert!(
        !request.output_dir.exists() || fs::read_dir(&request.output_dir).unwrap().next().is_none(),
        "a refused export left something behind"
    );
}

/// We accept the same request in the command-line export, and a project
/// saved and opened through the command line still has the choice on.
#[test]
fn the_command_line_and_a_project_carry_intel_macs() {
    let root = workspace();
    let request = universal_request(&root);
    cached_core(&root, "macos-x86_64", "x86_64");
    let mut stated = serde_json::to_value(&request).unwrap();
    stated["intelMacs"] = json!(true);

    let exported = cli("export", &stated);
    let app = exported
        .iter()
        .find(|event| event["type"] == "result")
        .map(|event| PathBuf::from(event["result"]["appPath"].as_str().unwrap()))
        .unwrap_or_else(|| panic!("no app: {exported:?}"));
    assert_eq!(listed(&app.join("Contents/MacOS/retroarch")), ["arm64", "x86_64"]);

    let archive = root.join("game.rominabox");
    cli("project-save", &json!({ "archivePath": archive, "settings": stated }));
    let opened = cli(
        "project-open",
        &json!({ "archivePath": archive, "extractionDir": root.join("opened") }),
    );
    assert_eq!(opened.last().unwrap()["result"]["settings"]["intelMacs"], json!(true));

    let mut unstated = stated.clone();
    unstated.as_object_mut().unwrap().remove("intelMacs");
    let archive = root.join("unstated.rominabox");
    cli("project-save", &json!({ "archivePath": archive, "settings": unstated }));
    let opened = cli(
        "project-open",
        &json!({ "archivePath": archive, "extractionDir": root.join("opened-unstated") }),
    );
    assert_eq!(opened.last().unwrap()["result"]["settings"]["intelMacs"], json!(false));
}

/// Run a command-line command with `request` on stdin and return its JSON lines.
fn cli(command: &str, request: &Value) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg(command)
        .env(rominabox_desktop::cores::OFFLINE_VARIABLE, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{command}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn zip_of(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap();
    }
    cursor.into_inner()
}
