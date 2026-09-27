//! The packaged achievement capability. We sign in and evaluate achievements
//! with the RetroArch client in the player, never in the builder.
use crate::{lists::List, menu};
use std::path::Path;

pub const SCREEN: &str = "achievements";

/// Default for a current project file without this field. For a new game we
/// use the builder's (`crate::builder::defaults`).
pub fn default_included() -> bool {
    true
}

/// Without a menu the player cannot sign in or manage an account.
pub fn included(requested: bool, show_menu: bool) -> bool {
    requested && show_menu
}

/// The QUICK SIGN IN folder, common to every game with achievements, by
/// name, directly in the platform's per-user application data: Application
/// Support outside the sandbox on macOS, `%LOCALAPPDATA%` on Windows. It is
/// not inside `ROM-in-a-Box/` because a sandboxed game cannot create a
/// missing parent of the folder in its entitlement.
///
/// We never give a namespaced export (from a worktree or a test) a player's
/// accounts. It gets the folder named in its environment (we set one in
/// `scripts/worktree.py`), or else its own folder, named after the namespace.
pub fn accounts_folder(namespace: Option<&str>, named: Option<&str>) -> Result<String, String> {
    fn present(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|value| !value.is_empty())
    }
    const SHARED: &str = "ROM-in-a-Box Accounts";
    let folder = match (present(namespace), present(named)) {
        (_, Some(folder)) => folder.to_string(),
        (Some(namespace), None) => format!("{SHARED}-{namespace}"),
        (None, None) => return Ok(SHARED.to_string()),
    };
    if folder.starts_with('.') || folder.contains(['/', '\\', ':', '\n', '\t']) {
        return Err(format!("'{folder}' is not a folder name for QUICK SIGN IN"));
    }
    Ok(folder)
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

/// The list of saved accounts for QUICK SIGN IN, which we fill in the player.
/// We add it with the achievements screen whenever the design declares it.
/// Native declares it, so every design has it unless it replaces that part.
pub fn accounts_screen(manifest: &menu::Manifest) -> Option<List> {
    manifest
        .screen(menu::ScreenRole::Accounts)
        .cloned()
        .map(|screen| List {
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

    #[test]
    fn every_game_shares_one_accounts_folder_and_a_namespace_has_its_own() {
        assert_eq!(accounts_folder(None, None).unwrap(), "ROM-in-a-Box Accounts");
        assert_eq!(accounts_folder(Some("  "), Some(" ")).unwrap(), "ROM-in-a-Box Accounts");
        assert_eq!(
            accounts_folder(Some("app.rominabox.game.wt-x"), Some("ROM-in-a-Box Accounts-wt-x")).unwrap(),
            "ROM-in-a-Box Accounts-wt-x"
        );
        // For a test's namespace with no named folder, we use a folder of its
        // own, never the player's.
        assert_eq!(
            accounts_folder(Some("size-featured"), None).unwrap(),
            "ROM-in-a-Box Accounts-size-featured"
        );
        assert!(accounts_folder(Some("a/b"), None).is_err());
        // We accept a plain name here, never a path.
        assert!(accounts_folder(None, Some("../x")).is_err());
        assert!(accounts_folder(None, Some(".hidden")).is_err());
    }
}
