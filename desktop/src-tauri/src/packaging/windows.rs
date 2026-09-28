//! A Windows game: the packager. We write the icons and names into its
//! programs with `crate::windows_program`.

use super::app_files::{copy_file, tree_size};
use super::{kit_file, system_libraries, ErrorStage, ExportError, ExportRequest, Packager};
use crate::icons;
use crate::launch_contract::{app_file, core_file, shipped, windows_part};
use crate::target::Target;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// A Windows game, a folder named after the game. It contains the program of
/// the game, which is the launcher, next to `Resources`, the game files, and
/// `Runtime`, the player. The player is one program that requires only Windows.
#[derive(Default)]
pub(super) struct WindowsPackager {
    app: PathBuf,
    resources: PathBuf,
    player: PathBuf,
    launcher: PathBuf,
    core: PathBuf,
}

impl WindowsPackager {
    const TARGET: Target = Target::WindowsX86_64;
}

impl Packager for WindowsPackager {
    fn check_host(&self) -> Result<(), ExportError> {
        // We sign and compile nothing, so someone can make a Windows game on
        // any machine with a Windows kit.
        Ok(())
    }

    fn player_in_kit(&self) -> PathBuf {
        kit_file(Self::TARGET, "player")
    }

    fn lay_out(&mut self, app: &Path) -> Result<PathBuf, ExportError> {
        self.app = app.to_path_buf();
        self.resources = app.join(windows_part!(Resources));
        self.player = app.join(windows_part!(Player));
        let name = app.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        self.launcher = app.join(format!("{name}.exe"));
        fs::create_dir_all(&self.resources)
            .map_err(|error| ExportError::io(ErrorStage::Stage, &self.resources, error))?;
        Ok(self.resources.clone())
    }

    fn place_player(&mut self, runtime_kit: &Path) -> Result<(), ExportError> {
        copy_file(&runtime_kit.join(self.player_in_kit()), &self.player)
    }

    fn core_file(&self) -> &'static str {
        core_file!(Windows)
    }

    /// A Windows game has one target, so one core.
    fn place_core(
        &mut self,
        builds: &[(Target, PathBuf)],
        destination: &Path,
        _system_name: &str,
    ) -> Result<(), ExportError> {
        match builds {
            [(_, only)] => copy_file(only, destination),
            several => Err(ExportError::new(
                ErrorStage::Stage,
                format!("a Windows game has one core, not {}", several.len()),
            )),
        }
    }

    fn stage_dependencies(
        &mut self,
        _runtime_kit: &Path,
        core: &Path,
        _cancelled: &AtomicBool,
    ) -> Result<(), ExportError> {
        // We checked the player and the launcher when we built them, but a
        // core comes from a download, so we check it here. With a library that
        // Windows does not have, the game would fail when the core loads.
        self.core = core.to_path_buf();
        let image = fs::read(core).map_err(|error| ExportError::io(ErrorStage::Dependencies, core, error))?;
        let imported = crate::portable_executable::imports(&image).map_err(|message| {
            ExportError::new(ErrorStage::Dependencies, format!("{}: {message}", core.display()))
        })?;
        let system = system_libraries(Self::TARGET);
        let foreign: Vec<String> = imported
            .into_iter()
            .filter(|name| {
                let name = name.to_ascii_lowercase();
                !system.contains(&name) && !name.starts_with("api-ms-win-")
            })
            .collect();
        if foreign.is_empty() {
            Ok(())
        } else {
            Err(ExportError::new(
                ErrorStage::Dependencies,
                format!(
                    "The core needs libraries Windows does not have: {}",
                    foreign.join(", ")
                ),
            ))
        }
    }

    fn install_launcher(&mut self, runtime_kit: &Path) -> Result<(), ExportError> {
        // The person opens the launcher, so it has the game's name.
        copy_file(&runtime_kit.join(kit_file(Self::TARGET, "launcher")), &self.launcher)
    }

    fn describe(
        &mut self,
        request: &ExportRequest,
        _identity: &str,
        _staging: &Path,
    ) -> Result<(), ExportError> {
        let default_icon = icons::default_icon_path(&request.runtime_kit);
        let icon = request
            .icon
            .as_deref()
            .or(default_icon.as_deref())
            .map(icons::windows_icon)
            .transpose()?;
        for program in [&self.launcher, &self.player] {
            crate::windows_program::describe(program, icon.as_deref(), &request.title)?;
        }
        Ok(())
    }

    fn finishing(&self) -> Option<&'static str> {
        None
    }

    fn finish(
        &mut self,
        _request: &ExportRequest,
        _identity: &str,
        _staging: &Path,
        _cancelled: &AtomicBool,
    ) -> Result<(), ExportError> {
        Ok(())
    }

    fn runtime_bytes(&self) -> Result<u64, ExportError> {
        Ok(tree_size(&self.player)?
            + tree_size(&self.launcher)?
            + tree_size(&self.core)?
            + tree_size(&self.resources.join(app_file!(MenuAssets)))?
            + tree_size(&self.resources.join(shipped!(Autoconfig).0))?)
    }
}
