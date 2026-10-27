use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cadcraft_doc::{ArchId, Wall};
use cadcraft_geom::{Bounds2, Vec2};

use crate::graph::WallGraph;

pub use cadcraft_doc::MAX_WALLS;
/// Max wall ends allowed to share one join point before we stop mitring.
pub const MAX_JOIN_FAN: usize = 8;
/// Endpoint clustering tolerance (world units).
pub const ENDPOINT_TOL: f64 = 1e-6;
/// Generic geometric epsilon.
pub const JOIN_EPS: f64 = 1e-9;
/// Parallelism test for "corresponding" (collinear) neighbour detection.
const PARALLEL_EPS: f64 = 1e-9;
/// Mitre limit, in multiples of the thicker wall's half thickness (SVG's default
/// miter-limit rule). A corner whose two walls meet at 29 deg or wider still mitres;
/// anything sharper squares off instead of spiking far down the bisector.
pub const MITRE_LIMIT: f64 = 4.0;
/// How far (in world units, scaled by combined wall thickness) a T join's
/// face-line intersection may be from the join point before we fall back to a
/// square end.
const T_JOIN_REACH: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum End {
    Start,
    End,
}

impl End {
    /// The opposite end.
    pub fn other(self) -> End {
        match self {
            End::Start => End::End,
            End::End => End::Start,
        }
    }

    /// The point on `w` corresponding to this end.
    pub fn point(self, w: &Wall) -> Vec2 {
        match self {
            End::Start => w.start,
            End::End => w.end,
        }
    }
}

use crate::wall::{wall_dir, wall_polygon};
use cadcraft_geom::line_line_infinite;

pub use cadcraft_doc::JoinKind;

/// Per-wall join overrides, keyed by `ArchId` and `(start_kind, end_kind)`.
///
/// `per_wall` maps a wall id to `(start_kind, end_kind)`. `kind(id, end)` returns the
/// override for that end, or `JoinKind::Auto` when the wall (or that end) is absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinOverrides {
    pub per_wall: std::collections::BTreeMap<cadcraft_doc::ArchId, (JoinKind, JoinKind)>,
}

impl JoinOverrides {
    /// No overrides (everything is `JoinKind::Auto`).
    pub fn new() -> Self {
        Self { per_wall: BTreeMap::new() }
    }

    /// Set the override kinds for `id`'s start and end.
    pub fn set(&mut self, id: cadcraft_doc::ArchId, start: JoinKind, end: JoinKind) {
        self.per_wall.insert(id, (start, end));
    }

    /// The override for `id`'s `end`, or `JoinKind::Auto` when absent.
    pub fn kind(&self, id: cadcraft_doc::ArchId, end: End) -> JoinKind {
        match end {
            End::Start => self.per_wall.get(&id).map_or(JoinKind::Auto, |&(s, _)| s),
            End::End => self.per_wall.get(&id).map_or(JoinKind::Auto, |&(_, e)| e),
        }
    }
}

/// The join applied at one end of a wall shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Join {
    L(End),
    T(End),
    X(End),
    Butt(End),
    Square(End),
}

impl Join {
    /// The `End` carried by this join variant.
    pub fn end(self) -> End {
        match self {
            Join::L(e) | Join::T(e) | Join::X(e) | Join::Butt(e) | Join::Square(e) => e,
        }
    }
}

/// A wall's solved 2D poché ring plus the joins that produced it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WallShape {
    pub id: ArchId,
    /// CCW, no repeated first point.
    pub polygon: Vec<Vec2>,
    /// Centre-line endpoints (start, end).
    pub centre: (Vec2, Vec2),
    /// At most one `Join` per `End`.
    pub joins: Vec<Join>,
}

impl WallShape {
    /// A shape with no joins yet. `id`, `polygon` and `centre` are the required fields; build it
    /// with this rather than an exhaustive literal so a new field cannot break other crates.
    pub fn new(id: ArchId, polygon: Vec<Vec2>, centre: (Vec2, Vec2)) -> Self {
        WallShape { id, polygon, centre, joins: Vec::new() }
    }

    /// Bounds of the polygon; `Bounds2::EMPTY` for an empty polygon.
    pub fn bounds(&self) -> Bounds2 {
        if self.polygon.is_empty() {
            return Bounds2::EMPTY;
        }
        Bounds2::from_points(self.polygon.iter().cloned())
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ArchError {
    #[error("no wall with id {0:?}")]
    NoSuchWall(ArchId),
    #[error("point is not finite")]
    NotFinite,
    #[error("too many walls (max {MAX_WALLS})")]
    TooManyWalls,
}

/// The two long face lines of a wall's poché ring, as `[left, right]` where
/// "left" is `+thickness/2` along `wall_dir().perp()`. `None` when the wall is
/// degenerate (reuse `wall::wall_dir` and `wall::offset_line`).
pub fn face_lines(w: &Wall) -> Option<[(Vec2, Vec2); 2]> {
    let half = w.thickness / 2.0;
    let left = crate::wall::offset_line(w.start, w.end, half)?;
    let right = crate::wall::offset_line(w.start, w.end, -half)?;
    Some([(left.0, left.1), (right.0, right.1)])
}

/// Assemble a CCW 4-corner ring from the two corner points at each end.
/// `left_start`/`left_end` lie on `face_lines()[0]`, `right_*` on `face_lines()[1]`.
/// Returns `[right_start, right_end, left_end, left_start]` (that order is CCW, and
/// equals `wall::wall_polygon` when the corners are untrimmed). `None` if any point
/// is non-finite or the ring is not simple (signed area <= JOIN_EPS, or the two
/// ends have crossed over: `(right_end - right_start).dot(left_end - left_start) <= 0`).
pub fn ring_from_corners(left_start: Vec2, right_start: Vec2, left_end: Vec2, right_end: Vec2) -> Option<Vec<Vec2>> {
    if !left_start.is_finite() || !right_start.is_finite() || !left_end.is_finite() || !right_end.is_finite() {
        return None;
    }

    let right_dir = right_end - right_start;
    let left_dir = left_end - left_start;
    if right_dir.dot(left_dir) <= 0.0 {
        return None;
    }

    let ring = vec![right_start, right_end, left_end, left_start];
    let area = polygon_area(&ring);
    if area <= JOIN_EPS {
        return None;
    }

    Some(ring)
}

/// Signed polygon area (CCW positive). 0.0 for fewer than 3 points.
pub fn polygon_area(pts: &[Vec2]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        area += pts[i].x * pts[j].y - pts[j].x * pts[i].y;
    }
    area / 2.0
}

/// Union of every shape's bounds; `Bounds2::EMPTY` for an empty slice.
pub fn solve_bounds(shapes: &[WallShape]) -> Bounds2 {
    if shapes.is_empty() {
        return Bounds2::EMPTY;
    }
    let mut all_points: Vec<Vec2> = Vec::new();
    for shape in shapes {
        all_points.extend(shape.polygon.iter().cloned());
    }
    Bounds2::from_points(all_points)
}

/// The planned corners and join at one end of a wall after solving.
struct EndPlan {
    left: Vec2,
    right: Vec2,
    join: Option<Join>,
}

/// End each of the incoming wall's two face lines on the host face line the
/// incoming wall approaches, producing a `Join::T` end.
///
/// The host face is chosen by which side of the host the incoming wall comes
/// from (a point along the incoming wall away from the join, projected onto the
/// host's normal). Each face line is intersected with that host face line; the
/// pair is accepted only when both points are finite and within
/// `T_JOIN_REACH * (w.thickness + host.thickness)` of the join point, otherwise
/// we fall back to a square end on the face. `Join::T(end)` is recorded either
/// way.
fn t_join(w: &Wall, end: End, raw: (Vec2, Vec2), host: &Wall) -> EndPlan {
    let pt = end.point(w);
    let Some(wdir) = wall_dir(w) else {
        return EndPlan { left: raw.0, right: raw.1, join: Some(Join::T(end)) };
    };
    let Some(hdir) = wall_dir(host) else {
        return EndPlan { left: raw.0, right: raw.1, join: Some(Join::T(end)) };
    };
    let hperp = Vec2::new(-hdir.y, hdir.x);
    let hhalf = host.thickness / 2.0;
    let whalf = w.thickness / 2.0;

    // Which side of the host the incoming wall approaches: a point along the
    // incoming wall away from the join, dotted with the host's normal.
    let away = if end == End::End { -wdir } else { wdir };
    let away_pt = pt + away * 1.0;
    let side = (away_pt - host.start).dot(hperp);
    let sign = if side > 0.0 { 1.0 } else { -1.0 };
    let face_a = host.start + hperp * (sign * hhalf);
    let face_b = host.end + hperp * (sign * hhalf);

    // End each incoming face line on the host face line.
    let left = line_line_infinite(raw.0, raw.0 + wdir * 1000.0, face_a, face_b).map(|(p, _, _)| p);
    let right = line_line_infinite(raw.1, raw.1 + wdir * 1000.0, face_a, face_b).map(|(p, _, _)| p);

    let reach = T_JOIN_REACH * (w.thickness + host.thickness);
    if let (Some(l), Some(r)) = (left, right) {
        if l.is_finite() && r.is_finite() && l.dist(pt) < reach && r.dist(pt) < reach {
            return EndPlan { left: l, right: r, join: Some(Join::T(end)) };
        }
    }

    // Fall back to today's square end on the face.
    let face_pt = if side > 0.0 {
        host.start + hdir * ((pt - host.start).dot(hdir)) + hperp * hhalf
    } else {
        host.start + hdir * ((pt - host.start).dot(hdir)) - hperp * hhalf
    };
    let wperp = Vec2::new(-wdir.y, wdir.x);
    let left = face_pt + wperp * whalf;
    let right = face_pt - wperp * whalf;
    EndPlan { left, right, join: Some(Join::T(end)) }
}

/// A neighbour wall relevant to a join at one end.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Neighbour {
    /// Another wall's end meets this wall's end at the same point.
    EndToEnd { id: ArchId, end: End },
    /// Another wall's centre line passes through this wall's end (T-junction).
    Mid { id: ArchId, end: End },
    /// A collinear wall that this wall should butt against.
    Collinear { id: ArchId, end: End },
    /// More than MAX_JOIN_FAN walls meet here; treat as no join.
    Fan { count: usize },
    /// A parallel (not collinear) neighbour whose end is aligned with this end;
    /// fall back to a square end.
    Parallel { id: ArchId, end: End },
    /// A crossing: two centre lines intersect at an interior point of both.
    X,
}

