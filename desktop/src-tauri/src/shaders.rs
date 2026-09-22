//! Shaders that an author can bundle.
//!
//! The list code for every screen of rows is in `lists`, and here we only
//! define the content of a shader row. The presets are GLSL. In exported
//! games we set `video_driver` to OpenGL and build without Metal and Vulkan,
//! so we reject slang and Cg presets. We do not enable those drivers in this
//! code.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The choice that means no preset. It is not a file, and an author does not
/// tick it, because when we bundle any preset, the player can always return
/// to the unfiltered picture.
pub const UNFILTERED_ID: &str = "none";

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

#[derive(Clone, Debug)]
pub struct StagedShaders {
    /// The screen and its rows, which we write into the menu with
    /// `lists::install`. None when we bundle no shader.
    pub list: Option<crate::lists::List>,
    /// The text of `shaders.cfg` in the game, or empty when we bundle nothing.
    pub config: String,
    /// The preset to enable at launch, relative to the menu assets, if any.
    pub initial_relative: Option<String>,
}

fn catalog_file() -> Result<CatalogFile, String> {
    let text = include_str!("../../../integrations/shaders/catalog.json");
    serde_json::from_str(text).map_err(|error| format!("shader catalog is not readable: {error}"))
}

/// Every catalog preset with the GLSL of an exported game.
///
/// We make the preview of a filter by running the filter, so a preset we add
/// to the catalog has a preview, and editing a fragment changes both the game
/// and its picture.
pub fn sources() -> Result<Vec<(CatalogEntry, String)>, String> {
    // Unfiltered is not a preset and has no file, but it is a row in the
    // list and must have a picture, of which the other pictures are filtered
    // versions. We draw it in the same way as the others, with no special
    // case, because that is exactly what "no filter" does to a picture.
    let unfiltered = unfiltered();
    let mut listed = vec![(
        CatalogEntry {
            id: unfiltered.id.clone(),
            name: unfiltered.name.clone(),
            detail: unfiltered.detail.clone(),
        },
        glsl_source("FragColor = COMPAT_TEXTURE(Texture, TEX0.xy);"),
    )];
    listed.extend(catalog_file()?
        .presets
        .into_iter()
        .map(|preset| {
            let glsl = glsl_source(&preset.fragment);
            (
                CatalogEntry {
                    id: preset.id,
                    name: preset.name,
                    detail: preset.detail,
                },
                glsl,
            )
        }));
    Ok(listed)
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

/// The picture next to a shader in the list, which is the test card with that
/// shader applied.
///
/// We render the previews from the GLSL of each shader with
/// `scripts/render_shader_previews.py` and check them in the `shaderpreview`
/// scope, so we notice a fragment that changes without its picture before we
/// ship it.
const PREVIEWS: &[(&str, &[u8])] = &[
    (
        UNFILTERED_ID,
        include_bytes!("../../../integrations/shaders/previews/none.png"),
    ),
    (
        "scanlines",
        include_bytes!("../../../integrations/shaders/previews/scanlines.png"),
    ),
    (
        "phosphor",
        include_bytes!("../../../integrations/shaders/previews/phosphor.png"),
    ),
];

fn icon_png(id: &str) -> Result<Vec<u8>, String> {
    if let Some((_, bytes)) = PREVIEWS.iter().find(|(name, _)| *name == id) {
        return Ok(bytes.to_vec());
    }
    // A shader from the author. We do not compile its GLSL here, so we cannot
    // make a true picture of it. We show an empty card, which means "this is
    // your shader" and does not claim to show its effect.
    use image::{ImageBuffer, Rgba};
    let (width, height) = (256u32, 192u32);
    let mut image: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(width, height, Rgba([14, 14, 18, 255]));
    for x in 0..width {
        for y in [0, 1, height - 2, height - 1] {
            image.put_pixel(x, y, Rgba([120, 128, 150, 255]));
        }
    }
    for y in 0..height {
        for x in [0, 1, width - 2, width - 1] {
            image.put_pixel(x, y, Rgba([120, 128, 150, 255]));
        }
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
            // Inside Options, not a separate button on the pause row. One
            // Options screen contains controls, shaders and sound, and it is
            // the only way to reach this screen with a pad. We move through the
            // pause row as a fixed range of five actions, so a sixth button
            // generated there could be clicked but never focused. We move
            // through the Options entries as whatever the panel contains.
            option_label: Some("SHADERS".into()),
            option_default: false,
            images: None,
            mark: None,
            toggle: None,
        })
}


