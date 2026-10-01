//! The controller scene we write in an export agrees with `scene_layout`,
//! which we also use in the builder and the overlay renderer.

mod support;

use rominabox_desktop::{controls, menu, scene_layout, themes};
use std::fs;
use support::{designs, kit};

/// `left` and `top` of the element with `id`, in dp, from its inline style.
fn placed(scene: &str, id: &str) -> Option<(i32, i32)> {
    let at = scene.find(&format!("id=\"{id}\""))?;
    let tag = &scene[at..at + scene[at..].find('>')?];
    let style = tag.split("style=\"").nth(1)?.split('"').next()?;
    let value = |name: &str| -> Option<i32> {
        style
            .split(';')
            .find_map(|rule| rule.trim().strip_prefix(&format!("{name}:")))?
            .trim()
            .strip_suffix("dp")?
            .parse()
            .ok()
    };
    Some((value("left")?, value("top")?))
}

/// The markup of a stop, from its opening tag to the end of the element,
/// with the stop's ring and leader runs inside it.
fn stop_markup<'a>(scene: &'a str, id: &str) -> Option<&'a str> {
    let at = scene.find(&format!("id=\"{id}\""))?;
    let end = at + scene[at..].find("</button>")?;
    Some(&scene[at..end])
}

/// A stick's ring is where `scene_layout` puts it, at the marker radius of
/// the design, in every design, including the 30dp ring in Disc.
///
/// We write the ring inside the stick's box, which takes the focus, so we
/// place it from the padding edge of the box: the box's position and its
/// declared border in from the scene coordinates.
#[test]
fn a_sticks_ring_is_where_the_scene_layout_puts_it() {
    let root = rominabox_scratch::Scratch::dir("rominabox-stick-rings");
    let kit = kit(&root);
    for design in designs() {
        let destination = root.join(&design);
        support::compose_for(&kit, &design, "ps1", &destination);
        let metrics = menu::scene_metrics(&themes::staged_design(&kit, &design)).unwrap();
        let mut checked = 0;
        for profile in controls::variants_for_system("ps1").unwrap() {
            let scene = fs::read_to_string(destination.join(format!("scene-{}.rml", profile.id)))
                .unwrap_or_else(|error| panic!("{design}/{}: {error}", profile.id));
            let layout = scene_layout::layout(&profile, metrics);
            for group in &layout.groups {
                let Some(marker) = group.marker else { continue };
                let anchor = profile
                    .controls
                    .iter()
                    .find(|c| {
                        c.group.as_deref() == Some(group.name.as_str()) && (c.x != 0 || c.y != 0)
                    })
                    .unwrap();
                let id = format!("control-hit-{}", anchor.id);
                let strip = format!("control-group-{}", group.name);
                assert_eq!(
                    placed(&scene, &strip),
                    Some((group.strip.x, group.strip.y)),
                    "{design}/{}: #{strip}",
                    profile.id
                );
                let inside = stop_markup(&scene, &strip)
                    .unwrap_or_else(|| panic!("{design}/{}: no #{strip}", profile.id));
                assert_eq!(
                    placed(inside, &id),
                    Some((
                        marker.x - group.strip.x - metrics.group_border,
                        marker.y - group.strip.y - metrics.group_border
                    )),
                    "{design}/{}: #{id} is not inside #{strip} where the scene layout puts the ring",
                    profile.id
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "{design}: no stick was checked");
    }
}

/// Every pad we draw, as `desktop/controls.json` declares it.
fn illustrated_profiles() -> Vec<controls::ControlProfile> {
    #[derive(serde::Deserialize)]
    struct Registry {
        profiles: Vec<controls::ControlProfile>,
    }
    let declared = fs::read_to_string(rominabox_desktop::repo::at("desktop/controls.json")).unwrap();
    let registry: Registry = serde_json::from_str(&declared).unwrap();
    registry
        .profiles
        .into_iter()
        .filter(|profile| !profile.image.is_empty())
        .collect()
}

/// Every design's scene geometry, from the design itself.
fn design_metrics() -> Vec<(String, menu::SceneMetrics)> {
    designs()
        .into_iter()
        .map(|design| {
            let package = rominabox_desktop::repo::at("integrations/designs").join(&design);
            (design, menu::scene_metrics(&package).unwrap())
        })
        .collect()
}

/// Return whether a leader run passes through the inside of a painted box. A
/// run that ends on the box's edge meets it, and one that enters it crosses it.
fn crosses(run: &scene_layout::Segment, left: i32, top: i32, right: i32, bottom: i32) -> bool {
    let (x0, x1) = (run.x, run.x + run.width);
    let (y0, y1) = (run.y, run.y + run.height);
    x0.max(left) < x1.min(right) && y0 > top && y0 < bottom
        || y0.max(top) < y1.min(bottom) && x0 > left && x0 < right
}

/// No leader runs across a stick's box. On the PlayStation pad, Select and
/// Start leaders drawn along their callouts' midlines would cross both
/// boxes, over the bindings written in them.
#[test]
fn no_leader_crosses_a_stick_box_on_any_pad() {
    let mut crossings = Vec::new();
    let mut boxes = 0;
    for (design, metrics) in design_metrics() {
        for profile in illustrated_profiles() {
            let layout = scene_layout::layout(&profile, metrics);
            let runs = layout
                .controls
                .iter()
                .map(|placed| (format!("control-{}", placed.id), &placed.leader))
                .chain(
                    layout
                        .groups
                        .iter()
                        .map(|group| (format!("control-group-{}", group.name), &group.leader)),
                );
            let runs: Vec<_> = runs.collect();
            for group in &layout.groups {
                boxes += 1;
                let painted = 2 * metrics.group_border;
                let (left, top) = (group.strip.x, group.strip.y);
                let (right, bottom) = (left + group.strip.width + painted, top + group.strip.height + painted);
                for (owner, leader) in &runs {
                    for run in leader.iter() {
                        if crosses(run, left, top, right, bottom) {
                            crossings.push(format!(
                                "{design}/{}: a leader run of {owner} at {},{} {}x{} crosses the {} box {left},{top}..{right},{bottom}",
                                profile.id, run.x, run.y, run.width, run.height, group.name
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(boxes > 0, "no pad with a stick was checked");
    assert!(crossings.is_empty(), "{}", crossings.join("\n"));
}

/// The text of the element with `id`'s first `control-label`, from the
/// composed scene.
fn box_title<'a>(scene: &'a str, id: &str) -> Option<&'a str> {
    let inside = stop_markup(scene, id)?;
    let label = inside.split("class=\"control-label\">").nth(1)?;
    label.split('<').next()
}

/// A stick's box shows the name its pad gives the stick. The group's id is
/// not that name: GameCube's C-stick has the id of a right stick, and a pad
/// with one stick has the id of a left stick.
#[test]
fn a_sticks_box_reads_the_name_its_pad_gives_it() {
    let root = rominabox_scratch::Scratch::dir("rominabox-stick-titles");
    let kit = kit(&root);
    let expected = [
        ("gamecube", "l_stick", "Control stick"),
        ("gamecube", "r_stick", "C-stick"),
        ("dreamcast", "l_stick", "Stick"),
        ("n64", "l_stick", "Stick"),
        ("ps1", "l_stick", "Left stick"),
        ("ps1", "r_stick", "Right stick"),
    ];
    let mut wrong = Vec::new();
    for design in designs() {
        for (system, group, title) in expected {
            let destination = root.join(&design).join(system);
            let composed = support::compose_for(&kit, &design, system, &destination);
            let id = format!("control-group-{group}");
            let read = box_title(&composed.menu, &id);
            if read != Some(title) {
                wrong.push(format!("{design}/{system}: #{id} reads {read:?}, not {title:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Whether a leader run passes inside the ring drawn over a button: the
/// circle that fills `marker`. A run that only touches it does not.
fn through_ring(run: &scene_layout::Segment, marker: &scene_layout::Rect) -> bool {
    let radius = marker.width / 2;
    let (x, y) = (marker.x + radius, marker.y + radius);
    let nearest_x = x.clamp(run.x, run.x + run.width);
    let nearest_y = y.clamp(run.y, run.y + run.height);
    (nearest_x - x).pow(2) + (nearest_y - y).pow(2) < radius * radius
}

/// Whether two runs lie along each other: both horizontal or both vertical,
/// closer than `apart` across, over a shared stretch.
fn along(first: &scene_layout::Segment, second: &scene_layout::Segment, apart: i32) -> bool {
    let shared = |a0: i32, a1: i32, b0: i32, b1: i32| a0.max(b0) < a1.min(b1);
    match (first.height == 0, second.height == 0) {
        (true, true) => {
            (first.y - second.y).abs() < apart
                && shared(first.x, first.x + first.width, second.x, second.x + second.width)
        }
        (false, false) => {
            (first.x - second.x).abs() < apart
                && shared(first.y, first.y + first.height, second.y, second.y + second.height)
        }
        _ => false,
    }
}

/// No leader runs through the ring over another button, and no two leaders
/// run along each other. The cases include GameCube's B near the C-stick's
/// ring, and Dreamcast's stick near D-pad Left's line and ring.
#[test]
fn no_leader_runs_through_another_ring_or_along_another_leader() {
    let mut faults = Vec::new();
    let mut checked = 0;
    for (design, metrics) in design_metrics() {
        // Runs closer than a leader's own breadth read as one line.
        let apart = 4;
        for profile in illustrated_profiles() {
            let layout = scene_layout::layout(&profile, metrics);
            let drawn: Vec<(String, scene_layout::Rect, &Vec<scene_layout::Segment>)> = layout
                .controls
                .iter()
                .map(|placed| (placed.id.clone(), placed.marker, &placed.leader))
                .chain(layout.groups.iter().filter_map(|group| {
                    group.marker.map(|marker| (group.name.clone(), marker, &group.leader))
                }))
                .collect();
            checked += drawn.len();
            for (owner, _, leader) in &drawn {
                for run in leader.iter() {
                    for (other, marker, other_leader) in &drawn {
                        if other == owner {
                            continue;
                        }
                        if through_ring(run, marker) {
                            faults.push(format!(
                                "{design}/{}: {owner}'s run at {},{} {}x{} goes through {other}'s ring",
                                profile.id, run.x, run.y, run.width, run.height
                            ));
                        }
                        if other_leader.iter().any(|theirs| along(run, theirs, apart)) {
                            faults.push(format!(
                                "{design}/{}: {owner}'s run at {},{} {}x{} lies along {other}'s leader",
                                profile.id, run.x, run.y, run.width, run.height
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 0, "no leader was checked");
    assert!(faults.is_empty(), "{} faults:\n{}", faults.len(), faults.join("\n"));
}
