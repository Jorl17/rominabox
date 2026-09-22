//! Headless JSON-lines interface to the engine behind the desktop app.

use rominabox_desktop::{controls, metadata, packaging, projects, shaders, systems, themes};
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
        println!("ROM-in-a-Box native authoring CLI\n\nUsage: rominabox-cli <inspect|systems|controls|stage-controls|preview|export|firmware|project-save|project-open|shaders|shaders-check|schemas|freeze-macos-executable>\n\nRequests are JSON on stdin; progress and results are JSON Lines on stdout.\nshaders prints the catalog. shaders-check reads a selection on stdin.\nfreeze-macos-executable is a developer-only macOS runtime-kit preparation command.");
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
                "export": { "request": ["rom", "title", "system", "description?", "icon?", "background?", "showMenu", "startAtMenu", "theme", "palette", "menuSounds?", "controls?", "firmware?", "splash?", "advancedEmulatorAccess?", "shaders?", "outputDir", "target", "runtimeKit", "core?"], "controls": { "profile": "optional controller variant ID", "bindings": { "<control-id>": ["label?", "key?", "button?", "axis?", "mouse?"] } }, "shaders": { "bundled": ["catalog id"], "custom": [{ "name": "string", "path": "path" }], "initial": "bundled id or absent for unfiltered" }, "events": ["progress", "result", "error"] },
                "firmware": { "request": ["system", "files?"], "result": "FirmwareAssessment" },
                "project-save": { "request": ["archivePath", "settings"], "settings": ["rom", "title", "system", "description?", "icon?", "background?", "showMenu", "startAtMenu", "theme", "palette", "menuSounds?", "controls?", "firmware?", "splash?", "advancedEmulatorAccess?", "shaders?", "target"], "result": "ProjectArchiveResult" },
                "shaders": { "request": [], "result": "Catalog presets an author can bundle" },
                "shaders-check": { "request": { "bundled": ["catalog id"], "custom": [{ "name": "string", "path": "path" }], "initial": "optional id" }, "result": "Resolved shaders, or an error" },
                "project-open": { "request": ["archivePath", "extractionDir"], "result": "OpenProject" }
            })
        );
        return Ok(());
    }
    if command == "shaders" {
        println!(
            "{}",
            json!({ "type": "result", "result": { "presets": shaders::catalog()? } })
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
        "firmware" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                system: String,
                #[serde(default)]
                files: Vec<PathBuf>,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid firmware request: {error}"))?;
            let system = systems::find(&request.system)
                .ok_or_else(|| format!("no console is named {}", request.system))?;
            let result = systems::assess_firmware(system, &request.files);
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
        "scene-geometry" => {
            // The position of everything in the controller scene, for every
            // renderer, so that we place it the same way in all of them.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                system: String,
                #[serde(default)]
                profile: Option<String>,
                /// The design in whose frame we draw it.
                design: PathBuf,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid scene-geometry request: {error}"))?;
            let mut options = controls::Controls::default();
            options.profile = request.profile.clone();
            let profile = controls::validate_for_system(&request.system, &options)?;
            let metrics = themes::scene_metrics(&request.design)?;
            let layout = rominabox_desktop::scene_layout::layout(&profile.controls, metrics);
            println!(
                "{}",
                json!({ "type": "result", "result": layout })
            );
            Ok(())
        }
        "stage-theme" => {
            // The stylesheet that we write in an export, on demand. To see it,
            // for example in a screenshot harness or when checking a design,
            // get it from the exporter. The design's own file contains tokens
            // instead of colours, and is not what a player gets.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                source: PathBuf,
                destination: PathBuf,
                palette: String,
                #[serde(default)]
                background: Option<PathBuf>,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid stage-theme request: {error}"))?;
            themes::prepare_theme_assets(
                &request.source,
                &request.destination,
                &request.palette,
                request.background.as_deref(),
            )?;
            println!(
                "{}",
                json!({ "type": "result", "result": {
                    "stylesheet": request.destination.join("menu.rcss"),
                }})
            );
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
        "shaders-check" => {
            let selection: shaders::ShaderSelection = serde_json::from_str(&input)
                .map_err(|error| format!("invalid shader selection: {error}"))?;
            let resolved = shaders::resolve(&selection)?;
            let presets: Vec<_> = resolved
                .iter()
                .map(|item| {
                    json!({
                        "id": item.id,
                        "name": item.name,
                        "detail": item.detail,
                        "preset": item.relative_preset,
                    })
                })
                .collect();
            println!("{}", json!({ "type": "result", "result": { "shaders": presets } }));
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
