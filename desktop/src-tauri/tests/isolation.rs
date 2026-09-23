//! The exported game runs in a sandbox, and with these tests we would notice
//! if the sandbox were missing.
//!
//! We mark them ignored so that the exporter tests do not launch anything,
//! and we run them in the isolation scope.

#![cfg(target_os = "macos")]

use rominabox_desktop::packaging::{ExportRequest, ExportTarget};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

struct RemoveDir(PathBuf);

impl Drop for RemoveDir {
    fn drop(&mut self) {
        let Some(name) = self.0.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        if !self.0.is_dir() {
            return;
        }
        let container = name.starts_with("app.rominabox.game.")
            && self
                .0
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|parent| parent.to_str())
                == Some("Containers");
        let game = name.len() == 24
            && name.chars().all(|character| character.is_ascii_hexdigit())
            && self
                .0
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|parent| parent.to_str())
                == Some("Games");
        if container || game {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        if self.0.file_name().and_then(|name| name.to_str()) != Some("rominabox-isolation-probe") {
            return;
        }
        if self
            .0
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|parent| parent.to_str())
            != Some("builtin")
        {
            return;
        }
        let _ = fs::remove_file(&self.0);
    }
}

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

fn stay_quiet(command: &mut Command) {
    // ROMINABOX_QUIET_ENV in the launcher. Without it the game uses CoreAudio.
    // In the quiet tests we read that define and compare it with this name.
    command.env("ROMINABOX_QUIET", "1");
}

/// A path in the checkout of this run, not in the checkout where the binary
/// was built. Several checkouts can share one cargo target, so they can differ.
fn repo_at(relative: &str) -> PathBuf {
    rominabox_desktop::repo::at(relative)
}

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-isolation")
}

fn fixture_kit(root: &Path) -> PathBuf {
    let kit = root.join("runtime-kit");
    fs::create_dir_all(kit.join("bin")).unwrap();
    fs::create_dir_all(kit.join("cores")).unwrap();
    fs::create_dir_all(kit.join("Frameworks")).unwrap();
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

fn request(root: &Path, rom_bytes: &[u8], title: &str, kit: PathBuf, system: &str) -> ExportRequest {
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
        firmware: Vec::new(),
        splash: false,
        advanced_emulator_access: false,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: rominabox_desktop::shaders::ShaderSelection::default(),
        achievements: Default::default(),
        output_dir: root.join("out"),
        target: ExportTarget::Macos,
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
        serde_json::from_slice(&fs::read(app.join("Contents/Resources/game.json")).unwrap())
            .unwrap();
    manifest["identity"].as_str().unwrap().to_string()
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap())
}

fn container_for(identity: &str) -> PathBuf {
    home()
        .join("Library/Containers")
        .join(format!("app.rominabox.game.{identity}"))
}

fn data_dir_for(identity: &str) -> PathBuf {
    container_for(identity)
        .join("Data/Library/Application Support/ROM-in-a-Box/Games")
        .join(identity)
}

fn codesign_text(path: &Path) -> String {
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "--entitlements", "-"])
        .arg(path)
        .output()
        .expect("codesign can be executed");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_main_executable_keeps_the_sandbox(app: &Path) {
    let plist = fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
    let marker = "<key>CFBundleExecutable</key><string>";
    let start = plist.find(marker).expect("the bundle names an executable");
    let rest = &plist[start + marker.len()..];
    let name = rest.split('<').next().unwrap();
    let executable = app.join("Contents/MacOS").join(name);
    let mut magic = [0u8; 4];
    fs::File::open(&executable)
        .unwrap()
        .read_exact(&mut magic)
        .unwrap();
    let mach_o = magic == [0xcf, 0xfa, 0xed, 0xfe]
        || magic == [0xfe, 0xed, 0xfa, 0xcf]
        || magic == [0xca, 0xfe, 0xba, 0xbe];
    assert!(mach_o, "main executable is a script");
    let signed = codesign_text(&executable);
    assert!(
        signed.contains("com.apple.security.app-sandbox"),
        "entitlements were dropped\n{signed}"
    );
    let library = codesign_text(&app.join("Contents/MacOS/librominabox-launch.dylib"));
    assert!(
        !library.contains("com.apple.security.app-sandbox"),
        "the launcher library carries the sandbox entitlement\n{library}"
    );
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
                let _ = child.kill();
                let _ = child.wait();
                panic!("the launched game did not exit");
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => panic!("could not wait for the launcher: {error}"),
        }
    }
}

