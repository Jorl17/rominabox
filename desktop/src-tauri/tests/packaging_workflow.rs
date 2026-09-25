#![cfg(target_os = "macos")]

mod export_fixture;

use export_fixture::{export_request, workspace};

use rominabox_desktop::cores::{Response, Transport, Version};
use rominabox_desktop::export_cores::CoreActivity;
use rominabox_desktop::hotkeys::{isolated_hotkey_config, HOTKEY_BINDS};
use rominabox_desktop::packaging::{
    ErrorStage, ExportRequest, ExportStage, ExportTarget, MANAGED_DATA_DIRECTORIES,
};
use std::{
    cell::Cell,
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

fn embedded_runtime_config(plan: &str) -> String {
    let marker = "---config---\n";
    let start = plan
        .find(marker)
        .expect("exported launch plan contains the runtime config");
    plan[start + marker.len()..].to_string()
}

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

fn assert_no_export_staging(output_dir: &Path) {
    let staging_directories = fs::read_dir(output_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".rominabox-export-")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(
        staging_directories.is_empty(),
        "export must remove owned staging directories: {staging_directories:?}"
    );
}

/// An export is the app alone. A second file in the output directory, such as
/// a zip beside the app, is a defect.
#[test]
fn an_export_writes_the_app_and_nothing_else() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();

    let mut names = fs::read_dir(&request.output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["Hotkey Isolation.app".to_string()],
        "the output directory must contain the app and nothing else"
    );
    assert!(result.app_path.is_dir());
    assert_eq!(
        result.app_path,
        request.output_dir.join("Hotkey Isolation.app")
    );
}

#[test]
fn export_writes_the_reviewed_hotkey_policy_and_managed_paths() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {}).unwrap();
    assert_no_export_staging(&request.output_dir);
    let plan_path = result.app_path.join("Contents/Resources/launch.plan");
    let plan = fs::read_to_string(&plan_path).unwrap();
    let config = embedded_runtime_config(&plan);
    let policy = isolated_hotkey_config(false, false);

    assert!(
        config.contains(&policy),
        "exported retroarch.cfg must embed the single hotkey policy"
    );
    assert_eq!(
        config_value(&config, "input_toggle_fast_forward"),
        Some("nul")
    );
    assert_eq!(
        config_value(&config, "input_hold_fast_forward"),
        Some("nul")
    );
    assert_eq!(config_value(&config, "input_menu_toggle"), Some("nul"));
    // Fullscreen is Alt+Enter, so f stays a gameplay key.
    assert_eq!(
        config_value(&config, "input_toggle_fullscreen"),
        Some("nul")
    );
    // There is no exit key in a default export. Quit is in the menu, not on Q.
    assert_eq!(config_value(&config, "input_exit_emulator"), Some("nul"));
    assert!(!config.contains("input_player1_"));
    for directory in MANAGED_DATA_DIRECTORIES {
        assert!(
            plan.contains(&format!("managed\t{directory}\n")),
            "the launch plan must name {directory}"
        );
    }
    assert!(plan.contains("data_dir\t$HOME/Library/Application Support/ROM-in-a-Box/Games/"));
    assert!(
        plan.contains(&format!(
            "volume_file\t{}\n",
            rominabox_desktop::volume::file_name()
        )),
        "the launcher has to merge the per-game volume file, by the name the player writes"
    );
    assert!(!plan.contains("export HOME="));
    assert_eq!(
        HOTKEY_BINDS
            .iter()
            .filter(|bind| bind.name == "toggle_fast_forward")
            .count(),
        1
    );
}

#[test]
fn cancelled_export_removes_its_staging_directory() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);

    let error = rominabox_desktop::packaging::export_game(&request, &cancelled, |progress| {
        if matches!(progress.stage, ExportStage::Stage) {
            cancelled.store(true, Ordering::Relaxed);
        }
    })
    .unwrap_err();

    assert_eq!(error.stage, ErrorStage::Cancelled);
    assert_no_export_staging(&request.output_dir);
}

#[test]
fn failed_export_removes_its_staging_directory() {
    let root = workspace();
    let request = export_request(&root);
    let cancelled = AtomicBool::new(false);
    let runtime = request.runtime_kit.join("bin/retroarch");

    let error = rominabox_desktop::packaging::export_game(&request, &cancelled, |progress| {
        if matches!(progress.stage, ExportStage::Stage) {
            fs::remove_file(&runtime).unwrap();
        }
    })
    .unwrap_err();

    assert_eq!(error.stage, ErrorStage::Stage);
    assert_no_export_staging(&request.output_dir);
}

