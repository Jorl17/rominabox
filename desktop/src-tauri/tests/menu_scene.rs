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
            let layout = scene_layout::layout(&profile.controls, metrics);
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
