//! Shaders that an author can bundle, and the list code for every screen of
//! rows.
//!
//! An achievement list and a shader list work the same way. Each has one row
//! per item, generated when we bundle the game, because we cannot create
//! elements in the player while it runs. We make each row by filling in the
//! row template of the menu design, with the same row for every kind of list.
//!
//! The presets are GLSL. In exported games we set `video_driver` to OpenGL and
//! build without Metal and Vulkan, so we reject slang and Cg presets. We do
//! not enable those drivers in this code.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The choice that means no preset. It is not a file, and an author does not
/// tick it, because when we bundle any preset, the player can always return
/// to the unfiltered picture.
pub const UNFILTERED_ID: &str = "none";

/// Where we insert the rows of a generated screen and the button that opens
/// it. There is one marker for each, shared by every list. Without both
/// markers, we cannot show a generated list in a design.
pub const SCREENS_SLOT: &str = "<!--SCREENS-->";
pub const LINKS_SLOT: &str = "<!--SCREEN-LINKS-->";

/// The row template for a design without its own `row.rml`.
///
/// The holes are the contract: `ROW-ID`, `ICON`, `TITLE`, `DETAIL`, `STATE`,
/// `SELECTED`. The class stays `list-row`, and the id of the state element is
/// `ROW-ID-state`, so we can mark the active item in the player whatever the
/// design of the row.
const BUILT_IN_ROW: &str = concat!(
    "<button id=\"ROW-ID\" class=\"list-row SELECTED\">",
    "<img class=\"list-row-icon\" src=\"ICON\"/>",
    "<div id=\"ROW-ID-title\" class=\"list-row-title\">TITLE</div>",
    "<div id=\"ROW-ID-detail\" class=\"list-row-detail\">DETAIL</div>",
    "<div id=\"ROW-ID-state\" class=\"list-row-state\">STATE</div>",
    "</button>\n",
);

