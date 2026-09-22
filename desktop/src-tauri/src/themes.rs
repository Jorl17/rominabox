//! Menu design and palette declarations shared by previews and exports.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct Design {
    pub id: String,
    pub name: String,
    pub slots: u8,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub screen: String,
    pub background: String,
    pub surface: String,
    pub picture: String,
    pub edge: String,
    pub highlight: String,
    pub muted: String,
    pub focus: String,
    /// Values for tokens that the design declares and the palette roles above
    /// do not name, such as the outer frame, the bevels and the disabled greys.
    ///
    /// Without them, the design's defaults would apply in every palette. A
    /// palette gives a value for each token the design declares, so to add a
    /// token, add a line to each palette and not a field here.
    #[serde(default)]
    pub tokens: std::collections::BTreeMap<String, String>,
}
/// One pack is one complete set of the four menu cues, `up`, `down`, `ok` and
/// `cancel`. Packs have no variants or layers. `off` is the one entry with no
/// assets, and we write it as `audio_enable_menu=false` at export.
#[derive(Debug, Deserialize, Serialize)]
pub struct SoundPack {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// The basenames of menu sounds in RetroArch. A pack must have all of them.
pub const SOUND_CUES: [&str; 4] = ["up.wav", "down.wav", "ok.wav", "cancel.wav"];
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub designs: Vec<Design>,
    pub palettes: Vec<Palette>,
    pub sound_packs: Vec<SoundPack>,
}
pub fn registry() -> Result<Registry, String> {
    serde_json::from_str(include_str!("../../designs.json")).map_err(|e| e.to_string())
}

/// The directory that contains a design's documents and fonts.
///
/// A design defines its screens as well as its colours, and another design may
/// lay out the menu in a different way (three save slots instead of six), so
/// each design is a directory and not only a name.
pub fn design_root(design: &str) -> Result<PathBuf, String> {
    let declared = registry()?
        .designs
        .into_iter()
        .find(|entry| entry.id == design)
        .ok_or_else(|| format!("Unknown menu design: {design}"))?;
    // We resolve the path against this crate and not the working directory,
    // because the builder does not run from the repository root. We check the
    // path on disk, so we fail here for a design with no directory.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../integrations/designs")
        .join(&declared.id);
    if !root.is_dir() {
        return Err(format!(
            "Menu design '{design}' is declared but its package is missing at {}",
            root.display()
        ));
    }
    Ok(root)
}

/// The files in every design, under the names declared in it.
/// The geometry of the scene, written from the declaration in the design.
///
/// We append the scene size, marker diameter, callout size and stick strip
/// here from the declaration, like the palette block, so the frame is the same
/// in the stylesheet and in `scripts/render_control_overlays.py`.
/// The frame in which a design draws its controller scene.
///
/// We read it from the design, the only source for these numbers.
/// `control_group_markup`, the stylesheet and
/// `scripts/render_control_overlays.py` all use the declaration, so a change
/// to it moves the CSS box and the generated coordinates together.
#[derive(Clone, Copy)]
pub struct SceneMetrics {
    pub scene_width: i32,
    pub scene_height: i32,
    pub callout_width: i32,
    pub callout_height: i32,
    /// The part of a callout drawn outside its declared size. The leader ends at
    /// the drawn edge, not the content edge, and we use this value in both the
    /// exporter and the builder.
    pub callout_border: i32,
    pub marker: i32,
    pub group_width: i32,
    pub group_height: i32,
    pub group_gap: i32,
    pub group_bottom_margin: i32,
}

impl Default for SceneMetrics {
    /// The values in the stylesheet, for a staged kit that contains the
    /// documents but not the declaration. Equal to `integrations/designs/native`.
    fn default() -> Self {
        Self {
            scene_width: 960,
            scene_height: 380,
            callout_width: 196,
            callout_height: 54,
            callout_border: 2,
            marker: 42,
            group_width: 236,
            group_height: 62,
            group_gap: 16,
            group_bottom_margin: 12,
        }
    }
}

/// A screen in the in-game menu, as declared in the design.
///
/// In the player we read these declarations and no fixed panel ids or heading
/// strings, so a new screen requires no change to the player and is not tied
/// to one design.
///
/// The words of the heading and the footer hint come from the design, so
/// that each design can word them differently, also in another language.
#[derive(Clone, Debug)]
pub struct Screen {
    pub id: String,
    pub panel: String,
    pub heading: String,
    pub footer: String,
    /// The button that opens this screen. A back button is the button that
    /// opens the screen behind, so we need no separate kind for it.
    pub button: String,
    /// The pause-row label, when the player opens this screen from another one.
    /// The heading is the title on the open screen. A button can be shorter.
    pub label: Option<String>,
    /// The words on the back button of this screen. We use them only on the
    /// screen that contains the option entries.
    pub back_label: Option<String>,
    pub place: ScreenPlace,
    /// Set when this screen is an entry inside Options. The words are the
    /// design's, on the button that opens it.
    pub option_label: Option<String>,
    /// Whether we ship the entry in a game without a set of entries. Shaders
    /// and the rest stay off until the author turns them on for a game.
    pub option_default: bool,
}

/// The place of a declared screen. We parse it from the design so that we do
/// not compare screen ids in the exporter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPlace {
    Plain,
    Options,
}

/// The screens for a design that declares none, the two default screens with
/// the default words of the player.
fn built_in_screens() -> Vec<Screen> {
    vec![
        Screen {
            id: "pause".into(),
            panel: "pause-panel".into(),
            heading: "GAME PAUSED".into(),
            footer: "ESC  CONTINUE".into(),
            button: "controls-back".into(),
            label: None,
            back_label: None,
            place: ScreenPlace::Plain,
            option_label: None,
            option_default: false,
        },
        Screen {
            id: "controls".into(),
            panel: "controls-panel".into(),
            heading: "CONTROLS".into(),
            footer: "ESC  BACK".into(),
            button: "controls".into(),
            label: None,
            back_label: None,
            place: ScreenPlace::Plain,
            option_label: None,
            option_default: false,
        },
    ]
}

fn screen_place(entry: &serde_json::Value, index: usize, declaration: &Path) -> Result<ScreenPlace, String> {
    match entry.get("place").and_then(|value| value.as_str()) {
        None => Ok(ScreenPlace::Plain),
        Some("options") => Ok(ScreenPlace::Options),
        Some(other) => Err(format!(
            "screen {index} in {} has place '{other}', which is not a place a menu has",
            declaration.display()
        )),
    }
}

fn screen_option(
    entry: &serde_json::Value,
    index: usize,
    declaration: &Path,
) -> Result<(Option<String>, bool), String> {
    let Some(option) = entry.get("option") else {
        return Ok((None, false));
    };
    if option.is_null() {
        return Ok((None, false));
    }
    let Some(label) = option.get("label").and_then(|value| value.as_str()) else {
        return Err(format!(
            "screen {index} in {} has an option entry with no label",
            declaration.display()
        ));
    };
    if label.is_empty() {
        return Err(format!(
            "screen {index} in {} has an empty option label",
            declaration.display()
        ));
    }
    let default = match option.get("default") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| {
            format!(
                "screen {index} in {} has an option default that is not true or false",
                declaration.display()
            )
        })?,
    };
    Ok((Some(label.to_string()), default))
}

