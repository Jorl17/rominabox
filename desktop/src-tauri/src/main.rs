#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rominabox_engine::export_error::{AuthorError, ErrorStage};
use rominabox_engine::game_library::{self, Library};
use rominabox_engine::{
    builder, controls, game_data, hotkeys, icons, menu, metadata, packaging, pads, projects, systems,
    traveling,
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
    files: Option<rominabox_engine::content::GameFiles>,
) -> Result<metadata::Inspection, String> {
    let cache = places(&app).metadata_cache()?;
    tauri::async_runtime::spawn_blocking(move || {
        let files = files.unwrap_or_default();
        metadata::inspect_game_with_files(&path, &cache, online, system_override.as_deref(), &files)
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
    tint_background: Option<bool>,
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
    if let Some(tint) = tint_background {
        stated.insert("tintBackground".into(), json!(tint));
    }
    // The pictures we render with the kit, for every design and palette.
    if let Ok(rendered) = resource(&app, "menu-previews") {
        stated.insert("rendered".into(), json!(rendered));
    }
    let request = builder::complete_preview(stated, &|relative| resource(&app, relative))?;
    tauri::async_runtime::spawn_blocking(move || {
        let preview = menu::render_preview(&request)?;
        fs::read(preview.path).map_err(|e| e.to_string())
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

/// The hotkeys at the start of a game for `system`, which we give the draft
/// when the author chooses its console. On the command line we print them
/// with `hotkey-defaults`.
#[tauri::command]
fn hotkey_defaults(system: String) -> Result<hotkeys::Hotkeys, String> {
    hotkeys::defaults_for(&system)
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

/// Whether a new game also runs on Intel Macs until its author changes it.
#[tauri::command]
fn intel_macs_default() -> bool {
    rominabox_engine::builder::unstated::intel_macs()
}

/// Every component the builder and its games are made from, with its
/// licence, for the About dialog.
#[tauri::command]
fn about_components(app: tauri::AppHandle) -> Result<Vec<rominabox_engine::licences::Row>, String> {
    rominabox_engine::licences::read_index(&resource(&app, "licenses")?)
}

/// One component's licence text, from the file listed in its row.
#[tauri::command]
fn licence_text(app: tauri::AppHandle, file: String) -> Result<String, String> {
    rominabox_engine::licences::text(&resource(&app, "licenses")?, &file)
}

/// ROM-in-a-Box's web address, opened in the browser.
#[tauri::command]
fn open_website(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(rominabox_engine::WEBSITE, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_bug_report(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(rominabox_engine::BUG_REPORTS, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn traveling_files(
    path: PathBuf,
    system: Option<String>,
    files: Option<rominabox_engine::content::GameFiles>,
) -> Result<traveling::Traveling, String> {
    traveling::files_with(&path, system.as_deref(), &files.unwrap_or_default())
}

/// What we show on the Menu step about the shaders the author added, on
/// every platform, as in `shaders-check`.
#[tauri::command]
fn shader_warnings(
    app: tauri::AppHandle,
    selection: rominabox_engine::shaders::ShaderSelection,
) -> Result<rominabox_engine::shaders::ShaderWarnings, String> {
    let library = rominabox_engine::shaders::kit_library(&resource(&app, "runtime")?);
    rominabox_engine::shaders::shader_warnings(&selection, &library)
}

/// What we ask the author about on Create app, before the export.
#[tauri::command]
fn before_export(
    app: tauri::AppHandle,
    request: packaging::ExportRequest,
) -> Result<builder::BeforeExport, String> {
    let library = rominabox_engine::shaders::kit_library(&resource(&app, "runtime")?);
    builder::before_export(&request, &library)
}

/// The name of a shader file that the author adds, as in `shaders-check` and
/// in an export, for one without a name in the request.
#[tauri::command]
fn custom_shader_name(path: PathBuf) -> String {
    rominabox_engine::shaders::named_after_file(&path)
}

/// Run `work` on the games on this computer, away from the window's thread.
/// We call the same functions in the game-data commands of the command line.
async fn with_library<T: Send + 'static>(
    work: impl FnOnce(Library) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(Library::here()?))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn games() -> Result<Vec<game_library::InstalledGame>, String> {
    with_library(|library| Ok(library.games())).await
}

#[tauri::command]
async fn game_data_export(identities: Option<Vec<String>>, zip: PathBuf) -> Result<Vec<game_data::Game>, String> {
    with_library(move |library| library.export(identities.as_deref(), &zip)).await
}

#[tauri::command]
async fn game_data_open(zip: PathBuf) -> Result<Vec<game_library::BackupGame>, String> {
    with_library(move |library| library.open(&zip)).await
}

#[tauri::command]
async fn game_data_check(zip: PathBuf, which: usize, identity: String) -> Result<game_data::Check, String> {
    with_library(move |library| Ok(library.check(&zip, which, &identity))).await
}

#[tauri::command]
async fn game_data_import(zip: PathBuf, which: usize, identity: String) -> Result<(), String> {
    with_library(move |library| library.import(&zip, which, &identity)).await
}

#[tauri::command]
async fn game_data_import_all(zip: PathBuf) -> Result<game_library::BulkImport, String> {
    with_library(move |library| library.import_all(&zip)).await
}

#[tauri::command]
async fn game_data_remove(identity: String) -> Result<(), String> {
    with_library(move |library| library.remove(&identity)).await
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
/// The id of the About item in the macOS menu bar, and the event that opens
/// the About dialog of the builder.
#[cfg(target_os = "macos")]
const ABOUT_REQUESTED: &str = "about-requested";

/// The standard macOS menu, with its About item replaced by one that opens
/// the About dialog of the builder instead of the standard About panel.
#[cfg(target_os = "macos")]
fn macos_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let menu = Menu::default(app)?;
    if let Some(MenuItemKind::Submenu(application)) = menu.items()?.into_iter().next() {
        application.remove_at(0)?;
        let title = format!("About {}", app.package_info().name);
        application.insert(&MenuItem::with_id(app, ABOUT_REQUESTED, title, true, None::<&str>)?, 0)?;
    }
    Ok(menu)
}

/// `builder` with the macOS menu; unchanged on other platforms, which show
/// no menu bar.
fn with_menu(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    #[cfg(target_os = "macos")]
    let builder = builder.menu(macos_menu).on_menu_event(|app, event| {
        if event.id().as_ref() == ABOUT_REQUESTED {
            let _ = app.emit(ABOUT_REQUESTED, ());
        }
    });
    builder
}

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
    with_menu(tauri::Builder::default())
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
            about_components,
            licence_text,
            open_website,
            open_bug_report,
            intel_macs_default,
            assess_firmware,
            check_hotkeys,
            hotkey_defaults,
            export_game,
            cancel_export,
            save_project,
            open_project,
            traveling_files,
            shader_warnings,
            before_export,
            custom_shader_name,
            capture_pad_position,
            cancel_pad_capture,
            games,
            game_data_export,
            game_data_open,
            game_data_check,
            game_data_import,
            game_data_import_all,
            game_data_remove
        ])
        .run(context)
        .expect("failed to run ROM-in-a-Box desktop shell");
}
