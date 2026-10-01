//! Shaders an author can bundle.
//!
//! The code for every list of rows is in `lists`, and here we only say what a
//! shader row contains. The shaders of a game are all GLSL or all slang, and
//! we tell which by reading the author's files (`shader_format`). We choose
//! the game's video driver from that language, and write the catalog presets
//! in it (`shader_source`). We refuse Cg.
//!
//! The catalog has two kinds of preset. Each ROM-in-a-Box preset is one
//! fragment body, which we write out in the game's language. We keep the
//! libretro presets in the shader library of the runtime kit as their packs
//! lay them out, one folder per language, and copy the files of a preset into
//! a game at the same paths in the same folder. So every path in the preset
//! still leads to its file, and we copy a file that presets share once. A
//! game with no author's shader uses GLSL, unless a preset is only in slang.

use crate::shader_format::{Language, VideoDriver};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The choice that means no preset. It is not a file, and an author does not
/// tick it, because when we bundle any preset, the player can always return
/// to the unfiltered picture.
pub const UNFILTERED_ID: &str = "none";

#[derive(Clone, Debug, Deserialize)]
struct CatalogFile {
    /// Where the shader library's files come from, per language.
    libraries: Libraries,
    presets: Vec<CatalogPreset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Libraries {
    glsl: Source,
    slang: Source,
}

impl Libraries {
    fn of(&self, language: Language) -> &Source {
        match language {
            Language::Glsl => &self.glsl,
            Language::Slang => &self.slang,
        }
    }
}

/// A libretro shader pack, at the commit we took the library from.
#[derive(Clone, Debug, Deserialize)]
struct Source {
    repository: String,
    commit: String,
}

/// The note beside a libretro preset in a game, naming its authors and source.
const CREDITS: &str = "CREDITS.txt";

#[derive(Clone, Debug, Deserialize)]
struct CatalogPreset {
    id: String,
    name: String,
    detail: String,
    #[serde(flatten)]
    made: Made,
    /// Who wrote a libretro preset, as its files credit them.
    #[serde(default)]
    authors: Option<String>,
}

/// How a catalog preset is made.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Made {
    /// Ours, one fragment body that we write in the game's language.
    Fragment(String),
    /// A libretro preset in the shader library.
    Files(LibraryPreset),
}

/// A libretro preset's file in each of its languages, by its path in that
/// language's pack, which is the same as in that language's library folder.
#[derive(Clone, Debug, Deserialize)]
struct LibraryPreset {
    glsl: Option<String>,
    slang: Option<String>,
}

impl LibraryPreset {
    fn in_language(&self, language: Language) -> Option<&str> {
        match language {
            Language::Glsl => self.glsl.as_deref(),
            Language::Slang => self.slang.as_deref(),
        }
    }
}

/// The shader library folders, one per language, in the kit and in a game's
/// `shaders`. No other shader folder may use their names.
const LIBRARY_FOLDERS: [&str; 2] = ["glsl", "slang"];

/// Where the shader library is in a runtime kit, from which we take the
/// files of a catalog preset.
pub fn kit_library(kit: &Path) -> PathBuf {
    kit.join("shaders")
}

/// The shader library's folder for a language's presets.
fn library_folder(language: Language) -> &'static str {
    match language {
        Language::Glsl => "glsl",
        Language::Slang => "slang",
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
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

/// A shader file the author added.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CustomShader {
    /// The name we show in the game's list. When it is missing or blank, we
    /// use the name of the file ([`named_after_file`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub path: PathBuf,
}

impl CustomShader {
    /// The name we show in the game's list.
    pub fn name(&self) -> String {
        match self.name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => named_after_file(&self.path),
        }
    }
}