/// A preset for the OpenGL driver. Slang, Cg and Metal are not included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderFormat {
    Glsl,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogFile {
    presets: Vec<CatalogPreset>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogPreset {
    id: String,
    name: String,
    detail: String,
    fragment: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub detail: String,
}

/// The shaders the author chose. When it is empty, which is usual, we add no
/// shader screen and no preset, and the game runs without a shader.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShaderSelection {
    /// Catalog ids. We ignore `none` here, because we add it ourselves.
    #[serde(default)]
    pub bundled: Vec<String>,
    #[serde(default)]
    pub custom: Vec<CustomShader>,
    /// The bundled id we enable at launch, or none for the unfiltered picture.
    #[serde(default)]
    pub initial: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CustomShader {
    pub name: String,
    pub path: PathBuf,
}

impl ShaderSelection {
    pub fn is_empty(&self) -> bool {
        self.bundled.iter().all(|id| id == UNFILTERED_ID) && self.custom.is_empty()
    }
}

/// One item in a bundled list, after we have checked catalog ids and custom
/// files. `preset` is the source `.glslp` when there is one.
#[derive(Clone, Debug)]
pub struct ResolvedShader {
    pub id: String,
    pub name: String,
    pub detail: String,
    /// The path from the staged menu assets, empty for the unfiltered choice.
    pub relative_preset: String,
    /// Files to copy into that directory, as (source, file name).
    files: Vec<(PathBuf, String)>,
    /// The `.glsl` we write when we generate this preset instead of copying it.
    generated: Option<String>,
}

/// One row, the same struct that we use for an achievement.
#[derive(Clone, Debug)]
pub struct ListItem {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub detail: String,
    pub state: String,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct StagedShaders {
    /// Markup for [`LINKS_SLOT`]. Empty when nothing was bundled.
    pub links: String,
    /// Markup for [`SCREENS_SLOT`].
    pub screens: String,
    /// The text of `shaders.cfg` in the game, or empty when we bundle nothing.
    pub config: String,
    /// The preset to enable at launch, relative to the menu assets, if any.
    pub initial_relative: Option<String>,
}

fn catalog_file() -> Result<CatalogFile, String> {
    let text = include_str!("../../../integrations/shaders/catalog.json");
    serde_json::from_str(text).map_err(|error| format!("shader catalog is not readable: {error}"))
}

pub fn catalog() -> Result<Vec<CatalogEntry>, String> {
    Ok(catalog_file()?
        .presets
        .into_iter()
        .map(|preset| CatalogEntry {
            id: preset.id,
            name: preset.name,
            detail: preset.detail,
        })
        .collect())
}

fn require_id(id: &str) -> Result<(), String> {
    let ok = (1..=32).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--");
    if ok {
        return Ok(());
    }
    Err(format!(
        "shader id '{id}' must be a short lowercase name"
    ))
}

/// The row template of the design, or the built-in one. The markup in a
/// design's `row.rml` can be anything, but it still has to be a row.
pub fn row_template(design: &Path) -> Result<String, String> {
    let path = design.join("row.rml");
    if !path.is_file() {
        return Ok(BUILT_IN_ROW.to_string());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read the row template: {error}"))?;
    if !text.contains("ROW-ID") || !text.contains("list-row") {
        return Err(
            "a row template must keep the ROW-ID hole and the list-row class".into(),
        );
    }
    Ok(text)
}

/// How many rows fit on one page, as declared in the design. When it is absent,
/// we use four, which fit under the Native heading and above its back button.
pub fn page_size(design: &Path) -> Result<usize, String> {
    let path = design.join("design.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(4);
    };
    let declared: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    match declared
        .get("list")
        .and_then(|list| list.get("pageSize"))
        .and_then(|value| value.as_u64())
    {
        None => Ok(4),
        Some(0) => Err("list.pageSize must be at least 1".into()),
        Some(size) => Ok(size as usize),
    }
}

fn rml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fill the row template. We replace `ROW-ID` before the other placeholders,
/// so that text in a title cannot add a placeholder.
pub fn render_row(template: &str, item: &ListItem) -> String {
    let selected = if item.selected { "selected" } else { "" };
    let mut html = template
        .replace("ROW-ID", &item.id)
        .replace("ICON", &item.icon)
        .replace("TITLE", &rml_text(&item.title))
        .replace("DETAIL", &rml_text(&item.detail))
        .replace("STATE", &rml_text(&item.state))
        .replace("SELECTED", selected);
    if item.icon.is_empty() {
        // A row with no picture. We remove the img, because loading a missing
        // texture in RmlUi makes the render fail.
        html = html.replace("<img class=\"list-row-icon\" src=\"\"/>", "");
    }
    html
}

/// One list, with the same template on every page, and a pager only when the
/// items do not fit on one page. Page and arrow ids are `{screen}-page-N`,
/// `{screen}-prev`, `{screen}-next` and `{screen}-page-count`.
pub fn render_list(screen: &str, template: &str, items: &[ListItem], page_size: usize) -> String {
    if items.is_empty() || page_size == 0 {
        return String::new();
    }
    let page_count = items.len().div_ceil(page_size);
    let mut html = String::from("<div class=\"list\">");
    for (index, chunk) in items.chunks(page_size).enumerate() {
        let hidden = if index == 0 {
            ""
        } else {
            " style=\"display:none;\""
        };
        html.push_str(&format!(
            "<div id=\"{screen}-page-{}\" class=\"list-page\"{hidden}>",
            index + 1
        ));
        for item in chunk {
            html.push_str(&render_row(template, item));
        }
        html.push_str("</div>");
    }
    if page_count > 1 {
        html.push_str(&format!(
            "<div id=\"{screen}-pager\" class=\"list-pager\"><button id=\"{screen}-prev\" class=\"menu-action list-pager-prev\">PREV</button><div id=\"{screen}-page-count\" class=\"list-pager-count\">1 / {page_count}</div><button id=\"{screen}-next\" class=\"menu-action list-pager-next\">NEXT</button></div>"
        ));
    }
    html.push_str("</div>");
    html
}

pub fn fill_slot(document: &str, slot: &str, body: &str) -> Result<String, String> {
    let count = document.matches(slot).count();
    if count == 0 {
        if body.is_empty() {
            return Ok(document.to_string());
        }
        return Err(format!("the menu has no {slot} slot for a list to go into"));
    }
    if count != 1 {
        return Err(format!("{slot} appears {count} times; a slot is one place"));
    }
    Ok(document.replace(slot, body))
}

/// Append a screen that is not in the design, and optionally a second button
/// that opens a screen already declared (BACK in a list opens pause).
pub fn declare_screen(
    cfg: &str,
    screen: &crate::themes::Screen,
    also_opens: Option<(&str, &str)>,
) -> Result<String, String> {
    let mut cfg = cfg.to_string();
    let marker = "screens = \"";
    let Some(start) = cfg.find(marker) else {
        return Err("design.cfg has no screens list".into());
    };
    let value_at = start + marker.len();
    let Some(end) = cfg[value_at..].find('"') else {
        return Err("design.cfg has an unclosed screens list".into());
    };
    let existing = cfg[value_at..value_at + end].to_string();
    if !existing.split_whitespace().any(|id| id == screen.id) {
        let next = if existing.is_empty() {
            screen.id.clone()
        } else {
            format!("{existing} {}", screen.id)
        };
        cfg.replace_range(value_at..value_at + end, &next);
    }
    cfg.push_str(&format!(
        "screen_panel_{id} = \"{panel}\"\nscreen_heading_{id} = \"{heading}\"\nscreen_footer_{id} = \"{footer}\"\nscreen_button_{id} = \"{button}\"\n",
        id = screen.id,
        panel = screen.panel,
        heading = screen.heading,
        footer = screen.footer,
        button = screen.button,
    ));
    if let Some((host, button)) = also_opens {
        let key = format!("screen_button_{host} = \"");
        let Some(start) = cfg.find(&key) else {
            return Err(format!("design.cfg has no button for screen {host}"));
        };
        let value_at = start + key.len();
        let Some(end) = cfg[value_at..].find('"') else {
            return Err(format!("design.cfg has an unclosed button for {host}"));
        };
        let existing = cfg[value_at..value_at + end].to_string();
        if !existing.split_whitespace().any(|id| id == button) {
            let next = if existing.is_empty() {
                button.to_string()
            } else {
                format!("{existing} {button}")
            };
            cfg.replace_range(value_at..value_at + end, &next);
        }
    }
    Ok(cfg)
}

fn glsl_source(fragment_body: &str) -> String {
    format!(
        r#"/* Original ROM-in-a-Box preset. RetroArch's OpenGL driver compiles this
 * twice, once with VERTEX defined and once with FRAGMENT defined. */
#if defined(VERTEX)
#if __VERSION__ >= 130
#define COMPAT_VARYING out
#define COMPAT_ATTRIBUTE in
#define COMPAT_TEXTURE texture
#else
#define COMPAT_VARYING varying
#define COMPAT_ATTRIBUTE attribute
#define COMPAT_TEXTURE texture2D
#endif
#ifdef GL_ES
#define COMPAT_PRECISION mediump
#else
#define COMPAT_PRECISION
#endif
COMPAT_ATTRIBUTE vec4 VertexCoord;
COMPAT_ATTRIBUTE vec4 COLOR;
COMPAT_ATTRIBUTE vec4 TexCoord;
COMPAT_VARYING vec4 COL0;
COMPAT_VARYING vec4 TEX0;
uniform mat4 MVPMatrix;
uniform COMPAT_PRECISION int FrameDirection;
uniform COMPAT_PRECISION int FrameCount;
uniform COMPAT_PRECISION vec2 OutputSize;
uniform COMPAT_PRECISION vec2 TextureSize;
uniform COMPAT_PRECISION vec2 InputSize;
void main()
{{
    gl_Position = MVPMatrix * VertexCoord;
    COL0 = COLOR;
    TEX0.xy = TexCoord.xy;
}}
#elif defined(FRAGMENT)
#if __VERSION__ >= 130
#define COMPAT_VARYING in
#define COMPAT_TEXTURE texture
out vec4 FragColor;
#else
#define COMPAT_VARYING varying
#define FragColor gl_FragColor
#define COMPAT_TEXTURE texture2D
#endif
#ifdef GL_ES
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
#define COMPAT_PRECISION mediump
#else
#define COMPAT_PRECISION
#endif
uniform COMPAT_PRECISION int FrameDirection;
uniform COMPAT_PRECISION int FrameCount;
uniform COMPAT_PRECISION vec2 OutputSize;
uniform COMPAT_PRECISION vec2 TextureSize;
uniform COMPAT_PRECISION vec2 InputSize;
uniform sampler2D Texture;
COMPAT_VARYING vec4 TEX0;
void main()
{{
    {fragment_body}
}}
#endif
"#
    )
}

fn preset_text(shader_file: &str) -> String {
    format!("shaders = 1\nshader0 = {shader_file}\nfilter_linear0 = false\n")
}

fn shader_format(path: &Path) -> Result<ShaderFormat, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "glsl" | "glslp" => Ok(ShaderFormat::Glsl),
        "slang" | "slangp" => Err(
            "This game's video driver is OpenGL. Slang presets need Vulkan, Metal or GLCore, and those are switched off. Add a .glsl or .glslp shader."
                .into(),
        ),
        "cg" | "cgp" => Err(
            "Cg presets are not built into exported games. Add a .glsl or .glslp shader.".into(),
        ),
        _ => Err("Add a .glsl or .glslp shader.".into()),
    }
}

fn safe_relative(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "a shader preset can only use files beside it, not {}",
            path.display()
        ));
    }
    Ok(())
}

