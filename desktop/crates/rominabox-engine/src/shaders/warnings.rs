//! What we tell an author about shaders that will not load in a game on one
//! of the platforms. On a Mac, GLSL above version 120 does not compile
//! (`shader_format::newest_glsl`), and on Windows, a file at a path longer
//! than `packaging::LONGEST_PATH` does not open. In both cases the player
//! sees the game without that filter, so we warn and make the game either
//! way.
//!
//! We show the sentences in two places. On the Menu step, we write them for
//! every platform, each naming its platform, because the author chooses the
//! platforms later, and we mark the shaders they are about
//! (`shader_warnings`). On Create app, we show one pop-up for the platforms
//! of the export (`notice`).

use super::{resolved, Destination, ResolvedShader, ShaderSelection};
use crate::packaging::ExportTarget;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A shader the author added, by its name in the game's list and its file
/// in the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Added {
    name: String,
    path: PathBuf,
}

/// Why shaders will not load in a game on a platform.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Problem {
    /// Shaders with a GLSL pass above the newest GLSL version for the
    /// platform, in a GLSL game: the author's own (`theirs`), which have no
    /// other version, and libretro presets (`ours`), which are in GLSL because
    /// the author's shaders (`authors`, every shader the author added) are.
    TooNewGlsl {
        theirs: Vec<Added>,
        ours: Vec<String>,
        authors: Vec<Added>,
    },
    /// The author's shaders with a file at a path that may be longer than
    /// Windows allows, each with that file's path in the shader's folder.
    LongPaths { shaders: Vec<(Added, String)> },
}

/// The words for a platform in our sentences.
struct Words {
    on: &'static str,
    heading: &'static str,
    unsupported: &'static str,
}

fn words(platform: ExportTarget) -> Words {
    match platform {
        ExportTarget::Macos => Words {
            on: "on a Mac",
            heading: "On a Mac",
            unsupported: "Macs do not support",
        },
        ExportTarget::Windows => Words {
            on: "on Windows",
            heading: "On Windows",
            unsupported: "Windows does not support",
        },
    }
}

/// The shaders in `selection` that will not load in a game made for
/// `destination`, by the reason.
fn problems(selection: &ShaderSelection, destination: &Destination) -> Result<Vec<Problem>, String> {
    let (_, shaders) = resolved(selection, Some(destination))?;
    // A shader from the author is one that has files, and the file in the
    // selection comes first.
    let added = |item: &ResolvedShader| {
        item.files.first().map(|(path, _)| Added {
            name: item.name.clone(),
            path: path.clone(),
        })
    };
    let mut problems = Vec::new();
    if let Some(newest) = crate::shader_format::newest_glsl(destination.platform) {
        let (theirs, ours): (Vec<&ResolvedShader>, Vec<&ResolvedShader>) = shaders
            .iter()
            .filter(|item| item.glsl_version.is_some_and(|version| version > newest))
            .partition(|item| !item.files.is_empty());
        let authors: Vec<Added> = shaders.iter().filter_map(added).collect();
        // Without a shader of the author's, we chose slang for any preset
        // whose GLSL does not compile on the platform.
        if !theirs.is_empty() || (!ours.is_empty() && !authors.is_empty()) {
            problems.push(Problem::TooNewGlsl {
                theirs: theirs.into_iter().filter_map(added).collect(),
                ours: ours.into_iter().map(|item| item.name.clone()).collect(),
                authors,
            });
        }
    }
    if destination.platform == ExportTarget::Windows {
        let mut deep = Vec::new();
        for item in &shaders {
            let Some(shader) = added(item) else {
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
                    deep.push((shader, name.clone()));
                }
            }
        }
        if !deep.is_empty() {
            problems.push(Problem::LongPaths { shaders: deep });
        }
    }
    Ok(problems)
}

/// `items` in a sentence: "A", "A and B", or "A, B and C".
fn joined(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A path in a shader's folder, with the folders in the middle replaced by
/// an ellipsis when there are more than three parts: its first folder, then
/// its last folder and the file.
fn shortened(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [first, .., folder, file] if parts.len() > 3 => format!("{first}/\u{2026}/{folder}/{file}"),
        _ => path.to_string(),
    }
}

/// The names in quotes, joined in a sentence.
fn quoted(names: impl IntoIterator<Item = String>) -> String {
    joined(&names.into_iter().map(|name| format!("\u{201c}{name}\u{201d}")).collect::<Vec<_>>())
}

/// `one` for a single item, else `many`.
fn counted<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}

/// What we tell the author about a problem: what will happen, beside a help
/// button, and in the tooltip of that button, why and what to do about it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub text: String,
    pub detail: String,
}

impl Problem {
    /// The files in the selection of the author's shaders that the problem
    /// is about. When presets will not load, every shader of the author's,
    /// because we make the game in GLSL for them.
    fn paths(&self) -> Vec<&Path> {
        match self {
            Problem::TooNewGlsl { theirs, ours, authors } => {
                let about = if ours.is_empty() { theirs } else { authors };
                about.iter().map(|shader| shader.path.as_path()).collect()
            }
            Problem::LongPaths { shaders } => shaders.iter().map(|(shader, _)| shader.path.as_path()).collect(),
        }
    }

