//! Components and their licences, as listed in a licence index. We write the
//! index of the repository with scripts/licences.py and show it in About in
//! the builder. We write the index of a kit and of a game in
//! packaging/legal.rs and show the game's index on its ABOUT screen.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The index file beside the licence entries.
pub const INDEX: &str = "index.json";

/// One component and its licence.
/// Where a component belongs: a group of the licence index, and the folder
/// of its entries in licenses/.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    /// The player and the libraries linked into it.
    Native,
    Cores,
    /// Data we ship in games, such as the controller profiles.
    Data,
    Fonts,
    Crates,
    Npm,
    Toolchains,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub group: Group,
    pub title: String,
    #[serde(default)]
    pub version: String,
    pub licence: String,
    /// The licence text, relative to the folder with the index. Empty when
    /// the game contains no text for the component.
    #[serde(default)]
    pub file: String,
    /// A copyright notice to show under the title, for ROM-in-a-Box itself.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub copyright: String,
}

/// The rows of the index in `folder`.
pub fn read_index(folder: &Path) -> Result<Vec<Row>, String> {
    let path = folder.join(INDEX);
    let text = fs::read(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&text).map_err(|error| format!("invalid {}: {error}", path.display()))
}

/// The path in `folder` of the licence text in `file`. We refuse a path that
/// is absolute or that steps out of the folder.
pub fn entry(folder: &Path, file: &str) -> Result<PathBuf, String> {
    let relative = Path::new(file);
    if file.is_empty() || !relative.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(format!("{file} is not a licence entry"));
    }
    Ok(folder.join(relative))
}

/// The licence text `file` names, read from `folder`.
pub fn text(folder: &Path, file: &str) -> Result<String, String> {
    let path = entry(folder, file)?;
    fs::read_to_string(&path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

/// The line under the ABOUT list in a game made for `target`, with the
/// location of the licence texts. In a Mac game they are in the app, and
/// in a Windows game they are in the folder into which we unpack the game.
fn texts_at(target: crate::packaging::ExportTarget) -> &'static str {
    match target {
        crate::packaging::ExportTarget::Macos => "LICENCE TEXTS: CONTENTS/RESOURCES/LEGAL/LICENSES",
        crate::packaging::ExportTarget::Windows => r"LICENCE TEXTS: %LOCALAPPDATA%\ROM-IN-A-BOX\RUNTIMES",
    }
}

/// The ABOUT screen, with one row per component with its title and its
/// licence, on the screen whose rows are the licences. None when the design
/// has no such screen or there is no component to list.
pub fn list(
    manifest: &crate::menu::Manifest,
    rows: &[Row],
    target: crate::packaging::ExportTarget,
) -> Option<crate::lists::List> {
    let screen = manifest
        .screens
        .iter()
        .find(|screen| screen.rows == Some(crate::menu::RowSource::Licences))?
        .clone();
    if rows.is_empty() {
        return None;
    }
    let items = rows
        .iter()
        .enumerate()
        .map(|(index, row)| crate::lists::ListItem {
            id: format!("{}-{index}", screen.id),
            icon: String::new(),
            title: row.title.clone(),
            detail: row.licence.clone(),
            state: String::new(),
            selected: false,
            accent: false,
            line: false,
        })
        .collect();
    Some(crate::lists::List {
        prompt: format!("{}\n{}", crate::website_name().to_uppercase(), texts_at(target)),
        screen,
        content: crate::lists::ListContent::Static(items),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{compose_menu, MenuRequest};

    fn row(title: &str, licence: &str) -> Row {
        Row { group: Group::Native, title: title.into(), version: String::new(), licence: licence.into(), file: String::new(), copyright: String::new() }
    }

    /// In every design we list the game's components on an ABOUT screen in
    /// Options, one row each, in pages like any list, and say where the
    /// licence texts are.
    #[test]
    fn every_design_lists_the_licences_on_its_about_screen() {
        let designs = crate::repo::at("integrations/designs");
        let mut names: Vec<_> = std::fs::read_dir(&designs)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir())
            .collect();
        names.sort();
        assert!(names.len() >= 3, "{names:?}");
        let rows: Vec<Row> = (0..7).map(|index| row(&format!("Part & {index}"), "MIT")).collect();
        for design in names {
            let request = MenuRequest {
                licences: rows.clone(),
                target: crate::packaging::ExportTarget::Windows,
                ..MenuRequest::new(&design, crate::repo::at("desktop/assets/controllers"))
            };
            let composed = compose_menu(&request).unwrap_or_else(|error| panic!("{}: {error}", design.display()));
            let menu = composed.text("menu.rml").unwrap();
            let cfg = composed.text("design.cfg").unwrap();
            let name = design.display();
            assert!(menu.contains("id=\"about-panel\""), "{name} has no ABOUT screen");
            assert!(menu.contains("id=\"about\""), "{name} has no ABOUT entry in Options");
            assert!(cfg.contains("screen_panel_about = \"about-panel\""), "{name}: {cfg}");
            for index in 0..7 {
                assert!(menu.contains(&format!("id=\"about-{index}\"")), "{name} lacks row {index}");
            }
            assert!(menu.contains(">Part &amp; 6<"), "{name} does not write a title as text");
            assert!(menu.contains(r"ROM-IN-A-BOX\RUNTIMES"), "{name} does not say where the texts are");
            assert!(menu.contains("WWW.ROMINABOX.APP"), "{name} does not give the web address");
            // ABOUT is the last entry of Options, after the switches too.
            // Only UNINSTALL comes after it.
            let at = |id: &str| menu.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("{name} has no {id}"));
            let about = at("about");
            let settings = crate::player_settings::declared(request.settings);
            for setting in settings.iter().filter(|setting| menu.contains(&format!("id=\"{}\"", setting.control()))) {
                assert!(at(&setting.control()) < about, "{name}: {} comes after ABOUT", setting.id);
            }
            for entry in ["controls", "hotkeys"] {
                assert!(at(entry) < about, "{name}: {entry} comes after ABOUT");
            }
            assert!(about < at("uninstall"), "{name}: UNINSTALL comes before ABOUT");
        }
    }

    /// In the repository's index, which we show in About in the builder,
    /// every row has a title and a licence, and we can open its text.
    #[test]
    fn every_row_of_the_repositorys_index_opens_its_text() {
        let folder = crate::repo::at("licenses");
        let rows = read_index(&folder).unwrap();
        assert!(rows.len() > 100, "{}", rows.len());
        for row in &rows {
            assert!(!row.title.is_empty() && !row.licence.is_empty(), "{row:?}");
            let body = text(&folder, &row.file).unwrap_or_else(|error| panic!("{error}"));
            assert!(!body.trim().is_empty(), "{} is empty", row.file);
        }
    }

    #[test]
    fn a_text_outside_the_folder_is_refused() {
        let folder = crate::repo::at("licenses");
        for file in ["../README.md", "/etc/passwd", r"C:\Windows\win.ini", "native/../../README.md", ""] {
            assert!(text(&folder, file).is_err(), "{file} was read");
        }
    }

    /// With no component to list, we add no ABOUT entry that leads nowhere.
    #[test]
    fn a_menu_with_no_licences_has_no_about_entry() {
        let design = crate::repo::at("integrations/designs/native");
        let composed = compose_menu(&MenuRequest::new(&design, crate::repo::at("desktop/assets/controllers"))).unwrap();
        let menu = composed.text("menu.rml").unwrap();
        assert!(!menu.contains("id=\"about\"") && !menu.contains("about-panel"), "{menu}");
    }
}