/// Copy presets into the menu assets and describe the screen for them.
///
/// We return the rows and do not write them, because all lists use one marker
/// in the menu, so we fill it with all of them in one place.
pub fn stage(
    design: &Path,
    menu_assets: &Path,
    selection: &ShaderSelection,
) -> Result<StagedShaders, String> {
    let resolved = resolve(selection)?;
    if resolved.is_empty() {
        return Ok(StagedShaders {
            list: None,
            config: String::new(),
            initial_relative: None,
        });
    }

    let screen = shader_screen(design);
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
        items.push(crate::lists::ListItem {
            id: item.id.clone(),
            icon: format!("shaders/{}/icon.png", item.id),
            title: item.name.to_uppercase(),
            detail: item.detail.clone(),
            state: if selected { "ON".into() } else { String::new() },
            selected,
            accent: false,
            line: false,
        });
        config.push_str(&format!(
            "shader_preset_{id} = \"{preset}\"\n",
            id = item.id,
            preset = item.relative_preset,
        ));
    }
    fs::write(menu_assets.join("shaders.cfg"), &config).map_err(|error| error.to_string())?;
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
        list: Some(crate::lists::List { screen, items }),
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

    fn staged_menu(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "rominabox-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let design =
            crate::repo::at("integrations/designs/native");
        fs::create_dir_all(&root).unwrap();
        fs::copy(design.join("menu.rml"), root.join("menu.rml")).unwrap();
        // As in an export. Options must be here, because the shader screen is
        // an entry inside it, and BACK in an entry returns to the screen that
        // contains the entry.
        fs::write(
            root.join("design.cfg"),
            "screens = \"pause options controls\"\n\
             screen_panel_pause = \"pause-panel\"\nscreen_button_pause = \"options-back\"\n\
             screen_panel_options = \"options-panel\"\nscreen_button_options = \"options\"\n",
        )
        .unwrap();
        (design, root)
    }

    fn pause_and_controls() -> Vec<crate::themes::Screen> {
        crate::themes::declared_screens(
            &crate::repo::at("integrations/designs/native"),
        )
        .unwrap()
        .into_iter()
        .filter(|screen| screen.option_label.is_none())
        .collect()
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
        assert!(error.contains("OpenGL") && error.contains("Metal"), "{error}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bundling_a_preset_uses_the_row_and_writes_glsl() {
        let (design, root) = staged_menu("shader-stage");
        let staged = stage(
            &design,
            &root,
            &ShaderSelection {
                bundled: vec!["scanlines".into(), "phosphor".into()],
                initial: Some("phosphor".into()),
                custom: Vec::new(),
            },
        )
        .unwrap();
        let lists: Vec<crate::lists::List> = staged.list.clone().into_iter().collect();
        crate::lists::install(&design, &root, &pause_and_controls(), &lists).unwrap();
        let document = fs::read_to_string(root.join("menu.rml")).unwrap();
        assert_eq!(document.matches("class=\"list-row ").count(), 3);
        assert!(document.contains("id=\"phosphor\""));
        // No button on the pause row. The shader screen is an entry inside
        // Options, which contains controls, shaders and sound. That is also
        // the only way to reach it with a pad, because we move through the
        // pause row as a fixed range of five actions, so a sixth button there
        // could be clicked but never focused. We generate the button of the
        // entry with Options.
        assert!(
            !document.contains("class=\"menu-action screen-link\" id=\"shaders\""),
            "the shader screen must not add a button to the pause row"
        );
        assert_eq!(
            staged.list.as_ref().unwrap().screen.option_label.as_deref(),
            Some("SHADERS")
        );
        assert!(!document.contains(crate::lists::LINKS_SLOT));
        assert!(staged
            .config
            .contains("shader_preset_scanlines = \"shaders/scanlines/scanlines.glslp\""));
        assert!(staged.initial_relative.as_deref() == Some("shaders/phosphor/phosphor.glslp"));
        let preset = fs::read_to_string(root.join("shaders/scanlines/scanlines.glslp")).unwrap();
        assert!(preset.contains("shader0 = scanlines.glsl"));
        let source = fs::read_to_string(root.join("shaders/phosphor/phosphor.glsl")).unwrap();
        assert!(source.contains("#if defined(VERTEX)"));
        assert!(source.contains("#elif defined(FRAGMENT)"));
        let declarations = fs::read_to_string(root.join("design.cfg")).unwrap();
        assert!(declarations.contains("screens = \"pause options controls shaders\""));
        // BACK on the shader screen returns to the screen that contains it,
        // which is Options, not the pause row.
        assert!(
            declarations.contains("screen_button_options = \"options shaders-back\""),
            "{declarations}"
        );
        assert!(declarations.contains("screen_button_pause = \"options-back\""));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_ordinary_menu_gains_no_shader_screen() {
        let (design, root) = staged_menu("shader-plain");
        let staged = stage(&design, &root, &ShaderSelection::default()).unwrap();
        assert!(staged.list.is_none());
        crate::lists::install(&design, &root, &pause_and_controls(), &[]).unwrap();
        let document = fs::read_to_string(root.join("menu.rml")).unwrap();
        assert!(!document.contains("id=\"shaders\""));
        assert!(!document.contains("list-row"));
        assert!(!root.join("shaders.cfg").exists());
        assert!(!document.contains("video_shader"));
        let _ = fs::remove_dir_all(&root);
    }
}
