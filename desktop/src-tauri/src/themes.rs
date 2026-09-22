//! Menu design and palette declarations shared by previews and exports.
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

#[derive(Debug, Deserialize, Serialize)]
pub struct Design {
    pub id: String,
    pub name: String,
    pub slots: u8,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub screen: String,
    pub background: String,
    pub surface: String,
    pub picture: String,
    pub edge: String,
    pub highlight: String,
    pub muted: String,
    pub focus: String,
}
/// One pack is one complete set of the four menu cues, `up`, `down`, `ok` and
/// `cancel`. Packs have no variants or layers. `off` is the one entry with no
/// assets, and we write it as `audio_enable_menu=false` at export.
#[derive(Debug, Deserialize, Serialize)]
pub struct SoundPack {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// The basenames of menu sounds in RetroArch. A pack must have all of them.
pub const SOUND_CUES: [&str; 4] = ["up.wav", "down.wav", "ok.wav", "cancel.wav"];
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub designs: Vec<Design>,
    pub palettes: Vec<Palette>,
    pub sound_packs: Vec<SoundPack>,
}
pub fn registry() -> Result<Registry, String> {
    serde_json::from_str(include_str!("../../designs.json")).map_err(|e| e.to_string())
}

/// The directory that contains a design's documents and fonts.
///
/// A design defines its screens as well as its colours, and another design may
/// lay out the menu in a different way (three save slots instead of six), so
/// each design is a directory and not only a name.
pub fn design_root(design: &str) -> Result<PathBuf, String> {
    let declared = registry()?
        .designs
        .into_iter()
        .find(|entry| entry.id == design)
        .ok_or_else(|| format!("Unknown menu design: {design}"))?;
    // We resolve the path against this crate and not the working directory,
    // because the builder does not run from the repository root. We check the
    // path on disk, so we fail here for a design with no directory.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../integrations/designs")
        .join(&declared.id);
    if !root.is_dir() {
        return Err(format!(
            "Menu design '{design}' is declared but its package is missing at {}",
            root.display()
        ));
    }
    Ok(root)
}

/// The files in every design, under the names declared in it.
/// The geometry of the scene, written from the declaration in the design.
///
/// We append the scene size, marker diameter, callout size and stick strip
/// here from the declaration, like the palette block, so the frame is the same
/// in the stylesheet and in `scripts/render_control_overlays.py`.
/// The frame in which a design draws its controller scene.
///
/// We read it from the design, the only source for these numbers.
/// `control_group_markup`, the stylesheet and
/// `scripts/render_control_overlays.py` all use the declaration, so a change
/// to it moves the CSS box and the generated coordinates together.
#[derive(Clone, Copy)]
pub struct SceneMetrics {
    pub scene_width: i32,
    pub scene_height: i32,
    pub callout_width: i32,
    pub callout_height: i32,
    /// The part of a callout drawn outside its declared size. The leader ends at
    /// the drawn edge, not the content edge, and we use this value in both the
    /// exporter and the builder.
    pub callout_border: i32,
    pub marker: i32,
    pub group_width: i32,
    pub group_height: i32,
    pub group_gap: i32,
    pub group_bottom_margin: i32,
}

impl Default for SceneMetrics {
    /// The values in the stylesheet, for a staged kit that contains the
    /// documents but not the declaration. Equal to `integrations/designs/native`.
    fn default() -> Self {
        Self {
            scene_width: 960,
            scene_height: 380,
            callout_width: 196,
            callout_height: 54,
            callout_border: 2,
            marker: 42,
            group_width: 236,
            group_height: 62,
            group_gap: 16,
            group_bottom_margin: 12,
        }
    }
}

/// A screen in the in-game menu, as declared in the design.
///
/// In the player we read these declarations and no fixed panel ids or heading
/// strings, so a new screen requires no change to the player and is not tied
/// to one design.
///
/// The words of the heading and the footer hint come from the design, so
/// that each design can word them differently, also in another language.
pub struct Screen {
    pub id: String,
    pub panel: String,
    pub heading: String,
    pub footer: String,
    /// The button that opens this screen. A back button is the button that
    /// opens the screen behind, so we need no separate kind for it.
    pub button: String,
}

