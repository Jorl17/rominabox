//! Headless JSON-lines interface to the engine behind the desktop app.

use rominabox_desktop::{
    builder, controls, cores, game::Game, menu, hotkeys::Hotkeys, metadata, packaging,
    projects, shaders, systems, themes, traveling,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InspectRequest {
    rom: PathBuf,
    /// The builder's lookup cache when absent.
    #[serde(default)]
    cache: Option<PathBuf>,
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

/// Print what we show in the builder's details step for `game`, before we
/// export or save it. That is the lookup, when there was one, the files that go
/// with the game, and for a console that takes a BIOS, the assessment of the
/// files given. We print no content line for a set of files we refuse in
/// export, and refuse it with its own error in the export or save that follows.
fn print_details(game: &Game, identified: Option<&metadata::Inspection>) {
    if let Some(found) = identified {
        println!("{}", json!({ "type": "identified", "identified": found }));
    }
    if let Ok(content) = traveling::files_for(&game.rom, Some(&game.system)) {
        println!("{}", json!({ "type": "content", "content": content }));
    }
    if let Some(system) = systems::find(&game.system).filter(|system| !system.firmware.is_empty()) {
        let assessment = systems::assess_firmware(system, &game.firmware);
        println!("{}", json!({ "type": "firmware", "firmware": assessment }));
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
        println!("ROM-in-a-Box native authoring CLI\n\nUsage: rominabox-cli <inspect|content|systems|controls|stage-controls|preview|export|firmware|hotkeys-check|project-save|project-open|volume-markup|design-screens|shaders|shaders-check|designs|defaults|places|cores|schemas|freeze-macos-executable>\n       rominabox-cli export GAME [FOLDER]\n\nA command that takes a request reads one JSON object from stdin through EOF. Progress and results are JSON Lines on stdout.\ncontent names every file export will copy for a dropped path.\nexport makes the game that dropping GAME into the builder makes, in FOLDER or the builder's; a request on stdin can say more, and whatever it leaves out is the builder's. Before it starts it prints what the builder's details step shows: the lookup, the files that travel with the game and the BIOS assessment.\nproject-save completes its game as export does.\nexport and project-save accept includeAchievements (default true); player authentication is per game.\nshaders prints the catalog. shaders-check reads a selection on stdin.\ndesigns lists the menu designs, palettes and sound packs. defaults prints the settings a request leaves out, places the folders and platform it leaves to the builder.\npreview draws the builder's menu preview; hotkeys-check checks hotkeys as export does.\ncores fetches the download list for one target into cache.\nfreeze-macos-executable is a developer-only macOS runtime-kit preparation command.");
        return Ok(());
    }
    if command == "schemas" {
        let targets = rominabox_desktop::target::Target::ALL.map(|target| target.key()).join(" | ");
        let custom_shader = json!({ "name?": "what the game's list calls it; without one, the file's name without its shader extension", "path": "path" });
        let hotkey_ids = format!(
            "<{}>",
            rominabox_desktop::hotkeys::Hotkey::all().map(|hotkey| hotkey.id()).collect::<Vec<_>>().join("|")
        );
        println!(
            "{}",
            json!({
                "inspect": { "request": ["rom", "cache?", "online?", "system?"], "cache": "where lookups are cached; defaults to the builder's", "result": "Inspection" },
                "content": { "request": ["rom", "system?"], "result": { "entry": "the game file collect ran on", "files": ["relative names, entry first, then whatever rides with it"] } },
                "systems": { "request": ["runtimeKit?"], "result": "System declarations and optional available system IDs" },
                "controls": { "request": ["system", "profile?"], "result": "Controller profile, console labels, stable IDs and default keys; variants lists the profiles the console offers, whose id an export names as controls.profile" },
                "preview": { "request": ["outputDir", "theme?", "palette?", "background?", "design?", "assets?", "renderer?", "width?", "height?"], "omitted": "the builder's Menu step preview: theme's design (else the builder's) from the runtime kit, the builder's palette, the kit's controller artwork, the builder's renderer, 960 by 600", "design": "a design's folder, for one the kit does not carry", "result": { "imagePath": "path" } },
                "export": { "request": ["rom", "title?", "system?", "icon?", "background?", "showMenu?", "startAtMenu?", "theme?", "palette?", "menuSounds?", "controls?", "hotkeys?", "firmware?", "splash?", "includeAchievements?", "advancedEmulatorAccess?", "intelMacs?", "keepPlayingInBackground?", "autosaveOnQuit?", "shaders?", "menuEntries?", "outputDir?", "replace?", "target?", "bothPlatforms?", "runtimeKit?", "core?", "coreCache?", "online?", "metadataCache?"], "arguments": "export GAME [FOLDER] is the request {rom: GAME, outputDir: FOLDER} and reads nothing from stdin", "omitted": "what a request leaves out is what dropping rom into the builder gives: without title or system the game is identified as inspect does, which fills in the title, system and icon it leaves out (one that names both is not looked up); each setting the builder's default (the defaults command); outputDir ROM-in-a-Box in Downloads; target this machine; runtimeKit the one beside this command, else this checkout's; coreCache the builder's. What a request states wins, null included", "online": "defaults true, and the lookup may download catalogues and covers; false uses only what metadataCache already holds. A test that looks a game up passes false", "metadataCache": "where lookups are cached; defaults to the builder's", "coreCache": "directory of downloaded cores; each needed core is downloaded or updated there first, and a progress event carrying cores {kind: fetching, downloading, updating} or {kind: failed, missing} says so; null takes the core from runtimeKit alone", "controls": { "profile": "optional controller variant ID", "bindings": { "<control-id>": ["label?", "key?", "pad?", "mouse?"] }, "pad": "the pad position the control is read from, one of controls.json padPositions, a stick's directions among them; a direction moves with its opposite, and a position another offered pad's control keeps cannot be taken" }, "hotkeys": { (hotkey_ids.clone()): ["key:<RetroArch key name>", "pad:<pad button position or home>[+<pad input>...]"], "omitted": "a hotkey left out keeps the defaults command's; menu keeps a key, confirm and back a binding, and the others may be left with none; an input is held by one hotkey, or by both menu and back; a hotkey that acts while the game plays (menu, quick-save, quick-load, previous-slot, next-slot) holds none of the game's keys" }, "shaders": { "bundled": ["catalog id"], "custom": [custom_shader.clone()], "initial": "bundled id or absent for unfiltered" }, "includeAchievements": "defaults true; effective only with showMenu; packages authenticated Casual support, not account data or downloaded rules", "intelMacs": "defaults false; a macos game also runs on Intel Macs: its player, core and launcher carry x86_64 code beside arm64, the core for macos-x86_64 fetched into a folder of that name beside coreCache. Refused when the runtime kit's player has no x86_64 code", "menuEntries": "option entry ids; omit for resolved defaults; an explicit list must agree with includeAchievements", "replace": "replace an app already at the destination; without it such an export does nothing and prints {type: exists, appPath}", "bothPlatforms": "defaults false; the game for Mac and for Windows, each from its platform's kit, in one <title>.zip holding Mac/<title>.app and Windows/<title>; intelMacs is the Mac game's", "events": ["identified", "content", "firmware", "progress", "result", "exists", "error"], "identified": "the lookup, as inspect prints it; absent for a request that names title and system", "content": "the files that travel with the game, as content prints them", "firmware": "for a console that takes a BIOS, the assessment firmware prints for the files given" },
                "firmware": { "request": ["system", "files?"], "result": "FirmwareAssessment" },
                "hotkeys-check": { "request": { "hotkeys?": { (hotkey_ids.clone()): ["binding"] }, "system?": "the game's console, whose keys a hotkey that acts while it plays may not hold", "controls?": "as export takes them" }, "result": { "hotkeys": "every hotkey's bindings, the builder's for a hotkey left out" }, "refused": { "type": "error", "message": "the sentence export refuses with", "refusal": { "kind": "noBinding | noKey | twice | shared | gameKey | controls", "hotkey?": "id", "binding?": "binding", "other?": "id", "control?": "the game's control whose key it is", "message?": "why the game's keys could not be read" } } },
                "places": { "request": [], "result": { "outputDir": "where export writes a game", "target": "the platform export makes games for", "runtimeKit": "the kit beside this command, else this checkout's", "coreCache": "the builder's downloaded cores for this machine", "metadataCache": "the builder's lookup cache" } },
                "cores": { "request": ["cache", "target"], "target": targets, "result": "per-core present, installed, unreachable or notRecorded" },
                "project-save": { "request": ["archivePath", "settings"], "settings": ["rom", "title?", "system?", "icon?", "background?", "showMenu?", "startAtMenu?", "theme?", "palette?", "menuSounds?", "controls?", "hotkeys?", "firmware?", "splash?", "includeAchievements?", "advancedEmulatorAccess?", "intelMacs?", "keepPlayingInBackground?", "autosaveOnQuit?", "shaders?", "menuEntries?", "target?", "bothPlatforms?", "online?", "metadataCache?"], "omitted": "the game is completed as export completes it", "events": ["identified", "content", "firmware", "result", "error"], "result": "ProjectArchiveResult" },
                "shaders": { "request": [], "result": "Catalog presets an author can bundle" },
                "designs": { "request": [], "result": "The menu designs (theme ids), palettes and sound packs an export can name" },
                "defaults": { "request": [], "result": "The builder's settings, which a request leaves out" },
                "shaders-check": { "request": { "bundled": ["catalog id"], "custom": [custom_shader.clone()], "initial": "optional id" }, "result": "Resolved shaders, and warnings for the author's filters a Windows game may not load, or an error" },
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
    if command == "places" {
        let places = builder::Places::of(builder::identifier());
        let target = packaging::ExportTarget::of_host();
        let core_cache = target
            .as_ref()
            .and_then(|platform| platform.target())
            .map(|core_target| places.core_cache(core_target))
            .transpose()?;
        println!(
            "{}",
            json!({ "type": "result", "result": {
                "outputDir": builder::destination()?,
                "target": target,
                "runtimeKit": builder::runtime_kit(),
                "coreCache": core_cache,
                "metadataCache": places.metadata_cache()?,
            }})
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
            let variants: Vec<Value> = controls::variants_for_system(&request.system)?
                .into_iter()
                .map(|variant| json!({"id": variant.id, "name": variant.name}))
                .collect();
            let mut result = serde_json::to_value(profile).map_err(|error| error.to_string())?;
            result["variants"] = json!(variants);
            println!("{}", json!({"type":"result", "result":result}));
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
            let cache = match request.cache {
                Some(cache) => cache,
                None => builder::Places::of(builder::identifier()).metadata_cache()?,
            };
            let result = metadata::inspect_game_with_system(
                &request.rom,
                &cache,
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
        "hotkeys-check" => {
            let input = read_request()?;
            #[derive(Deserialize)]
            struct Request {
                #[serde(default = "rominabox_desktop::builder::unstated::hotkeys")]
                hotkeys: Hotkeys,
                /// The game's console. A hotkey that works during play may not
                /// use the keys of this console.
                system: Option<String>,
                #[serde(default)]
                controls: controls::Controls,
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|error| format!("invalid hotkeys request: {error}"))?;
            let checked = match &request.system {
                Some(system) => request.hotkeys.check_for(system, &request.controls),
                None => request.hotkeys.check(),
            };
            match checked {
                Ok(()) => println!(
                    "{}",
                    json!({ "type": "result", "result": { "hotkeys": request.hotkeys } })
                ),
                Err(refusal) => {
                    println!(
                        "{}",
                        json!({ "type": "error", "message": refusal.to_string(), "refusal": refusal })
                    );
                    std::process::exit(1);
                }
            }
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
            let completed = builder::complete_export(request)?;
            print_details(&completed.request.game, completed.identified.as_ref());
            let cancelled = AtomicBool::new(false);
            let report = |event| println!("{}", json!({ "type": "progress", "progress": event }));
            let result = builder::export(
                &completed.request,
                builder::runtime_kit().as_deref(),
                &builder::Places::of(builder::identifier()),
                &cancelled,
                report,
            );
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
            // We complete the game as in `export`, so for a request with
            // only its file we save what we save for it in the builder.
            let mut request: Map<String, Value> = serde_json::from_str(&input)
                .map_err(|error| format!("invalid project save request: {error}"))?;
            let Some(Value::Object(settings)) = request.remove("settings") else {
                return Err("a project-save request names its game as settings".into());
            };
            let completed = builder::complete_game(settings)?;
            print_details(&completed.request, completed.identified.as_ref());
            request.insert(
                "settings".into(),
                serde_json::to_value(&completed.request).map_err(|error| error.to_string())?,
            );
            let request: projects::ProjectSaveRequest = serde_json::from_value(Value::Object(request))
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
                /// The hotkeys. For one left out, we use the builder's.
                #[serde(default = "rominabox_desktop::builder::unstated::hotkeys")]
                hotkeys: rominabox_desktop::hotkeys::Hotkeys,
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
                hotkeys: request.hotkeys,
                menu_entries: request.menu_entries,
                include_achievements: request.include_achievements,
                shaders: request.shaders,
                // We take a catalog preset's files from the kit of this command,
                // as we take them in an export from the kit we make it from.
                shader_library: builder::runtime_kit()
                    .map(|kit| shaders::kit_library(&kit))
                    .unwrap_or_default(),
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
            let request: Map<String, Value> = serde_json::from_str(&input)
                .map_err(|error| format!("invalid preview request: {error}"))?;
            let request = builder::complete_preview(request, &builder::resource)?;
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
            let warnings = shaders::windows_warnings(&selection)?;
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
                json!({ "type": "result", "result": { "shaders": presets, "warnings": warnings } })
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
