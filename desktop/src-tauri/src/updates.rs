//! Updates of the builder itself, with the updater plugin of Tauri. We read
//! the manifest of the newest release from the website, or from GitHub when
//! the website answers with an error, and check the signature of the package
//! against the public key in tauri.conf.json before we install it.
//!
//! Each platform has its own entry in the manifest, declared in
//! update-platforms.json. We choose the entry from the program that runs: on
//! a Mac, from the processors its code is for, so that a universal builder
//! stays universal after an update.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::Manager;
use tauri_plugin_updater::{Update, UpdaterExt};

/// The platforms we publish packages for, with their keys in the manifest.
const PLATFORMS: &str = include_str!("../update-platforms.json");
/// The name of the manifest in a release.
const MANIFEST: &str = "latest.json";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Declared {
    format_version: u32,
    platforms: Vec<Platform>,
}

#[derive(Deserialize)]
struct Platform {
    key: String,
    system: String,
    processors: Vec<String>,
}

/// The addresses of the manifest, in the order we try them: the website,
/// which redirects to the newest release, then the newest release on GitHub.
fn manifest_addresses() -> Vec<String> {
    vec![
        format!("{}/updates/{MANIFEST}", rominabox_engine::WEBSITE),
        format!("{}/releases/latest/download/{MANIFEST}", rominabox_engine::REPOSITORY),
    ]
}

/// The processors that `program`, a Mach-O file, has code for, by the names
/// in update-platforms.json, sorted.
#[cfg(any(target_os = "macos", test))]
fn mach_o_processors(program: &[u8]) -> Result<Vec<String>, String> {
    use rominabox_engine::mach_o::{slices, Cpu};
    let mut names = Vec::new();
    for slice in slices(program)? {
        names.push(match slice.cpu {
            Cpu::ARM64 => "arm64".to_string(),
            Cpu::X86_64 => "x86_64".to_string(),
            other => return Err(format!("The builder has code for {}, which we publish no update for.", other.name())),
        });
    }
    names.sort();
    Ok(names)
}

/// The key in the manifest of the builder for `system` with code for
/// `processors`, or `None` when we publish no package for it.
fn platform_key(system: &str, processors: &[String]) -> Result<Option<String>, String> {
    let declared: Declared =
        serde_json::from_str(PLATFORMS).map_err(|error| format!("update-platforms.json: {error}"))?;
    if declared.format_version != 1 {
        return Err(format!("update-platforms.json has the format {}.", declared.format_version));
    }
    Ok(declared
        .platforms
        .into_iter()
        .find(|platform| {
            let mut wanted = platform.processors.clone();
            wanted.sort();
            platform.system == system && wanted == processors
        })
        .map(|platform| platform.key))
}

/// The key in the manifest of the builder that runs.
fn own_platform_key() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let program = std::env::current_exe().map_err(|error| error.to_string())?;
        let bytes = std::fs::read(&program).map_err(|error| format!("{}: {error}", program.display()))?;
        platform_key("macos", &mach_o_processors(&bytes)?)
    }
    #[cfg(windows)]
    {
        platform_key("windows", &[std::env::consts::ARCH.to_string()])
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Ok(None)
    }
}

/// Whether we can replace the builder where it is. On a Mac, an app opened
/// from a downloaded folder runs from a read-only copy (App Translocation),
/// and an app in a folder the person cannot change cannot be replaced either.
fn installable() -> bool {
    #[cfg(target_os = "macos")]
    {
        let Ok(program) = std::env::current_exe() else {
            return false;
        };
        // The program is Contents/MacOS/<name> in ROM-in-a-Box.app.
        let Some(folder) = program.ancestors().nth(4) else {
            return false;
        };
        if program.to_string_lossy().contains("/AppTranslocation/") {
            return false;
        }
        let probe = folder.join(format!(".rominabox-update-probe-{}", std::process::id()));
        let writable = std::fs::File::create(&probe).is_ok();
        let _ = std::fs::remove_file(&probe);
        writable
    }
    #[cfg(windows)]
    {
        // The installer installs the builder for the person who runs it.
        true
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        false
    }
}

/// A newer builder, found by `builder_update_check`, until we install it.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// What we show about a newer builder.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    version: String,
    notes: Option<String>,
    /// Whether we can install it here. Otherwise we open the download page.
    installable: bool,
}

/// The progress of a download, in bytes.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    received: u64,
    total: Option<u64>,
}

/// The updater that reads the manifest at `addresses`, in that order, for
/// the platform `key`.
fn updater<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    addresses: &[String],
    key: &str,
) -> Result<tauri_plugin_updater::Updater, String> {
    let addresses = addresses
        .iter()
        .map(|address| address.parse().map_err(|error| format!("{address}: {error}")))
        .collect::<Result<Vec<_>, String>>()?;
    app.updater_builder()
        .endpoints(addresses)
        .map_err(|error| error.to_string())?
        .target(key)
        .build()
        .map_err(|error| error.to_string())
}

/// The newer builder in the manifest, if there is one for this platform.
#[tauri::command]
pub async fn builder_update_check(app: tauri::AppHandle) -> Result<Option<UpdateOffer>, String> {
    let Some(key) = own_platform_key()? else {
        return Ok(None);
    };
    let checked = updater(&app, &manifest_addresses(), &key)?.check().await;
    let Some(update) = checked.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let offer = UpdateOffer { version: update.version.clone(), notes: update.body.clone(), installable: installable() };
    *app.state::<PendingUpdate>().0.lock().map_err(|_| "The update state is broken.")? = Some(update);
    Ok(Some(offer))
}

/// Download and install the newer builder that `builder_update_check` found,
/// then start it. On Windows, the installer closes the builder first, so we
/// refuse while an export runs.
#[tauri::command]
pub async fn builder_update_install(app: tauri::AppHandle, progress: Channel<UpdateProgress>) -> Result<(), String> {
    if app.state::<crate::ExportControl>().0.lock().map_err(|_| "The export state is broken.")?.is_some() {
        return Err("Wait for the export to finish, then update.".to_string());
    }
    let update = app
        .state::<PendingUpdate>()
        .0
        .lock()
        .map_err(|_| "The update state is broken.")?
        .take()
        .ok_or("There is no update to install. Check for updates again.")?;
    let mut received = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                received += chunk as u64;
                let _ = progress.send(UpdateProgress { received, total });
            },
            || {},
        )
        .await
        .map_err(|error| format!("We could not install the update: {error}"))?;
    app.restart();
}

#[cfg(test)]
mod tests;
