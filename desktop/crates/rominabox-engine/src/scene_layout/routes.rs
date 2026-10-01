//! How we route a leader from its start to its end. We choose the route with
//! the fewest bends that passes through no other button's ring, across no box
//! and along no other leader, and of those the shortest. Each point of room
//! under [`ROOM`] that a route leaves next to anything counts as [`CROWDING`]
//! more length.

use super::{crosses, Rect, Segment};

/// A route this far from every ring, box and leader is as good as any;
/// nearer, it crowds them.
const ROOM: i32 = 8;
/// How much longer a route may be to keep one more point of room.
const CROWDING: i32 = 12;
/// Two runs side by side nearer than this read as one line.
const APART: i32 = 4;
/// The most a route turns.
const MOST_BENDS: usize = 4;

/// A point on the scene.
pub(super) type Point = (i32, i32);

/// Which way a run goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Axis {
    Across,
    Down,
}

impl Axis {
    fn turned(self) -> Self {
        match self {
            Axis::Across => Axis::Down,
            Axis::Down => Axis::Across,
        }
    }
}

/// Where a leader may end, and which way its last run must go to get there.
#[derive(Clone, Copy, Debug)]
pub(super) struct End {
    pub at: Point,
    pub last: Option<Axis>,
}

/// Everything already on the scene, by the control or stick it is for: the
/// rings, the painted boxes, and the leaders we placed so far.
pub(super) struct Scene {
    pub bounds: Rect,
    pub rings: Vec<(String, Rect)>,
    pub boxes: Vec<(String, Rect)>,
    pub runs: Vec<(String, Segment)>,
}

/// The run from `from` to `to`, which share a row or a column.
fn run(from: Point, to: Point) -> Segment {
    Segment {
        x: from.0.min(to.0),
        y: from.1.min(to.1),
        width: (from.0 - to.0).abs(),
        height: (from.1 - to.1).abs(),
    }
}

/// The runs joining `points` in turn, with no empty run and no two in a
/// row along one line.
pub(super) fn runs(points: &[Point]) -> Vec<Segment> {
    let mut kept: Vec<Point> = Vec::new();
    for point in points {
        if kept.last() == Some(point) {
            continue;
        }
        if let [.., before, last] = kept[..] {
            let straight = (before.0 == last.0 && last.0 == point.0)
                || (before.1 == last.1 && last.1 == point.1);
            if straight {
                kept.pop();
            }
        }
        kept.push(*point);
    }
    kept.windows(2).map(|pair| run(pair[0], pair[1])).collect()
}

/// How far a run passes from the ring filling `marker`: below zero, it
/// runs through the ring.
fn from_ring(run: &Segment, marker: &Rect) -> i32 {
    let radius = marker.width / 2;
    let (x, y) = (marker.x + radius, marker.y + radius);
    let dx = x.clamp(run.x, run.x + run.width) - x;
    let dy = y.clamp(run.y, run.y + run.height) - y;
    f64::from(dx * dx + dy * dy).sqrt().floor() as i32 - radius
}

/// How far a run passes from a box; zero when it touches or enters it.
fn from_box(run: &Segment, painted: &Rect) -> i32 {
    let gap = |low: i32, high: i32, start: i32, end: i32| (start - high).max(low - end).max(0);
    let across = gap(painted.x, painted.x + painted.width, run.x, run.x + run.width);
    let down = gap(painted.y, painted.y + painted.height, run.y, run.y + run.height);
    across.max(down)
}

/// How far apart two runs lie side by side, when they share a stretch.
fn beside(first: &Segment, second: &Segment) -> Option<i32> {
    let shared = |a0: i32, a1: i32, b0: i32, b1: i32| a0.max(b0) < a1.min(b1);
    match (first.height == 0, second.height == 0) {
        (true, true) if shared(first.x, first.x + first.width, second.x, second.x + second.width) => {
            Some((first.y - second.y).abs())
        }
        (false, false)
            if shared(first.y, first.y + first.height, second.y, second.y + second.height) =>
        {
            Some((first.x - second.x).abs())
        }
        _ => None,
    }
}

/// Whether a run stays on the scene.
fn on(run: &Segment, bounds: &Rect) -> bool {
    run.x >= bounds.x
        && run.y >= bounds.y
        && run.x + run.width <= bounds.x + bounds.width
        && run.y + run.height <= bounds.y + bounds.height
}

impl Scene {
    /// The room `run` keeps from everything not `owner`'s, up to [`ROOM`],
    /// or `None` when it runs through a ring, across a box, along another
    /// leader or off the scene.
    fn room(&self, owner: &str, run: &Segment) -> Option<i32> {
        if !on(run, &self.bounds) {
            return None;
        }
        let mut room = ROOM;
        for (id, marker) in &self.rings {
            if id != owner {
                let away = from_ring(run, marker);
                if away < 0 {
                    return None;
                }
                room = room.min(away);
            }
        }
        for (id, painted) in &self.boxes {
            if crosses(run, painted) {
                return None;
            }
            if id != owner {
                room = room.min(from_box(run, painted));
            }
        }
        for (id, other) in &self.runs {
            if id == owner {
                continue;
            }
            if let Some(apart) = beside(run, other) {
                if apart < APART {
                    return None;
                }
                room = room.min(apart);
            }
        }
        Some(room)
    }

