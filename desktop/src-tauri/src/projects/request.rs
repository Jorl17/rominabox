//! A project's settings and an export request, each made from the other.

use super::ProjectSettings;
use crate::packaging::ExportRequest;
use std::path::PathBuf;

impl From<&ExportRequest> for ProjectSettings {
    fn from(request: &ExportRequest) -> Self {
        Self {
            rom: request.rom.clone(),
            title: request.title.clone(),
            system: request.system.clone(),
            description: request.description.clone(),
            icon: request.icon.clone(),
            background: request.background.clone(),
            show_menu: request.show_menu,
            start_at_menu: request.start_at_menu,
            theme: request.theme.clone(),
            palette: request.palette.clone(),
            menu_sounds: request.menu_sounds.clone(),
            controls: request.controls.clone(),
            menu_controls: request.menu_controls.clone(),
            firmware: request.firmware.clone(),
            splash: request.splash,
            advanced_emulator_access: request.advanced_emulator_access,
            keep_playing_in_background: request.keep_playing_in_background,
            autosave_on_quit: request.autosave_on_quit,
            menu_entries: request.menu_entries.clone(),
            shaders: request.shaders.clone(),
            include_achievements: request.include_achievements,
            target: request.target.clone(),
            intel_macs: request.intel_macs,
        }
    }
}

impl ProjectSettings {
    /// Add the host-local export dependencies after someone opens a project.
    pub fn into_export_request(
        self,
        output_dir: PathBuf,
        runtime_kit: PathBuf,
        core: Option<PathBuf>,
    ) -> ExportRequest {
        ExportRequest {
            rom: self.rom,
            title: self.title,
            system: self.system,
            description: self.description,
            icon: self.icon,
            background: self.background,
            show_menu: self.show_menu,
            start_at_menu: self.start_at_menu,
            theme: self.theme,
            palette: self.palette,
            menu_sounds: self.menu_sounds,
            controls: self.controls,
            menu_controls: self.menu_controls,
            firmware: self.firmware,
            splash: self.splash,
            advanced_emulator_access: self.advanced_emulator_access,
            intel_macs: self.intel_macs,
            keep_playing_in_background: self.keep_playing_in_background,
            autosave_on_quit: self.autosave_on_quit,
            menu_entries: self.menu_entries,
            shaders: self.shaders,
            include_achievements: self.include_achievements,
            output_dir,
            replace: false,
            target: self.target,
            runtime_kit,
            core,
            core_cache: None,
        }
    }
}
