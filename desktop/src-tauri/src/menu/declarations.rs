//! `design.cfg`: the design's facts about itself, which we read in the player.
//!
//! We write that file only here. We write every value quoted, and in
//! RetroArch's config reader a quoted value ends at the next `"`, with no
//! escapes, so we refuse a value with a quote or a line break here instead of
//! writing it as a different value.

use super::manifest::{Manifest, Screen, ScreenPlace};
use crate::lists::Installed;
use crate::player_settings::{Kind, PlayerSetting};

/// One `key = "value"` line.
fn line(text: &mut String, key: &str, value: &str) -> Result<(), String> {
    if value.contains(['"', '\n', '\r']) {
        return Err(format!(
            "design.cfg cannot hold {key} = {value:?}: a quote or a line break would end the value"
        ));
    }
    text.push_str(&format!("{key} = \"{value}\"\n"));
    Ok(())
}

/// Every element that opens `screen`, as a space-separated list that we read
/// in the player.
///
/// A design can draw a BACK button on a screen of its own, such as
/// `disc-back` on the disc screen. We add it to the list of the screen behind
/// every screen, which is not an Options entry and not reached from Options,
/// so no BACK in a design leads nowhere. We declare only what the design has.
fn shown_by(screen: &Screen, screens: &[&Screen], markup: &str) -> Vec<String> {
    let mut buttons: Vec<String> = screen
        .button
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let host = screens
        .iter()
        .find(|entry| entry.place == ScreenPlace::Plain && entry.option_label.is_none());
    if host.map(|entry| entry.id.as_str()) != Some(screen.id.as_str()) {
        return buttons;
    }
    for other in screens {
        // Only a screen of the design, beside the host. BACK on an Options
        // entry leads to Options, and BACK on the Options screen is already a
        // button of the host.
        if other.id == screen.id
            || other.place != ScreenPlace::Plain
            || other.option_label.is_some()
        {
            continue;
        }
        let back = format!("{}-back", other.id);
        if markup.contains(&format!("id=\"{back}\"")) && !buttons.contains(&back) {
            buttons.push(back);
        }
    }
    buttons
}

fn screen_lines(text: &mut String, screen: &Screen, buttons: &[String]) -> Result<(), String> {
    let id = &screen.id;
    line(text, &format!("screen_panel_{id}"), &screen.panel)?;
    line(text, &format!("screen_heading_{id}"), &screen.heading)?;
    line(text, &format!("screen_footer_{id}"), &screen.footer)?;
    line(text, &format!("screen_button_{id}"), &buttons.join(" "))?;
    if let Some(images) = &screen.images {
        line(text, &format!("screen_images_{id}"), images)?;
    }
    // The role of the screen, so that we find Pause or the achievements
    // screen in the player by role, not by a literal id.
    if let Some(role) = screen.role {
        line(text, &format!("screen_role_{id}"), role.name())?;
    }
    Ok(())
}

fn setting_lines(text: &mut String, setting: &PlayerSetting) -> Result<(), String> {
    let id = setting.id;
    line(text, &format!("setting_control_{id}"), &setting.control())?;
    line(text, &format!("setting_key_{id}"), setting.key.name())?;
    line(text, &format!("setting_file_{id}"), &setting.file())?;
    match setting.kind {
        Kind::Level {
            low,
            high,
            positions,
            ..
        } => {
            line(text, &format!("setting_kind_{id}"), "level")?;
            line(text, &format!("setting_low_{id}"), &low.to_string())?;
            line(text, &format!("setting_high_{id}"), &high.to_string())?;
            line(
                text,
                &format!("setting_positions_{id}"),
                &positions.to_string(),
            )
        }
        Kind::Switch { inverted } => {
            line(text, &format!("setting_kind_{id}"), "switch")?;
            line(
                text,
                &format!("setting_inverted_{id}"),
                &inverted.to_string(),
            )
        }
    }
}

