//! Where Options entries are, from the boxes in the RmlUi layout.
//!
//! The words of an entry come from the design, and in another language, or
//! with longer words, a label can wrap. In every registered design, we make
//! the first entry's label long enough to wrap, and it must end inside its
//! own entry, above the next one. A marker after the last word shows where
//! the text ends, because the box of an inline label is its first line.
//!
//! The entries are a list of pages. A full page, with as many entries as the
//! design's page size, is at least one entry gap from the volume row and from
//! BACK, and where the page is under the volume row the space is that same
//! gap, so the column has one even spacing. The pager, when shown, overlaps
//! none of them. A focused entry does not move, so focus never closes a gap.
//!
//! The boxes come from the RmlUi layout through the windowless probe. We do
//! not check how it looks here (see the state pictures), or that we split
//! the pages in the player (see the navigation tests).

mod support;

use std::fs;
use support::{boxes, compose, compose_with, designs, kit, showing};

const LONG: &str =
    "CONTROLLER SETUP FOR THIS CONSOLE AND THEN SOME MORE WORDS THAT DO NOT FIT ON ONE LINE";
/// A mark after the label's last word: where its text ends.
const END: &str = "long-entry-label-end";

/// The ids of the Options entries, in document order.
fn entries(menu: &str) -> Vec<String> {
    menu.match_indices("<button class=\"")
        .filter_map(|(at, _)| {
            let tag = &menu[at..at + menu[at..].find('>')?];
            let class = tag.split("class=\"").nth(1)?.split('"').next()?;
            if !class.split(' ').any(|name| name == "option-entry") {
                return None;
            }
            Some(tag.split("id=\"").nth(1)?.split('"').next()?.to_string())
        })
        .collect()
}

/// `menu` with the words of `entry`'s label replaced by `LONG`, ended by a
/// mark the probe can measure.
fn with_long_label(menu: &str, entry: &str) -> String {
    let at = menu
        .find(&format!("id=\"{entry}\""))
        .unwrap_or_else(|| panic!("no entry {entry}"));
    let label = at + menu[at..]
        .find("class=\"option-label\">")
        .unwrap_or_else(|| panic!("{entry} has no option-label"));
    let words = label + "class=\"option-label\">".len();
    let end = words + menu[words..].find("</span>").unwrap();
    format!(
        "{}{LONG}<span id=\"{END}\">.</span>{}",
        &menu[..words],
        &menu[end..]
    )
}

