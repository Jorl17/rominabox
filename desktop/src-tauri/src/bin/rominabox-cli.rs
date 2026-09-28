//! Headless JSON-lines interface to the engine behind the desktop app.

use rominabox_desktop::{
    builder, controls, cores, kits, menu, metadata, packaging, projects, shaders, systems, themes,
};
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
    #[serde(default = "builder::unstated::online")]
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

fn main() {
    if let Err(error) = run() {
        println!("{}", json!({ "type": "error", "message": error }));
        std::process::exit(1);
    }
}

fn read_request() -> Result<String, String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("could not read stdin: {error}"))?;
    Ok(input)
}

fn run() -> Result<(), String> {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--help".to_string());
    if command == "--help" || command == "-h" {
        println!("ROM-in-a-Box native authoring CLI\n\nUsage: rominabox-cli <inspect|content|systems|controls|stage-controls|preview|export|firmware|project-save|project-open|volume-markup|design-screens|shaders|shaders-check|designs|defaults|cores|schemas|where|freeze-macos-executable>\n       rominabox-cli export GAME [FOLDER]\n\nA command that takes a request reads one JSON object from stdin through EOF. Progress and results are JSON Lines on stdout.\ncontent names every file export will copy for a dropped path.\nexport makes the game that dropping GAME into the builder makes, in FOLDER or the builder's; a request on stdin can say more, and whatever it leaves out is the builder's.\nexport and project-save accept includeAchievements (default true); player authentication is per game.\nshaders prints the catalog. shaders-check reads a selection on stdin.\ndesigns lists the menu designs, palettes and sound packs. defaults prints the settings a request leaves out.\ncores fetches the download list for one target into cache.\nfreeze-macos-executable is a developer-only macOS runtime-kit preparation command.");
        return Ok(());
    }
    // The checkout from which we built this binary.
    //
    // Checkouts can use one cargo target directory, and then
    // release/rominabox-cli is a single file, replaced by the last build from
    // any checkout. A caller could then photograph a menu, stage a design or
    // measure an export with another checkout's code.
    //
    // We compile in the manifest directory, so we can print the origin of the
    // binary, and a caller can reject a binary from another checkout.
    if command == "where" {
        println!("{}", env!("CARGO_MANIFEST_DIR"));
        return Ok(());
    }
    if command == "schemas" {
        println!(
            "{}",
            json!({
                "inspect": { "request": ["rom", "cache", "online?", "system?"], "result": "Inspection" },
                "content": { "request": ["rom", "system?"], "result": { "entry": "the game file collect ran on", "files": ["relative names, entry first, then whatever rides with it"] } },
                "systems": { "request": ["runtimeKit?"], "result": "System declarations and optional available system IDs" },
                "controls": { "request": ["system", "profile?"], "result": "Controller profile, console labels, stable IDs and default keys" },
                "preview": { "request": ["assets", "renderer", "outputDir", "palette", "background?", "width", "height"], "result": { "imagePath": "path" } },
                "export": { "request": ["rom", "title?", "system?", "description?", "icon?", "background?", "showMenu?", "startAtMenu?", "theme?", "palette?", "menuSounds?", "controls?", "menuControls?", "firmware?", "splash?", "includeAchievements?", "advancedEmulatorAccess?", "intelMacs?", "keepPlayingInBackground?", "autosaveOnQuit?", "shaders?", "menuEntries?", "outputDir?", "replace?", "target?", "bothPlatforms?", "runtimeKit?", "core?", "coreCache?", "online?", "metadataCache?"], "arguments": "export GAME [FOLDER] is the request {rom: GAME, outputDir: FOLDER} and reads nothing from stdin", "omitted": "what a request leaves out is what dropping rom into the builder gives: without title or system the game is identified as inspect does, and the title, system, description and icon it leaves out come from that (one that names both is not looked up); each setting the builder's default (the defaults command); outputDir ROM-in-a-Box in Downloads; target this machine; runtimeKit the one beside this command, else this checkout's; coreCache the builder's. What a request states wins, null included", "online": "defaults true, and the lookup may download catalogues and covers; false uses only what metadataCache already holds. A test that looks a game up passes false", "metadataCache": "where lookups are cached; defaults to the builder's", "coreCache": "directory of downloaded cores; each needed core is downloaded or updated there first, and a progress event carrying cores {kind: fetching, downloading, updating} or {kind: failed, missing} says so; null takes the core from runtimeKit alone", "controls": { "profile": "optional controller variant ID", "bindings": { "<control-id>": ["label?", "key?", "pad?", "mouse?"] }, "pad": "the pad position the control is read from, one of controls.json padPositions, a stick's directions among them; a direction moves with its opposite, and a position another offered pad's control keeps cannot be taken" }, "menuControls": { "<menu|confirm|back>": ["key:<RetroArch key name>", "pad:<pad button position or home>[+<pad input>...]"], "omitted": "an action left out keeps the defaults command's; MENU keeps a key and every action a binding, and an input is held by one action, or by both MENU and BACK" }, "shaders": { "bundled": ["catalog id"], "custom": [{ "name": "string", "path": "path" }], "initial": "bundled id or absent for unfiltered" }, "includeAchievements": "defaults true; effective only with showMenu; packages authenticated Casual support, not account data or downloaded rules", "intelMacs": "defaults false; a macos game also runs on Intel Macs: its player, core and launcher carry x86_64 code beside arm64, the core for macos-x86_64 fetched into a folder of that name beside coreCache. Refused when the runtime kit's player has no x86_64 code", "menuEntries": "option entry ids; omit for resolved defaults; an explicit list must agree with includeAchievements", "replace": "replace an app already at the destination; without it such an export does nothing and prints {type: exists, appPath}", "bothPlatforms": "defaults false; the game for Mac and for Windows, each from its platform's kit, in one <title>.zip holding Mac/<title>.app and Windows/<title>; intelMacs is the Mac game's", "events": ["progress", "result", "exists", "error"] },
                "firmware": { "request": ["system", "files?"], "result": "FirmwareAssessment" },
                "cores": { "request": ["cache", "target"], "target": "macos-arm64 | macos-x86_64 | windows-x86_64 | linux-x86_64", "result": "per-core present, installed, unreachable or notRecorded" },
                "project-save": { "request": ["archivePath", "settings"], "settings": ["rom", "title", "system", "description?", "icon?", "background?", "showMenu?", "startAtMenu?", "theme?", "palette?", "menuSounds?", "controls?", "menuControls?", "firmware?", "splash?", "includeAchievements?", "advancedEmulatorAccess?", "intelMacs?", "keepPlayingInBackground?", "autosaveOnQuit?", "shaders?", "menuEntries?", "target", "bothPlatforms?"], "result": "ProjectArchiveResult" },
                "shaders": { "request": [], "result": "Catalog presets an author can bundle" },
                "designs": { "request": [], "result": "The menu designs (theme ids), palettes and sound packs an export can name" },
                "defaults": { "request": [], "result": "The builder's settings, which a request leaves out" },
                "shaders-check": { "request": { "bundled": ["catalog id"], "custom": [{ "name": "string", "path": "path" }], "initial": "optional id" }, "result": "Resolved shaders, or an error" },
                "project-open": { "request": ["archivePath", "extractionDir"], "result": "OpenProject" },
                "volume-markup": { "request": ["design"], "result": { "markup": "the volume control, in the design's slider, with an arrow either side" } },
                "design-screens": { "request": ["design"], "result": { "screens": "the design's screens as the menu resolves them, Native's merged with its own, in design.json's words" } }
            })
        );
        return Ok(());
    }
    if command == "designs" {
        println!("{}", json!({ "type": "result", "result": themes::registry()? }));
        return Ok(());
    }
    if command == "defaults" {
        println!("{}", json!({ "type": "result", "result": builder::defaults() }));
        return Ok(());
    }
    if command == "shaders" {
        println!(
            "{}",
            json!({ "type": "result", "result": { "presets": shaders::catalog()? } })
        );
        return Ok(());
    }
    // For each command that takes a request, we read the request here, in
    // its own arm. For a command with no request we never read stdin.
    match command.as_str() {
        "systems" => {
            let input = read_request()?;
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
            let input = read_request()?;
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
        "content" => {
            let input = read_request()?;
            #[derive(Deserialize)]
            struct Request {
                rom: PathBuf,
                system: Option<String>,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid content request: {error}"))?;
            let result =
                rominabox_desktop::traveling::files_for(&request.rom, request.system.as_deref())?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "inspect" => {
            let input = read_request()?;
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
        "cores" => {
            let input = read_request()?;
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Request {
                cache: PathBuf,
                target: rominabox_desktop::target::Target,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid cores request: {error}"))?;
            let report =
                cores::install_target(&request.cache, request.target, &cores::UreqTransport);
            println!("{}", json!({ "type": "result", "result": report }));
            Ok(())
        }
        "firmware" => {
            let input = read_request()?;
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
            // `export GAME [FOLDER]` is like dropping GAME into the builder.
            // When someone names the game on the command line, it is the whole
            // request, and we do not read stdin.
            let arguments: Vec<String> = std::env::args().skip(2).collect();
            let request = match arguments.as_slice() {
                [] => {
                    let input = read_request()?;
                    serde_json::from_str(&input)
                        .map_err(|error| format!("invalid export request: {error}"))?
                }
                [game, folder @ ..] if folder.len() <= 1 => {
                    let mut request = serde_json::Map::new();
                    request.insert("rom".into(), json!(game));
                    if let Some(folder) = folder.first() {
                        request.insert("outputDir".into(), json!(folder));
                    }
                    request
                }
                _ => return Err("export takes a game and, optionally, the folder to write it to".into()),
            };
            let request = builder::complete_export(request)?;
            let cancelled = AtomicBool::new(false);
            let report = |event| println!("{}", json!({ "type": "progress", "progress": event }));
            // Both platforms at once, with the game for each in one zip.
            let result = if request.game.both_platforms {
                let own = builder::runtime_kit().ok_or("no runtime kit is beside this command or in this checkout")?;
                let places = builder::Places::of(builder::identifier());
                let store = places.kit_store()?;
                let kit_for = |platform: &packaging::ExportTarget| {
                    kits::for_export(platform, &own, &store, &cores::UreqTransport)
                };
                let core_cache_for = |target| places.core_cache(target).ok();
                packaging::export_for_both(&request, &kit_for, &core_cache_for, &cancelled, report)
            } else {
                packaging::export_game(&request, &cancelled, report)
            };
            match result {
                Ok(result) => println!("{}", json!({ "type": "result", "result": result })),
                // This is not a failure. We did nothing, and with `replace` in
                // the request we go ahead.
                Err(error) if error.stage == packaging::ErrorStage::Exists => {
                    println!(
                        "{}",
                        json!({ "type": "exists", "appPath": error.path, "message": error.sentence() })
                    );
                    std::process::exit(1);
                }
                Err(error) => {
                    println!(
                        "{}",
                        json!({ "type": "error", "message": error.to_string(), "error": error, "sentence": error.sentence() })
                    );
                    std::process::exit(1);
                }
            }
            Ok(())
        }
        "project-save" => {
            let input = read_request()?;
            let request: projects::ProjectSaveRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid project save request: {error}"))?;
            let result = projects::save_project(&request)?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "project-open" => {
            let input = read_request()?;
            let request: projects::ProjectOpenRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid project open request: {error}"))?;
            let result = projects::open_project(&request)?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "scene-geometry" => {
            let input = read_request()?;
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
            let metrics = menu::scene_metrics(&request.design)?;
            let layout = rominabox_desktop::scene_layout::layout(&profile.controls, metrics);
            // And the title in each stick's box, as in the composed menu.
            let mut result = serde_json::to_value(layout).map_err(|error| error.to_string())?;
            result["titles"] = serde_json::to_value(menu::scene_titles(&profile))
                .map_err(|error| error.to_string())?;
            println!("{}", json!({ "type": "result", "result": result }));
            Ok(())
        }
        "stage-theme" | "stage-controls" => {
            let input = read_request()?;
            // We write the menu of an export on demand, with the one composer
            // for the export, the builder's preview and both of these commands.
            // When we need the menu elsewhere, such as in a screenshot harness
            // or a design check, we make it here and never assemble another, so
            // the menu is the same in every renderer.
            //
            // For stage-theme `source` is the design. For stage-controls
            // `source` is the controller artwork and `design` is the design.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Request {
                source: PathBuf,
                #[serde(default)]
                design: Option<PathBuf>,
                #[serde(default)]
                artwork: Option<PathBuf>,
                destination: PathBuf,
                #[serde(default)]
                palette: Option<String>,
                #[serde(default)]
                background: Option<PathBuf>,
                #[serde(default)]
                system: Option<String>,
                #[serde(default)]
                controls: controls::Controls,
                /// Options entries to stage, or the design's defaults when absent.
                #[serde(default)]
                menu_entries: Option<Vec<String>>,
                /// The achievements screen, which we stage as in an export that includes it.
                #[serde(default)]
                include_achievements: bool,
                /// Bundled shaders. With any of them we add the Filters screen.
                #[serde(default)]
                shaders: shaders::ShaderSelection,
                /// How many discs the game has. With more than one we add the disc list.
                #[serde(default)]
                discs: Option<usize>,
                /// The author's default for playing on in the background, which
                /// the player can change in Options.
                #[serde(default)]
                keep_playing_in_background: bool,
                /// The inputs to open the menu, and to confirm and go back in it.
                /// For an action left out, we use the builder's default.
                #[serde(default = "rominabox_desktop::builder::unstated::menu_controls")]
                menu_controls: rominabox_desktop::menu_controls::MenuControls,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid {command} request: {error}"))?;
            let (design, artwork) = if command == "stage-theme" {
                let artwork = request
                    .artwork
                    .unwrap_or_else(|| rominabox_desktop::repo::at("desktop/assets/controllers"));
                (request.design.unwrap_or(request.source), artwork)
            } else {
                // The artwork directory is a design directory when the two
                // are the same place.
                (
                    request.design.unwrap_or_else(|| request.source.clone()),
                    request.artwork.unwrap_or(request.source),
                )
            };
            let defaults = menu::MenuRequest::new(design, artwork);
            menu::compose_menu(&menu::MenuRequest {
                palette: request.palette.unwrap_or(defaults.palette.clone()),
                background: request.background,
                system: request.system.unwrap_or(defaults.system.clone()),
                controls: request.controls,
                menu_controls: request.menu_controls,
                menu_entries: request.menu_entries,
                include_achievements: request.include_achievements,
                shaders: request.shaders,
                discs: request.discs.unwrap_or(defaults.discs),
                settings: rominabox_desktop::player_settings::Defaults {
                    keep_playing_in_background: request.keep_playing_in_background,
                },
                ..defaults
            })?
            .write(&request.destination)?;
            println!(
                "{}",
                json!({ "type": "result", "result": {
                    "stylesheet": request.destination.join(menu::STYLESHEET),
                    "document": request.destination.join(menu::DOCUMENT),
                }})
            );
            Ok(())
        }
        "preview" => {
            let input = read_request()?;
            let request: menu::PreviewRequest = serde_json::from_str(&input)
                .map_err(|error| format!("invalid preview request: {error}"))?;
            let image_path = menu::render_preview(&request)?;
            println!(
                "{}",
                json!({ "type": "result", "result": { "imagePath": image_path } })
            );
            Ok(())
        }
        "design-screens" => {
            let input = read_request()?;
            #[derive(Deserialize)]
            struct Request {
                design: PathBuf,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid design-screens request: {error}"))?;
            let screens: Vec<_> = menu::declared_screens(&request.design)?
                .iter()
                .map(|screen| {
                    json!({
                        "id": screen.id,
                        "role": screen.role.map(|role| role.name()),
                        "panel": screen.panel,
                        "button": screen.button,
                        "place": (screen.place == menu::ScreenPlace::Options).then_some("options"),
                        "option": screen.option_label.as_ref().map(|label| json!({ "label": label, "default": screen.option_default })),
                        "images": screen.images,
                    })
                })
                .collect();
            println!("{}", json!({ "type": "result", "result": { "screens": screens } }));
            Ok(())
        }
        "volume-markup" => {
            let input = read_request()?;
            #[derive(Deserialize)]
            struct Request {
                design: PathBuf,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid volume-markup request: {error}"))?;
            let words = menu::Manifest::load(&request.design)?.words;
            let markup = menu::volume_control_markup(&request.design, &words)?;
            println!(
                "{}",
                json!({ "type": "result", "result": { "markup": markup } })
            );
            Ok(())
        }
        // We render a preview from the same GLSL as in the exported game, and
        // not from a description of it.
        "shader-sources" => {
            let listed: Vec<_> = shaders::sources()?
                .into_iter()
                .map(|(entry, glsl)| {
                    json!({
                        "id": entry.id,
                        "name": entry.name,
                        "detail": entry.detail,
                        "glsl": glsl,
                    })
                })
                .collect();
            println!(
                "{}",
                json!({ "type": "result", "result": { "shaders": listed } })
            );
            Ok(())
        }
        "shaders-check" => {
            let input = read_request()?;
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
            println!(
                "{}",
                json!({ "type": "result", "result": { "shaders": presets } })
            );
            Ok(())
        }
        "freeze-macos-executable" => {
            let input = read_request()?;
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
