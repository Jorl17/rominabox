//! We run every exported game in a sandbox, and with these tests we notice
//! when it is missing. The checks are the same on every platform. What the
//! game is on each platform, how we start it and where we keep its data in
//! the sandbox are in isolation/macos.rs and isolation/windows.rs.
//!
//! We mark the tests ignored so that the exporter tests launch nothing, and
//! we run them in the isolation tests.

#![cfg(any(target_os = "macos", windows))]

mod export_fixture;
mod support;

#[cfg(target_os = "macos")]
#[path = "isolation/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "isolation/windows.rs"]
mod platform;

use rominabox_desktop::packaging::ExportRequest;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// A game folder that the test makes where games kept their data before they
/// had a sandbox. We remove it when the test ends.
struct GameFolder(PathBuf);

impl Drop for GameFolder {
    fn drop(&mut self) {
        let Some(name) = self.0.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let game = name.len() == 24
            && name.chars().all(|character| character.is_ascii_hexdigit())
            && self
                .0
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|parent| parent.to_str())
                == Some("Games");
        if game && self.0.is_dir() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn stay_quiet(command: &mut Command) {
    // In the launcher this is ROMINABOX_QUIET_ENV. Without it we open the
    // sound device. In the quiet tests we check that the define and this
    // name are the same.
    command.env("ROMINABOX_QUIET", "1");
}

/// A path in the checkout of this run, not in the checkout where the binary
/// was built. Several checkouts can share one cargo target, so they can differ.
fn repo_at(relative: &str) -> PathBuf {
    rominabox_desktop::repo::at(relative)
}

/// Whether a controller is plugged into the host.
fn gamepad_connected() -> bool {
    let Ok(mut gilrs) = gilrs::Gilrs::new() else {
        return false;
    };
    let until = std::time::Instant::now() + Duration::from_millis(500);
    while std::time::Instant::now() < until {
        while gilrs.next_event().is_some() {}
        std::thread::sleep(Duration::from_millis(20));
    }
    gilrs.gamepads().next().is_some()
}

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-isolation")
}

fn request(
    root: &Path,
    rom_bytes: &[u8],
    title: &str,
    kit: PathBuf,
    system: &str,
) -> ExportRequest {
    let rom = root.join("game.bin");
    fs::write(&rom, rom_bytes).unwrap();
    ExportRequest {
        rom,
        title: title.to_string(),
        system: system.to_string(),
        description: None,
        icon: None,
        background: None,
        show_menu: false,
        start_at_menu: false,
        theme: "native".to_string(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls: rominabox_desktop::controls::Controls::default(),
        menu_controls: rominabox_desktop::builder::unstated::menu_controls(),
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: false,
        intel_macs: false,
        zip: None,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        include_achievements: false,
        output_dir: root.join("out"),
        replace: false,
        target: platform::TARGET,
        runtime_kit: kit,
        core: None,
        core_cache: None,
    }
}

fn export(request: &ExportRequest) -> PathBuf {
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    rominabox_desktop::packaging::export_game(request, &cancelled, |_| {})
        .unwrap_or_else(|error| panic!("export failed: {error}"))
        .app_path
}

fn identity_of(app: &Path) -> String {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(platform::resources_of(app).join("game.json")).unwrap())
            .unwrap();
    manifest["identity"].as_str().unwrap().to_string()
}

/// The folder a game kept its data in before it had a sandbox.
fn previous_game_folder(identity: &str) -> PathBuf {
    platform::user_data()
        .join("ROM-in-a-Box/Games")
        .join(identity)
}

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

fn run_until(command: &mut Command, limit: Duration) -> std::process::ExitStatus {
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    let mut child = command.spawn().expect("the launcher can be executed");
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status,
            Ok(None) if started.elapsed() > limit => {
                panic!(
                    "the launched game did not exit within {limit:?} (pid {}); left running for inspection",
                    child.id()
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => panic!("could not wait for the launcher: {error}"),
        }
    }
}

