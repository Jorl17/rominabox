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
    if extension.eq_ignore_ascii_case("m3u") || extension.eq_ignore_ascii_case("gdi") {
        return Err(format!(
            "{} manifests are not yet supported safely. Export the primary CUE sheet or a single-file CHD/ISO image instead.",
            extension.to_ascii_uppercase()
        ));
    }
    let mut files = vec![ContentFile {
        source: entrypoint.clone(),
        relative: entry_relative.clone(),
        staged_bytes: None,
    }];

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
}
