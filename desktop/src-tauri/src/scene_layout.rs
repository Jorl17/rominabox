//! Where everything on the controller scene goes.
//!
//! This is the only implementation of the scene layout. We draw it in the
//! exporter and read it in the builder and the overlay renderer, so every
//! leader line starts at the same edge in all three. To change how we route
//! a leader, change this file and its `routes`, and nowhere else.

use crate::controls::{ControlDefinition, ControlProfile, StickDirection};
use crate::menu::SceneMetrics;
use routes::{Axis, End, Scene};
use serde::Serialize;

mod routes;

/// A rectangle on the scene, in the scene's own coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// One straight run of a leader line. Horizontal runs have height 0 and
/// vertical runs have width 0. We give them their thickness when we render,
/// and the thickness comes from the design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Everything we draw for one control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Placement {
    pub id: String,
    /// The ring over the button on the artwork.
    pub marker: Rect,
    /// Where the pointer reaches the control on the artwork; see [`reach`].
    pub reach: Rect,
    /// The label box in a gutter.
    pub callout: Rect,
    /// The line joining them.
    pub leader: Vec<Segment>,
}

/// A stick, which we draw once below the pad instead of once per direction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GroupPlacement {
    pub name: String,
    /// The stick's name, as declared in the pad profile.
    pub title: String,
    pub strip: Rect,
    /// The member that has the group's position on the pad, if one does.
    pub anchor: Option<String>,
    /// Present when one member of the group has the anchor on the pad.
    pub marker: Option<Rect>,
    /// Where the pointer reaches the stick on the artwork, with the marker.
    pub reach: Option<Rect>,
    pub leader: Vec<Segment>,
    /// One per member, on the marker; none without one.
    pub marks: Vec<Mark>,
}

/// Where we mark a stick member on the pad: an arrow on the side of the
/// marker its direction points to, or the marker's centre for the click.
/// The size and form of the mark come from the design.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Mark {
    /// The member's control id.
    pub id: String,
    pub direction: StickDirection,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneLayout {
    pub scene: Rect,
    pub controls: Vec<Placement>,
    pub groups: Vec<GroupPlacement>,
}

/// The side of a callout where its leader starts.
///
/// The inner edge: the right side for a callout in the left gutter, the left
/// side for one in the right gutter. We measure it on the painted box, with
/// borders, because that is where a person sees the line meet the box.
fn inner_edge(callout_x: i32, metrics: SceneMetrics) -> i32 {
    let painted = metrics.callout_width + 2 * metrics.callout_border;
    if callout_x < metrics.scene_width / 2 {
        callout_x + painted
    } else {
        callout_x
    }
}

/// The L from a callout to its button: out of the callout's inner edge
/// along the callout's own midline, then up or down the button's column.
/// The route we use when we find no clear one. We find the others in
/// [`routes`].
fn leader(control: &ControlDefinition, metrics: SceneMetrics) -> Vec<Segment> {
    routes::runs(&[
        (inner_edge(control.callout_x, metrics), midline(control, metrics)),
        (control.x, midline(control, metrics)),
        (control.x, control.y),
    ])
}

fn midline(control: &ControlDefinition, metrics: SceneMetrics) -> i32 {
    control.callout_y + metrics.callout_height / 2 + metrics.callout_border / 2
}

/// Whether a run passes through the inside of a painted box. A run that ends
/// on the box's edge meets it, and one that enters it crosses it.
fn crosses(run: &Segment, painted: &Rect) -> bool {
    let (left, top) = (painted.x, painted.y);
    let (right, bottom) = (left + painted.width, top + painted.height);
    if run.height == 0 {
        top < run.y && run.y < bottom && run.x.max(left) < (run.x + run.width).min(right)
    } else {
        left < run.x && run.x < right && run.y.max(top) < (run.y + run.height).min(bottom)
    }
}

/// The straight drop from a stick to its box: down the stick's column to
/// the top of the box, then along that edge to the box when the stick is
/// beside it. We use it for a stick's leader when we find no clear route.
fn stick_leader(anchor: &ControlDefinition, strip: Rect, metrics: SceneMetrics) -> Vec<Segment> {
    let painted = strip.width + 2 * metrics.group_border;
    let edge = anchor.x.clamp(strip.x, strip.x + painted);
    routes::runs(&[(anchor.x, anchor.y), (anchor.x, strip.y), (edge, strip.y)])
}

