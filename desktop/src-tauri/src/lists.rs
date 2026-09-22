//! Generated lists of rows in the menu, such as the achievements list and
//! the shader list.
//!
//! We make each row by filling in the row template of the menu design with
//! the id, icon, title, detail and state of one item, when we bundle the
//! game, because no elements can be created in the player while it runs. We
//! use this code for every kind of list.
//!
//! A menu has one `<!--SCREENS-->` marker, where we put the list screens,
//! and one `<!--SCREEN-LINKS-->` marker, where we put the buttons that open
//! them. We replace each marker once, with every list at the same time,
//! because the marker is gone after the first replacement.

use crate::themes::{Screen, ScreenPlace};
use std::fs;
use std::path::Path;

/// Where we insert the rows of a generated screen and the button that opens
/// it. There is one marker for each, shared by every list. Without both
/// markers, we cannot show a generated list in a design.
pub const SCREENS_SLOT: &str = "<!--SCREENS-->";
pub const LINKS_SLOT: &str = "<!--SCREEN-LINKS-->";

/// The row template for a design without its own `row.rml`.
///
/// The holes are the contract: `ROW-ID`, `ICON`, `TITLE`, `DETAIL`, `STATE`,
/// `SELECTED`. The class stays `list-row`, and the id of the state element is
/// `ROW-ID-state`, so we can mark the active item in the player whatever the
/// design of the row.
const BUILT_IN_ROW: &str = concat!(
    "<button id=\"ROW-ID\" class=\"list-row SELECTED\">",
    "<img class=\"list-row-icon\" src=\"ICON\"/>",
    "<div class=\"list-row-title\">TITLE</div>",
    "<div class=\"list-row-detail\">DETAIL</div>",
    "<div id=\"ROW-ID-state\" class=\"list-row-state\">STATE</div>",
    "</button>\n",
);

/// One row, for example for a shader or for an achievement.
#[derive(Clone, Debug)]
pub struct ListItem {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub detail: String,
    pub state: String,
    /// The row in effect, such as the shader that is on. We mark it `selected`.
    pub selected: bool,
    /// A row we single out in its list, drawn as the design styles `accent`.
    /// In an achievement list, we mark the game's win condition.
    pub accent: bool,
}

/// One screen of rows, ready to write into the menu.
#[derive(Clone, Debug)]
pub struct List {
    pub screen: Screen,
    pub items: Vec<ListItem>,
}

