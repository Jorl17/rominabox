//! Generated lists of rows in the menu, such as the achievements list and
//! the shader list.
//!
//! We fill each list when we bundle the game, or live in the player. We make
//! each row by filling in the row template of the menu design with one item.
//! We use this code for every kind of list.
//!
//! A menu has one `<!--SCREENS-->` marker and one `<!--SCREEN-LINKS-->`
//! marker. We replace each marker once, with every list at the same time,
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
    "<div id=\"ROW-ID-title\" class=\"list-row-title\">TITLE</div>",
    "<div id=\"ROW-ID-detail\" class=\"list-row-detail\">DETAIL</div>",
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
    /// No picture and no second line. We fill a bind slot after export, so
    /// an empty string does not mean this, and the caller must say so.
    pub line: bool,
}

/// One screen of rows, ready to write into the menu.
#[derive(Clone, Debug)]
pub struct List {
    pub screen: Screen,
    pub content: ListContent,
}

#[derive(Clone, Debug)]
pub enum ListContent {
    Static(Vec<ListItem>),
    /// We clone this design's row template in the player for paged live data.
    Live,
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

/// A selected screen may frame the shared list parts without replacing their
/// rows, toggle, Back action, or status. The optional base wrapper is in
/// Native, beside the selected package. Without it we use the built-in one.
fn screen_template(design: &Path, id: &str) -> Result<Option<String>, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("Invalid list screen id '{id}'"));
    }
    let name = format!("screen-{id}.rml");
    let selected = design.join(&name);
    let base = crate::themes::base_design(design)?.join(&name);
    let source = if selected.is_file() {
        selected
    } else if base.is_file() {
        base
    } else {
        return Ok(None);
    };
    let template = fs::read_to_string(&source)
        .map_err(|error| format!("Could not read {}: {error}", source.display()))?;
    for required in [
        "id=\"PANEL-ID\"",
        "class=\"screen-panel",
        "<!--ROWS-->",
        "<!--ACTIONS-->",
        "<!--STATUS-->",
    ] {
        if template.matches(required).count() != 1 {
            return Err(format!("{} must contain one {required}", source.display()));
        }
    }
    let panel_at = template.find("id=\"PANEL-ID\"").unwrap();
    let tag_at = template[..panel_at]
        .rfind('<')
        .ok_or("Panel id must belong to an element")?;
    let tag_end = template[panel_at..]
        .find('>')
        .ok_or("Panel opening tag is incomplete")?
        + panel_at;
    let opening = &template[tag_at..tag_end];
    if !opening.contains("display:none") {
        return Err(format!(
            "{} must hide the PANEL-ID element with display:none",
            source.display()
        ));
    }
    Ok(Some(template))
}

