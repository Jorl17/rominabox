//! The licences in a game, and the list we show in its About views.
//!
//! A kit contains the licence files for everything that a game made from it
//! can include, and an index in `licenses/index.json`, which we write with
//! scripts/build_kit.py from the rows in scripts/licences.py. `Legal/Licenses/`
//! in a game contains the files for what that game includes, with a separate
//! index: the player and its linked libraries, the controller profiles and
//! artwork, the fonts of the menu design, and the core. We list that index on
//! the in-game ABOUT screen and in the About panel of a Mac game.

use super::{ErrorStage, ExportError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// The index file beside the entries, in a kit and in a game.
pub const INDEX: &str = "index.json";

/// One component that we ship in a game, as we list it in the About views.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// native, data, fonts or cores.
    #[serde(default)]
    pub group: String,
    pub title: String,
    #[serde(default)]
    pub version: String,
    pub licence: String,
    /// The licence text, relative to the folder with the index. Empty when
    /// the game contains no text for the component.
    #[serde(default)]
    pub file: String,
}

/// The kit's rows.
fn kit_rows(runtime_kit: &Path) -> Result<Vec<Row>, ExportError> {
    let path = runtime_kit.join("licenses").join(INDEX);
    if !path.is_file() {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!("the runtime kit has no {}; build it again with scripts/build_kit.py", path.display()),
        ));
    }
    let text = fs::read(&path).map_err(|error| ExportError::io(ErrorStage::Stage, &path, error))?;
    serde_json::from_slice(&text)
        .map_err(|error| ExportError::new(ErrorStage::Stage, format!("invalid {}: {error}", path.display())))
}

/// The rows of a game: its core, then the entries in the kit for every game
/// (the player's libraries and the controller data), then the fonts of the
/// menu design among `fonts` (their families).
pub fn game_rows(runtime_kit: &Path, core: &crate::systems::Core, fonts: &[String]) -> Result<Vec<Row>, ExportError> {
    let mut rows = vec![Row {
        group: "cores".into(),
        title: core.library_name.clone().unwrap_or_else(|| core.component.clone()),
        version: String::new(),
        licence: core.license.clone(),
        file: core.license_file.clone(),
    }];
    rows.extend(kit_rows(runtime_kit)?.into_iter().filter(|row| match row.group.as_str() {
        "native" | "data" => true,
        "fonts" => fonts.contains(&row.title),
        _ => false,
    }));
    Ok(rows)
}

/// Copy the entries in the kit for `rows` into `licenses`, and leave a row
/// without a file when the kit has no text for it.
pub fn copy_entries(runtime_kit: &Path, licenses: &Path, rows: &mut [Row]) -> Result<(), ExportError> {
    for row in rows.iter_mut().filter(|row| row.group != "cores" && !row.file.is_empty()) {
        let source = runtime_kit.join("licenses").join(&row.file);
        if !source.is_file() {
            row.file.clear();
            continue;
        }
        let destination = licenses.join(&row.file);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| ExportError::io(ErrorStage::Stage, parent, error))?;
        }
        fs::copy(&source, &destination).map_err(|error| ExportError::io(ErrorStage::Stage, &destination, error))?;
    }
    Ok(())
}

pub const README: &str = "\
The licences of the software this game is made from.

index.json lists each component with its licence and the file in this folder
that holds the licence text: the emulator core, the player and the libraries
it links (native/), the controller profiles and artwork (data/) and the fonts
of the game's menu (fonts/). NATIVE-DEPENDENCIES.txt lists the player's
components with the versions it was built from.
";

/// Write a game's index and README beside its entries.
pub fn write_index(licenses: &Path, rows: &[Row]) -> Result<(), ExportError> {
    let index = licenses.join(INDEX);
    fs::write(&index, serde_json::to_vec_pretty(rows).unwrap())
        .map_err(|error| ExportError::io(ErrorStage::Stage, &index, error))?;
    let readme = licenses.join("README.txt");
    fs::write(&readme, README).map_err(|error| ExportError::io(ErrorStage::Stage, &readme, error))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The page under the name of a Mac game in its standard About panel: every
/// component with its licence, then each licence text, which we read from the
/// game's `licenses` folder.
pub fn credits_html(licenses: &Path) -> Result<String, ExportError> {
    let index = licenses.join(INDEX);
    let text = fs::read(&index).map_err(|error| ExportError::io(ErrorStage::Stage, &index, error))?;
    let rows: Vec<Row> = serde_json::from_slice(&text)
        .map_err(|error| ExportError::new(ErrorStage::Stage, format!("invalid {}: {error}", index.display())))?;
    let mut page = String::from(
        "<html><body style=\"font-family: -apple-system, sans-serif; font-size: 11px\">\
         <p>Made with ROM-in-a-Box.</p><p><b>Licences</b></p><table>",
    );
    for row in &rows {
        page += &format!("<tr><td>{}</td><td>{}</td></tr>", escape(&row.title), escape(&row.licence));
    }
    page += "</table><p>The licence texts follow. They are also in this app, in \
             Contents/Resources/Legal/Licenses.</p>";
    for row in rows.iter().filter(|row| !row.file.is_empty()) {
        let body = fs::read_to_string(licenses.join(&row.file)).unwrap_or_default();
        page += &format!(
            "<p><b>{}</b></p><pre style=\"font-size: 10px; white-space: pre-wrap\">{}</pre>",
            escape(&row.title),
            escape(&body)
        );
    }
    page += "</body></html>\n";
    Ok(page)
}