/// Classify the neighbours at one end of a wall.
///
/// The Mid, X and Parallel scans are prefilted: only walls whose centre-line
/// bounding box overlaps this wall's centre-line bounding box grown by
/// `reach = w.thickness.max(0) + nw.thickness.max(0) + ENDPOINT_TOL` are
/// considered. A Mid neighbour's end point lies on its centre line, an X
/// neighbour's crossing lies inside both centre-line boxes, and a Parallel
/// neighbour's end is within the thickness sum of this end, so no true
/// neighbour can be lost.
fn classify(model: &cadcraft_doc::ArchModel, g: &WallGraph, id: ArchId, end: End) -> Vec<Neighbour> {
    let Some(w) = model.walls.iter().find(|w| w.id == id) else {
        return Vec::new();
    };
    let pt = end.point(w);
    let dir = wall_dir(w);

    let mut neighbours = Vec::new();

    // Check cluster for EndToEnd, Collinear, and Fan
    if let Some(cluster_idx) = g.cluster_of(id, end) {
        let cluster = g.cluster(cluster_idx);
        if cluster.len() > MAX_JOIN_FAN {
            return vec![Neighbour::Fan { count: cluster.len() }];
        }
        for (nid, nend) in cluster {
            if *nid == id {
                continue;
            }
            let Some(nw) = model.walls.iter().find(|w| w.id == *nid) else {
                continue;
            };
            let Some(ndir) = wall_dir(nw) else {
                continue;
            };
            if let Some(d) = dir {
                let cross = d.cross(ndir);
                // Collinear: parallel and on the same line.
                if cross.abs() < PARALLEL_EPS {
                    let dist = (pt - nw.start).cross(ndir);
                    if dist.abs() < ENDPOINT_TOL {
                        let nend_at_pt = if nw.start.dist(pt) < ENDPOINT_TOL { End::Start } else { End::End };
                        neighbours.push(Neighbour::Collinear { id: *nid, end: nend_at_pt });
                        continue;
                    }
                }
            }
            // EndToEnd: the neighbour's end is also at this point.
            if nend.point(nw).dist(pt) < ENDPOINT_TOL {
                neighbours.push(Neighbour::EndToEnd { id: *nid, end: *nend });
            }
        }
    }

    // Prefilter for the Mid, X and Parallel scans: only walls whose centre-line
    // bounding box overlaps this wall's centre-line bounding box grown by
    // `reach = w.thickness.max(0) + nw.thickness.max(0) + ENDPOINT_TOL` can be
    // a neighbour. A Mid neighbour's end point lies on its centre line, an X
    // neighbour's crossing lies inside both centre-line boxes, and a Parallel
    // neighbour's end is within the thickness sum of this end, so no true
    // neighbour is lost.
    let candidates: Vec<&Wall> = model
        .walls
        .iter()
        .filter(|nw| nw.id != id)
        .filter(|nw| centreline_box_overlaps(w, nw, w.thickness.max(0.0) + nw.thickness.max(0.0) + ENDPOINT_TOL))
        .collect();

    // Check candidates for Mid (centre line passes through this point)
    for nw in &candidates {
        // Skip if already found as EndToEnd or Collinear in the cluster
        if neighbours.iter().any(|n| matches!(n, Neighbour::EndToEnd { id: nid, .. } | Neighbour::Collinear { id: nid, .. } if *nid == nw.id)) {
            continue;
        }
        let Some(ndir) = wall_dir(nw) else {
            continue;
        };
        let d = (pt - nw.start).cross(ndir);
        let t = (pt - nw.start).dot(ndir);
        let nw_len = nw.start.dist(nw.end);
        if d.abs() < ENDPOINT_TOL && t > ENDPOINT_TOL && t < nw_len - ENDPOINT_TOL {
            let nend = if nw.start.dist(pt) < nw.end.dist(pt) { End::Start } else { End::End };
            neighbours.push(Neighbour::Mid { id: nw.id, end: nend });
        }
    }

    // X and Parallel only make sense if this wall has a direction (non-zero length).
    if let Some(dir) = dir {
        // X: this wall's centre line is crossed by another wall's centre line at a point
        // interior to both, and this end is the nearest to the crossing.
        for nw in &candidates {
            if neighbours.iter().any(|n| matches!(n, Neighbour::EndToEnd { id: nid, .. } | Neighbour::Collinear { id: nid, .. } | Neighbour::Mid { id: nid, .. } | Neighbour::Parallel { id: nid, .. } if *nid == nw.id)) {
                continue;
            }
            let Some(ndir) = wall_dir(nw) else {
                continue;
            };
            let Some(cross_pt) = line_line_infinite(w.start, w.end, nw.start, nw.end) else {
                continue;
            };
            let cross_pt = cross_pt.0;
            // Crossing must be interior (strictly between endpoints) on both centre lines.
            let t1 = (cross_pt - w.start).dot(dir);
            let t2 = (cross_pt - nw.start).dot(ndir);
            let w_len = w.start.dist(w.end);
            let nw_len = nw.start.dist(nw.end);
            if t1 <= ENDPOINT_TOL || t1 >= w_len - ENDPOINT_TOL {
                continue;
            }
            if t2 <= ENDPOINT_TOL || t2 >= nw_len - ENDPOINT_TOL {
                continue;
            }
            // This end is the nearest to the crossing (End wins the tie when equidistant).
            let this_is_nearest =
                if end == End::Start { pt.dist(cross_pt) < w.end.dist(cross_pt) } else { pt.dist(cross_pt) <= w.start.dist(cross_pt) };
            if this_is_nearest {
                neighbours.push(Neighbour::X);
            }
        }

        // Parallel: parallel (not collinear) neighbour whose end is aligned with this end
        // along the wall direction and close in the perpendicular direction.
        for nw in &candidates {
            if neighbours.iter().any(|n| matches!(n, Neighbour::EndToEnd { id: nid, .. } | Neighbour::Collinear { id: nid, .. } | Neighbour::Mid { id: nid, .. } | Neighbour::Parallel { id: nid, .. } if *nid == nw.id)) {
                continue;
            }
            let Some(ndir) = wall_dir(nw) else {
                continue;
            };
            let cross = dir.cross(ndir);
            if cross.abs() < PARALLEL_EPS {
                let d = (pt - nw.start).cross(ndir);
                if d.abs() < ENDPOINT_TOL {
                    continue; // collinear, handled in the cluster
                }
                // parallel but not collinear
                let nend_pt = if nw.start.dist(pt) < nw.end.dist(pt) { nw.start } else { nw.end };
                let nend = if nw.start.dist(pt) < nw.end.dist(pt) { End::Start } else { End::End };
                let aligned = (nend_pt - pt).dot(dir).abs() < ENDPOINT_TOL;
                let close = pt.dist(nend_pt) < w.thickness + nw.thickness + ENDPOINT_TOL;
                if aligned && close {
                    neighbours.push(Neighbour::Parallel { id: nw.id, end: nend });
                }
            }
        }
    }

    neighbours
}

/// Does `nw`'s centre-line bounding box overlap `w`'s centre-line bounding box
/// grown by `reach` (world units)? Non-finite corners compare false, so such
/// walls are never candidates.
fn centreline_box_overlaps(w: &Wall, nw: &Wall, reach: f64) -> bool {
    let wx0 = w.start.x.min(w.end.x) - reach;
    let wx1 = w.start.x.max(w.end.x) + reach;
    let wy0 = w.start.y.min(w.end.y) - reach;
    let wy1 = w.start.y.max(w.end.y) + reach;
    let nx0 = nw.start.x.min(nw.end.x);
    let nx1 = nw.start.x.max(nw.end.x);
    let ny0 = nw.start.y.min(nw.end.y);
    let ny1 = nw.start.y.max(nw.end.y);
    nx0 <= wx1 && nx1 >= wx0 && ny0 <= wy1 && ny1 >= wy0
}