/// The declarations for `markup`, the finished document.
///
/// We declare a screen when the author left it on (`staged`) and the document
/// contains it. We declare generated lists (`lists`) after the design's
/// screens, and add the BACK of each list to the buttons of the screen it
/// leads to. Overlays have no switch for the author, so we declare those in
/// the document, and for a logo-only document we declare only the logo.
pub(crate) fn write(
    manifest: &Manifest,
    staged: &[Screen],
    lists: &[Installed],
    settings: &[PlayerSetting],
    markup: &str,
) -> Result<String, String> {
    let drawn: Vec<&Screen> = staged
        .iter()
        .filter(|screen| !lists.iter().any(|list| list.screen.id == screen.id))
        .filter(|screen| markup.contains(&format!("id=\"{}\"", screen.panel)))
        .collect();
    // Every declared screen, first the design's and then the lists, with the
    // buttons that open it. We add a list's BACK to its host's buttons, and
    // the host can be a list, for example the achievements screen.
    let declared: Vec<&Screen> = drawn
        .iter()
        .copied()
        .chain(lists.iter().map(|list| &list.screen))
        .collect();
    let mut buttons: Vec<Vec<String>> = drawn
        .iter()
        .map(|screen| shown_by(screen, &drawn, markup))
        .chain(lists.iter().map(|list| vec![list.screen.button.clone()]))
        .collect();
    for list in lists {
        let Some((host, back)) = &list.host else {
            continue;
        };
        let Some(at) = declared.iter().position(|screen| screen.id == *host) else {
            return Err(format!("design.cfg has no button for screen {host}"));
        };
        if !buttons[at].contains(back) {
            buttons[at].push(back.clone());
        }
    }

    let mut text = String::new();
    let ids: Vec<&str> = drawn
        .iter()
        .map(|screen| screen.id.as_str())
        .chain(lists.iter().map(|list| list.screen.id.as_str()))
        .collect();
    line(&mut text, "screens", &ids.join(" "))?;
    for (screen, buttons) in declared.iter().zip(&buttons).take(drawn.len()) {
        screen_lines(&mut text, screen, buttons)?;
    }
    // The player's settings in this document, each with its element, its
    // RetroArch key and the file in which we save it.
    let drawn_settings: Vec<&PlayerSetting> = settings
        .iter()
        .filter(|setting| markup.contains(&format!("id=\"{}\"", setting.control())))
        .collect();
    let setting_ids: Vec<&str> = drawn_settings.iter().map(|setting| setting.id).collect();
    line(&mut text, "settings", &setting_ids.join(" "))?;
    for setting in drawn_settings {
        setting_lines(&mut text, setting)?;
    }

    let overlays: Vec<_> = manifest
        .overlays
        .iter()
        .filter(|overlay| markup.contains(&format!("id=\"{}\"", overlay.id)))
        .collect();
    let overlay_ids: Vec<&str> = overlays.iter().map(|overlay| overlay.id.as_str()).collect();
    line(&mut text, "overlays", &overlay_ids.join(" "))?;
    for overlay in &overlays {
        let id = &overlay.id;
        // When the overlay before this one is not in the document, we wait for
        // the game instead, so with the logo off we never wait for an overlay
        // that does not play.
        let follows = if overlays.iter().any(|before| before.id == overlay.follows) {
            overlay.follows.as_str()
        } else {
            ""
        };
        line(&mut text, &format!("overlay_follows_{id}"), follows)?;
        line(
            &mut text,
            &format!("overlay_after_{id}"),
            &overlay.after_ms.to_string(),
        )?;
        line(
            &mut text,
            &format!("overlay_hold_{id}"),
            &overlay.hold_ms.to_string(),
        )?;
        line(
            &mut text,
            &format!("overlay_leave_{id}"),
            &overlay.leave_ms.to_string(),
        )?;
        line(&mut text, &format!("overlay_needs_{id}"), &overlay.needs)?;
    }

    let binds = manifest.binds;
    line(&mut text, "binds_after", &binds.after_ms.to_string())?;
    line(
        &mut text,
        "binds_hover_after",
        &binds.hover_after_ms.to_string(),
    )?;
    line(&mut text, "binds_width", &binds.width.to_string())?;
    line(&mut text, "binds_list", "control-binds")?;
    // The font files staged beside the document, which we load in the player.
    let fonts: Vec<&str> = manifest.fonts.iter().map(|font| font.file.as_str()).collect();
    line(&mut text, "fonts", &fonts.join(" "))?;
    // The design's wording for the words we write in the player, or English.
    for (id, words) in &manifest.words {
        line(&mut text, &format!("word_{id}"), words)?;
    }

    for (index, list) in lists.iter().enumerate() {
        screen_lines(&mut text, &list.screen, &buttons[drawn.len() + index])?;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_the_reader_would_cut_short_is_refused() {
        let mut text = String::new();
        let error = line(&mut text, "screen_heading_pause", "SAY \"HI\"").unwrap_err();
        assert!(error.contains("screen_heading_pause"), "{error}");
        assert!(line(&mut text, "screen_footer_pause", "A\nB").is_err());
        assert!(text.is_empty(), "nothing is written for a refused value");
        line(&mut text, "screen_heading_pause", "GAME PAUSED").unwrap();
        assert_eq!(text, "screen_heading_pause = \"GAME PAUSED\"\n");
    }
}
