//! The menu document: the page skeleton of Native with the fragments of the
//! design, the Options screen for this game, the volume control, the shared
//! part stylesheets and the state classes for the design to style.
//!
//! We work on text here, and write nothing until the composition is
//! complete.

use super::manifest::{Manifest, Screen, ScreenPlace, ScreenRole};
use super::{contract, words};
use crate::player_settings::{Kind, PlayerSetting};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// The id of BACK on the built-in Options screen, which we make a button of
/// the screen behind Options.
const OPTIONS_BACK: &str = "options-back";

/// Which screens we put in a game.
///
/// `chosen` is the set for the export. Without it, each entry has its
/// default. An empty set means a game with no Options button. We refuse an
/// id that is not an entry in this design, instead of ignoring it.
pub(crate) fn staged_screens(
    screens: &[Screen],
    chosen: Option<&[String]>,
    discs: usize,
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
    // QUICK SIGN IN's accounts come with the achievements screen.
    let achievements_shipped = screens.iter().any(|screen| {
        screen.role == Some(ScreenRole::Achievements) && included.contains(screen.id.as_str())
    });
    let mut staged: Vec<Screen> = screens
        .iter()
        .filter(|screen| match screen.place {
            ScreenPlace::Options => show_options,
            // The disc list exists only for a game of several discs.
            ScreenPlace::Plain if screen.is_disc_list() && discs <= 1 => false,
            ScreenPlace::Plain if screen.role == Some(ScreenRole::Accounts) => achievements_shipped,
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
    // When someone leaves Options, we show the screen behind it, the first
    // screen that is not itself an entry. The generated back button is a
    // button of that screen, because in the player we know only "the button
    // that shows a screen".
    if show_options {
        if let Some(behind) = staged
            .iter_mut()
            .find(|screen| screen.place == ScreenPlace::Plain && screen.option_label.is_none())
        {
            behind.button = OPTIONS_BACK.to_string();
        }
    }
    Ok(staged)
}

/// The page skeleton of Native with the chrome and screen fragments of the
/// design. The extra panels are in the `screen-order.rml` of the design, and
/// we draw only those for this game. An export with no Options entries has
/// no Options panel, instead of one that nobody can reach.
pub(crate) fn skeleton(manifest: &Manifest, staged: &[Screen]) -> Result<String, String> {
    let skeleton = manifest.base.join(&manifest.documents.menu);
    let mut menu = fs::read_to_string(&skeleton)
        .map_err(|e| format!("Could not read {}: {e}", skeleton.display()))?;
    for (slot, name) in [
        ("<!--SPINE-->", "spine.rml"),
        ("<!--FOOTER-->", "footer.rml"),
        ("<!--SCREEN:pause-->", "screen-pause.rml"),
        ("<!--SCREEN:controls-->", "screen-controls.rml"),
    ] {
        let value = if manifest.has_fragment(name) {
            manifest.fragment(name)?
        } else {
            String::new()
        };
        if !menu.contains(slot) {
            return Err(format!("Native menu has no {slot} insertion point"));
        }
        menu = menu.replace(slot, &value);
    }
    // The other screens of Native, at their places in its skeleton. We draw
    // each one only in a game that has it.
    menu = place_screens(manifest, &menu, staged, &skeleton)?;
    let order = manifest.design.join("screen-order.rml");
    let extra = if order.is_file() {
        fs::read_to_string(&order)
            .map_err(|e| format!("Could not read {}: {e}", order.display()))?
    } else {
        String::new()
    };
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
        if !manifest.screens.iter().any(|screen| screen.id == id) {
            return Err(format!(
                "Screen '{id}' in {} is not declared",
                order.display()
            ));
        }
        if !placed.insert(id) {
            return Err(format!("Screen '{id}' occurs twice in {}", order.display()));
        }
        if staged.iter().any(|screen| screen.id == id) {
            expanded.push_str(&manifest.fragment(&format!("screen-{id}.rml"))?);
        }
        remaining = &after[end + "-->".len()..];
    }
    expanded.push_str(remaining);
    if !menu.contains("<!--EXTRA-SCREENS-->") {
        return Err("Native menu has no extra-screen insertion point".into());
    }
    let slots = manifest.fragment("save-slots.rml")?;
    Ok(menu
        .replace("<!--EXTRA-SCREENS-->", &expanded)
        .replace("<!--SAVE-SLOTS-->", &slots))
}

/// `markup` with each `<!--SCREEN:id-->` in it replaced by that screen's
/// fragment when the game has the screen, and by nothing when it does not.
fn place_screens(
    manifest: &Manifest,
    markup: &str,
    staged: &[Screen],
    source: &Path,
) -> Result<String, String> {
    let mut placed = String::new();
    let mut remaining = markup;
    while let Some(start) = remaining.find("<!--SCREEN:") {
        placed.push_str(&remaining[..start]);
        let after = &remaining[start + "<!--SCREEN:".len()..];
        let end = after
            .find("-->")
            .ok_or_else(|| format!("Unclosed screen in {}", source.display()))?;
        let id = &after[..end];
        if !manifest.screens.iter().any(|screen| screen.id == id) {
            return Err(format!("Screen '{id}' in {} is not declared", source.display()));
        }
        if staged.iter().any(|screen| screen.id == id) {
            placed.push_str(&manifest.fragment(&format!("screen-{id}.rml"))?);
        }
        remaining = &after[end + "-->".len()..];
    }
    placed.push_str(remaining);
    Ok(placed)
}

/// The page as it appears when the menu opens on Pause. We write the heading
/// declared for Pause into the heading, and its footer, when it has one, into
/// the footer hint, as we do at run time when Pause appears. A preview, an
/// offscreen picture and the first screen of the game then show the same
/// words, from one declaration.
pub(crate) fn opening_screen(manifest: &Manifest, document: &str) -> Result<String, String> {
    let pause = manifest.screen(ScreenRole::Pause).ok_or_else(|| {
        format!(
            "design '{}' declares no Pause screen to open on",
            manifest.id
        )
    })?;
    let document = set_text(document, contract!(Heading), &pause.heading)?;
    if pause.footer.is_empty() {
        return Ok(document);
    }
    set_text(&document, contract!(FooterHint), &pause.footer)
}

/// `document` with everything inside the element `id` replaced by `text`,
/// as we write words into an element in the player.
fn set_text(document: &str, id: &str, text: &str) -> Result<String, String> {
    let missing = || format!("the menu has no element #{id} to write \"{text}\" into");
    let at = document.find(&format!("id=\"{id}\"")).ok_or_else(missing)?;
    let start = document[..at].rfind('<').ok_or_else(missing)?;
    let end = at + document[at..].find('>').ok_or_else(missing)?;
    let name: String = document[start + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let text = crate::lists::rml_text(text);
    let mut out = document.to_string();
    if document[..end].ends_with('/') {
        out.replace_range(end - 1..=end, &format!(">{text}</{name}>"));
        return Ok(out);
    }
    // Its matching closing tag, since it may contain elements of the same name.
    let (opening, closing) = (format!("<{name}"), format!("</{name}>"));
    let mut depth = 1;
    let mut cursor = end + 1;
    let close = loop {
        let next_close = cursor
            + document[cursor..]
                .find(&closing)
                .ok_or_else(|| format!("#{id} is never closed"))?;
        let next_open = document[cursor..next_close]
            .match_indices(&opening)
            .map(|(index, _)| cursor + index)
            .find(|&index| {
                document[index + opening.len()..]
                    .starts_with(|c: char| c == '>' || c == '/' || c.is_whitespace())
            });
        match next_open {
            Some(open) => {
                let tag_end = open + document[open..].find('>').ok_or_else(missing)?;
                if !document[..tag_end].ends_with('/') {
                    depth += 1;
                }
                cursor = tag_end + 1;
            }
            None if depth == 1 => break next_close,
            None => {
                depth -= 1;
                cursor = next_close + closing.len();
            }
        }
    };
    out.replace_range(end + 1..close, &text);
    Ok(out)
}

pub(crate) fn button_bounds(document: &str, id: &str) -> Option<(usize, usize)> {
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

/// One Options entry in the design's entry template. We place the entries
/// with the design's stylesheet and write no positions. `label` is markup.
fn entry_markup(manifest: &Manifest, button: &str, label: &str) -> Result<String, String> {
    let template_path = manifest.design.join("option-entry.rml");
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|e| format!("Could not read {}: {e}", template_path.display()))?
    } else {
        format!(
            "<button class=\"{} {}\" id=\"BUTTON\"><span class=\"option-label\">LABEL</span></button>",
            contract!(MenuAction),
            contract!(OptionEntry)
        )
    };
    Ok(template
        .replace("BUTTON", button)
        .replace("LABEL", label))
}

/// A screen's Options entry.
fn entry_button(manifest: &Manifest, screen: &Screen) -> Result<String, String> {
    let label = screen.option_label.clone().unwrap_or_default();
    let button = entry_markup(manifest, &screen.button, &crate::lists::rml_text(&label))?;
    // We get the count from the core after the game has loaded, so the entry
    // must be in the document already. It starts hidden. A display:none
    // button can still get the focus unless it is disabled too.
    Ok(if screen.is_disc_list() {
        add_attributes(
            &button,
            &screen.button,
            &[("disabled", "disabled"), ("style", "display: none;")],
        )
    } else {
        button
    })
}

/// A switch that the design does not place is one more Options entry, drawn
/// like the others: its name, then an empty state that we fill in the player,
/// with the control's id and the contract's state suffix. We mark it with the
/// contract's switch class, which is how we recognise a press in the player.
fn switch_entry(manifest: &Manifest, setting: &PlayerSetting) -> Result<String, String> {
    let control = setting.control();
    let label = format!(
        "{} <span id=\"{control}{}\" class=\"setting-state\"></span>",
        crate::lists::rml_text(&words::say(&manifest.words, setting.label, &[])),
        contract!(StateSuffix),
    );
    Ok(add_class(
        &entry_markup(manifest, &control, &label)?,
        &control,
        contract!(Switch),
    ))
}

/// Rewrite the menu so that Options has exactly the entries for this game.
///
/// We cannot create elements in the player while it runs, so the buttons
/// must be in the document. When a design has an options panel, we add the
/// entries to it. Otherwise we add the built-in panel, with the button class
/// of the design.
pub(crate) fn apply_options(
    manifest: &Manifest,
    document: &str,
    staged: &[Screen],
    settings: &[PlayerSetting],
) -> Result<String, String> {
    let options = staged
        .iter()
        .find(|screen| screen.place == ScreenPlace::Options);
    let included: Vec<&Screen> = staged
        .iter()
        .filter(|screen| screen.option_label.is_some())
        .collect();
    // Every entry in the design, so that we take a disabled one off the pause
    // row instead of leaving it there as a separate button.
    let declared_entries: Vec<&Screen> = manifest
        .screens
        .iter()
        .filter(|screen| screen.option_label.is_some())
        .collect();
    // The panel in the design for Options, which contains the design's
    // version of an entry.
    let panel_id = manifest
        .screens
        .iter()
        .find(|screen| screen.place == ScreenPlace::Options)
        .map(|screen| format!("id=\"{}\"", screen.panel));
    let in_options =
        |document: &str, at: usize| panel_id.as_ref().is_some_and(|id| document[..at].contains(id));

    let mut document = document.to_string();
    let Some(options) = options else {
        for entry in &declared_entries {
            if let Some((start, end)) = button_bounds(&document, &entry.button) {
                if !in_options(&document, start) {
                    document.replace_range(start..end, "");
                }
            }
        }
        return Ok(document);
    };
    let opener_label = options
        .label
        .clone()
        .unwrap_or_else(|| options.heading.clone());
    let opener = format!(
        "<button class=\"{}\" id=\"{}\">{}</button>",
        contract!(MenuAction),
        options.button,
        crate::lists::rml_text(&opener_label)
    );
    let mut placed_opener = document.contains(&format!("id=\"{}\"", options.button));
    for entry in &declared_entries {
        // A button already inside the options panel is the design's version
        // of that entry. We move a top-level button on the pause row into the
        // options panel.
        let Some((start, end)) = button_bounds(&document, &entry.button) else {
            continue;
        };
        if in_options(&document, start) {
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
        if let Some(quit) = button_bounds(&document, contract!(Quit)) {
            document.insert_str(quit.0, &opener);
        }
    }

    let mut entries = String::new();
    for entry in &included {
        // We removed the pause-row button when we replaced the opener, so an id
        // still in the document is the design's version of the entry.
        if document.contains(&format!("id=\"{}\"", entry.button)) {
            continue;
        }
        entries.push_str(&entry_button(manifest, entry)?);
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
        let shell = if manifest.has_fragment(&name) {
            manifest.fragment(&name)?
        } else {
            let back = options.back_label.clone().unwrap_or_else(|| "BACK".into());
            format!(
                "<div {panel_id} class=\"{panel_class}\" style=\"display:none;\"><div id=\"options-entries\"><!--OPTIONS--></div><button class=\"{action} {options_back}\" id=\"{OPTIONS_BACK}\">{back}</button></div>",
                panel_class = contract!(ScreenPanel),
                action = contract!(MenuAction),
                options_back = contract!(OptionsBack),
                back = crate::lists::rml_text(&back),
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
        let footer = format!("<div id=\"{}\">", contract!(Footer));
        let Some(at) = document.find(&footer) else {
            return Err("menu.rml has no footer, so the options screen has nowhere to go".into());
        };
        document.insert_str(at, &shell);
    }
    // After the screens, in the same column, the switches that the design
    // does not place with a marker in the panel.
    for setting in settings {
        if !matches!(setting.kind, Kind::Switch { .. })
            || document.contains(&setting_slot(setting))
            || document.contains(&format!("id=\"{}\"", setting.control()))
        {
            continue;
        }
        entries.push_str(&switch_entry(manifest, setting)?);
    }
    // One list, as on every screen of rows. In the player we split the
    // entries into pages of the size the design sets for Options, and show
    // the pager when there is more than one page.
    let entries = crate::lists::paged(
        &crate::lists::list_id(&options.id),
        "",
        &options.id,
        &entries,
        options.list_page_size.unwrap_or(manifest.list_page_size),
    );
    if document.contains("<!--OPTIONS-->") {
        document = document.replace("<!--OPTIONS-->", &entries);
    } else if document.contains(&panel_id) {
        document = document.replacen(
            "id=\"options-entries\">",
            &format!("id=\"options-entries\">{entries}"),
            1,
        );
    }
    Ok(document)
}

/// The element that contains a level's slider, arrows, ends and name.
fn level_holder(setting: &PlayerSetting) -> String {
    format!("{}-control", setting.id)
}

/// The marker in a design for a player setting, instead of its place in
/// Options.
pub fn setting_slot(setting: &PlayerSetting) -> String {
    format!("<!--SETTING:{}-->", setting.id)
}

/// Where the shared parts are: beside the `designs` directory, in the
/// repository and in a runtime kit alike.
pub(crate) fn parts_root(design: &Path) -> Result<PathBuf, String> {
    let parts = design
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("parts"))
        .ok_or_else(|| format!("{} is not inside a designs directory", design.display()))?;
    if !parts.is_dir() {
        return Err(format!(
            "the shared menu parts are missing at {}: they are staged beside the designs",
            parts.display()
        ));
    }
    Ok(parts)
}

fn has_class(template: &str, class: &str) -> bool {
    template.split("class=\"").skip(1).any(|rest| {
        rest.split('"')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .any(|token| token == class)
    })
}

/// A part's markup: the design's own `parts/<name>.rml`, or the shared one.
fn part_template(design: &Path, name: &str) -> Result<String, String> {
    let override_path = design.join("parts").join(format!("{name}.rml"));
    let path = if override_path.is_file() {
        override_path
    } else {
        parts_root(design)?.join(format!("{name}.rml"))
    };
    fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))
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

/// A level: its name, low, an arrow, the design's slider, an arrow, high, in
/// the design's words `given`.
///
/// The slider is first in the document, so the keyboard focus goes to it and
/// the left and right keys move it. We place the name, the arrows and the
/// ends as set in the design, so the order in the document is not the order
/// a person sees. There is no number, because the ends say what they are.
pub fn level_markup(
    design: &Path,
    given: &BTreeMap<String, String>,
    setting: &PlayerSetting,
) -> Result<String, String> {
    let Kind::Level { .. } = setting.kind else {
        return Err(format!("{} is not a level", setting.id));
    };
    let slider = part_template(design, "slider")?;
    require_classes(
        "slider",
        &slider,
        &[
            contract!(Slider),
            contract!(SliderTrack),
            contract!(SliderFill),
            contract!(SliderThumb),
            contract!(SliderReadout),
        ],
    )?;
    let id = setting.id;
    Ok(format!(
        "<div id=\"{holder}\">{slider}<button id=\"{id}-down\" class=\"{action} {arrow} {down}\">&lt;</button><button id=\"{id}-up\" class=\"{action} {arrow} {up}\">&gt;</button><div id=\"{id}-low\" class=\"volume-end\">{low}</div><div id=\"{id}-high\" class=\"volume-end\">{high}</div><div class=\"volume-name\">{name}</div></div>",
        holder = level_holder(setting),
        action = contract!(MenuAction),
        arrow = contract!(VolumeArrow),
        down = contract!(ArrowDown),
        up = contract!(ArrowUp),
        slider = slider.replace("PART-ID", &setting.control()).replace("LABEL", ""),
        low = crate::lists::rml_text(&words::say(given, "level-low", &[])),
        high = crate::lists::rml_text(&words::say(given, "level-high", &[])),
        name = crate::lists::rml_text(&words::say(given, setting.label, &[])),
    ))
}

/// The volume control, the same in every design, in the design's words
/// `given`.
pub fn volume_control_markup(
    design: &Path,
    given: &BTreeMap<String, String>,
) -> Result<String, String> {
    level_markup(design, given, &crate::player_settings::volume())
}

/// A switch at a marker in the design, drawn with the design's toggle part.
fn switch_markup(
    design: &Path,
    given: &BTreeMap<String, String>,
    setting: &PlayerSetting,
) -> Result<String, String> {
    let toggle = part_template(design, "toggle")?;
    require_classes("toggle", &toggle, &[contract!(Toggle)])?;
    Ok(toggle.replace("PART-ID", &setting.control()).replace(
        "LABEL",
        &crate::lists::rml_text(&words::say(given, setting.label, &[])),
    ))
}

/// What we draw the player's settings with and where: the design with the
/// parts for them, its words, and the panel of the game's Options screen, if
/// the game has one.
pub struct SettingsPlace<'a> {
    pub design: &'a Path,
    pub words: &'a BTreeMap<String, String>,
    pub options_panel: Option<&'a str>,
}

/// Put in Options the player's settings that `apply_options` did not add.
///
/// Where a design has `<!--SETTING:id-->`, we put a level's slider or a
/// switch's toggle from the design there. Otherwise we put a level at the
/// start of the Options panel. A menu with no Options screen has no place
/// for them, so we leave them out, as we leave out the screen itself.
pub fn install_settings(
    document: &str,
    place: &SettingsPlace,
    settings: &[PlayerSetting],
) -> Result<String, String> {
    let SettingsPlace {
        design,
        words,
        options_panel,
    } = *place;
    let mut document = document.to_string();
    let marker = options_panel.map(|panel| format!("id=\"{panel}\""));
    for setting in settings {
        let slot = setting_slot(setting);
        let placed = match setting.kind {
            Kind::Level { .. } => format!("id=\"{}\"", level_holder(setting)),
            Kind::Switch { .. } => format!("id=\"{}\"", setting.control()),
        };
        if document.contains(&placed) {
            document = document.replace(&slot, "");
            continue;
        }
        if document.contains(&slot) {
            let markup = match setting.kind {
                Kind::Level { .. } => level_markup(design, words, setting)?,
                Kind::Switch { .. } => switch_markup(design, words, setting)?,
            };
            document = document.replacen(&slot, &markup, 1);
            continue;
        }
        let Some(at) = marker.as_deref().and_then(|marker| document.find(marker)) else {
            continue;
        };
        if !matches!(setting.kind, Kind::Level { .. }) {
            continue;
        }
        let markup = level_markup(design, words, setting)?;
        let tag_end = document[at..]
            .find('>')
            .map(|end| at + end + 1)
            .ok_or_else(|| "the options panel tag is never closed".to_string())?;
        document.insert_str(tag_end, &markup);
    }
    Ok(document)
}

/// Every shared part stylesheet, by name, in name order. We link them into
/// every composed document before the design's stylesheet, so a design can
/// restyle a part with ordinary rules.
pub(crate) fn part_sheets(design: &Path) -> Result<Vec<(String, String)>, String> {
    let root = parts_root(design)?;
    let mut sheets = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| format!("{}: {e}", root.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "rcss")
            && path.is_file()
        {
            let name = path
                .file_name()
                .expect("a file has a name")
                .to_string_lossy()
                .into_owned();
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
            sheets.push((name, text));
        }
    }
    sheets.sort();
    Ok(sheets)
}

