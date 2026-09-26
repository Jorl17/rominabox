//! The controller scene: every offered pad's illustration, hit regions,
//! callouts and stick strips, the picker between pads and the bind list.
//!
//! We take the positions on the scene from `scene_layout`, as we do in the
//! builder and the overlay renderer, and write them here as markup.

use super::manifest::{Manifest, SceneMetrics};
use super::{contract, file_name, Content};
use std::path::{Path, PathBuf};

/// The scene for one game, and the files it needs beside the menu.
pub(crate) struct Scene {
    /// The chosen pad's scene, for the document's `<!--CONTROLS-->`.
    pub markup: String,
    /// The controller picker, empty when the console has one pad.
    pub picker: String,
    /// The rows the bind list is filled into.
    pub binds: String,
    /// Every offered pad's artwork and scene file.
    pub files: Vec<(PathBuf, Content)>,
}

/// Generate the hit regions and callouts for every controller of the console,
/// from the same declaration that we use in the builder.
///
/// We stage the illustration of every offered pad, and write its scene beside
/// the menu as `scene-<id>.rml`. Only the chosen one goes into the document,
/// and the others are there so the player can switch to them.
pub(crate) fn compose(
    artwork: &Path,
    manifest: &Manifest,
    system: &str,
    controls: &crate::controls::Controls,
) -> Result<Scene, String> {
    let metrics = manifest.scene;
    let profile = crate::controls::validate_for_system(system, controls)?;
    let offered = carried(system, &profile)?;
    let mut files: Vec<(PathBuf, Content)> = Vec::new();
    for image in offered
        .iter()
        .map(|entry| entry.image.as_str())
        .filter(|image| !image.is_empty())
    {
        // Two variants can share a drawing: a PlayStation Dual Analog is a
        // DualShock without the vibration.
        if !files.iter().any(|(name, _)| name == Path::new(image)) {
            files.push((PathBuf::from(image), Content::Copy(artwork.join(image))));
        }
    }
    if !profile.image.is_empty() {
        files.push((
            PathBuf::from("CONTROLLERS.txt"),
            Content::Copy(artwork.join("CONTROLLERS.txt")),
        ));
    }
    // We swap pads in the player by reading one of these files, because we
    // cannot generate markup there.
    for entry in &offered {
        files.push((
            PathBuf::from(file_name!(Scene, entry.id)),
            Content::Text(scene_markup(entry, controls, metrics)),
        ));
    }
    Ok(Scene {
        markup: scene_markup(&profile, controls, metrics),
        picker: controller_picker_markup(&offered, &profile.id),
        binds: bind_list_markup(manifest, &offered)?,
        files,
    })
}

/// Where the chosen pad's scene goes, inside `#controller-scene`.
pub(crate) const CONTROLS_SLOT: &str = "<!--CONTROLS-->";

/// Where the bind list goes, as a sibling of the scene like the picker, so
/// it stays when we replace the scene after someone swaps pads.
pub(crate) const BINDS_SLOT: &str = "<!--BINDS-->";

/// The element of the bind list, which we name in design.cfg for the player.
pub(crate) const BIND_LIST: &str = "control-binds";

/// One row for each input that the bundled pads can bind to a single control.
///
/// A `retro_keybind` contains a key, a button, an axis and a mouse button. A
/// stick is several controls drawn as one object, so its list has the inputs
/// of every direction. We write the rows now because we cannot create
/// elements in the player while the game runs. There we fill these rows and
/// hide the rest. There is always one row more than a page, so we can say
/// that the rest did not fit, even on a pad whose busiest control would fill
/// exactly one page.
fn bind_list_markup(
    manifest: &Manifest,
    profiles: &[crate::controls::ControlProfile],
) -> Result<String, String> {
    let template = crate::lists::row_template(&manifest.design)?;
    let page_size = manifest.list_page_size;
    let mut slots = 4usize;
    for profile in profiles {
        let mut groups: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for control in &profile.controls {
            if let Some(name) = control.group.as_deref() {
                *groups.entry(name).or_default() += 1;
            }
        }
        for size in groups.values() {
            slots = slots.max(size * 4);
        }
    }
    slots = slots.max(page_size.saturating_add(1));
    let items: Vec<crate::lists::ListItem> = (1..=slots)
        .map(|index| crate::lists::ListItem {
            id: format!("bind-{index}"),
            icon: String::new(),
            title: String::new(),
            detail: String::new(),
            state: String::new(),
            selected: false,
            accent: false,
            line: false,
        })
        .collect();
    // Hidden until someone chooses a control, with the id named in design.cfg.
    Ok(crate::lists::render_list_in(
        BIND_LIST,
        " style=\"display:none;\"",
        "binds",
        &template,
        &items,
        page_size,
        &manifest.words,
    ))
}

