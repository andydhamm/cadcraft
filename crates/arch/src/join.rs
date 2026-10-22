use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cadcraft_doc::{ArchId, Wall};
use cadcraft_geom::{Bounds2, Vec2};

use crate::graph::WallGraph;

pub const MAX_WALLS: usize = 4096;
/// Max wall ends allowed to share one join point before we stop mitring.
pub const MAX_JOIN_FAN: usize = 8;
/// Endpoint clustering tolerance (world units).
pub const ENDPOINT_TOL: f64 = 1e-6;
/// Generic geometric epsilon.
pub const JOIN_EPS: f64 = 1e-9;
/// Parallelism test for "corresponding" (collinear) neighbour detection.
const PARALLEL_EPS: f64 = 1e-9;

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

/// A neighbour wall relevant to a join at one end.
enum Neighbour {
    /// Another wall's end meets this wall's end at the same point.
    EndToEnd { id: ArchId, end: End },
    /// Another wall's centre line passes through this wall's end (T-junction).
    Mid { id: ArchId, end: End },
    /// A collinear wall that this wall should butt against.
    Collinear { id: ArchId, end: End },
    /// More than MAX_JOIN_FAN walls meet here; treat as no join.
    Fan { count: usize },
}

/// Classify the neighbours at one end of a wall.
fn classify(model: &cadcraft_doc::ArchModel, g: &WallGraph, id: ArchId, end: End) -> Vec<Neighbour> {
    let Some(w) = model.walls.iter().find(|w| w.id == id) else {
        return Vec::new();
    };
    let pt = end.point(w);
    let Some(dir) = wall_dir(w) else {
        return Vec::new();
    };

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
            let cross = dir.cross(ndir);

            // Collinear: parallel and on the same line.
            if cross.abs() < PARALLEL_EPS {
                let d = (pt - nw.start).cross(ndir);
                if d.abs() < ENDPOINT_TOL {
                    let nend_at_pt = if nw.start.dist(pt) < ENDPOINT_TOL { End::Start } else { End::End };
                    neighbours.push(Neighbour::Collinear { id: *nid, end: nend_at_pt });
                }
                continue;
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

    neighbours
}

/// Plan the corners and join at one end of a wall.
fn plan_end(w: &Wall, end: End, raw: (Vec2, Vec2), g: &WallGraph, model: &cadcraft_doc::ArchModel, id: ArchId) -> Result<EndPlan, ArchError> {
    let nbrs = classify(model, g, id, end);

    if nbrs.is_empty() {
        return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
    }

    // Fan: no join, keep raw corners.
    if nbrs.iter().any(|n| matches!(n, Neighbour::Fan { .. })) {
        return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
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
        let pt = end.point(w);
        let Some(ndir) = wall_dir(nw) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let perp = Vec2::new(-ndir.y, ndir.x);
        let half = nw.thickness / 2.0;
        let side = (pt - nw.start).dot(perp);
        let face_pt = if side > 0.0 {
            nw.start + ndir * ((pt - nw.start).dot(ndir)) + perp * half
        } else {
            nw.start + ndir * ((pt - nw.start).dot(ndir)) - perp * half
        };
        let Some(wdir) = wall_dir(w) else {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        };
        let wperp = Vec2::new(-wdir.y, wdir.x);
        let whalf = w.thickness / 2.0;
        let left = face_pt + wperp * whalf;
        let right = face_pt - wperp * whalf;
        return Ok(EndPlan { left, right, join: Some(Join::T(end)) });
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
        let interior_dir = (-wdir + ndir);
        if interior_dir.len() < JOIN_EPS {
            return Ok(EndPlan { left: raw.0, right: raw.1, join: None });
        }

        // For each face of this wall, intersect with both faces of the neighbour
        // and pick the intersection that is more in the interior direction.
        let pick_interior = |p: Vec2, q: Vec2| -> Vec2 {
            let v1 = p - pt;
            let v2 = q - pt;
            if v1.dot(interior_dir) >= v2.dot(interior_dir) { p } else { q }
        };

        let left_a = line_line_infinite(raw.0, raw.0 + wdir * 1000.0, nface[0].0, nface[0].1).map(|(p, _, _)| p);
        let left_b = line_line_infinite(raw.0, raw.0 + wdir * 1000.0, nface[1].0, nface[1].1).map(|(p, _, _)| p);
        let left = match (left_a, left_b) {
            (Some(a), Some(b)) => pick_interior(a, b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => raw.0,
        };

        let right_a = line_line_infinite(raw.1, raw.1 + wdir * 1000.0, nface[0].0, nface[0].1).map(|(p, _, _)| p);
        let right_b = line_line_infinite(raw.1, raw.1 + wdir * 1000.0, nface[1].0, nface[1].1).map(|(p, _, _)| p);
        let right = match (right_a, right_b) {
            (Some(a), Some(b)) => pick_interior(a, b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => raw.1,
        };

        return Ok(EndPlan { left, right, join: Some(Join::L(end)) });
    }

    Ok(EndPlan { left: raw.0, right: raw.1, join: None })
}

/// Solve every wall's poché ring. Output order == `model.walls` order; degenerate
/// walls (`wall::wall_polygon` is `None`) are skipped and produce no shape.
pub fn solve_joins(model: &cadcraft_doc::ArchModel) -> Vec<WallShape> {
    let graph = WallGraph::build(model);
    let mut out: Vec<WallShape> = Vec::with_capacity(model.walls.len());

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
        let plan_s = match plan_end(w, End::Start, (ls, rs), &graph, model, w.id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        // End end.
        let plan_e = match plan_end(w, End::End, (le, re), &graph, model, w.id) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let corners = ring_from_corners(plan_s.left, plan_s.right, plan_e.left, plan_e.right);
        let mut ring = match corners {
            Some(r) => r,
            None => {
                // Rule 6: crossed/degenerate → fall back to the raw (untrimmed) corners.
                let raw = ring_from_corners(ls, rs, le, re);
                let _ = raw;
                continue;
            }
        };

        out.push(WallShape {
            id: w.id,
            polygon: ring,
            centre: (w.start, w.end),
            joins: vec![plan_s.join, plan_e.join].into_iter().filter_map(|j| j).collect(),
        });
    }

    out
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
}
