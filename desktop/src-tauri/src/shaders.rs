//! Shaders that an author can bundle.
//!
//! The list code for every screen of rows is in `lists`, and here we only
//! define the content of a shader row. The shaders of a game are all GLSL or
//! all slang, as we read from the files of the author (`shader_format`). We
//! choose the video driver of the game by that language and write the catalog
//! presets in it (`shader_source`). Cg is not supported.

use crate::shader_format::{Language, VideoDriver};
use serde::{Deserialize, Serialize};
use std::fs;
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

/// The kind of a shader from the author, which is its language and, for a
/// preset, the listed files. At export we wrap a single pass in a preset.
struct Authored {
    language: Language,
    named: Option<Vec<(PathBuf, String)>>,
}

fn authored(path: &Path) -> Result<Authored, String> {
    use crate::shader_format::{kind, require_runnable_pass, text, Kind};
    Ok(match kind(&text(path)?) {
        Kind::Pass => Authored {
            language: require_runnable_pass(path)?,
            named: None,
        },
        Kind::Preset => {
            let (language, named) = crate::shader_preset::files(path)?;
            if named.iter().any(|(_, name)| name == ROW_PICTURE) {
                return Err(format!(
                    "a shader preset cannot use a file named {ROW_PICTURE}; the menu keeps the row's picture there"
                ));
            }
            Authored {
                language,
                named: Some(named),
            }
        }
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
        let (preset_file, files, written) = match author.named {
            // We copy a pass under its name in the game and write a
            // one-pass preset beside it.
            None => {
                let pass = format!("{id}.{pass_extension}");
                let preset_file = format!("{id}.{preset_extension}");
                let written = vec![(preset_file.clone(), crate::shader_source::preset(&pass))];
                (preset_file, vec![(path.clone(), pass)], written)
            }
            // We copy a preset unchanged, with the extension of its
            // language, because RetroArch reads it by that extension.
            Some(named) => {
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or_else(|| "shader file has no name".to_string())?;
                let preset_file = format!("{stem}.{preset_extension}");
                let mut files = vec![(path.clone(), preset_file.clone())];
                for (source, relative) in named {
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
mod tests {
    use super::*;

    /// The menu that we compose in an export with this shader selection.
    fn composed(selection: ShaderSelection) -> crate::menu::Composition {
        let entries = if selection.bundled.is_empty() && selection.custom.is_empty() {
            vec!["controls".to_string()]
        } else {
            vec!["controls".to_string(), "shaders".to_string()]
        };
        crate::menu::compose_menu(&crate::menu::MenuRequest {
            shaders: selection,
            menu_entries: Some(entries),
            ..crate::menu::MenuRequest::new(
                crate::repo::at("integrations/designs/native"),
                crate::repo::at("desktop/assets/controllers"),
            )
        })
        .unwrap()
    }

    #[test]
    fn an_empty_selection_is_no_shaders() {
        let resolved = resolve(&ShaderSelection::default()).unwrap();
        assert!(resolved.is_empty());
    }

    /// A pass for the RetroArch OpenGL driver, as far as this check can tell.
    const PASS: &str = "#if defined(VERTEX)\n#elif defined(FRAGMENT)\n#endif\n";
    /// A slang pass, as far as this check can tell.
    const SLANG: &str = "#version 450\n#pragma stage vertex\n#pragma stage fragment\n";

    /// We recognise a slang preset by the content of its pass, whatever its
    /// name, stage it under the file name RetroArch uses for slang, and run the
    /// game on glcore. A game without one stays on gl.
    #[test]
    fn a_slang_preset_is_staged_as_slang_and_runs_on_glcore() {
        let root = rominabox_scratch::Scratch::dir("rominabox-slang");
        let selection = custom_preset(
            &root,
            "crt.glslp",
            &[("crt.glslp", "shaders = 1\nshader0 = crt.slang\n"), ("crt.slang", SLANG)],
        );
        let resolved = resolve(&selection).unwrap();
        assert_eq!(resolved[1].relative_preset, "shaders/pal/crt.slangp");
        assert_eq!(video_driver(&selection).unwrap(), VideoDriver::Glcore);
        assert_eq!(video_driver(&ShaderSelection::default()).unwrap(), VideoDriver::Gl);
        let glsl = custom_preset(
            &root,
            "pal.glslp",
            &[("pal.glslp", "shaders = 1\nshader0 = pass.glsl\n"), ("pass.glsl", PASS)],
        );
        assert_eq!(video_driver(&glsl).unwrap(), VideoDriver::Gl);
        let _ = fs::remove_dir_all(&root);
    }

    /// All shaders of a game run on one driver, and each driver is for one
    /// language, so we reject a slang shader next to a GLSL one, in a game or
    /// in one preset, and we reject Cg too.
    #[test]
    fn shaders_in_two_languages_and_cg_are_refused() {
        let root = rominabox_scratch::Scratch::dir("rominabox-shader-languages");
        let file = |name: &str, text: &str| {
            fs::write(root.join(name), text).unwrap();
            CustomShader { name: name.split('.').next().unwrap().into(), path: root.join(name) }
        };
        let selection = ShaderSelection {
            custom: vec![file("crt.slang", SLANG), file("pal.glsl", PASS)],
            ..ShaderSelection::default()
        };
        let error = resolve(&selection).unwrap_err();
        assert!(error.contains("crt is slang and pal is GLSL"), "{error}");
        assert!(error.contains("one language"), "{error}");
        let mixed = custom_preset(
            &root,
            "mixed.slangp",
            &[("mixed.slangp", "shaders = 2\nshader0 = crt.slang\nshader1 = pal.glsl\n")],
        );
        let error = resolve(&mixed).unwrap_err();
        assert!(error.contains("crt.slang is slang and pal.glsl is GLSL"), "{error}");
        let cg = ShaderSelection {
            custom: vec![file("old.cg", "float4 main_fragment() : COLOR { return 0; }\n")],
            ..ShaderSelection::default()
        };
        assert!(resolve(&cg).unwrap_err().contains("old.cg is a Cg shader"));
        let _ = fs::remove_dir_all(&root);
    }

    /// Each catalog preset is one fragment, written in the language of the
    /// other shaders of the game, so we bundle them as slang in a slang game.
    #[test]
    fn catalog_presets_are_written_in_a_slang_game_s_language() {
        let root = rominabox_scratch::Scratch::dir("rominabox-slang-catalog");
        fs::write(root.join("crt.slang"), SLANG).unwrap();
        let selection = ShaderSelection {
            bundled: vec!["scanlines".into()],
            custom: vec![CustomShader { name: "CRT".into(), path: root.join("crt.slang") }],
            initial: Some("scanlines".into()),
        };
        let resolved = resolve(&selection).unwrap();
        assert_eq!(resolved[1].relative_preset, "shaders/scanlines/scanlines.slangp");
        assert_eq!(resolved[2].relative_preset, "shaders/crt/crt.slangp");
        let staged = rominabox_scratch::Scratch::dir("rominabox-slang-catalog-staged");
        composed(selection.clone()).write(&staged).unwrap();
        let pass = fs::read_to_string(staged.join("shaders/scanlines/scanlines.slang")).unwrap();
        assert_eq!(crate::shader_format::language(&pass), Ok(Language::Slang));
        assert!(pass.contains("float line = mod(floor(TEX0.y * TextureSize.y), 2.0);"), "{pass}");
        let preset = fs::read_to_string(staged.join("shaders/scanlines/scanlines.slangp")).unwrap();
        assert!(preset.contains("shader0 = scanlines.slang"), "{preset}");
        assert!(staged.join("shaders/crt/crt.slang").is_file());
        assert_eq!(
            launch_preset(&selection).unwrap().as_deref(),
            Some("shaders/scanlines/scanlines.slangp")
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staged);
    }

    /// We recognise a preset by its number of passes and stage it under the
    /// file name that RetroArch uses for a GLSL preset.
    #[test]
    fn a_preset_is_known_by_what_it_holds() {
        let root = rominabox_scratch::Scratch::dir("rominabox-preset-named-otherwise");
        let selection = custom_preset(
            &root,
            "pal.txt",
            &[
                ("pal.txt", "shaders = 1\nshader0 = pass.glsl\n"),
                ("pass.glsl", PASS),
            ],
        );
        let resolved = resolve(&selection).unwrap();
        assert_eq!(resolved[1].relative_preset, "shaders/pal/pal.glslp");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bundling_a_preset_uses_the_row_and_writes_glsl() {
        let composed = composed(ShaderSelection {
            bundled: vec!["scanlines".into(), "phosphor".into()],
            initial: Some("phosphor".into()),
            custom: Vec::new(),
        });
        let root = rominabox_scratch::Scratch::dir("rominabox-shader-stage");
        composed.write(&root).unwrap();
        let document = composed.text("menu.rml").unwrap();
        let list_at = document.find("id=\"shaders-list\"").expect("the shader list");
        let list = &document[list_at..document[list_at..].find("id=\"shaders-back\"").unwrap() + list_at];
        assert_eq!(list.matches("class=\"list-row ").count(), 3, "{list}");
        assert!(document.contains("id=\"phosphor\""));
        // No button on the pause row. The shader screen is an entry inside
        // Options, which contains controls, shaders and sound.
        assert!(
            !document.contains("class=\"menu-action screen-link\" id=\"shaders\""),
            "the shader screen must not add a button to the pause row"
        );
        assert!(document.contains(">SHADERS<"), "the Options entry");
        assert!(!document.contains(crate::lists::LINKS_SLOT));
        let config = composed.text("shaders.cfg").unwrap();
        assert!(config.contains("shader_preset_scanlines = \"shaders/scanlines/scanlines.glslp\""));
        assert_eq!(
            launch_preset(&ShaderSelection {
                bundled: vec!["scanlines".into(), "phosphor".into()],
                initial: Some("phosphor".into()),
                custom: Vec::new(),
            })
            .unwrap()
            .as_deref(),
            Some("shaders/phosphor/phosphor.glslp")
        );
        let preset = fs::read_to_string(root.join("shaders/scanlines/scanlines.glslp")).unwrap();
        assert!(preset.contains("shader0 = scanlines.glsl"));
        let source = fs::read_to_string(root.join("shaders/phosphor/phosphor.glsl")).unwrap();
        assert!(source.contains("#if defined(VERTEX)"));
        assert!(source.contains("#elif defined(FRAGMENT)"));
        assert!(root.join("shaders/phosphor/icon.png").is_file());
        let declarations = composed.text("design.cfg").unwrap();
        assert!(declarations.contains("screens = \"pause options controls shaders\""));
        // BACK on the shader screen returns to the screen that contains it,
        // which is Options, not the pause row.
        assert!(
            declarations.contains("screen_button_options = \"options shaders-back\""),
            "{declarations}"
        );
        assert!(declarations.contains("screen_button_pause = \"options-back\""));
    }

    /// The libretro presets have their passes in `shaders/` and their lookup
    /// textures in `resources/`. When we copy all into one folder, a preset
    /// such as `pal-r57shell.glslp` lists files that are not there, and the
    /// game runs with no shader and no warning.
    #[test]
    fn a_preset_keeps_its_folders_so_every_file_it_names_is_there() {
        let source = rominabox_scratch::Scratch::dir("rominabox-shader-folders");
        fs::create_dir_all(source.join("shaders")).unwrap();
        fs::create_dir_all(source.join("resources")).unwrap();
        let preset = source.join("pal.glslp");
        fs::write(
            &preset,
            "shaders = 1\nshader0 = shaders/pass.glsl\ntextures = \"lut\"\nlut = \"resources/lut.png\"\n",
        )
        .unwrap();
        fs::write(source.join("shaders/pass.glsl"), PASS).unwrap();
        fs::write(source.join("resources/lut.png"), b"lut").unwrap();
        let composed = composed(ShaderSelection {
            custom: vec![CustomShader {
                name: "PAL".into(),
                path: preset,
            }],
            initial: Some("pal".into()),
            bundled: Vec::new(),
        });
        let root = rominabox_scratch::Scratch::dir("rominabox-shader-folders-staged");
        composed.write(&root).unwrap();
        let staged = root.join("shaders/pal");
        assert!(staged.join("pal.glslp").is_file());
        for named in ["shaders/pass.glsl", "resources/lut.png"] {
            assert!(
                staged.join(named).is_file(),
                "the preset names {named}, which is not beside it"
            );
        }
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&root);
    }

    /// An author's preset at `folder/name`, with the other files given, as
    /// the only custom shader of a selection.
    fn custom_preset(folder: &Path, name: &str, files: &[(&str, &str)]) -> ShaderSelection {
        for (path, text) in files {
            let path = folder.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        ShaderSelection {
            custom: vec![CustomShader {
                name: "PAL".into(),
                path: folder.join(name),
            }],
            initial: Some("pal".into()),
            bundled: Vec::new(),
        }
    }

    /// A `#reference` leads to another preset, which has its files next to
    /// it. The libretro `presets/` folders contain such presets.
    #[test]
    fn a_referenced_preset_and_its_files_are_copied_too() {
        let source = rominabox_scratch::Scratch::dir("rominabox-shader-reference");
        let selection = custom_preset(
            &source,
            "pal.glslp",
            &[
                ("pal.glslp", "#reference \"base/base.glslp\"\n"),
                ("base/base.glslp", "shaders = 1\nshader0 = shaders/pass.glsl\n"),
                ("base/shaders/pass.glsl", PASS),
            ],
        );
        let root = rominabox_scratch::Scratch::dir("rominabox-shader-reference-staged");
        composed(selection).write(&root).unwrap();
        for named in ["pal.glslp", "base/base.glslp", "base/shaders/pass.glsl"] {
            assert!(root.join("shaders/pal").join(named).is_file(), "{named} was not copied");
        }
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&root);
    }

    /// We apply the rule for passes to the directives used by RetroArch, so
    /// their files must be next to the preset. We do not follow `#include`
    /// here, so we reject it and do not copy it with an unchecked path.
    #[test]
    fn a_preset_cannot_reach_outside_its_folder_through_a_directive() {
        let source = rominabox_scratch::Scratch::dir("rominabox-shader-directives");
        for (name, text) in [
            ("outside.glslp", "#reference \"../elsewhere.glslp\"\nshaders = 1\nshader0 = pass.glsl\n"),
            ("included.glslp", "#include \"more.cfg\"\nshaders = 1\nshader0 = pass.glsl\n"),
        ] {
            let selection = custom_preset(
                &source,
                name,
                &[(name, text), ("pass.glsl", PASS), ("more.cfg", "\n")],
            );
            assert!(resolve(&selection).is_err(), "{name} was accepted");
        }
        let _ = fs::remove_dir_all(&source);
    }

    /// The picture of the row is `icon.png` in the preset folder, written last,
    /// so it would replace a preset file of that name without a warning.
    #[test]
    fn a_preset_file_the_row_picture_would_replace_is_refused() {
        let source = rominabox_scratch::Scratch::dir("rominabox-shader-icon");
        let selection = custom_preset(
            &source,
            "pal.glslp",
            &[
                (
                    "pal.glslp",
                    "shaders = 1\nshader0 = pass.glsl\ntextures = \"icon\"\nicon = \"icon.png\"\n",
                ),
                ("pass.glsl", PASS),
                ("icon.png", "lookup"),
            ],
        );
        assert!(resolve(&selection).is_err(), "a preset's icon.png would be overwritten");
        let _ = fs::remove_dir_all(&source);
    }

    /// A slang pass is compiled with the files it `#include`s, read from next
    /// to the file that includes them, and without one of them the game runs
    /// unfiltered with no warning. The libretro passes include from a folder
    /// next to the preset folder (`../include/`), which still belongs to it.
    #[test]
    fn a_slang_pass_takes_the_files_it_includes() {
        let source = rominabox_scratch::Scratch::dir("rominabox-slang-includes");
        let pass = "#version 450\n#include \"../include/common.inc\"\n";
        let selection = custom_preset(
            &source,
            "crt.slangp",
            &[
                ("crt.slangp", "shaders = 1\nshader0 = shaders/crt.slang\n"),
                ("shaders/crt.slang", pass),
                ("include/common.inc", "#pragma stage vertex\n#include \"nested.inc\"\n"),
                ("include/nested.inc", "#pragma stage fragment\n"),
            ],
        );
        let root = rominabox_scratch::Scratch::dir("rominabox-slang-includes-staged");
        composed(selection).write(&root).unwrap();
        let staged = ["crt.slangp", "shaders/crt.slang", "include/common.inc", "include/nested.inc"];
        for named in staged {
            let path = root.join("shaders/pal").join(named);
            assert!(path.is_file(), "{named} was not copied");
        }

        let lone_files = [("lone/crt.slang", "#version 450\n#include \"stages.inc\"\n"), ("lone/stages.inc", SLANG)];
        let lone = resolve(&custom_preset(&source, "lone/crt.slang", &lone_files)).unwrap();
        let names: Vec<&str> = lone[1].files.iter().map(|(_, name)| name.as_str()).collect();
        assert!(names.contains(&"stages.inc"), "a lone pass's include was not copied: {names:?}");

        let away = [("away/crt.slang", "#version 450\n#include \"../include/common.inc\"\n")];
        let error = resolve(&custom_preset(&source, "away/crt.slang", &away)).unwrap_err();
        assert!(error.contains("beside it"), "{error}");
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_ordinary_menu_gains_no_shader_screen() {
        let composed = composed(ShaderSelection::default());
        let document = composed.text("menu.rml").unwrap();
        assert!(!document.contains("id=\"shaders\""));
        assert!(!document.contains("id=\"shaders-panel\""));
        assert!(composed.text("shaders.cfg").is_none());
        assert!(!composed
            .names()
            .iter()
            .any(|name| name.starts_with("shaders")));
        assert!(!document.contains("video_shader"));
    }
}
