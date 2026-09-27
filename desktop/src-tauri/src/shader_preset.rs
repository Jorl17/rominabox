//! The files listed in a shader preset of the author, read from the preset.
//!
//! A preset lists its passes, its lookup textures and the presets it
//! `#reference`s, each relative to itself. At export we copy them next to the
//! copied preset at the same relative paths, so each must stay inside the
//! preset folder, and each pass must be in a language for exported games.

use crate::shader_format::{one_language, require_runnable_pass, Language};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The maximum depth of `#reference` between presets, as in RetroArch.
const REFERENCE_DEPTH: usize = 16;

/// The files listed in a preset, each as (source, its path next to the
/// preset as written in the presets), and the language of its passes. The
/// libretro presets have passes in `shaders/` and textures in `resources/`.
pub fn files(preset: &Path) -> Result<(Language, Vec<(PathBuf, String)>), String> {
    let mut files = Vec::new();
    let mut passes = Vec::new();
    collect_files(preset, Path::new(""), 0, &mut files, &mut passes)?;
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
                collect_files(&source, &inner, depth + 1, files, passes)?;
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