#[test]
#[ignore = "exports a game and reads how it keeps its sandbox; the isolation scope runs it"]
fn every_export_keeps_its_sandbox() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-isolation-entitlement-v1",
        "Entitlement Probe",
        platform::fixture_kit(&root),
        "megadrive",
    ));
    platform::assert_keeps_the_sandbox(&app);
    #[cfg(target_os = "macos")]
    {
        let bytes = fs::metadata(app.join("Contents/MacOS/librominabox-launch.dylib"))
            .unwrap()
            .len();
        println!("signed launcher bytes: {bytes}");
        // We build for macOS 11 so that games open on older and Intel Macs.
        // The signed file is then larger than a build of the same sources for
        // a newer macOS, because of the layout for the older target.
        assert!(bytes < 100_000, "the launcher is {bytes} bytes");
    }
}

/// Every library of the exported game is where the game can find it.
///
/// The launch library is beside the executable, signed separately, with an
/// install name of `@executable_path/`. At export we rewrite every dependency
/// to `@executable_path/../Frameworks`, so that a game contains only the
/// libraries it loads. Together, the two would point the player at a missing
/// file, and the player would exit before `main` with a dyld message about a
/// search path.
///
/// We resolve each load command here instead of waiting for dyld, so that we
/// can report a path that leads nowhere as such. On Windows, we refuse to
/// export a program that imports a library Windows does not include
/// (tests/windows_export.rs).
#[cfg(target_os = "macos")]
#[test]
#[ignore = "signs an exported app; the isolation scope runs it"]
fn every_library_the_game_loads_is_inside_the_bundle() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-isolation-loadpath-v1",
        "Load Path Probe",
        platform::fixture_kit(&root),
        "megadrive",
    ));
    let macos = app.join("Contents/MacOS");
    let mut checked = 0;
    let mut missing: Vec<String> = Vec::new();
    for entry in fs::read_dir(&macos).expect("the bundle has a MacOS directory") {
        let object = entry.expect("a readable entry").path();
        if !object.is_file() {
            continue;
        }
        let listed = Command::new("/usr/bin/otool")
            .args(["-L"])
            .arg(&object)
            .output()
            .expect("otool runs");
        for line in String::from_utf8_lossy(&listed.stdout).lines().skip(1) {
            let Some(path) = line.split_whitespace().next() else {
                continue;
            };
            let Some(rest) = path.strip_prefix("@executable_path/") else {
                continue;
            };
            checked += 1;
            // @executable_path is Contents/MacOS, and we resolve both
            // `../Frameworks/x` and `x` from there.
            let mut resolved = macos.clone();
            for part in rest.split('/') {
                if part == ".." {
                    resolved.pop();
                } else if !part.is_empty() && part != "." {
                    resolved.push(part);
                }
            }
            if !resolved.is_file() {
                missing.push(format!(
                    "{} loads {path}, which resolves to {} and is not there",
                    object.file_name().unwrap().to_string_lossy(),
                    resolved.display()
                ));
            }
        }
    }
    assert!(
        checked > 0,
        "no bundle-relative load command was found to check"
    );
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
#[ignore = "launches a probe that exits in the game's sandbox; the isolation scope runs it"]
fn sandboxed_export_cannot_reach_the_host_or_another_game() {
    let root = scratch();
    let kit = platform::fixture_kit(&root);
    platform::use_probe_as_player(&kit);
    let app = export(&request(
        &root,
        b"rominabox-isolation-sandbox-v1",
        "Sandbox Probe",
        kit,
        "megadrive",
    ));
    let other = export(&request(
        &root.join("other"),
        b"rominabox-isolation-other-v1",
        "Other Game",
        platform::fixture_kit(&root.join("other")),
        "megadrive",
    ));
    fs::create_dir_all(root.join("other")).unwrap();
    let identity = identity_of(&app);
    let other_identity = identity_of(&other);
    let _container = platform::sandbox_for(&identity);
    let _other_container = platform::sandbox_for(&other_identity);
    let previous_game = previous_game_folder(&identity);
    if previous_game.exists() {
        let marker = previous_game.join("saves/migrated-marker");
        let ours = fs::read(&marker).ok().as_deref() == Some(b"migrated-from-host\n");
        assert!(
            ours,
            "a game directory for this identity already exists and is not this test's"
        );
        fs::remove_dir_all(&previous_game).unwrap();
    }
    let _previous = GameFolder(previous_game);

    let host = platform::host_file();
    let leak = host.read.clone();
    let leak_before = fs::read(&leak).expect("the host RetroArch history is not there to protect");
    let write_path = host.write.clone();
    let secret = "isolation-secret-marker";
    platform::prepare_storage(&other);
    let other_secret = platform::data_dir_for(&other_identity).join("secret.txt");
    fs::create_dir_all(other_secret.parent().unwrap()).unwrap();
    fs::write(&other_secret, secret).unwrap();

    let previous = previous_game_folder(&identity).join("saves/migrated-marker");
    assert!(
        !previous.parent().unwrap().parent().unwrap().exists(),
        "a game directory for this identity already exists"
    );
    fs::create_dir_all(previous.parent().unwrap()).unwrap();
    fs::write(&previous, b"migrated-from-host\n").unwrap();

    platform::prepare_storage(&app);
    let data = platform::data_dir_for(&identity);
    fs::create_dir_all(&data).unwrap();
    fs::write(
        data.join("controls.cfg"),
        "savefile_directory = \"/tmp/rominabox-isolation-pwned\"\n\
         network_cmd_enable = \"true\"\n\
         audio_volume = \"0.25\"\n\
         pause_nonactive = \"false\"\n\
         input_player1_a = \"x\"\n",
    )
    .unwrap();
    fs::write(
        data.join("volume.cfg"),
        "savefile_directory = \"/tmp/rominabox-isolation-pwned\"\n\
         audio_mute_enable = \"true\"\n\
         audio_volume = \"-17.8\"\n",
    )
    .unwrap();
    let resources = platform::resources_of(&app).join("core-options/probe-core");
    fs::create_dir_all(&resources).unwrap();
    fs::write(resources.join("copied.cfg"), b"copied-from-kit\n").unwrap();
    fs::write(resources.join("kept.cfg"), b"from-kit\n").unwrap();
    fs::create_dir_all(data.join("config/probe-core")).unwrap();
    fs::write(data.join("config/probe-core/kept.cfg"), b"player-copy\n").unwrap();

    platform::assert_keeps_the_sandbox(&app);

    let mut command = Command::new(platform::launcher_of(&app));
    stay_quiet(&mut command);
    command
        .env("ROMINABOX_PROBE_READ", &leak)
        .env("ROMINABOX_PROBE_WRITE", &write_path)
        .env("ROMINABOX_PROBE_OTHER", &other_secret);
    let targets = platform::probe_targets(&mut command);
    let status = run_until(&mut command, Duration::from_secs(20));
    drop(targets);
    let log_path = data.join("logs/launch.log");
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(status.success(), "probe launch failed\n{log}");
    assert!(
        log.contains("READ_DENIED"),
        "the host profile was readable\n{log}"
    );
    assert!(
        log.contains("WRITE_DENIED"),
        "the host profile was writable\n{log}"
    );
    assert!(
        log.contains("OTHER_DENIED"),
        "another game's container was readable\n{log}"
    );
    assert!(!log.contains(secret), "another game's marker leaked\n{log}");
    assert!(
        log.contains("SHM_DENIED"),
        "shared memory was available\n{log}"
    );
    assert!(
        log.contains("UDP_DENIED"),
        "the network command port could be reached\n{log}"
    );
    let home_line = log
        .lines()
        .find(|line| line.starts_with("HOME="))
        .unwrap_or("");
    assert!(
        platform::mentions(home_line, &platform::sandbox_home(&identity)),
        "HOME was not the container\n{log}"
    );
    let tmp_line = log
        .lines()
        .find(|line| line.starts_with("TMPDIR="))
        .unwrap_or("");
    assert!(
        platform::mentions(tmp_line, &platform::container_for(&identity)),
        "TMPDIR was not inside the container\n{log}"
    );
    assert!(
        !write_path.exists(),
        "the probe created a file on the host profile"
    );
    assert_eq!(fs::read(&leak).unwrap(), leak_before);

    let config = fs::read_to_string(data.join("retroarch.cfg")).unwrap();
    let saves = config_value(&config, "savefile_directory").unwrap_or("");
    assert!(
        saves.contains(&identity) && !saves.contains("rominabox-isolation-pwned"),
        "a player file moved the save directory: {saves}"
    );
    assert_eq!(config_value(&config, "network_cmd_enable"), Some("false"));
    // We read the volume the player set from its own file only. The controls
    // file contains no volume, and the volume file contains nothing else.
    assert_eq!(config_value(&config, "audio_volume"), Some("-17.8"));
    assert_eq!(config_value(&config, "input_player1_a"), Some("x"));
    assert_eq!(config_value(&config, "audio_mute_enable"), None);
    assert_eq!(
        fs::read(data.join("config/probe-core/kept.cfg")).unwrap(),
        b"player-copy\n"
    );
    assert_eq!(
        fs::read(data.join("config/probe-core/copied.cfg")).unwrap(),
        b"copied-from-kit\n"
    );
    assert_eq!(
        fs::read(data.join("saves/migrated-marker")).unwrap(),
        b"migrated-from-host\n"
    );
    assert_eq!(fs::read(&previous).unwrap(), b"migrated-from-host\n");
}

