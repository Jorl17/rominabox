//! Real designs composed for the menu integration tests, and the windowless
//! RmlUi probe in which we lay them out.
#![allow(dead_code)]

use rominabox_desktop::{menu, themes};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

/// A kit with only the files we read when we compose a menu.
pub fn kit(root: &Path) -> PathBuf {
    let kit = root.join("kit");
    with_menu_assets(&kit);
    kit
}

/// The designs, their parts, the controller pictures and the branding from
/// which we compose a menu, staged into `kit` in the layout of a kit.
pub fn with_menu_assets(kit: &Path) {
    copy_tree(
        &rominabox_desktop::repo::at("integrations/designs"),
        &kit.join("designs"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("integrations/parts"),
        &kit.join("parts"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("desktop/assets/controllers"),
        &kit.join("menu-assets"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("desktop/assets/branding"),
        &kit.join("branding"),
    );
}

/// `kit` with the shader library from which we take libretro's presets in an
/// export, for a test that bundles them. No other test uses these 5 MB.
pub fn with_shader_library(kit: &Path) {
    copy_tree(
        &rominabox_desktop::repo::at("integrations/shaders/library"),
        &kit.join("shaders"),
    );
}

/// The hypothetical designs under `tests/fixtures/designs`, by name. They have
/// layouts that no shipped design has, and we must still compose them.
pub fn hypothetical_designs() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(fixture_designs())
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn fixture_designs() -> PathBuf {
    rominabox_desktop::repo::at("desktop/src-tauri/tests/fixtures/designs")
}

/// A kit with the hypothetical designs beside the real ones, where we look
/// for Native when we compose. A style-only fixture contains only its changes
/// to Native's stylesheet, and we ship the whole sheet in the design it stands
/// for.
pub fn kit_with_hypothetical(root: &Path) -> PathBuf {
    let kit = kit(root);
    for name in hypothetical_designs() {
        let target = kit.join("designs").join(&name);
        copy_tree(&fixture_designs().join(&name), &target);
        let changes = target.join("changes.rcss");
        if changes.is_file() {
            let mut sheet = fs::read_to_string(rominabox_desktop::repo::at(
                "integrations/designs/native/menu.rcss",
            ))
            .unwrap();
            sheet.push_str("\n/* The hypothetical design's own rules. */\n");
            sheet.push_str(&fs::read_to_string(&changes).unwrap());
            fs::write(target.join("menu.rcss"), sheet).unwrap();
            fs::remove_file(changes).unwrap();
        }
    }
    kit
}

pub struct Composed {
    pub menu: String,
    pub cfg: String,
}

pub fn compose(
    kit: &Path,
    design: &str,
    entries: Option<&[String]>,
    discs: usize,
    destination: &Path,
) -> Composed {
    compose_with(kit, design, "megadrive", entries, discs, destination)
}

/// The default menu for a game on `system`.
pub fn compose_for(kit: &Path, design: &str, system: &str, destination: &Path) -> Composed {
    compose_with(kit, design, system, None, 1, destination)
}

pub fn compose_with(
    kit: &Path,
    design: &str,
    system: &str,
    entries: Option<&[String]>,
    discs: usize,
    destination: &Path,
) -> Composed {
    let composition = menu::compose_menu(&menu::MenuRequest {
        system: system.into(),
        include_achievements: entries
            .is_some_and(|entries| entries.iter().any(|entry| entry == "achievements")),
        menu_entries: entries.map(<[String]>::to_vec),
        shaders: shaders_for(entries),
        discs,
        ..menu::MenuRequest::new(
            rominabox_desktop::themes::staged_design(kit, design),
            kit.join("menu-assets"),
        )
    })
    .unwrap_or_else(|error| panic!("{design}: {error}"));
    composition.write(destination).unwrap();
    Composed {
        menu: fs::read_to_string(destination.join("menu.rml")).unwrap(),
        cfg: fs::read_to_string(destination.join("design.cfg")).unwrap(),
    }
}

pub fn designs() -> Vec<String> {
    themes::registry()
        .unwrap()
        .designs
        .into_iter()
        .map(|design| design.id)
        .collect()
}

pub fn declared_screens(cfg: &str) -> Vec<String> {
    cfg.lines()
        .find_map(|line| line.strip_prefix("screens = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

pub use rominabox_desktop::repo::python;

/// The windowless RmlUi probe used by the `menu` tests. We build it with the
/// same recipe as those tests, so this test cannot link a different RmlUi.
pub fn rml_probe() -> PathBuf {
    // We build it, or find it up to date, once for the whole test program.
    // Each check starts Python and reads every source of the probe, and
    // nothing can change it during the run.
    static PROBE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    PROBE.get_or_init(build_rml_probe).clone()
}

fn build_rml_probe() -> PathBuf {
    let output = Command::new(python())
        .current_dir(rominabox_desktop::repo::root())
        .args([
            "-c",
            "import sys; sys.path.insert(0, 'scripts'); import menu_interaction as m; m.build(); print(m.PROBE)",
        ])
        .output()
        .expect("the test Python runs");
    assert!(
        output.status.success(),
        "could not build the RmlUi probe (python3 scripts/prepare_rmlui.py first): {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

/// What is under the pointer at each point, laid out at 960x600.
pub fn hovered(document: &Path, points: &[(i32, i32)]) -> Vec<String> {
    // In a file beside the document, because a sweep of the screen is longer
    // than the 32,767 characters allowed on a Windows command line.
    let steps = document.with_extension("steps");
    let listed: String = points.iter().map(|(x, y)| format!("move:{x},{y}\n")).collect();
    std::fs::write(&steps, listed).unwrap();
    let output = Command::new(rml_probe())
        .arg("--document")
        .arg(document)
        .args(["--size", "960x600"])
        .arg("--steps")
        .arg(&steps)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        document.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let step: serde_json::Value = serde_json::from_str(line).unwrap();
            step["hover"].as_str().unwrap_or_default().to_string()
        })
        .collect()
}

/// The position of each element laid out at `width`x`height`, as the border
/// box x, y, width, height in document pixels, or None when it is absent.
pub fn boxes(document: &Path, (width, height): (u32, u32), ids: &[&str]) -> Vec<Option<[f64; 4]>> {
    let mut command = Command::new(rml_probe());
    command
        .arg("--document")
        .arg(document)
        .args(["--size", &format!("{width}x{height}")]);
    for id in ids {
        command.args(["--step", &format!("box:{id}")]);
    }
    let output = command.args(["--step", "move:0,0"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        document.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let step: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one step of probe output");
    ids.iter()
        .map(|id| {
            step["boxes"][id].as_array().map(|values| {
                let mut found = [0.0; 4];
                for (slot, value) in found.iter_mut().zip(values) {
                    *slot = value.as_f64().unwrap();
                }
                found
            })
        })
        .collect()
}

/// The panel declared in `cfg` for the screen with the role `role`. Each
/// design file lists its own panels.
pub fn panel_with_role(cfg: &str, role: &str) -> String {
    let value = |key: &str| {
        cfg.lines()
            .find_map(|line| line.strip_prefix(&format!("{key} = \"")))
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_owned)
    };
    let screen = cfg
        .lines()
        .find_map(|line| {
            line.strip_prefix("screen_role_")
                .and_then(|rest| rest.strip_suffix(&format!(" = \"{role}\"")))
        })
        .unwrap_or_else(|| panic!("design.cfg declares no {role} screen:\n{cfg}"));
    value(&format!("screen_panel_{screen}"))
        .unwrap_or_else(|| panic!("design.cfg declares no panel for {screen}:\n{cfg}"))
}

/// `menu` with one panel shown instead of Pause.
pub fn showing(menu: &str, panel: &str) -> String {
    let hidden = format!("id=\"{panel}\" class=\"screen-panel\" style=\"display:none;\"");
    let plain = format!("id=\"{panel}\" style=\"display:none;\"");
    let shown = menu
        .replacen(
            "<div id=\"pause-panel\">",
            "<div id=\"pause-panel\" style=\"display:none;\">",
            1,
        )
        .replacen(
            &hidden,
            &format!("id=\"{panel}\" class=\"screen-panel\""),
            1,
        )
        .replacen(&plain, &format!("id=\"{panel}\""), 1);
    assert_ne!(shown, menu, "{panel} could not be shown");
    shown
}

/// The artwork every design shares, in this checkout.
pub fn artwork() -> PathBuf {
    rominabox_desktop::repo::at("desktop/assets/controllers")
}

/// The menu we compose from `design` for `system`, written to `destination`.
pub fn stage_controls(
    artwork: &Path,
    design: &Path,
    destination: &Path,
    system: &str,
    controls: &rominabox_desktop::controls::Controls,
    entries: Option<&[String]>,
) -> Result<(), String> {
    menu::compose_menu(&menu::MenuRequest {
        system: system.into(),
        controls: controls.clone(),
        include_achievements: entries
            .is_some_and(|entries| entries.iter().any(|entry| entry == "achievements")),
        menu_entries: entries.map(<[String]>::to_vec),
        shaders: shaders_for(entries),
        ..menu::MenuRequest::new(design, artwork)
    })?
    .write(destination)
}

/// The default menu we compose from `design` in `palette`, written to `destination`.
pub fn stage_theme(design: &Path, destination: &Path, palette: &str) -> Result<(), String> {
    menu::compose_menu(&menu::MenuRequest {
        palette: palette.into(),
        ..menu::MenuRequest::new(design, artwork())
    })?
    .write(destination)
}

/// The logo-only document we compose from `design` in `palette`.
pub fn stage_splash(design: &Path, destination: &Path, palette: &str) -> Result<(), String> {
    menu::compose_menu(&menu::MenuRequest {
        palette: palette.into(),
        show_menu: false,
        splash: true,
        ..menu::MenuRequest::new(design, artwork())
    })?
    .write(destination)
}

/// The player opens the list of bundled shaders from the Shaders entry, so we
/// bundle a shader in every set that lists the entry.
pub fn shaders_for(entries: Option<&[String]>) -> rominabox_desktop::shaders::ShaderSelection {
    let mut selection = rominabox_desktop::shaders::ShaderSelection::default();
    if entries.is_some_and(|entries| entries.iter().any(|entry| entry == "shaders")) {
        selection.bundled = vec!["scanlines".into()];
    }
    selection
}