/// A stand-in for the network that we answer from a table. A download is a
/// URL that ends in one of the keys of `files`, and a check gets `heads`. We
/// record every URL, so that a test can tell what an export fetched, and we
/// fail any URL outside the table like an unreachable server.
#[derive(Default)]
struct Table {
    files: std::cell::RefCell<Vec<(String, Vec<u8>)>>,
    heads: std::cell::RefCell<Vec<(String, Version)>>,
    served: std::cell::RefCell<Version>,
    downloads: std::cell::RefCell<Vec<String>>,
    checks: Cell<usize>,
}

impl Table {
    /// A server that has the Dreamcast core as `bytes`, called `etag`.
    fn flycast(core: &str, bytes: &[u8], etag: &str) -> Self {
        let table = Table::default();
        table.publish(core, bytes, etag);
        table
    }

    fn publish(&self, core: &str, bytes: &[u8], etag: &str) {
        let version = Version {
            etag: Some(etag.to_string()),
            ..Version::default()
        };
        *self.files.borrow_mut() = vec![
            (format!("/latest/{core}.zip"), zip_of(core, bytes)),
            (
                "/flyinghead/flycast/master/LICENSE".into(),
                b"flycast-licence".to_vec(),
            ),
            (
                "/flyinghead/flycast@master/LICENSE".into(),
                b"flycast-licence".to_vec(),
            ),
        ];
        *self.heads.borrow_mut() = vec![(format!("/latest/{core}.zip"), version.clone())];
        *self.served.borrow_mut() = version;
    }

    fn unreachable(&self) {
        self.files.borrow_mut().clear();
        self.heads.borrow_mut().clear();
    }
}

impl Transport for Table {
    fn get(&self, url: &str) -> Result<Response, ()> {
        self.downloads.borrow_mut().push(url.to_string());
        let files = self.files.borrow();
        let (_, body) = files
            .iter()
            .find(|(key, _)| url.ends_with(key.as_str()))
            .ok_or(())?;
        Ok(Response {
            body: body.clone(),
            version: self.served.borrow().clone(),
        })
    }

    fn head(&self, url: &str) -> Result<Version, ()> {
        self.checks.set(self.checks.get() + 1);
        let heads = self.heads.borrow();
        let (_, version) = heads
            .iter()
            .find(|(key, _)| url.ends_with(key.as_str()))
            .ok_or(())?;
        Ok(version.clone())
    }
}

fn zip_of(name: &str, bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
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

fn dreamcast_request(root: &Path) -> rominabox_desktop::packaging::ExportRequest {
    let mut request = export_request(root);
    let rom = root.join("sonic.cdi");
    fs::write(&rom, b"RIBdreamcast").unwrap();
    request.rom = rom;
    request.title = "Dreamcast Fetch".into();
    request.system = "dreamcast".into();
    request.core_cache = Some(root.join("core-cache"));
    fs::create_dir_all(request.core_cache.as_ref().unwrap()).unwrap();
    request
}

/// Export once into a new directory beside the cache, and return the messages
/// about cores and the core in the export.
fn export_with(
    request: &mut ExportRequest,
    table: &Table,
) -> (
    Vec<CoreActivity>,
    Result<Vec<u8>, rominabox_desktop::packaging::ExportError>,
) {
    let root = request.core_cache.as_ref().unwrap().parent().unwrap();
    let exports = fs::read_dir(root).unwrap().count();
    request.output_dir = root.join(format!("out-{exports}"));
    let mut said = Vec::new();
    let result = rominabox_desktop::packaging::export_game_fetching(
        request,
        &AtomicBool::new(false),
        |progress| said.extend(progress.cores),
        table,
    );
    let shipped = result
        .map(|app| fs::read(app.app_path.join("Contents/Resources/game-core.dylib")).unwrap());
    (said, shipped)
}

#[test]
fn a_missing_core_is_downloaded_before_the_game_is_built_and_then_reused() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    let table = Table::flycast("flycast_libretro.dylib", b"flycast-bytes", "\"1\"");
    let (said, shipped) = export_with(&mut request, &table);
    assert_eq!(
        said,
        [CoreActivity::Fetching {
            downloading: 1,
            updating: 0
        }]
    );
    assert_eq!(
        shipped.expect("the game is built after the download"),
        b"flycast-bytes"
    );
    let fetched = table.downloads.borrow().len();

    // Nothing changed on the server, so we show nothing and download nothing.
    let (said, shipped) = export_with(&mut request, &table);
    assert_eq!(
        said,
        [],
        "an export with nothing to fetch showed the pop-up"
    );
    assert_eq!(shipped.unwrap(), b"flycast-bytes");
    assert_eq!(
        table.downloads.borrow().len(),
        fetched,
        "the cached core was downloaded again"
    );
    assert!(
        table.checks.get() >= 1,
        "the export did not ask whether the core changed"
    );
}