/// QUICK SIGN IN: a game exported with achievements can create and use its
/// accounts folder in the per-user application data, and nothing beside it,
/// and cannot open the container of another game. The test makes its own
/// folder, named as in a worktree, so we never touch a player's accounts.
#[test]
#[ignore = "launches a probe that exits in the game's sandbox; the isolation scope runs it"]
fn an_export_with_achievements_reaches_its_accounts_folder_and_nothing_beside_it() {
    const FOLDER: &str = "ROM-in-a-Box Accounts-isolation-test";
    let accounts = platform::user_data().join(FOLDER);
    let beside = platform::user_data().join(format!("{FOLDER}.beside"));
    assert!(
        !accounts.exists() && !beside.exists(),
        "{} or its neighbour is left over from an earlier run",
        accounts.display()
    );
    std::env::set_var("ROMINABOX_ACCOUNTS_FOLDER", FOLDER);

    let root = scratch();
    let kit = platform::fixture_kit(&root);
    platform::use_probe_as_player(&kit);
    let manifest = kit.join("manifest.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["components"][0]["capabilities"] = serde_json::json!({"achievements": true});
    fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    support::copy_tree(&repo_at("integrations/designs"), &kit.join("designs"));
    support::copy_tree(&repo_at("integrations/parts"), &kit.join("parts"));
    support::copy_tree(&repo_at("desktop/assets/controllers"), &kit.join("menu-assets"));
    let mut settings = request(&root, b"rominabox-isolation-accounts-v1", "Accounts Probe", kit, "megadrive");
    settings.show_menu = true;
    settings.include_achievements = true;
    let app = export(&settings);
    let other = export(&request(
        &root.join("other"),
        b"rominabox-isolation-accounts-other-v1",
        "Other Game",
        platform::fixture_kit(&root.join("other")),
        "megadrive",
    ));
    fs::create_dir_all(root.join("other")).unwrap();
    let identity = identity_of(&app);
    let other_identity = identity_of(&other);
    let _container = platform::sandbox_for(&identity);
    let _other_container = platform::sandbox_for(&other_identity);
    platform::prepare_storage(&other);
    let other_secret = platform::data_dir_for(&other_identity).join("secret.txt");
    fs::create_dir_all(other_secret.parent().unwrap()).unwrap();
    fs::write(&other_secret, "isolation-secret-marker").unwrap();
    fs::write(&beside, b"beside").unwrap();

    assert!(
        platform::names_accounts_folder(&app, FOLDER),
        "the export does not name its accounts folder"
    );

    let mut command = Command::new(platform::launcher_of(&app));
    stay_quiet(&mut command);
    command
        .env("ROMINABOX_PROBE_OTHER", &other_secret)
        .env("ROMINABOX_PROBE_BESIDE", &beside);
    let targets = platform::probe_targets(&mut command);
    let status = run_until(&mut command, Duration::from_secs(20));
    drop(targets);
    let log = fs::read_to_string(platform::data_dir_for(&identity).join("logs/launch.log")).unwrap_or_default();

    // Remove only what this test made, after checking its name.
    fs::remove_file(&beside).ok();
    if accounts.is_dir() && accounts.file_name().and_then(|name| name.to_str()) == Some(FOLDER) {
        fs::remove_dir_all(&accounts).unwrap();
    }
    std::env::remove_var("ROMINABOX_ACCOUNTS_FOLDER");

    assert!(status.success(), "probe launch failed\n{log}");
    assert!(log.contains("ACCOUNTS_ALLOWED"), "the accounts folder was not usable\n{log}");
    assert!(log.contains("BESIDE_DENIED"), "a file beside the accounts folder was readable\n{log}");
    assert!(log.contains("OTHER_DENIED"), "another game's container was readable\n{log}");
}

/// The author left background play off, and the controls.cfg of an older
/// export of the same game already has `pause_nonactive = "false"`. In a
/// screenshot run we need the console to keep running, and we must set that
/// at launch, without writing to the player's file.
#[test]
#[ignore = "launches a stub that exits in the game's sandbox; the isolation scope runs it"]
fn author_background_play_survives_an_old_controls_file() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-background-play-author-v1",
        "Background Play",
        platform::fixture_kit(&root),
        "megadrive",
    ));
    let identity = identity_of(&app);
    let _container = platform::sandbox_for(&identity);
    platform::prepare_storage(&app);
    let data = platform::data_dir_for(&identity);
    fs::create_dir_all(&data).unwrap();
    let controls = data.join("controls.cfg");
    let leftover = "pause_nonactive = \"false\"\n";
    fs::write(&controls, leftover).unwrap();

    platform::assert_keeps_the_sandbox(&app);
    // Launch as a person would, because we never pause an automated run in
    // the background, whatever the author chose. The stub has no sound.
    let mut person = Command::new(platform::launcher_of(&app));
    person
        .env_remove("ROMINABOX_QUIET")
        .env("ROMINABOX_SOUND", "1")
        .env_remove("ROMINABOX_MENU_SHOT");
    let status = run_until(&mut person, Duration::from_secs(20));
    assert!(status.success(), "the stub did not exit");
    let config = fs::read_to_string(data.join("retroarch.cfg")).unwrap();
    assert_eq!(
        config_value(&config, "pause_nonactive"),
        Some("true"),
        "an older controls.cfg turned background play on"
    );
    assert_eq!(
        fs::read_to_string(&controls).unwrap(),
        leftover,
        "the launch rewrote the player's controls.cfg"
    );

    let pad = "input_player1_a = \"x\"\n";
    fs::write(&controls, pad).unwrap();
    let mut shot = Command::new(platform::launcher_of(&app));
    stay_quiet(&mut shot);
    shot.env("ROMINABOX_MENU_SHOT", "/tmp/rominabox-menu-shot-proof.png");
    let status = run_until(&mut shot, Duration::from_secs(20));
    assert!(status.success(), "the screenshot stub did not exit");
    let shooting = fs::read_to_string(data.join("retroarch.cfg")).unwrap();
    assert_eq!(
        config_value(&shooting, "pause_nonactive"),
        Some("false"),
        "a screenshot run still pauses when its window is not focused"
    );
    assert_eq!(
        fs::read_to_string(&controls).unwrap(),
        pad,
        "the screenshot run wrote pause_nonactive into the player's controls.cfg"
    );
}

