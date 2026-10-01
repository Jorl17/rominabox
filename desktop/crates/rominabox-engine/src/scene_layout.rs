//! Where everything on the controller scene goes.
//!
//! This is the only placement code for the scene. We draw it in the exporter,
//! and request it from the builder and the overlay renderer. To change how
//! we route a leader line, change this file and nowhere else.

use crate::controls::{ControlDefinition, StickDirection};
use crate::menu::SceneMetrics;
use serde::Serialize;

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

fn horizontal(at_y: i32, from_x: i32, to_x: i32) -> Segment {
    Segment {
        x: from_x.min(to_x),
        y: at_y,
        width: (from_x - to_x).abs(),
        height: 0,
    }
}

fn vertical(at_x: i32, from_y: i32, to_y: i32) -> Segment {
    Segment {
        x: at_x,
        y: from_y.min(to_y),
        width: 0,
        height: (from_y - to_y).abs(),
    }
}

/// The route from a callout to its button.
///
/// From the callout's inner edge along the callout's own midline, then up or
/// down the button's column, in two segments that form an L.
///
/// We use this route in every export. We define it only here, so a change to
/// it is one edit.
fn leader(control: &ControlDefinition, metrics: SceneMetrics) -> Vec<Segment> {
    let edge = inner_edge(control.callout_x, metrics);
    let midline = midline(control, metrics);
    vec![
        horizontal(midline, edge, control.x),
        vertical(control.x, midline, control.y),
    ]
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

/// The route from a callout to its button when the L would cross a stick's
/// box: out along the midline to halfway between the callout and the box, up
/// or down beside the box to clear it, across to the button's column, and on
/// to the button.
///
/// It passes the box halfway between the box and the nearest thing on that
/// side of it the run would otherwise crowd: another leader's run across the
/// same span, or the button itself.
fn around(
    control: &ControlDefinition,
    obstacle: &Rect,
    others: &[Vec<Segment>],
    metrics: SceneMetrics,
) -> Vec<Segment> {
    let edge = inner_edge(control.callout_x, metrics);
    let near = if edge < obstacle.x { obstacle.x } else { obstacle.x + obstacle.width };
    let beside = (edge + near) / 2;
    let span = (beside.min(control.x), beside.max(control.x));
    let across = |run: &&Segment| {
        run.height == 0 && run.x < span.1 && span.0 < run.x + run.width
    };
    let above = control.y < obstacle.y;
    let clear = if above {
        let nearest = others
            .iter()
            .flatten()
            .filter(across)
            .map(|run| run.y)
            .filter(|y| *y < obstacle.y)
            .fold(control.y, i32::max);
        (nearest + obstacle.y) / 2
    } else {
        let bottom = obstacle.y + obstacle.height;
        let nearest = others
            .iter()
            .flatten()
            .filter(across)
            .map(|run| run.y)
            .filter(|y| *y > bottom)
            .fold(control.y, i32::min);
        (nearest + bottom) / 2
    };
    let midline = midline(control, metrics);
    vec![
        horizontal(midline, edge, beside),
        vertical(beside, midline, clear),
        horizontal(clear, beside, control.x),
        vertical(control.x, clear, control.y),
    ]
}

/// The route from a stick to its box: down the stick's column to the top of
/// the box, then along that edge to the box when the stick is beside it
/// instead of above it. It stops where the box's painted edge begins. We draw
/// the leader over the box, so a run along the box's border would cover that
/// border.
fn stick_leader(anchor: &ControlDefinition, strip: Rect, metrics: SceneMetrics) -> Vec<Segment> {
    let mut route = vec![vertical(anchor.x, anchor.y, strip.y)];
    let painted = strip.width + 2 * metrics.group_border;
    let edge = anchor.x.clamp(strip.x, strip.x + painted);
    if edge != anchor.x {
        route.push(horizontal(strip.y, anchor.x, edge));
    }
    route
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

pub fn layout(controls: &[ControlDefinition], metrics: SceneMetrics) -> SceneLayout {
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

    // Every leader follows the L unless the L would cross a stick's box. Then
    // it goes around the box, clear of the Ls beside it.
    let direct: Vec<Vec<Segment>> = drawn.iter().map(|c| leader(c, metrics)).collect();
    let placements = drawn
        .iter()
        .zip(&direct)
        .map(|(control, route)| {
            let obstacle = boxes
                .iter()
                .find(|obstacle| route.iter().any(|run| crosses(run, obstacle)));
            Placement {
                id: control.id.clone(),
                marker: marker(control.x, control.y, metrics),
                reach: reaches.next().expect("a reach for every drawn control"),
                callout: Rect {
                    x: control.callout_x,
                    y: control.callout_y,
                    width: metrics.callout_width,
                    height: metrics.callout_height,
                },
                leader: match obstacle {
                    Some(obstacle) => around(control, obstacle, &direct, metrics),
                    None => route.clone(),
                },
            }
        })
        .collect();

    let groups = names
        .iter()
        .zip(strips)
        .zip(anchors)
        .map(|((name, strip), anchor)| {
            let reach = anchor.map(|_| reaches.next().expect("a reach for every stick's anchor"));
            let ring = anchor.map(|anchor| marker(anchor.x, anchor.y, metrics));
            let members = controls
                .iter()
                .filter(|control| control.group.as_deref() == Some(*name));
            GroupPlacement {
                name: (*name).to_string(),
                strip,
                anchor: anchor.map(|anchor| anchor.id.clone()),
                marker: ring,
                reach,
                leader: anchor
                    .map(|anchor| stick_leader(anchor, strip, metrics))
                    .unwrap_or_default(),
                marks: ring.map(|ring| marks(members, ring)).unwrap_or_default(),
            }
        })
        .collect();

    SceneLayout {
        scene: Rect {
            x: 0,
            y: 0,
            width: metrics.scene_width,
            height: metrics.scene_height,
        },
        controls: placements,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let group = layout(&stick, metrics()).groups[0].clone();
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
        let group = layout(&stick, metrics()).groups[0].clone();
        assert_eq!((group.strip.x, group.strip.y), (230, 240));
        let default = layout(&members(Some((420, 200)), &[("up", Up), ("right", Right)]), metrics())
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
        let marks = layout(&stick, metrics()).groups[0].marks.clone();
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
        let group = layout(&stick, metrics()).groups[0].clone();
        assert_eq!(group.marker, None);
        assert!(group.marks.is_empty());
    }

    /// The leader of a stick above its box goes straight down onto the box.
    /// A run along the box's top border would cover that border.
    #[test]
    fn a_stick_above_its_box_drops_onto_it_and_runs_along_nothing() {
        let layout = layout(&stick(420), metrics());
        let group = &layout.groups[0];
        assert!(group.strip.x < 420 && 420 < group.strip.x + group.strip.width);
        assert_eq!(group.leader, vec![vertical(420, 200, group.strip.y)]);
    }

    /// For a stick beside its box, the leader turns along the box's top edge
    /// and stops where the painted edge begins, on whichever side it is.
    #[test]
    fn a_stick_beside_its_box_turns_to_the_near_painted_edge() {
        let metrics = metrics();
        let left = layout(&stick(100), metrics).groups[0].clone();
        assert_eq!(
            left.leader,
            vec![
                vertical(100, 200, left.strip.y),
                horizontal(left.strip.y, 100, left.strip.x),
            ]
        );
        let right = layout(&stick(900), metrics).groups[0].clone();
        let painted_right = right.strip.x + metrics.group_width + 2 * metrics.group_border;
        assert_eq!(
            right.leader,
            vec![
                vertical(900, 200, right.strip.y),
                horizontal(right.strip.y, 900, painted_right),
            ]
        );
    }

    /// A leader whose L would pass through a stick's box goes around it: it
    /// leaves the callout the same way, turns up beside the box, crosses
    /// halfway between the box and the leader above it, and meets the button
    /// the same way.
    #[test]
    fn a_leader_that_would_cross_a_box_goes_around_it() {
        let metrics = metrics();
        let callout = |id: &str, x: i32, y: i32, callout_y: i32| ControlDefinition {
            id: id.to_string(),
            label: id.to_string(),
            key: String::new(),
            group: None,
            direction: None,
            x,
            y,
            callout_x: 16,
            callout_y,
        };
        let mut controls = stick(420);
        controls.push(callout("down", 323, 174, 270));
        controls.push(callout("select", 430, 156, 324));
        let layout = layout(&controls, metrics);
        let strip = layout.groups[0].strip;
        let down = &layout.controls[0];
        let select = &layout.controls[1];
        assert_eq!(down.leader, leader(&controls[2], metrics), "down's L is clear");
        let edge = 16 + 196 + 2 * 2;
        let beside = (edge + strip.x) / 2;
        let clear = (down.leader[0].y + strip.y) / 2;
        assert_eq!(
            select.leader,
            vec![
                horizontal(350, edge, beside),
                vertical(beside, 350, clear),
                horizontal(clear, beside, 430),
                vertical(430, clear, 156),
            ]
        );
    }
}
