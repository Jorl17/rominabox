//! Collect the complete, bounded set of files required to load game content.
//!
//! Most cartridges are a single file. A CUE sheet lists other files, and we
//! require every `FILE` entry and keep it at the same relative path beside the
//! staged CUE sheet. We use this module for playable exports and for project
//! archives, so neither can lose disc tracks without an error.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentFile {
    pub source: PathBuf,
    pub relative: PathBuf,
    /// Normalized manifest bytes when the staged file must use portable path
    /// separators. Otherwise we copy the file directly from `source`.
    pub staged_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentSet {
    pub entrypoint: PathBuf,
    pub files: Vec<ContentFile>,
}

/// Find every file referenced from `entrypoint`, without leaving its
/// folder. Refuse a symbolic link that resolves to a path outside that
/// folder.
pub fn collect(entrypoint: &Path) -> Result<ContentSet, String> {
    if !entrypoint.is_file() {
        return Err(format!(
            "game content does not exist or is not a regular file: {}",
            entrypoint.display()
        ));
    }
    let entrypoint = fs::canonicalize(entrypoint)
        .map_err(|error| format!("resolve game content {}: {error}", entrypoint.display()))?;
    let root = entrypoint
        .parent()
        .ok_or_else(|| "game content has no containing directory".to_string())?
        .to_path_buf();
    let name = entrypoint
        .file_name()
        .ok_or_else(|| "game content has no filename".to_string())?;
    let entry_relative = PathBuf::from(name);
    let extension = entrypoint
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    // We may recognise a file that we cannot export. The formats of that kind
    // are declared in the console packages, not listed here. We collect
    // without a system, so we refuse any extension that some console
    // declares recognise-only.
    if crate::systems::registry()
        .iter()
        .any(|system| system.is_recognize_only(extension))
    {
        return Err(format!(
            "{} manifests can be recognised but not exported yet: they point at other files, and collecting those safely is not implemented. Export the primary CUE sheet or a single-file CHD/ISO image instead.",
            extension.to_ascii_uppercase()
        ));
    }
    let mut files = vec![ContentFile {
        source: entrypoint.clone(),
        relative: entry_relative.clone(),
        staged_bytes: None,
    }];

    // Some protection data is outside the disc image, and nothing inside the
    // image refers to it. A LibCrypt PlayStation game has its subchannel data
    // in a sibling .sbi. Without it the game boots and fails later, which is
    // far worse than refusing to export. The extensions that go with the
    // content are declared per console, not listed here.
    let stem = entrypoint.file_stem().unwrap_or_default();
    let mut support: Vec<String> = crate::systems::registry()
        .iter()
        .flat_map(|system| system.support_files.iter().cloned())
        .collect();
    support.sort_unstable();
    support.dedup();
    for extension in support {
        let sibling = root.join(stem).with_extension(&extension);
        if sibling.is_file() && sibling != entrypoint {
            let sibling_name = sibling
                .file_name()
                .ok_or_else(|| format!("support file has no name: {}", sibling.display()))?;
            files.push(ContentFile {
                source: sibling.clone(),
                relative: PathBuf::from(sibling_name),
                staged_bytes: None,
            });
        }
    }

    if extension.eq_ignore_ascii_case("cue") {
        let cue = fs::read_to_string(&entrypoint)
            .map_err(|error| format!("read CUE sheet {}: {error}", entrypoint.display()))?;
        let dependencies = cue_file_references(&cue)?;
        if dependencies.is_empty() {
            return Err(format!(
                "CUE sheet has no FILE entries and cannot be exported safely: {}",
                entrypoint.display()
            ));
        }
        let mut seen = HashSet::from([entry_relative.clone()]);
        for relative in dependencies {
            validate_relative_content_path(&relative)?;
            if !seen.insert(relative.clone()) {
                continue;
            }
            let candidate = root.join(&relative);
            if !candidate.is_file() {
                return Err(format!(
                    "CUE sheet references a missing track file: {} (from {})",
                    relative.display(),
                    entrypoint.display()
                ));
            }
            let resolved = fs::canonicalize(&candidate)
                .map_err(|error| format!("resolve CUE track {}: {error}", candidate.display()))?;
            if !resolved.starts_with(&root) {
                return Err(format!(
                    "CUE track escapes the game content folder: {}",
                    relative.display()
                ));
            }
            files.push(ContentFile {
                source: resolved,
                relative,
                staged_bytes: None,
            });
        }
        // On macOS, paths in a sheet must have POSIX separators for the cores.
        // Keep the source file untouched and normalize only the private staged/project copy.
        files[0].staged_bytes = Some(normalize_cue_separators(&cue).into_bytes());
    }

    Ok(ContentSet {
        entrypoint: entry_relative,
        files,
    })
}