/// The row template of the design, or the built-in one. The markup in a
/// design's `row.rml` can be anything, but it still has to be a row.
pub fn row_template(design: &Path) -> Result<String, String> {
    let path = design.join("row.rml");
    if !path.is_file() {
        return Ok(BUILT_IN_ROW.to_string());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read the row template: {error}"))?;
    if !text.contains("ROW-ID") || !text.contains("list-row") {
        return Err("a row template must keep the ROW-ID hole and the list-row class".into());
    }
    Ok(text)
}

/// How many rows fit on one page, as declared in the design. When it is absent,
/// we use four, which fit under the Native heading and above its back button.
pub fn page_size(design: &Path) -> Result<usize, String> {
    let path = design.join("design.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(4);
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    match declared
        .get("list")
        .and_then(|list| list.get("pageSize"))
        .and_then(|value| value.as_u64())
    {
        None => Ok(4),
        Some(0) => Err("list.pageSize must be at least 1".into()),
        Some(size) => Ok(size as usize),
    }
}

pub fn rml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fill the row template in one pass.
///
/// One `String::replace` per placeholder is wrong, because each call reads
/// the result of the previous one. The placeholders are ordinary English
/// words and the data comes from a service, so the title "State of the Art"
/// would turn into the word on the right of its row.
pub fn render_row(template: &str, item: &ListItem) -> String {
    let mut marks: Vec<&str> = Vec::new();
    if item.selected {
        marks.push("selected");
    }
    if item.accent {
        marks.push("accent");
    }
    let selected = marks.join(" ");
    let title = rml_text(&item.title);
    let detail = rml_text(&item.detail);
    let state = rml_text(&item.state);
    // Longest first, so we replace a placeholder that starts with another.
    let holes: [(&str, &str); 6] = [
        ("SELECTED", selected.as_str()),
        ("ROW-ID", item.id.as_str()),
        ("DETAIL", detail.as_str()),
        ("TITLE", title.as_str()),
        ("STATE", state.as_str()),
        ("ICON", item.icon.as_str()),
    ];
    let mut out = String::with_capacity(template.len() + 64);
    let bytes = template.as_bytes();
    let mut at = 0;
    'outer: while at < bytes.len() {
        for (hole, value) in &holes {
            if template[at..].starts_with(hole) {
                out.push_str(value);
                at += hole.len();
                continue 'outer;
            }
        }
        let next = template[at..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| at + offset)
            .unwrap_or(bytes.len());
        out.push_str(&template[at..next]);
        at = next;
    }
    out
}

/// One list, with the same template on every page, and a pager only when the
/// items do not fit on one page. On page one the back arrow is disabled,
/// and we move that mark in the player as the page turns. Page and arrow ids
/// are `{screen}-page-N`, `{screen}-prev`, `{screen}-next` and `{screen}-page-count`.
pub fn render_list(screen: &str, template: &str, items: &[ListItem], page_size: usize) -> String {
    if items.is_empty() || page_size == 0 {
        return String::new();
    }
    let page_count = items.len().div_ceil(page_size);
    let mut html = String::from("<div class=\"list\">");
    for (index, chunk) in items.chunks(page_size).enumerate() {
        let hidden = if index == 0 {
            ""
        } else {
            " style=\"display:none;\""
        };
        html.push_str(&format!(
            "<div id=\"{screen}-page-{}\" class=\"list-page\"{hidden}>",
            index + 1
        ));
        for item in chunk {
            html.push_str(&render_row(template, item));
        }
        html.push_str("</div>");
    }
    if page_count > 1 {
        html.push_str(&format!(
            "<div class=\"list-pager\"><button id=\"{screen}-prev\" class=\"menu-action list-pager-prev disabled\">&lt;</button><div id=\"{screen}-page-count\" class=\"list-pager-count\">1/{page_count}</div><button id=\"{screen}-next\" class=\"menu-action list-pager-next\">&gt;</button></div>"
        ));
    }
    html.push_str("</div>");
    html
}

pub fn fill_slot(document: &str, slot: &str, body: &str) -> Result<String, String> {
    let count = document.matches(slot).count();
    if count == 0 {
        if body.is_empty() {
            return Ok(document.to_string());
        }
        return Err(format!("the menu has no {slot} slot for a list to go into"));
    }
    if count != 1 {
        return Err(format!("{slot} appears {count} times; a slot is one place"));
    }
    Ok(document.replace(slot, body))
}

fn declared_value(cfg: &str, key: &str) -> Option<(usize, usize)> {
    let marker = format!("{key} = \"");
    let start = cfg.find(&marker)?;
    let value_at = start + marker.len();
    let end = cfg[value_at..].find('"')?;
    Some((value_at, value_at + end))
}

/// Append a screen that is not drawn in the design, and optionally a second
/// button that opens a screen already declared (a list's BACK opens the screen
/// the player came from).
///
/// A screen declared in the design is already in this file. Writing it again
/// would leave two of every key, so we add only the missing parts.
pub fn declare_screen(
    cfg: &str,
    screen: &Screen,
    also_opens: Option<(&str, &str)>,
) -> Result<String, String> {
    let mut cfg = cfg.to_string();
    let Some((value_at, value_end)) = declared_value(&cfg, "screens") else {
        return Err("design.cfg has no screens list".into());
    };
    let existing = cfg[value_at..value_end].to_string();
    if !existing.split_whitespace().any(|id| id == screen.id) {
        let next = if existing.is_empty() {
            screen.id.clone()
        } else {
            format!("{existing} {}", screen.id)
        };
        cfg.replace_range(value_at..value_end, &next);
    }
    if declared_value(&cfg, &format!("screen_panel_{}", screen.id)).is_none() {
        cfg.push_str(&format!(
            "screen_panel_{id} = \"{panel}\"\nscreen_heading_{id} = \"{heading}\"\nscreen_footer_{id} = \"{footer}\"\nscreen_button_{id} = \"{button}\"\n",
            id = screen.id,
            panel = screen.panel,
            heading = screen.heading,
            footer = screen.footer,
            button = screen.button,
        ));
    }
    if let Some((host, button)) = also_opens {
        let key = format!("screen_button_{host}");
        let Some((value_at, value_end)) = declared_value(&cfg, &key) else {
            return Err(format!("design.cfg has no button for screen {host}"));
        };
        let existing = cfg[value_at..value_end].to_string();
        if !existing.split_whitespace().any(|id| id == button) {
            let next = if existing.is_empty() {
                button.to_string()
            } else {
                format!("{existing} {button}")
            };
            cfg.replace_range(value_at..value_end, &next);
        }
    }
    Ok(cfg)
}

/// The screen to which BACK leads from a list: the one whose button opens it.
///
/// From an entry inside Options, BACK leads to Options. From anything else it
/// leads to the screen behind every screen, the first declared screen that is
/// not an entry.
fn host_of<'a>(staged: &'a [Screen], list: &Screen) -> Option<&'a Screen> {
    if list.option_label.is_some() {
        return staged
            .iter()
            .find(|screen| screen.place == ScreenPlace::Options);
    }
    staged
        .iter()
        .find(|screen| screen.place == ScreenPlace::Plain && screen.option_label.is_none())
}

