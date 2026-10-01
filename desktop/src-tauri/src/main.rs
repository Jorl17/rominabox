#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rominabox_engine::export_error::{AuthorError, ErrorStage};
use rominabox_engine::{
    builder, controls, hotkeys, icons, menu, metadata, packaging, pads, projects, systems, traveling,
};
use serde_json::json;
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
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let directory = cache.join(format!(
        "menu-preview-{}-{}",
        std::process::id(),
        PREVIEW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    // The design that the author chose, by its id. For what the author has not
    // chosen we use the builder's defaults, as in the command line's `preview`.
    let mut stated = serde_json::Map::new();
    stated.insert("outputDir".into(), json!(directory));
    if let Some(design) = design {
        stated.insert("theme".into(), json!(design));
    }
    if let Some(palette) = palette {
        stated.insert("palette".into(), json!(palette));
    }
    if let Some(background) = background {
        stated.insert("background".into(), json!(background));
    }
    let request = builder::complete_preview(stated, &|relative| resource(&app, relative))?;
    tauri::async_runtime::spawn_blocking(move || {
        let output = menu::render_preview(&request)?;
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
    let bundled = resource(&app, "runtime").map_err(shell)?;
    let places = places(&app);
    request.core = None;
    request.core_cache = places.core_cache(request.game.target.target()).ok();
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
        let report = |progress| {
            let _ = events.emit("export-progress", progress);
        };
        // We make a game for the other platform from that platform's kit,
        // which we may have to download first.
        if !request.game.both_platforms {
            request.runtime_kit = builder::kit_for(&request.game.target, &bundled, &places)
                .map_err(|message| packaging::ExportError::new(ErrorStage::Refused, message))?;
        }
        builder::export(&request, Some(&bundled), &places, &cancelled, report)
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

/// Whether the hotkeys follow the rules of the game's menu, for the game
/// `system` with `controls`, or else the error that we would show in an
/// export.
#[tauri::command]
fn check_hotkeys(
    hotkeys: hotkeys::Hotkeys,
    system: String,
    controls: controls::Controls,
) -> Result<(), hotkeys::Refusal> {
    hotkeys.check_for(&system, &controls)
}

/// The folder for this builder's downloads, which we find in the same way for
/// the command line.
fn places(app: &tauri::AppHandle) -> builder::Places {
    builder::Places::of(app.config().identifier.clone())
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

/// The author's filters that a Windows game cannot load, as in `shaders-check`.
#[tauri::command]
fn shader_warnings(
    selection: rominabox_engine::shaders::ShaderSelection,
) -> Result<Vec<rominabox_engine::shaders::ShaderWarning>, String> {
    rominabox_engine::shaders::windows_warnings(&selection)
}

/// The name of a shader file that the author adds, as in `shaders-check` and
/// in an export, for one without a name in the request.
#[tauri::command]
fn custom_shader_name(path: PathBuf) -> String {
    rominabox_engine::shaders::named_after_file(&path)
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

/// The window at its size in tauri.conf.json, made smaller when the screen
/// has less room, and centred there. We create it hidden and show it once it
/// has its size, so it never appears at a size it will not keep.
fn fit_to_screen(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    if let Some(monitor) = window.current_monitor()? {
        let area = monitor.work_area().size;
        let outer = window.outer_size()?;
        let inner = window.inner_size()?;
        // The frame and title bar keep their size when the inside shrinks.
        let frame_width = outer.width.saturating_sub(inner.width);
        let frame_height = outer.height.saturating_sub(inner.height);
        let width = inner.width.min(area.width.saturating_sub(frame_width));
        let height = inner.height.min(area.height.saturating_sub(frame_height));
        if (width, height) != (inner.width, inner.height) {
            window.set_size(tauri::PhysicalSize::new(width, height))?;
        }
        window.center()?;
    }
    Ok(())
}

/// The title bar's icon, also shown in the taskbar's window preview, from the
/// program's own icon at the size for this screen. With Tauri, Windows gets
/// only one picture, the first in the .ico at 16 pixels, which is stretched
/// on a scaled screen.
#[cfg(windows)]
fn sharp_window_icon(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::LoadIconWithScaleDown;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageW, ICON_SMALL, SM_CXSMICON, SM_CYSMICON, WM_SETICON,
    };
    // The resource name of icons/icon.ico, as embedded by tauri-build.
    const PROGRAM_ICON: usize = 32512;
    let Ok(handle) = window.hwnd() else {
        return;
    };
    let hwnd = handle.0 as windows_sys::Win32::Foundation::HWND;
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        let mut icon = std::ptr::null_mut();
        let loaded = LoadIconWithScaleDown(
            GetModuleHandleW(std::ptr::null()),
            PROGRAM_ICON as *const u16,
            GetSystemMetricsForDpi(SM_CXSMICON, dpi),
            GetSystemMetricsForDpi(SM_CYSMICON, dpi),
            &mut icon,
        );
        if loaded == 0 && !icon.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon as isize);
        }
    }
}

/// The tasks of the Windows installer in the builder (installation.rs), done
/// before a window opens. Returns the exit code once done, or None for an
/// ordinary start. `--add-to-path FOLDER` adds the command line's folder to
/// the person's Path. `--uninstall-cleanup LOCAL ROAMING FOLDER` removes the
/// builder's and the games' files in the per-user folders LOCAL and ROAMING,
/// and removes FOLDER from the Path.
#[cfg(windows)]
fn installer_request(identifier: &str) -> Option<i32> {
    use rominabox_engine::installation::{announce_environment, Installation, ENVIRONMENT};
    let arguments: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let installation = |local: &PathBuf, roaming: &PathBuf, command_line: &PathBuf| Installation {
        local: local.clone(),
        roaming: roaming.clone(),
        identifier: identifier.to_string(),
        command_line: command_line.clone(),
        environment: ENVIRONMENT.to_string(),
    };
    let (changed, failures) = match (arguments.first()?.to_str()?, &arguments[1..]) {
        ("--add-to-path", [folder]) => match installation(&PathBuf::new(), &PathBuf::new(), folder).installed() {
            Ok(changed) => (changed, Vec::new()),
            Err(error) => (false, vec![format!("the Path: {error}")]),
        },
        ("--uninstall-cleanup", [local, roaming, folder]) => installation(local, roaming, folder).uninstalled(),
        _ => return None,
    };
    if changed {
        announce_environment();
    }
    for failure in &failures {
        eprintln!("ROM-in-a-Box: could not change {failure}");
    }
    Some(i32::from(!failures.is_empty()))
}

fn main() {
    let context = tauri::generate_context!();
    #[cfg(windows)]
    if let Some(code) = installer_request(&context.config().identifier) {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                // Shown whatever the result of the fitting. When we cannot
                // measure the window, it opens at its declared size.
                let _ = fit_to_screen(&window);
                #[cfg(windows)]
                {
                    sharp_window_icon(&window);
                    let moved = window.clone();
                    window.on_window_event(move |event| {
                        if let tauri::WindowEvent::ScaleFactorChanged { .. } = event {
                            sharp_window_icon(&moved);
                        }
                    });
                }
                window.show()?;
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(ExportControl::default())
        .invoke_handler(tauri::generate_handler![
            inspect_game,
            image_preview,
            menu_preview,
            default_destination,
            export_target,
            assess_firmware,
            check_hotkeys,
            export_game,
            cancel_export,
            save_project,
            open_project,
            traveling_files,
            shader_warnings,
            custom_shader_name,
            capture_pad_position,
            cancel_pad_capture
        ])
        .run(context)
        .expect("failed to run ROM-in-a-Box desktop shell");
}
