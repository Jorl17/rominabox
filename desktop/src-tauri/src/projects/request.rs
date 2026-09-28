//! A project's game, exported.

use crate::game::Game;
use crate::packaging::ExportRequest;
use std::path::PathBuf;

impl Game {
    /// An export of this game from `runtime_kit` into `output_dir`, with the
    /// host's own cores, that replaces nothing.
    pub fn into_export_request(
        self,
        output_dir: PathBuf,
        runtime_kit: PathBuf,
        core: Option<PathBuf>,
    ) -> ExportRequest {
        ExportRequest {
            game: self,
            zip: None,
            output_dir,
            replace: false,
            runtime_kit,
            core,
            core_cache: None,
        }
    }
}