/// Without the signature there is no sandbox, and `$HOME` is whatever we set
/// at launch. With HOME at the account, we would create the game's
/// `Games/<identity>` folder there. So we set HOME for the stub to a scratch
/// directory that we remove with the test. On Windows we cannot give an
/// unsandboxed launch another per-user folder, so this test is macOS only.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "launches an unsigned stub; the isolation scope runs it"]
fn an_unsandboxed_launch_does_not_write_the_account_game_directory() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-background-play-author-v1",
        "Background Play",
        platform::fixture_kit(&root),
        "megadrive",
    ));
    let identity = identity_of(&app);
    let _container = platform::sandbox_for(&identity);
    let host = previous_game_folder(&identity).join("retroarch.cfg");
    let before = fs::metadata(&host)
        .ok()
        .map(|info| (info.len(), info.modified().ok()));
    let executable = platform::launcher_of(&app);
    let removed = Command::new("/usr/bin/codesign")
        .args(["--remove-signature"])
        .arg(&executable)
        .status()
        .expect("codesign can be executed");
    assert!(removed.success(), "could not drop the signature");
    let launch_home = rominabox_scratch::Scratch::dir("rominabox-isolation-home");
    let mut command = Command::new(&executable);
    stay_quiet(&mut command);
    let output = command
        .env_remove("ROMINABOX_MENU_SHOT")
        .env("HOME", launch_home.path())
        .output()
        .expect("the unsigned launcher can be executed");
    let after = fs::metadata(&host)
        .ok()
        .map(|info| (info.len(), info.modified().ok()));
    assert_eq!(
        before,
        after,
        "an unsandboxed launch wrote {}\n{}",
        host.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "the unsigned stub did not exit\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = launch_home
        .join("Library/Application Support/ROM-in-a-Box/Games")
        .join(&identity)
        .join("retroarch.cfg");
    assert!(
        written.is_file(),
        "the unsandboxed launch did not write under {}",
        launch_home.display()
    );
}

