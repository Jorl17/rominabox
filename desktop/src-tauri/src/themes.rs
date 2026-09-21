//! Menu design and palette declarations shared by previews and exports.
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

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
#[derive(Debug, Deserialize, Serialize)]
pub struct SoundPack {
    pub id: String,
    pub name: String,
}
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

/// Stage only the selected design's assets and apply the same palette/background
/// for both an offscreen preview and an exported player.
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
    let mut css = fs::read_to_string(destination.join("menu.rcss")).map_err(|e| e.to_string())?;
    css.push_str(&format!(r#"
body {{ background-color: {background}; }}
#screen {{ background-color: {screen}; border-color: {edge}; }}
#heading, #status {{ color: {highlight}; }}
.slot {{ background-color: {surface}; border-color: {edge}; }}
.slot.focused, .slot:hover {{ border-color: #ffffff; }}
.slot.selected, .slot.selected:hover, .slot.selected.focused {{ border-color: {highlight}; }}
.slot:active, .slot.selected:active {{ border-top-color: {background}; border-left-color: {background}; border-bottom-color: #ffffff; border-right-color: #ffffff; }}
.slot.selected .slot-label {{ color: {highlight}; }}
.slot-label {{ background-color: {surface}; }}
.slot-picture {{ background-color: {picture}; border-top-color: {background}; border-left-color: {background}; border-right-color: {edge}; border-bottom-color: {edge}; }}
.slot-state {{ color: {muted}; }}
#footer {{ color: #ffffff; }}
#screen .menu-action {{ background-color: {surface}; color: #ffffff; border-color: {edge}; }}
#screen .menu-action:hover, #screen .menu-action.focused {{ background-color: {highlight}; color: {surface}; border-color: #ffffff; }}
#screen .menu-action:active {{ border-top-color: {background}; border-left-color: {background}; border-bottom-color: #ffffff; border-right-color: #ffffff; }}
#screen .menu-action.disabled, #screen .menu-action:disabled {{ background-color: {background}; color: {edge}; border-color: {surface}; }}
.control-callout {{ background-color: {surface}; border-color: {edge}; }}
.control-callout:hover, .control-hit:hover {{ border-color: #ffffff; }}
.control-callout.focused {{ background-color: {focus}; border-color: {highlight}; }}
.control-hit.focused {{ border-color: {highlight}; }}
.control-original, #controls-status {{ color: {highlight}; }}
.control-assignment {{ color: {muted}; }}
@keyframes capture-pulse {{ from {{ border-color: {highlight}; }} to {{ border-color: transparent; }} }}
"#,background=palette.background,screen=palette.screen,edge=palette.edge,highlight=palette.highlight,surface=palette.surface,focus=palette.focus,picture=palette.picture,muted=palette.muted));
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
    for name in ["ok.wav", "cancel.wav", "up.wav", "down.wav"] {
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

/// Generate the selected controller's hit regions and callouts from the same
/// declaration used by the builder. We copy only the illustration of this profile.
pub fn prepare_controls_assets(
    source: &Path,
    destination: &Path,
    system: &str,
    controls: &crate::controls::Controls,
) -> Result<(), String> {
    let profile = crate::controls::validate_for_system(system, controls)?;
    for name in [&profile.image, &"CONTROLLERS.txt".to_string()]
        .into_iter()
        .filter(|name| !name.is_empty() && !profile.image.is_empty())
    {
        fs::copy(source.join(name), destination.join(name))
            .map_err(|e| format!("Could not prepare controller artwork {name}: {e}"))?;
    }
    let illustrated = !profile.image.is_empty();
    let mut markup = if illustrated {
        format!("<img id=\"controller-image\" src=\"{}\"/>", profile.image)
    } else {
        String::new()
    };
    for item in profile.controls {
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
        let x = item.x - 22;
        let y = item.y - 22;
        let cx = item.callout_x;
        let cy = item.callout_y;
        let edge = if cx < 400 { cx + 200 } else { cx };
        let horizontal_left = edge.min(item.x);
        let horizontal_width = (edge - item.x).abs();
        let vertical_top = (cy + 28).min(item.y);
        let vertical_height = (cy + 28 - item.y).abs();
        if illustrated {
            markup.push_str(&format!(
                r#"
<div class="control-leader horizontal" style="left:{horizontal_left}dp;top:{}dp;width:{horizontal_width}dp;"/>
<div class="control-leader vertical" style="left:{}dp;top:{vertical_top}dp;height:{vertical_height}dp;"/>
<button id="control-hit-{id}" class="control-hit" style="left:{x}dp;top:{y}dp;"/>
"#,
                cy + 28,
                item.x
            ));
        }
        markup.push_str(&control_callout_markup(id, label, original, key, cx, cy));
    }
    let template = fs::read_to_string(source.join("menu.rml")).map_err(|e| e.to_string())?;
    fs::write(
        destination.join("menu.rml"),
        template.replace("<!--CONTROLS-->", &markup),
    )
    .map_err(|e| e.to_string())
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
