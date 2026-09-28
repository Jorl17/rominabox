//! The unit tests of the exporter, one file for each file of the exporter,
//! and their shared parts: a request, and the launch plan we write for it.

mod app_files;
mod availability;
mod identity;
mod launch_plan;
#[cfg(target_os = "macos")]
mod slices;
mod validation;

use super::*;

fn request(splash: bool) -> ExportRequest {
    ExportRequest {
        rom: PathBuf::from("game.bin"),
        title: "Game".to_string(),
        system: "megadrive".to_string(),
        description: None,
        icon: None,
        background: None,
        show_menu: false,
        start_at_menu: false,
        theme: "native".to_string(),
        palette: "blue".to_string(),
        menu_sounds: "off".to_string(),
        controls: controls::Controls::default(),
        menu_controls: crate::builder::unstated::menu_controls(),
        firmware: Vec::new(),
        splash,
        advanced_emulator_access: false,
        intel_macs: false,
        zip: None,
        keep_playing_in_background: false,
        autosave_on_quit: false,
        menu_entries: None,
        shaders: crate::shaders::ShaderSelection::default(),
        include_achievements: false,
        output_dir: PathBuf::from("output"),
        replace: false,
        target: ExportTarget::Macos,
        runtime_kit: PathBuf::from("runtime"),
        core: None,
        core_cache: None,
    }
}

fn write_test_launcher(settings: ExportRequest) -> String {
    let directory = rominabox_scratch::Scratch::dir("rominabox-hotkey");
    let launcher = directory.join("launcher");
    write_launch_plan(
        &launcher,
        "identity",
        OsStr::new("content/game.bin"),
        &settings,
    )
    .unwrap();
    fs::read_to_string(launcher).unwrap()
}

fn embedded_runtime_config(plan: &str) -> String {
    let marker = "---config---\n";
    let start = plan
        .find(marker)
        .expect("launch plan contains the runtime config");
    plan[start + marker.len()..].to_string()
}

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}