    /// What we tell the author. With `on`, the text names the platform, and
    /// without it, the heading above it does.
    fn warning(&self, platform: ExportTarget, on: bool) -> Warning {
        let words = words(platform);
        let on = if on { format!(" {}", words.on) } else { String::new() };
        let will_not = |names: Vec<String>, verb: &str| {
            let count = names.len();
            format!(
                "{} {} {verb}{on}. The game will run fine, but without {}.",
                counted(count, "The shader", "The shaders"),
                quoted(names),
                counted(count, "the filter", "these filters"),
            )
        };
        let names = |shaders: &[Added]| shaders.iter().map(|shader| shader.name.clone()).collect::<Vec<_>>();
        match self {
            Problem::TooNewGlsl { theirs, ours, .. } if ours.is_empty() => {
                let count = theirs.len();
                Warning {
                    text: will_not(names(theirs), "won\u{2019}t load"),
                    detail: format!(
                        "This is because {} {} GLSL {}. If you have {} of the {}, add {} instead.",
                        words.unsupported,
                        counted(count, "this", "these"),
                        counted(count, "shader", "shaders"),
                        counted(count, "a slang version", "slang versions"),
                        counted(count, "shader", "shaders"),
                        counted(count, "that", "those"),
                    ),
                }
            }
            Problem::TooNewGlsl { theirs, ours, authors } => {
                let failing: Vec<String> = names(theirs).into_iter().chain(ours.iter().cloned()).collect();
                let by = authors.len();
                let reason = quoted(names(authors));
                Warning {
                    text: will_not(failing.clone(), "won\u{2019}t load"),
                    detail: format!(
                        "This is because {reason} {}, so the game uses the GLSL version of every shader, \
                         and {} the {} of {}. If you have {} of {reason}, add {} instead.",
                        counted(by, "is a GLSL shader", "are GLSL shaders"),
                        words.unsupported,
                        counted(failing.len(), "GLSL version", "GLSL versions"),
                        joined(&failing),
                        counted(by, "a slang version", "slang versions"),
                        counted(by, "that", "those"),
                    ),
                }
            }
            Problem::LongPaths { shaders } => Warning {
                text: will_not(shaders.iter().map(|(shader, _)| shader.name.clone()).collect(), "may not load"),
                detail: match shaders.as_slice() {
                    [(_, file)] => format!(
                        "This is because the full path of one of its files, \u{201c}{}\u{201d}, may be \
                         longer than Windows allows. If you can, move the shader\u{2019}s files into fewer folders.",
                        shortened(file)
                    ),
                    _ => "This is because the full path of a file in each of them may be longer than Windows \
                          allows. If you can, move the shaders\u{2019} files into fewer folders."
                        .to_string(),
                },
            },
        }
    }
}

/// What we show on the Menu step about the shaders the author added.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShaderWarnings {
    /// The shaders the warnings are about, by their files in the selection.
    pub warned: Vec<PathBuf>,
    /// One warning for each problem on each platform, naming the platform.
    pub warnings: Vec<Warning>,
}

/// What we show on the Menu step, for every platform, because the author
/// chooses the platforms after the shaders. `library` is the shader library
/// of a runtime kit.
pub fn shader_warnings(selection: &ShaderSelection, library: &Path) -> Result<ShaderWarnings, String> {
    let mut warnings = ShaderWarnings::default();
    for platform in ExportTarget::ALL {
        for problem in problems(selection, &Destination { platform, library: library.to_path_buf() })? {
            for path in problem.paths() {
                if !warnings.warned.iter().any(|warned| warned == path) {
                    warnings.warned.push(path.to_path_buf());
                }
            }
            warnings.warnings.push(problem.warning(platform, true));
        }
    }
    Ok(warnings)
}

/// One part of the pop-up on Create app.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoticeSection {
    /// The platform, when more than one platform has a section.
    pub heading: Option<String>,
    pub warnings: Vec<Warning>,
}

/// What we tell the author on Create app about the shaders of a game for
/// `platforms`: nothing when every shader loads, one section without a
/// heading when one platform has problems, and a section under each
/// platform's heading when more do. `library` is the shader library of a
/// runtime kit.
pub fn notice(
    selection: &ShaderSelection,
    platforms: &[ExportTarget],
    library: &Path,
) -> Result<Vec<NoticeSection>, String> {
    let mut found = Vec::new();
    for &platform in platforms {
        let problems = problems(selection, &Destination { platform, library: library.to_path_buf() })?;
        if !problems.is_empty() {
            found.push((platform, problems));
        }
    }
    let headed = found.len() > 1;
    Ok(found
        .into_iter()
        .map(|(platform, problems)| NoticeSection {
            heading: headed.then(|| words(platform).heading.to_string()),
            warnings: problems.iter().map(|problem| problem.warning(platform, !headed)).collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests;