pub fn declared_screens(design: &Path) -> Result<Vec<Screen>, String> {
    let declaration = design.join("design.json");
    let Ok(text) = fs::read_to_string(&declaration) else {
        return Ok(built_in_screens());
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", declaration.display()))?;
    let Some(listed) = declared.get("screens").and_then(|v| v.as_array()) else {
        return Ok(built_in_screens());
    };
    let mut screens = Vec::new();
    for (index, entry) in listed.iter().enumerate() {
        let at = |key: &str| -> Result<String, String> {
            entry[key]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("screen {index} in {} declares no {key}", declaration.display()))
        };
        let (option_label, option_default) = screen_option(entry, index, &declaration)?;
        screens.push(Screen {
            id: at("id")?,
            panel: at("panel")?,
            heading: at("heading")?,
            footer: at("footer")?,
            // Optional, because a screen we open only in code has no button.
            button: entry["button"].as_str().unwrap_or_default().to_string(),
            label: entry["label"].as_str().map(str::to_string),
            back_label: entry["back"].as_str().map(str::to_string),
            place: screen_place(entry, index, &declaration)?,
            option_label,
            option_default,
        });
    }
    if screens.is_empty() {
        return Ok(built_in_screens());
    }
    Ok(screens)
}

/// The screens, written to the file that the player reads.
///
/// We use the same format as for the controller list, a space-separated list
/// of ids and one key per field, read with `config_get_array`. The player
/// code contains the name of no particular screen.
///
/// `screens` is already the set for the export, the screens that the author
/// left on. We drop every screen in that set that this document cannot draw,
/// so a logo-only export lists no screens at all, instead of panels that are
/// not in the file.
fn screen_declarations(screens: &[Screen], markup: &str) -> String {
    let screens: Vec<&Screen> = screens
        .iter()
        .filter(|screen| markup.contains(&format!("id=\"{}\"", screen.panel)))
        .collect();
    let ids: Vec<&str> = screens.iter().map(|s| s.id.as_str()).collect();
    let mut text = format!("screens = \"{}\"\n", ids.join(" "));
    for screen in &screens {
        text.push_str(&format!(
            "screen_panel_{id} = \"{}\"\nscreen_heading_{id} = \"{}\"\nscreen_footer_{id} = \"{}\"\nscreen_button_{id} = \"{}\"\n",
            screen.panel,
            screen.heading,
            screen.footer,
            screen.button,
            id = screen.id,
        ));
    }
    text
}

/// Which screens we put in a game.
///
/// `chosen` is the set for the export. Without it, each entry has its
/// default. An empty set means a game with no Options button. We refuse an
/// id that is not an entry in this design, instead of ignoring it.
fn screens_for_export(screens: &[Screen], chosen: Option<&[String]>) -> Result<Vec<Screen>, String> {
    let entries: Vec<&Screen> = screens.iter().filter(|screen| screen.option_label.is_some()).collect();
    let included: BTreeSet<&str> = match chosen {
        None => entries
            .iter()
            .filter(|screen| screen.option_default)
            .map(|screen| screen.id.as_str())
            .collect(),
        Some(ids) => {
            for id in ids {
                if !entries.iter().any(|screen| screen.id == *id) {
                    return Err(format!(
                        "'{id}' is not an options entry this design declares"
                    ));
                }
            }
            ids.iter().map(String::as_str).collect()
        }
    };
    let show_options = !included.is_empty();
    let mut staged: Vec<Screen> = screens
        .iter()
        .filter(|screen| match screen.place {
            ScreenPlace::Options => show_options,
            ScreenPlace::Plain => {
                screen.option_label.is_none() || included.contains(screen.id.as_str())
            }
        })
        .cloned()
        .collect();
    if show_options && !staged.iter().any(|screen| screen.place == ScreenPlace::Options) {
        // For a design with entries and no Options screen we still add one, with
        // the built-in words. A design that declares the screen can choose the
        // words.
        staged.insert(
            1.min(staged.len()),
            Screen {
                id: "options".into(),
                panel: "options-panel".into(),
                heading: "OPTIONS".into(),
                footer: "ESC  BACK".into(),
                button: "options".into(),
                label: Some("OPTIONS".into()),
                back_label: Some("BACK".into()),
                place: ScreenPlace::Options,
                option_label: None,
                option_default: false,
            },
        );
    }
    // When the player leaves Options, the screen behind it appears, which is
    // the first screen that is not an entry. The generated back button is the
    // button of that screen, so it opens that screen, because in the player we
    // only handle "the button that shows a screen". If a design listed the
    // controls back button here, the two would become one element.
    if show_options {
        if let Some(behind) = staged
            .iter_mut()
            .find(|screen| screen.place == ScreenPlace::Plain && screen.option_label.is_none())
        {
            behind.button = "options-back".to_string();
        }
    }
    Ok(staged)
}

/// The step between option entries. RmlUi does not stack children of an
/// absolutely positioned parent, so we place each button.
const OPTION_ENTRY_STEP: usize = 60;

const OPTIONS_LAYOUT_CSS: &str = r#"
#options-entries { position: absolute; left: 276dp; top: 148dp; width: 400dp; height: 320dp; }
.option-entry { position: absolute; left: 0; width: 400dp; height: 48dp; line-height: 42dp; font-family: Silkscreen; font-size: 20dp; border-width: 3dp; text-align: center; }
.options-back { position: absolute; left: 56dp; top: 480dp; width: 160dp; height: 42dp; line-height: 38dp; font-family: Silkscreen; font-size: 18dp; border-width: 2dp; text-align: center; }
"#;

/// The four-button pause row, for a game without Options. We append it after
/// the five-button rule of the stylesheet, so it overrides that rule.
const COMPACT_PAUSE_CSS: &str = r#"
#resume { width: 260dp; }
#save { left: 284dp; width: 168dp; }
#load { left: 476dp; width: 168dp; }
#quit { left: 668dp; width: 168dp; }
"#;

fn button_bounds(document: &str, id: &str) -> Option<(usize, usize)> {
    let marker = format!("id=\"{id}\"");
    let mut from = 0;
    while let Some(found) = document[from..].find(&marker) {
        let at = from + found;
        let start = document[..at].rfind('<')?;
        let tag = &document[start..at];
        if !tag.contains("button") {
            from = at + marker.len();
            continue;
        }
        let close = document[at..].find("</button>")? + at + "</button>".len();
        return Some((start, close));
    }
    None
}

fn entry_button(design: &Path, screen: &Screen, index: usize) -> Result<String, String> {
    let label = screen.option_label.clone().unwrap_or_default();
    let top = (index * OPTION_ENTRY_STEP).to_string();
    let template_path = design.join("option-entry.rml");
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|e| format!("Could not read {}: {e}", template_path.display()))?
    } else {
        "<button class=\"menu-action option-entry\" id=\"BUTTON\" style=\"top: TOPdp;\">LABEL</button>"
            .to_string()
    };
    Ok(template
        .replace("BUTTON", &screen.button)
        .replace("TOP", &top)
        .replace("LABEL", &rml_text(&label)))
}

