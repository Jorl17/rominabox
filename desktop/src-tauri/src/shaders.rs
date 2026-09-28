//! Shaders that an author can bundle.
//!
//! The list code for every screen of rows is in `lists`, and here we only
//! define the content of a shader row. The shaders of a game are all GLSL or
//! all slang, as we read from the files of the author (`shader_format`). We
//! choose the video driver of the game by that language and write the catalog
//! presets in it (`shader_source`). Cg is not supported.

use crate::shader_format::{Language, VideoDriver};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The choice that means no preset. It is not a file, and an author does not
/// tick it, because when we bundle any preset, the player can always return
/// to the unfiltered picture.
pub const UNFILTERED_ID: &str = "none";

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
/// files.
#[derive(Clone, Debug)]
pub struct ResolvedShader {
    pub id: String,
    pub name: String,
    pub detail: String,
    /// The path from the staged menu assets, empty for the unfiltered choice.
    pub relative_preset: String,
    /// Files to copy into that directory, as (source, path within it). The
    /// file the author added comes first.
    files: Vec<(PathBuf, String)>,
    /// Files we write into it, as (path within it, text), which are the pass
    /// of a catalog preset and the one-pass preset we make for a single pass.
    written: Vec<(String, String)>,
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
    /// The presets, sources and row pictures, relative to the menu assets.
    pub files: Vec<(PathBuf, crate::menu::Content)>,
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
        crate::shader_source::pass(Language::Glsl, "FragColor = COMPAT_TEXTURE(Texture, TEX0.xy);"),
    )];
    listed.extend(catalog_file()?.presets.into_iter().map(|preset| {
        let glsl = crate::shader_source::pass(Language::Glsl, &preset.fragment);
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
    Err(format!("shader id '{id}' must be a short lowercase name"))
}

/// The row's picture, in each shader's folder beside its preset.
const ROW_PICTURE: &str = "icon.png";

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
        written: Vec::new(),
    }
}

/// Check a selection. Return nothing for an empty one, which is the usual case.
pub fn resolve(selection: &ShaderSelection) -> Result<Vec<ResolvedShader>, String> {
    Ok(resolved(selection)?.1)
}

/// The video driver for a game with this selection. We choose it from the
/// language of the shaders, and use the GLSL driver for a game with none.
pub fn video_driver(selection: &ShaderSelection) -> Result<VideoDriver, String> {
    Ok(resolved(selection)?.0.video_driver())
}

/// A shader the author added: whether it is a preset or a single pass,
/// for which we make a one-pass preset at export, its language, and
/// where we stage it and the files it lists.
struct Authored {
    kind: crate::shader_format::Kind,
    language: Language,
    layout: crate::shader_preset::Layout,
}

fn authored(path: &Path) -> Result<Authored, String> {
    use crate::shader_format::{kind, require_runnable_pass, text, Kind};
    let kind = kind(&text(path)?);
    let (language, layout) = match kind {
        Kind::Pass => {
            let language = require_runnable_pass(path)?;
            (language, crate::shader_preset::pass(path, language)?)
        }
        Kind::Preset => crate::shader_preset::preset(path)?,
    };
    if layout.files.iter().any(|(_, name)| name == ROW_PICTURE) {
        return Err(format!(
            "a shader cannot use a file named {ROW_PICTURE}; the menu keeps the row's picture there"
        ));
    }
    Ok(Authored {
        kind,
        language,
        layout,
    })
}