/// The game file for a dropped path.
///
/// A folder is not a game file, so when someone drops a folder, for example
/// one with a Dreamcast game, we use the GD-ROM inside it. A sibling `.sbi` is
/// subchannel data, not a game, so for that file we use the disc next to it.
pub fn resolve_dropped(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        return sole_game_in(path);
    }
    if !path.is_file() {
        return Err(format!(
            "game content does not exist or is not a regular file: {}",
            path.display()
        ));
    }
    let extension = extension_of(path);
    if is_support_extension(&extension) {
        if let Some(game) = sibling_game(path) {
            return Ok(game);
        }
    }
    Ok(path.to_path_buf())
}

fn sole_game_in(directory: &Path) -> Result<PathBuf, String> {
    let mut games = Vec::new();
    let mut sheets = Vec::new();
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read game folder {}: {error}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("read game folder {}: {error}", directory.display()))?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        let extension = extension_of(&path);
        if extension.is_empty() || is_support_extension(&extension) {
            continue;
        }
        if crate::systems::candidates_for_extension(&extension).is_empty() {
            continue;
        }
        if is_sheet(&extension) {
            sheets.push(path);
        } else {
            games.push(path);
        }
    }
    if sheets.len() == 1 {
        return Ok(sheets.remove(0));
    }
    if sheets.is_empty() && games.len() == 1 {
        return Ok(games.remove(0));
    }
    if sheets.is_empty() && games.is_empty() {
        return Err(format!(
            "This folder has no game file: {}",
            directory.display()
        ));
    }
    Err(format!(
        "This folder contains more than one game. Drop the game file itself: {}",
        directory.display()
    ))
}

fn sibling_game(support: &Path) -> Option<PathBuf> {
    let stem = support.file_stem()?;
    let directory = support.parent()?;
    let mut found = Vec::new();
    for system in crate::systems::registry() {
        for extension in &system.extensions {
            let candidate = directory.join(stem).with_extension(extension);
            if candidate.is_file() && !found.contains(&candidate) {
                found.push(candidate);
            }
        }
    }
    if found.len() == 1 {
        return found.pop();
    }
    found
        .into_iter()
        .find(|candidate| is_sheet(&extension_of(candidate)))
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_support_extension(extension: &str) -> bool {
    crate::systems::registry().iter().any(|system| {
        system
            .support_files
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    })
}

fn is_sheet(extension: &str) -> bool {
    extension == "cue"
        || crate::systems::registry()
            .iter()
            .any(|system| system.is_recognize_only(extension))
}

fn normalize_cue_separators(cue: &str) -> String {
    cue.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let indentation = &line[..line.len() - trimmed.len()];
            let Some(rest) = strip_ascii_keyword(trimmed, "FILE") else {
                return line.to_string();
            };
            format!("{indentation}FILE{}", rest.replace('\\', "/"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if cue.ends_with('\n') { "\n" } else { "" }
}

fn cue_file_references(cue: &str) -> Result<Vec<PathBuf>, String> {
    let mut references = Vec::new();
    for (index, raw_line) in cue.lines().enumerate() {
        let line = raw_line.trim_start();
        let Some(rest) = strip_ascii_keyword(line, "FILE") else {
            continue;
        };
        let rest = rest.trim_start();
        let value = if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').ok_or_else(|| {
                format!(
                    "invalid CUE FILE entry on line {}: missing closing quote",
                    index + 1
                )
            })?;
            &quoted[..end]
        } else {
            rest.split_ascii_whitespace().next().unwrap_or_default()
        };
        if value.is_empty() {
            return Err(format!(
                "invalid CUE FILE entry on line {}: filename is empty",
                index + 1
            ));
        }
        // CUE sheets often contain Windows separators, even on macOS.
        references.push(PathBuf::from(value.replace('\\', "/")));
    }
    Ok(references)
}