/// The passes listed in a `.glslp`, each checked to be inside the preset folder.
fn referenced_passes(preset: &Path) -> Result<Vec<PathBuf>, String> {
    let text = fs::read_to_string(preset)
        .map_err(|error| format!("could not read shader preset: {error}"))?;
    let directory = preset.parent().unwrap_or_else(|| Path::new("."));
    let mut passes = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        let is_pass = key.starts_with("shader")
            && key.len() > "shader".len()
            && key["shader".len()..].chars().all(|c| c.is_ascii_digit());
        if !is_pass {
            continue;
        }
        let relative = Path::new(value);
        safe_relative(relative)?;
        let source = directory.join(relative);
        if !source.is_file() {
            return Err(format!(
                "shader preset line {} names a missing file: {value}",
                line_number + 1
            ));
        }
        shader_format(&source)?;
        passes.push(source);
    }
    if passes.is_empty() {
        return Err("a shader preset names no shader pass".into());
    }
    Ok(passes)
}

fn slug(name: &str) -> Result<String, String> {
    let mut slug = String::new();
    let mut pending_hyphen = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    require_id(&slug)?;
    if slug == UNFILTERED_ID {
        return Err("a shader cannot take the unfiltered id".into());
    }
    Ok(slug)
}

fn unique_id(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|id| id == base) {
        return base.to_string();
    }
    let mut suffix = 2_u32;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !taken.iter().any(|id| id == &candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn unfiltered() -> ResolvedShader {
    ResolvedShader {
        id: UNFILTERED_ID.into(),
        name: "Unfiltered".into(),
        detail: "The picture as the console draws it".into(),
        relative_preset: String::new(),
        files: Vec::new(),
        generated: None,
    }
}

/// Check a selection. Return nothing for an empty one, which is the usual case.
pub fn resolve(selection: &ShaderSelection) -> Result<Vec<ResolvedShader>, String> {
    if selection.is_empty() {
        if selection.initial.is_some() {
            return Err("Choose a shader before setting which one starts.".into());
        }
        return Ok(Vec::new());
    }
    let catalog = catalog_file()?;
    let mut resolved = vec![unfiltered()];
    for id in &selection.bundled {
        if id == UNFILTERED_ID {
            continue;
        }
        require_id(id)?;
        let Some(preset) = catalog.presets.iter().find(|preset| preset.id == *id) else {
            return Err(format!("unknown shader '{id}'"));
        };
        if resolved.iter().any(|item| item.id == preset.id) {
            continue;
        }
        let file = format!("{id}.glsl");
        resolved.push(ResolvedShader {
            id: id.clone(),
            name: preset.name.clone(),
            detail: preset.detail.clone(),
            relative_preset: format!("shaders/{id}/{id}.glslp"),
            files: Vec::new(),
            generated: Some(file),
        });
    }
    for custom in &selection.custom {
        let name = custom.name.trim();
        if name.is_empty() {
            return Err("a custom shader needs a name".into());
        }
        if resolved.iter().any(|item| item.name == name) {
            return Err(format!("shader name '{name}' is already used"));
        }
        shader_format(&custom.path)?;
        if !custom.path.is_file() {
            return Err(format!(
                "shader file does not exist: {}",
                custom.path.display()
            ));
        }
        let taken: Vec<String> = resolved.iter().map(|item| item.id.clone()).collect();
        let id = unique_id(&slug(name)?, &taken);
        let (relative, files, generated) = match custom
            .path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "glsl" => {
                let file_name = custom
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| "shader file has no name".to_string())?;
                (
                    format!("shaders/{id}/{id}.glslp"),
                    vec![(custom.path.clone(), file_name.to_string())],
                    Some(file_name.to_string()),
                )
            }
            "glslp" => {
                let passes = referenced_passes(&custom.path)?;
                let mut files = vec![(
                    custom.path.clone(),
                    custom
                        .path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("preset.glslp")
                        .to_string(),
                )];
                for pass in passes {
                    let file_name = pass
                        .file_name()
                        .and_then(|value| value.to_str())
                        .ok_or_else(|| "shader pass has no name".to_string())?
                        .to_string();
                    if files.iter().any(|(_, name)| name == &file_name) {
                        continue;
                    }
                    files.push((pass, file_name));
                }
                let preset_name = files[0].1.clone();
                (format!("shaders/{id}/{preset_name}"), files, None)
            }
            _ => return Err("Add a .glsl or .glslp shader.".into()),
        };
        // We wrap a bare .glsl. `generated` is the source file name, and we write
        // a one-pass preset next to it. We copy a .glslp unchanged.
        resolved.push(ResolvedShader {
            id,
            name: name.to_string(),
            detail: "Added shader".into(),
            relative_preset: relative,
            files,
            generated,
        });
    }
    starting(selection, &resolved)?;
    Ok(resolved)
}