/// Export the generated cartridge from the builder's kit and the core cache,
/// and return the export and the folder we export it into.
fn the_test_cartridge() -> (rominabox_scratch::Scratch, ExportRequest) {
    let rom = repo_at("scripts/fixtures/test-game.gbc");
    assert!(
        rom.is_file(),
        "scripts/fixtures/test-game.gbc is not in this checkout"
    );
    let kit = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime");
    let root = scratch();
    fs::copy(&rom, root.join("game.bin")).unwrap();
    let mut settings = request(&root, b"", "Sandbox Game", kit, "gbc");
    // The kit contains no cores, so at export we take them from a core cache:
    // the one for the developer core source named in scripts/test.py, or else
    // the seeded one (scripts/core_source.py).
    let target = rominabox_desktop::target::Target::host().expect("the host platform is one the builder builds for");
    settings.core_cache = Some(
        std::env::var_os("ROMINABOX_CORE_SOURCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_at(&format!("work/core-cache/{}", target.key()))),
    );
    settings.rom = rom;
    (root, settings)
}

#[test]
#[ignore = "runs an exported core for a few frames, then exits"]
fn exported_game_loads_a_core_stays_quiet_and_sees_a_gamepad() {
    let (_root, settings) = the_test_cartridge();
    loads_a_core_stays_quiet_and_sees_a_gamepad(&export(&settings));
}

/// When we make a Mac game on Windows, where files have no Unix modes, we
/// write it into a zip with those modes. After an unpack as on a Mac
/// (`ditto -x -k`, as in Archive Utility), the game passes a deep and strict
/// verification, still has its sandbox and runs.
#[test]
#[cfg(target_os = "macos")]
#[ignore = "runs an exported core for a few frames, then exits"]
fn a_zipped_mac_game_unzips_verifies_and_runs() {
    let (root, mut settings) = the_test_cartridge();
    settings.zip = Some(true);
    let zip = export(&settings);
    assert_eq!(zip.file_name().and_then(|name| name.to_str()), Some("Sandbox Game.zip"));
    assert_eq!(fs::read_dir(&settings.output_dir).unwrap().count(), 1, "the zip is all an export leaves");
    let unzipped = root.join("unzipped");
    let status = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&zip).arg(&unzipped).status().unwrap();
    assert!(status.success());
    let app = unzipped.join("Sandbox Game.app");
    let verified = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict", "-vv"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(verified.status.success(), "{}", String::from_utf8_lossy(&verified.stderr));
    loads_a_core_stays_quiet_and_sees_a_gamepad(&app);
}