/// Plan the corners and join at one end of a wall.
///
/// `classify` is injected so the test-only full-scan variant can be run
/// against the same planning code.
fn plan_end(
    w: &Wall,
    end: End,
    raw: (Vec2, Vec2),
    g: &WallGraph,
    model: &cadcraft_doc::ArchModel,
    id: ArchId,
    ov: &JoinOverrides,
    classify: &dyn Fn(&cadcraft_doc::ArchModel, &WallGraph, ArchId, End) -> Vec<Neighbour>,
) -> Result<EndPlan, ArchError> {
    let nbrs = classify(model, g, id, end);
    let kind = match ov.kind(id, end) {
        JoinKind::Auto => match end {
            End::Start => w.join_start,
            End::End => w.join_end,
        },
        k => k,
    };

    // Overrides win over L/T/X.
    match kind {
        JoinKind::Butt => {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: Some(Join::Butt(end)) });
        }
        JoinKind::Square => {
            let pt = end.point(w);
            let Some(wdir) = wall_dir(w) else {
                return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
            };
            let wperp = Vec2::new(-wdir.y, wdir.x);
            let whalf = w.thickness / 2.0;
            let left = pt + wperp * whalf;
            let right = pt - wperp * whalf;
            return Ok(EndPlan { left, right, join: Some(Join::Square(end)) });
        }
        JoinKind::Auto | JoinKind::Mitre => {}
    }

    if nbrs.is_empty() {
        return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
    }

    // Fan: more than MAX_JOIN_FAN ends share this point; butt every end at the
    // cluster point (square end through the point, perpendicular to the wall's
    // own direction). Never mitre, never loop.
    if nbrs.iter().any(|n| matches!(n, Neighbour::Fan { .. })) {
        let pt = end.point(w);
        let Some(wdir) = wall_dir(w) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let wperp = Vec2::new(-wdir.y, wdir.x);
        let whalf = w.thickness / 2.0;
        let left = pt + wperp * whalf;
        let right = pt - wperp * whalf;
        return Ok(EndPlan { left, right, join: Some(Join::Butt(end)) });
    }

    // Parallel neighbour: square end at the shared join point.
    if let Some(Neighbour::Parallel { .. }) = nbrs.iter().find(|n| matches!(n, Neighbour::Parallel { .. })) {
        let pt = end.point(w);
        let Some(wdir) = wall_dir(w) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let wperp = Vec2::new(-wdir.y, wdir.x);
        let whalf = w.thickness / 2.0;
        let left = pt + wperp * whalf;
        let right = pt - wperp * whalf;
        return Ok(EndPlan { left, right, join: Some(Join::Square(end)) });
    }

    // Collinear: butt. Only trim this wall if it's the one that should be trimmed.
    if let Some(Neighbour::Collinear { id: nid, end: nend }) = nbrs.iter().find(|n| matches!(n, Neighbour::Collinear { .. })) {
        let Some(nw) = model.walls.iter().find(|w| w.id == *nid) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        // This wall is trimmed if its end is Start and the neighbour's end is End.
        // If both are the same end type, trim the one with the larger ArchId.
        let this_is_trimmed = if end == End::Start && *nend == End::End {
            true
        } else if end == End::End && *nend == End::Start {
            false
        } else {
            w.id > *nid
        };
        if !this_is_trimmed {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        }
        let other_pt = nend.point(nw);
        let Some(ndir) = wall_dir(nw) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let perp = Vec2::new(-ndir.y, ndir.x);
        let half = w.thickness / 2.0;
        let left = other_pt + perp * half;
        let right = other_pt - perp * half;
        return Ok(EndPlan { left, right, join: Some(Join::Butt(end)) });
    }

    // Mid (T-junction): trim to the host's face.
    if let Some(Neighbour::Mid { id: nid, .. }) = nbrs.iter().find(|n| matches!(n, Neighbour::Mid { .. })) {
        let Some(nw) = model.walls.iter().find(|w| w.id == *nid) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        return Ok(t_join(w, end, raw, nw));
    }

    // Tee: two collinear EndToEnd neighbours are one straight host; butt it
    // with a T join against one of them.
    {
        let e2e: Vec<ArchId> = nbrs
            .iter()
            .filter(|n| matches!(n, Neighbour::EndToEnd { .. }))
            .filter_map(|n| match n {
                Neighbour::EndToEnd { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        for i in 0..e2e.len() {
            for j in i + 1..e2e.len() {
                let Some(wa) = model.walls.iter().find(|w| w.id == e2e[i]) else {
                    continue;
                };
                let Some(wb) = model.walls.iter().find(|w| w.id == e2e[j]) else {
                    continue;
                };
                let Some(da) = wall_dir(wa) else {
                    continue;
                };
                let Some(db) = wall_dir(wb) else {
                    continue;
                };
                if da.cross(db).abs() < PARALLEL_EPS {
                    // Pick the thicker host: its face is further from the centre line,
                    // so the stem meets that face.
                    let host = if wa.thickness >= wb.thickness { wa } else { wb };
                    return Ok(t_join(w, end, raw, host));
                }
            }
        }
    }

    // EndToEnd (L or angled): mitre.
    if let Some(Neighbour::EndToEnd { id: nid, .. }) = nbrs.iter().find(|n| matches!(n, Neighbour::EndToEnd { .. })) {
        let Some(nw) = model.walls.iter().find(|w| w.id == *nid) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let Some(nface) = face_lines(nw) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let pt = end.point(w);
        let Some(wdir) = wall_dir(w) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let Some(ndir) = wall_dir(nw) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };

        // Interior bisector direction: between -wdir (back along this wall) and ndir (along neighbour)
        let bisector = -wdir + ndir;
        if bisector.len() < JOIN_EPS {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        }

        // For each face of this wall, intersect with both faces of the neighbour
        // and pick the intersection that lies on (or closest to) the bisector line.
        let pick_on_bisector = |p: Vec2, q: Vec2| -> Vec2 {
            let d1 = (p - pt).cross(bisector).abs();
            let d2 = (q - pt).cross(bisector).abs();
            if d1 <= d2 { p } else { q }
        };

        let left_a = line_line_infinite(raw.0, raw.0 + wdir * 1000.0, nface[0].0, nface[0].1).map(|(p, _, _)| p);
        let left_b = line_line_infinite(raw.0, raw.0 + wdir * 1000.0, nface[1].0, nface[1].1).map(|(p, _, _)| p);
        let left = match (left_a, left_b) {
            (Some(a), Some(b)) => pick_on_bisector(a, b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => raw.0,
        };

        let right_a = line_line_infinite(raw.1, raw.1 + wdir * 1000.0, nface[0].0, nface[0].1).map(|(p, _, _)| p);
        let right_b = line_line_infinite(raw.1, raw.1 + wdir * 1000.0, nface[1].0, nface[1].1).map(|(p, _, _)| p);
        let right = match (right_a, right_b) {
            (Some(a), Some(b)) => pick_on_bisector(a, b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => raw.1,
        };

        // Mitre limit: an acute corner would spike far down the bisector. If either
        // corner is further than MITRE_LIMIT half-thicknesses of the thicker wall from
        // the join point, square the end off instead. Both walls of the corner see the
        // same two shared points, so they make the same choice and stay gap-free.
        let limit = MITRE_LIMIT * (w.thickness.max(nw.thickness) / 2.0);
        if left.dist(pt) > limit || right.dist(pt) > limit {
            let wperp = Vec2::new(-wdir.y, wdir.x);
            let whalf = w.thickness / 2.0;
            let left = pt + wperp * whalf;
            let right = pt - wperp * whalf;
            return Ok(EndPlan { left, right, join: Some(Join::Square(end)) });
        }

        return Ok(EndPlan { left, right, join: Some(Join::L(end)) });
    }

    // X: a crossing joins nothing — record it and keep the end's own corners.
    if nbrs.iter().any(|n| matches!(n, Neighbour::X)) {
        return Ok(EndPlan { left: raw.0, right: raw.1, join: Some(Join::X(end)) });
    }

    Ok(EndPlan { left: raw.0, right: raw.1, join: None })
}

/// Solve every wall's poché ring with explicit join overrides. Output order ==
/// `model.walls` order; degenerate walls (`wall::wall_polygon` is `None`) are
/// skipped and produce no shape.
///
/// Join kinds are resolved per wall end: an explicit `JoinOverrides` entry wins;
/// otherwise the wall's own `join_start` / `join_end` fields are used.
pub fn solve_joins_with(model: &cadcraft_doc::ArchModel, ov: &JoinOverrides) -> Vec<WallShape> {
    let graph = WallGraph::build(model);
    let mut out: Vec<WallShape> = Vec::with_capacity(model.walls.len());

    let classify = |m: &cadcraft_doc::ArchModel, g: &WallGraph, i: ArchId, e: End| classify(m, g, i, e);

    for w in &model.walls {
        if wall_polygon(w).is_none() {
            continue;
        }
        let Some(face) = face_lines(w) else {
            continue;
        };
        let (ls, le) = face[0];
        let (rs, re) = face[1];

        // Start end.
        let plan_s = match plan_end(w, End::Start, (ls, rs), &graph, model, w.id, ov, &classify) {
            Ok(p) => p,
            Err(_) => continue,
        };
        // End end.
        let plan_e = match plan_end(w, End::End, (le, re), &graph, model, w.id, ov, &classify) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let corners = ring_from_corners(plan_s.left, plan_s.right, plan_e.left, plan_e.right);
        let (ring, joins) = match corners {
            Some(r) => (r, vec![plan_s.join, plan_e.join].into_iter().filter_map(|j| j).collect()),
            None => {
                // Rule 6: crossed/degenerate → fall back to the raw (untrimmed)
                // corners. The wall still exists; it simply records no joins.
                match ring_from_corners(ls, rs, le, re) {
                    Some(r) => (r, Vec::new()),
                    None => continue,
                }
            }
        };

        out.push(WallShape { id: w.id, polygon: ring, centre: (w.start, w.end), joins });
    }

    out
}

/// Solve every wall's poché ring (no overrides).
///
/// `solve_joins(m)` == `solve_joins_with(m, &JoinOverrides::default())`.
/// Each wall's own `join_start` / `join_end` fields are honoured.
pub fn solve_joins(model: &cadcraft_doc::ArchModel) -> Vec<WallShape> {
    solve_joins_with(model, &JoinOverrides::default())
}

/// Move one end of a wall to `to`, then transitively move every neighbour end that
/// shared that point (and their neighbours, and so on) so the wall graph stays
/// connected. Returns the ids of every wall whose geometry changed, sorted ascending
/// and deduplicated.
pub fn stretch_wall(
    model: &mut cadcraft_doc::ArchModel,
    id: cadcraft_doc::ArchId,
    end: End,
    to: cadcraft_geom::Vec2,
) -> Result<Vec<cadcraft_doc::ArchId>, ArchError> {
    if !to.is_finite() {
        return Err(ArchError::NotFinite);
    }

    let wall_idx = model.walls.iter().position(|w| w.id == id);
    let wall_idx = match wall_idx {
        Some(i) => i,
        None => return Err(ArchError::NoSuchWall(id)),
    };

    let graph = crate::graph::WallGraph::build(model);

    // Seed with the cluster containing (id, end); if the wall is degenerate
    // (no cluster), just move its own end.
    let initial: Vec<(ArchId, End)> = match graph.cluster_of(id, end) {
        Some(idx) => graph.cluster(idx).to_vec(),
        None => vec![(id, end)],
    };

    // Fixed-point BFS over the original graph snapshot.
    let mut visited: BTreeSet<(ArchId, u8)> = initial.iter().map(|(w, e)| (*w, end_key(*e))).collect();
    let mut moving: Vec<(ArchId, End)> = Vec::new();
    let mut queue: VecDeque<(ArchId, End)> = initial.into_iter().collect();
    let mut processed: usize = 0;

    while let Some((w, e)) = queue.pop_front() {
        if processed >= MAX_WALLS {
            break;
        }
        processed += 1;
        moving.push((w, e));

        if let Some(idx) = graph.cluster_of(w, e) {
            let cluster = graph.cluster(idx);
            for (w2, e2) in cluster {
                let key = (*w2, end_key(*e2));
                if visited.insert(key) {
                    queue.push_back((*w2, *e2));
                }
            }
        }
    }

    // Snapshot original positions before mutating.
    let mut originals: BTreeMap<ArchId, (Vec2, Vec2)> = BTreeMap::new();
    for (w, _) in &moving {
        if let Some(wall) = model.walls.iter().find(|wall| wall.id == *w) {
            originals.insert(*w, (wall.start, wall.end));
        }
    }

    // Apply the new position to each affected end.
    for (w, e) in &moving {
        if let Some(wall_idx) = model.walls.iter().position(|wall| wall.id == *w) {
            match e {
                End::Start => model.walls[wall_idx].start = to,
                End::End => model.walls[wall_idx].end = to,
            }
        }
    }

    // Collect ids of walls whose geometry actually changed (sorted, deduped).
    let mut changed: BTreeSet<ArchId> = BTreeSet::new();
    for (w, (old_start, old_end)) in &originals {
        if let Some(wall) = model.walls.iter().find(|wall| wall.id == *w) {
            if wall.start != *old_start || wall.end != *old_end {
                changed.insert(*w);
            }
        }
    }

    Ok(changed.into_iter().collect())
}

/// Maps an [`End`] to a small integer key for use in a `BTreeSet` pair.
fn end_key(e: End) -> u8 {
    match e {
        End::Start => 0,
        End::End => 1,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use cadcraft_doc::Wall;
    use cadcraft_geom::Vec2;

    #[test]
    fn solve_bounds_of_no_shapes_is_empty() {
        let shapes: Vec<WallShape> = Vec::new();
        let bounds = solve_bounds(&shapes);
        assert!(bounds.is_empty());
    }

    #[test]
    fn solve_bounds_covers_every_ring() {
        let shapes = vec![
            WallShape {
                id: ArchId(1),
                polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 1.0), Vec2::new(0.0, 1.0)],
                centre: (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
                joins: vec![],
            },
            WallShape {
                id: ArchId(2),
                polygon: vec![Vec2::new(5.0, 2.0), Vec2::new(15.0, 2.0), Vec2::new(15.0, 3.0), Vec2::new(5.0, 3.0)],
                centre: (Vec2::new(5.0, 2.0), Vec2::new(15.0, 2.0)),
                joins: vec![],
            },
        ];
        let bounds = solve_bounds(&shapes);
        assert!(!bounds.is_empty());
        for shape in &shapes {
            for p in &shape.polygon {
                assert!(p.x >= bounds.min.x && p.x <= bounds.max.x);
                assert!(p.y >= bounds.min.y && p.y <= bounds.max.y);
            }
        }
    }

    #[test]
    fn face_lines_are_the_two_long_edges_of_the_poché() {
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 2.0, height: 3.0, layers: vec![], ..Wall::default() };
        let lines = face_lines(&wall).unwrap();
        // Left face: offset +1.0 (left of a→b direction (1,0) → (0,1))
        assert_eq!(lines[0].0, Vec2::new(0.0, 1.0));
        assert_eq!(lines[0].1, Vec2::new(10.0, 1.0));
        // Right face: offset -1.0 (right of a→b direction (1,0) → (0,-1))
        assert_eq!(lines[1].0, Vec2::new(0.0, -1.0));
        assert_eq!(lines[1].1, Vec2::new(10.0, -1.0));
    }

    #[test]
    fn ring_from_corners_matches_wall_polygon_when_untrimmed() {
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 2.0, height: 3.0, layers: vec![], ..Wall::default() };
        let lines = face_lines(&wall).unwrap();
        let left_start = lines[0].0;
        let left_end = lines[0].1;
        let right_start = lines[1].0;
        let right_end = lines[1].1;

        let ring = ring_from_corners(left_start, right_start, left_end, right_end).unwrap();
        let expected = crate::wall::wall_polygon(&wall).unwrap();
        assert_eq!(ring, expected);
    }

    #[test]
    fn ring_from_corners_rejects_crossed_or_non_finite_corners() {
        // Non-finite point
        assert!(ring_from_corners(Vec2::new(f64::NAN, 0.0), Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 1.0),).is_none());

        // Crossed ends: (right_end - right_start).dot(left_end - left_start) <= 0
        // right_start=(0,1), right_end=(-10,1) → right_dir=(-10,0)
        // left_start=(0,-1), left_end=(10,-1) → left_dir=(10,0)
        // dot = -100 <= 0
        assert!(ring_from_corners(Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0), Vec2::new(10.0, -1.0), Vec2::new(-10.0, 1.0),).is_none());
    }

    #[test]
    fn end_other_round_trips() {
        assert_eq!(End::Start.other(), End::End);
        assert_eq!(End::End.other(), End::Start);
    }

    #[test]
    fn arch_error_display_messages_are_stable() {
        let e: String = ArchError::NoSuchWall(ArchId(42)).to_string();
        assert!(e.contains("no wall with id"));
        assert!(e.contains("42"));

        assert_eq!(ArchError::NotFinite.to_string(), "point is not finite");
        assert_eq!(ArchError::TooManyWalls.to_string(), "too many walls (max 4096)");
    }

    fn make_wall(model: &mut cadcraft_doc::ArchModel, id: u32, start: Vec2, end: Vec2) {
        model.walls.push(Wall { id: ArchId(id), start, end, thickness: 2.0, height: 3.0, layers: vec![], ..Wall::default() });
    }

    fn wall(model: &mut cadcraft_doc::ArchModel, id: u32, start: Vec2, end: Vec2, t: f64) {
        model.walls.push(Wall { id: ArchId(id), start, end, thickness: t, height: 3.0, layers: vec![], ..Wall::default() });
    }

    #[test]
    fn stretch_wall_moves_the_neighbour_end() {
        let mut model = cadcraft_doc::ArchModel::default();
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        make_wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0));

        let result = stretch_wall(&mut model, ArchId(0), End::End, Vec2::new(12.0, 3.0));
        let ids: Vec<ArchId> = result.unwrap();
        assert_eq!(ids, vec![ArchId(0), ArchId(1)]);

        // Wall 0's end moved to (12,3); wall 1's start followed.
        assert_eq!(model.walls[0].end, Vec2::new(12.0, 3.0));
        assert_eq!(model.walls[1].start, Vec2::new(12.0, 3.0));
        assert_eq!(model.walls[1].end, Vec2::new(10.0, 10.0));
    }

    #[test]
    fn stretch_wall_on_missing_id_is_an_error() {
        let mut model = cadcraft_doc::ArchModel::default();
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        let before = model.walls.clone();

        let result = stretch_wall(&mut model, ArchId(999), End::Start, Vec2::new(1.0, 1.0));
        assert_eq!(result, Err(ArchError::NoSuchWall(ArchId(999))));
        assert_eq!(model.walls, before);
    }

    #[test]
    fn stretch_wall_rejects_a_non_finite_point() {
        let mut model = cadcraft_doc::ArchModel::default();
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        let before = model.walls.clone();

        let r = stretch_wall(&mut model, ArchId(0), End::End, Vec2::new(f64::NAN, 0.0));
        assert_eq!(r, Err(ArchError::NotFinite));
        assert_eq!(model.walls, before);

        let r = stretch_wall(&mut model, ArchId(0), End::End, Vec2::new(f64::INFINITY, 0.0));
        assert_eq!(r, Err(ArchError::NotFinite));
        assert_eq!(model.walls, before);
    }

    #[test]
    fn stretch_wall_propagates_through_a_chain() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Three walls all sharing the point (10,0) — a chain/fan.
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        make_wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0));
        make_wall(&mut model, 2, Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0));

        let result = stretch_wall(&mut model, ArchId(0), End::End, Vec2::new(12.0, 3.0));
        let ids: Vec<ArchId> = result.unwrap();
        assert_eq!(ids, vec![ArchId(0), ArchId(1), ArchId(2)]);

        assert_eq!(model.walls[0].end, Vec2::new(12.0, 3.0));
        assert_eq!(model.walls[1].start, Vec2::new(12.0, 3.0));
        assert_eq!(model.walls[2].start, Vec2::new(12.0, 3.0));
    }

    #[test]
    fn stretch_wall_returns_only_changed_ids() {
        let mut model = cadcraft_doc::ArchModel::default();
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        make_wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0));

        // Drag wall 0's end to the point it already occupies.
        let result = stretch_wall(&mut model, ArchId(0), End::End, Vec2::new(10.0, 0.0));
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn stretch_wall_on_a_self_touching_loop_terminates() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Triangle: three walls meeting at three corners.
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        make_wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(5.0, 10.0));
        make_wall(&mut model, 2, Vec2::new(5.0, 10.0), Vec2::new(0.0, 0.0));

        // Dragging one corner must terminate (bounded by the visited set + MAX_WALLS).
        let result = stretch_wall(&mut model, ArchId(0), End::Start, Vec2::new(-1.0, 0.0));
        let ids: Vec<ArchId> = result.unwrap();
        // At most all three walls could be affected; the set is bounded.
        assert!((ids.len()..=3).contains(&ids.len()));
        assert!(!ids.is_empty());
    }

    #[test]
    fn l_join_mitres_both_walls_to_the_shared_corner() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        let s1 = &shapes[1];

        // Both shapes should have a Join::L at the touching end.
        assert!(s0.joins.iter().any(|j| matches!(j, Join::L(End::End))));
        assert!(s1.joins.iter().any(|j| matches!(j, Join::L(End::Start))));

        // The shared mitre corner (9.5, 0.5) should lie on the bisector line x + y = 10.
        let shared = Vec2::new(9.5, 0.5);
        assert!((shared.x + shared.y - 10.0).abs() < 1e-9, "mitre corner not on bisector");

        // Both rings should contain the shared corner.
        assert!(s0.polygon.iter().any(|p| p.dist(shared) < 1e-9), "wall 0 missing shared corner");
        assert!(s1.polygon.iter().any(|p| p.dist(shared) < 1e-9), "wall 1 missing shared corner");

        // Neither ring should have its raw square corner at the join.
        for p in &s0.polygon {
            assert!(!((p.x - 10.0).abs() < 1e-9 && (p.y - 0.5).abs() < 1e-9), "wall 0 still has raw corner (10, 0.5)");
            assert!(!((p.x - 10.0).abs() < 1e-9 && (p.y + 0.5).abs() < 1e-9), "wall 0 still has raw corner (10, -0.5)");
        }
        for p in &s1.polygon {
            assert!(!((p.x - 9.5).abs() < 1e-9 && p.y.abs() < 1e-9), "wall 1 still has raw corner (9.5, 0)");
            assert!(!((p.x - 10.5).abs() < 1e-9 && p.y.abs() < 1e-9), "wall 1 still has raw corner (10.5, 0)");
        }
    }

    #[test]
    fn t_join_butts_the_incoming_wall_at_the_face() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 1.0); // host
        wall(&mut model, 1, Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0), 1.0); // incoming

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let host = &shapes[0];
        let incoming = &shapes[1];

        // The incoming wall should have a Join::T at its end.
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::T(End::End))));

        // The incoming wall's end face should lie on y = 0.5 (the host's upper face).
        let end_face_corners: Vec<Vec2> = incoming.polygon.iter().filter(|p| (p.y - 0.5).abs() < 1e-9).cloned().collect();
        assert_eq!(end_face_corners.len(), 2, "incoming wall should have 2 corners on y=0.5");
        assert!((end_face_corners[0].x - 9.5).abs() < 1e-9 || (end_face_corners[0].x - 10.5).abs() < 1e-9);
        assert!((end_face_corners[1].x - 9.5).abs() < 1e-9 || (end_face_corners[1].x - 10.5).abs() < 1e-9);

        // The host's polygon should be unchanged (its bounds still span y in [-0.5, 0.5]).
        let host_bounds = host.bounds();
        assert!((host_bounds.min.y + 0.5).abs() < 1e-9, "host bounds min.y should be -0.5");
        assert!((host_bounds.max.y - 0.5).abs() < 1e-9, "host bounds max.y should be 0.5");
    }

    #[test]
    fn angled_join_mitres_at_the_bisector() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        let angle = std::f64::consts::PI / 3.0; // 60 degrees
        let end = Vec2::new(10.0 + 10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, Vec2::new(10.0, 0.0), end, 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        let s1 = &shapes[1];

        // Both shapes should have a Join::L at the touching end.
        assert!(s0.joins.iter().any(|j| matches!(j, Join::L(End::End))));
        assert!(s1.joins.iter().any(|j| matches!(j, Join::L(End::Start))));

        // The bisector direction is at 120 degrees from the x-axis.
        let bisector_dir = Vec2::new(-0.5, 3.0_f64.sqrt() / 2.0);
        let corner = Vec2::new(10.0, 0.0);

        // Both shared mitre corners should lie on the bisector line.
        let mut shared_count = 0;
        for p in &s0.polygon {
            for q in &s1.polygon {
                if p.dist(*q) < 1e-9 {
                    shared_count += 1;
                    let v = *p - corner;
                    let cross = v.cross(bisector_dir);
                    assert!(cross.abs() < 1e-9, "shared corner {:?} not on bisector", p);
                }
            }
        }
        assert_eq!(shared_count, 2, "expected 2 shared corners");
    }

    #[test]
    fn collinear_walls_butt_and_do_not_overlap() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        let s1 = &shapes[1];

        // Each wall should have area 10.0 (no overlap, no gap).
        let a0 = polygon_area(&s0.polygon);
        let a1 = polygon_area(&s1.polygon);
        assert!((a0 - 10.0).abs() < 1e-9, "wall 0 area should be 10.0, got {}", a0);
        assert!((a1 - 10.0).abs() < 1e-9, "wall 1 area should be 10.0, got {}", a1);

        // The sum should be 20.0 exactly.
        assert!((a0 + a1 - 20.0).abs() < 1e-9, "total area should be 20.0");
    }

    #[test]
    fn closed_rectangle_ring_is_gap_free() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.3);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.3);
        wall(&mut model, 2, Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.3);
        wall(&mut model, 3, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.3);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 4);

        // Every shape should have two Join::L joins.
        for s in &shapes {
            let l_count = s.joins.iter().filter(|j| matches!(j, Join::L(_))).count();
            assert_eq!(l_count, 2, "shape {:?} should have 2 Join::L joins", s.id);
        }

        // Outer corners of the ring.
        let outer = vec![Vec2::new(10.15, -0.15), Vec2::new(10.15, 8.15), Vec2::new(-0.15, 8.15), Vec2::new(-0.15, -0.15)];
        // Inner corners of the ring.
        let inner = vec![Vec2::new(9.85, 0.15), Vec2::new(9.85, 7.85), Vec2::new(0.15, 7.85), Vec2::new(0.15, 0.15)];

        // Each corner should appear in exactly two shapes' polygons.
        for c in outer.iter().chain(inner.iter()) {
            let mut count = 0;
            for s in &shapes {
                for p in &s.polygon {
                    if p.dist(*c) < 1e-9 {
                        count += 1;
                        break;
                    }
                }
            }
            assert_eq!(count, 2, "corner {:?} should appear in exactly 2 shapes, got {}", c, count);
        }
    }

    #[test]
    fn x_join_butts_both_at_the_crossing_centre() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 1.0);
        wall(&mut model, 1, Vec2::new(5.0, 0.0), Vec2::new(5.0, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        let s1 = &shapes[1];

        assert!(s0.joins.iter().any(|j| matches!(j, Join::X(_))));
        assert!(s1.joins.iter().any(|j| matches!(j, Join::X(_))));

        // An X crossing joins nothing: both walls keep their full length.
        // Wall 0 spans x 0..10 (the crossing at x=5 must not cut it).
        let b0 = s0.bounds();
        assert!((b0.min.x - 0.0).abs() < 1e-9, "wall 0 min.x: {}", b0.min.x);
        assert!((b0.max.x - 10.0).abs() < 1e-9, "wall 0 max.x: {}", b0.max.x);

        // Wall 1 spans y 0..10.
        let b1 = s1.bounds();
        assert!((b1.min.y - 0.0).abs() < 1e-9, "wall 1 min.y: {}", b1.min.y);
        assert!((b1.max.y - 10.0).abs() < 1e-9, "wall 1 max.y: {}", b1.max.y);
    }

    #[test]
    fn x_join_records_the_crossing_and_keeps_both_walls_whole() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 1.0);
        wall(&mut model, 1, Vec2::new(5.0, 0.0), Vec2::new(5.0, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        let s1 = &shapes[1];

        // Both walls record an X crossing at the shared end.
        assert!(s0.joins.iter().any(|j| matches!(j, Join::X(_))));
        assert!(s1.joins.iter().any(|j| matches!(j, Join::X(_))));

        // Wall 0 keeps its full x extent (0..10); the crossing at x=5 does not
        // cut it.
        let b0 = s0.bounds();
        assert!((b0.min.x - 0.0).abs() < 1e-9, "wall 0 min.x: {}", b0.min.x);
        assert!((b0.max.x - 10.0).abs() < 1e-9, "wall 0 max.x: {}", b0.max.x);

        // Wall 1 keeps its full y extent (0..10).
        let b1 = s1.bounds();
        assert!((b1.min.y - 0.0).abs() < 1e-9, "wall 1 min.y: {}", b1.min.y);
        assert!((b1.max.y - 10.0).abs() < 1e-9, "wall 1 max.y: {}", b1.max.y);
    }

    #[test]
    fn an_off_centre_crossing_does_not_delete_part_of_either_wall() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        wall(&mut model, 1, Vec2::new(7.0, -3.0), Vec2::new(7.0, 5.0), 0.2);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let a0 = polygon_area(&shapes[0].polygon);
        let a1 = polygon_area(&shapes[1].polygon);

        // Wall 0 is 10 m long × 0.2 m thick → 2.0 m²; the off-centre crossing
        // at x=7 must not delete any of it.
        assert!((a0 - 2.0).abs() < 1e-9, "wall 0 area: {}", a0);

        // Wall 1 is 8 m long × 0.2 m thick → 1.6 m².
        assert!((a1 - 1.6).abs() < 1e-9, "wall 1 area: {}", a1);
    }

    #[test]
    fn a_crossing_wall_does_not_undo_the_corner_mitre() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Closed rectangle 10×8, thickness 0.2.
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2);
        wall(&mut model, 2, Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2);
        wall(&mut model, 3, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2);
        // Short wall crossing the bottom wall at x=3.
        wall(&mut model, 4, Vec2::new(3.0, -2.0), Vec2::new(3.0, 2.0), 0.1);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 5);

        let bottom = &shapes[0];

        // The bottom wall's start corner is still mitred (L) — the crossing at
        // x=3 (interior) must not undo the corner mitre at x=0.
        assert!(bottom.joins.iter().any(|j| matches!(j, Join::L(End::Start))));

        // The mitre corner at (-0.1, -0.1) is still present.
        assert!(bottom.polygon.iter().any(|p| (p.x + 0.1).abs() < 1e-9 && (p.y + 0.1).abs() < 1e-9), "bottom wall missing mitre corner (-0.1, -0.1)");
    }

    #[test]
    fn a_wall_crossed_by_two_walls_keeps_its_full_length() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Main wall: (0,0) to (10,0), thickness 0.2
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        // Crossing wall 1: (3,-3) to (3,3), thickness 0.2
        wall(&mut model, 1, Vec2::new(3.0, -3.0), Vec2::new(3.0, 3.0), 0.2);
        // Crossing wall 2: (7,-3) to (7,3), thickness 0.2
        wall(&mut model, 2, Vec2::new(7.0, -3.0), Vec2::new(7.0, 3.0), 0.2);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3);

        // The main wall should span the full x range 0..10
        let main = &shapes[0];
        let b = main.bounds();
        assert!((b.min.x - 0.0).abs() < 1e-9, "main wall min.x: {}", b.min.x);
        assert!((b.max.x - 10.0).abs() < 1e-9, "main wall max.x: {}", b.max.x);
        // Area should be 10 * 0.2 = 2.0
        assert!((polygon_area(&main.polygon) - 2.0).abs() < 1e-9, "main wall area: {}", polygon_area(&main.polygon));
    }

    #[test]
    fn a_crossing_near_the_end_does_not_affect_the_end_join() {
        let mut model = cadcraft_doc::ArchModel::default();
        // L-corner: wall 0 horizontal (0,0)-(10,0), wall 1 vertical (10,0)-(10,10)
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 0.2);
        // A crossing wall near wall 0's start end: (1,-2) to (1,2)
        wall(&mut model, 2, Vec2::new(1.0, -2.0), Vec2::new(1.0, 2.0), 0.1);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3);

        let w0 = &shapes[0];
        // Wall 0's end (at x=10) should still be an L join with wall 1
        assert!(w0.joins.iter().any(|j| matches!(j, Join::L(End::End))), "wall 0 end should be L join");
        // Wall 0 should span the full x range 0..10
        let b = w0.bounds();
        assert!((b.min.x - 0.0).abs() < 1e-9, "wall 0 min.x: {}", b.min.x);
        assert!((b.max.x - 10.0).abs() < 1e-9, "wall 0 max.x: {}", b.max.x);
    }

    #[test]
    fn multiple_crossings_on_one_wall() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Main wall: (0,0) to (12,0), thickness 0.2
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(12.0, 0.0), 0.2);
        // Crossing at x=2
        wall(&mut model, 1, Vec2::new(2.0, -2.0), Vec2::new(2.0, 2.0), 0.1);
        // Crossing at x=6
        wall(&mut model, 2, Vec2::new(6.0, -2.0), Vec2::new(6.0, 2.0), 0.1);
        // Crossing at x=10
        wall(&mut model, 3, Vec2::new(10.0, -2.0), Vec2::new(10.0, 2.0), 0.1);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 4);

        let main = &shapes[0];
        // The main wall should span the full x range 0..12
        let b = main.bounds();
        assert!((b.min.x - 0.0).abs() < 1e-9, "main wall min.x: {}", b.min.x);
        assert!((b.max.x - 12.0).abs() < 1e-9, "main wall max.x: {}", b.max.x);
        // Area should be 12 * 0.2 = 2.4
        assert!((polygon_area(&main.polygon) - 2.4).abs() < 1e-9, "main wall area: {}", polygon_area(&main.polygon));
    }

    #[test]
    fn a_crossing_wall_with_nan_coordinates_does_not_crash() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Main wall: (0,0) to (10,0), thickness 0.2
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        // Crossing wall with NaN in one coordinate
        wall(&mut model, 1, Vec2::new(5.0, f64::NAN), Vec2::new(5.0, 3.0), 0.2);

        // Should not crash
        let shapes = solve_joins(&model);
        // The main wall should still produce a shape
        assert!(shapes.iter().any(|s| s.id == ArchId(0)), "main wall should produce a shape");
    }

    #[test]
    fn a_zero_length_wall_crossed_by_another() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Zero-length "wall" at (5,0)
        wall(&mut model, 0, Vec2::new(5.0, 0.0), Vec2::new(5.0, 0.0), 0.2);
        // Crossing wall: (0,0) to (10,0)
        wall(&mut model, 1, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);

        // Should not crash
        let shapes = solve_joins(&model);
        // The crossing wall should produce a shape and span the full length
        let crossing = shapes.iter().find(|s| s.id == ArchId(1));
        assert!(crossing.is_some(), "crossing wall should produce a shape");
        let b = crossing.unwrap().bounds();
        assert!((b.min.x - 0.0).abs() < 1e-9, "crossing wall min.x: {}", b.min.x);
        assert!((b.max.x - 10.0).abs() < 1e-9, "crossing wall max.x: {}", b.max.x);
    }

    #[test]
    fn join_kind_butt_overrides_the_mitre() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0); // a
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0); // b

        let mut ov = JoinOverrides::new();
        ov.set(ArchId(0), JoinKind::Auto, JoinKind::Butt);

        let shapes = solve_joins_with(&model, &ov);
        assert_eq!(shapes.len(), 2);

        let a = &shapes[0];
        let b = &shapes[1];

        // Wall a's End::End is overridden to Butt: raw square corner, not a mitre.
        assert!(a.joins.iter().any(|j| matches!(j, Join::Butt(End::End))));
        assert!(a.polygon.iter().any(|p| (p.x - 10.0).abs() < 1e-9 && (p.y - 0.5).abs() < 1e-9), "wall a should keep raw corner (10, 0.5)");
        assert!(a.polygon.iter().any(|p| (p.x - 10.0).abs() < 1e-9 && (p.y + 0.5).abs() < 1e-9), "wall a should keep raw corner (10, -0.5)");
        assert!(!a.polygon.iter().any(|p| (p.x - 9.5).abs() < 1e-9 && (p.y - 0.5).abs() < 1e-9), "wall a must not mitre to (9.5, 0.5)");

        // Wall b still mitres.
        assert!(b.joins.iter().any(|j| matches!(j, Join::L(End::Start))));
    }

    #[test]
    fn join_kind_square_trims_to_the_join_point() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0); // a
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0); // b

        let mut ov = JoinOverrides::new();
        ov.set(ArchId(0), JoinKind::Auto, JoinKind::Square);

        let shapes = solve_joins_with(&model, &ov);
        assert_eq!(shapes.len(), 2);

        let a = &shapes[0];

        // Wall a's End::End is overridden to Square: end face perpendicular to a's
        // direction (vertical) and passing through the shared centre point (10,0).
        assert!(a.joins.iter().any(|j| matches!(j, Join::Square(End::End))));
        let corners: Vec<Vec2> = a.polygon.iter().filter(|p| (p.x - 10.0).abs() < 1e-9).cloned().collect();
        assert_eq!(corners.len(), 2, "wall a should have 2 corners at x=10");
        for p in &corners {
            assert!((p.y - 0.0).abs() < 1e-9 || (p.y - 0.5).abs() < 1e-9 || (p.y + 0.5).abs() < 1e-9, "wall a corner {:?} not on the join line", p);
        }
        // The end face is perpendicular to a's direction (vertical at x=10).
        assert!((corners[0].x - corners[1].x).abs() < 1e-9, "wall a end face not perpendicular to its direction");
    }

    #[test]
    fn parallel_neighbours_fall_back_to_square() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(0.0, 1.0), Vec2::new(10.0, 1.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        for s in &shapes {
            assert!(s.joins.iter().any(|j| matches!(j, Join::Square(End::End))), "shape {:?} should record Join::Square", s.id);
        }
    }

    // The pre-change full-model scan, kept test-only to verify the centre-line
    // box prefilter in `classify` loses no neighbour.
    #[cfg(test)]
    fn classify_full(model: &cadcraft_doc::ArchModel, g: &WallGraph, id: ArchId, end: End) -> Vec<Neighbour> {
        let Some(w) = model.walls.iter().find(|w| w.id == id) else {
            return Vec::new();
        };
        let pt = end.point(w);
        let dir = wall_dir(w);

        let mut neighbours = Vec::new();

        // Check cluster for EndToEnd, Collinear, and Fan
        if let Some(cluster_idx) = g.cluster_of(id, end) {
            let cluster = g.cluster(cluster_idx);
            if cluster.len() > MAX_JOIN_FAN {
                return vec![Neighbour::Fan { count: cluster.len() }];
            }
            for (nid, nend) in cluster {
                if *nid == id {
                    continue;
                }
                let Some(nw) = model.walls.iter().find(|w| w.id == *nid) else {
                    continue;
                };
                let Some(ndir) = wall_dir(nw) else {
                    continue;
                };
                if let Some(d) = dir {
                    let cross = d.cross(ndir);
                    // Collinear: parallel and on the same line.
                    if cross.abs() < PARALLEL_EPS {
                        let dist = (pt - nw.start).cross(ndir);
                        if dist.abs() < ENDPOINT_TOL {
                            let nend_at_pt = if nw.start.dist(pt) < ENDPOINT_TOL { End::Start } else { End::End };
                            neighbours.push(Neighbour::Collinear { id: *nid, end: nend_at_pt });
                            continue;
                        }
                    }
                }
                // EndToEnd: the neighbour's end is also at this point.
                if nend.point(nw).dist(pt) < ENDPOINT_TOL {
                    neighbours.push(Neighbour::EndToEnd { id: *nid, end: *nend });
                }
            }
        }

        // Check all walls for Mid (centre line passes through this point)
        for nw in &model.walls {
            if nw.id == id {
                continue;
            }
            // Skip if already found as EndToEnd or Collinear in the cluster
            if neighbours.iter().any(|n| matches!(n, Neighbour::EndToEnd { id: nid, .. } | Neighbour::Collinear { id: nid, .. } if *nid == nw.id)) {
                continue;
            }
            let Some(ndir) = wall_dir(nw) else {
                continue;
            };
            let d = (pt - nw.start).cross(ndir);
            let t = (pt - nw.start).dot(ndir);
            let nw_len = nw.start.dist(nw.end);
            if d.abs() < ENDPOINT_TOL && t > ENDPOINT_TOL && t < nw_len - ENDPOINT_TOL {
                let nend = if nw.start.dist(pt) < nw.end.dist(pt) { End::Start } else { End::End };
                neighbours.push(Neighbour::Mid { id: nw.id, end: nend });
            }
        }

        // X and Parallel only make sense if this wall has a direction (non-zero length).
        if let Some(dir) = dir {
            // X: this wall's centre line is crossed by another wall's centre line at a point
            // interior to both, and this end is the nearest to the crossing.
            for nw in &model.walls {
                if nw.id == id {
                    continue;
                }
                if neighbours.iter().any(|n| matches!(n, Neighbour::EndToEnd { id: nid, .. } | Neighbour::Collinear { id: nid, .. } | Neighbour::Mid { id: nid, .. } | Neighbour::Parallel { id: nid, .. } if *nid == nw.id)) {
                    continue;
                }
                let Some(ndir) = wall_dir(nw) else {
                    continue;
                };
                let Some(cross_pt) = line_line_infinite(w.start, w.end, nw.start, nw.end) else {
                    continue;
                };
                let cross_pt = cross_pt.0;
                // Crossing must be interior (strictly between endpoints) on both centre lines.
                let t1 = (cross_pt - w.start).dot(dir);
                let t2 = (cross_pt - nw.start).dot(ndir);
                let w_len = w.start.dist(w.end);
                let nw_len = nw.start.dist(nw.end);
                if t1 <= ENDPOINT_TOL || t1 >= w_len - ENDPOINT_TOL {
                    continue;
                }
                if t2 <= ENDPOINT_TOL || t2 >= nw_len - ENDPOINT_TOL {
                    continue;
                }
                // This end is the nearest to the crossing (End wins the tie when equidistant).
                let this_is_nearest =
                    if end == End::Start { pt.dist(cross_pt) < w.end.dist(cross_pt) } else { pt.dist(cross_pt) <= w.start.dist(cross_pt) };
                if this_is_nearest {
                    neighbours.push(Neighbour::X);
                }
            }

            // Parallel: parallel (not collinear) neighbour whose end is aligned with this end
            // along the wall direction and close in the perpendicular direction.
            for nw in &model.walls {
                if nw.id == id {
                    continue;
                }
                if neighbours.iter().any(|n| matches!(n, Neighbour::EndToEnd { id: nid, .. } | Neighbour::Collinear { id: nid, .. } | Neighbour::Mid { id: nid, .. } | Neighbour::Parallel { id: nid, .. } if *nid == nw.id)) {
                    continue;
                }
                let Some(ndir) = wall_dir(nw) else {
                    continue;
                };
                let cross = dir.cross(ndir);
                if cross.abs() < PARALLEL_EPS {
                    let d = (pt - nw.start).cross(ndir);
                    if d.abs() < ENDPOINT_TOL {
                        continue; // collinear, handled in the cluster
                    }
                    // parallel but not collinear
                    let nend_pt = if nw.start.dist(pt) < nw.end.dist(pt) { nw.start } else { nw.end };
                    let nend = if nw.start.dist(pt) < nw.end.dist(pt) { End::Start } else { End::End };
                    let aligned = (nend_pt - pt).dot(dir).abs() < ENDPOINT_TOL;
                    let close = pt.dist(nend_pt) < w.thickness + nw.thickness + ENDPOINT_TOL;
                    if aligned && close {
                        neighbours.push(Neighbour::Parallel { id: nw.id, end: nend });
                    }
                }
            }
        }

        neighbours
    }

    // The pre-change `solve_joins`, driven by the full-model scan, kept test-only
    // to verify the prefilted output is identical.
    #[cfg(test)]
    fn solve_joins_full(model: &cadcraft_doc::ArchModel) -> Vec<WallShape> {
        let graph = WallGraph::build(model);
        let mut out: Vec<WallShape> = Vec::with_capacity(model.walls.len());

        let classify = |m: &cadcraft_doc::ArchModel, g: &WallGraph, i: ArchId, e: End| classify_full(m, g, i, e);

        for w in &model.walls {
            if wall_polygon(w).is_none() {
                continue;
            }
            let Some(face) = face_lines(w) else {
                continue;
            };
            let (ls, le) = face[0];
            let (rs, re) = face[1];

            // Start end.
            let plan_s = match plan_end(w, End::Start, (ls, rs), &graph, model, w.id, &JoinOverrides::default(), &classify) {
                Ok(p) => p,
                Err(_) => continue,
            };
            // End end.
            let plan_e = match plan_end(w, End::End, (le, re), &graph, model, w.id, &JoinOverrides::default(), &classify) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let corners = ring_from_corners(plan_s.left, plan_s.right, plan_e.left, plan_e.right);
            let (ring, joins) = match corners {
                Some(r) => (r, vec![plan_s.join, plan_e.join].into_iter().filter_map(|j| j).collect()),
                None => {
                    // Rule 6: crossed/degenerate → fall back to the raw (untrimmed)
                    // corners. The wall still exists; it simply records no joins.
                    match ring_from_corners(ls, rs, le, re) {
                        Some(r) => (r, Vec::new()),
                        None => continue,
                    }
                }
            };

            out.push(WallShape { id: w.id, polygon: ring, centre: (w.start, w.end), joins });
        }

        out
    }

    #[test]
    fn a_wall_shorter_than_its_neighbours_mitre_keeps_its_raw_ring() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(0.1, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(0.1, 0.0), Vec2::new(0.1, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2, "both walls should produce a shape");

        // w0 is the 0.1 m stub mitred into a 1 m thick wall. Its trimmed corners
        // cross over, so it must fall back to its raw ring and record no joins.
        let w0 = &model.walls[0];
        let shape0 = &shapes[0];

        assert_eq!(shape0.id, w0.id);
        assert_eq!(shape0.polygon, crate::wall::wall_polygon(w0).unwrap());
        assert!(shape0.joins.is_empty());
    }

    #[test]
    fn a_short_wall_between_two_walls_keeps_its_raw_ring_at_both_ends() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Short horizontal wall between two vertical walls; wall 1 is thicker so the
        // mitres at both ends cross over and the raw ring is kept.
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(0.1, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(0.0, 0.0), Vec2::new(0.0, 10.0), 1.5);
        wall(&mut model, 2, Vec2::new(0.1, 0.0), Vec2::new(0.1, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3, "all three walls should produce shapes");

        // The short wall should keep its raw ring with no joins at either end.
        let shape0 = &shapes[0];
        assert_eq!(shape0.id, ArchId(0));
        assert_eq!(shape0.polygon, crate::wall::wall_polygon(&model.walls[0]).unwrap());
        assert!(shape0.joins.is_empty());
    }

    #[test]
    fn a_thin_stub_mitred_into_thick_wall_keeps_raw_ring() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Thin stub (thickness 0.2) mitred into thick wall (thickness 2.0).
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(0.1, 0.0), 0.2);
        wall(&mut model, 1, Vec2::new(0.1, 0.0), Vec2::new(0.1, 10.0), 2.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2, "both walls should produce shapes");

        // The thin stub should keep its raw ring with no joins.
        let shape0 = &shapes[0];
        assert_eq!(shape0.id, ArchId(0));
        assert_eq!(shape0.polygon, crate::wall::wall_polygon(&model.walls[0]).unwrap());
        assert!(shape0.joins.is_empty());
    }

    #[test]
    fn a_short_stub_at_large_coordinates_keeps_its_raw_ring() {
        let mut model = cadcraft_doc::ArchModel::default();
        let base = 1e15;
        wall(&mut model, 0, Vec2::new(base, 0.0), Vec2::new(base + 0.1, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(base + 0.1, 0.0), Vec2::new(base + 0.1, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2, "both walls should produce shapes");

        // Large coordinates must not break the raw-ring fallback.
        let shape0 = &shapes[0];
        assert_eq!(shape0.id, ArchId(0));
        assert_eq!(shape0.polygon, crate::wall::wall_polygon(&model.walls[0]).unwrap());
        assert!(shape0.joins.is_empty());
    }

    #[test]
    fn a_wall_with_one_crossed_end_falls_back_to_raw_ring_for_both_ends() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Short wall: the thicker wall 1 makes the mitres cross over at the start,
        // so the whole wall falls back to the raw ring with no joins.
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(0.1, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(0.0, 0.0), Vec2::new(0.0, 10.0), 1.5);
        wall(&mut model, 2, Vec2::new(0.1, 0.0), Vec2::new(0.1, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3, "all three walls should produce shapes");

        // The short wall should have the raw ring with no joins (both ends).
        let shape0 = &shapes[0];
        assert_eq!(shape0.id, ArchId(0));
        assert!(shape0.joins.is_empty());
        // The raw ring has 4 corners for a 0.1 x 1.0 rectangle.
        assert_eq!(shape0.polygon.len(), 4);
    }

    #[test]
    fn solve_joins_is_deterministic_and_skips_degenerate_walls() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0); // good
        wall(&mut model, 1, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), 1.0); // zero length
        wall(&mut model, 2, Vec2::new(f64::NAN, 0.0), Vec2::new(1.0, 0.0), 1.0); // NaN
        wall(&mut model, 3, Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), -1.0); // negative thickness

        let first = solve_joins(&model);
        assert_eq!(first.len(), 1, "only the good wall should produce a shape");
        let second = solve_joins(&model);
        assert_eq!(first, second, "solve_joins must be deterministic");
    }

    #[test]
    fn eight_walls_at_one_point_are_capped_not_panicky() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Nine walls radiating from (0,0) with far endpoints.
        for i in 0..9 {
            let angle = (i as f64) * (std::f64::consts::PI / 4.0);
            let far = Vec2::new(10.0 * angle.cos(), 10.0 * angle.sin());
            wall(&mut model, i, Vec2::new(0.0, 0.0), far, 1.0);
        }

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 9, "all nine walls should produce shapes");

        // Every shape has a Join at the shared (start) end.
        for s in &shapes {
            assert!(
                s.joins.iter().any(|j| matches!(
                    j,
                    Join::Butt(End::Start) | Join::Square(End::Start) | Join::L(End::Start) | Join::X(End::Start) | Join::T(End::Start)
                )),
                "shape {:?} missing join at shared end",
                s.id
            );
        }

        // No polygon self-intersects: every ring has positive area.
        for s in &shapes {
            assert!(polygon_area(&s.polygon) > 0.0, "shape {:?} has non-positive area (self-intersecting)", s.id);
        }
    }

    #[test]
    fn an_angled_t_join_meets_the_host_face_with_both_corners() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 1.0); // host
        wall(&mut model, 1, Vec2::new(5.0, 5.0), Vec2::new(10.0, 0.0), 0.5); // incoming

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let incoming = &shapes[1];

        // The incoming wall ends in a T join at its End (the end on the host).
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::T(End::End))));

        // Exactly two polygon vertices on y = 0.5 (the host's upper face).
        let on_face: Vec<Vec2> = incoming.polygon.iter().filter(|p| (p.y - 0.5).abs() < 1e-9).cloned().collect();
        assert_eq!(on_face.len(), 2, "expected 2 vertices on the host face, got {:?}", on_face);

        // No vertex below the host face.
        for p in &incoming.polygon {
            assert!(p.y >= 0.5 - 1e-9, "vertex {:?} below the host face", p);
        }
    }

    #[test]
    fn an_acute_corner_squares_off_instead_of_spiking() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        // Second wall from (10,0) turning back by 10 deg: a very acute corner.
        let angle = std::f64::consts::PI / 18.0; // 10 degrees
        let end = Vec2::new(10.0, 0.0) + Vec2::new(-10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, Vec2::new(10.0, 0.0), end, 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // Both shapes carry Join::Square at the corner end (no mitre spike).
        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::Square(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::Square(End::Start))));

        // No vertex spikes far from the corner: every vertex within 10.1 of (10,0)
        // and none has x > 10.5.
        let corner = Vec2::new(10.0, 0.0);
        for s in &shapes {
            for p in &s.polygon {
                assert!(p.dist(corner) < 10.1, "vertex {:?} too far from the corner", p);
                assert!(p.x <= 10.5, "vertex {:?} spikes past x=10.5", p);
            }
        }
    }

    #[test]
    fn a_right_angle_corner_is_within_the_mitre_limit() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // A right angle is well within the mitre limit: still an L mitre at both ends.
        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::L(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::L(End::Start))));
    }

    #[test]
    fn a_corner_wider_than_29_degrees_still_mitres() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        // 30-degree corner (well within the 29-degree threshold for mitring).
        let angle = std::f64::consts::PI / 6.0;
        let end = Vec2::new(10.0, 0.0) + Vec2::new(-10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, Vec2::new(10.0, 0.0), end, 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // Both walls should still mitre (L join), not square off.
        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::L(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::L(End::Start))));
    }

    #[test]
    fn a_thin_wall_meeting_a_thick_wall_at_an_acute_corner_squares_off() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        // 10-degree corner with a thick wall.
        let angle = std::f64::consts::PI / 18.0;
        let end = Vec2::new(10.0, 0.0) + Vec2::new(-10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, Vec2::new(10.0, 0.0), end, 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // The mitre limit uses the thicker wall (1.0), so both should square off.
        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::Square(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::Square(End::Start))));
    }

    #[test]
    fn both_walls_of_a_squared_off_corner_agree() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        let angle = std::f64::consts::PI / 18.0; // 10 degrees
        let end = Vec2::new(10.0, 0.0) + Vec2::new(-10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, Vec2::new(10.0, 0.0), end, 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // Both walls must agree: either both L or both Square. Never one L and one Square.
        let s0_has_l = shapes[0].joins.iter().any(|j| matches!(j, Join::L(End::End)));
        let s0_has_sq = shapes[0].joins.iter().any(|j| matches!(j, Join::Square(End::End)));
        let s1_has_l = shapes[1].joins.iter().any(|j| matches!(j, Join::L(End::Start)));
        let s1_has_sq = shapes[1].joins.iter().any(|j| matches!(j, Join::Square(End::Start)));

        assert!(
            (s0_has_l && s1_has_l) || (s0_has_sq && s1_has_sq),
            "walls disagree on join type: s0 L={} Sq={}, s1 L={} Sq={}",
            s0_has_l,
            s0_has_sq,
            s1_has_l,
            s1_has_sq
        );
    }

    #[test]
    fn an_acute_corner_with_large_coordinates_squares_off() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(1e15, 0.0), Vec2::new(1e15 + 10.0, 0.0), 1.0);
        let angle = std::f64::consts::PI / 18.0; // 10 degrees
        let corner = Vec2::new(1e15 + 10.0, 0.0);
        let end = corner + Vec2::new(-10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, corner, end, 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // Large coordinates should not break the mitre limit calculation.
        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::Square(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::Square(End::Start))));
    }

    #[test]
    fn a_mitre_limit_with_very_thin_walls() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.001);
        // 20-degree corner with very thin walls: limit = 4.0 * 0.001 / 2.0 = 0.002
        // mitre distance = 0.0005 / sin(10°) = 0.00288 > 0.002 → squares off
        let angle = std::f64::consts::PI / 9.0; // 20 degrees
        let end = Vec2::new(10.0, 0.0) + Vec2::new(-10.0 * angle.cos(), 10.0 * angle.sin());
        wall(&mut model, 1, Vec2::new(10.0, 0.0), end, 0.001);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::Square(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::Square(End::Start))));
    }

    #[test]
    fn a_tee_of_three_ends_butts_the_straight_pair() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 1.0); // host left
        wall(&mut model, 1, Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 1.0); // host right
        wall(&mut model, 2, Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0), 0.5); // stem

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3);

        let stem = &shapes[2];

        // The stem butts the straight pair with a T join at its Start.
        assert!(stem.joins.iter().any(|j| matches!(j, Join::T(End::Start))));

        let b = stem.bounds();
        assert!((b.min.y - 0.5).abs() < 1e-9, "stem min.y: {}", b.min.y);
        assert!((b.min.x - 4.75).abs() < 1e-9, "stem min.x: {}", b.min.x);
        assert!((b.max.x - 5.25).abs() < 1e-9, "stem max.x: {}", b.max.x);
    }

    #[test]
    fn t_join_with_different_thicknesses_meets_host_face() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 2.0); // thick host
        wall(&mut model, 1, Vec2::new(5.0, 5.0), Vec2::new(10.0, 0.0), 0.5); // thin incoming at angle

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let incoming = &shapes[1];

        // The incoming wall ends in a T join at its End.
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::T(End::End))));

        // The host's upper face is at y = 1.0 (thickness 2.0).
        // Exactly two polygon vertices of the incoming wall should lie on that face.
        let on_face: Vec<Vec2> = incoming.polygon.iter().filter(|p| (p.y - 1.0).abs() < 1e-9).cloned().collect();
        assert_eq!(on_face.len(), 2, "expected 2 vertices on the host face y=1.0, got {:?}", on_face);

        // No vertex should be below the host face.
        for p in &incoming.polygon {
            assert!(p.y >= 1.0 - 1e-9, "vertex {:?} below the host face", p);
        }
    }

    #[test]
    fn t_join_falls_back_to_square_when_intersection_is_far() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Host wall is very short; the incoming wall's face lines, extended, would
        // intersect the host face line far from the join point (beyond T_JOIN_REACH).
        wall(&mut model, 0, Vec2::new(9.5, 0.0), Vec2::new(10.5, 0.0), 1.0); // very short host
        wall(&mut model, 1, Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0), 1.0); // incoming

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let incoming = &shapes[1];

        // Still records a T join, but the corners fall back to the square end on the face.
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::T(End::End))));

        // The end face should be perpendicular to the incoming wall's direction (horizontal).
        let end_corners: Vec<Vec2> = incoming.polygon.iter().filter(|p| (p.y - 0.5).abs() < 1e-9).cloned().collect();
        assert_eq!(end_corners.len(), 2, "expected 2 corners on the host face");
        // The corners should be at x = 9.5 and x = 10.5 (the incoming wall's face lines).
        let xs: Vec<f64> = end_corners.iter().map(|p| p.x).collect();
        assert!((xs[0] - 9.5).abs() < 1e-9 || (xs[0] - 10.5).abs() < 1e-9, "corner x: {}", xs[0]);
        assert!((xs[1] - 9.5).abs() < 1e-9 || (xs[1] - 10.5).abs() < 1e-9, "corner x: {}", xs[1]);
    }

    #[test]
    fn tee_with_different_thicknesses_butts_the_straight_pair() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 1.0); // host left, thick
        wall(&mut model, 1, Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 1.5); // host right, thicker
        wall(&mut model, 2, Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0), 0.5); // stem, thin

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3);

        let stem = &shapes[2];

        // The stem butts the straight pair with a T join despite different thicknesses.
        assert!(stem.joins.iter().any(|j| matches!(j, Join::T(End::Start))));

        // The stem's end face should lie on the host's upper face (y = max(1.0, 1.5)/2 = 0.75).
        let b = stem.bounds();
        assert!((b.min.y - 0.75).abs() < 1e-9, "stem min.y should be 0.75, got {}", b.min.y);
    }

    #[test]
    fn t_join_with_thicker_incoming_wall() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 1.0); // thin host
        wall(&mut model, 1, Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0), 2.0); // thick incoming

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let incoming = &shapes[1];
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::T(End::End))));

        // The incoming wall's end face should lie on y = 0.5 (the host's upper face).
        let on_face: Vec<Vec2> = incoming.polygon.iter().filter(|p| (p.y - 0.5).abs() < 1e-9).cloned().collect();
        assert_eq!(on_face.len(), 2, "expected 2 vertices on the host face");

        // No vertex below the host face.
        for p in &incoming.polygon {
            assert!(p.y >= 0.5 - 1e-9, "vertex {:?} below the host face", p);
        }
    }

    #[test]
    fn t_join_with_zero_length_host_still_works() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Host is a point at (10,0) with thickness 1.0
        wall(&mut model, 0, Vec2::new(10.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0), 1.0); // incoming

        let shapes = solve_joins(&model);
        // The zero-length host produces no shape; only the incoming wall should.
        assert!(shapes.len() <= 1, "at most 1 shape");

        // Should not crash regardless of how many shapes are produced.
    }

    #[test]
    fn tee_with_three_different_thicknesses() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.5); // host left, thin
        wall(&mut model, 1, Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 2.0); // host right, thick
        wall(&mut model, 2, Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0), 1.0); // stem, medium

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 3);

        let stem = &shapes[2];
        assert!(stem.joins.iter().any(|j| matches!(j, Join::T(End::Start))));

        // The stem should butt against the thicker host's face (y = 2.0/2 = 1.0).
        let b = stem.bounds();
        assert!((b.min.y - 1.0).abs() < 1e-9, "stem min.y should be 1.0, got {}", b.min.y);
    }

    #[test]
    fn t_join_with_nan_coordinates_does_not_crash() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 1.0); // host
        wall(&mut model, 1, Vec2::new(f64::NAN, 5.0), Vec2::new(10.0, 0.0), 1.0); // incoming with NaN start

        // Should not crash; the incoming wall may or may not produce a shape.
        let shapes = solve_joins(&model);
        assert!(shapes.len() <= 2, "at most 2 shapes");

        // The host should still produce a shape.
        let host = shapes.iter().find(|s| s.id == ArchId(0));
        assert!(host.is_some(), "host wall should produce a shape");
    }

    #[test]
    fn t_join_with_large_coordinates() {
        let mut model = cadcraft_doc::ArchModel::default();
        let base = 1e15;
        wall(&mut model, 0, Vec2::new(base, 0.0), Vec2::new(base + 20.0, 0.0), 1.0); // host
        wall(&mut model, 1, Vec2::new(base + 10.0, 10.0), Vec2::new(base + 10.0, 0.0), 1.0); // incoming

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let incoming = &shapes[1];
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::T(End::End))));

        // The incoming wall's end face should lie on y = 0.5.
        let on_face: Vec<Vec2> = incoming.polygon.iter().filter(|p| (p.y - 0.5).abs() < 1e-6).cloned().collect();
        assert_eq!(on_face.len(), 2, "expected 2 vertices on the host face");
    }

    #[test]
    fn a_walls_own_join_kind_is_honoured_without_overrides() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Wall 0: (0,0)-(10,0), Wall 1: (10,0)-(10,10)
        // Default join kinds are Auto, which usually results in an L join (mitre).
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);

        // Set Wall 0's end join to Butt explicitly on the wall itself.
        model.walls[0].join_end = JoinKind::Butt;

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        // Wall 0 should have a Butt join at its End, overriding the default L join logic.
        assert!(s0.joins.iter().any(|j| matches!(j, Join::Butt(End::End))), "Wall 0 should have Join::Butt(End::End)");

        // Now test that an explicit override in JoinOverrides wins over the wall's own setting.
        let mut ov = JoinOverrides::new();
        // Override Wall 0's End to Square.
        ov.set(ArchId(0), JoinKind::Auto, JoinKind::Square);

        let shapes_ov = solve_joins_with(&model, &ov);
        assert_eq!(shapes_ov.len(), 2);

        let s0_ov = &shapes_ov[0];
        // Wall 0 should now have a Square join at its End, because the override wins.
        assert!(s0_ov.joins.iter().any(|j| matches!(j, Join::Square(End::End))), "Wall 0 should have Join::Square(End::End) due to override");
    }

    #[test]
    fn join_kind_mitre_on_wall_produces_l_join() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);

        // Explicit Mitre on the wall — same as Auto for an L corner, but must not error.
        model.walls[0].join_end = JoinKind::Mitre;
        model.walls[1].join_start = JoinKind::Mitre;

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);
        assert!(shapes[0].joins.iter().any(|j| matches!(j, Join::L(End::End))));
        assert!(shapes[1].joins.iter().any(|j| matches!(j, Join::L(End::Start))));
    }

    #[test]
    fn both_ends_independently_honoured() {
        let mut model = cadcraft_doc::ArchModel::default();
        // L-corner: wall 0 horizontal, wall 1 vertical.
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);

        // Wall 0: Butt at start, Square at end.
        model.walls[0].join_start = JoinKind::Butt;
        model.walls[0].join_end = JoinKind::Square;

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let s0 = &shapes[0];
        assert!(s0.joins.iter().any(|j| matches!(j, Join::Butt(End::Start))), "wall 0 start should be Butt");
        assert!(s0.joins.iter().any(|j| matches!(j, Join::Square(End::End))), "wall 0 end should be Square");
        // Neither end should be an L mitre.
        assert!(!s0.joins.iter().any(|j| matches!(j, Join::L(_))), "wall 0 must not mitre");
    }

    #[test]
    fn override_with_auto_falls_through_to_wall_kind() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);

        // Wall 0 has Butt at its end.
        model.walls[0].join_end = JoinKind::Butt;

        // Override with Auto — should fall through to the wall's own Butt.
        let mut ov = JoinOverrides::new();
        ov.set(ArchId(0), JoinKind::Auto, JoinKind::Auto);

        let shapes = solve_joins_with(&model, &ov);
        assert_eq!(shapes.len(), 2);
        let s0 = &shapes[0];
        assert!(s0.joins.iter().any(|j| matches!(j, Join::Butt(End::End))), "Auto override must fall through to wall's own Butt");
        assert!(!s0.joins.iter().any(|j| matches!(j, Join::L(End::End))), "must not mitre when wall says Butt");
    }

    #[test]
    fn t_junction_honours_walls_own_join_kind() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 1.0); // host
        wall(&mut model, 1, Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0), 1.0); // incoming

        // Incoming wall says Butt instead of T.
        model.walls[1].join_end = JoinKind::Butt;

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let incoming = &shapes[1];
        assert!(incoming.joins.iter().any(|j| matches!(j, Join::Butt(End::End))), "incoming wall should Butt, not T");
        assert!(!incoming.joins.iter().any(|j| matches!(j, Join::T(_))), "must not T when wall says Butt");
    }

    #[test]
    fn classify_with_prefilter_matches_the_full_scan() {
        let mut model = cadcraft_doc::ArchModel::default();

        // L join pair.
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0);
        // T join (stem into a straight host).
        wall(&mut model, 10, Vec2::new(0.0, 5.0), Vec2::new(20.0, 5.0), 1.0);
        wall(&mut model, 11, Vec2::new(10.0, 10.0), Vec2::new(10.0, 5.0), 1.0);
        // X crossing.
        wall(&mut model, 20, Vec2::new(30.0, 0.0), Vec2::new(40.0, 0.0), 1.0);
        wall(&mut model, 21, Vec2::new(35.0, -3.0), Vec2::new(35.0, 3.0), 1.0);
        // Parallel pair.
        wall(&mut model, 30, Vec2::new(50.0, 0.0), Vec2::new(60.0, 0.0), 1.0);
        wall(&mut model, 31, Vec2::new(50.0, 1.0), Vec2::new(60.0, 1.0), 1.0);
        // Collinear pair.
        wall(&mut model, 40, Vec2::new(70.0, 0.0), Vec2::new(75.0, 0.0), 1.0);
        wall(&mut model, 41, Vec2::new(75.0, 0.0), Vec2::new(80.0, 0.0), 1.0);
        // 200 far-away walls that no join scan should ever touch.
        for i in 0..200 {
            let x = 1000.0 + (i as f64) * 2.0;
            wall(&mut model, 100 + i, Vec2::new(x, 2000.0), Vec2::new(x + 2.0, 2000.0), 1.0);
        }

        let graph = WallGraph::build(&model);

        // Every end: prefilted scan must match the full-model scan.
        for w in &model.walls {
            for end in [End::Start, End::End] {
                let pref = classify(&model, &graph, w.id, end);
                let full = classify_full(&model, &graph, w.id, end);
                assert_eq!(pref, full, "prefilter lost a neighbour for wall {:?} end {:?}", w.id, end);
            }
        }

        // solve_joins output is identical with and without the prefilter.
        assert_eq!(solve_joins(&model), solve_joins_full(&model), "solve_joins output changed with the prefilter");
    }

    #[test]
    fn solve_joins_with_zero_thickness_wall() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 0, "zero thickness wall should produce no shape");
    }

    #[test]
    fn solve_joins_with_nan_coordinates() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(f64::NAN, 0.0), Vec2::new(10.0, 0.0), 1.0);
        wall(&mut model, 1, Vec2::new(0.0, 0.0), Vec2::new(10.0, f64::NAN), 1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 0, "walls with NaN coordinates should produce no shapes");
    }

    #[test]
    fn solve_joins_with_negative_thickness() {
        let mut model = cadcraft_doc::ArchModel::default();
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), -1.0);

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 0, "negative thickness wall should produce no shape");
    }

    #[test]
    fn stretch_wall_with_no_neighbours() {
        let mut model = cadcraft_doc::ArchModel::default();
        make_wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));

        let result = stretch_wall(&mut model, ArchId(0), End::End, Vec2::new(12.0, 3.0));
        let ids: Vec<ArchId> = result.unwrap();
        assert_eq!(ids, vec![ArchId(0)]);
        assert_eq!(model.walls[0].end, Vec2::new(12.0, 3.0));
    }

    #[test]
    fn prefilter_with_zero_length_wall_being_classified() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Wall 0: zero-length at (0,0), thickness 1.0
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), 1.0);
        // Wall 1: (10,0)-(20,0), thickness 1.0 (far away)
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), 1.0);

        let graph = WallGraph::build(&model);

        // Wall 0 is a point at (0,0). Wall 1 is far away.
        // The prefilter should exclude wall 1.
        let pref = classify(&model, &graph, ArchId(0), End::Start);
        let full = classify_full(&model, &graph, ArchId(0), End::Start);
        assert_eq!(pref, full, "prefilter and full scan disagree for zero-length wall");
        // No neighbours expected.
        assert!(pref.is_empty());
    }

    #[test]
    fn prefilter_with_zero_length_candidate() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Wall 0: (0,0)-(10,0), thickness 1.0
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        // Wall 1: zero-length at (5,0), thickness 1.0 (on wall 0's centre line)
        wall(&mut model, 1, Vec2::new(5.0, 0.0), Vec2::new(5.0, 0.0), 1.0);

        let graph = WallGraph::build(&model);

        // Wall 1 is a point at (5,0). Wall 0's centre line passes through (5,0).
        // So wall 0 should be a Mid neighbour of wall 1.
        let pref = classify(&model, &graph, ArchId(1), End::Start);
        let full = classify_full(&model, &graph, ArchId(1), End::Start);
        assert_eq!(pref, full, "prefilter and full scan disagree for zero-length wall");
        // Wall 0 should be a Mid neighbour.
        assert!(pref.iter().any(|n| matches!(n, Neighbour::Mid { id, .. } if *id == ArchId(0))));
    }

    #[test]
    fn prefilter_with_nan_in_wall_being_classified() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Wall 0: NaN in start, thickness 1.0
        wall(&mut model, 0, Vec2::new(f64::NAN, 0.0), Vec2::new(10.0, 0.0), 1.0);
        // Wall 1: (10,0)-(20,0), thickness 1.0
        wall(&mut model, 1, Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), 1.0);

        let graph = WallGraph::build(&model);

        // Wall 0 has NaN in its start. The prefilter should not crash.
        // The grown box of wall 0 has NaN corners, so no candidates are found.
        let pref = classify(&model, &graph, ArchId(0), End::End);
        let full = classify_full(&model, &graph, ArchId(0), End::End);
        assert_eq!(pref, full, "prefilter and full scan disagree for NaN wall");
    }

    #[test]
    fn prefilter_with_large_coordinates() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Wall 0: (0,0)-(10,0), thickness 1.0
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        // Wall 1: (1e15, 0)-(1e15 + 10, 0), thickness 1.0 (far away)
        wall(&mut model, 1, Vec2::new(1e15, 0.0), Vec2::new(1e15 + 10.0, 0.0), 1.0);

        let graph = WallGraph::build(&model);

        // Wall 1 is far away. The prefilter should exclude it.
        let pref = classify(&model, &graph, ArchId(0), End::End);
        let full = classify_full(&model, &graph, ArchId(0), End::End);
        assert_eq!(pref, full, "prefilter and full scan disagree for large coordinates");
        // No neighbours expected.
        assert!(pref.is_empty());
    }

    #[test]
    fn prefilter_with_wall_just_inside_and_outside_reach() {
        let mut model = cadcraft_doc::ArchModel::default();
        // Wall 0: (0,0)-(10,0), thickness 1.0
        wall(&mut model, 0, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0);
        // Wall 1: at x=12, just inside the reach (reach = 2.0 + 1e-6)
        wall(&mut model, 1, Vec2::new(12.0, 0.0), Vec2::new(12.0, 1.0), 1.0);
        // Wall 2: at x=13, just outside the reach
        wall(&mut model, 2, Vec2::new(13.0, 0.0), Vec2::new(13.0, 1.0), 1.0);

        let graph = WallGraph::build(&model);

        // Wall 0's End::End is at (10,0).
        // Wall 1 is at x=12, which is 2 units away. Reach is 2.0 + 1e-6, so wall 1 is inside.
        // Wall 2 is at x=13, which is 3 units away. Wall 2 is outside.
        // Neither wall 1 nor wall 2 is a neighbour of wall 0.
        let pref = classify(&model, &graph, ArchId(0), End::End);
        let full = classify_full(&model, &graph, ArchId(0), End::End);
        assert_eq!(pref, full, "prefilter and full scan disagree");
    }
}