/// The extra buttons on the action row of a screen, from `actions-<id>.rml`
/// in the design or in Native, or none when neither has the file. We put them
/// in the one generated strip, because a second strip in the same box would
/// receive every pointer hit instead of the first.
fn screen_actions(design: &Path, id: &str) -> Result<String, String> {
    let name = format!("actions-{id}.rml");
    for package in [design.to_path_buf(), crate::themes::base_design(design)?] {
        let path = package.join(&name);
        if path.is_file() {
            return fs::read_to_string(&path)
                .map_err(|error| format!("Could not read {}: {error}", path.display()));
        }
    }
    Ok(String::new())
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

/// The vertical space of one row, so that a list shorter than a page ends
/// where its content does.
///
/// In the design, the actions and the status are placed for a full page. These
/// two numbers are also in the design's stylesheet. We declare them here so
/// that we can read them in the exporter without parsing CSS.
pub fn row_step(design: &Path) -> Result<usize, String> {
    let path = design.join("design.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(0);
    };
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let at = |key: &str| {
        declared
            .get("list")
            .and_then(|list| list.get(key))
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize
    };
    // Both or neither, because with a height and no gap we would move the
    // actions up by slightly too little each time, which looks like a mistake.
    let (height, gap) = (at("rowHeight"), at("rowGap"));
    if height == 0 {
        return Ok(0);
    }
    Ok(height + gap)
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
    if item.line {
        // The same code for every list. According to the caller, this row has no
        // picture and no second line, so we do not name the disc list here.
        out = out.replacen("class=\"list-row", "class=\"list-row line", 1);
    }
    if item.icon.is_empty() {
        // A row with no picture. We remove the img, because loading a missing
        // texture in RmlUi makes the render fail.
        out = out.replace("<img class=\"list-row-icon\" src=\"\"/>", "");
    }
    if item.line && item.detail.is_empty() {
        let detail = format!(
            "<div id=\"{}-detail\" class=\"list-row-detail\"></div>",
            item.id
        );
        out = out.replace(&detail, "");
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
    let mut html = format!("<div id=\"{screen}-list\" class=\"list\">");
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
        html.push_str(&pager(screen, page_count));
    }
    html.push_str("</div>");
    html
}

fn pager(screen: &str, page_count: usize) -> String {
    let hidden = if page_count < 2 {
        " style=\"display:none;\""
    } else {
        ""
    };
    format!("<div id=\"{screen}-pager\" class=\"list-pager\"{hidden}><button id=\"{screen}-prev\" class=\"menu-action list-pager-prev disabled\">&lt;</button><div id=\"{screen}-page-count\" class=\"list-pager-count\">1/{page_count}</div><button id=\"{screen}-next\" class=\"menu-action list-pager-next\">&gt;</button></div>")
}

/// A live list contains a hidden prototype row from the same template as the
/// static lists. We clone it in the native list component, so there is no
/// second template and no layout specific to achievements.
fn live_list(screen: &str, template: &str, page_size: usize) -> String {
    let row = render_row(
        template,
        &ListItem {
            id: format!("{screen}-prototype"),
            icon: String::new(),
            title: String::new(),
            detail: String::new(),
            state: String::new(),
            selected: false,
            accent: false,
            line: false,
        },
    );
    let pager = pager(screen, 0);
    format!("<div id=\"{screen}-list\" class=\"list live-list\" data-page-size=\"{page_size}\"><div class=\"list-prototype\" style=\"display:none;\">{row}</div>{pager}</div>")
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
    // We also declare a switch on a generated screen here. We write the
    // design's own declarations before this panel exists, and in main we
    // filter out the markup of a screen whose panel is not yet in the
    // document, so the switch's labels would be lost.
    if let Some(toggle) = &screen.toggle {
        if declared_value(&cfg, &format!("toggle_on_{}", toggle.id)).is_none() {
            let declared = crate::themes::toggle_declarations(std::slice::from_ref(screen));
            let (list_at, list_end) = declared_value(&cfg, "toggles")
                .ok_or_else(|| "design.cfg has no toggles list".to_string())?;
            let existing = cfg[list_at..list_end].to_string();
            let ids = if existing.is_empty() {
                toggle.id.clone()
            } else {
                format!("{existing} {}", toggle.id)
            };
            cfg.replace_range(list_at..list_end, &ids);
            for line in declared
                .lines()
                .filter(|line| !line.starts_with("toggles = "))
            {
                cfg.push_str(line);
                cfg.push('\n');
            }
        }
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
    if let Some(images) = &screen.images {
        let key = format!("screen_images_{}", screen.id);
        if declared_value(&cfg, &key).is_none() {
            cfg.push_str(&format!(
                "screen_images_{id} = \"{images}\"\n",
                id = screen.id
            ));
        }
    }
    if let Some(mark) = &screen.mark {
        let key = format!("screen_mark_{}", screen.id);
        if declared_value(&cfg, &key).is_none() {
            cfg.push_str(&format!("screen_mark_{id} = \"{mark}\"\n", id = screen.id));
        }
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
    let default_pages = page_size(design)?;
    let step = row_step(design)?;
    let mut screens = String::new();
    let mut links = String::new();
    for list in lists {
        let pages = list.screen.list_page_size.unwrap_or(default_pages);
        let wrapper = screen_template(design, &list.screen.id)?;
        let (rows, lift) = match &list.content {
            ListContent::Static(items) => {
                if items.is_empty() {
                    continue;
                }
                (
                    render_list(&list.screen.id, &template, items, pages),
                    pages.saturating_sub(items.len().min(pages)) * step,
                )
            }
            ListContent::Live => (live_list(&list.screen.id, &template, pages), 0),
        };
        let back = list
            .screen
            .back_label
            .clone()
            .unwrap_or_else(|| "BACK".into());
        // In the design, the actions are placed for a full page of rows. For a
        // list with fewer rows, we move them up by the missing rows, so the
        // screen ends where its content does. For a full list we do not move
        // them.
        let up = if lift > 0 {
            format!(" style=\"margin-top:-{lift}dp;\"")
        } else {
            String::new()
        };
        if document.contains(&format!("id=\"{}\"", list.screen.panel))
            || screens.contains(&format!("id=\"{}\"", list.screen.panel))
        {
            return Err(format!(
                "List screen '{}' is already in menu.rml; its wrapper belongs at <!--SCREENS-->",
                list.screen.id
            ));
        }
        let toggle = toggle_markup(&list.screen);
        let own = screen_actions(design, &list.screen.id)?;
        let actions = format!(
            "<div class=\"list-actions\"{up}>{toggle}{own}<button class=\"menu-action list-back\" id=\"{id}-back\">{back}</button></div>",
            id = list.screen.id,
            back = rml_text(&back),
        );
        let status = format!(
            "<div id=\"{}-status\" class=\"list-status\"{up}></div>",
            list.screen.id
        );
        let panel = if let Some(template) = wrapper {
            template
                .replace("PANEL-ID", &list.screen.panel)
                .replace("<!--ROWS-->", &rows)
                .replace("<!--ACTIONS-->", &actions)
                .replace("<!--STATUS-->", &status)
        } else {
            format!(
                "<div id=\"{panel}\" class=\"screen-panel\" style=\"display:none;\">{rows}{actions}{status}</div>",
                panel = list.screen.panel,
            )
        };
        screens.push_str(&panel);
        // The player opens an entry inside Options from there, so it has no
        // button on the pause row. We generate the Options entries separately.
        // The player opens an entry inside Options from there. A list whose
        // button is already drawn in the design, such as the disc column's
        // DISC, gets no second one, and for a list without a button of its
        // own we retarget that existing button.
        if list.screen.option_label.is_none()
            && !list.screen.button.is_empty()
            && !document.contains(&format!("id=\"{}\"", list.screen.button))
        {
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
            line: false,
        }
    }

    #[test]
    fn a_row_with_no_picture_and_no_detail_is_one_line() {
        let template = row_template(Path::new("/nonexistent")).expect("built-in row");
        let bare = ListItem {
            id: "discs-0".into(),
            icon: String::new(),
            title: "Ape Escape".into(),
            detail: String::new(),
            state: "IN".into(),
            selected: true,
            accent: false,
            line: true,
        };
        let row = render_row(&template, &bare);
        assert!(
            row.contains("class=\"list-row line"),
            "a row with nothing beside the name was still the two-line picture row: {row}"
        );
        assert!(
            !row.contains("list-row-icon"),
            "a row with no picture still reserved one: {row}"
        );
        let pictured = render_row(&template, &item("shader", "Scanlines"));
        assert!(
            !pictured.contains("list-row line"),
            "a shader row lost the picture column: {pictured}"
        );
        let slot = ListItem {
            id: "bind-1".into(),
            icon: String::new(),
            title: String::new(),
            detail: String::new(),
            state: String::new(),
            selected: false,
            accent: false,
            line: false,
        };
        let bind = render_row(&template, &slot);
        assert!(
            bind.contains("id=\"bind-1-detail\""),
            "a bind slot lost the detail it is filled with later: {bind}"
        );
        assert!(
            !bind.contains("list-row line"),
            "an empty bind slot was treated as a one-line row: {bind}"
        );
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
        assert!(
            !single.contains("list-pager"),
            "one page does not grow arrows"
        );
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
                line: false,
            },
        );
        assert!(row.contains(">STATE OF THE ART<"), "{row}");
        assert!(row.contains(">TITLE FIGHT, WITH DETAIL<"), "{row}");
        assert_eq!(row.matches("LOCKED 5 PTS").count(), 1, "{row}");
    }

    #[test]
    fn a_generated_screens_switch_is_declared_with_it() {
        let mut screen = Screen {
            id: "achievements".into(),
            panel: "achievements-panel".into(),
            heading: "ACHIEVEMENTS".into(),
            footer: "ESC  BACK".into(),
            button: "achievements".into(),
            label: None,
            back_label: None,
            list_page_size: None,
            place: ScreenPlace::Plain,
            option_label: Some("ACHIEVEMENTS".into()),
            option_default: false,
            images: None,
            mark: None,
            toggle: None,
        };
        screen.toggle = Some(crate::themes::Toggle {
            id: "achievement-mode".into(),
            label: "ACHIEVEMENT MODE".into(),
            on: "ON".into(),
            off: "OFF".into(),
            default_on: false,
            guard: crate::themes::ToggleGuard::Saves,
            guard_label: "ACHIEVEMENTS ON".into(),
            guard_status: "SAVE SLOTS ARE OFF WHILE ACHIEVEMENTS ARE ON".into(),
        });
        // The design's own declarations before this panel exists. We filter out
        // the screen, and its switch with it.
        let cfg = "screens = \"pause options controls\"\nscreen_panel_pause = \"pause-panel\"\nscreen_button_pause = \"options-back\"\ntoggles = \"\"\n";
        let out = declare_screen(cfg, &screen, None).expect("declared");
        assert!(out.contains("toggles = \"achievement-mode\""), "{out}");
        assert!(out.contains("toggle_on_achievement-mode = \"ON\""), "{out}");
        assert!(
            out.contains("toggle_guard_achievement-mode = \"saves\""),
            "{out}"
        );
        assert!(
            out.contains("toggle_guard_status_achievement-mode = \"SAVE SLOTS ARE OFF WHILE ACHIEVEMENTS ARE ON\""),
            "{out}"
        );
        // When we declare the same screen again, it must not appear twice.
        let again = declare_screen(&out, &screen, None).expect("declared");
        assert_eq!(
            again.matches("toggle_on_achievement-mode = ").count(),
            1,
            "{again}"
        );
        assert_eq!(
            again.matches("achievement-mode achievement-mode").count(),
            0,
            "{again}"
        );
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
            list_page_size: None,
            place: ScreenPlace::Plain,
            option_label: Some("ACHIEVEMENTS".into()),
            option_default: false,
            images: None,
            mark: None,
            toggle: None,
        };
        let cfg = "screens = \"pause achievements\"\nscreen_panel_achievements = \"achievements-panel\"\nscreen_heading_achievements = \"ACHIEVEMENTS\"\nscreen_footer_achievements = \"ESC  BACK\"\nscreen_button_achievements = \"achievements\"\nscreen_panel_pause = \"pause-panel\"\nscreen_button_pause = \"options-back\"\n";
        let out =
            declare_screen(cfg, &screen, Some(("pause", "achievements-back"))).expect("declared");
        assert_eq!(out.matches("screen_panel_achievements = ").count(), 1);
        assert!(out.contains("screen_button_pause = \"options-back achievements-back\""));
    }
}
