//! Named values for a design. In a stylesheet, the design author writes
//! `design(surface)` for a colour, `design(scene-width)dp` for a size and
//! `design(version)` for the product's version.
//!
//! We take each value from the chosen palette, or from the `tokens` of the
//! design when the palette does not name it, so a design can have extra
//! colours without this code knowing their names. We replace characters
//! inside the rules of the design and append nothing, so nothing can
//! override the rules that the design author wrote.

use super::manifest::Manifest;
use crate::themes::Palette;
use std::collections::BTreeMap;

pub type Tokens = BTreeMap<String, String>;

/// The product values that a document can name. A design author writes
/// `design(version)` where the version number goes, so nobody has to edit
/// the design when the number changes.
pub fn product() -> Tokens {
    let mut tokens = Tokens::new();
    tokens.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    tokens
}

/// Every value named in the stylesheets of `manifest`, in `palette`.
pub fn design(manifest: &Manifest, palette: &Palette) -> Tokens {
    // The design's values first, so we can replace any with a palette value.
    let mut tokens = manifest.tokens.clone();
    tokens.extend(product());
    // The canvas on which we lay out every design, from the player contract.
    let (width, height) = super::contract::canvas();
    tokens.insert("canvas-width".to_string(), width.to_string());
    tokens.insert("canvas-height".to_string(), height.to_string());
    let m = manifest.scene;
    for (name, value) in [
        ("scene-width", m.scene_width),
        ("scene-height", m.scene_height),
        ("marker-diameter", m.marker),
        ("marker-radius", m.marker / 2),
        ("callout-width", m.callout_width),
        ("callout-height", m.callout_height),
        ("group-width", m.group_width),
        ("group-height", m.group_height),
    ] {
        tokens.insert(name.to_string(), value.to_string());
    }
    // The duration of the animation when an overlay leaves, in seconds. We
    // hide the element when this time is over, and the animation in the
    // design lasts exactly as long, because both come from one declaration.
    for overlay in &manifest.overlays {
        tokens.insert(
            format!("overlay-leave-{}", overlay.id),
            seconds(overlay.leave_ms),
        );
    }
    for (name, value) in [
        ("screen", &palette.screen),
        ("background", &palette.background),
        ("surface", &palette.surface),
        ("picture", &palette.picture),
        ("edge", &palette.edge),
        ("highlight", &palette.highlight),
        ("muted", &palette.muted),
        ("focus", &palette.focus),
    ] {
        tokens.insert(name.to_string(), value.clone());
    }
    // Last, so that for a name in both, we use the palette's value instead of
    // the design's default. The design author decides what each colour is
    // for, and the palette author picks the colour.
    for (name, value) in &palette.tokens {
        tokens.insert(name.clone(), value.clone());
    }
    tokens
}

/// `text` with every `design(name)` replaced by its value.
pub fn substitute(text: &str, tokens: &Tokens) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("design(") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "design(".len()..];
        let close = after
            .find(')')
            .ok_or_else(|| "a design( token is never closed".to_string())?;
        let name = after[..close].trim();
        let value = tokens.get(name).ok_or_else(|| {
            format!(
                "the stylesheet asks for design({name}), which the design does \
                 not declare and no palette names. Declared: {}",
                tokens.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;
        out.push_str(value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Seconds, as we write them in a stylesheet: `design(overlay-leave-notice)s`.
fn seconds(milliseconds: u32) -> String {
    let text = format!("{:.3}", milliseconds as f32 / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stylesheet is in seconds and we declare milliseconds. In RmlUi,
    /// either number is valid, so a wrong unit would cause no error.
    #[test]
    fn a_leaving_time_reaches_the_stylesheet_in_seconds() {
        assert_eq!(seconds(500), "0.5");
        assert_eq!(seconds(250), "0.25");
        assert_eq!(seconds(1000), "1");
        assert_eq!(seconds(0), "0");
        assert_eq!(seconds(120), "0.12");
    }

    #[test]
    fn an_undeclared_token_is_refused_by_name() {
        let error = substitute("a { color: design(nope); }", &product()).unwrap_err();
        assert!(error.contains("design(nope)"), "{error}");
    }
}
