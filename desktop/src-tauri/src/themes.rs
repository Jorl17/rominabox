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
    let root = crate::repo::at("integrations/designs").join(&declared.id);
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
    /// Optional capacity for screens with content above their list.
    pub list_page_size: Option<usize>,
    pub place: ScreenPlace,
    /// Set when this screen is an entry inside Options. The words are the
    /// design's, on the button that opens it.
    pub option_label: Option<String>,
    /// Whether we ship the entry in a game without a set of entries. Shaders
    /// and the rest stay off until the author turns them on for a game.
    pub option_default: bool,
    /// What happens on this screen once the core has loaded more than one
    /// disc.
    ///
    /// `list` means the disc list, and any other value is the screen to open
    /// instead of this one. Without a value, the screen is unrelated to discs.
    /// We read the number of discs from the core after loading.
    pub images: Option<String>,
    /// The word on the row of the disc in the tray. We read it from the design
    /// in the player and keep no second copy of it.
    pub mark: Option<String>,
    /// A switch on this screen, with the words from the design and the value
    /// it has while it is on.
    pub toggle: Option<Toggle>,
}

/// An optional switch declared by a screen.
///
/// Every word on it comes from the design. In the player we act only on
/// `guard`, which is a closed set, so a design can choose only an effect that
/// we implement. We reject any other value here, so that it is not ignored
/// when the player presses the switch.
#[derive(Clone, Debug)]
pub struct Toggle {
    pub id: String,
    pub label: String,
    pub on: String,
    pub off: String,
    pub default_on: bool,
    pub guard: ToggleGuard,
    pub guard_label: String,
    pub guard_status: String,
}

impl Screen {
    /// The list we fill with the discs from the core once the game has loaded.
    pub fn is_disc_list(&self) -> bool {
        self.images.as_deref() == Some("list")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleGuard {
    Nothing,
    Saves,
}

impl ToggleGuard {
    fn declared(self) -> &'static str {
        match self {
            ToggleGuard::Nothing => "",
            ToggleGuard::Saves => "saves",
        }
    }
}

/// The place of a declared screen. We parse it from the design so that we do
/// not compare screen ids in the exporter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPlace {
    Plain,
    Options,
}

/// The base must come from the same source tree or frozen kit as the design.
/// With a fallback to a path in the repository, an export could mix
/// different versions of the menu without anyone noticing.
pub(crate) fn base_design(design: &Path) -> Result<PathBuf, String> {
    let base = if design.file_name().is_some_and(|name| name == "native") {
        design.to_path_buf()
    } else {
        design
            .parent()
            .ok_or_else(|| format!("Design has no package parent: {}", design.display()))?
            .join("native")
    };
    if !base.is_dir() {
        return Err(format!(
            "Native base design is missing beside {}: {}",
            design.display(),
            base.display()
        ));
    }
    Ok(base)
}

fn screen_place(
    entry: &serde_json::Value,
    index: usize,
    declaration: &Path,
) -> Result<ScreenPlace, String> {
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

fn screen_toggle(
    entry: &serde_json::Value,
    index: usize,
    declaration: &Path,
) -> Result<Option<Toggle>, String> {
    let Some(declared) = entry.get("toggle") else {
        return Ok(None);
    };
    if declared.is_null() {
        return Ok(None);
    }
    let word = |key: &str| -> Result<String, String> {
        declared
            .get(key)
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "the switch on screen {index} in {} declares no {key}",
                    declaration.display()
                )
            })
    };
    let guard = match declared.get("guard").and_then(|value| value.as_str()) {
        None | Some("") => ToggleGuard::Nothing,
        Some("saves") => ToggleGuard::Saves,
        Some(other) => {
            return Err(format!(
                "the switch on screen {index} in {} guards '{other}', which is not \
                 something a player can hold",
                declaration.display()
            ))
        }
    };
    let optional = |key: &str| -> String {
        declared
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Ok(Some(Toggle {
        id: word("id")?,
        label: word("label")?,
        on: word("on")?,
        off: word("off")?,
        default_on: declared
            .get("default")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        guard,
        guard_label: optional("guardLabel"),
        guard_status: optional("guardStatus"),
    }))
}