fn toggle_markup(screen: &Screen) -> String {
    let Some(toggle) = &screen.toggle else {
        return String::new();
    };
    format!(
        "<button class=\"menu-action list-toggle\" id=\"{id}\"><span class=\"list-toggle-label\">{label}</span><span id=\"{id}-state\" class=\"list-toggle-state\">{word}</span></button>",
        id = toggle.id,
        label = rml_text(&toggle.label),
        word = rml_text(if toggle.default_on { &toggle.on } else { &toggle.off }),
    )
}

/// Write every generated list into the staged menu.
///
/// We fill both markers once, with the markup of every list, because each
/// marker appears only once. We declare the screens where the player reads
/// them, and add each list's BACK to the buttons of the screen it returns to.
pub fn install(
    design: &Path,
    menu_assets: &Path,
    staged_screens: &[Screen],
    lists: &[List],
) -> Result<(), String> {
    let document_path = menu_assets.join("menu.rml");
    let config_path = menu_assets.join("design.cfg");
    let document = fs::read_to_string(&document_path)
        .map_err(|error| format!("could not read the staged menu: {error}"))?;
    let mut design_cfg = fs::read_to_string(&config_path)
        .map_err(|error| format!("could not read the staged screen list: {error}"))?;

    let template = row_template(design)?;
    let pages = page_size(design)?;
    let mut screens = String::new();
    let mut links = String::new();
    for list in lists {
        if list.items.is_empty() {
            continue;
        }
        let rows = render_list(&list.screen.id, &template, &list.items, pages);
        let back = list.screen.back_label.clone().unwrap_or_else(|| "BACK".into());
        screens.push_str(&format!(
            "<div id=\"{panel}\" class=\"screen-panel\" style=\"display:none;\">{rows}<div class=\"list-actions\">{toggle}<button class=\"menu-action list-back\" id=\"{id}-back\">{back}</button></div><div id=\"{id}-status\" class=\"list-status\"></div></div>",
            panel = list.screen.panel,
            id = list.screen.id,
            toggle = toggle_markup(&list.screen),
            back = rml_text(&back),
        ));
        // The player opens an entry inside Options from there, so it has no
        // button on the pause row. We generate the Options entries separately.
        if list.screen.option_label.is_none() {
            links.push_str(&format!(
                "<button class=\"menu-action screen-link\" id=\"{button}\">{heading}</button>",
                button = list.screen.button,
                heading = rml_text(&list.screen.heading),
            ));
        }
        let host = host_of(staged_screens, &list.screen).map(|screen| screen.id.clone());
        let back_button = format!("{}-back", list.screen.id);
        design_cfg = declare_screen(
            &design_cfg,
            &list.screen,
            host.as_deref().map(|host| (host, back_button.as_str())),
        )?;
    }

    let document = fill_slot(&document, LINKS_SLOT, &links)?;
    let document = fill_slot(&document, SCREENS_SLOT, &screens)?;
    fs::write(&document_path, document).map_err(|error| error.to_string())?;
    fs::write(&config_path, &design_cfg).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, title: &str) -> ListItem {
        ListItem {
            id: id.into(),
            icon: format!("{id}/icon.png"),
            title: title.into(),
            detail: "detail".into(),
            state: String::new(),
            selected: false,
            accent: false,
        }
    }

    #[test]
    fn a_list_is_the_one_row_template_however_many_items_it_has() {
        let template = row_template(Path::new("/nonexistent")).expect("built-in row");
        let rows = render_list("shaders", &template, &[item("a", "A"), item("b", "B")], 8);
        let other = render_list(
            "achievements",
            &template,
            &[item("a", "A"), item("b", "B"), item("c", "C")],
            8,
        );
        assert_eq!(rows.matches("class=\"list-row ").count(), 2);
        assert_eq!(other.matches("class=\"list-row ").count(), 3);
    }

    #[test]
    fn more_rows_than_fit_page_with_a_visible_count() {
        let template = row_template(Path::new("/nonexistent")).expect("built-in row");
        let paged = render_list("x", &template, &[item("a", "A"), item("b", "B")], 1);
        assert!(paged.contains("x-page-2"));
        assert!(paged.contains("1/2"));
        let single = render_list("x", &template, &[item("a", "A")], 4);
        assert!(!single.contains("list-pager"), "one page does not grow arrows");
    }

    #[test]
    fn a_title_that_contains_a_hole_is_still_that_title() {
        let template = row_template(Path::new("/nonexistent")).expect("built-in row");
        let row = render_row(
            &template,
            &ListItem {
                id: "achievement-1".into(),
                icon: "a.png".into(),
                title: "STATE OF THE ART".into(),
                detail: "TITLE FIGHT, WITH DETAIL".into(),
                state: "LOCKED 5 PTS".into(),
                selected: false,
                accent: false,
            },
        );
        assert!(row.contains(">STATE OF THE ART<"), "{row}");
        assert!(row.contains(">TITLE FIGHT, WITH DETAIL<"), "{row}");
        assert_eq!(row.matches("LOCKED 5 PTS").count(), 1, "{row}");
    }

    #[test]
    fn declaring_a_screen_twice_leaves_one_of_each_key() {
        let screen = Screen {
            id: "achievements".into(),
            panel: "achievements-panel".into(),
            heading: "ACHIEVEMENTS".into(),
            footer: "ESC  BACK".into(),
            button: "achievements".into(),
            label: None,
            back_label: None,
            place: ScreenPlace::Plain,
            option_label: Some("ACHIEVEMENTS".into()),
            option_default: false,
            toggle: None,
        };
        let cfg = "screens = \"pause achievements\"\nscreen_panel_achievements = \"achievements-panel\"\nscreen_heading_achievements = \"ACHIEVEMENTS\"\nscreen_footer_achievements = \"ESC  BACK\"\nscreen_button_achievements = \"achievements\"\nscreen_panel_pause = \"pause-panel\"\nscreen_button_pause = \"options-back\"\n";
        let out = declare_screen(cfg, &screen, Some(("pause", "achievements-back")))
            .expect("declared");
        assert_eq!(out.matches("screen_panel_achievements = ").count(), 1);
        assert!(out.contains("screen_button_pause = \"options-back achievements-back\""));
    }
}