/// The name of a shader file the author added without naming it, the same in
/// the builder and on the command line. It is the file name without a shader
/// extension (`crt.glsl` is "crt"), or "Shader" when that leaves nothing.
pub fn named_after_file(path: &Path) -> String {
    let shader = path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        Language::ALL.iter().any(|language| {
            extension.eq_ignore_ascii_case(language.pass_extension())
                || extension.eq_ignore_ascii_case(language.preset_extension())
        })
    });
    let name = if shader { path.file_stem() } else { path.file_name() };
    name.map(|name| name.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Shader".to_string())
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
    /// A libretro preset's path in the shader library, starting with the
    /// language folder. We find its files there when we stage the game.
    library: Option<String>,
    /// Files to copy into that directory, as (source, path within it). The
    /// file the author added comes first.
    files: Vec<(PathBuf, String)>,
    /// Files we write into it, as (path within it, text), which are the pass
    /// of a catalog preset and the one-pass preset we make for a single pass.
    written: Vec<(String, String)>,
}

impl ResolvedShader {
    /// Where we stage its files, among the game's menu assets.
    fn folder(&self) -> PathBuf {
        Path::new("shaders").join(&self.id)
    }
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
    let catalog: CatalogFile = serde_json::from_str(text)
        .map_err(|error| format!("shader catalog is not readable: {error}"))?;
    for preset in &catalog.presets {
        if LIBRARY_FOLDERS.contains(&preset.id.as_str()) {
            return Err(format!("shader catalog preset '{}' takes a library folder's name", preset.id));
        }
        if let Made::Files(files) = &preset.made {
            if files.glsl.is_none() && files.slang.is_none() {
                return Err(format!("shader catalog preset '{}' names no file", preset.id));
            }
        }
    }
    Ok(catalog)
}

/// Every libretro preset in the catalog, as its path in the shader library.
pub fn library_presets() -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for preset in catalog_file()?.presets {
        if let Made::Files(files) = preset.made {
            for language in [Language::Glsl, Language::Slang] {
                if let Some(path) = files.in_language(language) {
                    paths.push(format!("{}/{path}", library_folder(language)));
                }
            }
        }
    }
    Ok(paths)
}

/// A libretro preset and every file it lists, below `root`, which is the
/// folder of the language's pack or of the library that contains it. Each is
/// (the file, its path from `root` with `/` between parts). `preset` is the
/// path from `root` to the preset. We find the files as for an author's preset.
pub fn library_files(root: &Path, preset: &str) -> Result<Vec<(PathBuf, String)>, String> {
    let path = root.join(preset);
    if !path.is_file() {
        return Err(format!("the shader library has no {preset} in {}", root.display()));
    }
    let (_, layout) = crate::shader_preset::preset(&path)?;
    // The paths in the layout start at the lowest folder that contains every
    // file, which is the preset's folder or one above it.
    let mut base: Vec<&str> = preset.split('/').collect();
    base.pop();
    for part in layout.folder.split('/').filter(|part| !part.is_empty()).rev() {
        if base.pop() != Some(part) {
            return Err(format!("{preset} names files outside its pack"));
        }
    }
    let from_root = |name: &str| {
        base.iter().copied().chain(name.split('/')).collect::<Vec<_>>().join("/")
    };
    let mut files = vec![(path, preset.to_string())];
    files.extend(layout.files.into_iter().map(|(source, name)| (source, from_root(&name))));
    Ok(files)
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
            authors: None,
        },
        crate::shader_source::pass(Language::Glsl, "FragColor = COMPAT_TEXTURE(Texture, TEX0.xy);"),
    )];
    listed.extend(catalog_file()?.presets.into_iter().filter_map(|preset| {
        let Made::Fragment(fragment) = &preset.made else {
            return None;
        };
        let glsl = crate::shader_source::pass(Language::Glsl, fragment);
        Some((
            CatalogEntry {
                id: preset.id,
                name: preset.name,
                detail: preset.detail,
                authors: preset.authors,
            },
            glsl,
        ))
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
            authors: preset.authors,
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
        library: None,
        files: Vec::new(),
        written: Vec::new(),
    }
}

/// Check a selection. Return nothing for an empty one, which is the usual case.
pub fn resolve(selection: &ShaderSelection) -> Result<Vec<ResolvedShader>, String> {
    Ok(resolved(selection)?.1)
}

/// A filter from the author that a Windows game may fail to load, because one
/// of its files would be at a path longer than Windows can open. The player
/// then sees the game without a filter.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShaderWarning {
    /// The filter, as the selection names it.
    pub path: PathBuf,
    pub sentence: String,
}

