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

use crate::menu::{contract, Manifest, Screen, ScreenPlace};
use std::fs;
use std::path::Path;

/// Where we insert the rows of a generated screen and the button that opens
/// it. There is one marker for each, shared by every list. Without both
/// markers, we cannot show a generated list in a design.
pub const SCREENS_SLOT: &str = "<!--SCREENS-->";
pub const LINKS_SLOT: &str = "<!--SCREEN-LINKS-->";

/// The row template we use when a design has no `row.rml`.
///
/// The placeholders `ROW-ID`, `ICON`, `TITLE`, `DETAIL`, `STATE` and
/// `SELECTED`, the row class and the state suffix are in the contract. The
/// id of the state element is the row id with that suffix, so we can mark
/// the active item in the player without knowing how the design draws rows.
fn built_in_row() -> String {
    format!(
        "<button id=\"ROW-ID\" class=\"{row} SELECTED\">\
         <img class=\"{icon}\" src=\"ICON\"/>\
         <div id=\"ROW-ID{title_id}\" class=\"{title}\">TITLE</div>\
         <div id=\"ROW-ID{detail_id}\" class=\"{detail}\">DETAIL</div>\
         <div id=\"ROW-ID{state_id}\" class=\"{state}\">STATE</div>\
         </button>\n",
        row = contract!(ListRow),
        icon = contract!(ListRowIcon),
        title_id = contract!(TitleSuffix),
        title = contract!(ListRowTitle),
        detail_id = contract!(DetailSuffix),
        detail = contract!(ListRowDetail),
        state_id = contract!(StateSuffix),
        state = contract!(ListRowState),
    )
}

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
        return Ok(built_in_row());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read the row template: {error}"))?;
    if !text.contains("ROW-ID") || !text.contains(contract!(ListRow)) {
        return Err(format!(
            "a row template must keep the ROW-ID hole and the {} class",
            contract!(ListRow)
        ));
    }
    Ok(text)
}