/// Rewrite the menu so that Options has exactly the entries for this game.
///
/// We cannot create elements in the player while it runs, so the buttons
/// must be in the document. When a design has an options panel, we add the
/// entries to it. Otherwise we add the built-in panel, with the button class
/// of the design.
fn apply_options(
    design: &Path,
    document: &str,
    chosen: Option<&[String]>,
) -> Result<(String, Vec<Screen>), String> {
    let declared = declared_screens(design)?;
    let staged = screens_for_export(&declared, chosen)?;
    let show = staged.iter().any(|screen| screen.place == ScreenPlace::Options);
    let options = staged.iter().find(|screen| screen.place == ScreenPlace::Options);
    let included: Vec<&Screen> = staged
        .iter()
        .filter(|screen| screen.option_label.is_some())
        .collect();
    // Every entry in the design, so that we take a disabled one off the pause
    // row instead of leaving it there as a separate button.
    let declared_entries: Vec<&Screen> = declared
        .iter()
        .filter(|screen| screen.option_label.is_some())
        .collect();

    let mut document = document.to_string();
    if show {
        let options = options.expect("show means an options screen is staged");
        let opener_label = options.label.clone().unwrap_or_else(|| options.heading.clone());
        let opener = format!(
            "<button class=\"menu-action\" id=\"{}\">{}</button>",
            options.button,
            rml_text(&opener_label)
        );
        let mut placed_opener = document.contains(&format!("id=\"{}\"", options.button));
        for entry in &declared_entries {
            // A button already inside the options panel is the version of that
            // entry from the design. We move a button on the pause row into the
            // panel.
            let Some((start, end)) = button_bounds(&document, &entry.button) else {
                continue;
            };
            if document[..start].contains("id=\"options-panel\"") {
                continue;
            }
            if !placed_opener && included.iter().any(|screen| screen.id == entry.id) {
                document.replace_range(start..end, &opener);
                placed_opener = true;
            } else {
                document.replace_range(start..end, "");
            }
        }
        if !placed_opener {
            if let Some(quit) = button_bounds(&document, "quit") {
                document.insert_str(quit.0, &opener);
            }
        }

        let mut entries = String::new();
        for (index, entry) in included.iter().enumerate() {
            if document.contains(&format!("id=\"{}\"", entry.button))
                && document[..document
                    .find(&format!("id=\"{}\"", entry.button))
                    .unwrap_or(0)]
                .contains("id=\"options-panel\"")
            {
                continue;
            }
            // We have removed the pause-row button at this point. Generate
            // the entry into the panel even if an element with the same id was cut.
            if document.contains(&format!("id=\"{}\"", entry.button)) {
                continue;
            }
            entries.push_str(&entry_button(design, entry, index)?);
        }

        if document.contains("<!--OPTIONS-->") {
            document = document.replace("<!--OPTIONS-->", &entries);
        } else if document.contains("id=\"options-panel\"") {
            document = document.replacen(
                "id=\"options-entries\">",
                &format!("id=\"options-entries\">{entries}"),
                1,
            );
        } else {
            let back = options.back_label.clone().unwrap_or_else(|| "BACK".into());
            let shell = format!(
                "<div id=\"options-panel\" style=\"display:none;\"><div id=\"options-entries\">{entries}</div><button class=\"menu-action options-back\" id=\"options-back\">{back}</button></div>",
                entries = entries,
                back = rml_text(&back),
            );
            let footer = "<div id=\"footer\">";
            if let Some(at) = document.find(footer) {
                document.insert_str(at, &shell);
            } else {
                return Err(
                    "menu.rml has no footer, so the options screen has nowhere to go".into(),
                );
            }
        }
    } else {
        for entry in &declared_entries {
            if let Some((start, end)) = button_bounds(&document, &entry.button) {
                if !document[..start].contains("id=\"options-panel\"") {
                    document.replace_range(start..end, "");
                }
            }
        }
    }
    Ok((document, staged))
}

fn write_options_css(destination: &Path, show_options: bool) -> Result<(), String> {
    let css_path = destination.join("menu.rcss");
    if !css_path.exists() {
        return Ok(());
    }
    let mut css = fs::read_to_string(&css_path).map_err(|e| e.to_string())?;
    if show_options && !css.contains(".option-entry") {
        css.push_str(OPTIONS_LAYOUT_CSS);
    }
    if !show_options {
        css.push_str(COMPACT_PAUSE_CSS);
    }
    fs::write(&css_path, css).map_err(|e| e.to_string())
}

/// Something from the design that we draw over the running game for a moment.
///
/// An overlay is not a screen. Nobody opens it by name, it has no input, it
/// hides nothing, and it is over the game and not inside the menu. So it has
/// no heading, footer or button, and we add no bridge action for it.
///
/// In the player we show it, mark it as leaving and hide it on this clock.
/// The stylesheet of the design styles its arrival and its exit, with the same
/// exit time as `design(overlay-leave-<id>)`.
pub struct Overlay {
    /// The element in the design's markup, which is also the overlay's name.
    pub id: String,
    /// An overlay declared before this one that has to finish first. Empty
    /// means that we wait for the start of the game instead.
    pub follows: String,
    /// How long after that before it appears.
    pub after_ms: u32,
    /// How long it stays once it has arrived.
    pub hold_ms: u32,
    /// How long the overlay takes to leave. The animation in the stylesheet
    /// lasts exactly this long, from the same declaration.
    pub leave_ms: u32,
    /// A staged file required for the overlay, which we check in the player
    /// against the files shipped in the export. Empty means none.
    pub needs: String,
}

