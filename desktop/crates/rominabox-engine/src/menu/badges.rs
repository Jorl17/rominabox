//! Badges: on a button that does what a hotkey does, a binding of that
//! hotkey, as on every Back button and on the arrows of a list. A design
//! marks such a button with `<!--BADGE:<hotkey>-->`, and so do we in the
//! buttons we write. The badge is the design's `badge.rml`, or else Native's,
//! with the hotkey's id in its HOTKEY hole, and it names the hotkey with
//! data-binding (document_contract.inc, BindingAttribute). A design whose
//! `badge.rml` is empty has no badges.

use super::manifest::Manifest;
use crate::hotkeys::Hotkey;

/// The design part that is a badge.
const PART: &str = "badge.rml";
/// Where the part names the hotkey.
const HOLE: &str = "HOTKEY";
const OPEN: &str = "<!--BADGE:";
const CLOSE: &str = "-->";

/// The marker of a badge of the hotkey `id` in a button.
pub(crate) fn marker(id: &str) -> String {
    format!("{OPEN}{id}{CLOSE}")
}

/// `markup` with the design's badge at each marker.
pub(crate) fn place(manifest: &Manifest, markup: &str) -> Result<String, String> {
    let badge = if manifest.has_fragment(PART) {
        manifest.fragment(PART)?.trim().to_string()
    } else {
        String::new()
    };
    if !badge.is_empty() && !badge.contains(HOLE) {
        return Err(format!("the badge of design '{}' has no {HOLE} hole for its hotkey", manifest.id));
    }
    let mut written = String::new();
    let mut rest = markup;
    while let Some(at) = rest.find(OPEN) {
        let start = at + OPEN.len();
        let end = rest[start..]
            .find(CLOSE)
            .map(|end| start + end)
            .ok_or_else(|| "a badge marker is never closed".to_string())?;
        let id = &rest[start..end];
        let hotkey = Hotkey::named(id).ok_or_else(|| format!("a badge marker names no hotkey '{id}'"))?;
        written.push_str(&rest[..at]);
        written.push_str(&badge.replace(HOLE, hotkey.id()));
        rest = &rest[end + CLOSE.len()..];
    }
    written.push_str(rest);
    Ok(written)
}

#[cfg(test)]
mod tests;
