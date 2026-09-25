//! Whether a long Options label stays inside its entry.
//!
//! The words of an entry are in the design, and in another language or with
//! longer words a label can wrap. In every registered design we make the
//! label of the first entry long enough to wrap. It must end inside its entry,
//! and the next entry must start below it. We find the end of the text with a
//! marker after its last word, because the box of an inline label is only its
//! first line. The boxes come from the RmlUi layout through the windowless
//! probe. This does NOT prove how it looks: see the states tests.

mod support;

use std::fs;
use support::{boxes, compose, designs, kit, showing};

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