pub fn declared_overlays(design: &Path) -> Result<Vec<Overlay>, String> {
    let declaration = design.join("design.json");
    let Ok(text) = fs::read_to_string(&declaration) else {
        return Ok(Vec::new());
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", declaration.display()))?;
    let Some(listed) = declared.get("overlays").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut overlays = Vec::new();
    for (index, entry) in listed.iter().enumerate() {
        let id = entry["id"].as_str().ok_or_else(|| {
            format!("overlay {index} in {} declares no id", declaration.display())
        })?;
        let ms = |key: &str| -> Result<u32, String> {
            entry[key]
                .as_u64()
                .map(|v| v as u32)
                .ok_or_else(|| format!("overlay '{id}' declares no {key}"))
        };
        // Only an overlay declared earlier, so that no design can make two
        // overlays wait for each other forever.
        let follows = entry["follows"].as_str().unwrap_or_default();
        if !follows.is_empty() && !overlays.iter().any(|before: &Overlay| before.id == follows) {
            return Err(format!(
                "overlay '{id}' follows '{follows}', which {} does not declare \
                 before it",
                declaration.display()
            ));
        }
        overlays.push(Overlay {
            id: id.to_string(),
            follows: follows.to_string(),
            after_ms: ms("afterMs")?,
            hold_ms: ms("holdMs")?,
            leave_ms: ms("leaveMs")?,
            needs: entry["needs"].as_str().unwrap_or_default().to_string(),
        });
    }
    Ok(overlays)
}

/// The overlays, written to the file that the player reads, and only those in
/// this document.
///
/// For a splash-only export we stage a document with a logo and nothing else.
/// Without this check it would still list every overlay in the design, and in
/// the first seconds of the game we would wait in the player to show an element
/// that is not there. We go by the markup, as we go by the number of pads for
/// the controller picker.
fn overlay_declarations(design: &Path, markup: &str) -> Result<String, String> {
    let drawn: Vec<Overlay> = declared_overlays(design)?
        .into_iter()
        .filter(|overlay| markup.contains(&format!("id=\"{}\"", overlay.id)))
        .collect();
    let ids: Vec<&str> = drawn.iter().map(|o| o.id.as_str()).collect();
    let mut text = format!("overlays = \"{}\"\n", ids.join(" "));
    for overlay in &drawn {
        text.push_str(&format!(
            "overlay_follows_{id} = \"{}\"\noverlay_after_{id} = \"{}\"\noverlay_hold_{id} = \"{}\"\noverlay_leave_{id} = \"{}\"\noverlay_needs_{id} = \"{}\"\n",
            // When the previous overlay is not in this document, we time this
            // overlay from the game start instead, so with the logo off the
            // notice does not wait for something that never plays.
            if drawn.iter().any(|before| before.id == overlay.follows) {
                overlay.follows.as_str()
            } else {
                ""
            },
            overlay.after_ms,
            overlay.hold_ms,
            overlay.leave_ms,
            overlay.needs,
            id = overlay.id,
        ));
    }
    Ok(text)
}

/// Seconds, as we write them in a stylesheet: `design(overlay-leave-notice)s`.
fn seconds(milliseconds: u32) -> String {
    let text = format!("{:.3}", milliseconds as f32 / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Everything declared in a design by name, its colours and its geometry.
///
/// A design contains `design(surface)` where a colour goes, and
/// `design(scene-width)dp` where a size goes. We take the value
/// from the chosen palette, or from the `tokens` of the design when the
/// palette does not have it, so a design may have extra colours whose names
/// are not in this code.
///
/// We substitute the values into the rules of the design. Nothing goes after
/// the stylesheet of the design, because appended rules with equal
/// specificity would override the selectors of the design, such as the
/// separate hover, keyboard focus and pressed styles of the picker.
fn design_tokens(
    design: &Path,
    palette: &Palette,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut tokens = std::collections::BTreeMap::new();
    // The values of the design first, so a palette may override any of them.
    if let Ok(text) = fs::read_to_string(design.join("design.json")) {
        let declared: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if let Some(own) = declared.get("tokens").and_then(|v| v.as_object()) {
            for (name, value) in own {
                if let Some(value) = value.as_str() {
                    tokens.insert(name.clone(), value.to_string());
                }
            }
        }
    }
    let m = scene_metrics(design)?;
    for (name, value) in [
        ("scene-width", m.scene_width),
        ("scene-height", m.scene_height),
        ("marker-diameter", m.marker),
        ("marker-radius", m.marker / 2),
        ("callout-width", m.callout_width),
        ("callout-height", m.callout_height),
        ("group-width", m.group_width),
        ("group-height", m.group_height),
    ] {
        tokens.insert(name.to_string(), value.to_string());
    }
    // The exit time of an overlay, in seconds, the unit of a stylesheet. In
    // the player we hide the element after this time, and the animation in the
    // design lasts exactly as long, from this one declaration.
    for overlay in declared_overlays(design)? {
        tokens.insert(
            format!("overlay-leave-{}", overlay.id),
            seconds(overlay.leave_ms),
        );
    }
    for (name, value) in [
        ("screen", &palette.screen),
        ("background", &palette.background),
        ("surface", &palette.surface),
        ("picture", &palette.picture),
        ("edge", &palette.edge),
        ("highlight", &palette.highlight),
        ("muted", &palette.muted),
        ("focus", &palette.focus),
    ] {
        tokens.insert(name.to_string(), value.clone());
    }
    // Last, so the values of a palette replace the defaults of the design for
    // the same names. The design defines where a colour is used, and the
    // palette defines which colour it is.
    for (name, value) in &palette.tokens {
        tokens.insert(name.clone(), value.clone());
    }
    Ok(tokens)
}

/// Put the colours into the rules of the design.
///
/// The result is the stylesheet of the design with other characters in its
/// values, with the same rules and selectors in the same order. We append
/// nothing, so nothing can override the rules of the design.
fn substitute_tokens(
    css: &str,
    tokens: &std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("design(") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "design(".len()..];
        let close = after
            .find(')')
            .ok_or_else(|| "a design( token is never closed".to_string())?;
        let name = after[..close].trim();
        let value = tokens.get(name).ok_or_else(|| {
            format!(
                "the stylesheet asks for design({name}), which the design does \
                 not declare and no palette names. Declared: {}",
                tokens.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;
        out.push_str(value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

pub fn scene_metrics(design: &Path) -> Result<SceneMetrics, String> {
    let declaration = design.join("design.json");
    let Ok(text) = fs::read_to_string(&declaration) else {
        return Ok(SceneMetrics::default());
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", declaration.display()))?;
    let Some(metrics) = declared.get("metrics") else {
        return Ok(SceneMetrics::default());
    };
    let fallback = SceneMetrics::default();
    let at = |group: &str, key: &str, default: i32| -> i32 {
        metrics[group][key].as_i64().map(|v| v as i32).unwrap_or(default)
    };
    Ok(SceneMetrics {
        scene_width: at("scene", "width", fallback.scene_width),
        scene_height: at("scene", "height", fallback.scene_height),
        callout_width: at("callout", "width", fallback.callout_width),
        callout_height: at("callout", "height", fallback.callout_height),
        callout_border: at("callout", "border", fallback.callout_border),
        marker: at("marker", "diameter", fallback.marker),
        group_width: at("group", "width", fallback.group_width),
        group_height: at("group", "height", fallback.group_height),
        group_gap: at("group", "gap", fallback.group_gap),
        group_bottom_margin: at("group", "bottomMargin", fallback.group_bottom_margin),
    })
}

/// Stage only the selected design's assets and apply the same palette/background
/// for both an offscreen preview and an exported player.
/// The folder of the staged files of a design in a prepared kit.
///
/// Each design has a separate folder in the kit, so we stage the set of files
/// for the design id.
pub fn staged_design(kit: &Path, design: &str) -> PathBuf {
    kit.join("designs").join(design)
}

/// The declarations of the design, written next to the document they describe.
///
/// We list a screen only when it is in `screens`, the screens the author left
/// on, and also in this document, as staged. The author has no switch for
/// overlays, so for them only the document counts. We write both in one place
/// because they are in one file. In the controls stage we rewrite the file once
/// the markup is final, and code that wrote only the screens would remove the
/// overlays again.
fn write_declarations(
    design: &Path,
    destination: &Path,
    screens: &[Screen],
    markup: &str,
) -> Result<(), String> {
    let text = format!(
        "{}{}",
        screen_declarations(screens, markup),
        overlay_declarations(design, markup)?
    );
    fs::write(destination.join("design.cfg"), text)
        .map_err(|e| format!("Could not write the design's declarations: {e}"))
}

pub fn prepare_theme_assets(
    source: &Path,
    destination: &Path,
    palette: &str,
    background: Option<&Path>,
) -> Result<(), String> {
    let palette = registry()?
        .palettes
        .into_iter()
        .find(|p| p.id == palette)
        .ok_or_else(|| "Choose an available colour palette.".to_string())?;
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for name in [
        "menu.rml",
        "menu.rcss",
        "Silkscreen-Regular.ttf",
        "Silkscreen-OFL.txt",
    ] {
        fs::copy(source.join(name), destination.join(name))
            .map_err(|e| format!("Could not prepare menu asset {name}: {e}"))?;
    }
    // The declarations of the design, in the file that the player reads. We
    // write them next to the stylesheet because both belong to the design. A
    // design lists its screens, and we show them by name in the player.
    // These are defaults until we apply the set of the game in the controls
    // stage. For a game with a set we overwrite them with the same function.
    let staged = screens_for_export(&declared_screens(source)?, None)?;
    let markup = fs::read_to_string(destination.join("menu.rml")).map_err(|e| e.to_string())?;
    write_declarations(source, destination, &staged, &markup)?;
    let mut css = fs::read_to_string(destination.join("menu.rcss")).map_err(|e| e.to_string())?;
    // The colours of the design, in the rules of the design. We append nothing,
    // because a palette contains values and no styles. Appended rules would
    // declare selectors of the design again and, coming later with equal
    // specificity, override them.
    css = substitute_tokens(&css, &design_tokens(source, &palette)?)?;
    if let Some(image_path) = background {
        let image = crate::icons::read_image(image_path).map_err(|e| e.to_string())?;
        image
            .resize(1920, 1200, image::imageops::FilterType::Lanczos3)
            .save_with_format(destination.join("background.png"), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        css.push_str("\n#screen { decorator: image(\"background.png\" cover); }\n");
    }
    fs::write(destination.join("menu.rcss"), css).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub assets: std::path::PathBuf,
    pub renderer: std::path::PathBuf,
    pub output_dir: std::path::PathBuf,
    pub palette: String,
    pub background: Option<std::path::PathBuf>,
    pub width: u32,
    pub height: u32,
}

/// Render with the windowless RmlUi helper. We keep the intermediate files in the
/// given output folder for inspection.
pub fn render_preview(request: &PreviewRequest) -> Result<std::path::PathBuf, String> {
    if !(320..=3840).contains(&request.width) || !(200..=2400).contains(&request.height) {
        return Err("Preview dimensions are outside the supported range.".into());
    }
    prepare_theme_assets(
        &request.assets,
        &request.output_dir,
        &request.palette,
        request.background.as_deref(),
    )?;
    prepare_controls_assets(
        &request.assets,
        // The preview assets are a design folder, so the frame is declared
        // there.
        &request.assets,
        &request.output_dir,
        "megadrive",
        &crate::controls::Controls::default(),
        None,
    )?;
    let output = request.output_dir.join("preview.png");
    let run = std::process::Command::new(&request.renderer)
        .arg(request.output_dir.join("menu.rml"))
        .arg(&output)
        .arg(request.width.to_string())
        .arg(request.height.to_string())
        .output()
        .map_err(|e| e.to_string())?;
    if !run.status.success() {
        return Err(format!(
            "Menu renderer failed: {}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(output)
}

/// Copy only the selected menu cue pack. With Off, we add no audio.
pub fn prepare_sound_assets(source: &Path, destination: &Path, pack: &str) -> Result<(), String> {
    if !registry()?.sound_packs.iter().any(|sound| sound.id == pack) {
        return Err("Choose an available menu sound pack.".into());
    }
    if pack == "off" {
        return Ok(());
    }
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for name in SOUND_CUES {
        fs::copy(source.join(pack).join(name), destination.join(name))
            .map_err(|e| e.to_string())?;
    }
    fs::copy(
        source.join("PROVENANCE.txt"),
        destination.join("PROVENANCE.txt"),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn default_menu_sounds() -> String {
    "off".into()
}

/// Copy one staged file, and reject a copy onto itself.
///
/// Copying a file onto itself empties it, so staging into the folder that we
/// read from would destroy the artwork that we are about to use.
fn stage_file(source: &Path, destination: &Path, name: &str) -> Result<(), String> {
    let from = source.join(name);
    let to = destination.join(name);
    // When both sides canonicalize to None, neither exists, and they are not
    // the same file. We report missing artwork as missing.
    let same_file = match (from.canonicalize(), to.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if from == to || same_file {
        return Err(format!(
            "refusing to stage {name} onto itself: source and destination are \
             the same directory, which would truncate the artwork"
        ));
    }
    fs::copy(&from, &to)
        .map_err(|e| format!("Could not prepare controller artwork {name}: {e}"))?;
    Ok(())
}

/// Generate the hit regions and callouts for every controller offered for
/// the console, from the same declaration as in the builder.
///
/// We stage the illustration of every offered pad, and write the scene of
/// each next to the menu as `scene-<id>.rml`. Only the chosen one goes into
/// the document. The others are there so the player can switch to them, and
/// when the player picks another pad, both the emulated device and the
/// drawing change.
pub fn prepare_controls_assets(
    source: &Path,
    design: &Path,
    destination: &Path,
    system: &str,
    controls: &crate::controls::Controls,
    entries: Option<&[String]>,
) -> Result<(), String> {
    // We take the frame from the design, so a change of the scene position in
    // design.json moves the box in the stylesheet and every generated
    // coordinate together.
    let metrics = scene_metrics(design)?;
    let profile = crate::controls::validate_for_system(system, controls)?;
    let offered = carried(system, &profile)?;
    let mut staged: Vec<&str> = Vec::new();
    for image in offered
        .iter()
        .map(|entry| entry.image.as_str())
        .filter(|image| !image.is_empty())
    {
        if staged.contains(&image) {
            // Two variants can share a drawing: a PlayStation Dual Analog is a
            // DualShock without the vibration.
            continue;
        }
        stage_file(source, destination, image)?;
        staged.push(image);
    }
    if !profile.image.is_empty() {
        stage_file(source, destination, "CONTROLLERS.txt")?;
    }
    // We write the scene of every offered pad next to the menu. In the player
    // we switch pads by loading one of these files, because we cannot
    // generate markup there.
    for entry in &offered {
        fs::write(
            destination.join(format!("{}{}.rml", SCENE_PREFIX, entry.id)),
            scene_markup(entry, controls, metrics),
        )
        .map_err(|e| format!("Could not write the scene for {}: {e}", entry.id))?;
    }
    let markup = scene_markup(&profile, controls, metrics);
    let picker = controller_picker_markup(&offered, &profile.id);

    // The document is the menu.rml of the design. If we filled the controls
    // from any other copy, we would lose changes made to the design.
    let template = fs::read_to_string(design.join("menu.rml")).map_err(|e| e.to_string())?;
    // The picker is a sibling of the scene, not a child of it. Inside the
    // scene its coordinates would be scene coordinates, and the scene starts
    // 80 dp down the screen, so the picker would cover the first two
    // callouts. We reject a design without a marker for it, so that no game
    // is exported without a way to change controller.
    if !picker.is_empty() && !template.contains(PICKER_SLOT) {
        return Err(format!(
            "this design has no {PICKER_SLOT} for the controller picker, and \
             {} offers more than one controller. Add the slot to menu.rml, \
             outside #controller-scene.",
            system
        ));
    }
    let menu = template
        .replace("<!--CONTROLS-->", &markup)
        .replace(PICKER_SLOT, &picker);
    let (menu, screens) = apply_options(design, &menu, entries)?;
    // The same file as in prepare_theme_assets. This version replaces it,
    // because here we know the entries chosen for the game and we just built
    // the markup from them.
    write_declarations(design, destination, &screens, &menu)?;
    fs::write(destination.join("menu.rml"), menu).map_err(|e| e.to_string())?;
    let show_options = screens.iter().any(|screen| screen.place == ScreenPlace::Options);
    write_options_css(destination, show_options)
}

/// The pads in an export: every pad in the picker.
///
/// We use this one rule for the artwork staging, the scene files and the
/// picker, because they have to agree. We do not copy a console illustration
/// for a pad that is not in the picker.
///
/// With fewer than two there is no picker, and we export only the chosen pad.
fn carried(
    system: &str,
    profile: &crate::controls::ControlProfile,
) -> Result<Vec<crate::controls::ControlProfile>, String> {
    let offered = crate::controls::variants_for_system(system)?;
    let swappable = offered.len() > 1 && offered.iter().any(|entry| entry.id == profile.id);
    Ok(if swappable {
        offered
    } else {
        vec![profile.clone()]
    })
}

/// The marker for the controller picker in a design. It is separate from the
/// marker for the scene, because the picker is not part of the scene.
const PICKER_SLOT: &str = "<!--CONTROLLER-PICKER-->";

/// The scene of each offered pad, next to the menu, in a file named after the
/// pad id. We load one in the player when someone picks another controller.
pub const SCENE_PREFIX: &str = "scene-";

/// One controller's scene: its illustration, hit regions, callouts and groups.
///
/// We write it once for each offered pad, because a player who picks another
/// pad needs its scene, and we cannot generate markup in the game.
fn scene_markup(
    profile: &crate::controls::ControlProfile,
    controls: &crate::controls::Controls,
    metrics: SceneMetrics,
) -> String {
    let illustrated = !profile.image.is_empty();
    let mut markup = if illustrated {
        format!("<img id=\"controller-image\" src=\"{}\"/>", profile.image)
    } else {
        String::new()
    };
    // We draw a group once, as one object on the pad, not one callout per
    // bind. Otherwise each of the eight analogue directions of the PlayStation
    // DualShock would need a callout, and both gutters are already full with
    // seven 54 dp callouts.
    let grouped: Vec<&crate::controls::ControlDefinition> = profile
        .controls
        .iter()
        .filter(|item| item.group.is_some())
        .collect();
    let mut group_names: Vec<&str> = grouped
        .iter()
        .filter_map(|item| item.group.as_deref())
        .collect();
    group_names.sort_unstable();
    group_names.dedup();
    markup.push_str(&control_group_markup(&group_names, &grouped, controls, illustrated, metrics));
    let placed_scene = crate::scene_layout::layout(&profile.controls, metrics);
    let placements = &placed_scene.controls;
    for item in profile.controls.iter().filter(|item| item.group.is_none()) {
        let item = item.clone();
        let custom = controls.bindings.get(&item.id);
        let author_label = custom
            .and_then(|value| value.label.as_deref())
            .filter(|label| !label.trim().is_empty());
        let label = author_label.unwrap_or(&item.label);
        let key = custom
            .and_then(|value| value.key.as_deref())
            .unwrap_or(&item.key);
        let original = author_label
            .filter(|value| value.trim() != item.label.trim())
            .map(|_| item.label.as_str());
        let id = item.id.as_str();
        let cx = item.callout_x;
        let cy = item.callout_y;
        if illustrated {
            // We take the placement from the shared scene layout, so these
            // are in the same place in the exporter, the builder and the
            // overlay renderer.
            let placed = placements
                .iter()
                .find(|placement| placement.id == item.id)
                .expect("every drawn control is placed");
            for run in &placed.leader {
                let orientation = if run.height == 0 { "horizontal" } else { "vertical" };
                let extent = if run.height == 0 {
                    format!("width:{}dp;", run.width)
                } else {
                    format!("height:{}dp;", run.height)
                };
                markup.push_str(&format!(
                    "\n<div class=\"control-leader {orientation}\" style=\"left:{}dp;top:{}dp;{extent}\"/>",
                    run.x, run.y
                ));
            }
            markup.push_str(&format!(
                "\n<button id=\"control-hit-{id}\" class=\"control-hit\" style=\"left:{}dp;top:{}dp;\"/>\n",
                placed.marker.x, placed.marker.y
            ));
        }
        markup.push_str(&control_callout_markup(id, label, original, key, cx, cy));
    }
    markup
}


/// The in-game controller picker.
///
/// We draw it at the place set in the design: the markup has no coordinates,
/// and we place it with `menu.rcss`. In the native design it is on the action
/// row beside BACK and RESET DEFAULTS, the one band that no console's pad
/// covers. Both gutters are full of callouts, the heading is at the top
/// centre, and the scene fills everything between.
///
/// We emit it only when there is a choice. A dropdown with one option is
/// noise, and most consoles have exactly one pad.
fn controller_picker_markup(offered: &[crate::controls::ControlProfile], chosen: &str) -> String {
    if offered.len() < 2 {
        return String::new();
    }
    let chosen_name = offered
        .iter()
        .find(|entry| entry.id == chosen)
        .map(|entry| rml_text(&entry.name.to_uppercase()))
        .unwrap_or_default();
    let mut markup = format!(
        r#"
<div id="controls-device" class="control-picker">
<div id="controls-device-label" class="control-picker-label">CONTROLLER</div>
<button id="controls-device-current" class="control-picker-current">{chosen_name}</button>
<div id="controls-device-list" class="control-picker-list" style="display:none;">
"#,
    );
    for entry in offered {
        let selected = if entry.id == chosen { " selected" } else { "" };
        markup.push_str(&format!(
            r#"<button id="controls-device-option-{}" class="control-picker-option{selected}">{}</button>
"#,
            entry.id,
            rml_text(&entry.name.to_uppercase()),
        ));
    }
    markup.push_str("</div>
</div>
");
    markup
}

/// Draw each group once, below the illustration.
///
/// We put the strip at the bottom of the scene because the side margins are
/// full, with seven 54 dp callouts filling 378 of 380 dp. We take its geometry
/// from the declaration in the design, so the layout is the same in
/// `scripts/render_control_overlays.py` and in this markup.
#[allow(non_snake_case)]
fn control_group_markup(
    names: &[&str],
    grouped: &[&crate::controls::ControlDefinition],
    controls: &crate::controls::Controls,
    illustrated: bool,
    metrics: SceneMetrics,
) -> String {
    let (WIDTH, HEIGHT, GAP, SCENE_WIDTH, SCENE_HEIGHT) = (
        metrics.group_width,
        metrics.group_height,
        metrics.group_gap,
        metrics.scene_width,
        metrics.scene_height,
    );

    if names.is_empty() {
        return String::new();
    }
    let count = names.len() as i32;
    let total = count * WIDTH + (count - 1) * GAP;
    let left_edge = (SCENE_WIDTH - total) / 2;
    let top = SCENE_HEIGHT - HEIGHT - metrics.group_bottom_margin;

    let mut markup = String::new();
    for (index, name) in names.iter().enumerate() {
        let members: Vec<&&crate::controls::ControlDefinition> = grouped
            .iter()
            .filter(|item| item.group.as_deref() == Some(*name))
            .collect();
        let box_x = left_edge + index as i32 * (WIDTH + GAP);

        // One member has the anchor for the whole group. We reject a group in
        // the catalog without exactly one, so a missing anchor here would be a
        // defect in the generated data, which we must not hide.
        if illustrated {
            if let Some(anchor) = members.iter().find(|item| item.x != 0 || item.y != 0) {
                let centre = box_x + WIDTH / 2;
                markup.push_str(&format!(
                    r#"
<div class="control-leader vertical" style="left:{}dp;top:{}dp;height:{}dp;"/>
<div class="control-leader horizontal" style="left:{}dp;top:{top}dp;width:{}dp;"/>
<button id="control-hit-{}" class="control-hit" style="left:{}dp;top:{}dp;"/>
"#,
                    anchor.x,
                    anchor.y.min(top),
                    (top - anchor.y).abs(),
                    centre.min(anchor.x),
                    (centre - anchor.x).abs(),
                    anchor.id,
                    anchor.x - 22,
                    anchor.y - 22,
                ));
            }
        }

        // The directions are written on one line.
        let keys: Vec<String> = members
            .iter()
            .filter(|item| item.id.ends_with("_plus") || item.id.ends_with("_minus"))
            .map(|item| {
                controls
                    .bindings
                    .get(&item.id)
                    .and_then(|value| value.key.clone())
                    .unwrap_or_else(|| item.key.clone())
                    .to_uppercase()
            })
            .collect();
        let title = name.replace('_', " ").to_uppercase();
        markup.push_str(&format!(
            r#"
<button id="control-group-{name}" class="control-group" style="left:{box_x}dp;top:{top}dp;">
<div class="control-label">{}</div>
<div class="control-assignment">{}</div>
</button>
"#,
            rml_text(&title),
            rml_text(&keys.join(" ")),
        ));
    }
    markup
}

fn control_callout_markup(
    id: &str,
    label: &str,
    original: Option<&str>,
    key: &str,
    cx: i32,
    cy: i32,
) -> String {
    let label = rml_text(label);
    let key = rml_text(key);
    let original = original
        .map(|value| {
            format!(
                r#"<span class="control-original">{}</span>"#,
                rml_text(value)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<button id="control-{id}" class="control-callout" style="left:{cx}dp;top:{cy}dp;"><div id="control-label-{id}" class="control-label">{label}</div><div class="control-assignment">{original}<span id="control-binding-{id}">{key}</span></div></button>"#
    )
}

fn rml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Stage the logo-only document without pause controls, controller art or backgrounds.
///
/// We still apply the palette here. A stylesheet copied unchanged would
/// contain values such as `design(background)`, which RmlUi cannot parse, so
/// every rule that uses one would be lost.
pub fn prepare_splash_assets(
    source: &Path,
    destination: &Path,
    palette: &str,
) -> Result<(), String> {
    let palette = registry()?
        .palettes
        .into_iter()
        .find(|p| p.id == palette)
        .ok_or_else(|| "Choose an available colour palette.".to_string())?;
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for (from, to) in [
        ("splash.rml", "menu.rml"),
        ("menu.rcss", "menu.rcss"),
        ("Silkscreen-Regular.ttf", "Silkscreen-Regular.ttf"),
        ("Silkscreen-OFL.txt", "Silkscreen-OFL.txt"),
    ] {
        fs::copy(source.join(from), destination.join(to))
            .map_err(|e| format!("Could not prepare splash asset {from}: {e}"))?;
    }
    let staged = screens_for_export(&declared_screens(source)?, None)?;
    let markup = fs::read_to_string(destination.join("menu.rml")).map_err(|e| e.to_string())?;
    write_declarations(source, destination, &staged, &markup)?;
    let css = fs::read_to_string(destination.join("menu.rcss")).map_err(|e| e.to_string())?;
    let css = substitute_tokens(&css, &design_tokens(source, &palette)?)?;
    fs::write(destination.join("menu.rcss"), css).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn sound_source() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/menu-sounds")
    }

    /// The stylesheet uses seconds, the declaration uses milliseconds, and
    /// RmlUi parses either number without an error. `500s` would be a fade
    /// that never seems to end, and only the game would look wrong.
    #[test]
    fn a_leaving_time_reaches_the_stylesheet_in_seconds() {
        assert_eq!(seconds(500), "0.5");
        assert_eq!(seconds(250), "0.25");
        assert_eq!(seconds(1000), "1");
        assert_eq!(seconds(0), "0");
        assert_eq!(seconds(120), "0.12");
    }

    /// A pack is one complete set that we can play. We declare no partial
    /// pack, and ship no assets that the author cannot pick.
    #[test]
    fn every_declared_sound_pack_is_one_complete_cue_set() {
        let declared: BTreeSet<String> = registry()
            .unwrap()
            .sound_packs
            .into_iter()
            .map(|pack| pack.id)
            .filter(|id| id != "off")
            .collect();
        assert!(!declared.is_empty(), "no menu sound packs are declared");

        let source = sound_source();
        let mut present = BTreeSet::new();
        for entry in fs::read_dir(&source).expect("menu sound assets") {
            let entry = entry.expect("menu sound entry");
            if entry.file_type().expect("file type").is_dir() {
                present.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
        assert_eq!(
            declared, present,
            "declared packs and shipped pack directories must match exactly"
        );

        for id in &declared {
            for cue in SOUND_CUES {
                let path = source.join(id).join(cue);
                let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert!(bytes.len() > 44, "{} is not a usable WAV", path.display());
                assert_eq!(&bytes[0..4], b"RIFF", "{} is not RIFF", path.display());
                assert_eq!(&bytes[8..12], b"WAVE", "{} is not WAVE", path.display());
                // 44100 Hz, 16-bit, mono, the format we give the RetroArch mixer.
                assert_eq!(
                    u16::from_le_bytes([bytes[22], bytes[23]]),
                    1,
                    "{} is not mono",
                    path.display()
                );
                assert_eq!(
                    u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
                    44_100,
                    "{} is not 44100 Hz",
                    path.display()
                );
                assert_eq!(
                    u16::from_le_bytes([bytes[34], bytes[35]]),
                    16,
                    "{} is not 16-bit",
                    path.display()
                );
            }
        }
    }

    /// In the picker we show a name and a description for every pack, `off` included.
    #[test]
    fn every_sound_pack_is_described_for_the_picker() {
        for pack in registry().unwrap().sound_packs {
            assert!(!pack.name.trim().is_empty(), "{} has no name", pack.id);
            assert!(
                !pack.description.trim().is_empty(),
                "{} has no description",
                pack.id
            );
            assert!(
                !pack.id.contains("--"),
                "{} keeps an authoring variant separator",
                pack.id
            );
        }
    }

    #[test]
    fn unknown_sound_packs_are_rejected_before_staging() {
        let temporary =
            std::env::temp_dir().join(format!("rominabox-sound-pack-{}", std::process::id()));
        let error = prepare_sound_assets(&sound_source(), &temporary, "pulse")
            .expect_err("retired pack must not stage");
        assert!(error.contains("available menu sound pack"), "{error}");
        assert!(!temporary.exists(), "rejection must not create output");
    }

    fn native_menu() -> (PathBuf, String) {
        let design = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../integrations/designs/native");
        let menu = fs::read_to_string(design.join("menu.rml")).expect("native menu");
        (design, menu)
    }

    /// We move the Controls button from the pause row into Options. A game with
    /// nothing enabled has no button that opens an empty screen.
    #[test]
    fn options_lists_only_the_entries_a_game_enables() {
        let (design, menu) = native_menu();
        let (staged, screens) = apply_options(&design, &menu, None).expect("defaults");
        let panel = staged.find("id=\"options-panel\"").expect("options panel");
        let controls = staged.find("id=\"controls\"").expect("controls entry");
        let actions = staged.find("id=\"actions\"").expect("pause actions");
        assert!(actions < panel && panel < controls, "controls sits inside options");
        assert!(staged.contains("id=\"options\""));
        assert!(staged.contains(">OPTIONS<"));
        assert!(staged.contains(">CONTROLS<"));
        assert!(staged.contains(">BACK<"));
        assert_eq!(
            screens.iter().find(|screen| screen.id == "pause").unwrap().button,
            "options-back"
        );
        let cfg = screen_declarations(&screens, &staged);
        assert!(cfg.contains("screens = \"pause options controls\"") || cfg.contains("options"));
        assert!(cfg.contains("screen_button_pause = \"options-back\""));
        assert!(cfg.contains("screen_button_options = \"options\""));
        assert!(cfg.contains("screen_button_controls = \"controls\""));

        let (empty, empty_screens) = apply_options(&design, &menu, Some(&[])).expect("nothing enabled");
        assert!(!empty.contains("id=\"options\""), "no options button when nothing is enabled");
        assert!(!empty.contains("id=\"options-panel\""));
        assert!(button_bounds(&empty, "controls").is_none(), "controls is not left on the pause row");
        assert!(!empty_screens.iter().any(|screen| screen.place == ScreenPlace::Options));
    }

    /// Shaders is an entry that a design can declare. It is absent until the
    /// author turns it on, and we reject an unknown id and do not drop it.
    #[test]
    fn an_entry_appears_only_when_that_game_enables_it() {
        let root = std::env::temp_dir().join(format!(
            "rominabox-options-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("menu.rml"),
            "<rml><body><div id=\"screen\"><div id=\"actions\"><button class=\"menu-action\" id=\"resume\">CONTINUE</button><button class=\"menu-action\" id=\"quit\">QUIT</button></div><div id=\"footer\"></div></div></body></rml>",
        )
        .unwrap();
        fs::write(
            root.join("design.json"),
            r#"{
                "screens": [
                    {"id":"pause","panel":"pause-panel","heading":"PAUSED","footer":"ESC  CONTINUE","button":"controls-back"},
                    {"id":"controls","panel":"controls-panel","heading":"CONTROLS","footer":"ESC  BACK","button":"controls","option":{"label":"CONTROLS","default":true}},
                    {"id":"shaders","panel":"shaders-panel","heading":"SHADERS","footer":"ESC  BACK","button":"shaders","option":{"label":"SHADERS","default":false}}
                ]
            }"#,
        )
        .unwrap();
        let menu = fs::read_to_string(root.join("menu.rml")).unwrap();
        // The design declares no options screen. We still add one, and leave
        // shaders out because it is not enabled for this game.
        let (defaults, screens) = apply_options(&root, &menu, None).unwrap();
        assert!(defaults.contains(">OPTIONS<"), "a design that declares no options screen still gets one");
        assert!(defaults.contains(">CONTROLS<"));
        assert!(!defaults.contains("SHADERS"));
        assert!(screens.iter().any(|screen| screen.place == ScreenPlace::Options));

        let (both, _) = apply_options(
            &root,
            &menu,
            Some(&["controls".to_string(), "shaders".to_string()]),
        )
        .unwrap();
        assert!(both.contains(">SHADERS<"));
        assert!(both.contains(">CONTROLS<"));
        let shaders_at = both.find("id=\"shaders\"").unwrap();
        let controls_at = both.find("id=\"controls\"").unwrap();
        assert!(controls_at < shaders_at, "entries follow the design's order");

        let refused = apply_options(&root, &menu, Some(&["nope".to_string()])).unwrap_err();
        assert!(refused.contains("nope"), "{refused}");

        fs::write(
            root.join("option-entry.rml"),
            "<div id=\"BUTTON\" class=\"list-row\">LABEL</div>",
        )
        .unwrap();
        let (templated, _) = apply_options(&root, &menu, Some(&["shaders".to_string()])).unwrap();
        assert!(
            templated.contains("<div id=\"shaders\" class=\"list-row\">SHADERS</div>"),
            "a design's entry template is what gets filled, got {templated}"
        );
        assert!(!templated.contains(">CONTROLS<"), "controls was not in the set");
    }

    /// The person who bundles the game picks the BIOS. The player never does.
    ///
    /// We put no BIOS picker and no BIOS uploader in the exported game. A
    /// player who wants a different BIOS goes through Advanced, which unlocks
    /// the whole emulator.
    ///
    /// Everything about the BIOS is in the builder (`assess_firmware`, the
    /// details step, the export refusal), and the player sees none of it. A
    /// design may not contain a screen, a button or a declaration that offers a
    /// BIOS choice. We still bundle a BIOS, with no way to change it in the menu.
    #[test]
    fn no_design_offers_the_player_a_bios() {
        let designs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/designs");
        let mut looked = 0;
        for entry in fs::read_dir(&designs).expect("designs directory") {
            let design = entry.expect("design entry").path();
            if !design.is_dir() {
                continue;
            }
            for file in fs::read_dir(&design).expect("design files") {
                let file = file.expect("design file").path();
                let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.ends_with(".rml") && !name.ends_with(".rcss") && !name.ends_with(".json") {
                    continue;
                }
                let body = fs::read_to_string(&file).unwrap_or_default();
                looked += 1;
                for (number, line) in body.lines().enumerate() {
                    assert!(
                        !line.to_ascii_lowercase().contains("bios"),
                        "{}:{} offers the player a BIOS: {}\n\
                         The BIOS is chosen by whoever bundles the game. A player \
                         who wants another one uses Advanced.",
                        file.display(),
                        number + 1,
                        line.trim()
                    );
                }
            }
        }
        assert!(looked > 0, "no design files were read, so this proved nothing");
    }
}
