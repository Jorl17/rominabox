//! Where everything on the controller scene goes.
//!
//! This is the only placement code for the scene. We draw it in the exporter,
//! and request it from the builder and the overlay renderer. To change how
//! we route a leader line, change this file and nowhere else.

use crate::controls::ControlDefinition;
use crate::themes::SceneMetrics;
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
            let (marker, leader) = match anchor {
                Some(anchor) => (
                    Some(Rect {
                        x: anchor.x - radius,
                        y: anchor.y - radius,
                        width: metrics.marker,
                        height: metrics.marker,
                    }),
                    vec![
                        vertical(anchor.x, anchor.y, top),
                        horizontal(top, anchor.x, left + metrics.group_width / 2),
                    ],
                ),
                None => (None, Vec::new()),
            };
            GroupPlacement {
                name: (*name).to_string(),
                strip,
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