#[test]
fn a_long_entry_label_stays_inside_its_entry() {
    let scratch = rominabox_scratch::Scratch::dir("rominabox-options-layout");
    let kit = kit(&scratch);
    let mut failures = Vec::new();
    for design in designs() {
        let composed = compose(&kit, &design, None, 1, &scratch.join(format!("composed-{design}")));
        let listed = entries(&composed.menu);
        assert!(listed.len() >= 2, "{design}: fewer than two Options entries: {listed:?}");
        let (first, second) = (&listed[0], &listed[1]);
        let document = scratch.join(format!("composed-{design}/long-entry.rml"));
        fs::write(
            &document,
            with_long_label(&showing(&composed.menu, "options-panel"), first),
        )
        .unwrap();
        let found = boxes(&document, (960, 600), &[first, second, END]);
        let [Some(entry), Some(next), Some(label)] = found[..] else {
            panic!("{design}: the probe did not lay out {first}, {second} and the label: {found:?}");
        };
        let bottom = |b: [f64; 4]| b[1] + b[3];
        if label[3] <= 0.0 {
            failures.push(format!("{design}: the long label has no height"));
        } else if bottom(label) > bottom(entry) + 0.5 {
            failures.push(format!(
                "{design}: the label ends at {:.0}, below its entry, which ends at {:.0}",
                bottom(label),
                bottom(entry)
            ));
        } else if next[1] + 0.5 < bottom(label) {
            failures.push(format!(
                "{design}: {second} starts at {:.0}, over the label, which ends at {:.0}",
                next[1],
                bottom(label)
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Every Options entry of the design, as we bundle them in a full export.
fn every_entry(kit: &std::path::Path, design: &str) -> Vec<String> {
    rominabox_desktop::menu::declared_screens(&rominabox_desktop::themes::staged_design(kit, design))
        .unwrap()
        .into_iter()
        .filter(|screen| screen.option_label.is_some())
        .map(|screen| screen.id)
        .collect()
}

/// The Options list with one full page of copies of its first entry,
/// `full-1` to `full-N`, `focused` on those in `focused`, and its pager shown,
/// as in the player for a list of more than one page. Returns the page size
/// that we wrote the list with.
fn with_full_page(menu: &str, focused: &[usize]) -> (String, usize) {
    let list = menu.find("id=\"options-list\"").expect("an Options list");
    let size_at = list + menu[list..].find("data-page-size=\"").unwrap() + "data-page-size=\"".len();
    let page_size: usize = menu[size_at..size_at + menu[size_at..].find('"').unwrap()]
        .parse()
        .unwrap();
    let open = "<div class=\"list-page\">";
    let start = list + menu[list..].find(open).unwrap() + open.len();
    let end = start + menu[start..].find("</div><div id=\"options-pager\"").unwrap();
    let first = &menu[start..start + menu[start..].find("</button>").unwrap() + "</button>".len()];
    let id_at = first.find("id=\"").unwrap() + "id=\"".len();
    let id = &first[id_at..id_at + first[id_at..].find('"').unwrap()];
    let rows: String = (1..=page_size)
        .map(|index| {
            let row = first.replace(&format!("id=\"{id}\""), &format!("id=\"full-{index}\""));
            if focused.contains(&index) {
                row.replacen("option-entry", "option-entry focused", 1)
            } else {
                row
            }
        })
        .collect();
    let mut full = menu.to_string();
    full.replace_range(start..end, &rows);
    let full = full.replacen(
        "id=\"options-pager\" class=\"list-pager\" style=\"display:none;\"",
        "id=\"options-pager\" class=\"list-pager\"",
        1,
    );
    (full, page_size)
}

#[test]
fn a_full_page_of_entries_keeps_one_rhythm_clear_of_the_volume_and_back() {
    let scratch = rominabox_scratch::Scratch::dir("rominabox-options-page");
    let kit = kit(&scratch);
    let mut problems = Vec::new();
    for design in designs() {
        let entries = every_entry(&kit, &design);
        let destination = scratch.join(format!("composed-{design}"));
        let composed = compose_with(&kit, &design, "megadrive", Some(&entries), 3, &destination);
        let shown = showing(&composed.menu, "options-panel");
        let (plain, page_size) = with_full_page(&shown, &[]);
        let (focused, _) = with_full_page(&shown, &(1..=page_size).collect::<Vec<_>>());
        let rows: Vec<String> = (1..=page_size).map(|index| format!("full-{index}")).collect();
        let mut ids: Vec<&str> = rows.iter().map(String::as_str).collect();
        ids.extend(["volume-control", "options-back", "options-prev", "options-next"]);
        let measure = |name: &str, text: &str| {
            let document = destination.join(format!("{name}.rml"));
            fs::write(&document, text).unwrap();
            boxes(&document, (960, 600), &ids)
                .into_iter()
                .zip(&ids)
                .map(|(found, id)| found.unwrap_or_else(|| panic!("{design}: {id} is not laid out")))
                .collect::<Vec<[f64; 4]>>()
        };
        let at_rest = measure("full-page", &plain);
        let with_focus = measure("full-page-focused", &focused);
        let (page, others) = at_rest.split_at(page_size);
        let [volume, back, previous, next] = [others[0], others[1], others[2], others[3]];
        let bottom = |b: [f64; 4]| b[1] + b[3];
        let overlaps = |a: [f64; 4], b: [f64; 4]| {
            a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
        };
        let gaps: Vec<f64> = page.windows(2).map(|pair| pair[1][1] - bottom(pair[0])).collect();
        let gap = gaps.first().copied().unwrap_or(0.0);
        if gaps.iter().any(|other| (other - gap).abs() > 0.5) || gap < 0.5 {
            problems.push(format!("{design}: the gaps between entries are {gaps:?}"));
        }
        let (top, end) = (page[0][1], bottom(page[page_size - 1]));
        for (name, other) in [("the volume row", volume), ("BACK", back)] {
            let clear = if other[1] >= end { other[1] - end } else { top - bottom(other) };
            if clear + 0.5 < gap {
                problems.push(format!(
                    "{design}: a full page of {page_size} ends {clear}dp from {name}, closer than the {gap}dp between entries"
                ));
            }
        }
        if bottom(volume) <= top && (top - bottom(volume) - gap).abs() > 0.5 {
            problems.push(format!(
                "{design}: the gap under the volume row is {}dp and between entries {gap}dp",
                top - bottom(volume)
            ));
        }
        for (name, arrow) in [("<", previous), (">", next)] {
            for (what, other) in [("the volume row", volume), ("BACK", back)]
                .into_iter()
                .chain(page.iter().map(|row| ("an entry", *row)))
            {
                if overlaps(arrow, other) {
                    problems.push(format!("{design}: the pager's {name} {arrow:?} is on {what} {other:?}"));
                }
            }
        }
        for (index, (still, lit)) in page.iter().zip(&with_focus).enumerate() {
            if (still[1] - lit[1]).abs() > 0.5 || (bottom(*still) - bottom(*lit)).abs() > 0.5 {
                problems.push(format!(
                    "{design}: entry {} focused is {lit:?}, at rest {still:?}: focus moves an edge into a gap",
                    index + 1
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