/// The bundled shader to enable when the game starts. No choice and the
/// unfiltered id both mean the unfiltered picture. The author may name a
/// custom shader by its id or by the name they gave it.
fn starting<'a>(
    selection: &ShaderSelection,
    resolved: &'a [ResolvedShader],
) -> Result<&'a ResolvedShader, String> {
    let wanted = selection.initial.as_deref().unwrap_or(UNFILTERED_ID);
    resolved
        .iter()
        .find(|item| item.id == wanted || item.name == wanted)
        .ok_or_else(|| {
            format!("the starting shader '{wanted}' is not one of the bundled shaders")
        })
}

fn icon_png(id: &str) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, Rgba};
    let mut image: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(56, 56, Rgba([6, 26, 72, 255]));
    // A light frame, so the unfiltered row still has a picture on the dark
    // background. The pictures of the named presets cover it.
    for x in 0..56 {
        for y in [0, 1, 2, 53, 54, 55] {
            image.put_pixel(x, y, Rgba([180, 230, 255, 255]));
            image.put_pixel(y, x, Rgba([180, 230, 255, 255]));
        }
    }
    match id {
        "scanlines" => {
            for y in (0..56).step_by(4) {
                for x in 0..56 {
                    image.put_pixel(x, y, Rgba([180, 230, 255, 255]));
                    if y + 1 < 56 {
                        image.put_pixel(x, y + 1, Rgba([180, 230, 255, 255]));
                    }
                }
            }
        }
        "phosphor" => {
            for pixel in image.pixels_mut() {
                *pixel = Rgba([30, 160, 55, 255]);
            }
        }
        _ => {}
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| format!("could not draw a shader icon: {error}"))?;
    Ok(bytes.into_inner())
}