/// Where a stick's leader may meet its box: on the side of the box that
/// faces the stick, anywhere along it but its corners.
fn stick_ends(anchor: &ControlDefinition, painted: &Rect, scene: &Scene) -> Vec<End> {
    let (left, top) = (painted.x, painted.y);
    let (right, bottom) = (left + painted.width, top + painted.height);
    let along = |axis: Axis, from: i32, to: i32| scene.turns(axis, (from + to) / 2, (from + 1, to - 1));
    let mut ends = Vec::new();
    for (faces, row) in [(anchor.y < top, top), (anchor.y > bottom, bottom)] {
        if faces {
            ends.extend(along(Axis::Across, left, right).into_iter().map(|x| End {
                at: (x, row),
                last: Some(Axis::Down),
            }));
        }
    }
    for (faces, column) in [(anchor.x < left, left), (anchor.x > right, right)] {
        if faces {
            ends.extend(along(Axis::Down, top, bottom).into_iter().map(|y| End {
                at: (column, y),
                last: Some(Axis::Across),
            }));
        }
    }
    ends
}

/// Where each stick's box goes when its pad profile gives no place: side by
/// side, centred, along the bottom of the scene, in name order.
fn strips(names: &[&str], metrics: SceneMetrics) -> Vec<Rect> {
    let count = names.len() as i32;
    let total = count * metrics.group_width + (count - 1) * metrics.group_gap;
    let top = metrics.scene_height - metrics.group_height - metrics.group_bottom_margin;
    (0..count)
        .map(|index| Rect {
            x: (metrics.scene_width - total) / 2 + index * (metrics.group_width + metrics.group_gap),
            y: top,
            width: metrics.group_width,
            height: metrics.group_height,
        })
        .collect()
}

/// A box as we draw it, borders included.
fn painted(strip: &Rect, border: i32) -> Rect {
    Rect {
        width: strip.width + 2 * border,
        height: strip.height + 2 * border,
        ..*strip
    }
}

fn marker(x: i32, y: i32, metrics: SceneMetrics) -> Rect {
    let radius = metrics.marker / 2;
    Rect {
        x: x - radius,
        y: y - radius,
        width: metrics.marker,
        height: metrics.marker,
    }
}

/// Each member's mark on a stick's marker, in the members' order: a member the
/// stick does not have has no mark.
fn marks<'a>(members: impl Iterator<Item = &'a ControlDefinition>, marker: Rect) -> Vec<Mark> {
    let (left, top) = (marker.x, marker.y);
    let (right, bottom) = (left + marker.width, top + marker.height);
    let (centre_x, centre_y) = (left + marker.width / 2, top + marker.height / 2);
    members
        .filter_map(|member| {
            let direction = member.direction?;
            let (x, y) = match direction {
                StickDirection::Up => (centre_x, top),
                StickDirection::Right => (right, centre_y),
                StickDirection::Down => (centre_x, bottom),
                StickDirection::Left => (left, centre_y),
                StickDirection::Press => (centre_x, centre_y),
            };
            Some(Mark {
                id: member.id.clone(),
                direction,
                x,
                y,
            })
        })
        .collect()
}

/// Where the pointer reaches a control on the pad: the largest square,
/// centred on its button, that lies inside the ring we draw over it and
/// nearer its button than any other.
///
/// The ring cannot be that area itself. In RmlUi the pointer area of an
/// element is its whole square box, and a ring is wider than the space
/// between neighbouring buttons (a D-pad's arrows are closer together than
/// one ring's diameter), so neighbouring rings would overlap as squares and
/// a pointer nearer Up would reach Left.
fn reach(button: usize, buttons: &[(i32, i32)], metrics: SceneMetrics) -> Rect {
    let (x, y) = buttons[button];
    let nearest = buttons
        .iter()
        .enumerate()
        .filter(|(other, _)| *other != button)
        .map(|(_, (ox, oy))| f64::from(ox - x).hypot(f64::from(oy - y)))
        .fold(f64::INFINITY, f64::min);
    let limit = (f64::from(metrics.marker) / 2.0).min(nearest / 2.0);
    let half = (limit / std::f64::consts::SQRT_2).floor() as i32;
    Rect {
        x: x - half,
        y: y - half,
        width: 2 * half,
        height: 2 * half,
    }
}