/// The pads in an export: every pad in the picker.
///
/// We use this one rule for the artwork staging, the scene files and the
/// picker, because they have to agree. We do not copy a console illustration
/// for a pad that is not in the picker.
///
/// With fewer than two there is no picker, and we export only the chosen pad.
fn carried(
    system: &str,
    profile: &crate::controls::ControlProfile,
) -> Result<Vec<crate::controls::ControlProfile>, String> {
    let offered = crate::controls::variants_for_system(system)?;
    let swappable = offered.len() > 1 && offered.iter().any(|entry| entry.id == profile.id);
    Ok(if swappable {
        offered
    } else {
        vec![profile.clone()]
    })
}

/// The marker for the controller picker in a design. It is separate from the
/// marker for the scene, because the picker is not part of the scene.
pub(crate) const PICKER_SLOT: &str = "<!--CONTROLLER-PICKER-->";

/// One controller's scene: its illustration, hit regions, callouts and groups.
///
/// We write it once for each offered pad, because a player who picks another
/// pad needs its scene, and we cannot generate markup in the game.
fn scene_markup(
    profile: &crate::controls::ControlProfile,
    controls: &crate::controls::Controls,
    metrics: SceneMetrics,
) -> String {
    let illustrated = !profile.image.is_empty();
    let mut markup = if illustrated {
        format!("<img id=\"controller-image\" src=\"{}\"/>", profile.image)
    } else {
        String::new()
    };
    // We draw a group once, as one object on the pad, not one callout per
    // bind. Otherwise each of the eight analogue directions of the PlayStation
    // DualShock would need a callout, and both gutters are already full with
    // seven 54 dp callouts.
    let placed_scene = crate::scene_layout::layout(&profile.controls, metrics);
    markup.push_str(&control_group_markup(
        &placed_scene.groups,
        &profile.controls,
        controls,
        illustrated,
        metrics.group_border,
    ));
    for item in profile.controls.iter().filter(|item| item.group.is_none()) {
        // We take the positions from the shared scene layout, as in the
        // builder and the overlay renderer.
        let placed = placed_scene
            .controls
            .iter()
            .find(|placement| placement.id == item.id)
            .expect("every drawn control is placed");
        let custom = controls.bindings.get(&item.id);
        let author_label = custom
            .and_then(|value| value.label.as_deref())
            .filter(|label| !label.trim().is_empty());
        let label = author_label.unwrap_or(&item.label);
        let key = callout_line(&binding_words(
            custom
                .and_then(|value| value.key.as_deref())
                .unwrap_or(&item.key),
            custom.and_then(|value| value.button.as_deref()),
            custom.and_then(|value| value.axis.as_deref()),
            custom.and_then(|value| value.mouse),
        ));
        let original = author_label
            .filter(|value| value.trim() != item.label.trim())
            .map(|_| item.label.as_str());
        let ring = if illustrated {
            markup.push_str(&leader_markup(&placed.leader));
            ring_markup(
                &item.id,
                placed.marker,
                placed.reach,
                placed.callout,
                metrics.callout_border,
            )
        } else {
            String::new()
        };
        markup.push('\n');
        markup.push_str(&control_callout_markup(
            &item.id,
            label,
            original,
            &key,
            placed.callout,
            &ring,
        ));
    }
    markup
}

