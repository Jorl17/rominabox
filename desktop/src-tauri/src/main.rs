#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rominabox_desktop::export_error::{AuthorError, ErrorStage};
use rominabox_desktop::target::Target;
use rominabox_desktop::{
    builder, cores, icons, menu, menu_controls, metadata, packaging, pads, projects, systems,
    traveling,
};
use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

#[derive(Default)]
struct ExportControl(Mutex<Option<Arc<AtomicBool>>>);
static PREVIEW_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn resource(app: &tauri::AppHandle, relative: &str) -> Result<PathBuf, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join(relative);
    if bundled.exists() {
        return Ok(bundled);
    }
    #[cfg(debug_assertions)]
    {
        let development = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(relative);
        if development.exists() {
            return Ok(development);
        }
    }
    Err(format!(
        "The app is missing its {relative} resource. Rebuild or reinstall the builder."
    ))
}

#[tauri::command]
async fn inspect_game(
    app: tauri::AppHandle,
    path: PathBuf,
    online: bool,
    system_override: Option<String>,
) -> Result<metadata::Inspection, String> {
    let cache = places(&app).metadata_cache()?;
    tauri::async_runtime::spawn_blocking(move || {
        metadata::inspect_game_with_system(&path, &cache, online, system_override.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn image_preview(path: PathBuf) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let image = icons::read_image(&path).map_err(|e| e.sentence())?;
        let mut png = Cursor::new(Vec::new());
        image
            .thumbnail(512, 512)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(png.into_inner())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn menu_preview(
    app: tauri::AppHandle,
    background: Option<PathBuf>,
    palette: Option<String>,
    design: Option<String>,
) -> Result<Vec<u8>, String> {
    let target = Target::host().ok_or("this machine is not one the builder builds for")?;
    let renderer = resource(&app, &packaging::preview_renderer(target)?)?;
    // The design the author picked, from its own directory, and the shared
    // controller artwork from the kit's directory, which is not a design.
    let chosen = design.unwrap_or_else(builder::unstated::theme);
    let design = resource(&app, &format!("runtime/designs/{chosen}"))?;
    let assets = resource(&app, "runtime/menu-assets")?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let directory = cache.join(format!(
        "menu-preview-{}-{}",
        std::process::id(),
        PREVIEW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    tauri::async_runtime::spawn_blocking(move || {
        let output = menu::render_preview(&menu::PreviewRequest {
            design,
            assets,
            renderer,
            output_dir: directory,
            palette: palette.unwrap_or_else(builder::unstated::palette),
            background,
            width: 960,
            height: 600,
        })?;
        fs::read(output).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_project(
    request: projects::ProjectSaveRequest,
) -> Result<projects::ProjectArchiveResult, String> {
    tauri::async_runtime::spawn_blocking(move || projects::save_project(&request))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_project(
    app: tauri::AppHandle,
    archive_path: PathBuf,
) -> Result<projects::OpenProject, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let extraction_dir = cache.join(format!(
        "project-{}-{}",
        std::process::id(),
        PREVIEW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    tauri::async_runtime::spawn_blocking(move || {
        projects::open_project(&projects::ProjectOpenRequest {
            archive_path,
            extraction_dir,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn default_destination() -> Result<PathBuf, String> {
    builder::destination()
}

#[tauri::command]
fn cancel_export(state: tauri::State<'_, ExportControl>) -> Result<(), String> {
    if let Some(cancelled) = state.0.lock().map_err(|e| e.to_string())?.as_ref() {
        cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
/// We pass the stage to the builder, so that we can tell a core that could not
/// be downloaded from any other failure, and pass the sentence to show. We
/// write the details of the failure to the log.
async fn export_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, ExportControl>,
    request: packaging::ExportRequest,
) -> Result<packaging::ExportResult, AuthorError> {
    run_export(app, state, request).await.map_err(|error| {
        eprintln!("export failed: {error}");
        error.for_author()
    })
}

async fn run_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, ExportControl>,
    mut request: packaging::ExportRequest,
) -> Result<packaging::ExportResult, packaging::ExportError> {
    let shell = |message: String| packaging::ExportError::new(ErrorStage::Export, message);
    request.runtime_kit = resource(&app, "runtime").map_err(shell)?;
    request.core = None;
    request.core_cache = request
        .target
        .target()
        .and_then(|target| places(&app).core_cache(target).ok());
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.0.lock().map_err(|e| shell(e.to_string()))?;
        if active.is_some() {
            return Err(packaging::ExportError::new(
                ErrorStage::Refused,
                "Another export is already running.",
            ));
        }
        *active = Some(cancelled.clone());
    }
    let events = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        packaging::export_game(&request, &cancelled, |progress| {
            let _ = events.emit("export-progress", progress);
        })
    })
    .await;
    *state.0.lock().map_err(|e| shell(e.to_string()))? = None;
    result.map_err(|e| shell(e.to_string()))?
}

#[tauri::command]
fn assess_firmware(
    system: String,
    files: Vec<PathBuf>,
) -> Result<systems::FirmwareAssessment, String> {
    let system = systems::find(&system).ok_or_else(|| format!("no console is named {system}"))?;
    Ok(systems::assess_firmware(system, &files))
}

/// Whether the menu controls follow the rules of the game's menu, or else the
/// error that we would show in an export.
#[tauri::command]
fn check_menu_controls(
    menu_controls: menu_controls::MenuControls,
) -> Result<(), menu_controls::Refusal> {
    menu_controls.check()
}

/// The folder for this builder's downloads, which we find in the same way for
/// the command line.
fn places(app: &tauri::AppHandle) -> builder::Places {
    builder::Places::of(app.config().identifier.clone())
}

#[tauri::command]
async fn ensure_cores(app: tauri::AppHandle) -> Result<Vec<cores::CoreInstall>, String> {
    let target = Target::host().ok_or("this machine is not one the builder builds for")?;
    let cache = places(&app).core_cache(target)?;
    tauri::async_runtime::spawn_blocking(move || {
        cores::install_target(&cache, target, &cores::UreqTransport)
    })
    .await
    .map_err(|error| error.to_string())
}

/// The platform for exports from this builder, which is the one it runs on.
#[tauri::command]
fn export_target() -> Option<packaging::ExportTarget> {
    packaging::ExportTarget::of_host()
}

#[tauri::command]
fn traveling_files(path: PathBuf, system: Option<String>) -> Result<traveling::Traveling, String> {
    traveling::files_for(&path, system.as_deref())
}

#[tauri::command]
fn available_systems(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let kit = resource(&app, "runtime")?;
    let cache = Target::host().and_then(|target| places(&app).core_cache(target).ok());
    Ok(
        packaging::system_availability_in(&kit, cache.as_deref(), Target::host())
            .into_iter()
            .filter(|entry| entry.unavailable.is_none())
            .map(|entry| entry.id)
            .collect(),
    )
}

/// Wait for a press on a controller and return its pad position.
#[tauri::command]
async fn capture_pad_position(seconds: u64) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        pads::capture(std::time::Duration::from_secs(seconds))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_pad_capture() {
    pads::cancel();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(ExportControl::default())
        .invoke_handler(tauri::generate_handler![
            inspect_game,
            image_preview,
            menu_preview,
            default_destination,
            available_systems,
            ensure_cores,
            export_target,
            assess_firmware,
            check_menu_controls,
            export_game,
            cancel_export,
            save_project,
            open_project,
            traveling_files,
            capture_pad_position,
            cancel_pad_capture
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ROM-in-a-Box desktop shell");
}