/// The language of every shader in a selection, and each shader resolved.
fn resolved(selection: &ShaderSelection) -> Result<(Language, Vec<ResolvedShader>), String> {
    if selection.is_empty() {
        if selection.initial.is_some() {
            return Err("Choose a shader before setting which one starts.".into());
        }
        return Ok((Language::Glsl, Vec::new()));
    }
    // Read the author's shaders first. Catalog presets follow their language.
    let mut authors = Vec::new();
    for custom in &selection.custom {
        let name = custom.name.trim();
        if name.is_empty() {
            return Err("a custom shader needs a name".into());
        }
        if !custom.path.is_file() {
            return Err(format!(
                "shader file does not exist: {}",
                custom.path.display()
            ));
        }
        authors.push((name, &custom.path, authored(&custom.path)?));
    }
    let named = authors.iter().map(|(name, _, author)| (*name, author.language));
    let language = crate::shader_format::one_language(named)?.unwrap_or(Language::Glsl);
    let pass_extension = language.pass_extension();
    let preset_extension = language.preset_extension();
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
        let pass = format!("{id}.{pass_extension}");
        let preset_file = format!("{id}.{preset_extension}");
        resolved.push(ResolvedShader {
            id: id.clone(),
            name: preset.name.clone(),
            detail: preset.detail.clone(),
            relative_preset: format!("shaders/{id}/{preset_file}"),
            files: Vec::new(),
            written: vec![
                (pass.clone(), crate::shader_source::pass(language, &preset.fragment)),
                (preset_file, crate::shader_source::preset(&pass)),
            ],
        });
    }
    for (name, path, author) in authors {
        if resolved.iter().any(|item| item.name == name) {
            return Err(format!("shader name '{name}' is already used"));
        }
        let taken: Vec<String> = resolved.iter().map(|item| item.id.clone()).collect();
        let id = unique_id(&slug(name)?, &taken);
        // We keep the author's file at its place among the files it lists.
        let layout = author.layout;
        let at = |name: String| match layout.folder.as_str() {
            "" => name,
            folder => format!("{folder}/{name}"),
        };
        let (preset_file, files, written) = match author.kind {
            // We copy a pass under its name in the game and write a
            // one-pass preset beside it.
            crate::shader_format::Kind::Pass => {
                let pass = format!("{id}.{pass_extension}");
                let preset_file = at(format!("{id}.{preset_extension}"));
                let written = vec![(preset_file.clone(), crate::shader_source::preset(&pass))];
                let mut files = vec![(path.clone(), at(pass))];
                files.extend(layout.files);
                (preset_file, files, written)
            }
            // We copy a preset unchanged, with the extension of its
            // language, because RetroArch reads it by that extension.
            crate::shader_format::Kind::Preset => {
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or_else(|| "shader file has no name".to_string())?;
                let preset_file = at(format!("{stem}.{preset_extension}"));
                let mut files = vec![(path.clone(), preset_file.clone())];
                for (source, relative) in layout.files {
                    if !files.iter().any(|(_, name)| name == &relative) {
                        files.push((source, relative));
                    }
                }
                (preset_file, files, Vec::new())
            }
        };
        resolved.push(ResolvedShader {
            relative_preset: format!("shaders/{id}/{preset_file}"),
            id,
            name: name.to_string(),
            detail: "Added shader".into(),
            files,
            written,
        });
    }
    starting(selection, &resolved)?;
    Ok((language, resolved))
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
        .ok_or_else(|| format!("the starting shader '{wanted}' is not one of the bundled shaders"))
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

/// The presets bundled in a game, the files they require next to the menu,
/// and the screen for them.
///
/// We return the rows and do not write them, because all lists use one marker
/// in the menu, so we fill it with all of them in one place.
pub fn stage(
    manifest: &crate::menu::Manifest,
    selection: &ShaderSelection,
) -> Result<StagedShaders, String> {
    use crate::menu::Content;
    let resolved = resolve(selection)?;
    if resolved.is_empty() {
        return Ok(StagedShaders {
            list: None,
            config: String::new(),
            initial_relative: None,
            files: Vec::new(),
        });
    }

    let screen = manifest
        .screen(crate::menu::ScreenRole::Shaders)
        .cloned()
        .ok_or_else(|| "the design declares no shaders screen".to_string())?;
    let initial = starting(selection, &resolved)?.id.clone();
    let mut items = Vec::new();
    let mut files: Vec<(PathBuf, Content)> = Vec::new();
    let mut config = format!("{} = \"{}\"\n", crate::menu::key!(ShaderIds), {
        resolved
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    });
    for item in &resolved {
        let directory = Path::new("shaders").join(&item.id);
        for (source, name) in &item.files {
            files.push((directory.join(name), Content::Copy(source.clone())));
        }
        for (name, text) in &item.written {
            files.push((directory.join(name), Content::Text(text.clone())));
        }
        files.push((directory.join(ROW_PICTURE), Content::Bytes(icon_png(&item.id)?)));
        let selected = item.id == initial;
        items.push(crate::lists::ListItem {
            id: item.id.clone(),
            icon: format!("shaders/{}/{ROW_PICTURE}", item.id),
            title: item.name.to_uppercase(),
            detail: item.detail.clone(),
            // The mark that we move to whichever filter is running.
            state: if selected {
                crate::menu::words::say(&manifest.words, "shader-mark", &[])
            } else {
                String::new()
            },
            selected,
            accent: false,
            line: false,
        });
        config.push_str(&format!(
            "{key} = \"{preset}\"\n",
            key = crate::menu::key!(ShaderPreset, item.id),
            preset = item.relative_preset,
        ));
    }
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
        list: Some(crate::lists::List {
            screen,
            content: crate::lists::ListContent::Static(items),
        }),
        config,
        initial_relative,
        files,
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
        // Keep the author's file, the first one, not the one-pass preset we
        // wrap a pass in at export. We resolve it when someone opens the project.
        let (_, author) = item
            .files
            .first()
            .ok_or_else(|| format!("could not pack shader '{}'", custom.name))?;
        custom.path = PathBuf::from(format!("shaders/{}/{author}", item.id));
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
mod tests;