fn screen_entries(design: &Path) -> Result<(Vec<serde_json::Value>, Vec<String>), String> {
    let declaration = design.join("design.json");
    let text = fs::read_to_string(&declaration)
        .map_err(|e| format!("Could not read {}: {e}", declaration.display()))?;
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", declaration.display()))?;
    let screens = match declared.get("screens") {
        None => Vec::new(),
        Some(screens) => screens
            .as_array()
            .cloned()
            .ok_or_else(|| format!("Screens in {} must be an array", declaration.display()))?,
    };
    let order = match declared.get("screenOrder") {
        None => Vec::new(),
        Some(order) => order
            .as_array()
            .ok_or_else(|| format!("screenOrder in {} must be an array", declaration.display()))?
            .iter()
            .map(|id| {
                id.as_str().map(str::to_string).ok_or_else(|| {
                    format!(
                        "screenOrder in {} must contain screen ids",
                        declaration.display()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok((screens, order))
}

/// Every base screen comes from Native. A design may replace the fields of a
/// screen by id or add a screen, and for any other id we keep Native's.
pub fn declared_screens(design: &Path) -> Result<Vec<Screen>, String> {
    let base = base_design(design)?;
    let (base_entries, _) = screen_entries(&base)?;
    if base_entries.is_empty() {
        return Err(format!("Native declares no screens: {}", base.display()));
    }
    let (selected, order) = if design == base {
        (Vec::new(), Vec::new())
    } else {
        screen_entries(design)?
    };
    let mut overrides = Vec::new();
    let mut used = BTreeSet::new();
    for override_entry in selected {
        let id = override_entry["id"]
            .as_str()
            .ok_or_else(|| format!("Screen override in {} declares no id", design.display()))?;
        if !used.insert(id.to_string()) {
            return Err(format!(
                "Duplicate screen override '{id}' in {}",
                design.display()
            ));
        }
        let fields = override_entry.as_object().ok_or_else(|| {
            format!(
                "Screen override '{id}' in {} is not an object",
                design.display()
            )
        })?;
        overrides.push((id.to_string(), fields.clone()));
    }
    let mut listed = Vec::new();
    for entry in base_entries {
        let id = entry["id"]
            .as_str()
            .ok_or_else(|| format!("Native screen in {} declares no id", base.display()))?;
        let mut merged = entry.clone();
        if let Some((_, fields)) = overrides.iter().find(|(selected_id, _)| selected_id == id) {
            merged
                .as_object_mut()
                .expect("base screen is an object")
                .extend(fields.clone());
        }
        listed.push(merged);
    }
    for (id, fields) in &overrides {
        if !listed.iter().any(|entry| entry["id"] == *id) {
            listed.push(serde_json::Value::Object(fields.clone()));
        }
    }
    // A design with another declaration order must declare it explicitly.
    // In the native bridge, for example, we use the first pause-row button.
    if !order.is_empty() {
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        for id in order {
            if !seen.insert(id.clone()) {
                return Err(format!(
                    "Duplicate screen '{id}' in screenOrder for {}",
                    design.display()
                ));
            }
            let Some(index) = listed.iter().position(|entry| entry["id"] == id) else {
                return Err(format!(
                    "Unknown screen '{id}' in screenOrder for {}",
                    design.display()
                ));
            };
            ordered.push(listed.remove(index));
        }
        ordered.extend(listed);
        listed = ordered;
    }
    let declaration = design.join("design.json");
    let mut screens = Vec::new();
    for (index, entry) in listed.iter().enumerate() {
        let at = |key: &str| -> Result<String, String> {
            entry[key].as_str().map(str::to_string).ok_or_else(|| {
                format!(
                    "screen {index} in {} declares no {key}",
                    declaration.display()
                )
            })
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
            list_page_size: entry
                .get("pageSize")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|size| *size > 0)
                        .and_then(|size| usize::try_from(size).ok())
                        .ok_or_else(|| {
                            format!("screen {index} pageSize must be a positive integer")
                        })
                })
                .transpose()?,
            place: screen_place(entry, index, &declaration)?,
            option_label,
            option_default,
            images: entry["images"].as_str().map(str::to_string),
            mark: entry["mark"].as_str().map(str::to_string),
            toggle: screen_toggle(entry, index, &declaration)?,
        });
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
/// The switches on these screens, written to the file that the player reads.
///
/// We write the switches of every screen with this function, also for a
/// generated list, whose panel we write after the declarations of the design.
/// A second copy of this format could write an empty `toggles` for a switch
/// whose button is on screen.
pub fn toggle_declarations(screens: &[Screen]) -> String {
    let switches: Vec<&Toggle> = screens.iter().filter_map(|s| s.toggle.as_ref()).collect();
    let ids: Vec<&str> = switches.iter().map(|t| t.id.as_str()).collect();
    let mut text = format!("toggles = \"{}\"\n", ids.join(" "));
    for toggle in switches {
        text.push_str(&format!(
            "toggle_on_{id} = \"{on}\"\ntoggle_off_{id} = \"{off}\"\ntoggle_default_{id} = \"{default}\"\ntoggle_guard_{id} = \"{guard}\"\ntoggle_guard_label_{id} = \"{label}\"\ntoggle_guard_status_{id} = \"{status}\"\n",
            id = toggle.id,
            on = toggle.on,
            off = toggle.off,
            default = toggle.default_on,
            guard = toggle.guard.declared(),
            label = toggle.guard_label,
            status = toggle.guard_status,
        ));
    }
    text
}

/// Every element that opens this screen, which we read in the player as a
/// space-separated list.
///
/// A design may draw a BACK button on a screen that only it has, such as
/// `disc-back` on the disc screen. We read several ids here in the player,
/// and the BACK of a generated list goes onto the declaration of the host
/// screen in the same way. We add the back buttons of the design to that
/// list too, so every BACK button in a design works.
///
/// A BACK leads to the screen that is not an Options entry and not reached
/// from Options, the same host screen that every generated list returns to.
fn shown_by(screen: &Screen, screens: &[&Screen], markup: &str) -> String {
    let host = screens
        .iter()
        .find(|entry| entry.place == ScreenPlace::Plain && entry.option_label.is_none());
    if host.map(|entry| entry.id.as_str()) != Some(screen.id.as_str()) {
        return screen.button.clone();
    }
    let mut buttons: Vec<String> = screen
        .button
        .split_whitespace()
        .map(str::to_string)
        .collect();
    for other in screens {
        // Only a screen that is in the design alone, next to the host screen.
        // BACK in an Options entry returns to Options, and BACK on the Options
        // screen is already the button of the host screen. If we added either
        // here, two elements would become one.
        if other.id == screen.id
            || other.place != ScreenPlace::Plain
            || other.option_label.is_some()
        {
            continue;
        }
        let back = format!("{}-back", other.id);
        // Only the buttons in the design. If we declared a button that is not
        // in the document, we would wait in the player for an element that
        // never appears.
        if markup.contains(&format!("id=\"{back}\"")) && !buttons.contains(&back) {
            buttons.push(back);
        }
    }
    buttons.join(" ")
}

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
            shown_by(screen, &screens, markup),
            id = screen.id,
        ));
        if let Some(images) = &screen.images {
            text.push_str(&format!(
                "screen_images_{id} = \"{images}\"\n",
                id = screen.id
            ));
        }
        if let Some(mark) = &screen.mark {
            text.push_str(&format!("screen_mark_{id} = \"{mark}\"\n", id = screen.id));
        }
    }
    text.push_str(&toggle_declarations(
        &screens.iter().copied().cloned().collect::<Vec<Screen>>(),
    ));
    text
}

