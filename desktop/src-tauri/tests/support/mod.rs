//! Real designs composed for the menu integration tests, and the windowless
//! RmlUi probe in which we lay them out.
#![allow(dead_code)]

use rominabox_desktop::{controls::Controls, shaders::ShaderSelection, themes};
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

pub fn kit(root: &Path) -> PathBuf {
    let kit = root.join("kit");
    copy_tree(
        &rominabox_desktop::repo::at("integrations/designs"),
        &kit.join("designs"),
    );
    copy_tree(
        &rominabox_desktop::repo::at("desktop/assets/controllers"),
        &kit.join("menu-assets"),
    );
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
    themes::compose_menu(
        &themes::MenuRequest {
            kit,
            design,
            palette: "blue",
            background: None,
            system,
            controls: &Controls::default(),
            show_menu: true,
            splash: false,
            include_achievements: entries
                .is_some_and(|entries| entries.iter().any(|entry| entry == "achievements")),
            menu_entries: entries,
            shaders: &ShaderSelection::default(),
            discs,
        },
        destination,
    )
    .unwrap_or_else(|error| panic!("{design}: {error}"));
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

/// The windowless RmlUi probe used by the `menu` tests. We build it with the
/// same recipe as those tests, so this test cannot link a different RmlUi.
pub fn rml_probe() -> PathBuf {
    let output = Command::new("python3")
        .current_dir(rominabox_desktop::repo::root())
        .args([
            "-c",
            "import sys; sys.path.insert(0, 'scripts'); import menu_interaction as m; m.build(); print(m.PROBE)",
        ])
        .output()
        .expect("python3 runs");
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
    let mut command = Command::new(rml_probe());
    command.arg("--document").arg(document).args(["--size", "960x600"]);
    for (x, y) in points {
        command.args(["--step", &format!("move:{x},{y}")]);
    }
    let output = command.output().unwrap();
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
        .replacen(&hidden, &format!("id=\"{panel}\" class=\"screen-panel\""), 1)
        .replacen(&plain, &format!("id=\"{panel}\""), 1);
    assert_ne!(shown, menu, "{panel} could not be shown");
    shown
}