/// The staged stylesheet every document links.
pub const STYLESHEET: &str = "menu.rcss";

/// Where the part stylesheets are staged, beside the document.
pub(crate) const PARTS: &str = "parts";

/// Link each part stylesheet, under `PARTS`, before the design's own.
pub(crate) fn link_parts(document: &str, names: &[String]) -> Result<String, String> {
    let design_link = format!("<link type=\"text/rcss\" href=\"{STYLESHEET}\"/>");
    let Some(at) = document.find(&design_link) else {
        return Err(format!(
            "the document does not link its stylesheet as {design_link}, so the shared parts \
             have nothing to come before"
        ));
    };
    let links: String = names
        .iter()
        .map(|name| format!("<link type=\"text/rcss\" href=\"{PARTS}/{name}\"/>"))
        .collect();
    let mut linked = document.to_string();
    linked.insert_str(at, &links);
    Ok(linked)
}

/// Set each of `attributes` on the element with `id`, when the document has
/// one. We add a style after the style the element already has, so that its
/// declarations take effect. We leave any other attribute that the element
/// already has as it is.
pub(crate) fn add_attributes(document: &str, id: &str, attributes: &[(&str, &str)]) -> String {
    let marker = format!("id=\"{id}\"");
    let mut out = document.to_string();
    let Some(at) = out.find(&marker) else {
        return out;
    };
    let (Some(start), Some(mut end)) = (out[..at].rfind('<'), out[at..].find('>').map(|end| at + end))
    else {
        return out;
    };
    // The attributes the element lacks, in order, written after its id.
    let mut added = String::new();
    for (name, value) in attributes {
        let opening = format!(" {name}=\"");
        match out[start..end].find(&opening) {
            Some(existing) if *name == "style" => {
                let from = start + existing + opening.len();
                let close = from + out[from..end].find('"').unwrap_or(end - from);
                let declarations = out[from..close].trim_end().to_string();
                let joined = if declarations.is_empty() || declarations.ends_with(';') {
                    format!("{declarations} {value}")
                } else {
                    format!("{declarations}; {value}")
                };
                let joined = joined.trim_start();
                end = end + joined.len() - (close - from);
                out.replace_range(from..close, joined);
            }
            Some(_) => {}
            None => added.push_str(&format!(" {name}=\"{value}\"")),
        }
    }
    out.insert_str(at + marker.len(), &added);
    out
}