/// The screens for a design that declares none, the two default screens with
/// the default words of the player.
fn built_in_screens() -> Vec<Screen> {
    vec![
        Screen {
            id: "pause".into(),
            panel: "pause-panel".into(),
            heading: "GAME PAUSED".into(),
            footer: "ESC  CONTINUE".into(),
            button: "controls-back".into(),
        },
        Screen {
            id: "controls".into(),
            panel: "controls-panel".into(),
            heading: "CONTROLS".into(),
            footer: "ESC  BACK".into(),
            button: "controls".into(),
        },
    ]
}

pub fn declared_screens(design: &Path) -> Result<Vec<Screen>, String> {
    let declaration = design.join("design.json");
    let Ok(text) = fs::read_to_string(&declaration) else {
        return Ok(built_in_screens());
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", declaration.display()))?;
    let Some(listed) = declared.get("screens").and_then(|v| v.as_array()) else {
        return Ok(built_in_screens());
    };
    let mut screens = Vec::new();
    for (index, entry) in listed.iter().enumerate() {
        let at = |key: &str| -> Result<String, String> {
            entry[key]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("screen {index} in {} declares no {key}", declaration.display()))
        };
        screens.push(Screen {
            id: at("id")?,
            panel: at("panel")?,
            heading: at("heading")?,
            footer: at("footer")?,
            // Optional, because a screen we open only in code has no button.
            button: entry["button"].as_str().unwrap_or_default().to_string(),
        });
    }
    if screens.is_empty() {
        return Ok(built_in_screens());
    }
    Ok(screens)
}

/// The screens, written to the file that the player reads.
///
/// We use the same format as for the controller list, a space-separated list
/// of ids and one key per field, read with `config_get_array`. The player
/// code contains the name of no particular screen.
fn screen_declarations(design: &Path) -> Result<String, String> {
    let screens = declared_screens(design)?;
    let ids: Vec<&str> = screens.iter().map(|s| s.id.as_str()).collect();
    let mut text = format!("screens = \"{}\"\n", ids.join(" "));
    for screen in &screens {
        text.push_str(&format!(
            "screen_panel_{id} = \"{}\"\nscreen_heading_{id} = \"{}\"\nscreen_footer_{id} = \"{}\"\nscreen_button_{id} = \"{}\"\n",
            screen.panel,
            screen.heading,
            screen.footer,
            screen.button,
            id = screen.id,
        ));
    }
    Ok(text)
}

/// Everything declared in a design by name, its colours and its geometry.
///
/// A design contains `design(surface)` where a colour goes, and
/// `design(scene-width)dp` where a size goes. We take the value
/// from the chosen palette, or from the `tokens` of the design when the
/// palette does not have it, so a design may have extra colours whose names
/// are not in this code.
///
/// We substitute the values into the rules of the design. Nothing goes after
/// the stylesheet of the design, because appended rules with equal
/// specificity would override the selectors of the design, such as the
/// separate hover, keyboard focus and pressed styles of the picker.
fn design_tokens(
    design: &Path,
    palette: &Palette,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut tokens = std::collections::BTreeMap::new();
    // The values of the design first, so a palette may override any of them.
    if let Ok(text) = fs::read_to_string(design.join("design.json")) {
        let declared: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if let Some(own) = declared.get("tokens").and_then(|v| v.as_object()) {
            for (name, value) in own {
                if let Some(value) = value.as_str() {
                    tokens.insert(name.clone(), value.to_string());
                }
            }
        }
    }
    let m = scene_metrics(design)?;
    for (name, value) in [
        ("scene-width", m.scene_width),
        ("scene-height", m.scene_height),
        ("marker-diameter", m.marker),
        ("marker-radius", m.marker / 2),
        ("callout-width", m.callout_width),
        ("callout-height", m.callout_height),
        ("group-width", m.group_width),
        ("group-height", m.group_height),
    ] {
        tokens.insert(name.to_string(), value.to_string());
    }
    for (name, value) in [
        ("screen", &palette.screen),
        ("background", &palette.background),
        ("surface", &palette.surface),
        ("picture", &palette.picture),
        ("edge", &palette.edge),
        ("highlight", &palette.highlight),
        ("muted", &palette.muted),
        ("focus", &palette.focus),
    ] {
        tokens.insert(name.to_string(), value.clone());
    }
    Ok(tokens)
}