#[test]
#[ignore = "signs an exported app; the isolation scope runs it"]
fn signed_export_keeps_the_sandbox_entitlement() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-isolation-entitlement-v1",
        "Entitlement Probe",
        fixture_kit(&root),
        "megadrive",
    ));
    assert_main_executable_keeps_the_sandbox(&app);
    let bytes = fs::metadata(app.join("Contents/MacOS/librominabox-launch.dylib"))
        .unwrap()
        .len();
    println!("signed launcher bytes: {bytes}");
    assert!(bytes < 80_000, "the launcher is {bytes} bytes");
}

/// Every library that the exported game loads is where the game can find it.
///
/// The launch library is beside the executable, signed separately, with an
/// install name of `@executable_path/`. In the export we rewrite every
/// dependency to `@executable_path/../Frameworks`, so a game contains only
/// the libraries it loads. When a load command points at a missing file, the
/// player exits in dyld before `main`, with a message about a search path.
///
/// We resolve each load command here instead of waiting for dyld, so that we
/// can report a path that leads nowhere by name.
#[test]
#[ignore = "signs an exported app; the isolation scope runs it"]
fn every_library_the_game_loads_is_inside_the_bundle() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-isolation-loadpath-v1",
        "Load Path Probe",
        fixture_kit(&root),
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
    assert!(checked > 0, "no bundle-relative load command was found to check");
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
#[ignore = "launches a signed probe that exits; the isolation scope runs it"]
fn sandboxed_export_cannot_reach_the_host_or_another_game() {
    let root = scratch();
    let kit = fixture_kit(&root);
    let probe_source = repo_at("scripts/native_runtime/sandbox_probe.c");
    let status = Command::new("cc")
        .args(["-Oz", "-Wl,-headerpad_max_install_names", "-o"])
        .arg(kit.join("bin/retroarch"))
        .arg(&probe_source)
        .status()
        .unwrap();
    assert!(status.success(), "the sandbox probe failed to compile");
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
        fixture_kit(&root.join("other")),
        "megadrive",
    ));
    fs::create_dir_all(root.join("other")).unwrap();
    let identity = identity_of(&app);
    let other_identity = identity_of(&other);
    let _container = RemoveDir(container_for(&identity));
    let _other_container = RemoveDir(container_for(&other_identity));
    let previous_game = home()
        .join("Library/Application Support/ROM-in-a-Box/Games")
        .join(&identity);
    if previous_game.exists() {
        let marker = previous_game.join("saves/migrated-marker");
        let ours = fs::read(&marker).ok().as_deref() == Some(b"migrated-from-host\n");
        assert!(
            ours,
            "a game directory for this identity already exists and is not this test's"
        );
        fs::remove_dir_all(&previous_game).unwrap();
    }
    let _previous = RemoveDir(previous_game);

    let leak = home().join("Documents/RetroArch/playlists/builtin/content_history.lpl");
    let leak_before = fs::read(&leak).expect("the host RetroArch history is not there to protect");
    let write_path = leak
        .parent()
        .unwrap()
        .join("rominabox-isolation-probe");
    let _written = RemoveFile(write_path.clone());
    let secret = "isolation-secret-marker";
    let other_secret = data_dir_for(&other_identity).join("secret.txt");
    fs::create_dir_all(other_secret.parent().unwrap()).unwrap();
    fs::write(&other_secret, secret).unwrap();

    let previous = home()
        .join("Library/Application Support/ROM-in-a-Box/Games")
        .join(&identity)
        .join("saves/migrated-marker");
    assert!(
        !previous.parent().unwrap().parent().unwrap().exists(),
        "a game directory for this identity already exists"
    );
    fs::create_dir_all(previous.parent().unwrap()).unwrap();
    fs::write(&previous, b"migrated-from-host\n").unwrap();

    let data = data_dir_for(&identity);
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
         audio_mute_enable = \"true\"\n",
    )
    .unwrap();
    let resources = app.join("Contents/Resources/core-options/probe-core");
    fs::create_dir_all(&resources).unwrap();
    fs::write(resources.join("copied.cfg"), b"copied-from-kit\n").unwrap();
    fs::write(resources.join("kept.cfg"), b"from-kit\n").unwrap();
    fs::create_dir_all(data.join("config/probe-core")).unwrap();
    fs::write(data.join("config/probe-core/kept.cfg"), b"player-copy\n").unwrap();

    assert_main_executable_keeps_the_sandbox(&app);

    let mut command = Command::new(app.join("Contents/MacOS/retroarch"));
    stay_quiet(&mut command);
    command
        .env("ROMINABOX_PROBE_READ", &leak)
        .env("ROMINABOX_PROBE_WRITE", &write_path)
        .env("ROMINABOX_PROBE_OTHER", &other_secret);
    let status = run_until(&mut command, Duration::from_secs(20));
    let log_path = data.join("logs/launch.log");
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(status.success(), "probe launch failed\n{log}");
    assert!(log.contains("READ_DENIED"), "the host profile was readable\n{log}");
    assert!(log.contains("WRITE_DENIED"), "the host profile was writable\n{log}");
    assert!(
        log.contains("OTHER_DENIED"),
        "another game's container was readable\n{log}"
    );
    assert!(!log.contains(secret), "another game's marker leaked\n{log}");
    assert!(log.contains("SHM_DENIED"), "shared memory was available\n{log}");
    assert!(log.contains("UDP_DENIED"), "the network command port was bindable\n{log}");
    let container = container_for(&identity);
    let home_line = log
        .lines()
        .find(|line| line.starts_with("HOME="))
        .unwrap_or("");
    assert!(
        home_line.contains(container.join("Data").to_str().unwrap()),
        "HOME was not the container\n{log}"
    );
    let tmp_line = log
        .lines()
        .find(|line| line.starts_with("TMPDIR="))
        .unwrap_or("");
    assert!(
        tmp_line.contains(container.to_str().unwrap()),
        "TMPDIR was not inside the container\n{log}"
    );
    assert!(!write_path.exists(), "the probe created a file on the host profile");
    assert_eq!(fs::read(&leak).unwrap(), leak_before);

    let config = fs::read_to_string(data.join("retroarch.cfg")).unwrap();
    let saves = config_value(&config, "savefile_directory").unwrap_or("");
    assert!(
        saves.contains(&identity) && !saves.contains("rominabox-isolation-pwned"),
        "a player file moved the save directory: {saves}"
    );
    assert_eq!(config_value(&config, "network_cmd_enable"), Some("false"));
    assert_eq!(config_value(&config, "audio_volume"), Some("0.25"));
    assert_eq!(
        config_value(&config, "pause_nonactive"),
        Some("true"),
        "an older controls.cfg must not turn background play on when the author left it off"
    );
    assert_eq!(config_value(&config, "input_player1_a"), Some("x"));
    assert_eq!(config_value(&config, "audio_mute_enable"), Some("true"));
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