/// Which screens we put in a game.
///
/// `chosen` is the set for the export. Without it, each entry has its
/// default. An empty set means a game with no Options button. We refuse an
/// id that is not an entry in this design, instead of ignoring it.
fn screens_for_export(
    screens: &[Screen],
    chosen: Option<&[String]>,
) -> Result<Vec<Screen>, String> {
    let entries: Vec<&Screen> = screens
        .iter()
        .filter(|screen| screen.option_label.is_some())
        .collect();
    let included: BTreeSet<&str> = match chosen {
        // Without a set, the game has one disc. The disc list is an entry
        // only when the export has more than one disc (`compose_menu`).
        None => entries
            .iter()
            .filter(|screen| screen.option_default && !screen.is_disc_list())
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
    if show_options
        && !staged
            .iter()
            .any(|screen| screen.place == ScreenPlace::Options)
    {
        return Err("The Native base must declare an Options screen for enabled entries".into());
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

/// The step between option entries, from `metrics.options.entryStep` in the
/// design, or from Native when the design declares none. We place each entry,
/// because the entries of the design are absolutely positioned. With a step
/// shorter than an entry, entries overlap.
fn option_entry_step(design: &Path) -> Result<usize, String> {
    let declared = |package: &Path| -> Result<Option<usize>, String> {
        let path = package.join("design.json");
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(None);
        };
        let declared: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        match declared["metrics"]["options"].get("entryStep") {
            None => Ok(None),
            Some(value) => value
                .as_u64()
                .filter(|step| *step > 0)
                .map(|step| Some(step as usize))
                .ok_or_else(|| {
                    format!(
                        "metrics.options.entryStep in {} must be a positive number of dp",
                        path.display()
                    )
                }),
        }
    };
    if let Some(step) = declared(design)? {
        return Ok(step);
    }
    let base = base_design(design)?;
    declared(&base)?.ok_or_else(|| {
        format!(
            "{} declares no metrics.options.entryStep",
            base.join("design.json").display()
        )
    })
}

const OPTIONS_LAYOUT_CSS: &str = r#"
#options-entries { position: absolute; left: 56dp; top: 272dp; width: 840dp; height: 200dp; }
.option-entry { position: absolute; left: 0; width: 840dp; height: 64dp; line-height: 58dp; font-family: Silkscreen; font-size: 20dp; border-width: 3dp; text-align: center; white-space: nowrap; overflow: hidden; }
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
    let top = (index * option_entry_step(design)?).to_string();
    let template_path = design.join("option-entry.rml");
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|e| format!("Could not read {}: {e}", template_path.display()))?
    } else {
        "<button class=\"menu-action option-entry\" id=\"BUTTON\" style=\"top: TOPdp;\">LABEL</button>"
            .to_string()
    };
    let mut button = template
        .replace("BUTTON", &screen.button)
        .replace("TOP", &top)
        .replace("LABEL", &rml_text(&label));
    // We get the number of discs from the core after the game has loaded, so
    // the entry must already be in the document. It starts hidden. A
    // display:none button is still a focus stop unless it is also disabled,
    // and the focus could then move onto an invisible button.
    if screen.is_disc_list() {
        button = button.replace(
            "style=\"top: ",
            "disabled=\"disabled\" style=\"display: none; top: ",
        );
    }
    Ok(button)
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
    let show = staged
        .iter()
        .any(|screen| screen.place == ScreenPlace::Options);
    let options = staged
        .iter()
        .find(|screen| screen.place == ScreenPlace::Options);
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
        let opener_label = options
            .label
            .clone()
            .unwrap_or_else(|| options.heading.clone());
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

        let panel_id = format!("id=\"{}\"", options.panel);
        if !document.contains(&panel_id) {
            if options.id.is_empty()
                || !options
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(format!("Invalid Options screen id '{}'", options.id));
            }
            let name = format!("screen-{}.rml", options.id);
            let base = base_design(design)?;
            let shell = if design.join(&name).is_file() || base.join(&name).is_file() {
                design_fragment(design, &base, &name)?
            } else {
                let back = options.back_label.clone().unwrap_or_else(|| "BACK".into());
                format!(
                    "<div {panel_id} class=\"screen-panel\" style=\"display:none;\"><div id=\"options-entries\"><!--OPTIONS--></div><button class=\"menu-action options-back\" id=\"options-back\">{back}</button></div>",
                    back = rml_text(&back),
                )
            };
            if !shell.contains(&panel_id) {
                return Err(format!("{name} must contain {panel_id}"));
            }
            if !shell.contains("<!--OPTIONS-->") && !shell.contains("id=\"options-entries\">") {
                return Err(format!(
                    "{name} must contain <!--OPTIONS--> or #options-entries"
                ));
            }
            let footer = "<div id=\"footer\">";
            if let Some(at) = document.find(footer) {
                document.insert_str(at, &shell);
            } else {
                return Err(
                    "menu.rml has no footer, so the options screen has nowhere to go".into(),
                );
            }
        }
        if document.contains("<!--OPTIONS-->") {
            document = document.replace("<!--OPTIONS-->", &entries);
        } else if document.contains(&panel_id) {
            document = document.replacen(
                "id=\"options-entries\">",
                &format!("id=\"options-entries\">{entries}"),
                1,
            );
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
            format!(
                "overlay {index} in {} declares no id",
                declaration.display()
            )
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

/// When the bind list appears and how wide it is, written next to the
/// overlay clocks that the player reads.
///
/// `afterMs` is a declaration like that of an overlay. The time comes from
/// the design, and we wait for it in the player. Without a `binds` block in
/// the design, there is no list and the callout contains one assignment.
fn binds_declarations(design: &Path) -> Result<String, String> {
    let path = design.join("design.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(String::new());
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let Some(binds) = declared.get("binds") else {
        return Ok(String::new());
    };
    let after = binds
        .get("afterMs")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "binds.afterMs must be a number of milliseconds".to_string())?;
    let width = binds
        .get("width")
        .and_then(|value| value.as_u64())
        .filter(|value| *value > 0)
        .ok_or_else(|| "binds.width must be a width in dp".to_string())?;
    Ok(format!(
        "binds_after = \"{after}\"\nbinds_width = \"{width}\"\nbinds_list = \"control-binds\"\n"
    ))
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
/// The values that a document can use from the product, as opposed to the
/// values that a stylesheet uses from the design.
///
/// A design contains `design(version)` where the version number goes, so the
/// footer contains the product version and nobody has to edit the design
/// when the version changes.
fn product_tokens() -> std::collections::BTreeMap<String, String> {
    let mut tokens = std::collections::BTreeMap::new();
    tokens.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    tokens
}

/// The document of the design, with those values filled in.
///
/// We read the markup of the design in both stages that write the final menu,
/// the theme stage and the controls stage, and the second overwrites the
/// first. We write the document only through this function, so we replace
/// `design(version)` in both.
fn substitute_document(markup: &str) -> Result<String, String> {
    substitute_tokens(markup, &product_tokens())
}

fn design_tokens(
    design: &Path,
    palette: &Palette,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut tokens = std::collections::BTreeMap::new();
    // The values of the design first, so a palette may override any of them.
    if let Ok(text) = fs::read_to_string(design.join("design.json")) {
        let declared: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if let Some(own) = declared.get("tokens").and_then(|v| v.as_object()) {
            for (name, value) in own {
                if let Some(value) = value.as_str() {
                    tokens.insert(name.clone(), value.to_string());
                }
            }
        }
    }
    tokens.extend(product_tokens());
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
        metrics[group][key]
            .as_i64()
            .map(|v| v as i32)
            .unwrap_or(default)
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

/// The marker a design puts where the volume control goes, when it goes
/// somewhere other than the start of the Options panel. Without the marker,
/// we insert the control into the Options panel that we already built.
pub const VOLUME_SLOT: &str = "<!--VOLUME-->";

const BUILTIN_SLIDER: &str = include_str!("../../../integrations/parts/slider.rml");

fn has_class(template: &str, class: &str) -> bool {
    template.split("class=\"").skip(1).any(|rest| {
        rest.split('"')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .any(|token| token == class)
    })
}

fn part_template(design: &Path, name: &str, builtin: &str) -> Result<String, String> {
    let override_path = design.join("parts").join(format!("{name}.rml"));
    if override_path.is_file() {
        return fs::read_to_string(&override_path)
            .map_err(|e| format!("Could not read {}: {e}", override_path.display()));
    }
    Ok(builtin.to_string())
}

fn require_classes(kind: &str, template: &str, classes: &[&str]) -> Result<(), String> {
    for class in classes {
        if !has_class(template, class) {
            return Err(format!(
                "the {kind} part must carry class `{class}`, so every design draws it and the control can find it"
            ));
        }
    }
    if !template.contains("PART-ID") {
        return Err(format!(
            "the {kind} part must contain PART-ID, so each use can name its own"
        ));
    }
    Ok(())
}

fn fill_part(template: &str, id: &str, label: &str) -> String {
    template.replace("PART-ID", id).replace("LABEL", label)
}

/// The volume control, made of its name, the low end, an arrow, the slider
/// from the design, an arrow and the high end.
///
/// The slider comes first in the document, so the keyboard focus goes to it
/// first and the left and right keys move it. The design places the rest, so
/// the order in the document is not the order on screen. There is no number
/// and no mute button, because the quiet end is the quietest the volume goes.
pub fn volume_control_markup(design: &Path) -> Result<String, String> {
    let slider = part_template(design, "slider", BUILTIN_SLIDER)?;
    require_classes(
        "slider",
        &slider,
        &[
            "slider",
            "slider-track",
            "slider-fill",
            "slider-thumb",
            "slider-readout",
        ],
    )?;
    Ok(format!(
        "<div id=\"volume-control\">{slider}<button id=\"{down}\" class=\"menu-action volume-arrow arrow-down\">&lt;</button><button id=\"{up}\" class=\"menu-action volume-arrow arrow-up\">&gt;</button><div id=\"{low}\" class=\"volume-end\">LOW</div><div id=\"{high}\" class=\"volume-end\">HIGH</div><div class=\"volume-name\">VOLUME</div></div>",
        slider = fill_part(&slider, crate::volume::slider_id(), ""),
        down = crate::volume::down_id(),
        up = crate::volume::up_id(),
        low = crate::volume::low_id(),
        high = crate::volume::high_id(),
    ))
}

/// Put the volume control in Options.
///
/// When the design contains `<!--VOLUME-->`, we put the control there.
/// Otherwise we insert it at the start of the Options panel and leave the
/// Options entries where they are. A menu with no Options screen has no
/// volume control either.
pub fn install_volume_control(document: &str, design: &Path) -> Result<String, String> {
    if document.contains("id=\"volume-control\"") {
        return Ok(document.to_string());
    }
    let markup = volume_control_markup(design)?;
    if document.contains(VOLUME_SLOT) {
        return Ok(document.replacen(VOLUME_SLOT, &markup, 1));
    }
    let marker = "id=\"options-panel\"";
    let Some(at) = document.find(marker) else {
        return Ok(document.to_string());
    };
    let tag_end = document[at..]
        .find('>')
        .map(|end| at + end + 1)
        .ok_or_else(|| "the options panel tag is never closed".to_string())?;
    let mut installed = String::with_capacity(document.len() + markup.len());
    installed.push_str(&document[..tag_end]);
    installed.push_str(&markup);
    installed.push_str(&document[tag_end..]);
    Ok(installed)
}

/// Geometry for a design with no styles for a slider.
///
/// The colours come from the palette block, as for a button. We leave a design
/// that already styles `.slider` unchanged, because that styling is part of it.
pub fn builtin_part_rules(stylesheet: &str) -> &'static str {
    if stylesheet.contains(".slider") {
        ""
    } else {
        r#"
/* part:slider */
.slider { display: block; width: 100%; }
.slider-readout { display: block; width: 100%; height: 36dp; font-family: Silkscreen; font-size: 28dp; text-align: center; }
.slider-track { display: block; position: relative; width: 100%; height: 28dp; margin-top: 12dp; border-width: 4dp; }
.slider-fill { position: absolute; left: 0; top: 0; height: 100%; width: 0; }
.slider-thumb { position: absolute; top: -8dp; width: 22dp; height: 44dp; border-width: 4dp; }
.toggle { display: block; position: relative; width: 240dp; height: 48dp; margin-top: 28dp; font-family: Silkscreen; font-size: 18dp; line-height: 42dp; padding-left: 56dp; border-width: 3dp; }
.toggle-knob { position: absolute; left: 8dp; top: 6dp; width: 28dp; height: 28dp; border-width: 3dp; }
.toggle.on .toggle-knob { left: 196dp; }
"#
    }
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

fn design_fragment(design: &Path, base: &Path, name: &str) -> Result<String, String> {
    let selected = design.join(name);
    let source = if selected.is_file() {
        selected
    } else {
        base.join(name)
    };
    fs::read_to_string(&source).map_err(|e| format!("Could not read {}: {e}", source.display()))
}

/// Compose the Native document with the selected screen and chrome fragments.
/// The screen order is explicit. The small screen-order.rml of a design lists
/// only the extra panels that go before the generated list screens.
fn menu_document(design: &Path, staged: &[Screen]) -> Result<String, String> {
    let base = base_design(design)?;
    let mut menu = fs::read_to_string(base.join("menu.rml"))
        .map_err(|e| format!("Could not read Native menu.rml: {e}"))?;
    for (slot, name) in [
        ("<!--SPINE-->", "spine.rml"),
        ("<!--HEADING-->", "heading.rml"),
        ("<!--FOOTER-->", "footer.rml"),
        ("<!--SCREEN:pause-->", "screen-pause.rml"),
        ("<!--SCREEN:controls-->", "screen-controls.rml"),
    ] {
        let selected = design.join(name);
        let base_file = base.join(name);
        let value = if selected.is_file() || base_file.is_file() {
            design_fragment(design, &base, name)?
        } else {
            String::new()
        };
        if !menu.contains(slot) {
            return Err(format!("Native menu has no {slot} insertion point"));
        }
        menu = menu.replace(slot, &value);
    }
    let order = design.join("screen-order.rml");
    let extra = if order.is_file() {
        fs::read_to_string(&order)
            .map_err(|e| format!("Could not read {}: {e}", order.display()))?
    } else {
        String::new()
    };
    let screens = declared_screens(design)?;
    let mut expanded = String::new();
    let mut remaining = extra.as_str();
    let mut placed = BTreeSet::new();
    while let Some(start) = remaining.find("<!--SCREEN:") {
        expanded.push_str(&remaining[..start]);
        let after = &remaining[start + "<!--SCREEN:".len()..];
        let end = after
            .find("-->")
            .ok_or_else(|| format!("Unclosed screen in {}", order.display()))?;
        let id = &after[..end];
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || id.is_empty() {
            return Err(format!("Invalid screen id '{id}' in {}", order.display()));
        }
        if !screens.iter().any(|screen| screen.id == id) {
            return Err(format!(
                "Screen '{id}' in {} is not declared",
                order.display()
            ));
        }
        if !placed.insert(id) {
            return Err(format!("Screen '{id}' occurs twice in {}", order.display()));
        }
        // We do not draw a screen that is not in this game. An export with no
        // Options entries has no Options panel, not one that cannot be reached.
        if staged.iter().any(|screen| screen.id == id) {
            expanded.push_str(&design_fragment(
                design,
                &base,
                &format!("screen-{id}.rml"),
            )?);
        }
        remaining = &after[end + "-->".len()..];
    }
    expanded.push_str(remaining);
    if !menu.contains("<!--EXTRA-SCREENS-->") {
        return Err("Native menu has no extra-screen insertion point".into());
    }
    let slots = design_fragment(design, &base, "save-slots.rml")?;
    Ok(menu
        .replace("<!--EXTRA-SCREENS-->", &expanded)
        .replace("<!--SAVE-SLOTS-->", &slots))
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
        "{}{}{}",
        screen_declarations(screens, markup),
        overlay_declarations(design, markup)?,
        binds_declarations(design)?
    );
    fs::write(destination.join("design.cfg"), text)
        .map_err(|e| format!("Could not write the design's declarations: {e}"))
}

/// Compose shared component rules followed by optional selected-design rules.
/// Both use the selected design's metrics and palette, like the main sheet.
pub fn append_component_style(
    source: &Path,
    destination: &Path,
    palette: &str,
    name: &str,
) -> Result<(), String> {
    let palette = registry()?
        .palettes
        .into_iter()
        .find(|item| item.id == palette)
        .ok_or_else(|| "Choose an available colour palette.".to_string())?;
    let base = base_design(source)?;
    let mut paths = vec![base.join(name)];
    if source != base && source.join(name).is_file() {
        paths.push(source.join(name));
    }
    let tokens = design_tokens(source, &palette)?;
    let sheet = destination.join("menu.rcss");
    let mut css = fs::read_to_string(&sheet).map_err(|error| error.to_string())?;
    for path in paths {
        let rules = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        css.push('\n');
        css.push_str(&substitute_tokens(&rules, &tokens)?);
    }
    fs::write(sheet, css).map_err(|error| error.to_string())
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
    let base = base_design(source)?;
    for name in ["menu.rcss", "Silkscreen-Regular.ttf", "Silkscreen-OFL.txt"] {
        let selected = source.join(name);
        let input = if selected.is_file() {
            selected
        } else {
            base.join(name)
        };
        fs::copy(&input, destination.join(name))
            .map_err(|e| format!("Could not prepare menu asset {name}: {e}"))?;
    }
    let defaults = screens_for_export(&declared_screens(source)?, None)?;
    fs::write(destination.join("menu.rml"), menu_document(source, &defaults)?)
        .map_err(|e| format!("Could not prepare menu.rml: {e}"))?;
    let tokens = design_tokens(source, &palette)?;
    // The document too, not only the stylesheet. A design contains design(…)
    // where a value comes from outside the design, such as the version, and
    // the same rule applies to words as to colours.
    let menu = substitute_document(
        &fs::read_to_string(destination.join("menu.rml")).map_err(|e| e.to_string())?,
    )?;
    // The same Options panel as in the controls stage. We add volume to it
    // here too, because a theme staged alone, as in the interaction checks,
    // never reaches the controls stage.
    let (menu, _) = apply_options(source, &menu, None)?;
    fs::write(
        destination.join("menu.rml"),
        install_volume_control(&menu, source)?,
    )
    .map_err(|e| format!("Could not install the volume control: {e}"))?;
    // The declarations of the design, in the file that the player reads. We
    // write them next to the stylesheet because both belong to the design. A
    // design lists its screens, and we show them by name in the player.
    // These are defaults until we apply the set of the game in the controls
    // stage. For a game with a set we overwrite them with the same function.
    let staged = screens_for_export(&declared_screens(source)?, None)?;
    let markup = fs::read_to_string(destination.join("menu.rml")).map_err(|e| e.to_string())?;
    write_declarations(source, destination, &staged, &markup)?;
    let mut css = fs::read_to_string(destination.join("menu.rcss")).map_err(|e| e.to_string())?;
    css.push_str(builtin_part_rules(&css));
    // The colours of the design, in the rules of the design. We append nothing,
    // because a palette contains values and no styles. Appended rules would
    // declare selectors of the design again and, coming later with equal
    // specificity, override them.
    css = substitute_tokens(&css, &tokens)?;
    if let Some(image_path) = background {
        let image = crate::icons::read_image(image_path).map_err(|e| e.to_string())?;
        image
            .resize(1920, 1200, image::imageops::FilterType::Lanczos3)
            .save_with_format(destination.join("background.png"), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        css.push_str("\n#screen { decorator: image(\"background.png\" cover); }\n");
    }
    fs::write(destination.join("menu.rcss"), css).map_err(|e| e.to_string())?;
    // We place the options entries with this, because a theme staged alone
    // (for the offscreen pictures) never reaches the controls stage that writes
    // it for an export. The colour comes from the button in the design.
    let show_options = staged
        .iter()
        .any(|screen| screen.place == ScreenPlace::Options);
    write_options_css(destination, show_options)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    /// The design to draw, as a separate package, the same folder that we
    /// stage from in an export.
    ///
    /// The `assets` folder below is not a design. It contains the controller
    /// artwork shared by every design, and we do not use any menu documents in
    /// it. Choosing another design changes the preview, because we draw the
    /// preview from this folder.
    #[serde(default)]
    pub design: std::path::PathBuf,
    /// Where the controller artwork is. It is not part of any design.
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
    // For a caller without a design we use the artwork folder for both. The
    // call then fails as an export would, with the name, and we do not draw a
    // menu that no game ships.
    let design = if request.design.as_os_str().is_empty() {
        request.assets.clone()
    } else {
        request.design.clone()
    };
    prepare_theme_assets(
        &design,
        &request.output_dir,
        &request.palette,
        request.background.as_deref(),
    )?;
    prepare_controls_assets(
        // The artwork from the shared folder and the frame from the design,
        // the same two arguments from the same two places as in an export.
        &request.assets,
        &design,
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
) -> Result<Vec<Screen>, String> {
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

    // Use the same base-and-overrides composition as prepare_theme_assets.
    // If we read a raw design file again here, we would lose the resolved screens.
    let template = menu_document(design, &screens_for_export(&declared_screens(design)?, entries)?)?;
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
    let binds = bind_list_markup(design, &offered)?;
    if !template.contains(BINDS_SLOT) {
        return Err(format!(
            "this design has no {BINDS_SLOT} for the binds on a control. \
             Add the slot to menu.rml, outside #controller-scene."
        ));
    }
    let menu = template
        .replace("<!--CONTROLS-->", &markup)
        .replace(PICKER_SLOT, &picker)
        .replace(BINDS_SLOT, &binds);
    let (menu, screens) = apply_options(design, &menu, entries)?;
    // Volume is part of the Options screen that we just built. It is not
    // another screen, and we leave the Options entries where they are.
    let menu = install_volume_control(&menu, design)?;
    // The same file as in prepare_theme_assets. This version replaces it,
    // because here we know the entries chosen for the game and we just built
    // the markup from them.
    let menu = substitute_document(&menu)?;
    write_declarations(design, destination, &screens, &menu)?;
    fs::write(destination.join("menu.rml"), menu).map_err(|e| e.to_string())?;
    let show_options = screens
        .iter()
        .any(|screen| screen.place == ScreenPlace::Options);
    write_options_css(destination, show_options)?;
    Ok(screens)
}

/// The entries for a game without a set, each with its default.
///
/// We make this list explicit because we add to it at export. For example, we
/// ship the achievements entry in a game with achievements, and None cannot
/// express "the defaults and this one".
pub fn default_entries(design: &Path) -> Result<Vec<String>, String> {
    Ok(declared_screens(design)?
        .into_iter()
        .filter(|screen| screen.option_label.is_some() && screen.option_default)
        .map(|screen| screen.id)
        .collect())
}

/// Where the bind list goes, as a sibling of the scene like the picker, so
/// it stays when we replace the scene after someone swaps pads.
const BINDS_SLOT: &str = "<!--BINDS-->";

/// One row for each input that the bundled pads can bind to a single control.
///
/// A `retro_keybind` contains a key, a button, an axis and a mouse button. A
/// stick is several controls drawn as one object, so its list has the inputs
/// of every direction. We write the rows now because we cannot create
/// elements in the player while the game runs. There we fill these rows and
/// hide the rest. There is always one row more than a page, so we can say
/// that the rest did not fit, even on a pad whose busiest control would fill
/// exactly one page.
fn bind_list_markup(
    design: &Path,
    profiles: &[crate::controls::ControlProfile],
) -> Result<String, String> {
    let template = crate::lists::row_template(design)?;
    let page_size = crate::lists::page_size(design)?;
    let mut slots = 4usize;
    for profile in profiles {
        let mut groups: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for control in &profile.controls {
            if let Some(name) = control.group.as_deref() {
                *groups.entry(name).or_default() += 1;
            }
        }
        for size in groups.values() {
            slots = slots.max(size * 4);
        }
    }
    slots = slots.max(page_size.saturating_add(1));
    let items: Vec<crate::lists::ListItem> = (1..=slots)
        .map(|index| crate::lists::ListItem {
            id: format!("bind-{index}"),
            icon: String::new(),
            title: String::new(),
            detail: String::new(),
            state: String::new(),
            selected: false,
            accent: false,
            line: false,
        })
        .collect();
    // We name the wrapper after the screen in render_list (discs-list). In the
    // player we look this one up as control-binds, the id stored in binds_list
    // in design.cfg, so we replace the tag of this wrapper with that id.
    // Without it, no bind list has a box, and the placement check fails for
    // every one of them.
    Ok(
        crate::lists::render_list("binds", &template, &items, page_size).replacen(
            "<div id=\"binds-list\" class=\"list\">",
            "<div id=\"control-binds\" class=\"list\" style=\"display:none;\">",
            1,
        ),
    )
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
    markup.push_str(&control_group_markup(
        &group_names,
        &grouped,
        controls,
        illustrated,
        metrics,
    ));
    let placed_scene = crate::scene_layout::layout(&profile.controls, metrics);
    let placements = &placed_scene.controls;
    for item in profile.controls.iter().filter(|item| item.group.is_none()) {
        let item = item.clone();
        let custom = controls.bindings.get(&item.id);
        let author_label = custom
            .and_then(|value| value.label.as_deref())
            .filter(|label| !label.trim().is_empty());
        let label = author_label.unwrap_or(&item.label);
        let key = callout_line(&binding_words(
            custom
                .and_then(|value| value.key.as_deref())
                .unwrap_or(&item.key),
            custom.and_then(|value| value.button.as_deref()),
            custom.and_then(|value| value.axis.as_deref()),
            custom.and_then(|value| value.mouse),
        ));
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
                let orientation = if run.height == 0 {
                    "horizontal"
                } else {
                    "vertical"
                };
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
        markup.push_str(&control_callout_markup(id, label, original, &key, cx, cy));
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
    markup.push_str(
        "</div>
</div>
",
    );
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

        let mut words = Vec::new();
        for item in &members {
            let custom = controls.bindings.get(&item.id);
            words.extend(binding_words(
                custom
                    .and_then(|value| value.key.as_deref())
                    .unwrap_or(&item.key),
                custom.and_then(|value| value.button.as_deref()),
                custom.and_then(|value| value.axis.as_deref()),
                custom.and_then(|value| value.mouse),
            ));
        }
        let title = name.replace('_', " ").to_uppercase();
        markup.push_str(&format!(
            r#"
<button id="control-group-{name}" class="control-group" style="left:{box_x}dp;top:{top}dp;">
<div class="control-label">{}</div>
<div id="control-group-binding-{name}" class="control-assignment">{}</div>
</button>
"#,
            rml_text(&title),
            rml_text(&callout_line(&words)),
        ));
    }
    markup
}

/// The words that a callout can contain about one control, in list order.
fn binding_words(
    key: &str,
    button: Option<&str>,
    axis: Option<&str>,
    mouse: Option<u32>,
) -> Vec<String> {
    let mut words = Vec::new();
    if !key.is_empty() && key != "nul" {
        words.push(key.to_string());
    }
    if let Some(button) = button.filter(|value| !value.is_empty()) {
        words.push(format!("Button {button}"));
    }
    if let Some(axis) = axis.filter(|value| !value.is_empty()) {
        words.push(format!("Axis {axis}"));
    }
    if let Some(mouse) = mouse {
        words.push(match mouse {
            2 => "Left".to_string(),
            3 => "Right".to_string(),
            4 => "Wheel up".to_string(),
            5 => "Wheel down".to_string(),
            6 => "Middle".to_string(),
            other => format!("Mouse {other}"),
        });
    }
    words
}

/// Every binding, separated by commas. One binding stays that binding, and
/// none is a dash. We show no count such as "3 binds", which the player cannot see.
fn callout_line(words: &[String]) -> String {
    match words {
        [] => "---".to_string(),
        [only] => only.clone(),
        many => many.join(", "),
    }
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
    let base = base_design(source)?;
    for (from, to) in [
        ("splash.rml", "menu.rml"),
        ("menu.rcss", "menu.rcss"),
        ("Silkscreen-Regular.ttf", "Silkscreen-Regular.ttf"),
        ("Silkscreen-OFL.txt", "Silkscreen-OFL.txt"),
    ] {
        let selected = source.join(from);
        let input = if selected.is_file() {
            selected
        } else {
            base.join(from)
        };
        fs::copy(input, destination.join(to))
            .map_err(|e| format!("Could not prepare splash asset {from}: {e}"))?;
    }
    let staged = screens_for_export(&declared_screens(source)?, None)?;
    let markup = fs::read_to_string(destination.join("menu.rml")).map_err(|e| e.to_string())?;
    write_declarations(source, destination, &staged, &markup)?;
    let css = fs::read_to_string(destination.join("menu.rcss")).map_err(|e| e.to_string())?;
    let css = substitute_tokens(&css, &design_tokens(source, &palette)?)?;
    fs::write(destination.join("menu.rcss"), css).map_err(|e| e.to_string())
}

/// The options of an export for the in-game menu, borrowed from its request.
pub struct MenuRequest<'a> {
    /// The prepared kit: the design under `designs/<id>`, the shared
    /// controller artwork under `menu-assets`.
    pub kit: &'a Path,
    pub design: &'a str,
    pub palette: &'a str,
    pub background: Option<&'a Path>,
    pub system: &'a str,
    pub controls: &'a crate::controls::Controls,
    pub show_menu: bool,
    pub splash: bool,
    pub include_achievements: bool,
    pub menu_entries: Option<&'a [String]>,
    pub shaders: &'a crate::shaders::ShaderSelection,
    /// How many discs the game has. The disc list, and its Options entry,
    /// exist only for more than one.
    pub discs: usize,
}

/// Stage the menu an exported game shows into `destination`: the full menu
/// when it has one, the splash alone when it has only that, nothing otherwise.
pub fn compose_menu(request: &MenuRequest, destination: &Path) -> Result<(), String> {
    let design = staged_design(request.kit, request.design);
    if request.show_menu {
        prepare_theme_assets(&design, destination, request.palette, request.background)?;
        // We compose the data lists and the live account screen with one function.
        let mut lists: Vec<crate::lists::List> = Vec::new();
        let several_discs = request.discs > 1;
        if several_discs {
            lists.extend(crate::disc_menu::list(&design)?);
        }
        let staged_shaders = crate::shaders::stage(&design, destination, request.shaders)?;
        lists.extend(staged_shaders.list);
        if crate::achievements::included(request.include_achievements, request.show_menu) {
            lists.push(crate::achievements::screen(&design)?);
        }
        let mut entries = crate::achievements::entries(
            &design,
            request.include_achievements,
            request.show_menu,
            request.menu_entries,
        )?;
        // The disc entry depends on the content, not on the author. It is
        // present exactly when the game has more than one disc.
        if !several_discs {
            let disc_lists: Vec<String> = declared_screens(&design)?
                .into_iter()
                .filter(Screen::is_disc_list)
                .map(|screen| screen.id)
                .collect();
            entries.retain(|entry| !disc_lists.contains(entry));
        }
        for list in &lists {
            if list.screen.option_label.is_some() && !entries.contains(&list.screen.id) {
                entries.push(list.screen.id.clone());
            }
        }
        let screens = prepare_controls_assets(
            // The controller artwork is the same for every design, because all
            // designs show the same pads, so we keep it in the shared menu-assets.
            &request.kit.join("menu-assets"),
            // The frame in which we draw the pads comes from the design, and
            // the generated coordinates must match the stylesheet of that
            // design.
            &design,
            destination,
            request.system,
            request.controls,
            Some(&entries),
        )?;
        crate::lists::install(&design, destination, &screens, &lists)?;
        if crate::achievements::included(request.include_achievements, request.show_menu) {
            append_component_style(&design, destination, request.palette, "achievements.rcss")?;
        }
    } else if request.splash {
        prepare_splash_assets(&design, destination, request.palette)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn sound_source() -> std::path::PathBuf {
        crate::repo::at("desktop/assets/menu-sounds")
    }

    /// A callout with one binding of a control that has three is false, and a
    /// count with no bindings refers to something the player cannot see.
    #[test]
    fn a_callout_names_every_binding_and_never_a_count() {
        let three = binding_words("up", Some("0"), Some("+0"), None);
        let line = callout_line(&three);
        assert_eq!(line, "up, Button 0, Axis +0");
        assert!(line.starts_with("up"), "the first binding is visible");
        assert!(line.contains("Button 0") && line.contains("Axis +0"));
        assert!(!line.contains("bind"), "a count is not a binding: {line}");
        assert_eq!(callout_line(&binding_words("c", None, None, None)), "c");
        assert_eq!(callout_line(&[]), "---");
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

    /// The Off choice is already labelled Off, so we add no sentence under it.
    #[test]
    fn off_pack_does_not_restate_that_audio_is_off() {
        let off = registry()
            .unwrap()
            .sound_packs
            .into_iter()
            .find(|pack| pack.id == "off")
            .expect("off pack");
        assert!(
            !off.description
                .to_ascii_lowercase()
                .contains("no menu audio"),
            "the Off option already says it is off, and this is still under it: {}",
            off.description
        );
    }

    /// A pack that the author can hear has a line that describes its
    /// character. Off has none, because the control label is already Off.
    #[test]
    fn every_sound_pack_is_described_for_the_picker() {
        for pack in registry().unwrap().sound_packs {
            assert!(!pack.name.trim().is_empty(), "{} has no name", pack.id);
            if pack.id == "off" {
                continue;
            }
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
        let temporary = rominabox_scratch::Scratch::reserve("rominabox-sound-pack");
        let error = prepare_sound_assets(&sound_source(), &temporary, "pulse")
            .expect_err("retired pack must not stage");
        assert!(error.contains("available menu sound pack"), "{error}");
        assert!(!temporary.exists(), "rejection must not create output");
    }

    fn native_menu() -> (PathBuf, String) {
        let design = crate::repo::at("integrations/designs/native");
        let menu = menu_document(&design, &declared_screens(&design).unwrap()).expect("native menu");
        (design, menu)
    }

    fn design_menu(name: &str) -> (PathBuf, String) {
        let design = crate::repo::at("integrations/designs").join(name);
        let menu = menu_document(&design, &declared_screens(&design).unwrap()).expect("design menu");
        (design, menu)
    }

    /// A design may draw an extra screen, and its BACK must lead somewhere.
    ///
    /// The disc design contains `disc-back`. Without a declaration we would not
    /// listen for it in the player. The button would appear and could get the
    /// focus but would do nothing, and leaving with Escape would hide the fault.
    #[test]
    fn a_designs_own_screen_has_a_back_that_goes_somewhere() {
        let (design, menu) = design_menu("disc");
        assert!(
            menu.contains("id=\"disc-back\""),
            "the disc design draws this button"
        );
        let (staged, screens) = apply_options(&design, &menu, None).expect("defaults");
        let cfg = screen_declarations(&screens, &staged);

        let pause = cfg
            .lines()
            .find(|line| line.starts_with("screen_button_pause = "))
            .expect("the pause screen is declared");
        assert!(
            pause.contains("disc-back"),
            "pressing the disc screen's BACK must show the screen behind it; \
             the player is told {pause}"
        );

        // And only those of the design. BACK in an Options entry returns to
        // Options, and declaring it here would make the two one element.
        assert!(
            !pause.contains("controls-back"),
            "the controls screen is an Options entry and returns there: {pause}"
        );

        // The native design has no extra screen, so we add nothing.
        let (native, native_menu) = design_menu("native");
        let (native_staged, native_screens) =
            apply_options(&native, &native_menu, None).expect("defaults");
        let native_cfg = screen_declarations(&native_screens, &native_staged);
        assert!(
            native_cfg.contains("screen_button_pause = \"options-back\""),
            "a design with no screen of its own is unchanged"
        );
    }

    /// The footer of an exported game contains the product version, not a
    /// fixed label.
    ///
    /// The footer in the native design contains `design(version)`, as its
    /// stylesheet contains the name of a colour, so the fixed text
    /// "ROM-IN-A-BOX / PROTOTYPE" is in no exported game.
    #[test]
    fn an_exported_game_says_its_version_and_not_that_it_is_unfinished() {
        let (design, menu) = native_menu();
        assert!(
            menu.contains("design(version)"),
            "the design asks for the version rather than typing one in"
        );
        assert!(!menu.contains("PROTOTYPE"));

        let root = rominabox_scratch::Scratch::dir("rominabox-version");
        prepare_theme_assets(&design, &root, "blue", None).expect("staged");
        let staged = fs::read_to_string(root.join("menu.rml")).expect("the staged menu");
        assert!(
            !staged.contains("PROTOTYPE"),
            "the word survived staging: {staged}"
        );
        assert!(
            staged.contains(&format!("ROM-IN-A-BOX / {}", env!("CARGO_PKG_VERSION"))),
            "the footer should read the version: {staged}"
        );
        assert!(
            !staged.contains("design(version)"),
            "the token was left unsubstituted"
        );

        // Also run after the controls stage, in which we rewrite the same file
        // from the markup of the design. With substitution only in the theme
        // stage, `design(version)` would stay in every rendered state.
        prepare_controls_assets(
            &design,
            &design,
            &root,
            // A console with no drawing, so no artwork is required next to it.
            // We check the document here, not the pad.
            "atari2600",
            &crate::controls::Controls::default(),
            None,
        )
        .expect("controls staged");
        let after = fs::read_to_string(root.join("menu.rml")).expect("the staged menu");
        assert!(
            after.contains(&format!("ROM-IN-A-BOX / {}", env!("CARGO_PKG_VERSION"))),
            "the controls stage put the token back: {after}"
        );
        assert!(!after.contains("PROTOTYPE") && !after.contains("design(version)"));
        let _ = fs::remove_dir_all(&root);
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
        assert!(
            actions < panel && panel < controls,
            "controls sits inside options"
        );
        assert!(staged.contains("id=\"options\""));
        assert!(staged.contains(">OPTIONS<"));
        assert!(staged.contains(">CONTROLS<"));
        assert!(staged.contains(">BACK<"));
        assert_eq!(
            screens
                .iter()
                .find(|screen| screen.id == "pause")
                .unwrap()
                .button,
            "options-back"
        );
        let cfg = screen_declarations(&screens, &staged);
        assert!(cfg.contains("screens = \"pause options controls\"") || cfg.contains("options"));
        assert!(cfg.contains("screen_button_pause = \"options-back\""));
        assert!(cfg.contains("screen_button_options = \"options\""));
        assert!(cfg.contains("screen_button_controls = \"controls\""));

        let (empty, empty_screens) =
            apply_options(&design, &menu, Some(&[])).expect("nothing enabled");
        assert!(
            !empty.contains("id=\"options\""),
            "no options button when nothing is enabled"
        );
        assert!(!empty.contains("id=\"options-panel\""));
        assert!(
            button_bounds(&empty, "controls").is_none(),
            "controls is not left on the pause row"
        );
        assert!(!empty_screens
            .iter()
            .any(|screen| screen.place == ScreenPlace::Options));
    }

    /// Shaders is an entry that a design can declare. It is absent until the
    /// author turns it on, and we reject an unknown id and do not drop it.
    #[test]
    fn an_entry_appears_only_when_that_game_enables_it() {
        let root = rominabox_scratch::Scratch::dir("rominabox-options");
        let design = root.join("native");
        fs::create_dir_all(&design).unwrap();
        fs::write(
            design.join("menu.rml"),
            "<rml><body><div id=\"screen\"><div id=\"actions\"><button class=\"menu-action\" id=\"resume\">CONTINUE</button><button class=\"menu-action\" id=\"quit\">QUIT</button></div><div id=\"footer\"></div></div></body></rml>",
        )
        .unwrap();
        fs::write(
            design.join("design.json"),
            r#"{
                "screens": [
                    {"id":"pause","panel":"pause-panel","heading":"PAUSED","footer":"ESC  CONTINUE","button":"controls-back"},
                    {"id":"options","panel":"options-panel","heading":"OPTIONS","footer":"ESC  BACK","button":"options","label":"OPTIONS","back":"BACK","place":"options"},
                    {"id":"controls","panel":"controls-panel","heading":"CONTROLS","footer":"ESC  BACK","button":"controls","option":{"label":"CONTROLS","default":true}},
                    {"id":"shaders","panel":"shaders-panel","heading":"SHADERS","footer":"ESC  BACK","button":"shaders","option":{"label":"SHADERS","default":false}}
                ]
            }"#,
        )
        .unwrap();
        let menu = fs::read_to_string(design.join("menu.rml")).unwrap();
        // We leave shaders out because it is not enabled for this game. The
        // words of the screen come from its declaration, even when we supply a
        // plain panel for this small fixture during staging.
        let (defaults, screens) = apply_options(&design, &menu, None).unwrap();
        assert!(defaults.contains(">OPTIONS<"));
        assert!(defaults.contains(">CONTROLS<"));
        assert!(!defaults.contains("SHADERS"));
        assert!(screens
            .iter()
            .any(|screen| screen.place == ScreenPlace::Options));

        let (both, _) = apply_options(
            &design,
            &menu,
            Some(&["controls".to_string(), "shaders".to_string()]),
        )
        .unwrap();
        assert!(both.contains(">SHADERS<"));
        assert!(both.contains(">CONTROLS<"));
        let shaders_at = both.find("id=\"shaders\"").unwrap();
        let controls_at = both.find("id=\"controls\"").unwrap();
        assert!(
            controls_at < shaders_at,
            "entries follow the design's order"
        );

        let refused = apply_options(&design, &menu, Some(&["nope".to_string()])).unwrap_err();
        assert!(refused.contains("nope"), "{refused}");

        fs::write(
            design.join("option-entry.rml"),
            "<div id=\"BUTTON\" class=\"list-row\">LABEL</div>",
        )
        .unwrap();
        let (templated, _) = apply_options(&design, &menu, Some(&["shaders".to_string()])).unwrap();
        assert!(
            templated.contains("<div id=\"shaders\" class=\"list-row\">SHADERS</div>"),
            "a design's entry template is what gets filled, got {templated}"
        );
        assert!(
            !templated.contains(">CONTROLS<"),
            "controls was not in the set"
        );
    }

    #[test]
    fn an_enabled_entry_requires_the_base_options_screen() {
        let (design, _) = native_menu();
        let mut screens = declared_screens(&design).unwrap();
        screens.retain(|screen| screen.id != "options");
        let error = screens_for_export(&screens, Some(&["controls".into()])).unwrap_err();
        assert!(
            error.contains("Native base must declare an Options screen"),
            "{error}"
        );
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
        let designs = crate::repo::at("integrations/designs");
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
        assert!(
            looked > 0,
            "no design files were read, so this proved nothing"
        );
    }

    /// The disc list is in the document before the core has loaded, because
    /// we cannot create a button in the player afterwards. It starts hidden
    /// and disabled, and it is the last Options entry, so in a one-disc game
    /// there is no gap and Controls stays in place. It is not on the pause
    /// row.
    #[test]
    fn the_disc_list_is_hidden_until_the_core_has_several_images() {
        let (native, menu) = native_menu();
        let screens = declared_screens(&native).expect("native screens");
        let discs = screens
            .iter()
            .find(|screen| screen.id == "discs")
            .expect("native declares a disc list");
        assert_eq!(
            discs.images.as_deref(),
            Some("list"),
            "the disc screen is the list the core fills"
        );
        assert_eq!(discs.mark.as_deref(), Some("IN"), "the current row's word");
        // A game with several discs, for which the export lists the entry.
        let several = ["controls".to_string(), "discs".to_string()];
        let (staged, _) = apply_options(&native, &menu, Some(&several)).expect("several discs");
        let bounds = button_bounds(&staged, "discs").expect("the disc entry is in options");
        let button = &staged[bounds.0..bounds.1];
        assert!(
            button.contains("display: none"),
            "a one-disc game must not show the entry: {button}"
        );
        assert!(
            button.contains("disabled"),
            "a hidden entry is still a focus stop unless it is disabled: {button}"
        );
        let controls = button_bounds(&staged, "controls").expect("controls");
        let controls_button = &staged[controls.0..controls.1];
        assert!(
            controls_button.contains("top: 0dp"),
            "controls stays the first entry: {controls_button}"
        );
        assert!(
            button.contains(&format!("top: {}dp", option_entry_step(&native).unwrap())),
            "the disc entry is the last slot, so hiding it leaves no hole: {button}"
        );
        let actions_at = staged.find("id=\"actions\"").expect("pause row");
        let panel_at = staged.find("id=\"options-panel\"").expect("options");
        assert!(
            !staged[actions_at..panel_at].contains("id=\"discs\""),
            "the pause row stays Continue / Save / Load / Options / Quit"
        );

        let (disc_design, disc_menu) = design_menu("disc");
        let disc_screens = declared_screens(&disc_design).expect("disc screens");
        let circle = disc_screens
            .iter()
            .find(|screen| screen.id == "disc")
            .expect("the circle screen");
        assert_eq!(
            circle.images.as_deref(),
            Some("discs"),
            "several discs open the list from the button the column already has"
        );
        let (disc_staged, _) =
            apply_options(&disc_design, &disc_menu, None).expect("disc defaults");
        assert!(
            disc_staged.contains("id=\"disc-face\""),
            "one disc still opens the circle"
        );
        assert!(
            button_bounds(&disc_staged, "disc").is_some(),
            "the column keeps its DISC button, so it does not grow a gap"
        );
    }
}
