//! The files listed in a shader preset of the author, read from the preset.
//!
//! A preset lists its passes, its lookup textures and the presets it
//! `#reference`s, each relative to itself, and a slang pass lists the files
//! it `#include`s, relative to itself. At export we copy them next to the
//! copied preset at the same relative paths, so each must stay inside the
//! preset folder, and each pass must be in a language for exported games.

use crate::shader_format::{one_language, require_runnable_pass, Language};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// How many presets deep `#reference` may go, as in RetroArch, and how
/// many files deep a slang pass's `#include`s may go.
const REFERENCE_DEPTH: usize = 16;

/// The files listed in a preset, each as (source, its path next to the
/// preset as written in the presets), and the language of its passes. The
/// libretro presets have passes in `shaders/` and textures in `resources/`.
pub fn files(preset: &Path) -> Result<(Language, Vec<(PathBuf, String)>), String> {
    let mut files = Vec::new();
    let mut passes = Vec::new();
    let root = preset.parent().unwrap_or_else(|| Path::new("."));
    collect_files(preset, root, Path::new(""), 0, &mut files, &mut passes)?;
    let named = passes
        .iter()
        .map(|(name, language)| (name.as_str(), *language));
    match one_language(named)? {
        Some(language) => Ok((language, files)),
        None => Err("a shader preset names no shader pass".into()),
    }
}

/// Add the files listed in `preset` to `files`, as paths from the folder of
/// the top preset (`prefix` is the location of `preset` in it), and its
/// passes, by name and language, to `passes`.
fn collect_files(
    preset: &Path,
    root: &Path,
    prefix: &Path,
    depth: usize,
    files: &mut Vec<(PathBuf, String)>,
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
                let (source, spelled) = beside(
                    directory,
                    prefix,
                    value.trim().trim_matches('"'),
                    line_number,
                )?;
                add_file(files, source.clone(), &spelled);
                let inner = Path::new(&spelled)
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf();
                collect_files(&source, root, &inner, depth + 1, files, passes)?;
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
        let (source, spelled) = beside(directory, prefix, value, *line_number)?;
        if is_pass {
            let language = require_runnable_pass(&source)?;
            passes.push((crate::shader_format::name(&source), language));
            if language == Language::Slang {
                for (included, spelled) in includes(&source, root)? {
                    add_file(files, included, &spelled);
                }
            }
        }
        add_file(files, source, &spelled);
    }
    Ok(())
}

/// A file listed in a preset, with its location and its path from the folder
/// of the top preset, with `/` between parts.
fn beside(
    directory: &Path,
    prefix: &Path,
    value: &str,
    line_number: usize,
) -> Result<(PathBuf, String), String> {
    let relative = Path::new(value);
    safe_relative(relative)?;
    let source = directory.join(relative);
    if !source.is_file() {
        return Err(format!(
            "shader preset line {line_number} names a missing file: {value}"
        ));
    }
    let spelled = prefix
        .join(relative)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok((source, spelled))
}

/// The files that a slang pass `#include`s, and those they include, each as
/// (source, its path from `root` with `/` between parts). An included file is
/// read from next to the file that includes it when the pass is compiled, and
/// without it the game runs unfiltered, so we copy them with the pass. Each
/// must be inside `root`, the folder that we copy at export.
pub fn includes(pass: &Path, root: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let root = normalized(root);
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    let mut reading = vec![(normalized(pass), 0)];
    while let Some((file, depth)) = reading.pop() {
        if depth > REFERENCE_DEPTH {
            return Err("a shader's files include each other too deeply".into());
        }
        let text = String::from_utf8_lossy(
            &fs::read(&file)
                .map_err(|error| format!("could not read {}: {error}", file.display()))?,
        )
        .into_owned();
        for value in text.lines().filter_map(included) {
            let source = normalized(&file.parent().unwrap_or(&root).join(value));
            let relative = source.strip_prefix(&root).map_err(|_| {
                format!("a shader preset can only use files beside it, not {value}")
            })?;
            if !source.is_file() {
                return Err(format!(
                    "{} includes a missing file: {value}",
                    crate::shader_format::name(&file)
                ));
            }
            let spelled = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if !found.iter().any(|(_, existing)| existing == &spelled) {
                found.push((source.clone(), spelled));
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

/// `path` with its `.` and `..` parts resolved without a file system call,
/// because the file system would follow links out of the folder.
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

fn add_file(files: &mut Vec<(PathBuf, String)>, source: PathBuf, spelled: &str) {
    if !files.iter().any(|(_, existing)| existing == spelled) {
        files.push((source, spelled.to_string()));
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