/// The author's filters in `selection` with files too deep for Windows, at
/// the paths they have in an unpacked game under the longest per-user data
/// folder. We only warn, and make the game either way.
pub fn windows_warnings(selection: &ShaderSelection) -> Result<Vec<ShaderWarning>, String> {
    let mut warnings = Vec::new();
    for item in resolve(selection)? {
        // A filter from the author is one that has files. The file in
        // the selection comes first.
        let Some((own, _)) = item.files.first() else {
            continue;
        };
        let deepest = item
            .files
            .iter()
            .map(|(_, name)| {
                let read = crate::packaging::longest_menu_asset_path(&item.folder().join(name));
                (name, read.encode_utf16().count())
            })
            .max_by_key(|(_, length)| *length);
        if let Some((name, length)) = deepest {
            if length > crate::packaging::LONGEST_PATH {
                warnings.push(ShaderWarning {
                    path: own.clone(),
                    sentence: format!(
                        "On Windows this filter may not load, and the game would run without it: \
                         \u{201c}{name}\u{201d} sits too deep among its folders."
                    ),
                });
            }
        }
    }
    Ok(warnings)
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
        let name = custom.name();
        if !custom.path.is_file() {
            return Err(format!(
                "shader file does not exist: {}",
                custom.path.display()
            ));
        }
        authors.push((name, &custom.path, authored(&custom.path)?));
    }
    let catalog = catalog_file()?;
    let mut chosen: Vec<&CatalogPreset> = Vec::new();
    for id in &selection.bundled {
        if id == UNFILTERED_ID {
            continue;
        }
        require_id(id)?;
        let Some(preset) = catalog.presets.iter().find(|preset| preset.id == *id) else {
            return Err(format!("unknown shader '{id}'"));
        };
        if !chosen.iter().any(|seen| seen.id == preset.id) {
            chosen.push(preset);
        }
    }
    // We take the language of the author's shader, or slang when a preset
    // exists only in slang, or else GLSL.
    let named = authors.iter().map(|(name, _, author)| (name.as_str(), author.language));
    let decided = match crate::shader_format::one_language(named)? {
        Some(language) => Some((authors[0].0.as_str(), language)),
        None => chosen.iter().find_map(|preset| match &preset.made {
            Made::Files(files) if files.glsl.is_none() => {
                Some((preset.name.as_str(), Language::Slang))
            }
            _ => None,
        }),
    };
    let language = decided.map_or(Language::Glsl, |(_, language)| language);
    let pass_extension = language.pass_extension();
    let preset_extension = language.preset_extension();
    let mut resolved = vec![unfiltered()];
    for preset in chosen {
        let id = &preset.id;
        let (relative_preset, library, written) = match &preset.made {
            Made::Fragment(fragment) => {
                let pass = format!("{id}.{pass_extension}");
                let preset_file = format!("{id}.{preset_extension}");
                (
                    format!("shaders/{id}/{preset_file}"),
                    None,
                    vec![
                        (pass.clone(), crate::shader_source::pass(language, fragment)),
                        (preset_file, crate::shader_source::preset(&pass)),
                    ],
                )
            }
            Made::Files(files) => {
                let Some(path) = files.in_language(language) else {
                    let (decider, _) = decided.expect("GLSL is only missing from a slang game");
                    return Err(format!(
                        "{} has no {} version, and {decider} is {}. A game's shaders must all be in one language.",
                        preset.name,
                        language.name(),
                        language.name(),
                    ));
                };
                let source = catalog.libraries.of(language);
                let credits = format!(
                    "{name}, by {authors}.\nFrom {repository} at commit {commit}: {path}\nEach file keeps its own notice.\n",
                    name = preset.name,
                    authors = preset.authors.as_deref().unwrap_or("its authors"),
                    repository = source.repository,
                    commit = source.commit,
                );
                (
                    format!("shaders/{}/{path}", library_folder(language)),
                    Some(format!("{}/{path}", library_folder(language))),
                    vec![(CREDITS.to_string(), credits)],
                )
            }
        };
        resolved.push(ResolvedShader {
            id: id.clone(),
            name: preset.name.clone(),
            detail: preset.detail.clone(),
            relative_preset,
            library,
            files: Vec::new(),
            written,
        });
    }
    for (name, path, author) in authors {
        if resolved.iter().any(|item| item.name == name) {
            return Err(format!("shader name '{name}' is already used"));
        }
        let taken: Vec<String> = resolved
            .iter()
            .map(|item| item.id.clone())
            .chain(LIBRARY_FOLDERS.map(String::from))
            .collect();
        let id = unique_id(&slug(&name)?, &taken);
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
            library: None,
            id,
            name,
            // We show a shader that the author added by its name alone.
            detail: String::new(),
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

/// The picture beside a shader in the list, which is the test card with that
/// shader run over it.
///
/// We render the previews with `scripts/render_shader_previews.py`, from the
/// GLSL of each fragment preset, and from libretro presets with the player of
/// the runtime kit. In the `shaderpreview` tests we check that no fragment
/// changes without its picture. With `build.rs` we embed every picture in
/// the previews folder, each named after its shader.
const PREVIEWS: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/shader_previews.rs"));

fn icon_png(id: &str) -> Result<Vec<u8>, String> {
    if let Some((_, bytes)) = PREVIEWS.iter().find(|(name, _)| *name == id) {
        return Ok(bytes.to_vec());
    }
    // A shader the author added. We cannot know what it does before it runs
    // in the game, so we show a pixel S, for shader, in its row, instead of
    // a picture that claims to show its effect.
    use image::{ImageBuffer, Rgba};
    const GLYPH: [&str; 7] = [".###.", "#...#", "#....", ".###.", "....#", "#...#", ".###."];
    const SIZE: u32 = 256;
    const CELL: u32 = 28;
    const SHADOW: u32 = 8;
    let left = (SIZE - CELL * 5) / 2;
    let top = (SIZE - CELL * 7) / 2;
    let mut image: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(SIZE, SIZE, Rgba([16, 18, 24, 255]));
    // Draw a crisp shadow first, then the letter over it.
    for (offset, colour) in [(SHADOW, Rgba([0, 0, 0, 255])), (0, Rgba([232, 232, 232, 255]))] {
        for (row, line) in GLYPH.iter().enumerate() {
            for (column, cell) in line.chars().enumerate() {
                if cell != '#' {
                    continue;
                }
                let (x, y) = (left + column as u32 * CELL + offset, top + row as u32 * CELL + offset);
                for dy in 0..CELL {
                    for dx in 0..CELL {
                        image.put_pixel(x + dx, y + dy, colour);
                    }
                }
            }
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| format!("could not draw a shader icon: {error}"))?;
    Ok(bytes.into_inner())
}

/// The presets we bundle in a game, the files they need beside the menu, and
/// the screen we put them on.
///
/// We return the rows and do not write them, because every list goes in at one
/// marker in the menu, which we replace once with all the lists together.
///
/// `library` is the shader library of the runtime kit, from which we take the
/// files of a libretro preset.
pub fn stage(
    manifest: &crate::menu::Manifest,
    selection: &ShaderSelection,
    library: &Path,
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
        let directory = item.folder();
        for (source, name) in &item.files {
            files.push((directory.join(name), Content::Copy(source.clone())));
        }
        if let Some(preset) = &item.library {
            let (folder, path) = preset
                .split_once('/')
                .expect("a library path starts with its folder");
            for (source, name) in library_files(&library.join(folder), path)? {
                files.push((Path::new("shaders").join(folder).join(name), Content::Copy(source)));
            }
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
        let name = custom.name();
        let item = resolved
            .iter()
            .find(|item| item.name == name)
            .ok_or_else(|| format!("could not pack shader '{name}'"))?;
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
            .ok_or_else(|| format!("could not pack shader '{name}'"))?;
        // We name the packed file after its id in the game, so we keep the
        // name as given and do not read it from the file again.
        custom.name = Some(name);
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