#[test]
fn a_newer_nightly_replaces_the_cached_core() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    let table = Table::flycast("flycast_libretro.dylib", b"monday", "\"1\"");
    export_with(&mut request, &table).1.unwrap();
    table.publish("flycast_libretro.dylib", b"tuesday", "\"2\"");
    let (said, shipped) = export_with(&mut request, &table);
    assert_eq!(
        said,
        [CoreActivity::Fetching {
            downloading: 0,
            updating: 1
        }]
    );
    assert_eq!(shipped.unwrap(), b"tuesday");
}

#[test]
fn a_cached_core_is_used_without_a_word_when_the_check_fails() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    let table = Table::flycast("flycast_libretro.dylib", b"cached", "\"1\"");
    export_with(&mut request, &table).1.unwrap();
    table.unreachable();
    let fetched = table.downloads.borrow().len();
    let (said, shipped) = export_with(&mut request, &table);
    assert_eq!(said, []);
    assert_eq!(shipped.unwrap(), b"cached");
    assert_eq!(table.downloads.borrow().len(), fetched);
}

#[test]
fn a_missing_core_that_cannot_be_downloaded_stops_the_export_before_anything_is_built() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    let table = Table::default();
    let (said, shipped) = export_with(&mut request, &table);
    assert_eq!(
        said,
        [
            CoreActivity::Fetching {
                downloading: 1,
                updating: 0
            },
            CoreActivity::Failed { missing: 1 },
        ]
    );
    let error = shipped.unwrap_err();
    assert_eq!(error.stage, ErrorStage::Cores);
    assert_eq!(
        error.message,
        "The Dreamcast core could not be downloaded. Try again later."
    );
    assert!(
        !request.output_dir.exists() || fs::read_dir(&request.output_dir).unwrap().next().is_none()
    );
}

#[test]
fn a_windows_export_fetches_the_windows_core_not_this_machines() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    request.target = ExportTarget::Windows;
    let table = Table::default();
    let _ = export_with(&mut request, &table);
    let asked = table.downloads.borrow();
    assert!(
        asked
            .iter()
            .any(|url| url.contains("/windows/x86_64/latest/flycast_libretro.dll.zip")),
        "the export requested the host's core: {asked:?}"
    );
    assert!(!asked.iter().any(|url| url.contains("dylib")), "{asked:?}");
}

/// We find a Windows core that is already in the cache for a Windows export.
///
/// We name the download for the platform of the export, and look for the
/// same name in the cache. On a Mac, the core of a Windows Dreamcast export
/// is `flycast_libretro.dll`, and a missing `.dylib` in the kit is no reason
/// to refuse it.
#[test]
fn a_windows_export_with_the_windows_core_cached_does_not_ask_for_the_mac_file() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    request.target = ExportTarget::Windows;
    let cache = request.core_cache.as_ref().unwrap();
    fs::create_dir_all(cache.join("cores")).unwrap();
    fs::create_dir_all(cache.join("licenses")).unwrap();
    fs::write(cache.join("cores/flycast_libretro.dll"), b"flycast-windows").unwrap();
    fs::write(cache.join("licenses/flycast.txt"), b"flycast-licence").unwrap();
    let table = Table::default();
    let (said, shipped) = export_with(&mut request, &table);
    let error = shipped.expect_err("this build does not finish a windows package");
    assert_eq!(said, []);
    assert!(
        table.downloads.borrow().is_empty(),
        "the windows core was already cached; export fetched anyway"
    );
    assert!(
        !error.message.contains("flycast_libretro.dylib"),
        "windows core was cached; export said: {}",
        error.message
    );
}

