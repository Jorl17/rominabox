#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rominabox_desktop::{icons, metadata, packaging, projects, systems, themes};
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
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("metadata");
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
        let image = icons::read_image(&path).map_err(|e| e.to_string())?;
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
) -> Result<Vec<u8>, String> {
    let renderer = resource(&app, "preview/rml-preview")?;
    let assets = resource(&app, "runtime/menu-assets")?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let directory = cache.join(format!(
        "menu-preview-{}-{}",
        std::process::id(),
        PREVIEW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    tauri::async_runtime::spawn_blocking(move || {
        let output = themes::render_preview(&themes::PreviewRequest {
            assets,
            renderer,
            output_dir: directory,
            palette: palette.unwrap_or_else(|| "blue".into()),
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
fn default_destination(app: tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .download_dir()
        .map(|p| p.join("ROM-in-a-Box"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_export(state: tauri::State<'_, ExportControl>) -> Result<(), String> {
    if let Some(cancelled) = state.0.lock().map_err(|e| e.to_string())?.as_ref() {
        cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
async fn export_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, ExportControl>,
    mut request: packaging::ExportRequest,
) -> Result<packaging::ExportResult, String> {
    request.runtime_kit = resource(&app, "runtime")?;
    request.core = None;
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.0.lock().map_err(|e| e.to_string())?;
        if active.is_some() {
            return Err("Another export is already running.".into());
        }
        *active = Some(cancelled.clone());
    }
    let events = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        packaging::export_game(&request, &cancelled, |progress| {
            let _ = events.emit("export-progress", progress);
        })
        .map_err(|e| e.to_string())
    })
    .await;
    *state.0.lock().map_err(|e| e.to_string())? = None;
    result.map_err(|e| e.to_string())?
}

#[tauri::command]
fn assess_firmware(
    system: String,
    files: Vec<PathBuf>,
) -> Result<systems::FirmwareAssessment, String> {
    let system = systems::find(&system).ok_or_else(|| format!("no console is named {system}"))?;
    Ok(systems::assess_firmware(system, &files))
}

#[tauri::command]
fn available_systems(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Ok(packaging::available_systems(&resource(&app, "runtime")?))
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
            assess_firmware,
            export_game,
            cancel_export,
            save_project,
            open_project
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ROM-in-a-Box desktop shell");
}
