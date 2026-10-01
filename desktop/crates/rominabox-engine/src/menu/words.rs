//! The words that we write in the player, such as status lines, prompts and
//! the words on slots and lists, as we declare them in `words.inc`, and the
//! wording that a design gives for them.
//!
//! A design can name only the words we declare, by id. We refuse an unknown
//! id, and a `{hole}` that is not in the English text of the word, because
//! that hole would then appear unchanged on screen.

use std::{collections::BTreeMap, sync::OnceLock};

const SOURCE: &str = include_str!("../../../../../vendor/retroarch/menu/drivers/rmlui/words.inc");

/// One word that we write in the player: its id and its English text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub id: String,
    pub english: String,
}

/// Every word that we write in the player, in the order of `words.inc`.
pub fn known() -> &'static [Word] {
    static WORDS: OnceLock<Vec<Word>> = OnceLock::new();
    WORDS.get_or_init(|| {
        let words: Vec<Word> = super::inc::declarations(SOURCE)
            .filter(|declaration| declaration.macro_name() == "RIB_WORD")
            .map(|declaration| match declaration.fields()[..] {
                [_, id, english] => Word {
                    id: id.to_string(),
                    english: english.to_string(),
                },
                ref fields => panic!(
                    "words.inc: RIB_WORD({}) is not RIB_WORD(name, \"id\", \"English\")",
                    fields.join(", ")
                ),
            })
            .collect();
        assert!(!words.is_empty(), "words.inc declares no words");
        words
    })
}

const KEY_WORDS: &str =
    include_str!("../../../../../vendor/retroarch/menu/drivers/rmlui/key_words.inc");

/// Each key that has a word in `key_words.inc`, by its name in the
/// RetroArch config, and its word.
pub fn key_words() -> &'static [(String, String)] {
    static WORDS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    WORDS.get_or_init(|| {
        super::inc::declarations(KEY_WORDS)
            .filter(|declaration| declaration.macro_name() == "RIB_KEY_WORD")
            .map(|declaration| match declaration.fields()[..] {
                [name, word] => (name.to_string(), word.to_string()),
                ref fields => panic!(
                    "key_words.inc: RIB_KEY_WORD({}) is not RIB_KEY_WORD(\"name\", \"Word\")",
                    fields.join(", ")
                ),
            })
            .collect()
    })
}

const MOUSE_BUTTONS: &str =
    include_str!("../../../../../vendor/retroarch/menu/drivers/rmlui/mouse_buttons.inc");

/// A mouse button for a control, as we declare it in `mouse_buttons.inc`:
/// its value in a controls file, its RetroArch id and its word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MouseButton {
    pub value: String,
    pub id: String,
    pub word: String,
}

/// Every mouse button the player can bind a control to, in declared order.
pub fn mouse_buttons() -> &'static [MouseButton] {
    static BUTTONS: OnceLock<Vec<MouseButton>> = OnceLock::new();
    BUTTONS.get_or_init(|| {
        super::inc::declarations(MOUSE_BUTTONS)
            .filter(|declaration| declaration.macro_name() == "RIB_MOUSE_BUTTON")
            .map(|declaration| match declaration.fields()[..] {
                [value, id, word] => MouseButton {
                    value: value.to_string(),
                    id: id.to_string(),
                    word: word.to_string(),
                },
                ref fields => panic!(
                    "mouse_buttons.inc: RIB_MOUSE_BUTTON({}) is not (\"value\", RETRO_DEVICE_ID_MOUSE_id, \"Word\")",
                    fields.join(", ")
                ),
            })
            .collect()
    })
}

/// The text for `id` in `given`, the wording of the design, or else the
/// English text, with each hole filled in with the value we show there.
pub fn say(given: &BTreeMap<String, String>, id: &str, values: &[(&str, &str)]) -> String {
    let mut text = given.get(id).cloned().unwrap_or_else(|| {
        known()
            .iter()
            .find(|word| word.id == id)
            .unwrap_or_else(|| panic!("words.inc declares no word '{id}'"))
            .english
            .clone()
    });
    for (hole, value) in values {
        text = text.replace(&format!("{{{hole}}}"), value);
    }
    text
}

/// The `{holes}` in `text`, in order.
fn holes(text: &str) -> Vec<&str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(hole, _)| hole))
        .collect()
}

/// The words of a design, after we check that each one is a word we
/// declare and uses only the holes in the English text of that word.
pub fn check(design: &str, given: &BTreeMap<String, String>) -> Result<(), String> {
    for (id, text) in given {
        let Some(word) = known().iter().find(|word| word.id == *id) else {
            let ids: Vec<&str> = known().iter().map(|word| word.id.as_str()).collect();
            return Err(format!(
                "design '{design}' gives words for '{id}', which the menu does not write; \
                 its words are {}",
                ids.join(", ")
            ));
        };
        let allowed = holes(&word.english);
        if let Some(hole) = holes(text).into_iter().find(|hole| !allowed.contains(hole)) {
            let offered: Vec<String> = allowed.iter().map(|hole| format!("{{{hole}}}")).collect();
            return Err(format!(
                "design '{design}' words '{id}' as \"{text}\", but the menu fills in only {} \
                 there, not {{{hole}}}",
                if offered.is_empty() {
                    "nothing".to_string()
                } else {
                    offered.join(" and ")
                }
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(id, text)| (id.to_string(), text.to_string()))
            .collect()
    }

    #[test]
    fn the_players_words_are_read_with_their_english() {
        let slot = known().iter().find(|word| word.id == "slot").unwrap();
        assert_eq!(slot.english, "SLOT {slot}");
        assert!(known().iter().any(|word| word.id == "save-failed"));
    }

    #[test]
    fn a_word_is_the_designs_else_the_players_english_filled_in() {
        assert_eq!(say(&words(&[]), "page-count", &[("page", "1"), ("pages", "2")]), "1/2");
        assert_eq!(
            say(&words(&[("page-count", "PAGE {page} OF {pages}")]), "page-count", &[("page", "1"), ("pages", "2")]),
            "PAGE 1 OF 2"
        );
    }

    #[test]
    fn a_design_may_word_what_the_menu_writes_with_its_holes() {
        check("x", &words(&[("slot", "BLOCK {slot}"), ("empty", "FREE"), ("occupied", "")]))
            .unwrap();
    }

    #[test]
    fn a_word_the_menu_does_not_write_is_refused_by_name() {
        let error = check("x", &words(&[("slots", "BLOCK")])).unwrap_err();
        assert!(error.contains("'slots'") && error.contains("design 'x'"), "{error}");
        assert!(error.contains("slot,"), "the message lists the words: {error}");
    }

    #[test]
    fn a_hole_the_word_does_not_have_is_refused() {
        let error = check("x", &words(&[("slot", "BLOCK {number}")])).unwrap_err();
        assert!(error.contains("{slot}") && error.contains("{number}"), "{error}");
        let error = check("x", &words(&[("empty", "{slot} FREE")])).unwrap_err();
        assert!(error.contains("nothing"), "{error}");
    }
}
