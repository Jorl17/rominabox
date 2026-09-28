//! The files an author's shader lists, which we read from the shader.
//!
//! A preset lists its passes, its lookup textures and the presets it
//! `#reference`s, each relative to itself, and a slang pass lists the files
//! it `#include`s, relative to itself. Any of these paths may go through
//! `../`. For example, libretro's `crt/crt-royale-pal-r57shell.glslp` has
//! its pass in `../pal/shaders/`. At export we lay out the shader and its
//! files below the lowest folder that contains them all, each at its path
//! from there, so every relative path still leads to its file and none leads
//! out of the game's shader folder. We export only the named files, and each
//! must be the kind of file in its line, so a `../` cannot bring anything
//! else, such as a private key, into a game. A pass must be in a shader
//! language for an exported game, a lookup texture a picture, a `#reference`
//! a preset and an `#include` shader source.

use crate::shader_format::{one_language, require_runnable_pass, Language};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// How many presets deep `#reference` may go, as in RetroArch, and how
/// many files deep a slang pass's `#include`s may go.
const REFERENCE_DEPTH: usize = 16;

/// The kind of file in a line.
#[derive(Clone, Copy)]
enum Named {
    /// `shaderN`, which we check as a pass, for its language, once we find it.
    Pass,
    /// A lookup texture.
    Picture,
    /// A `#reference`d preset.
    Preset,
    /// A file a slang pass `#include`s.
    Source,
}

impl Named {
    /// Whether `path` is this kind of file. We recognise a picture by its
    /// first bytes, the header of a PNG, JPEG, BMP or TGA, which are the
    /// texture formats in RetroArch. A preset or an included file is text with
    /// a name as in libretro's packs. A `.params` file contains only a preset's
    /// parameter values.
    fn holds(self, path: &Path) -> Result<bool, String> {
        let extensions: &[&str] = match self {
            Named::Pass => return Ok(true),
            Named::Picture => {
                let bytes = read(path)?;
                let tga = bytes.len() >= 18
                    && bytes[1] <= 1
                    && matches!(bytes[2], 1 | 2 | 3 | 9 | 10 | 11)
                    && matches!(bytes[16], 8 | 15 | 16 | 24 | 32);
                return Ok(bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    || bytes.starts_with(b"\xff\xd8\xff")
                    || bytes.starts_with(b"BM")
                    || tga);
            }
            Named::Preset => &["glslp", "slangp", "params"],
            Named::Source => &["slang", "glsl", "inc", "h", "hlsl"],
        };
        let extension = path.extension().and_then(|extension| extension.to_str());
        let named = extension.is_some_and(|extension| {
            extensions.iter().any(|name| name.eq_ignore_ascii_case(extension))
        });
        Ok(named && !read(path)?.contains(&0))
    }

