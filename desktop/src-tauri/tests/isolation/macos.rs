//! The macOS part of the isolation tests: the stand-in kit, how we start a
//! game, where a game's data is under the App Sandbox (its container), and
//! how we tell that an export has the sandbox (from the signature of the
//! main executable).

use rominabox_desktop::packaging::ExportTarget;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

pub const TARGET: ExportTarget = ExportTarget::Macos;
/// The log line for an opened sound device.
pub const AUDIO_DEVICE_LOG: &str = "[CoreAudio]";
/// The log line for a gamepad that the game can see.
pub const GAMEPAD_LOG: &str = "[IOHID] Port ";
/// The log line when we keep a quiet game out of the Dock.
pub const QUIET_WINDOW_LOG: Option<&str> = Some("[RIB] quiet activation accessory");

/// A stand-in kit with the shipped launch library attached to its player,
/// as we attach it when we make a kit.
pub fn fixture_kit(root: &Path) -> PathBuf {
    let kit = crate::export_fixture::fixture_kit(root);
    crate::export_fixture::attach_real_launcher(&kit);
    kit
}

/// The sandbox probe in the kit where the player would be.
pub fn use_probe_as_player(kit: &Path) {
    let status = Command::new("cc")
        .args(["-Oz", "-Wl,-headerpad_max_install_names", "-o"])
        .arg(kit.join("bin/retroarch"))
        .arg(crate::repo_at("scripts/native_runtime/sandbox_probe.c"))
        .status()
        .unwrap();
    assert!(status.success(), "the sandbox probe failed to compile");
    crate::export_fixture::attach_real_launcher(kit);
}

/// The program a person opens.
pub fn launcher_of(app: &Path) -> PathBuf {
    app.join("Contents/MacOS/retroarch")
}

pub fn resources_of(app: &Path) -> PathBuf {
    app.join("Contents/Resources")
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap())
}

/// The per-user application data outside any sandbox.
pub fn user_data() -> PathBuf {
    home().join("Library/Application Support")
}

pub fn container_for(identity: &str) -> PathBuf {
    home()
        .join("Library/Containers")
        .join(format!("app.rominabox.game.{identity}"))
}

/// Where the sandboxed game's HOME is.
pub fn sandbox_home(identity: &str) -> PathBuf {
    container_for(identity).join("Data")
}

pub fn data_dir_for(identity: &str) -> PathBuf {
    sandbox_home(identity)
        .join("Library/Application Support/ROM-in-a-Box/Games")
        .join(identity)
}

/// Do nothing, because we put files in a game's container before its first
/// launch, and on macOS the files stay there.
pub fn prepare_storage(_app: &Path) {}

/// Whether `text` names `path`.
pub fn mentions(text: &str, path: &Path) -> bool {
    text.contains(path.to_str().unwrap())
}

/// A container that the test made, which we remove when the test ends.
pub struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let Some(name) = self.0.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let container = name.starts_with("app.rominabox.game.")
            && self.0.parent().and_then(|parent| parent.file_name()).and_then(|parent| parent.to_str())
                == Some("Containers");
        if container && self.0.is_dir() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub fn sandbox_for(identity: &str) -> Sandbox {
    Sandbox(container_for(identity))
}

/// A file on the host that a game must not read, and a place beside it where
/// it must not write: the history of the RetroArch installed on the host,
/// which must exist for the check to apply.
pub struct HostFile {
    pub read: PathBuf,
    pub write: PathBuf,
}

impl Drop for HostFile {
    fn drop(&mut self) {
        let named = self.write.file_name().and_then(|name| name.to_str()) == Some("rominabox-isolation-probe")
            && self.write.parent().and_then(|parent| parent.file_name()).and_then(|parent| parent.to_str())
                == Some("builtin");
        if named {
            let _ = fs::remove_file(&self.write);
        }
    }
}

pub fn host_file() -> HostFile {
    let read = home().join("Documents/RetroArch/playlists/builtin/content_history.lpl");
    let write = read.parent().unwrap().join("rominabox-isolation-probe");
    HostFile { read, write }
}

/// What we give the probe to try to reach while it runs: nothing on macOS,
/// where we make the shared memory inside the probe, and where a sandboxed
/// process may not bind the command port.
pub struct ProbeTargets;

pub fn probe_targets(_command: &mut Command) -> ProbeTargets {
    ProbeTargets
}

pub fn codesign_text(path: &Path) -> String {
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

pub fn assert_keeps_the_sandbox(app: &Path) {
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

/// Return whether the exported game can reach the QUICK SIGN IN `folder`.
pub fn names_accounts_folder(app: &Path, folder: &str) -> bool {
    codesign_text(app).contains(&format!("/Library/Application Support/{folder}/"))
}