fn strip_ascii_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let prefix = line.get(..keyword.len())?;
    if !prefix.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let rest = &line[keyword.len()..];
    rest.starts_with(char::is_whitespace).then_some(rest)
}

fn validate_relative_content_path(path: &Path) -> Result<(), String> {
    let text = path.to_string_lossy();
    if text.is_empty()
        || text.contains(':')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "CUE track path must stay within the game content folder: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rominabox-content-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cue_collects_each_referenced_track_with_relative_layout() {
        let root = fixture("multi-track");
        fs::create_dir(root.join("audio")).unwrap();
        fs::write(root.join("disc.bin"), b"data").unwrap();
        fs::write(root.join("audio/track 02.bin"), b"audio").unwrap();
        let cue = root.join("game.cue");
        fs::write(
            &cue,
            "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE1/2352\nFILE \"audio\\track 02.bin\" BINARY\n  TRACK 02 AUDIO\n",
        )
        .unwrap();

        let content = collect(&cue).unwrap();
        let relative = content
            .files
            .iter()
            .map(|file| file.relative.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            relative,
            ["game.cue", "disc.bin", "audio/track 02.bin"]
                .map(PathBuf::from)
                .to_vec()
        );
    }

    #[test]
    fn cue_rejects_parent_traversal() {
        let root = fixture("traversal");
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"../secret.bin\" BINARY\n").unwrap();

        let error = collect(&cue).unwrap_err();
        assert!(error.contains("must stay within"), "{error}");
    }

    #[test]
    fn cue_reports_a_missing_track_instead_of_exporting_only_the_sheet() {
        let root = fixture("missing");
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"missing.bin\" BINARY\n").unwrap();

        let error = collect(&cue).unwrap_err();
        assert!(error.contains("missing track file"), "{error}");
        assert!(error.contains("missing.bin"), "{error}");
    }

    /// The formats that we recognise but cannot export are declared in the
    /// console packages, so the drop step and the export step always agree.
    #[test]
    fn a_recognise_only_format_is_refused_with_a_reason() {
        let root = fixture("recognise-only");
        let playlist = root.join("game.m3u");
        fs::write(&playlist, b"disc1.cue\n").unwrap();

        let refusal = collect(&playlist).expect_err("m3u cannot be collected");
        assert!(refusal.contains("M3U"), "{refusal}");
        assert!(
            refusal.contains("recognised but not exported"),
            "the message should explain the gap, not just refuse: {refusal}"
        );
    }

    /// The declaration is the source of this rule, not a list in this file.
    #[test]
    fn the_refused_formats_come_from_the_console_packages() {
        let declared: Vec<&str> = crate::systems::registry()
            .iter()
            .flat_map(|system| system.recognize_only.iter().map(String::as_str))
            .collect();
        assert!(
            declared.contains(&"m3u") && declared.contains(&"gdi"),
            "expected the disc consoles to declare their manifest formats, got {declared:?}"
        );
        for system in crate::systems::registry() {
            for extension in &system.recognize_only {
                assert!(
                    system.extensions.contains(extension),
                    "{} declares {extension} recognise-only but does not recognise it at all",
                    system.id
                );
            }
        }
    }
}
