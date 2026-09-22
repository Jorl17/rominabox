#![cfg(target_os = "macos")]

use rominabox_desktop::packaging::{
    isolated_hotkey_config, ExportRequest, ExportStage, ExportTarget, HOTKEY_BINDS,
    MANAGED_DATA_DIRECTORIES,
};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

fn write_runtime_stub(path: &Path) {
    let source = path.with_extension("c");
    fs::write(
        &source,
        "int rarch_main(int c, char **v, void *d){(void)c;(void)v;(void)d;return 0;}\nint main(void){return rarch_main(0,0,0);}\n",
    )
    .unwrap();
    let status = Command::new("cc")
        .args(["-Oz", "-Wl,-headerpad_max_install_names", "-o"])
        .arg(path)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "could not compile the runtime stub");
}

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-packaging-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn fixture_kit(root: &Path) -> PathBuf {
    let kit = root.join("runtime-kit");
    fs::create_dir_all(kit.join("bin")).unwrap();
    fs::create_dir_all(kit.join("cores")).unwrap();
    fs::create_dir_all(kit.join("Frameworks")).unwrap();
    fs::create_dir_all(kit.join("licenses")).unwrap();
    fs::create_dir_all(kit.join("licenses/native")).unwrap();
    fs::create_dir_all(kit.join("provenance/native-rmlui")).unwrap();
    write_runtime_stub(&kit.join("bin/retroarch"));
    fs::write(kit.join("cores/genesis_plus_gx_libretro.dylib"), b"core").unwrap();
    for name in [
        "RetroArch.txt",
        "NATIVE-DEPENDENCIES.txt",
        "RmlUi-MIT.txt",
        "genesis_plus_gx.txt",
    ] {
        fs::write(kit.join("licenses").join(name), name).unwrap();
    }
    fs::write(
        kit.join("runtime-dependencies.json"),
        r#"{"formatVersion":1,"files":[]}"#,
    )
    .unwrap();
    fs::write(
        kit.join("manifest.json"),
        r#"{"schema_version":1,"components":[{"name":"RetroArch"},{"name":"RmlUi"},{"name":"genesis_plus_gx"}]}"#,
    )
    .unwrap();
    kit
}

fn export_request(root: &Path) -> ExportRequest {
    let rom = root.join("sonic.bin");
    fs::write(&rom, b"RIBtest").unwrap();
    ExportRequest {
        rom,
        title: "Hotkey Isolation".to_string(),
        system: "megadrive".to_string(),
        description: None,
        icon: None,
        background: None,
        show_menu: false,
        start_at_menu: false,
        theme: "native".to_string(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls: rominabox_desktop::controls::Controls::default(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: false,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        achievements: Default::default(),
        output_dir: root.join("out"),
        target: ExportTarget::Macos,
        runtime_kit: fixture_kit(root),
        core: None,
        core_cache: None,
    }
}

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
    // Fullscreen and quit require advanced emulator access, so in a default
    // export f is free for gameplay. The macOS window menu still has Full
    // Screen. This assertion checks that default.
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

    assert_eq!(error.stage, "cancelled");
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

    assert_eq!(error.stage, "stage");
    assert_no_export_staging(&request.output_dir);
}

/// Write the Dreamcast core into the cache and count how often we request it.
struct CountingFetch {
    calls: Cell<usize>,
    bytes: &'static [u8],
}

impl rominabox_desktop::packaging::CoreFetch for CountingFetch {
    fn fetch_component(&self, cache: &Path, component: &str, target: &str) -> Result<(), String> {
        let _ = target;
        assert_eq!(component, "flycast");
        self.calls.set(self.calls.get() + 1);
        let system = rominabox_desktop::systems::find("dreamcast").unwrap();
        let core = system.preferred_core().unwrap();
        let filename = core.artifact().unwrap();
        fs::create_dir_all(cache.join("cores")).unwrap();
        fs::create_dir_all(cache.join("licenses")).unwrap();
        fs::write(cache.join("cores").join(filename), self.bytes).unwrap();
        fs::write(
            cache.join("licenses").join(&core.license_file),
            b"flycast-licence",
        )
        .unwrap();
        Ok(())
    }
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

#[test]
fn a_missing_core_is_fetched_before_the_game_is_built_and_not_again() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    let fetch = CountingFetch {
        calls: Cell::new(0),
        bytes: b"flycast-bytes",
    };
    let cancelled = AtomicBool::new(false);
    let first =
        rominabox_desktop::packaging::export_game_fetching(&request, &cancelled, |_| {}, &fetch);
    assert!(
        fetch.calls.get() >= 1,
        "export did not fetch the missing core before building: {first:?}"
    );
    let first = first.expect("the game is built after the core is fetched");
    let embedded = fs::read(first.app_path.join("Contents/Resources/game-core.dylib")).unwrap();
    assert_eq!(embedded, b"flycast-bytes");
    let again = workspace();
    request.output_dir = again.join("out");
    let second =
        rominabox_desktop::packaging::export_game_fetching(&request, &cancelled, |_| {}, &fetch);
    assert!(second.is_ok(), "{second:?}");
    assert_eq!(
        fetch.calls.get(),
        1,
        "the second export fetched a core that was already kept"
    );
}

/// A Windows package contains the Windows core even when we run on a Mac.
struct RecordingFetch {
    calls: Cell<usize>,
    target: std::cell::RefCell<String>,
}

impl rominabox_desktop::packaging::CoreFetch for RecordingFetch {
    fn fetch_component(&self, _cache: &Path, component: &str, target: &str) -> Result<(), String> {
        assert_eq!(component, "flycast");
        self.calls.set(self.calls.get() + 1);
        *self.target.borrow_mut() = target.to_string();
        Ok(())
    }
}

#[test]
fn a_windows_export_fetches_the_windows_core_not_this_machines() {
    let root = workspace();
    let mut request = dreamcast_request(&root);
    request.target = ExportTarget::Windows;
    let fetch = RecordingFetch {
        calls: Cell::new(0),
        target: std::cell::RefCell::new(String::new()),
    };
    let _ = rominabox_desktop::packaging::export_game_fetching(
        &request,
        &AtomicBool::new(false),
        |_| {},
        &fetch,
    );
    assert_eq!(fetch.calls.get(), 1, "the missing core was not fetched");
    assert_eq!(
        fetch.target.borrow().as_str(),
        "windows-x86_64",
        "the export asked for the host platform's core"
    );
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
    let fetch = RecordingFetch {
        calls: Cell::new(0),
        target: std::cell::RefCell::new(String::new()),
    };
    let error = rominabox_desktop::packaging::export_game_fetching(
        &request,
        &AtomicBool::new(false),
        |_| {},
        &fetch,
    )
    .expect_err("this build does not finish a windows package");
    assert_eq!(
        fetch.calls.get(),
        0,
        "the windows core was already cached; export fetched anyway"
    );
    assert!(
        !error.message.contains("flycast_libretro.dylib"),
        "windows core was cached; export said: {}",
        error.message
    );
}
