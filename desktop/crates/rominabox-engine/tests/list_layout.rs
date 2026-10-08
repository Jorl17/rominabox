//! Whether a full page of the achievements list fills the screen.
//!
//! On a signed-in achievements screen with a full page of rows, the list
//! reaches as far right as BACK, and the space between the last row and the
//! buttons below it, without the pager, is less than one more row.
//!
//! We clone and rename the list's prototype to make the rows, as in the
//! player (`live_lists.cpp`). The boxes come from the RmlUi layout through
//! the windowless probe. We do not check how the screen looks here. We draw
//! it in the picture tests.

mod support;

use std::fs;
use support::{boxes, compose_with, kit, showing};

const SIZES: [(u32, u32); 2] = [(960, 600), (1280, 600)];

/// The signed-in screen, where we show the catalog and its session buttons
/// and hide the signed-out note.
fn signed_in(menu: &str) -> String {
    let mut shown = menu.to_string();
    // Show the pager too, because 116 achievements fill many pages.
    for id in [
        "achievements-catalog",
        "achievements-session-actions",
        "achievements-pager",
    ] {
        let hidden = shown
            .find(&format!("id=\"{id}\""))
            .unwrap_or_else(|| panic!("the menu has no {id}"));
        let tag_end = hidden + shown[hidden..].find('>').unwrap();
        let tag = shown[hidden..tag_end]
            .replacen(" style=\"display:none;\"", "", 1)
            .replacen(" style=\"display: none;\"", "", 1);
        shown.replace_range(hidden..tag_end, &tag);
    }
    shown.replacen(
        "id=\"achievements-signed-out\"",
        "id=\"achievements-signed-out\" style=\"display:none;\"",
        1,
    )
}

/// A full page of rows from the list's prototype, as in the player.
fn with_full_page(menu: &str) -> (String, usize) {
    let list = menu
        .find("id=\"achievements-list\"")
        .expect("an achievements list");
    let size_at =
        list + menu[list..].find("data-page-size=\"").unwrap() + "data-page-size=\"".len();
    let page_size: usize = menu[size_at..size_at + menu[size_at..].find('"').unwrap()]
        .parse()
        .unwrap();
    let open = "<div class=\"list-prototype\" style=\"display:none;\">";
    let start = list + menu[list..].find(open).unwrap() + open.len();
    let end = start
        + menu[start..]
            .find("</div><div id=\"achievements-pager\"")
            .unwrap();
    let prototype = &menu[start..end];
    let rows: String = (1..=page_size)
        .map(|index| {
            prototype
                .replace(
                    "id=\"achievements-prototype",
                    &format!("id=\"achievement-{index}"),
                )
                .replacen("class=\"list-row ", "class=\"list-row badge-loading ", 1)
        })
        .collect();
    let page = format!("<div class=\"list-page\">{rows}</div>");
    let insert_at = end + "</div>".len();
    let mut full = menu.to_string();
    full.insert_str(insert_at, &page);
    (full, page_size)
}

#[test]
fn a_full_page_of_achievements_uses_the_screen() {
    let scratch = rominabox_scratch::Scratch::dir("rominabox-list-layout");
    let kit = kit(&scratch);
    let entries: Vec<String> = ["controls", "video", "shaders", "achievements"]
        .map(String::from)
        .to_vec();
    let mut problems = Vec::new();
    for design in ["native", "disc"] {
        let composed = compose_with(
            &kit,
            design,
            "megadrive",
            Some(&entries),
            1,
            &scratch.join(format!("composed-{design}")),
        );
        let shown = signed_in(&showing(&composed.menu, "achievements-panel"));
        let (document_text, page_size) = with_full_page(&shown);
        let document = scratch
            .join(format!("composed-{design}"))
            .join("achievements-full.rml");
        fs::write(&document, document_text).unwrap();
        let last = format!("achievement-{page_size}");
        for size in SIZES {
            let measured = boxes(
                &document,
                size,
                &[
                    "achievement-1",
                    &last,
                    "achievements-pager",
                    "achievements-back",
                ],
            );
            let [first, last_row, pager, back] = [0, 1, 2, 3].map(|index| {
                measured[index].unwrap_or_else(|| {
                    panic!("{design} {size:?}: element {index} of the full page is not laid out")
                })
            });
            let [bx, by, bw, _] = back;
            let [fx, _, fw, fh] = first;
            let [_, _, _, ph] = pager;
            let last_bottom = last_row[1] + last_row[3];
            let row = last_row[1] + last_row[3] - first[1];
            let row_step = row / page_size as f64;
            if fx + fw + 1.0 < bx + bw {
                problems.push(format!(
                    "{design} {size:?}: the list ends at x={} and BACK at x={}: {}dp of width unused",
                    fx + fw,
                    bx + bw,
                    bx + bw - (fx + fw)
                ));
            }
            // Count the pager against the space only where it is between the
            // rows and the buttons, not beside the buttons.
            let [_, py, _, _] = pager;
            let between = py >= last_bottom && py + ph <= by;
            let unused = by - last_bottom - if between { ph } else { 0.0 };
            if unused >= row_step {
                problems.push(format!(
                    "{design} {size:?}: {unused}dp unused between the last row and the buttons (pager aside), room for {} more row(s) of {row_step}dp; a page is {page_size} of {fh}dp",
                    (unused / row_step).floor()
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