/// Put the colours into the rules of the design.
///
/// The result is the stylesheet of the design with other characters in its
/// values, with the same rules and selectors in the same order. We append
/// nothing, so nothing can override the rules of the design.
fn substitute_tokens(
    css: &str,
    tokens: &std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("design(") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "design(".len()..];
        let close = after
            .find(')')
            .ok_or_else(|| "a design( token is never closed".to_string())?;
        let name = after[..close].trim();
        let value = tokens.get(name).ok_or_else(|| {
            format!(
                "the stylesheet asks for design({name}), which the design does \
                 not declare and no palette names. Declared: {}",
                tokens.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;
        out.push_str(value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

pub fn scene_metrics(design: &Path) -> Result<SceneMetrics, String> {
    let declaration = design.join("design.json");
    let Ok(text) = fs::read_to_string(&declaration) else {
        return Ok(SceneMetrics::default());
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", declaration.display()))?;
    let Some(metrics) = declared.get("metrics") else {
        return Ok(SceneMetrics::default());
    };
    let fallback = SceneMetrics::default();
    let at = |group: &str, key: &str, default: i32| -> i32 {
        metrics[group][key].as_i64().map(|v| v as i32).unwrap_or(default)
    };
    Ok(SceneMetrics {
        scene_width: at("scene", "width", fallback.scene_width),
        scene_height: at("scene", "height", fallback.scene_height),
        callout_width: at("callout", "width", fallback.callout_width),
        callout_height: at("callout", "height", fallback.callout_height),
        callout_border: at("callout", "border", fallback.callout_border),
        marker: at("marker", "diameter", fallback.marker),
        group_width: at("group", "width", fallback.group_width),
        group_height: at("group", "height", fallback.group_height),
        group_gap: at("group", "gap", fallback.group_gap),
        group_bottom_margin: at("group", "bottomMargin", fallback.group_bottom_margin),
    })
}

/// Stage only the selected design's assets and apply the same palette/background
/// for both an offscreen preview and an exported player.
/// The folder of the staged files of a design in a prepared kit.
///
/// Each design has a separate folder in the kit, so we stage the set of files
/// for the design id.
pub fn staged_design(kit: &Path, design: &str) -> PathBuf {
    kit.join("designs").join(design)
}

pub fn prepare_theme_assets(
    source: &Path,
    destination: &Path,
    palette: &str,
    background: Option<&Path>,
) -> Result<(), String> {
    let palette = registry()?
        .palettes
        .into_iter()
        .find(|p| p.id == palette)
        .ok_or_else(|| "Choose an available colour palette.".to_string())?;
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for name in [
        "menu.rml",
        "menu.rcss",
        "Silkscreen-Regular.ttf",
        "Silkscreen-OFL.txt",
    ] {
        fs::copy(source.join(name), destination.join(name))
            .map_err(|e| format!("Could not prepare menu asset {name}: {e}"))?;
    }
    // The declarations of the design, in the file that the player reads. We
    // write them next to the stylesheet because both belong to the design. A
    // design lists its screens, and we show them by name in the player.
    fs::write(
        destination.join("design.cfg"),
        screen_declarations(source)?,
    )
    .map_err(|e| format!("Could not write the design's declarations: {e}"))?;
    let mut css = fs::read_to_string(destination.join("menu.rcss")).map_err(|e| e.to_string())?;
    // The colours of the design, in the rules of the design. We append nothing,
    // because a palette contains values and no styles. Appended rules would
    // declare selectors of the design again and, coming later with equal
    // specificity, override them.
    css = substitute_tokens(&css, &design_tokens(source, &palette)?)?;
    if let Some(image_path) = background {
        let image = crate::icons::read_image(image_path).map_err(|e| e.to_string())?;
        image
            .resize(1920, 1200, image::imageops::FilterType::Lanczos3)
            .save_with_format(destination.join("background.png"), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        css.push_str("\n#screen { decorator: image(\"background.png\" cover); }\n");
    }
    fs::write(destination.join("menu.rcss"), css).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub assets: std::path::PathBuf,
    pub renderer: std::path::PathBuf,
    pub output_dir: std::path::PathBuf,
    pub palette: String,
    pub background: Option<std::path::PathBuf>,
    pub width: u32,
    pub height: u32,
}

/// Render with the windowless RmlUi helper. We keep the intermediate files in the
/// given output folder for inspection.
pub fn render_preview(request: &PreviewRequest) -> Result<std::path::PathBuf, String> {
    if !(320..=3840).contains(&request.width) || !(200..=2400).contains(&request.height) {
        return Err("Preview dimensions are outside the supported range.".into());
    }
    prepare_theme_assets(
        &request.assets,
        &request.output_dir,
        &request.palette,
        request.background.as_deref(),
    )?;
    prepare_controls_assets(
        &request.assets,
        // The preview assets are a design folder, so the frame is declared
        // there.
        &request.assets,
        &request.output_dir,
        "megadrive",
        &crate::controls::Controls::default(),
    )?;
    let output = request.output_dir.join("preview.png");
    let run = std::process::Command::new(&request.renderer)
        .arg(request.output_dir.join("menu.rml"))
        .arg(&output)
        .arg(request.width.to_string())
        .arg(request.height.to_string())
        .output()
        .map_err(|e| e.to_string())?;
    if !run.status.success() {
        return Err(format!(
            "Menu renderer failed: {}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(output)
}

/// Copy only the selected menu cue pack. With Off, we add no audio.
pub fn prepare_sound_assets(source: &Path, destination: &Path, pack: &str) -> Result<(), String> {
    if !registry()?.sound_packs.iter().any(|sound| sound.id == pack) {
        return Err("Choose an available menu sound pack.".into());
    }
    if pack == "off" {
        return Ok(());
    }
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for name in SOUND_CUES {
        fs::copy(source.join(pack).join(name), destination.join(name))
            .map_err(|e| e.to_string())?;
    }
    fs::copy(
        source.join("PROVENANCE.txt"),
        destination.join("PROVENANCE.txt"),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn default_menu_sounds() -> String {
    "off".into()
}

/// Copy one staged file, and reject a copy onto itself.
///
/// Copying a file onto itself empties it, so staging into the folder that we
/// read from would destroy the artwork that we are about to use.
fn stage_file(source: &Path, destination: &Path, name: &str) -> Result<(), String> {
    let from = source.join(name);
    let to = destination.join(name);
    // When both sides canonicalize to None, neither exists, and they are not
    // the same file. We report missing artwork as missing.
    let same_file = match (from.canonicalize(), to.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if from == to || same_file {
        return Err(format!(
            "refusing to stage {name} onto itself: source and destination are \
             the same directory, which would truncate the artwork"
        ));
    }
    fs::copy(&from, &to)
        .map_err(|e| format!("Could not prepare controller artwork {name}: {e}"))?;
    Ok(())
}

/// Generate the hit regions and callouts for every controller offered for
/// the console, from the same declaration as in the builder.
///
/// We stage the illustration of every offered pad, and write the scene of
/// each next to the menu as `scene-<id>.rml`. Only the chosen one goes into
/// the document. The others are there so the player can switch to them, and
/// when the player picks another pad, both the emulated device and the
/// drawing change.
pub fn prepare_controls_assets(
    source: &Path,
    design: &Path,
    destination: &Path,
    system: &str,
    controls: &crate::controls::Controls,
) -> Result<(), String> {
    // We take the frame from the design, so a change of the scene position in
    // design.json moves the box in the stylesheet and every generated
    // coordinate together.
    let metrics = scene_metrics(design)?;
    let profile = crate::controls::validate_for_system(system, controls)?;
    let offered = carried(system, &profile)?;
    let mut staged: Vec<&str> = Vec::new();
    for image in offered
        .iter()
        .map(|entry| entry.image.as_str())
        .filter(|image| !image.is_empty())
    {
        if staged.contains(&image) {
            // Two variants can share a drawing: a PlayStation Dual Analog is a
            // DualShock without the vibration.
            continue;
        }
        stage_file(source, destination, image)?;
        staged.push(image);
    }
    if !profile.image.is_empty() {
        stage_file(source, destination, "CONTROLLERS.txt")?;
    }
    // We write the scene of every offered pad next to the menu. In the player
    // we switch pads by loading one of these files, because we cannot
    // generate markup there.
    for entry in &offered {
        fs::write(
            destination.join(format!("{}{}.rml", SCENE_PREFIX, entry.id)),
            scene_markup(entry, controls, metrics),
        )
        .map_err(|e| format!("Could not write the scene for {}: {e}", entry.id))?;
    }
    let markup = scene_markup(&profile, controls, metrics);
    let picker = controller_picker_markup(&offered, &profile.id);

    // The document is the menu.rml of the design. If we filled the controls
    // from any other copy, we would lose changes made to the design.
    let template = fs::read_to_string(design.join("menu.rml")).map_err(|e| e.to_string())?;
    // The picker is a sibling of the scene, not a child of it. Inside the
    // scene its coordinates would be scene coordinates, and the scene starts
    // 80 dp down the screen, so the picker would cover the first two
    // callouts. We reject a design without a marker for it, so that no game
    // is exported without a way to change controller.
    if !picker.is_empty() && !template.contains(PICKER_SLOT) {
        return Err(format!(
            "this design has no {PICKER_SLOT} for the controller picker, and \
             {} offers more than one controller. Add the slot to menu.rml, \
             outside #controller-scene.",
            system
        ));
    }
    fs::write(
        destination.join("menu.rml"),
        template
            .replace("<!--CONTROLS-->", &markup)
            .replace(PICKER_SLOT, &picker),
    )
    .map_err(|e| e.to_string())
}

/// The pads in an export: every pad in the picker.
///
/// We use this one rule for the artwork staging, the scene files and the
/// picker, because they have to agree. We do not copy a console illustration
/// for a pad that is not in the picker.
///
/// With fewer than two there is no picker, and we export only the chosen pad.
fn carried(
    system: &str,
    profile: &crate::controls::ControlProfile,
) -> Result<Vec<crate::controls::ControlProfile>, String> {
    let offered = crate::controls::variants_for_system(system)?;
    let swappable = offered.len() > 1 && offered.iter().any(|entry| entry.id == profile.id);
    Ok(if swappable {
        offered
    } else {
        vec![profile.clone()]
    })
}

/// The marker for the controller picker in a design. It is separate from the
/// marker for the scene, because the picker is not part of the scene.
const PICKER_SLOT: &str = "<!--CONTROLLER-PICKER-->";

/// The scene of each offered pad, next to the menu, in a file named after the
/// pad id. We load one in the player when someone picks another controller.
pub const SCENE_PREFIX: &str = "scene-";

/// One controller's scene: its illustration, hit regions, callouts and groups.
///
/// We write it once for each offered pad, because a player who picks another
/// pad needs its scene, and we cannot generate markup in the game.
fn scene_markup(
    profile: &crate::controls::ControlProfile,
    controls: &crate::controls::Controls,
    metrics: SceneMetrics,
) -> String {
    let illustrated = !profile.image.is_empty();
    let mut markup = if illustrated {
        format!("<img id=\"controller-image\" src=\"{}\"/>", profile.image)
    } else {
        String::new()
    };
    // We draw a group once, as one object on the pad, not one callout per
    // bind. Otherwise each of the eight analogue directions of the PlayStation
    // DualShock would need a callout, and both gutters are already full with
    // seven 54 dp callouts.
    let grouped: Vec<&crate::controls::ControlDefinition> = profile
        .controls
        .iter()
        .filter(|item| item.group.is_some())
        .collect();
    let mut group_names: Vec<&str> = grouped
        .iter()
        .filter_map(|item| item.group.as_deref())
        .collect();
    group_names.sort_unstable();
    group_names.dedup();
    markup.push_str(&control_group_markup(&group_names, &grouped, controls, illustrated, metrics));
    let placed_scene = crate::scene_layout::layout(&profile.controls, metrics);
    let placements = &placed_scene.controls;
    for item in profile.controls.iter().filter(|item| item.group.is_none()) {
        let item = item.clone();
        let custom = controls.bindings.get(&item.id);
        let author_label = custom
            .and_then(|value| value.label.as_deref())
            .filter(|label| !label.trim().is_empty());
        let label = author_label.unwrap_or(&item.label);
        let key = custom
            .and_then(|value| value.key.as_deref())
            .unwrap_or(&item.key);
        let original = author_label
            .filter(|value| value.trim() != item.label.trim())
            .map(|_| item.label.as_str());
        let id = item.id.as_str();
        let cx = item.callout_x;
        let cy = item.callout_y;
        if illustrated {
            // We take the placement from the shared scene layout, so these
            // are in the same place in the exporter, the builder and the
            // overlay renderer.
            let placed = placements
                .iter()
                .find(|placement| placement.id == item.id)
                .expect("every drawn control is placed");
            for run in &placed.leader {
                let orientation = if run.height == 0 { "horizontal" } else { "vertical" };
                let extent = if run.height == 0 {
                    format!("width:{}dp;", run.width)
                } else {
                    format!("height:{}dp;", run.height)
                };
                markup.push_str(&format!(
                    "\n<div class=\"control-leader {orientation}\" style=\"left:{}dp;top:{}dp;{extent}\"/>",
                    run.x, run.y
                ));
            }
            markup.push_str(&format!(
                "\n<button id=\"control-hit-{id}\" class=\"control-hit\" style=\"left:{}dp;top:{}dp;\"/>\n",
                placed.marker.x, placed.marker.y
            ));
        }
        markup.push_str(&control_callout_markup(id, label, original, key, cx, cy));
    }
    markup
}


/// The in-game controller picker.
///
/// We draw it at the place set in the design: the markup has no coordinates,
/// and we place it with `menu.rcss`. In the native design it is on the action
/// row beside BACK and RESET DEFAULTS, the one band that no console's pad
/// covers. Both gutters are full of callouts, the heading is at the top
/// centre, and the scene fills everything between.
///
/// We emit it only when there is a choice. A dropdown with one option is
/// noise, and most consoles have exactly one pad.
fn controller_picker_markup(offered: &[crate::controls::ControlProfile], chosen: &str) -> String {
    if offered.len() < 2 {
        return String::new();
    }
    let chosen_name = offered
        .iter()
        .find(|entry| entry.id == chosen)
        .map(|entry| rml_text(&entry.name.to_uppercase()))
        .unwrap_or_default();
    let mut markup = format!(
        r#"
<div id="controls-device" class="control-picker">
<div id="controls-device-label" class="control-picker-label">CONTROLLER</div>
<button id="controls-device-current" class="control-picker-current">{chosen_name}</button>
<div id="controls-device-list" class="control-picker-list" style="display:none;">
"#,
    );
    for entry in offered {
        let selected = if entry.id == chosen { " selected" } else { "" };
        markup.push_str(&format!(
            r#"<button id="controls-device-option-{}" class="control-picker-option{selected}">{}</button>
"#,
            entry.id,
            rml_text(&entry.name.to_uppercase()),
        ));
    }
    markup.push_str("</div>
</div>
");
    markup
}

/// Draw each group once, below the illustration.
///
/// We put the strip at the bottom of the scene because the side margins are
/// full, with seven 54 dp callouts filling 378 of 380 dp. We take its geometry
/// from the declaration in the design, so the layout is the same in
/// `scripts/render_control_overlays.py` and in this markup.
#[allow(non_snake_case)]
fn control_group_markup(
    names: &[&str],
    grouped: &[&crate::controls::ControlDefinition],
    controls: &crate::controls::Controls,
    illustrated: bool,
    metrics: SceneMetrics,
) -> String {
    let (WIDTH, HEIGHT, GAP, SCENE_WIDTH, SCENE_HEIGHT) = (
        metrics.group_width,
        metrics.group_height,
        metrics.group_gap,
        metrics.scene_width,
        metrics.scene_height,
    );

    if names.is_empty() {
        return String::new();
    }
    let count = names.len() as i32;
    let total = count * WIDTH + (count - 1) * GAP;
    let left_edge = (SCENE_WIDTH - total) / 2;
    let top = SCENE_HEIGHT - HEIGHT - metrics.group_bottom_margin;

    let mut markup = String::new();
    for (index, name) in names.iter().enumerate() {
        let members: Vec<&&crate::controls::ControlDefinition> = grouped
            .iter()
            .filter(|item| item.group.as_deref() == Some(*name))
            .collect();
        let box_x = left_edge + index as i32 * (WIDTH + GAP);

        // One member has the anchor for the whole group. We reject a group in
        // the catalog without exactly one, so a missing anchor here would be a
        // defect in the generated data, which we must not hide.
        if illustrated {
            if let Some(anchor) = members.iter().find(|item| item.x != 0 || item.y != 0) {
                let centre = box_x + WIDTH / 2;
                markup.push_str(&format!(
                    r#"
<div class="control-leader vertical" style="left:{}dp;top:{}dp;height:{}dp;"/>
<div class="control-leader horizontal" style="left:{}dp;top:{top}dp;width:{}dp;"/>
<button id="control-hit-{}" class="control-hit" style="left:{}dp;top:{}dp;"/>
"#,
                    anchor.x,
                    anchor.y.min(top),
                    (top - anchor.y).abs(),
                    centre.min(anchor.x),
                    (centre - anchor.x).abs(),
                    anchor.id,
                    anchor.x - 22,
                    anchor.y - 22,
                ));
            }
        }

        // The directions are written on one line.
        let keys: Vec<String> = members
            .iter()
            .filter(|item| item.id.ends_with("_plus") || item.id.ends_with("_minus"))
            .map(|item| {
                controls
                    .bindings
                    .get(&item.id)
                    .and_then(|value| value.key.clone())
                    .unwrap_or_else(|| item.key.clone())
                    .to_uppercase()
            })
            .collect();
        let title = name.replace('_', " ").to_uppercase();
        markup.push_str(&format!(
            r#"
<button id="control-group-{name}" class="control-group" style="left:{box_x}dp;top:{top}dp;">
<div class="control-label">{}</div>
<div class="control-assignment">{}</div>
</button>
"#,
            rml_text(&title),
            rml_text(&keys.join(" ")),
        ));
    }
    markup
}

fn control_callout_markup(
    id: &str,
    label: &str,
    original: Option<&str>,
    key: &str,
    cx: i32,
    cy: i32,
) -> String {
    let label = rml_text(label);
    let key = rml_text(key);
    let original = original
        .map(|value| {
            format!(
                r#"<span class="control-original">{}</span>"#,
                rml_text(value)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<button id="control-{id}" class="control-callout" style="left:{cx}dp;top:{cy}dp;"><div id="control-label-{id}" class="control-label">{label}</div><div class="control-assignment">{original}<span id="control-binding-{id}">{key}</span></div></button>"#
    )
}

fn rml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Stage the logo-only document without pause controls, controller art or backgrounds.
pub fn prepare_splash_assets(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for (from, to) in [
        ("splash.rml", "menu.rml"),
        ("menu.rcss", "menu.rcss"),
        ("Silkscreen-Regular.ttf", "Silkscreen-Regular.ttf"),
        ("Silkscreen-OFL.txt", "Silkscreen-OFL.txt"),
    ] {
        fs::copy(source.join(from), destination.join(to))
            .map_err(|e| format!("Could not prepare splash asset {from}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn sound_source() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/menu-sounds")
    }

    /// A pack is one complete set that we can play. We declare no partial
    /// pack, and ship no assets that the author cannot pick.
    #[test]
    fn every_declared_sound_pack_is_one_complete_cue_set() {
        let declared: BTreeSet<String> = registry()
            .unwrap()
            .sound_packs
            .into_iter()
            .map(|pack| pack.id)
            .filter(|id| id != "off")
            .collect();
        assert!(!declared.is_empty(), "no menu sound packs are declared");

        let source = sound_source();
        let mut present = BTreeSet::new();
        for entry in fs::read_dir(&source).expect("menu sound assets") {
            let entry = entry.expect("menu sound entry");
            if entry.file_type().expect("file type").is_dir() {
                present.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
        assert_eq!(
            declared, present,
            "declared packs and shipped pack directories must match exactly"
        );

        for id in &declared {
            for cue in SOUND_CUES {
                let path = source.join(id).join(cue);
                let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert!(bytes.len() > 44, "{} is not a usable WAV", path.display());
                assert_eq!(&bytes[0..4], b"RIFF", "{} is not RIFF", path.display());
                assert_eq!(&bytes[8..12], b"WAVE", "{} is not WAVE", path.display());
                // 44100 Hz, 16-bit, mono, the format we give the RetroArch mixer.
                assert_eq!(
                    u16::from_le_bytes([bytes[22], bytes[23]]),
                    1,
                    "{} is not mono",
                    path.display()
                );
                assert_eq!(
                    u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
                    44_100,
                    "{} is not 44100 Hz",
                    path.display()
                );
                assert_eq!(
                    u16::from_le_bytes([bytes[34], bytes[35]]),
                    16,
                    "{} is not 16-bit",
                    path.display()
                );
            }
        }
    }

    /// In the picker we show a name and a description for every pack, `off` included.
    #[test]
    fn every_sound_pack_is_described_for_the_picker() {
        for pack in registry().unwrap().sound_packs {
            assert!(!pack.name.trim().is_empty(), "{} has no name", pack.id);
            assert!(
                !pack.description.trim().is_empty(),
                "{} has no description",
                pack.id
            );
            assert!(
                !pack.id.contains("--"),
                "{} keeps an authoring variant separator",
                pack.id
            );
        }
    }

    #[test]
    fn unknown_sound_packs_are_rejected_before_staging() {
        let temporary =
            std::env::temp_dir().join(format!("rominabox-sound-pack-{}", std::process::id()));
        let error = prepare_sound_assets(&sound_source(), &temporary, "pulse")
            .expect_err("retired pack must not stage");
        assert!(error.contains("available menu sound pack"), "{error}");
        assert!(!temporary.exists(), "rejection must not create output");
    }

    /// The person who bundles the game picks the BIOS. The player never does.
    ///
    /// We put no BIOS picker and no BIOS uploader in the exported game. A
    /// player who wants a different BIOS goes through Advanced, which unlocks
    /// the whole emulator.
    ///
    /// Everything about the BIOS is in the builder (`assess_firmware`, the
    /// details step, the export refusal), and the player sees none of it. A
    /// design may not contain a screen, a button or a declaration that offers a
    /// BIOS choice. We still bundle a BIOS, with no way to change it in the menu.
    #[test]
    fn no_design_offers_the_player_a_bios() {
        let designs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs");
        let mut looked = 0;
        for entry in fs::read_dir(&designs).expect("designs directory") {
            let design = entry.expect("design entry").path();
            if !design.is_dir() {
                continue;
            }
            for file in fs::read_dir(&design).expect("design files") {
                let file = file.expect("design file").path();
                let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.ends_with(".rml") && !name.ends_with(".rcss") && !name.ends_with(".json") {
                    continue;
                }
                let body = fs::read_to_string(&file).unwrap_or_default();
                looked += 1;
                for (number, line) in body.lines().enumerate() {
                    assert!(
                        !line.to_ascii_lowercase().contains("bios"),
                        "{}:{} offers the player a BIOS: {}\n\
                         The BIOS is chosen by whoever bundles the game. A player \
                         who wants another one uses Advanced.",
                        file.display(),
                        number + 1,
                        line.trim()
                    );
                }
            }
        }
        assert!(looked > 0, "no design files were read, so this proved nothing");
    }
}