fn loads_a_core_stays_quiet_and_sees_a_gamepad(app: &Path) {
    let identity = identity_of(app);
    let _container = platform::sandbox_for(&identity);

    platform::assert_keeps_the_sandbox(&app);

    #[cfg(target_os = "macos")]
    let leak = platform::home().join("Documents/RetroArch/playlists/builtin/content_history.lpl");
    #[cfg(target_os = "macos")]
    let leak_before = fs::read(&leak).unwrap_or_default();
    let mut command = Command::new(platform::launcher_of(&app));
    stay_quiet(&mut command);
    command.env("ROMINABOX_VERBOSE", "1");
    command.env("ROMINABOX_MAX_FRAMES", "30");
    // The window is not focused, and the author chose to pause then, so the
    // console would stop before we log these lines. We set the screenshot
    // variable to keep it running without writing the player's controls.cfg.
    command.env("ROMINABOX_MENU_SHOT", "/tmp/rominabox-menu-shot-proof.png");
    // We append to this log, so it may have sound device lines from an
    // earlier screenshot run of the same ROM, and a quiet run could look loud.
    let log_path = platform::data_dir_for(&identity).join("logs/launch.log");
    let _ = fs::remove_file(&log_path);
    let status = run_until(&mut command, Duration::from_secs(60));
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    let tail = log
        .lines()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        status.success(),
        "the game did not exit cleanly ({status})\n{tail}"
    );
    assert!(
        log.contains("[Core] Loading dynamic libretro core from:"),
        "a core did not load\n{tail}"
    );
    assert!(
        !log.contains(platform::AUDIO_DEVICE_LOG),
        "a quiet run opened the sound device\n{tail}"
    );
    // A test game that runs as a regular app appears in the Dock, so in a
    // quiet run we launch it as an accessory app.
    match platform::QUIET_WINDOW_LOG {
        Some(line) => assert!(log.contains(line), "a quiet run took a Dock icon\n{tail}"),
        None => eprintln!("this player logs nothing for its quiet window: the quiet scope reads the window itself"),
    }
    let written =
        fs::read_to_string(platform::data_dir_for(&identity).join("retroarch.cfg")).unwrap_or_default();
    assert_eq!(
        config_value(&written, "audio_driver"),
        Some("null"),
        "a quiet run left a device driver in the config\n{written}"
    );
    assert_eq!(config_value(&written, "audio_enable"), Some("false"));
    // We can see a gamepad only when one is plugged in, so say when none is.
    if gamepad_connected() {
        assert!(
            log.contains(platform::GAMEPAD_LOG),
            "a gamepad was not seen\n{tail}"
        );
    } else {
        eprintln!("no gamepad is connected: whether the game sees one was not checked");
    }
    #[cfg(target_os = "macos")]
    if leak.is_file() {
        assert_eq!(fs::read(&leak).unwrap(), leak_before);
    }
}