    /// What a file of this kind is called in a sentence.
    fn called(self) -> &'static str {
        match self {
            Named::Pass => "a shader pass",
            Named::Picture => "a PNG, JPEG, BMP or TGA picture",
            Named::Preset => "a shader preset",
            Named::Source => "shader source",
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

/// An author's shader and the files it lists, laid out below the lowest
/// folder that contains them all.
pub struct Layout {
    /// The folder of the author's own file, relative to that folder, with `/`
    /// between parts, or empty when it is that folder.
    pub folder: String,
    /// The files the shader names, each as (source, its path from there with
    /// `/` between parts).
    pub files: Vec<(PathBuf, String)>,
}

/// A preset's layout, and the language its passes share.
pub fn preset(path: &Path) -> Result<(Language, Layout), String> {
    let path = absolute(path)?;
    let mut files = Vec::new();
    let mut passes = Vec::new();
    collect_files(&path, 0, &mut files, &mut passes)?;
    let languages = passes
        .iter()
        .map(|(name, language)| (name.as_str(), *language));
    match one_language(languages)? {
        Some(language) => Ok((language, laid_out(&path, &files)?)),
        None => Err("a shader preset names no shader pass".into()),
    }
}

/// A lone pass's layout: a slang pass names the files it `#include`s, and
/// a GLSL pass names none.
pub fn pass(path: &Path, language: Language) -> Result<Layout, String> {
    let path = absolute(path)?;
    let included = match language {
        Language::Slang => includes(&path)?,
        Language::Glsl => Vec::new(),
    };
    laid_out(&path, &included)
}

/// Add the files `preset` lists to `files`, and its passes, by name and
/// language, to `passes`.
fn collect_files(
    preset: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
    passes: &mut Vec<(String, Language)>,
) -> Result<(), String> {
    if depth > REFERENCE_DEPTH {
        return Err("shader presets reference each other too deeply".into());
    }
    let text = fs::read_to_string(preset)
        .map_err(|error| format!("could not read shader preset: {error}"))?;
    let directory = preset.parent().unwrap_or_else(|| Path::new("."));
    let mut entries: Vec<(usize, &str, &str)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        let line_number = index + 1;
        if line.is_empty() {
            continue;
        }
        // In RetroArch, `#include` and `#reference` are directives and every
        // other `#` line is a comment.
        if let Some(comment) = line.strip_prefix('#') {
            if comment.starts_with("include ") {
                return Err(format!(
                    "shader preset line {line_number} uses #include, which exported games do not follow"
                ));
            }
            if let Some(value) = comment.strip_prefix("reference ") {
                let source = named(directory, value.trim().trim_matches('"'), Named::Preset)
                    .map_err(|error| format!("shader preset line {line_number} {error}"))?;
                add_file(files, source.clone());
                collect_files(&source, depth + 1, files, passes)?;
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            entries.push((line_number, key.trim(), value.trim().trim_matches('"')));
        }
    }
    let textures: Vec<&str> = entries
        .iter()
        .find(|(_, key, _)| *key == "textures")
        .map(|(_, _, value)| {
            value
                .split(';')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .collect()
        })
        .unwrap_or_default();
    for (line_number, key, value) in &entries {
        let is_pass = key.starts_with("shader")
            && key.len() > "shader".len()
            && key["shader".len()..].chars().all(|c| c.is_ascii_digit());
        if !is_pass && !textures.contains(key) {
            continue;
        }
        let kind = if is_pass { Named::Pass } else { Named::Picture };
        let source = named(directory, value, kind)
            .map_err(|error| format!("shader preset line {line_number} {error}"))?;
        if is_pass {
            let language = require_runnable_pass(&source)?;
            passes.push((crate::shader_format::name(&source), language));
            if language == Language::Slang {
                for included in includes(&source)? {
                    add_file(files, included);
                }
            }
        }
        add_file(files, source);
    }
    Ok(())
}

/// The file at path `value`, relative to `directory`, with `../` resolved,
/// when it is the kind of file for its line. A backslash in the path is a
/// separator, as in RetroArch (`fill_pathname_expanded_and_absolute`). The
/// error message ends a sentence about the line or file with the path.
fn named(directory: &Path, value: &str, kind: Named) -> Result<PathBuf, String> {
    let separated = value.replace('\\', "/");
    let relative = Path::new(&separated);
    if relative
        .components()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(format!(
            "names a file by an absolute path, which an exported game cannot reach: {value}"
        ));
    }
    let source = normalized(&directory.join(relative));
    if !source.is_file() {
        return Err(format!("names a missing file: {value}"));
    }
    if !kind.holds(&source)? {
        return Err(format!("names {value}, which is not {}", kind.called()));
    }
    Ok(source)
}

/// The files a slang pass `#include`s, and those they include. In RetroArch
/// an included file must be beside the file that includes it when the pass
/// is compiled, and without it the game has no filter, so we export these
/// files with the pass.
fn includes(pass: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut reading = vec![(pass.to_path_buf(), 0)];
    while let Some((file, depth)) = reading.pop() {
        if depth > REFERENCE_DEPTH {
            return Err("a shader's files include each other too deeply".into());
        }
        let text = crate::shader_format::text(&file)?;
        let directory = file.parent().unwrap_or_else(|| Path::new("."));
        for value in text.lines().filter_map(included) {
            let source = named(directory, value, Named::Source).map_err(|error| {
                format!("{} {error}", crate::shader_format::name(&file))
            })?;
            if !found.contains(&source) {
                found.push(source.clone());
                reading.push((source, depth + 1));
            }
        }
    }
    Ok(found)
}

/// The file a line `#include "file"` names.
fn included(line: &str) -> Option<&str> {
    let rest = line
        .trim()
        .strip_prefix('#')?
        .trim_start()
        .strip_prefix("include")?;
    let (value, _) = rest.trim().strip_prefix('"')?.split_once('"')?;
    Some(value)
}

/// `own` and the files it lists, below the lowest folder that contains them.
fn laid_out(own: &Path, files: &[PathBuf]) -> Result<Layout, String> {
    let folder_of = |file: &Path| file.parent().unwrap_or(file).to_path_buf();
    let mut root = folder_of(own);
    for file in files {
        let folder = folder_of(file);
        while !folder.starts_with(&root) {
            if !root.pop() {
                return Err("a shader's files must all be on one drive".into());
            }
        }
    }
    Ok(Layout {
        folder: from(&root, &folder_of(own))?,
        files: files
            .iter()
            .map(|file| Ok((file.clone(), from(&root, file)?)))
            .collect::<Result<_, String>>()?,
    })
}

/// `path` below `root`, with `/` between parts. We accept only a plain name
/// as a part, so nothing we stage with it ends up outside the staging
/// folder.
fn from(root: &Path, path: &Path) -> Result<String, String> {
    let outside = || format!("{} is not below {}", path.display(), root.display());
    let relative = path.strip_prefix(root).map_err(|_| outside())?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy()),
            _ => return Err(outside()),
        }
    }
    Ok(parts.join("/"))
}

/// `path` from the file system's root, with its `.` and `..` parts resolved
/// by name, without asking the file system. What we stage has no links, so
/// we find the staged files where these names lead.
fn absolute(path: &Path) -> Result<PathBuf, String> {
    std::path::absolute(path)
        .map(|path| normalized(&path))
        .map_err(|error| format!("could not find {}: {error}", path.display()))
}

/// `path` with its `.` and `..` parts resolved by name.
fn normalized(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved
}

fn add_file(files: &mut Vec<PathBuf>, source: PathBuf) {
    if !files.contains(&source) {
        files.push(source);
    }
}
