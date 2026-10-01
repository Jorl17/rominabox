//! The Windows part of the isolation tests: the stand-in kit with the shipped
//! launcher, how we start a game, where a game's data is in its sandbox (an
//! AppContainer), and how we tell that an export has the sandbox (from its
//! launch plan, because we set up the sandbox in the launcher).

use rominabox_engine::packaging::ExportTarget;
use std::{
    fs,
    net::UdpSocket,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub const TARGET: ExportTarget = ExportTarget::Windows;
/// The log line for an opened sound device.
pub const AUDIO_DEVICE_LOG: &str = "[WASAPI]";
/// The log line when we open a gamepad in the joypad driver, after reading it
/// in the launcher outside the sandbox. The RetroArch line comes only with the
/// controller notification, which we turn off in games.
pub const GAMEPAD_LOG: &str = "[RIB] Controller \"";
/// There is no log line for the quiet window on Windows, so in the quiet
/// tests we read the window itself.
pub const QUIET_WINDOW_LOG: Option<&str> = None;

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileMappingW(file: isize, attributes: *const u8, protect: u32, high: u32, low: u32, name: *const u16)
        -> isize;
    fn CloseHandle(handle: isize) -> i32;
}

#[link(name = "userenv")]
extern "system" {
    fn DeleteAppContainerProfile(name: *const u16) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The stand-in kit with the shipped launcher, because we set up the sandbox
/// in the launcher.
pub fn fixture_kit(root: &Path) -> PathBuf {
    let kit = crate::export_fixture::windows_kit(root);
    let build = root.join("launcher-build");
    let built = Command::new(crate::support::python())
        .arg(crate::repo_at("scripts/build_launcher.py"))
        .arg(&build)
        .output()
        .expect("the launcher build runs");
    let printed = String::from_utf8_lossy(&built.stdout);
    assert!(
        built.status.success(),
        "the launcher did not build\n{printed}\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let launcher = PathBuf::from(printed.lines().last().expect("the build names its launcher").trim());
    fs::copy(&launcher, kit.join("bin/launcher.exe")).unwrap();
    kit
}

/// The sandbox probe in the kit where the player would be.
pub fn use_probe_as_player(kit: &Path) {
    let status = Command::new("cc")
        .args(["-O2", "-o"])
        .arg(kit.join("bin/retroarch.exe"))
        .arg(crate::repo_at("scripts/native_runtime/sandbox_probe.c"))
        .arg("-lws2_32")
        .status()
        .unwrap();
    assert!(status.success(), "the sandbox probe failed to compile");
}

/// The program a person opens.
pub fn launcher_of(app: &Path) -> PathBuf {
    app.join(format!("{}.exe", app.file_name().unwrap().to_string_lossy()))
}

pub fn resources_of(app: &Path) -> PathBuf {
    app.join("Resources")
}

/// The per-user application data outside any sandbox.
pub fn user_data() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap())
}

/// The name of the sandbox, as we make it in the launcher: the application
/// id prefix declared in the launcher and the player, then the game's identity.
fn sandbox_name(identity: &str) -> String {
    let header = fs::read_to_string(crate::repo_at("vendor/retroarch/rominabox_launch.h")).unwrap();
    let prefix = header
        .lines()
        .find_map(|line| line.strip_prefix("#define RIB_GAME_APP_ID_PREFIX \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("rominabox_launch.h declares the application id prefix");
    format!("{prefix}{identity}")
}

pub fn container_for(identity: &str) -> PathBuf {
    user_data().join("Packages").join(sandbox_name(identity))
}

/// Where the sandboxed game's per-user folder is.
pub fn sandbox_home(identity: &str) -> PathBuf {
    container_for(identity).join("AC")
}

pub fn data_dir_for(identity: &str) -> PathBuf {
    sandbox_home(identity).join("ROM-in-a-Box/Games").join(identity)
}

/// Make the game's sandbox and data folder as we do at its first launch, so
/// that the test can put files in them. Registering a sandbox for the first
/// time over a folder empties that folder.
pub fn prepare_storage(app: &Path) {
    let status = Command::new(launcher_of(app))
        .env("ROMINABOX_PLAN_ONLY", "1")
        .env("ROMINABOX_QUIET", "1")
        .stdin(std::process::Stdio::null())
        .status()
        .expect("the launcher can be executed");
    assert!(status.success(), "the plan-only launch did not prepare the game's storage");
}

/// Return whether `text` contains `path`, ignoring case, because the case of
/// a sandbox folder name varies on Windows.
pub fn mentions(text: &str, path: &Path) -> bool {
    text.to_lowercase().contains(&path.to_str().unwrap().to_lowercase())
}

/// A sandbox that we registered for the test's game, which we remove with its
/// folder when the test ends.
pub struct Sandbox(String);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let name = wide(&self.0);
        unsafe {
            DeleteAppContainerProfile(name.as_ptr());
        }
        // A folder that the test made where the folder of an unregistered
        // sandbox would be.
        let folder = user_data().join("Packages").join(&self.0);
        if folder.is_dir() && folder.file_name().and_then(|name| name.to_str()) == Some(self.0.as_str()) {
            let _ = fs::remove_dir_all(&folder);
        }
    }
}

