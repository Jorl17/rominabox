//! The disc list in the menu, which we fill from the core after loading.
//!
//! We cannot create rows in the player while it runs, and `get_num_images` is
//! unknown until the game has loaded, so we write the rows here and hide the
//! entry that opens them at first. The PlayStation tray takes eight discs
//! (`disks[8]`) and the Sega CD tray four. For a longer playlist we show the
//! first rows and say how many we could not list, from the number of rows
//! here and the count from the core. We keep no second cap.

use crate::lists::{List, ListItem};
use crate::themes::declared_screens;
use std::path::Path;

/// How many disc rows we write on export. In the player we fill as many rows
/// as the document contains.
pub const ROW_CAP: usize = 8;

pub fn list(design: &Path) -> Result<Option<List>, String> {
    let Some(screen) = declared_screens(design)?
        .into_iter()
        .find(|screen| screen.images.as_deref() == Some("list"))
    else {
        return Ok(None);
    };
    let items = (0..ROW_CAP)
        .map(|index| ListItem {
            id: format!("{}-{index}", screen.id),
            icon: String::new(),
            title: String::new(),
            detail: String::new(),
            state: String::new(),
            selected: false,
            accent: false,
            line: true,
        })
        .collect();
    Ok(Some(List {
        screen,
        content: crate::lists::ListContent::Static(items),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lists::install;
    use crate::themes::{declared_screens, prepare_theme_assets};
    use std::fs;

    #[test]
    fn both_designs_bake_a_disc_list_and_the_column_keeps_its_button() {
        for name in ["native", "disc"] {
            let design = crate::repo::at("integrations/designs").join(name);
            let baked = list(&design)
                .unwrap_or_else(|error| panic!("{name}: {error}"))
                .unwrap_or_else(|| panic!("{name} declares no disc list"));
            let crate::lists::ListContent::Static(items) = &baked.content else {
                panic!("disc slots must be static")
            };
            assert_eq!(items.len(), ROW_CAP, "{name}");
            assert_eq!(items[0].id, "discs-0");
            assert_eq!(items[ROW_CAP - 1].id, format!("discs-{}", ROW_CAP - 1));

            let dest =
                std::env::temp_dir().join(format!("rominabox-discs-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dest);
            prepare_theme_assets(&design, &dest, "blue", None).expect("staged");
            let screens = declared_screens(&design).unwrap();
            install(&design, &dest, &screens, std::slice::from_ref(&baked)).expect("installed");
            let menu = fs::read_to_string(dest.join("menu.rml")).unwrap();
            let cfg = fs::read_to_string(dest.join("design.cfg")).unwrap();
            assert!(
                menu.contains("id=\"discs-list\""),
                "{name} menu has no disc list"
            );
            assert!(
                menu.matches("id=\"discs-").count() >= ROW_CAP,
                "{name} baked fewer than {ROW_CAP} rows"
            );
            assert!(cfg.contains("screen_images_discs = \"list\""), "{cfg}");
            assert!(cfg.contains("screen_mark_discs = \"IN\""), "{cfg}");
            if name == "disc" {
                assert!(
                    menu.contains("id=\"disc-face\""),
                    "the circle stays for one disc"
                );
                assert!(
                    menu.contains("id=\"disc\""),
                    "the column keeps the DISC button"
                );
                assert!(
                    !cfg.contains("screen_button_discs = \"disc\""),
                    "the list must not steal the column button: {cfg}"
                );
            }
            let _ = fs::remove_dir_all(&dest);
        }
    }
}