/// The shader screen as declared in the design, read with the same function
/// as every other screen of a design.
fn shader_screen(design: &Path) -> crate::themes::Screen {
    crate::themes::declared_screens(design)
        .ok()
        .and_then(|screens| screens.into_iter().find(|screen| screen.id == "shaders"))
        .unwrap_or_else(|| crate::themes::Screen {
            id: "shaders".into(),
            panel: "shaders-panel".into(),
            heading: "SHADERS".into(),
            footer: "ESC  BACK".into(),
            button: "shaders".into(),
            label: None,
            back_label: None,
            place: crate::themes::ScreenPlace::Plain,
            option_label: None,
            option_default: false,
        })
}


/// Copy presets into the menu assets and fill the list markers. For an empty
/// selection we clear the markers and write nothing else.
pub fn install(
    design: &Path,
    menu_assets: &Path,
    selection: &ShaderSelection,
) -> Result<StagedShaders, String> {
    let document_path = menu_assets.join("menu.rml");
    let config_path = menu_assets.join("design.cfg");
    let document = fs::read_to_string(&document_path)
        .map_err(|error| format!("could not read the staged menu: {error}"))?;
    let design_cfg = fs::read_to_string(&config_path)
        .map_err(|error| format!("could not read the staged screen list: {error}"))?;
    let resolved = resolve(selection)?;
    if resolved.is_empty() {
        let document = fill_slot(&document, LINKS_SLOT, "")?;
        let document = fill_slot(&document, SCREENS_SLOT, "")?;
        fs::write(&document_path, document).map_err(|error| error.to_string())?;
        return Ok(StagedShaders {
            links: String::new(),
            screens: String::new(),
            config: String::new(),
            initial_relative: None,
        });
    }

    let screen = shader_screen(design);
    let template = row_template(design)?;
    let pages = page_size(design)?;
    let initial = starting(selection, &resolved)?.id.clone();
    let mut items = Vec::new();
    let mut config = format!("shader_ids = \"{}\"\n", {
        resolved
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    });
    config.push_str("shader_state_on = \"ON\"\nshader_state_off = \"\"\n");
    config.push_str(&format!("shader_initial = \"{initial}\"\n"));
    let catalog = catalog_file()?;
    for item in &resolved {
        let directory = menu_assets.join("shaders").join(&item.id);
        if item.id != UNFILTERED_ID {
            fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        }
        if let Some(source_name) = &item.generated {
            if item.files.is_empty() {
                let Some(preset) = catalog.presets.iter().find(|preset| preset.id == item.id) else {
                    return Err(format!("shader '{}' has no source", item.id));
                };
                fs::write(directory.join(format!("{}.glsl", item.id)), glsl_source(&preset.fragment))
                    .map_err(|error| error.to_string())?;
                fs::write(
                    directory.join(format!("{}.glslp", item.id)),
                    preset_text(&format!("{}.glsl", item.id)),
                )
                .map_err(|error| error.to_string())?;
            } else {
                for (source, name) in &item.files {
                    fs::copy(source, directory.join(name)).map_err(|error| {
                        format!("could not bundle shader file {}: {error}", source.display())
                    })?;
                }
                fs::write(directory.join(format!("{}.glslp", item.id)), preset_text(source_name))
                    .map_err(|error| error.to_string())?;
            }
        } else {
            for (source, name) in &item.files {
                fs::copy(source, directory.join(name)).map_err(|error| {
                    format!("could not bundle shader file {}: {error}", source.display())
                })?;
            }
        }
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        fs::write(directory.join("icon.png"), icon_png(&item.id)?)
            .map_err(|error| error.to_string())?;
        let selected = item.id == initial;
        items.push(ListItem {
            id: item.id.clone(),
            icon: format!("shaders/{}/icon.png", item.id),
            title: item.name.to_uppercase(),
            detail: item.detail.clone(),
            state: if selected { "ON".into() } else { String::new() },
            selected,
        });
        config.push_str(&format!(
            "shader_preset_{id} = \"{preset}\"\n",
            id = item.id,
            preset = item.relative_preset,
        ));
    }
    fs::write(menu_assets.join("shaders.cfg"), &config).map_err(|error| error.to_string())?;
    let list = render_list(&screen.id, &template, &items, pages);
    let screens = format!(
        "<div id=\"{panel}\" class=\"screen-panel\" style=\"display:none;\">{list}<div class=\"list-actions\"><button class=\"menu-action\" id=\"{id}-back\">BACK</button></div><div id=\"{id}-status\" class=\"list-status\"></div></div>",
        panel = screen.panel,
        id = screen.id,
    );
    let links = format!(
        "<button class=\"menu-action screen-link\" id=\"{button}\">{heading}</button>",
        button = screen.button,
        heading = rml_text(&screen.heading),
    );
    let document = fill_slot(&document, LINKS_SLOT, &links)?;
    let document = fill_slot(&document, SCREENS_SLOT, &screens)?;
    fs::write(&document_path, document).map_err(|error| error.to_string())?;
    let design_cfg = declare_screen(
        &design_cfg,
        &screen,
        Some(("pause", &format!("{}-back", screen.id))),
    )?;
    fs::write(&config_path, &design_cfg).map_err(|error| error.to_string())?;
    let initial_relative = resolved
        .iter()
        .find(|item| item.id == initial)
        .and_then(|item| {
            if item.relative_preset.is_empty() {
                None
            } else {
                Some(item.relative_preset.clone())
            }
        });
    Ok(StagedShaders {
        links,
        screens,
        config,
        initial_relative,
    })
}

