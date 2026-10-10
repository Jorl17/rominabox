use super::*;
use std::fs;
use std::path::{Path, PathBuf};

/// In `root`, a Native with `badge` as its badge, and a design beside it
/// with its own badge when `own` is some.
fn designs(root: &Path, badge: &str, own: Option<&str>) -> (PathBuf, PathBuf) {
    let native = root.join("native");
    fs::create_dir_all(&native).unwrap();
    fs::copy(crate::repo::at("integrations/designs/native/design.json"), native.join("design.json")).unwrap();
    fs::write(native.join(PART), badge).unwrap();
    let design = root.join("other");
    fs::create_dir_all(&design).unwrap();
    fs::write(design.join("design.json"), "{}").unwrap();
    if let Some(own) = own {
        fs::write(design.join(PART), own).unwrap();
    }
    (native, design)
}

fn placed(design: &Path, markup: &str) -> Result<String, String> {
    place(&Manifest::load(design).unwrap(), markup)
}

const BADGE: &str = "<span class=\"badge\" data-binding=\"HOTKEY\"></span>\n";

#[test]
fn a_badge_names_the_hotkey_of_its_marker() {
    let placed_root = rominabox_scratch::Scratch::dir("rominabox-badges-placed");
    let (native, _) = designs(&placed_root, BADGE, None);
    let markup = format!("<button>BACK{}</button><button>&gt;{}</button>", marker("back"), marker("next-page"));
    assert_eq!(
        placed(&native, &markup).unwrap(),
        "<button>BACK<span class=\"badge\" data-binding=\"back\"></span></button>\
         <button>&gt;<span class=\"badge\" data-binding=\"next-page\"></span></button>"
    );
}

#[test]
fn a_design_has_natives_badge_or_none_with_an_empty_one() {
    let markup = format!("<button>BACK{}</button>", marker("back"));
    let inherited_root = rominabox_scratch::Scratch::dir("rominabox-badges-inherited");
    let (_, inheriting) = designs(&inherited_root, BADGE, None);
    assert_eq!(
        placed(&inheriting, &markup).unwrap(),
        "<button>BACK<span class=\"badge\" data-binding=\"back\"></span></button>"
    );
    let none_root = rominabox_scratch::Scratch::dir("rominabox-badges-none");
    let (_, without) = designs(&none_root, BADGE, Some(""));
    assert_eq!(placed(&without, &markup).unwrap(), "<button>BACK</button>");
}

#[test]
fn a_marker_names_a_hotkey_and_a_badge_has_a_place_for_it() {
    let refused_root = rominabox_scratch::Scratch::dir("rominabox-badges-refused");
    let (native, _) = designs(&refused_root, BADGE, None);
    let unknown = placed(&native, &format!("<button>{}</button>", marker("pause"))).unwrap_err();
    assert!(unknown.contains("names no hotkey 'pause'"), "{unknown}");
    let holeless_root = rominabox_scratch::Scratch::dir("rominabox-badges-holeless");
    let (native, _) = designs(&holeless_root, "<span class=\"badge\"></span>", None);
    let holeless = placed(&native, &marker("back")).unwrap_err();
    assert!(holeless.contains("no HOTKEY hole"), "{holeless}");
}