/// The in-game controller picker.
///
/// We draw it at the place set in the design: the markup has no coordinates,
/// and we place it with `menu.rcss`. In the native design it is on the action
/// row beside BACK and RESET DEFAULTS, the one band that no console's pad
/// covers. Both gutters are full of callouts, the heading is at the top
/// centre, and the scene fills everything between.
///
/// We emit it only when there is a choice. A dropdown with one option is
/// noise, and most consoles have exactly one pad.
fn controller_picker_markup(offered: &[crate::controls::ControlProfile], chosen: &str) -> String {
    if offered.len() < 2 {
        return String::new();
    }
    let chosen_name = offered
        .iter()
        .find(|entry| entry.id == chosen)
        .map(|entry| crate::lists::rml_text(&entry.name.to_uppercase()))
        .unwrap_or_default();
    let (picker, label, current, list) = (
        contract!(ControlsDevice),
        contract!(ControlPickerLabel),
        contract!(ControlsDeviceCurrent),
        contract!(ControlsDeviceList),
    );
    let mut markup = format!(
        r#"
<div id="{picker}" class="control-picker">
<div id="controls-device-label" class="{label}">CONTROLLER</div>
<button id="{current}" class="{picker_current}">{chosen_name}</button>
<div id="{list}" class="control-picker-list" style="display:none;">
"#,
        picker_current = contract!(ControlPickerCurrent),
    );
    for entry in offered {
        let selected = if entry.id == chosen {
            format!(" {}", contract!(Selected))
        } else {
            String::new()
        };
        markup.push_str(&format!(
            r#"<button id="{}{}" class="control-picker-option{selected}">{}</button>
"#,
            contract!(ControlsDeviceOptionPrefix),
            entry.id,
            crate::lists::rml_text(&entry.name.to_uppercase()),
        ));
    }
    markup.push_str(
        "</div>
</div>
",
    );
    markup
}

/// A control's leader, at its place from `scene_layout`, written into the
/// scene just before the stop it leads to. We draw the stop after it, so the
/// stop covers its end, and a pointer on the leader is on no stop.
fn leader_markup(leader: &[crate::scene_layout::Segment]) -> String {
    let mut markup = String::new();
    for run in leader {
        let (orientation, extent) = if run.height == 0 {
            ("horizontal", format!("width:{}dp;", run.width))
        } else {
            ("vertical", format!("height:{}dp;", run.height))
        };
        markup.push_str(&format!(
            "\n<div class=\"control-leader {orientation}\" style=\"left:{}dp;top:{}dp;{extent}\"/>",
            run.x, run.y
        ));
    }
    markup
}

/// The ring over the button of a control on the pad, at the position from
/// `scene_layout`, written inside the focus stop for the control. A design
/// lights it by the state of that stop. The pointer reaches the stop through
/// the hit area of the control, never through the ring, because the square
/// box of a ring is wider than the space between neighbouring buttons.
///
/// RmlUi places a child from the padding edge of its parent, which is
/// `border` inside the box that `scene_layout` computed for the stop.
fn ring_markup(
    id: &str,
    marker: crate::scene_layout::Rect,
    reach: crate::scene_layout::Rect,
    stop: crate::scene_layout::Rect,
    border: i32,
) -> String {
    let (left, top) = (stop.x + border, stop.y + border);
    format!(
        "<div id=\"control-hit-{id}\" class=\"control-hit\" style=\"left:{}dp;top:{}dp;pointer-events:none;\"/>\
         <div id=\"control-reach-{id}\" class=\"control-reach\" style=\"position:absolute;left:{}dp;top:{}dp;width:{}dp;height:{}dp;\"/>",
        marker.x - left,
        marker.y - top,
        reach.x - left,
        reach.y - top,
        reach.width,
        reach.height,
    )
}

