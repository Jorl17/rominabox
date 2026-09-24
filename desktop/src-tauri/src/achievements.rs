//! The packaged achievement capability. We sign in and evaluate achievements
//! with the RetroArch client in the player, never in the builder.
use crate::{lists::List, menu};
use std::path::Path;

pub const SCREEN: &str = "achievements";

/// Default for new authoring requests and current project files.
pub fn default_included() -> bool {
    true
}

/// Without a menu the player cannot sign in or manage an account.
pub fn included(requested: bool, show_menu: bool) -> bool {
    requested && show_menu
}

/// Resolve the capability and entry together. An explicit composition must
/// match the selected features, and reachability never depends on the data.
pub fn entries(
    design: &Path,
    requested: bool,
    show_menu: bool,
    explicit: Option<&[String]>,
) -> Result<Vec<String>, String> {
    if !show_menu {
        return Ok(Vec::new());
    }
    entries_in(&menu::declared_screens(design)?, requested, show_menu, explicit)
}

/// The same, over screens already read.
pub fn entries_in(
    screens: &[menu::Screen],
    requested: bool,
    show_menu: bool,
    explicit: Option<&[String]>,
) -> Result<Vec<String>, String> {
    if !show_menu {
        return Ok(Vec::new());
    }
    if let Some(entries) = explicit {
        if entries.iter().any(|entry| entry == SCREEN) != requested {
            return Err(
                "menuEntries must include achievements exactly when includeAchievements is true"
                    .into(),
            );
        }
        return Ok(entries.to_vec());
    }
    Ok(screens
        .iter()
        .filter(|screen| {
            screen.option_label.is_some()
                && if screen.id == SCREEN {
                    requested
                } else {
                    screen.option_default
                }
        })
        .map(|screen| screen.id.clone())
        .collect())
}

/// The live screen may be empty, because sign-in comes before any rows.
pub fn screen(manifest: &menu::Manifest) -> Result<List, String> {
    let screen = manifest
        .screen(menu::ScreenRole::Achievements)
        .cloned()
        .ok_or_else(|| "The base design has no achievements screen".to_string())?;
    Ok(List {
        screen,
        content: crate::lists::ListContent::Live,
    })
}

/// The prepared artifact must state that the integration is compiled in.
/// An export without achievements works with a player that does not support them.
pub fn validate_runtime(kit: &Path, included: bool) -> Result<(), String> {
    if !included {
        return Ok(());
    }
    let path = kit.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path)
            .map_err(|error| format!("Could not read the runtime manifest: {error}"))?,
    )
    .map_err(|error| format!("Could not parse the runtime manifest: {error}"))?;
    let supported = manifest["components"]
        .as_array()
        .and_then(|components| {
            components
                .iter()
                .find(|component| component["name"] == "RetroArch")
        })
        .is_some_and(|runtime| runtime["capabilities"]["achievements"] == true);
    if !supported {
        return Err("The prepared player does not include verified achievements support. Rebuild the runtime kit or turn off Include achievements.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_and_explicit_entry_agree() {
        let design = crate::themes::design_root("native").unwrap();
        assert!(entries(&design, true, true, None)
            .unwrap()
            .iter()
            .any(|id| id == SCREEN));
        assert!(!entries(&design, false, true, None)
            .unwrap()
            .iter()
            .any(|id| id == SCREEN));
        assert!(entries(&design, true, false, None).unwrap().is_empty());
        assert!(entries(&design, true, true, Some(&[])).is_err());
        assert!(entries(&design, false, true, Some(&[SCREEN.into()])).is_err());
        assert_eq!(
            entries(&design, true, true, Some(&[SCREEN.into()])).unwrap(),
            vec![SCREEN]
        );
    }
}