/// No test uses the network. We set the variable for every test scope in
/// `scripts/test.py`, and we stop an export that would need the network
/// instead of downloading.
#[test]
fn an_export_under_test_cannot_reach_the_network() {
    let root = workspace();
    let request = dreamcast_request(&root);
    let output = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg("export")
        .env(rominabox_desktop::cores::OFFLINE_VARIABLE, "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(&request).unwrap())?;
            child.wait_with_output()
        })
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("a core was asked of the network"),
        "the export was not refused: {stderr}"
    );
    assert!(!root.join("core-cache/cores").exists());
}

#[test]
fn achievements_require_a_capable_artifact_before_export_staging() {
    let root = workspace();
    let mut request = export_request(&root);
    request.show_menu = true;
    request.include_achievements = true;
    let error =
        rominabox_desktop::packaging::export_game(&request, &AtomicBool::new(false), |_| {})
            .unwrap_err();
    assert!(
        error.message.contains("verified achievements support"),
        "{error}"
    );
    assert!(
        !request.output_dir.exists(),
        "preflight must not stage an incapable player"
    );
    request.show_menu = false;
    let result =
        rominabox_desktop::packaging::export_game(&request, &AtomicBool::new(false), |_| {})
            .unwrap();
    let resources = result.app_path.join("Contents/Resources");
    assert!(!resources.join("menu-assets/menu.rml").exists());
    let plan = fs::read_to_string(resources.join("launch.plan")).unwrap();
    assert!(plan.contains("achievements\t0\n"));
    assert!(!plan.contains("accounts_dir"), "a game without achievements names no accounts folder");
    let signed = Command::new("codesign")
        .args(["-d", "--entitlements", "-"])
        .arg(&result.app_path)
        .output()
        .unwrap();
    let entitlements = String::from_utf8_lossy(&signed.stdout);
    assert!(!entitlements.contains("com.apple.security.network.client"));
    assert!(!entitlements.contains("read-write"));
    assert!(!entitlements.contains("Accounts"));
}

#[test]
fn included_achievements_export_an_account_screen_and_network_permission() {
    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dest = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &dest);
            } else {
                fs::copy(entry.path(), dest).unwrap();
            }
        }
    }
    let root = workspace();
    let mut request = export_request(&root);
    request.show_menu = true;
    request.include_achievements = true;
    let manifest = request.runtime_kit.join("manifest.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["components"][0]["capabilities"] = serde_json::json!({"achievements": true});
    fs::write(manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    copy_tree(
        &rominabox_desktop::repo::at("integrations/designs"),
        &request.runtime_kit.join("designs"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("integrations/parts"),
        &request.runtime_kit.join("parts"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("desktop/assets/controllers"),
        &request.runtime_kit.join("menu-assets"),
    );
    let result =
        rominabox_desktop::packaging::export_game(&request, &AtomicBool::new(false), |_| {})
            .unwrap();
    let resources = result.app_path.join("Contents/Resources");
    let menu = fs::read_to_string(resources.join("menu-assets/menu.rml")).unwrap();
    assert!(menu.contains("id=\"achievements\"") && menu.contains("id=\"achievement-password\""));
    assert!(!menu.contains("achievement-mode"));
    let plan = fs::read_to_string(resources.join("launch.plan")).unwrap();
    assert!(plan.contains("achievements\t1\n"));
    let config = embedded_runtime_config(&plan);
    assert_eq!(
        config_value(&config, "cheevos_hardcore_mode_enable"),
        Some("false")
    );
    let signed = Command::new("codesign")
        .args(["-d", "--entitlements", "-"])
        .arg(&result.app_path)
        .output()
        .unwrap();
    assert!(signed.status.success());
    let entitlements = String::from_utf8_lossy(&signed.stdout);
    assert!(entitlements.contains("com.apple.security.network.client"));
    // The only folder we open in the sandbox is the launcher's folder.
    let folder = plan
        .lines()
        .find_map(|line| line.strip_prefix("accounts_dir\t"))
        .expect("a game with achievements names its accounts folder");
    assert!(folder.starts_with("ROM-in-a-Box Accounts"), "{folder}");
    assert!(
        entitlements.contains(&format!("/Library/Application Support/{folder}/")),
        "{entitlements}"
    );
    assert_eq!(entitlements.matches("read-write").count(), 1, "{entitlements}");
}
