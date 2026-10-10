use super::super::*;

fn hotkeys(json: serde_json::Value) -> Hotkeys {
    serde_json::from_value(json).unwrap()
}

fn named(id: &str) -> Hotkey {
    Hotkey::named(id).unwrap()
}

/// A preview shows the first key of a hotkey in the words of key_words.inc,
/// or its first binding when it has no key, with a position in its standard
/// word, Home in the word of hotkeys.inc and a chord joined as in the menu.
#[test]
fn a_hotkey_is_worded_by_its_first_key_or_else_its_first_binding() {
    let chosen = hotkeys(serde_json::json!({
        "back": ["pad:a", "key:escape"],
        "previous-page": ["key:pageup", "pad:l"],
        "quick-save": ["pad:l"],
        "quick-load": ["key:f4"],
        "next-slot": [],
        "fullscreen": ["pad:l3+r3"]
    }));
    let words = |id: &str| chosen.words(named(id)).unwrap();
    assert_eq!(words("back").as_deref(), Some("Escape"));
    assert_eq!(words("previous-page").as_deref(), Some("Page Up"));
    assert_eq!(words("quick-save").as_deref(), Some("L1"));
    assert_eq!(words("quick-load").as_deref(), Some("f4"));
    assert_eq!(words("next-slot"), None);
    assert_eq!(words("fullscreen").as_deref(), Some("L3+R3"));
    let home = hotkeys(serde_json::json!({ "menu": ["pad:home", "key:f1"], "back": ["pad:a", "key:escape"] }));
    assert_eq!(home.words(named("menu")).unwrap().as_deref(), Some("f1"));
}

/// A hint names a hotkey in braces, and we write the words of its binding
/// after the hotkey, which the menu then replaces with the words of the input
/// in use. Words in brackets stay as they are.
#[test]
fn a_hint_carries_the_words_of_the_hotkeys_it_names() {
    let defaults = crate::builder::defaults().hotkeys.clone();
    assert_eq!(defaults.hint("{back}  BACK").unwrap(), "{back:Escape}  BACK");
    assert_eq!(defaults.hint("[ESC]  CANCEL").unwrap(), "[ESC]  CANCEL");
    assert_eq!(
        defaults.hint("{previous-page} {next-page}  PAGE").unwrap(),
        "{previous-page:Page Up} {next-page:Page Down}  PAGE"
    );
    let refused = defaults.hint("{pause}  BACK").unwrap_err();
    assert!(refused.contains("names no hotkey 'pause'"), "{refused}");
}

/// An element marked data-binding shows the words of its binding.
#[test]
fn an_element_marked_data_binding_shows_the_words_of_its_binding() {
    let defaults = crate::builder::defaults().hotkeys.clone();
    assert_eq!(
        defaults
            .bound_words(r#"<div>PRESS <span class="hint-key" data-binding="menu">ESC</span> TO PAUSE</div>"#)
            .unwrap(),
        r#"<div>PRESS <span class="hint-key" data-binding="menu">Escape</span> TO PAUSE</div>"#
    );
    let refused = defaults.bound_words(r#"<span data-binding="pause"></span>"#).unwrap_err();
    assert!(refused.contains("names no hotkey 'pause'"), "{refused}");
}