/// The author left background play off, and an earlier export of the same game
/// left `pause_nonactive = "false"` in its controls.cfg. For a screenshot run
/// the console must keep running, and we must set that at launch, not by
/// writing to the file of the player.
#[test]
#[ignore = "launches a signed stub that exits; the isolation scope runs it"]
fn author_background_play_survives_an_old_controls_file() {
    let root = scratch();
    let app = export(&request(
        &root,
        b"rominabox-background-play-author-v1",
        "Background Play",
        fixture_kit(&root),
        "megadrive",
    ));
    let identity = identity_of(&app);
    let _container = RemoveDir(container_for(&identity));
    let data = data_dir_for(&identity);
    fs::create_dir_all(&data).unwrap();
    let controls = data.join("controls.cfg");
    let leftover = "pause_nonactive = \"false\"\n";
    fs::write(&controls, leftover).unwrap();

    let mut quiet = Command::new(app.join("Contents/MacOS/retroarch"));
    stay_quiet(&mut quiet);
    quiet.env_remove("ROMINABOX_MENU_SHOT");
    let status = run_until(&mut quiet, Duration::from_secs(20));
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
    let mut shot = Command::new(app.join("Contents/MacOS/retroarch"));
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

#[test]
#[ignore = "runs an exported core for a few frames, then exits"]
fn exported_game_loads_a_core_stays_quiet_and_sees_a_gamepad() {
    let rom = repo_at("work/test-game.gbc");
    assert!(rom.is_file(), "work/test-game.gbc is not in this checkout");
    let kit = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime");
    let root = scratch();
    fs::copy(&rom, root.join("game.bin")).unwrap();
    let mut settings = request(
        &root,
        b"",
        "Sandbox Game",
        kit,
        "gbc",
    );
    settings.rom = rom;
    let app = export(&settings);
    let identity = identity_of(&app);
    let _container = RemoveDir(container_for(&identity));

    assert_main_executable_keeps_the_sandbox(&app);

    let leak = home().join("Documents/RetroArch/playlists/builtin/content_history.lpl");
    let leak_before = fs::read(&leak).unwrap_or_default();
    let mut command = Command::new(app.join("Contents/MacOS/retroarch"));
    stay_quiet(&mut command);
    command.env("ROMINABOX_VERBOSE", "1");
    command.env("ROMINABOX_MAX_FRAMES", "30");
    // The window is not focused, and the author chose to pause then, so the
    // console would stop before we log these lines. We set the screenshot
    // variable to keep it running without writing the player's controls.cfg.
    command.env(
        "ROMINABOX_MENU_SHOT",
        "/tmp/rominabox-menu-shot-proof.png",
    );
    // We append to this log in the launcher. An earlier shot of this ROM put
    // [CoreAudio] in this file, so a quiet run would still look loud.
    let log_path = data_dir_for(&identity).join("logs/launch.log");
    let _ = fs::remove_file(&log_path);
    let status = run_until(&mut command, Duration::from_secs(60));
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    let tail = log.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    assert!(
        status.success(),
        "the game did not exit cleanly ({status})\n{tail}"
    );
    assert!(
        log.contains("[Core] Loading dynamic libretro core from:"),
        "a core did not load\n{tail}"
    );
    assert!(
        !log.contains("[CoreAudio]"),
        "a quiet run opened CoreAudio\n{tail}"
    );
    // A test game that runs as a regular app appears in the Dock, so in a
    // quiet run we launch it as an accessory app.
    assert!(
        log.contains("[RIB] quiet activation accessory"),
        "a quiet run took a Dock icon\n{tail}"
    );
    let written = fs::read_to_string(data_dir_for(&identity).join("retroarch.cfg")).unwrap_or_default();
    assert_eq!(
        config_value(&written, "audio_driver"),
        Some("null"),
        "a quiet run left a device driver in the config\n{written}"
    );
    assert_eq!(config_value(&written, "audio_enable"), Some("false"));
    assert!(
        log.contains("[IOHID] Port "),
        "a gamepad was not seen\n{tail}"
    );
    if leak.is_file() {
        assert_eq!(fs::read(&leak).unwrap(), leak_before);
    }
}