/// Where everything on `profile`'s scene goes, in a frame of `metrics`.
pub fn layout(profile: &ControlProfile, metrics: SceneMetrics) -> SceneLayout {
    let controls = profile.controls.as_slice();
    let drawn: Vec<&ControlDefinition> =
        controls.iter().filter(|c| c.group.is_none()).collect();

    let mut names: Vec<&str> = controls
        .iter()
        .filter_map(|control| control.group.as_deref())
        .collect();
    names.sort_unstable();
    names.dedup();
    let anchors: Vec<Option<&ControlDefinition>> = names
        .iter()
        .map(|name| {
            // One anchor for the whole stick, on whichever member has a position
            // on the pad.
            controls
                .iter()
                .find(|c| c.group.as_deref() == Some(*name) && (c.x != 0 || c.y != 0))
        })
        .collect();
    // When the pad profile gives a place on the anchor, the stick's box goes
    // there, as for every other control. A pad with something at its bottom
    // centre, such as the N64's Z trigger, has that place in its profile.
    let strips: Vec<Rect> = strips(&names, metrics)
        .into_iter()
        .zip(&anchors)
        .map(|(strip, anchor)| match anchor {
            Some(anchor) if (anchor.callout_x, anchor.callout_y) != (0, 0) => Rect {
                x: anchor.callout_x,
                y: anchor.callout_y,
                ..strip
            },
            _ => strip,
        })
        .collect();
    let boxes: Vec<Rect> = strips
        .iter()
        .map(|strip| painted(strip, metrics.group_border))
        .collect();

    // Every button on the pad, each stick once, for the pointer areas: the
    // controls we draw, in order, then each stick's anchor.
    let buttons: Vec<(i32, i32)> = drawn
        .iter()
        .copied()
        .chain(anchors.iter().flatten().copied())
        .map(|control| (control.x, control.y))
        .collect();
    let mut reaches = (0..buttons.len()).map(|button| reach(button, &buttons, metrics));

    let bounds = Rect {
        x: 0,
        y: 0,
        width: metrics.scene_width,
        height: metrics.scene_height,
    };
    let callout = |control: &ControlDefinition| Rect {
        x: control.callout_x,
        y: control.callout_y,
        width: metrics.callout_width,
        height: metrics.callout_height,
    };
    // Every ring and box on the pad, by the control or stick it is for, for
    // the leaders to keep clear of.
    let mut scene = Scene {
        bounds,
        rings: drawn
            .iter()
            .map(|control| (control.id.clone(), marker(control.x, control.y, metrics)))
            .chain(names.iter().zip(&anchors).filter_map(|(name, anchor)| {
                anchor.map(|anchor| ((*name).to_string(), marker(anchor.x, anchor.y, metrics)))
            }))
            .collect(),
        boxes: drawn
            .iter()
            .map(|control| (control.id.clone(), painted(&callout(control), metrics.callout_border)))
            .chain(names.iter().zip(&boxes).map(|(name, painted)| ((*name).to_string(), *painted)))
            .collect(),
        runs: Vec::new(),
    };

    // We route the sticks first, because they have the fewest ways to their
    // boxes, then every control in the pad's order, clear of earlier leaders.
    let stick_leaders: Vec<Vec<Segment>> = names
        .iter()
        .zip(&anchors)
        .zip(&strips)
        .map(|((name, anchor), strip)| {
            let Some(anchor) = anchor else {
                return Vec::new();
            };
            let ends = stick_ends(anchor, &painted(strip, metrics.group_border), &scene);
            let route = scene.route(
                name,
                (anchor.x, anchor.y),
                None,
                &ends,
                stick_leader(anchor, *strip, metrics),
            );
            scene.runs.extend(route.iter().map(|run| ((*name).to_string(), *run)));
            route
        })
        .collect();
    let placements: Vec<Placement> = drawn
        .iter()
        .map(|control| {
            let from = (inner_edge(control.callout_x, metrics), midline(control, metrics));
            let end = End {
                at: (control.x, control.y),
                last: None,
            };
            let route = scene.route(
                &control.id,
                from,
                Some(Axis::Across),
                &[end],
                leader(control, metrics),
            );
            scene.runs.extend(route.iter().map(|run| (control.id.clone(), *run)));
            Placement {
                id: control.id.clone(),
                marker: marker(control.x, control.y, metrics),
                reach: reaches.next().expect("a reach for every drawn control"),
                callout: callout(control),
                leader: route,
            }
        })
        .collect();

    let groups = names
        .iter()
        .zip(strips)
        .zip(anchors)
        .zip(stick_leaders)
        .map(|(((name, strip), anchor), leader)| {
            let reach = anchor.map(|_| reaches.next().expect("a reach for every stick's anchor"));
            let ring = anchor.map(|anchor| marker(anchor.x, anchor.y, metrics));
            let members = controls
                .iter()
                .filter(|control| control.group.as_deref() == Some(*name));
            GroupPlacement {
                name: (*name).to_string(),
                title: profile
                    .groups
                    .get(*name)
                    .map(|group| group.title.clone())
                    .expect("the catalog titles every stick"),
                strip,
                anchor: anchor.map(|anchor| anchor.id.clone()),
                marker: ring,
                reach,
                leader,
                marks: ring.map(|ring| marks(members, ring)).unwrap_or_default(),
            }
        })
        .collect();

    SceneLayout {
        scene: bounds,
        controls: placements,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

fn vertical(at_x: i32, from_y: i32, to_y: i32) -> Segment {
    Segment {
        x: at_x,
        y: from_y.min(to_y),
        width: 0,
        height: (from_y - to_y).abs(),
    }
}

    fn metrics() -> SceneMetrics {
        SceneMetrics {
            scene_width: 960,
            scene_height: 380,
            callout_width: 196,
            callout_height: 50,
            callout_border: 2,
            marker: 42,
            group_width: 236,
            group_height: 62,
            group_border: 2,
            group_gap: 16,
            group_bottom_margin: 12,
        }
    }

    /// A pad of `controls`, its one stick titled.
    fn pad(controls: Vec<ControlDefinition>) -> ControlProfile {
        ControlProfile {
            id: "pad".into(),
            name: "Pad".into(),
            systems: Vec::new(),
            image: String::new(),
            core_device: None,
            controls,
            groups: [("l_stick".to_string(), crate::controls::ControlGroup { title: "Stick".into() })]
                .into(),
        }
    }

    fn stick(x: i32) -> Vec<ControlDefinition> {
        let member = |id: &str, x: i32, y: i32| ControlDefinition {
            id: id.to_string(),
            label: id.to_string(),
            key: String::new(),
            group: Some("l_stick".to_string()),
            direction: Some(if id == "l3" {
                crate::controls::StickDirection::Press
            } else {
                crate::controls::StickDirection::Right
            }),
            x,
            y,
            callout_x: 0,
            callout_y: 0,
        };
        vec![member("l_x_plus", 0, 0), member("l3", x, 200)]
    }

    /// A stick's members, in the order given, all on the pad at `at` or
    /// all off it.
    fn members(at: Option<(i32, i32)>, directions: &[(&str, StickDirection)]) -> Vec<ControlDefinition> {
        let (x, y) = at.unwrap_or((0, 0));
        directions
            .iter()
            .map(|(id, direction)| ControlDefinition {
                id: id.to_string(),
                label: id.to_string(),
                key: String::new(),
                group: Some("l_stick".to_string()),
                direction: Some(*direction),
                x,
                y,
                callout_x: 0,
                callout_y: 0,
            })
            .collect()
    }

    fn mark(id: &str, direction: StickDirection, x: i32, y: i32) -> Mark {
        Mark {
            id: id.to_string(),
            direction,
            x,
            y,
        }
    }

    /// We mark every member of a stick on its ring: each direction on the
    /// side of the ring it points to, and the click in the middle, in the
    /// members' order.
    #[test]
    fn every_member_of_a_stick_is_marked_on_its_ring() {
        use StickDirection::*;
        let stick = members(
            Some((420, 200)),
            &[("up", Up), ("right", Right), ("down", Down), ("left", Left), ("press", Press)],
        );
        let group = layout(&pad(stick), metrics()).groups[0].clone();
        assert_eq!(group.marker, Some(Rect { x: 399, y: 179, width: 42, height: 42 }));
        assert_eq!(
            group.marks,
            vec![
                mark("up", Up, 420, 179),
                mark("right", Right, 441, 200),
                mark("down", Down, 420, 221),
                mark("left", Left, 399, 200),
                mark("press", Press, 420, 200),
            ]
        );
    }

    /// A pad profile may give a place for a stick's box on the stick's anchor,
    /// as for every other control's box.
    #[test]
    fn a_stick_box_goes_where_its_pad_says() {
        use StickDirection::*;
        let mut stick = members(Some((420, 200)), &[("up", Up), ("right", Right)]);
        stick[0].callout_x = 230;
        stick[0].callout_y = 240;
        let group = layout(&pad(stick), metrics()).groups[0].clone();
        assert_eq!((group.strip.x, group.strip.y), (230, 240));
        let default = layout(&pad(members(Some((420, 200)), &[("up", Up), ("right", Right)])), metrics())
            .groups[0]
            .strip;
        assert_ne!((default.x, default.y), (230, 240), "without one it keeps the bottom centre");
    }

    /// A stick without a click has no mark in the middle.
    #[test]
    fn a_stick_without_a_click_has_no_press_mark() {
        use StickDirection::*;
        let stick = members(
            Some((420, 200)),
            &[("up", Up), ("right", Right), ("down", Down), ("left", Left)],
        );
        let marks = layout(&pad(stick), metrics()).groups[0].marks.clone();
        assert_eq!(
            marks.iter().map(|mark| mark.direction).collect::<Vec<_>>(),
            vec![Up, Right, Down, Left]
        );
    }

    /// A stick with no place on the pad has no ring to mark.
    #[test]
    fn a_stick_off_the_pad_has_no_marks() {
        use StickDirection::*;
        let stick = members(None, &[("up", Up), ("right", Right), ("press", Press)]);
        let group = layout(&pad(stick), metrics()).groups[0].clone();
        assert_eq!(group.marker, None);
        assert!(group.marks.is_empty());
    }

    /// The leader of a stick above its box goes straight down onto the box.
    /// A run along the box's top border would cover that border.
    #[test]
    fn a_stick_above_its_box_drops_onto_it_and_runs_along_nothing() {
        let layout = layout(&pad(stick(420)), metrics());
        let group = &layout.groups[0];
        assert!(group.strip.x < 420 && 420 < group.strip.x + group.strip.width);
        assert_eq!(group.leader, vec![vertical(420, 200, group.strip.y)]);
    }

    /// Whether `point` lies on `run`.
    fn on_run(point: (i32, i32), run: &Segment) -> bool {
        (run.x..=run.x + run.width).contains(&point.0) && (run.y..=run.y + run.height).contains(&point.1)
    }

    /// Whether `route` is one line from `from` to `to`: each run starts where
    /// the one before it ends.
    fn joins(route: &[Segment], from: (i32, i32), to: (i32, i32)) -> bool {
        let mut here = from;
        for run in route {
            let ends = [(run.x, run.y), (run.x + run.width, run.y + run.height)];
            let Some(next) = ends.iter().find(|end| **end != here && on_run(here, run)) else {
                return false;
            };
            here = *next;
        }
        here == to
    }

    /// How near `route` comes to `centre`.
    fn nearest(route: &[Segment], centre: (i32, i32)) -> f64 {
        route
            .iter()
            .map(|run| {
                let dx = centre.0.clamp(run.x, run.x + run.width) - centre.0;
                let dy = centre.1.clamp(run.y, run.y + run.height) - centre.1;
                f64::from(dx * dx + dy * dy).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    }

    fn button(id: &str, at: (i32, i32), callout: (i32, i32)) -> ControlDefinition {
        ControlDefinition {
            id: id.to_string(),
            label: id.to_string(),
            key: String::new(),
            group: None,
            direction: None,
            x: at.0,
            y: at.1,
            callout_x: callout.0,
            callout_y: callout.1,
        }
    }

    /// For a stick beside its box, the leader crosses above the box and goes
    /// down to its top edge, away from its corners, on the stick's side.
    #[test]
    fn a_stick_beside_its_box_drops_onto_its_top() {
        let metrics = metrics();
        for x in [100, 900] {
            let group = layout(&pad(stick(x)), metrics).groups[0].clone();
            let painted = painted(&group.strip, metrics.group_border);
            let last = group.leader.last().expect("a leader");
            let landing = (last.x, group.strip.y);
            assert_eq!(group.leader.len(), 2, "{x}: across, then down: {:?}", group.leader);
            assert_eq!(last.width, 0, "{x}: it lands going down");
            assert!(painted.x < landing.0 && landing.0 < painted.x + painted.width, "{x}: {landing:?}");
            assert!(joins(&group.leader, (x, 200), landing), "{x}: {:?}", group.leader);
        }
    }

    /// A leader whose L would pass through a stick's box goes around it, and
    /// one whose L is clear keeps it.
    #[test]
    fn a_leader_that_would_cross_a_box_goes_around_it() {
        let metrics = metrics();
        let mut controls = stick(420);
        controls.push(button("down", (323, 174), (16, 270)));
        controls.push(button("select", (430, 156), (16, 324)));
        let layout = layout(&pad(controls.clone()), metrics);
        let strip = painted(&layout.groups[0].strip, metrics.group_border);
        let (down, select) = (&layout.controls[0], &layout.controls[1]);
        assert_eq!(down.leader, leader(&controls[2], metrics), "down's L is clear");
        assert!(select.leader.iter().all(|run| !crosses(run, &strip)), "{:?}", select.leader);
        assert!(joins(&select.leader, (16 + 196 + 2 * 2, 350), (430, 156)), "{:?}", select.leader);
    }

    /// A leader whose L runs through the ring over another button turns
    /// around it, as GameCube's B does around the C-stick's ring.
    #[test]
    fn a_leader_turns_around_a_ring_in_its_way() {
        let metrics = metrics();
        let controls = vec![
            button("c", (557, 226), (744, 0)),
            button("b", (570, 146), (744, 315)),
        ];
        let b = layout(&pad(controls), metrics).controls[1].clone();
        assert_ne!(b.leader, leader(&button("b", (570, 146), (744, 315)), metrics));
        assert!(nearest(&b.leader, (557, 226)) >= 21.0, "{:?}", b.leader);
        assert!(joins(&b.leader, (744, 341), (570, 146)), "{:?}", b.leader);
    }

    /// A stick's leader keeps off the leaders and rings below it, such as
    /// the Dreamcast's D-pad Left line and ring.
    #[test]
    fn a_sticks_leader_keeps_off_the_leader_and_ring_below_it() {
        let metrics = metrics();
        let mut controls = stick(351);
        controls[1].y = 126;
        controls.push(button("left", (352, 207), (16, 126)));
        let layout = layout(&pad(controls), metrics);
        let (stick, left) = (&layout.groups[0], &layout.controls[0]);
        assert!(nearest(&stick.leader, (352, 207)) >= 21.0, "{:?}", stick.leader);
        for run in &stick.leader {
            for other in &left.leader {
                let side_by_side = (run.width == 0) == (other.width == 0);
                let apart = if run.width == 0 { (run.x - other.x).abs() } else { (run.y - other.y).abs() };
                let shared = if run.width == 0 {
                    run.y.max(other.y) < (run.y + run.height).min(other.y + other.height)
                } else {
                    run.x.max(other.x) < (run.x + run.width).min(other.x + other.width)
                };
                assert!(!(side_by_side && shared && apart < 4), "{run:?} lies along {other:?}");
            }
        }
    }
}