pub fn sandbox_for(identity: &str) -> Sandbox {
    Sandbox(sandbox_name(identity))
}

/// A file on the host that a game must not read, and a place beside it where
/// it must not write: a folder that this test makes in the per-user
/// application data, outside every sandbox, and removes.
pub struct HostFile {
    pub read: PathBuf,
    pub write: PathBuf,
}

const HOST_FOLDER: &str = "ROM-in-a-Box isolation host";

impl Drop for HostFile {
    fn drop(&mut self) {
        let folder = self.read.parent().unwrap();
        if folder.file_name().and_then(|name| name.to_str()) == Some(HOST_FOLDER)
            && folder.parent() == Some(user_data().as_path())
        {
            let _ = fs::remove_dir_all(folder);
        }
    }
}

pub fn host_file() -> HostFile {
    let folder = user_data().join(HOST_FOLDER);
    assert!(!folder.exists(), "{} is left over from an earlier run", folder.display());
    fs::create_dir_all(&folder).unwrap();
    let read = folder.join("content_history.lpl");
    fs::write(&read, b"host history\n").unwrap();
    HostFile { read, write: folder.join("rominabox-isolation-probe") }
}

/// Shared memory that we make on the host, and datagrams that we send to the
/// network command port while the probe runs.
pub struct ProbeTargets {
    mapping: isize,
    sending: Arc<AtomicBool>,
    sender: Option<thread::JoinHandle<()>>,
}

impl Drop for ProbeTargets {
    fn drop(&mut self) {
        self.sending.store(false, Ordering::Relaxed);
        if let Some(sender) = self.sender.take() {
            let _ = sender.join();
        }
        unsafe {
            CloseHandle(self.mapping);
        }
    }
}

pub fn probe_targets(command: &mut Command) -> ProbeTargets {
    const SHARED: &str = "Local\\rominabox-isolation-host";
    const PAGE_READWRITE: u32 = 4;
    let name = wide(SHARED);
    let mapping = unsafe { CreateFileMappingW(-1, std::ptr::null(), PAGE_READWRITE, 0, 16, name.as_ptr()) };
    assert!(mapping != 0, "could not make the host's shared memory");
    command.env("ROMINABOX_PROBE_SHM", SHARED);
    let sending = Arc::new(AtomicBool::new(true));
    let running = sending.clone();
    let sender = thread::spawn(move || {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        while running.load(Ordering::Relaxed) {
            let _ = socket.send_to(b"rominabox-isolation", "127.0.0.1:55355");
            thread::sleep(Duration::from_millis(100));
        }
    });
    ProbeTargets { mapping, sending, sender: Some(sender) }
}

fn plan(app: &Path) -> String {
    fs::read_to_string(resources_of(app).join("launch.plan")).unwrap()
}

pub fn assert_keeps_the_sandbox(app: &Path) {
    let plan = plan(app);
    assert!(
        plan.lines().any(|line| line == "sandbox\t1"),
        "the export's plan does not ask for its sandbox\n{plan}"
    );
}

/// Return whether the exported game can reach the QUICK SIGN IN `folder`.
pub fn names_accounts_folder(app: &Path, folder: &str) -> bool {
    plan(app).lines().any(|line| line == format!("accounts_dir\t{folder}"))
}