/// The preset to pass to RetroArch at launch, relative to the menu assets.
/// For the unfiltered choice and an empty selection, we leave shaders off.
pub fn launch_preset(selection: &ShaderSelection) -> Result<Option<String>, String> {
    let resolved = resolve(selection)?;
    if resolved.is_empty() {
        return Ok(None);
    }
    let item = starting(selection, &resolved)?;
    if item.relative_preset.is_empty() {
        Ok(None)
    } else {
        Ok(Some(item.relative_preset.clone()))
    }
}

/// Shell code that we insert before the player starts. A choice saved in the
/// game's folder replaces the starting preset of the author. With a blank
/// choice we leave `video_shader_enable` unset, as in an ordinary game.
pub fn launcher_shader_shell(initial_relative: Option<&str>) -> String {
    let baked = match initial_relative {
        Some(relative) => format!("$bundle_dir/Resources/menu-assets/{relative}"),
        None => String::new(),
    };
    format!(
        r#"shader_preset=""
if [ -f "$data_dir/shader-choice" ]; then
  shader_preset=$(/usr/bin/head -n 1 "$data_dir/shader-choice" || true)
else
  shader_preset="{baked}"
fi
if [ -n "$shader_preset" ]; then
  printf 'video_shader_enable = "true"\n' >> "$cfg"
fi
"#
    )
}