    /// Where a run going across `axis` may turn within `within`: just clear
    /// of each ring, box and leader, and halfway between each neighbouring
    /// two, nearest `toward` first.
    pub(super) fn turns(&self, axis: Axis, toward: i32, within: (i32, i32)) -> Vec<i32> {
        let span = |rect: &Rect| match axis {
            Axis::Across => [rect.x, rect.x + rect.width],
            Axis::Down => [rect.y, rect.y + rect.height],
        };
        let mut walls: Vec<i32> = self
            .rings
            .iter()
            .chain(&self.boxes)
            .flat_map(|(_, rect)| span(rect))
            .chain(self.runs.iter().flat_map(|(_, other)| {
                span(&Rect { x: other.x, y: other.y, width: other.width, height: other.height })
            }))
            .collect();
        walls.sort_unstable();
        walls.dedup();
        let mut turns: Vec<i32> = walls
            .iter()
            .flat_map(|wall| [wall - ROOM, wall + ROOM])
            .chain(walls.windows(2).map(|pair| (pair[0] + pair[1]) / 2))
            .chain([toward])
            .filter(|at| within.0 <= *at && *at <= within.1)
            .collect();
        turns.sort_unstable_by_key(|at| ((at - toward).abs(), *at));
        turns.dedup();
        turns
    }

    /// The route for `owner` from `from` to one of `ends`, its first run going
    /// `first` when that is given; `direct` when nothing is clear.
    pub(super) fn route(
        &self,
        owner: &str,
        from: Point,
        first: Option<Axis>,
        ends: &[End],
        direct: Vec<Segment>,
    ) -> Vec<Segment> {
        let whole = |low: i32, size: i32| (low, low + size);
        for bends in 0..=MOST_BENDS {
            let mut search = Search {
                scene: self,
                owner,
                best: None,
            };
            for end in ends {
                let xs = self.turns(Axis::Across, end.at.0, whole(self.bounds.x, self.bounds.width));
                let ys = self.turns(Axis::Down, end.at.1, whole(self.bounds.y, self.bounds.height));
                for axis in [Axis::Across, Axis::Down] {
                    let last = if bends % 2 == 0 { axis } else { axis.turned() };
                    if first.is_some_and(|first| first != axis)
                        || end.last.is_some_and(|wanted| wanted != last)
                    {
                        continue;
                    }
                    let mut points = vec![from];
                    search.walk(&mut points, axis, bends, end, (&xs, &ys), (ROOM, 0));
                }
            }
            if let Some((_, points)) = search.best {
                return runs(&points);
            }
        }
        direct
    }
}

/// A search for one leader's route with a given number of bends: the best
/// found so far, and what it costs.
struct Search<'a> {
    scene: &'a Scene,
    owner: &'a str,
    best: Option<(i32, Vec<Point>)>,
}

/// What a route of this room and length costs: its length, and
/// [`CROWDING`] for every point of room it lacks.
fn cost(room: i32, length: i32) -> i32 {
    length + CROWDING * (ROOM - room)
}

impl Search<'_> {
    /// Whether a route with this room and length so far can still beat the
    /// best: going further only makes it longer and closer to things.
    fn promising(&self, room: i32, length: i32) -> bool {
        self.best
            .as_ref()
            .is_none_or(|(best, _)| cost(room, length) < *best)
    }

    /// Continue from the last of `points`, going `going`, with `bends` turns
    /// still to make, the last of them in line with `end`. `so_far` is the
    /// route's room and length up to here.
    fn walk(
        &mut self,
        points: &mut Vec<Point>,
        going: Axis,
        bends: usize,
        end: &End,
        turns: (&[i32], &[i32]),
        so_far: (i32, i32),
    ) {
        let here = *points.last().expect("a route starts somewhere");
        if bends <= 1 {
            // The rest follows from the end: straight to it, or in line with
            // it and then to it.
            let mut rest = Vec::new();
            if bends == 1 {
                rest.push(match going {
                    Axis::Across => (end.at.0, here.1),
                    Axis::Down => (here.0, end.at.1),
                });
            } else if (going == Axis::Across && here.1 != end.at.1)
                || (going == Axis::Down && here.0 != end.at.0)
            {
                return;
            }
            rest.push(end.at);
            let (mut room, mut length) = so_far;
            let mut from = here;
            for to in &rest {
                if *to == from {
                    // A route with fewer bends, from a search with fewer bends.
                    return;
                }
                let next = run(from, *to);
                let Some(clear) = self.scene.room(self.owner, &next) else {
                    return;
                };
                room = room.min(clear);
                length += next.width + next.height;
                from = *to;
            }
            if self.promising(room, length) {
                let mut route = points.clone();
                route.extend(rest);
                self.best = Some((cost(room, length), route));
            }
            return;
        }
        let choices = match going {
            Axis::Across => turns.0,
            Axis::Down => turns.1,
        };
        for at in choices {
            let next = match going {
                Axis::Across => (*at, here.1),
                Axis::Down => (here.0, *at),
            };
            if next == here {
                continue;
            }
            let stretch = run(here, next);
            let Some(clear) = self.scene.room(self.owner, &stretch) else {
                continue;
            };
            let so_far = (so_far.0.min(clear), so_far.1 + stretch.width + stretch.height);
            if !self.promising(so_far.0, so_far.1) {
                continue;
            }
            points.push(next);
            self.walk(points, going.turned(), bends - 1, end, turns, so_far);
            points.pop();
        }
    }
}
