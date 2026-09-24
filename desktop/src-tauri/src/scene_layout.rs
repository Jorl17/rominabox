//! Where everything on the controller scene goes.
//!
//! This is the only placement code for the scene. We draw it in the exporter,
//! and request it from the builder and the overlay renderer. To change how
//! we route a leader line, change this file and nowhere else.

use crate::controls::ControlDefinition;
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
    pub leader: Vec<Segment>,
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
    let midline = control.callout_y + metrics.callout_height / 2 + metrics.callout_border / 2;
    vec![
        horizontal(midline, edge, control.x),
        vertical(control.x, midline, control.y),
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

pub fn layout(controls: &[ControlDefinition], metrics: SceneMetrics) -> SceneLayout {
    let radius = metrics.marker / 2;
    let drawn: Vec<&ControlDefinition> =
        controls.iter().filter(|c| c.group.is_none()).collect();

    let placements = drawn
        .iter()
        .map(|control| Placement {
            id: control.id.clone(),
            marker: Rect {
                x: control.x - radius,
                y: control.y - radius,
                width: metrics.marker,
                height: metrics.marker,
            },
            callout: Rect {
                x: control.callout_x,
                y: control.callout_y,
                width: metrics.callout_width,
                height: metrics.callout_height,
            },
            leader: leader(control, metrics),
        })
        .collect();

    let mut names: Vec<&str> = controls
        .iter()
        .filter_map(|control| control.group.as_deref())
        .collect();
    names.sort_unstable();
    names.dedup();

    let count = names.len() as i32;
    let groups = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let total = count * metrics.group_width + (count - 1) * metrics.group_gap;
            let left = (metrics.scene_width - total) / 2
                + index as i32 * (metrics.group_width + metrics.group_gap);
            let top = metrics.scene_height - metrics.group_height - metrics.group_bottom_margin;
            let strip = Rect {
                x: left,
                y: top,
                width: metrics.group_width,
                height: metrics.group_height,
            };
            // One anchor for the whole stick, on the member that has a position
            // on the pad.
            let anchor = controls
                .iter()
                .find(|c| c.group.as_deref() == Some(*name) && (c.x != 0 || c.y != 0));
            let anchor_id = anchor.map(|anchor| anchor.id.clone());
            let (marker, leader) = match anchor {
                Some(anchor) => (
                    Some(Rect {
                        x: anchor.x - radius,
                        y: anchor.y - radius,
                        width: metrics.marker,
                        height: metrics.marker,
                    }),
                    stick_leader(anchor, strip, metrics),
                ),
                None => (None, Vec::new()),
            };
            GroupPlacement {
                name: (*name).to_string(),
                strip,
                anchor: anchor_id,
                marker,
                leader,
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
            x,
            y,
            callout_x: 0,
            callout_y: 0,
        };
        vec![member("l_x_plus", 0, 0), member("l3", x, 200)]
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
}