/// A design may frame the shared list parts of a screen without replacing
/// their rows, Back action or status. The optional base wrapper is in Native,
/// beside the design, and we use the built-in one when it is missing.
fn screen_template(manifest: &Manifest, id: &str) -> Result<Option<String>, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("Invalid list screen id '{id}'"));
    }
    let name = format!("screen-{id}.rml");
    if !manifest.has_fragment(&name) {
        return Ok(None);
    }
    let source = manifest.fragment_path(&name);
    let template = manifest.fragment(&name)?;
    let panel_class = format!("class=\"{}", contract!(ScreenPanel));
    for required in [
        "id=\"PANEL-ID\"",
        panel_class.as_str(),
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
fn screen_actions(manifest: &Manifest, id: &str) -> Result<String, String> {
    let name = format!("actions-{id}.rml");
    if manifest.has_fragment(&name) {
        manifest.fragment(&name)
    } else {
        Ok(String::new())
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
        marks.push(contract!(Selected));
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
        let row = format!("class=\"{}", contract!(ListRow));
        out = out.replacen(&row, &format!("{row} {}", contract!(Line)), 1);
    }
    if item.icon.is_empty() {
        // A row with no picture. We remove the img, because loading a missing
        // texture in RmlUi makes the render fail.
        out = out.replace(
            &format!("<img class=\"{}\" src=\"\"/>", contract!(ListRowIcon)),
            "",
        );
    }
    if item.line && item.detail.is_empty() {
        let detail = format!(
            "<div id=\"{}{}\" class=\"{}\"></div>",
            item.id,
            contract!(DetailSuffix),
            contract!(ListRowDetail)
        );
        out = out.replace(&detail, "");
    }
    out
}

/// One list with every row on one page, the page size and a hidden pager.
/// When the menu loads, we split the rows into pages in the player by the
/// same rule as a live list, and show the pager when there is more than one
/// page. The list id is the screen id with the list suffix of the contract.
/// A page has no id, because we find pages by their class. The arrow ids are
/// `{screen}-prev` and `{screen}-next`, and the page count has the screen id
/// with the page-count suffix of the contract.
pub fn render_list(screen: &str, template: &str, items: &[ListItem], page_size: usize) -> String {
    render_list_in(&list_id(screen), "", screen, template, items, page_size)
}

/// The id of a screen's list: the screen id with the contract's list suffix.
pub fn list_id(screen: &str) -> String {
    format!("{screen}{}", contract!(ListSuffix))
}

/// `render_list`, in the list element `id` with `attributes` after its class,
/// for a list that we look up by another id in the player.
pub fn render_list_in(
    id: &str,
    attributes: &str,
    screen: &str,
    template: &str,
    items: &[ListItem],
    page_size: usize,
) -> String {
    let rows: String = items.iter().map(|item| render_row(template, item)).collect();
    paged(id, attributes, screen, &rows, page_size)
}

/// Rows already written, as one list that we page in the player. A page
/// contains its rows, whatever they are. We write the Options entries this
/// way, and they stay the entries of the design instead of list rows.
pub fn paged(id: &str, attributes: &str, screen: &str, rows: &str, page_size: usize) -> String {
    if rows.is_empty() || page_size == 0 {
        return String::new();
    }
    format!(
        "<div id=\"{id}\" class=\"{list}\"{attributes} {page_size_attribute}=\"{page_size}\">\
         <div class=\"{page}\">{rows}</div>{pager}</div>",
        list = contract!(List),
        page_size_attribute = contract!(PageSizeAttribute),
        page = contract!(ListPage),
        pager = pager(screen),
    )
}

/// The pager of a list, hidden and empty. In the player we show it, write
/// its count and mark the arrow that cannot be used.
fn pager(screen: &str) -> String {
    format!(
        "<div id=\"{screen}-pager\" class=\"{pager}\" style=\"display:none;\">\
         <button id=\"{screen}-prev\" class=\"{action} {previous}\">&lt;</button>\
         <div id=\"{screen}{count_id}\" class=\"{count_class}\"></div>\
         <button id=\"{screen}-next\" class=\"{action} {next}\">&gt;</button></div>",
        pager = contract!(ListPager),
        action = contract!(MenuAction),
        previous = contract!(ListPagerPrev),
        count_id = contract!(PageCountSuffix),
        count_class = contract!(ListPagerCount),
        next = contract!(ListPagerNext),
    )
}

/// What we show in a live row while its picture loads: nine cells, laid out
/// as set in the design, which we show while the row has the class
/// `badge-loading` and hide otherwise.
const WAITING: &str = concat!(
    "<div class=\"list-row-wait\"><div class=\"list-row-wait-cells\">",
    "<div></div><div></div><div></div><div></div><div></div>",
    "<div></div><div></div><div></div><div></div>",
    "</div></div>",
);

/// A live list contains a hidden prototype row from the same template as the
/// static lists. We clone it in the native list component, so there is no
/// second template and no layout specific to achievements.
fn live_list(screen: &str, template: &str, page_size: usize) -> String {
    let mut row = render_row(
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
    // Inside the row element, positioned by the design like its picture.
    if let Some(opened) = row.find('>') {
        row.insert_str(opened + 1, WAITING);
    }
    let pager = pager(screen);
    format!(
        "<div id=\"{screen}{list_id}\" class=\"{list} live-list\" {page_size_attribute}=\"{page_size}\">\
         <div class=\"{prototype}\" style=\"display:none;\">{row}</div>{pager}</div>",
        list_id = contract!(ListSuffix),
        list = contract!(List),
        page_size_attribute = contract!(PageSizeAttribute),
        prototype = contract!(ListPrototype),
    )
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

/// The screen to which BACK leads from a list: the one whose button opens it.
///
/// From an entry inside Options, BACK leads to Options. From anything else it
/// leads to the screen behind every screen, the first declared screen that is
/// not an entry.
fn host_of<'a>(staged: &'a [Screen], list: &Screen) -> Option<&'a Screen> {
    if let Some(opener) = &list.opener {
        return staged.iter().find(|screen| &screen.id == opener);
    }
    if list.option_label.is_some() {
        return staged
            .iter()
            .find(|screen| screen.place == ScreenPlace::Options);
    }
    staged
        .iter()
        .find(|screen| screen.place == ScreenPlace::Plain && screen.option_label.is_none())
}

/// A generated screen we wrote into the menu, and the screen to which its
/// BACK leads.
#[derive(Clone, Debug)]
pub struct Installed {
    pub screen: Screen,
    /// The screen to which BACK on this one leads, and the id of that button.
    pub host: Option<(String, String)>,
}

/// Write every generated list into the menu document.
///
/// We fill each marker once, with the markup of every list, because the
/// marker is gone after that. Return what we wrote, for the declarations.
pub fn install(
    manifest: &Manifest,
    document: &str,
    staged_screens: &[Screen],
    lists: &[List],
) -> Result<(String, Vec<Installed>), String> {
    let template = row_template(&manifest.design)?;
    let default_pages = manifest.list_page_size;
    let step = manifest.list_row_step;
    let mut screens = String::new();
    let mut links = String::new();
    let mut installed = Vec::new();
    for list in lists {
        let pages = list.screen.list_page_size.unwrap_or(default_pages);
        let wrapper = screen_template(manifest, &list.screen.id)?;
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
            ListContent::Live => (
                live_list(&list.screen.id, &template, pages),
                0,
            ),
        };
        let back = list
            .screen
            .back_label
            .clone()
            .unwrap_or_else(|| "BACK".into());
        // The design places the actions below a full page of rows. For a list
        // with fewer rows we move the actions up by the missing rows, so the
        // screen ends where its content ends.
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
        let own = screen_actions(manifest, &list.screen.id)?;
        let actions = format!(
            "<div class=\"list-actions\"{up}>{own}<button class=\"{action} {list_back}\" id=\"{id}\">{back}</button></div>",
            action = contract!(MenuAction),
            list_back = contract!(ListBack),
            id = list.screen.back_button(),
            back = rml_text(&back),
        );
        let status = format!(
            "<div id=\"{}{}\" class=\"list-status\"{up}></div>",
            list.screen.id,
            contract!(StatusSuffix)
        );
        let panel = if let Some(template) = wrapper {
            template
                .replace("PANEL-ID", &list.screen.panel)
                .replace("<!--ROWS-->", &rows)
                .replace("<!--ACTIONS-->", &actions)
                .replace("<!--STATUS-->", &status)
        } else {
            format!(
                "<div id=\"{panel}\" class=\"{panel_class}\" style=\"display:none;\">{rows}{actions}{status}</div>",
                panel = list.screen.panel,
                panel_class = contract!(ScreenPanel),
            )
        };
        screens.push_str(&panel);
        for dialog in &list.screen.dialogs {
            screens.push_str(&manifest.fragment(&format!("dialog-{dialog}.rml"))?);
        }
        // We open an entry inside Options from there. When the button of a
        // list is already in the menu, as DISC in the disc column or QUICK
        // SIGN IN on the achievements screen composed just before, we add no
        // second one.
        let drawn = format!("id=\"{}\"", list.screen.button);
        if list.screen.option_label.is_none()
            && !list.screen.button.is_empty()
            && !document.contains(&drawn)
            && !screens.contains(&drawn)
        {
            links.push_str(&format!(
                "<button class=\"{action} screen-link\" id=\"{button}\">{heading}</button>",
                action = contract!(MenuAction),
                button = list.screen.button,
                heading = rml_text(&list.screen.heading),
            ));
        }
        installed.push(Installed {
            screen: list.screen.clone(),
            host: host_of(staged_screens, &list.screen)
                .map(|host| (host.id.clone(), list.screen.back_button())),
        });
    }

    let document = fill_slot(document, LINKS_SLOT, &links)?;
    let document = fill_slot(&document, SCREENS_SLOT, &screens)?;
    Ok((document, installed))
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
}
