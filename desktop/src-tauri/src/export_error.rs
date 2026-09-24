//! Export failures.
//!
//! An [`ExportError`] contains the technical details: the stage, the message
//! and the file involved. We write them to the log and to the structured
//! output of the CLI. In the builder we show [`ExportError::sentence`]
//! instead, which we write here and nowhere else.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Where an export stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStage {
    /// An app is already where we would put this one, and the request did not
    /// say to replace it. We changed nothing. The path is that app.
    Exists,
    /// A file the author chose is no longer there. The path is that file.
    Missing,
    /// We refuse these settings for the export, in words for the author.
    Refused,
    /// We could not start the export at all.
    Export,
    /// We could not download a required core. The message is for the author.
    Cores,
    Validate,
    Stage,
    Image,
    Icon,
    Dependencies,
    Configure,
    Sign,
    Measure,
    Complete,
    Cleanup,
    Cancelled,
    /// Preparing a runtime-kit helper, a developer command.
    Freeze,
}

impl ErrorStage {
    fn name(self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::Missing => "missing",
            Self::Refused => "refused",
            Self::Export => "export",
            Self::Cores => "cores",
            Self::Validate => "validate",
            Self::Stage => "stage",
            Self::Image => "image",
            Self::Icon => "icon",
            Self::Dependencies => "dependencies",
            Self::Configure => "configure",
            Self::Sign => "sign",
            Self::Measure => "measure",
            Self::Complete => "complete",
            Self::Cleanup => "cleanup",
            Self::Cancelled => "cancelled",
            Self::Freeze => "freeze",
        }
    }
}

impl fmt::Display for ErrorStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub stage: ErrorStage,
    pub message: String,
    /// The file or folder the failure is about, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// What we send to the builder: the stage, so that we can offer the controls
/// we have for some failures, and the sentence to show.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorError {
    pub stage: ErrorStage,
    pub sentence: String,
    /// The app in the way, for `Exists`, so we can ask the author about it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing: Option<ExistingApp>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingApp {
    /// The app's name as the Finder shows it, without `.app`.
    pub name: String,
    /// The name of the folder it is in.
    pub folder: String,
}

impl ExportError {
    pub fn new(stage: ErrorStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
            path: None,
        }
    }

    pub(crate) fn io(stage: ErrorStage, path: &Path, error: io::Error) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            ..Self::new(stage, format!("{}: {error}", path.display()))
        }
    }

    pub(crate) fn command(stage: ErrorStage, command: &str, output: &Output) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Self::new(
            stage,
            format!("{command} failed ({}): {}", output.status, stderr.trim()),
        )
    }

    /// The same failure, about `path`.
    pub(crate) fn about(self, path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            ..self
        }
    }

    /// What we show the author in the builder, with a sentence for every stage.
    pub fn sentence(&self) -> String {
        let file = || self.path.as_deref().map(name_of).unwrap_or_default();
        let folder = || {
            self.path
                .as_deref()
                .and_then(Path::parent)
                .map(name_of)
                .unwrap_or_default()
        };
        match self.stage {
            ErrorStage::Refused | ErrorStage::Cores => self.message.clone(),
            ErrorStage::Exists => {
                format!("An app with this name already exists in {}.", folder())
            }
            ErrorStage::Missing => format!(
                "The file \u{201c}{}\u{201d} can no longer be found. Choose it again, then create the app.",
                file()
            ),
            ErrorStage::Image => format!(
                "The picture \u{201c}{}\u{201d} could not be used. Choose a different image, smaller than 32 MB.",
                file()
            ),
            ErrorStage::Icon => "The app icon could not be made from this picture. Choose a different picture and try again.".into(),
            ErrorStage::Complete => format!(
                "The app could not be saved in {}. Check that the folder can be written to and has free space, then try again.",
                folder()
            ),
            ErrorStage::Stage | ErrorStage::Configure | ErrorStage::Measure | ErrorStage::Cleanup
                if self.path.is_some() =>
            {
                "The app could not be written to its folder. Check that there is free space, then try again.".into()
            }
            ErrorStage::Dependencies | ErrorStage::Freeze => "This copy of ROM-in-a-Box is missing some of its own files. Reinstall it, then try again.".into(),
            ErrorStage::Validate => "The app could not be created with these settings. Go back, check each step, and try again.".into(),
            ErrorStage::Sign => "The app could not be signed. Try creating it again.".into(),
            ErrorStage::Cancelled => "The export was cancelled, and no app was created.".into(),
            ErrorStage::Export
            | ErrorStage::Stage
            | ErrorStage::Configure
            | ErrorStage::Measure
            | ErrorStage::Cleanup => "The app could not be created. Try again; if it keeps failing, quit ROM-in-a-Box and open it again.".into(),
        }
    }

    pub fn for_author(&self) -> AuthorError {
        let existing = match (self.stage, self.path.as_deref()) {
            (ErrorStage::Exists, Some(app)) => Some(ExistingApp {
                name: app
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                folder: app.parent().map(name_of).unwrap_or_default(),
            }),
            _ => None,
        };
        AuthorError {
            stage: self.stage,
            sentence: self.sentence(),
            existing,
        }
    }
}