/// Draw each group once, beneath the illustration, in its strip from
/// `scene_layout`, so that we place it in the same way in this markup, in the
/// builder and in `scripts/render_control_overlays.py`.
fn control_group_markup(
    groups: &[crate::scene_layout::GroupPlacement],
    all: &[crate::controls::ControlDefinition],
    controls: &crate::controls::Controls,
    illustrated: bool,
    border: i32,
) -> String {
    let mut markup = String::new();
    for group in groups {
        let name = group.name.as_str();
        let ring = match (&group.anchor, group.marker, group.reach) {
            (Some(anchor), Some(marker), Some(reach)) if illustrated => {
                markup.push_str(&leader_markup(&group.leader));
                ring_markup(anchor, marker, reach, group.strip, border)
            }
            _ => String::new(),
        };
        let mut words = Vec::new();
        for item in all
            .iter()
            .filter(|item| item.group.as_deref() == Some(name))
        {
            let custom = controls.bindings.get(&item.id);
            words.extend(binding_words(
                custom
                    .and_then(|value| value.key.as_deref())
                    .unwrap_or(&item.key),
                custom.and_then(|value| value.button.as_deref()),
                custom.and_then(|value| value.axis.as_deref()),
                custom.and_then(|value| value.mouse),
            ));
        }
        let title = name.replace('_', " ").to_uppercase();
        markup.push_str(&format!(
            r#"
<button id="{group_id}{name}" class="{group_class}" style="left:{x}dp;top:{y}dp;">
<div class="control-label">{}</div>
<div id="{binding_id}{name}" class="control-assignment">{}</div>{ring}
</button>
"#,
            crate::lists::rml_text(&title),
            crate::lists::rml_text(&callout_line(&words)),
            group_id = contract!(ControlGroupPrefix),
            group_class = contract!(ControlGroup),
            binding_id = contract!(ControlGroupBindingPrefix),
            x = group.strip.x,
            y = group.strip.y,
        ));
    }
    markup
}

/// The words that a callout can contain about one control, in list order.
fn binding_words(
    key: &str,
    button: Option<&str>,
    axis: Option<&str>,
    mouse: Option<u32>,
) -> Vec<String> {
    let mut words = Vec::new();
    if !key.is_empty() && key != "nul" {
        words.push(key.to_string());
    }
    if let Some(button) = button.filter(|value| !value.is_empty()) {
        words.push(format!("Button {button}"));
    }
    if let Some(axis) = axis.filter(|value| !value.is_empty()) {
        words.push(format!("Axis {axis}"));
    }
    if let Some(mouse) = mouse {
        words.push(match mouse {
            2 => "Left".to_string(),
            3 => "Right".to_string(),
            4 => "Wheel up".to_string(),
            5 => "Wheel down".to_string(),
            6 => "Middle".to_string(),
            other => format!("Mouse {other}"),
        });
    }
    words
}

/// Every binding, separated by commas. One binding stays that binding, and
/// none is a dash. We show no count such as "3 binds", which the player cannot see.
fn callout_line(words: &[String]) -> String {
    match words {
        [] => "---".to_string(),
        [only] => only.clone(),
        many => many.join(", "),
    }
}

fn control_callout_markup(
    id: &str,
    label: &str,
    original: Option<&str>,
    key: &str,
    callout: crate::scene_layout::Rect,
    ring: &str,
) -> String {
    let label = crate::lists::rml_text(label);
    let key = crate::lists::rml_text(key);
    let original = original
        .map(|value| {
            format!(
                r#"<span class="control-original">{}</span>"#,
                crate::lists::rml_text(value)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<button id="{stop}{id}" class="{class}" style="left:{}dp;top:{}dp;"><div id="{label_id}{id}" class="control-label">{label}</div><div class="control-assignment">{original}<span id="{binding_id}{id}">{key}</span></div>{ring}</button>"#,
        callout.x,
        callout.y,
        stop = contract!(ControlPrefix),
        class = contract!(ControlCallout),
        label_id = contract!(ControlLabelPrefix),
        binding_id = contract!(ControlBindingPrefix),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A callout with one binding of a control that has three is false, and a
    /// count with no bindings refers to something the player cannot see.
    #[test]
    fn a_callout_names_every_binding_and_never_a_count() {
        let three = binding_words("up", Some("0"), Some("+0"), None);
        let line = callout_line(&three);
        assert_eq!(line, "up, Button 0, Axis +0");
        assert!(line.starts_with("up"), "the first binding is visible");
        assert!(line.contains("Button 0") && line.contains("Axis +0"));
        assert!(!line.contains("bind"), "a count is not a binding: {line}");
        assert_eq!(callout_line(&binding_words("c", None, None, None)), "c");
        assert_eq!(callout_line(&[]), "---");
    }
}
