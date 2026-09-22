//! Headless JSON-lines interface to the engine behind the desktop app.

use rominabox_desktop::{controls, metadata, packaging, projects, systems, themes, volume};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InspectRequest {
    rom: PathBuf,
    cache: PathBuf,
    #[serde(default = "online_default")]
    online: bool,
    #[serde(default)]
    system: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FreezeRequest {
    source: PathBuf,
    destination: PathBuf,
}

#[derive(Deserialize)]
struct ControlsRequest {
    system: String,
    #[serde(default)]
    profile: Option<String>,
}

fn online_default() -> bool {
    true
}

fn main() {
    if let Err(error) = run() {
        println!("{}", json!({ "type": "error", "message": error }));
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--help".to_string());
    if command == "--help" || command == "-h" {
        println!("ROM-in-a-Box native authoring CLI\n\nUsage: rominabox-cli <inspect|systems|controls|stage-controls|preview|export|project-save|project-open|volume|volume-markup|schemas|freeze-macos-executable>\n\nRequests are JSON on stdin; progress and results are JSON Lines on stdout.\nfreeze-macos-executable is a developer-only macOS runtime-kit preparation command.");
        return Ok(());
    }
    if command == "schemas" {
        println!(
            "{}",
            json!({
                "inspect": { "request": ["rom", "cache", "online?", "system?"], "result": "Inspection" },
                "systems": { "request": ["runtimeKit?"], "result": "System declarations and optional available system IDs" },
                "controls": { "request": ["system", "profile?"], "result": "Controller profile, console labels, stable IDs and default keys" },
                "preview": { "request": ["assets", "renderer", "outputDir", "palette", "background?", "width", "height"], "result": { "imagePath": "path" } },
                "export": { "request": ["rom", "title", "system", "description?", "icon?", "background?", "showMenu", "startAtMenu", "theme", "palette", "menuSounds?", "controls?", "firmware?", "splash?", "advancedEmulatorAccess?", "outputDir", "target", "runtimeKit", "core?"], "controls": { "profile": "optional controller variant ID", "bindings": { "<control-id>": ["label?", "key?", "button?", "axis?", "mouse?"] } }, "events": ["progress", "result", "error"] },
                "project-save": { "request": ["archivePath", "settings"], "settings": ["rom", "title", "system", "description?", "icon?", "background?", "showMenu", "startAtMenu", "theme", "palette", "menuSounds?", "controls?", "firmware?", "splash?", "advancedEmulatorAccess?", "target"], "result": "ProjectArchiveResult" },
                "project-open": { "request": ["archivePath", "extractionDir"], "result": "OpenProject" },
                "volume": { "request": ["dataDir", "decibels?", "muted?"], "result": { "decibels": "dB", "muted": "bool", "path": "volume.cfg" } },
                "volume-markup": { "request": ["design"], "result": { "markup": "the volume control, in the design's slider and toggle" } }
            })
        );
        return Ok(());
    }
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("could not read stdin: {error}"))?;
    match command.as_str() {
        "systems" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                runtime_kit: Option<PathBuf>,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|e| format!("invalid systems request: {e}"))?;
            let available = request
                .runtime_kit
                .as_deref()
                .map(packaging::available_systems);
            println!(
                "{}",
                json!({"type":"result", "result":{"systems":systems::registry(), "available":available}})
            );
            Ok(())
        }
        "controls" => {
            let request: ControlsRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid controls request: {error}"))?;
            let profile = controls::validate_for_system(
                &request.system,
                &controls::Controls {
                    profile: request.profile,
                    ..Default::default()
                },
            )?;
            println!("{}", json!({"type":"result", "result":profile}));
            Ok(())
        }
        "inspect" => {
            let request: InspectRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid inspect request: {error}"))?;
            let result = metadata::inspect_game_with_system(
                &request.rom,
                &request.cache,
                request.online,
                request.system.as_deref(),
            )
            .map_err(|error| error.to_string())?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "export" => {
            let request: packaging::ExportRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid export request: {error}"))?;
            let cancelled = AtomicBool::new(false);
            let result = packaging::export_game(&request, &cancelled, |event| {
                println!("{}", json!({ "type": "progress", "progress": event }));
            })
            .map_err(|error| error.to_string())?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "project-save" => {
            let request: projects::ProjectSaveRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid project save request: {error}"))?;
            let result = projects::save_project(&request)?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "project-open" => {
            let request: projects::ProjectOpenRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid project open request: {error}"))?;
            let result = projects::open_project(&request)?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "stage-controls" => {
            // We generate the controls scene in the exporter from a console
            // package. To see that markup, for example in a screenshot harness or
            // when checking a design, get it from the exporter instead of
            // assembling a copy, because two copies would come to differ.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                system: String,
                source: PathBuf,
                /// The design for whose frame we generate the scene. The default
                /// is the artwork directory, which is a design directory when
                /// the two are the same place.
                #[serde(default)]
                design: Option<PathBuf>,
                destination: PathBuf,
                #[serde(default)]
                controls: controls::Controls,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid stage-controls request: {error}"))?;
            themes::prepare_controls_assets(
                &request.source,
                request.design.as_deref().unwrap_or(&request.source),
                &request.destination,
                &request.system,
                &request.controls,
            )?;
            println!(
                "{}",
                json!({ "type": "result", "result": {
                    "document": request.destination.join("menu.rml"),
                }})
            );
            Ok(())
        }
        "preview" => {
            let request: themes::PreviewRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid preview request: {error}"))?;
            let image_path = themes::render_preview(&request)?;
            println!(
                "{}",
                json!({ "type": "result", "result": { "imagePath": image_path } })
            );
            Ok(())
        }
        "volume" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                data_dir: PathBuf,
                #[serde(default)]
                decibels: Option<f32>,
                #[serde(default)]
                muted: Option<bool>,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid volume request: {error}"))?;
            let mut level = volume::read(&request.data_dir);
            if request.decibels.is_some() || request.muted.is_some() {
                if let Some(decibels) = request.decibels {
                    level.decibels = decibels;
                }
                if let Some(muted) = request.muted {
                    level.muted = muted;
                }
                level = level.clamp();
                volume::write(&request.data_dir, level)?;
            }
            println!(
                "{}",
                json!({ "type": "result", "result": {
                    "decibels": level.decibels,
                    "muted": level.muted,
                    "path": request.data_dir.join(volume::file_name()),
                }})
            );
            Ok(())
        }
        "volume-markup" => {
            #[derive(Deserialize)]
            struct Request {
                design: PathBuf,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid volume-markup request: {error}"))?;
            let markup = themes::volume_control_markup(&request.design)?;
            println!(
                "{}",
                json!({ "type": "result", "result": { "markup": markup } })
            );
            Ok(())
        }
        "freeze-macos-executable" => {
            let request: FreezeRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid freeze request: {error}"))?;
            let installed_bytes =
                packaging::freeze_macos_executable(&request.source, &request.destination)
                    .map_err(|error| error.to_string())?;
            println!(
                "{}",
                json!({ "type": "result", "result": { "path": request.destination, "installedBytes": installed_bytes } })
            );
            Ok(())
        }
        _ => Err(format!("unknown command: {command}")),
    }
}