/// The last part of a path, as a person would call the file or folder.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.message)
    }
}

impl std::error::Error for ExportError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn io_error(stage: ErrorStage, path: &str) -> ExportError {
        ExportError::io(
            stage,
            Path::new(path),
            io::Error::other("No space left on device"),
        )
    }

    /// It starts with a capital, ends with a full stop, and contains neither
    /// the stage's name nor the technical message.
    fn assert_sentence(error: &ExportError) -> String {
        let sentence = error.sentence();
        assert!(
            sentence.starts_with(|c: char| c.is_uppercase()),
            "{sentence:?} for {error}"
        );
        assert!(sentence.ends_with('.'), "{sentence:?} for {error}");
        assert!(
            !sentence
                .to_lowercase()
                .starts_with(&format!("{}:", error.stage)),
            "{sentence:?} for {error}"
        );
        assert!(!sentence.contains(&error.message), "{sentence:?}");
        sentence
    }

    #[test]
    fn every_failure_the_builder_shows_is_a_sentence() {
        for error in [
            ExportError::new(ErrorStage::Export, "resource not found: runtime"),
            ExportError::new(ErrorStage::Validate, "title is required"),
            ExportError::new(ErrorStage::Validate, "startAtMenu requires showMenu"),
            io_error(ErrorStage::Stage, "/out/.rominabox-export-1-0/Game.app"),
            ExportError::new(
                ErrorStage::Dependencies,
                "checksum mismatch for libz.dylib: expected a, got b",
            ),
            io_error(
                ErrorStage::Configure,
                "/out/.rominabox-export-1-0/game.json",
            ),
            ExportError::new(
                ErrorStage::Sign,
                "/usr/bin/codesign failed (exit status: 1): no identity",
            ),
            io_error(ErrorStage::Measure, "/out/.rominabox-export-1-0/Game.app"),
            io_error(ErrorStage::Cleanup, "/out/.rominabox-export-1-0"),
            ExportError::new(ErrorStage::Icon, "icns encoding failed"),
            ExportError::new(ErrorStage::Cancelled, "export cancelled"),
        ] {
            let sentence = assert_sentence(&error);
            assert!(!sentence.contains('/'), "{sentence:?} names a path");
        }
    }

    #[test]
    fn a_failure_names_the_folder_or_file_only_where_it_helps() {
        let saved = assert_sentence(&io_error(ErrorStage::Complete, "/Users/a/Games/Game.app"));
        assert!(saved.contains("Games"), "{saved:?}");
        assert!(!saved.contains("/Users"), "{saved:?}");

        let picture = assert_sentence(&io_error(ErrorStage::Image, "/pictures/cover.webp"));
        assert!(picture.contains("cover.webp"), "{picture:?}");
        assert!(!picture.contains("/pictures"), "{picture:?}");
    }

    #[test]
    fn a_core_that_could_not_be_downloaded_keeps_its_words() {
        let words = "The Dreamcast core could not be downloaded. Try again later.";
        assert_eq!(ExportError::new(ErrorStage::Cores, words).sentence(), words);
    }

    #[test]
    fn an_app_in_the_way_is_named_for_the_builder_to_ask_about() {
        let app = Path::new("/Users/a/Games/Super Mario Bros. 3.app");
        let author = ExportError::new(ErrorStage::Exists, "already exists")
            .about(app)
            .for_author();
        assert_eq!(
            author.sentence,
            "An app with this name already exists in Games."
        );
        let existing = author.existing.expect("the builder is told which app");
        assert_eq!(existing.name, "Super Mario Bros. 3");
        assert_eq!(existing.folder, "Games");
        assert!(ExportError::new(ErrorStage::Validate, "x")
            .for_author()
            .existing
            .is_none());
    }

    #[test]
    fn a_chosen_file_that_is_gone_is_named_without_its_folders() {
        let sentence = assert_sentence(
            &ExportError::new(
                ErrorStage::Missing,
                "ROM file does not exist: /games/sonic.bin",
            )
            .about(Path::new("/games/sonic.bin")),
        );
        assert!(
            sentence.contains("\u{201c}sonic.bin\u{201d}"),
            "{sentence:?}"
        );
        assert!(!sentence.contains("/games"), "{sentence:?}");
    }

    #[test]
    fn a_refusal_written_for_the_author_keeps_its_words() {
        let words = "Shaders need the in-game menu. Turn the menu on, or leave shaders unset.";
        assert_eq!(
            ExportError::new(ErrorStage::Refused, words).sentence(),
            words
        );
    }
}
