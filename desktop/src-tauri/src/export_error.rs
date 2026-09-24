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

    /// The text we show the author in the builder.
    pub fn sentence(&self) -> String {
        self.to_string()
    }

    pub fn for_author(&self) -> AuthorError {
        AuthorError {
            stage: self.stage,
            sentence: self.sentence(),
        }
    }
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.message)
    }
}

impl std::error::Error for ExportError {}