/// The files we keep in a project archive so that we can still find a custom
/// shader when someone opens the project elsewhere. Catalog presets are not
/// among them, because they come with the builder.
pub fn pack_selection(
    selection: &ShaderSelection,
) -> Result<(ShaderSelection, Vec<(String, PathBuf)>), String> {
    let resolved = resolve(selection)?;
    if resolved.is_empty() {
        return Ok((selection.clone(), Vec::new()));
    }
    let mut stored = selection.clone();
    let mut files = Vec::new();
    for custom in &mut stored.custom {
        let item = resolved
            .iter()
            .find(|item| item.name == custom.name.trim())
            .ok_or_else(|| format!("could not pack shader '{}'", custom.name))?;
        for (source, name) in &item.files {
            let archive_name = format!("shaders/{}/{name}", item.id);
            if files.iter().any(|(existing, _)| existing == &archive_name) {
                continue;
            }
            files.push((archive_name, source.clone()));
        }
        // Keep the file of the author, not the one-pass preset that we wrap a
        // bare .glsl in at export. We resolve it again when someone opens the project.
        let file_name = custom
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("shader.glsl");
        custom.path = PathBuf::from(format!("shaders/{}/{file_name}", item.id));
    }
    Ok((stored, files))
}

/// Point packed custom shaders at the directory we extracted the project into.
pub fn unpack_selection(mut selection: ShaderSelection, root: &Path) -> ShaderSelection {
    for custom in &mut selection.custom {
        if custom.path.is_relative() {
            custom.path = root.join(&custom.path);
        }
    }
    selection
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, title: &str) -> ListItem {
        ListItem {
            id: id.into(),
            icon: format!("{id}.png"),
            title: title.into(),
            detail: format!("{title} detail"),
            state: String::new(),
            selected: false,
        }
    }

    #[test]
    fn a_list_is_the_one_row_template_however_many_items_it_has() {
        let template = BUILT_IN_ROW;
        let rows = render_list(
            "shaders",
            template,
            &[item("scanlines", "SCANLINES"), item("phosphor", "PHOSPHOR")],
            4,
        );
        let other = render_list(
            "achievements",
            template,
            &[item("first", "FIRST"), item("second", "SECOND"), item("third", "THIRD")],
            4,
        );
        assert_eq!(rows.matches("class=\"list-row ").count(), 2);
        assert_eq!(other.matches("class=\"list-row ").count(), 3);
        assert!(rows.contains("id=\"scanlines\""));
        assert!(rows.contains("id=\"scanlines-state\""));
        assert!(other.contains("id=\"first-state\""));
        assert!(!rows.contains("shader-row"));
        assert!(!other.contains("achievement-row"));
        assert_eq!(
            rows.matches("list-row-title").count(),
            other.matches("list-row-title").count() - 1
        );
    }

    #[test]
    fn a_design_that_declares_a_row_is_the_row_that_gets_filled() {
        let root = std::env::temp_dir().join(format!(
            "rominabox-row-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("row.rml"), "<div id=\"ROW-ID\" class=\"list-row bespoke\">TITLE</div>\n")
            .unwrap();
        let template = row_template(&root).unwrap();
        let row = render_row(&template, &item("one", "ONE"));
        assert!(row.contains("bespoke"), "{row}");
        assert!(row.contains(">ONE<"), "{row}");
        assert!(!row.contains("list-row-icon"), "the built-in row was used instead");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn more_rows_than_fit_page_with_a_visible_count() {
        let items: Vec<_> = (0..5).map(|index| item(&format!("item-{index}"), "ITEM")).collect();
        let list = render_list("shaders", BUILT_IN_ROW, &items, 2);
        assert!(list.contains("id=\"shaders-page-1\""));
        assert!(list.contains("id=\"shaders-page-3\""));
        assert!(list.contains("1 / 3"));
        assert!(list.contains("id=\"shaders-prev\""));
        assert!(list.contains("id=\"shaders-next\""));
        let single = render_list("shaders", BUILT_IN_ROW, &items[..2], 4);
        assert!(!single.contains("list-pager"), "one page does not grow arrows");
    }

    #[test]
    fn an_empty_selection_is_no_shaders() {
        let resolved = resolve(&ShaderSelection::default()).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn slang_is_refused_because_those_drivers_stay_off() {
        let root = std::env::temp_dir().join(format!(
            "rominabox-slang-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("crt.slangp");
        fs::write(&path, "shaders = 1\n").unwrap();
        let error = resolve(&ShaderSelection {
            custom: vec![CustomShader {
                name: "CRT".into(),
                path,
            }],
            ..ShaderSelection::default()
        })
        .expect_err("slang must not bundle");
        assert!(
            error.contains("OpenGL") && error.contains("Metal"),
            "{error}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bundling_a_preset_uses_the_row_and_writes_glsl() {
        let root = std::env::temp_dir().join(format!(
            "rominabox-shader-stage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let design = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs/native");
        fs::create_dir_all(&root).unwrap();
        fs::copy(design.join("menu.rml"), root.join("menu.rml")).unwrap();
        fs::write(
            root.join("design.cfg"),
            "screens = \"pause controls\"\nscreen_button_pause = \"controls-back\"\n",
        )
        .unwrap();
        let staged = install(
            &design,
            &root,
            &ShaderSelection {
                bundled: vec!["scanlines".into(), "phosphor".into()],
                initial: Some("phosphor".into()),
                custom: Vec::new(),
            },
        )
        .unwrap();
        let document = fs::read_to_string(root.join("menu.rml")).unwrap();
        assert_eq!(document.matches("class=\"list-row ").count(), 3);
        assert!(document.contains("id=\"phosphor\""));
        assert!(document.contains("id=\"shaders\""));
        assert!(!document.contains(LINKS_SLOT));
        assert!(staged.config.contains("shader_preset_scanlines = \"shaders/scanlines/scanlines.glslp\""));
        assert!(staged.initial_relative.as_deref() == Some("shaders/phosphor/phosphor.glslp"));
        let preset = fs::read_to_string(root.join("shaders/scanlines/scanlines.glslp")).unwrap();
        assert!(preset.contains("shader0 = scanlines.glsl"));
        let source = fs::read_to_string(root.join("shaders/phosphor/phosphor.glsl")).unwrap();
        assert!(source.contains("#if defined(VERTEX)"));
        assert!(source.contains("#elif defined(FRAGMENT)"));
        let declarations = fs::read_to_string(root.join("design.cfg")).unwrap();
        assert!(declarations.contains("screens = \"pause controls shaders\""));
        assert!(declarations.contains("screen_button_pause = \"controls-back shaders-back\""));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_ordinary_menu_gains_no_shader_screen() {
        let root = std::env::temp_dir().join(format!(
            "rominabox-shader-plain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let design = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs/native");
        fs::create_dir_all(&root).unwrap();
        fs::copy(design.join("menu.rml"), root.join("menu.rml")).unwrap();
        fs::write(root.join("design.cfg"), "screens = \"pause controls\"\n").unwrap();
        install(&design, &root, &ShaderSelection::default()).unwrap();
        let document = fs::read_to_string(root.join("menu.rml")).unwrap();
        assert!(!document.contains("id=\"shaders\""));
        assert!(!document.contains("list-row"));
        assert!(!root.join("shaders.cfg").exists());
        assert!(!document.contains("video_shader"));
        let _ = fs::remove_dir_all(&root);
    }

}