/// Add `class` to the element with `id`, when the document has one. How to
/// draw a state is up to the stylesheet of the design, and here we only say
/// which state applies.
pub(crate) fn add_class(document: &str, id: &str, class: &str) -> String {
    let marker = format!("id=\"{id}\"");
    let Some(at) = document.find(&marker) else {
        return document.to_string();
    };
    let Some(start) = document[..at].rfind('<') else {
        return document.to_string();
    };
    let Some(end) = document[at..].find('>').map(|end| at + end) else {
        return document.to_string();
    };
    let tag = &document[start..end];
    let mut out = document.to_string();
    if let Some(classes) = tag.find("class=\"") {
        let value = start + classes + "class=\"".len();
        out.insert_str(value, &format!("{class} "));
    } else {
        out.insert_str(at + marker.len(), &format!(" class=\"{class}\""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_replace_everything_inside_the_element_and_nothing_else() {
        assert_eq!(
            set_text("<div id=\"a\">OLD</div><div>B</div>", "a", "NEW").unwrap(),
            "<div id=\"a\">NEW</div><div>B</div>"
        );
        assert_eq!(
            set_text(
                "<span id=\"a\"><span class=\"k\">ESC</span><br/> GO</span><span>B</span>",
                "a",
                "ESC  BACK"
            )
            .unwrap(),
            "<span id=\"a\">ESC  BACK</span><span>B</span>"
        );
        assert_eq!(
            set_text("<div id=\"a\"></div>", "a", "A & <B>").unwrap(),
            "<div id=\"a\">A &amp; &lt;B&gt;</div>"
        );
        assert_eq!(
            set_text("<div id=\"a\"/><p/>", "a", "NEW").unwrap(),
            "<div id=\"a\">NEW</div><p/>"
        );
        assert!(set_text("<div id=\"b\">X</div>", "a", "NEW").is_err());
        assert!(set_text("<div id=\"a\">X", "a", "NEW").is_err());
    }

    #[test]
    fn a_state_class_joins_the_elements_own() {
        assert_eq!(
            add_class("<div id=\"actions\">", "actions", "no-options"),
            "<div id=\"actions\" class=\"no-options\">"
        );
        assert_eq!(
            add_class(
                "<div class=\"row\" id=\"actions\">",
                "actions",
                "no-options"
            ),
            "<div class=\"no-options row\" id=\"actions\">"
        );
        assert_eq!(
            add_class("<div id=\"other\">", "actions", "x"),
            "<div id=\"other\">"
        );
    }

    #[test]
    fn parts_are_linked_before_the_designs_stylesheet() {
        let document = "<head><link type=\"text/rcss\" href=\"menu.rcss\"/></head>";
        let linked = link_parts(document, &["a.rcss".into(), "b.rcss".into()]).unwrap();
        assert_eq!(
            linked,
            "<head><link type=\"text/rcss\" href=\"parts/a.rcss\"/><link type=\"text/rcss\" href=\"parts/b.rcss\"/><link type=\"text/rcss\" href=\"menu.rcss\"/></head>"
        );
        assert!(link_parts("<head></head>", &[]).is_err());
    }
}
