use std::collections::HashSet;

use cadcraft_doc::{ArchId, ArchModel, Wall};
use cadcraft_geom::{Bounds2, Vec2};

use crate::graph::WallGraph;

pub const MAX_WALLS: usize = 4096;
/// Max wall ends allowed to share one join point before we stop mitring.
pub const MAX_JOIN_FAN: usize = 8;
/// Endpoint clustering tolerance (world units).
pub const ENDPOINT_TOL: f64 = 1e-6;
/// Generic geometric epsilon.
pub const JOIN_EPS: f64 = 1e-9;

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

/// A neighbour relationship at one wall end, used by `classify` to pick a join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Neighbour {
    /// A second wall whose own end is also at this point (end-to-end join).
    EndToEnd { id: ArchId, end: End },
    /// A wall whose centre-line interior passes within `ENDPOINT_TOL` of this point.
    /// The current wall is the incoming wall; the neighbour is the host.
    Mid { id: ArchId, end: End },
    /// A wall collinear with the current wall.
    Collinear { id: ArchId },
    /// Cluster exceeds `MAX_JOIN_FAN`; treated as "no join".
    Fan { count: usize },
}

/// Corner plan for one end after joins.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EndPlan {
    left: Vec2,
    right: Vec2,
    join: Option<Join>,
}

/// Distance from `p` to the infinite line through `w`'s centre line.
fn dist_to_centre_line(w: &Wall, p: Vec2) -> f64 {
    let d = w.end - w.start;
    let len2 = d.len2();
    if len2 < 1e-24 || !len2.is_finite() {
        return p.dist(w.start);
    }
    (p - w.start).cross(d).abs() / len2.sqrt()
}

/// Whether the interior of `w`'s centre line passes within `tol` of `p`.
fn centre_interior_passes(w: &Wall, p: Vec2, tol: f64) -> bool {
    if !w.start.is_finite() || !w.end.is_finite() || !p.is_finite() {
        return false;
    }
    let d = w.end - w.start;
    let len2 = d.len2();
    if len2 < 1e-24 {
        return w.start.near(p, tol);
    }
    let t = (p - w.start).dot(d) / len2;
    if t < 1e-6 || t > 1.0 - 1e-6 {
        return false;
    }
    let foot = w.start + d * t;
    foot.near(p, tol)
}

/// Classify neighbours at `(id, end)`.
fn classify(model: &ArchModel, g: &WallGraph, id: ArchId, end: End) -> Vec<Neighbour> {
    let nbrs = g.neighbours(id, end);
    let total = nbrs.len() + 1;
    if total > MAX_JOIN_FAN {
        return vec![Neighbour::Fan { count: total }];
    }
    if nbrs.is_empty() {
        return Vec::new();
    }
    let Some(my_wall) = model.wall(id) else {
        return Vec::new();
    };
    let my_point = End::point(end, my_wall);
    if !my_point.is_finite() {
        return Vec::new();
    }
    let my_dir = crate::wall::wall_dir(my_wall);
    if !my_dir.is_finite() {
        return Vec::new();
    }

    let mut out: Vec<Neighbour> = Vec::new();
    for (nid, nend) in nbrs {
        if nid == id {
            continue;
        }
        let Some(nw) = model.wall(nid) else {
            continue;
        };
        let nbr_dir = crate::wall::wall_dir(nw);
        if !nbr_dir.is_finite() {
            continue;
        }
        let parallel = my_dir.cross(nbr_dir).abs() < 1e-9;
        if parallel {
            if dist_to_centre_line(nw, my_point) < ENDPOINT_TOL {
                out.push(Neighbour::Collinear { id: nid });
            }
        } else {
            let nbr_point = End::point(nend, nw);
            if nbr_point.near(my_point, ENDPOINT_TOL) {
                out.push(Neighbour::EndToEnd { id: nid, end: nend });
            } else if centre_interior_passes(nw, my_point, ENDPOINT_TOL) {
                out.push(Neighbour::Mid { id: nid, end: nend });
            }
        }
    }
    out
}

/// Intersect two line segments as infinite lines; return the point if finite.
fn intersect_inf(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> Option<Vec2> {
    let p = cadcraft_geom::intersect::line_line_infinite(a.0, a.1, b.0, b.1)?;
    (p.is_finite()).then_some(p)
}

/// Corner plan for one end of `w`, given the raw corners and classified neighbours.
fn plan_end(model: &ArchModel, w: &Wall, end: End, raw: (Vec2, Vec2), nbrs: &[Neighbour]) -> EndPlan {
    // Fan → no join.
    if nbrs.iter().any(|n| matches!(n, Neighbour::Fan { .. })) {
        return EndPlan { left: raw.0, right: raw.1, join: None };
    }

    // T join (Mid): trim the incoming wall's end onto the host's face.
    if let Some(Neighbour::Mid { id: nid, .. }) = nbrs.iter().find(|n| matches!(n, Neighbour::Mid { .. })) {
        let Some(host) = model.wall(nid) else {
            return EndPlan { left: raw.0, right: raw.1, join: None };
        };
        let Some(host_faces) = face_lines(host) else {
            return EndPlan { left: raw.0, right: raw.1, join: None };
        };
        let dir = w.end - w.start;
        let len2 = dir.len2();
        if len2 < 1e-24 || !len2.is_finite() {
            return EndPlan { left: raw.0, right: raw.1, join: None };
        }
        let perp = (dir / dir.len()).perp();
        let half = w.thickness / 2.0;
        for face in [host_faces[0], host_faces[1]] {
            if let Some(p) = intersect_inf((w.start, w.end), face) {
                if p.is_finite() {
                    let left_c = p + perp * half;
                    let right_c = p - perp * half;
                    if left_c.is_finite() && right_c.is_finite() {
                        return EndPlan { left: left_c, right: right_c, join: Some(Join::T(end)) };
                    }
                }
            }
        }
    }

    // L join (EndToEnd): mitre using cross-pairing of face lines.
    if let Some(Neighbour::EndToEnd { id: nid, .. }) = nbrs.iter().find(|n| matches!(n, Neighbour::EndToEnd { .. })) {
        let Some(neig) = model.wall(nid) else {
            return EndPlan { left: raw.0, right: raw.1, join: None };
        };
        if let (Some(my_faces), Some(nbr_faces)) = (face_lines(w), face_lines(neig)) {
            // Cross-pairing: my left ↔ nbr right, my right ↔ nbr left.
            let new_left = intersect_inf(my_faces[0], nbr_faces[1]).unwrap_or(raw.0);
            let new_right = intersect_inf(my_faces[1], nbr_faces[0]).unwrap_or(raw.1);
            if new_left.is_finite() && new_right.is_finite() {
                return EndPlan { left: new_left, right: new_right, join: Some(Join::L(end)) };
            }
        }
    }

    // No join.
    EndPlan { left: raw.0, right: raw.1, join: None }
}

/// Solve every wall's poché ring. Output order == `model.walls` order; degenerate
/// walls (wall_polygon is None) are skipped and produce no shape.
pub fn solve_joins(model: &ArchModel) -> Vec<WallShape> {
    let g = crate::graph::WallGraph::build(model);
    let mut shapes: Vec<WallShape> = Vec::new();

    for w in &model.walls {
        if crate::wall::wall_polygon(w).is_none() {
            continue;
        }
        let Some(my_faces) = face_lines(w) else {
            continue;
        };
        let raw_start = (my_faces[0].0, my_faces[1].0);
        let raw_end = (my_faces[0].1, my_faces[1].1);

        let nbrs_start = classify(model, &g, w.id, End::Start);
        let nbrs_end = classify(model, &g, w.id, End::End);

        // Compute base plans.
        let mut plan_start = plan_end(model, w, End::Start, raw_start, &nbrs_start);
        let mut plan_end_res = plan_end(model, w, End::End, raw_end, &nbrs_end);

        // Collinear butt: apply to the wall that starts at the shared point.
        // For each collinear neighbour, determine which wall is "trimmed".
        for n in &nbrs_start {
            if let Neighbour::Collinear { id: nid } = n {
                if let Some(other) = model.wall(nid) {
                    // The trimmed wall is the one whose Start is the shared point.
                    // i.e., this wall's Start == other's End → this wall is trimmed.
                    if End::point(End::Start, w).near(other.end, ENDPOINT_TOL) || End::point(End::End, w).near(other.start, ENDPOINT_TOL) {
                        // Determine trim direction
                        let shared = End::point(End::Start, w);
                        let other_endpoint = if shared.near(other.end, ENDPOINT_TOL) {
                            other.end
                        } else if shared.near(other.start, ENDPOINT_TOL) {
                            other.start
                        } else {
                            continue;
                        };
                        let dir = other.end - other.start;
                        let len2 = dir.len2();
                        if len2 < 1e-24 {
                            continue;
                        }
                        let perp = (dir / dir.len()).perp();
                        let half = other.thickness / 2.0;
                        let left_c = other_endpoint + perp * half;
                        let right_c = other_endpoint - perp * half;
                        if left_c.is_finite() && right_c.is_finite() {
                            plan_start = EndPlan { left: left_c, right: right_c, join: Some(Join::Butt(End::Start)) };
                        }
                    }
                }
            }
        }
        for n in &nbrs_end {
            if let Neighbour::Collinear { id: nid } = n {
                if let Some(other) = model.wall(nid) {
                    let shared = End::point(End::End, w);
                    let other_endpoint = if shared.near(other.end, ENDPOINT_TOL) {
                        other.end
                    } else if shared.near(other.start, ENDPOINT_TOL) {
                        other.start
                    } else {
                        continue;
                    };
                    let dir = other.end - other.start;
                    let len2 = dir.len2();
                    if len2 < 1e-24 {
                        continue;
                    }
                    let perp = (dir / dir.len()).perp();
                    let half = other.thickness / 2.0;
                    let left_c = other_endpoint + perp * half;
                    let right_c = other_endpoint - perp * half;
                    if left_c.is_finite() && right_c.is_finite() {
                        plan_end_res = EndPlan { left: left_c, right: right_c, join: Some(Join::Butt(End::End)) };
                    }
                }
            }
        }

        let ring = ring_from_corners(plan_start.left, plan_start.right, plan_end_res.left, plan_end_res.right);
        let (polygon, joins) = match ring {
            Some(p) => {
                let mut js: Vec<Join> = Vec::new();
                if let Some(j) = plan_start.join {
                    js.push(j);
                }
                if let Some(j) = plan_end_res.join {
                    js.push(j);
                }
                (p, js)
            }
            None => {
                // Ring not simple: fall back to raw corners, drop joins.
                match ring_from_corners(raw_start.0, raw_start.1, raw_end.0, raw_end.1) {
                    Some(fb) => (fb, Vec::new()),
                    None => {
                        continue; // Degenerate: skip this wall entirely
                    }
                }
            }
        };

        shapes.push(WallShape { id: w.id, polygon, centre: (w.start, w.end), joins });
    }

    shapes
}

fn polygon_area(pts: &[Vec2]) -> f64 {
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

    #[test]
    fn l_join_mitres_both_walls_to_the_shared_corner() {
        let mut model = ArchModel::default();
        model.add_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0));
        model.add_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 1.0));
        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        // Wall 0: end at (10,0) → Join::L(End)
        // Wall 1: start at (10,0) → Join::L(Start)
        let s0 = &shapes[0];
        let s1 = &shapes[1];

        let end_join = s0.joins.iter().find(|j| matches!(j, Join::L(e) if *e == End::End));
        let start_join = s1.joins.iter().find(|j| matches!(j, Join::L(e) if *e == End::Start));
        assert!(end_join.is_some(), "wall 0 end should be L");
        assert!(start_join.is_some(), "wall 1 start should be L");

        // Find the shared mitre corner on the bisector x - y = 10
        // A's End left corner should be (10.5, 0.5): 10.5 - 0.5 = 10 ✓
        // A's End right corner should be (9.5, -0.5): 9.5 - (-0.5) = 10 ✓
        // Check wall 0's End corners
        let a_end_left = s0.polygon[2]; // left_end
        let a_end_right = s0.polygon[1]; // right_end

        // The bisector check: x - y should be 10 (within 1e-9)
        let bis_a_left = (a_end_left.x - a_end_left.y - 10.0).abs();
        let bis_a_right = (a_end_right.x - a_end_right.y - 10.0).abs();
        assert!(
            bis_a_left < 1e-9 || bis_a_right < 1e-9,
            "at least one mitre corner should be on bisector x-y=10, got: ({:.6},{:.6}) / ({:.6},{:.6})",
            a_end_left.x,
            a_end_left.y,
            a_end_right.x,
            a_end_right.y
        );

        // The shared corner between the two rings
        // A's End left should equal B's Start left
        // A's End right should equal B's Start right
        let b_start_left = s1.polygon[0]; // left_start
        let b_start_right = s1.polygon[1]; // right_start

        assert!(a_end_left.near(b_start_left, 1e-9) || a_end_right.near(b_start_right, 1e-9), "rings should share a corner");

        // Raw square corner (10, 0.5) should NOT be in either ring
        let raw_corner = Vec2::new(10.0, 0.5);
        assert!(!s0.polygon.iter().any(|p| p.near(raw_corner, 1e-9)), "wall 0 should not retain raw corner (10, 0.5)");
        assert!(!s1.polygon.iter().any(|p| p.near(raw_corner, 1e-9)), "wall 1 should not retain raw corner (10, 0.5)");
    }

    #[test]
    fn t_join_butts_the_incoming_wall_at_the_face() {
        let mut model = ArchModel::default();
        // Host: (0,0)→(20,0), t=1
        model.add_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), 1.0));
        // Incoming: (10,10)→(10,0), t=1
        model.add_wall(Wall::new(Vec2::new(10.0, 10.0), Vec2::new(10.0, 0.0), 1.0));
        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let host = &shapes[0];
        let incoming = &shapes[1];

        // Incoming should have Join::T(End)
        let t_join = incoming.joins.iter().find(|j| matches!(j, Join::T(e) if *e == End::End));
        assert!(t_join.is_some(), "incoming should have Join::T(End)");

        // Incoming's end face should lie on y=0.5
        let end_left = incoming.polygon[2];
        let end_right = incoming.polygon[1];
        assert!((end_left.y - 0.5).abs() < 1e-9, "incoming end left y={:.9} should be 0.5", end_left.y);
        assert!((end_right.y - 0.5).abs() < 1e-9, "incoming end right y={:.9} should be 0.5", end_right.y);

        // Host polygon unchanged: bounds span y in [-0.5, 0.5]
        let hb = host.bounds();
        assert!((hb.min.y - (-0.5)).abs() < 1e-9, "host min.y={}", hb.min.y);
        assert!((hb.max.y - 0.5).abs() < 1e-9, "host max.y={}", hb.max.y);
    }

    #[test]
    fn angled_join_mitres_at_the_bisector() {
        // 60° join: wall A at 0° (east), wall B at 60° (northeast)
        let angle = std::f64::consts::FRAC_PI_3; // 60° = π/3
        let mut model = ArchModel::default();

        // Wall A: (0,0)→(10,0), dir = (1,0), angle 0°
        model.add_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0));
        // Wall B: (10,0)→(10+cos60, sin60) = (15, 5.196...), dir at 60°
        let b_end = Vec2::new(10.0 + 10.0 * angle.cos(), 0.0 + 10.0 * angle.sin());
        model.add_wall(Wall::new(Vec2::new(10.0, 0.0), b_end, 1.0));

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);
        let s0 = &shapes[0];
        let s1 = &shapes[1];

        // Both should have L join at the shared end
        let a_end_join = s0.joins.iter().find(|j| matches!(j, Join::L(e) if *e == End::End));
        let b_start_join = s1.joins.iter().find(|j| matches!(j, Join::L(e) if *e == End::Start));
        assert!(a_end_join.is_some(), "wall A should have L(End)");
        assert!(b_start_join.is_some(), "wall B should have L(Start)");

        // The bisector of 0° and 60° is at 30°.
        // Bisector direction: (cos30, sin30) = (√3/2, 0.5)
        // Bisector line through (10,0): parametric (10 + t·cos30, 0 + t·sin30)
        let bis_dir = Vec2::new(std::f64::consts::FRAC_PI_6.cos(), std::f64::consts::FRAC_PI_6.sin());
        let bis_origin = Vec2::new(10.0, 0.0);

        // Check A's End corners are on the bisector (within 1e-9)
        // For the mitre, the corners lie on the bisector line.
        // The distance from each corner to the bisector line should be ≈ 0.
        // Distance from point p to line through origin with direction d:
        //   |p × d| / |d|  (perpendicular distance)
        let check_corner = |corner: &Vec2| -> bool {
            // Distance from corner to the bisector line
            let v = *corner - bis_origin;
            let cross = v.cross(bis_dir);
            (cross.abs() / bis_dir.len()) < 1e-6
        };

        // A's End: polygon[1] (right_end) and polygon[2] (left_end)
        let a_corners = [s0.polygon[1], s0.polygon[2]];
        let b_corners = [s1.polygon[0], s1.polygon[1]];

        // At least one of A's End corners should be on the bisector
        assert!(a_corners.iter().any(|c| check_corner(c)), "wall A mitre corner should be on bisector of 0°/60°");
        // At least one of B's Start corners should be on the bisector
        assert!(b_corners.iter().any(|c| check_corner(c)), "wall B mitre corner should be on bisector of 0°/60°");
    }

    #[test]
    fn collinear_walls_butt_and_do_not_overlap() {
        let mut model = ArchModel::default();
        // Wall A: (0,0)→(10,0), t=1
        model.add_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 1.0));
        // Wall B: (10,0)→(20,0), t=1
        model.add_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), 1.0));

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 2);

        let a = &shapes[0];
        let b = &shapes[1];

        // Each polygon area should be 10.0 (length 10 × thickness 1)
        let area_a = polygon_area(&a.polygon);
        let area_b = polygon_area(&b.polygon);
        assert!((area_a - 10.0).abs() < 1e-9, "wall A area={}", area_a);
        assert!((area_b - 10.0).abs() < 1e-9, "wall B area={}", area_b);

        // Sum of areas = 20.0 (no double-covered strip)
        let sum = area_a + area_b;
        assert!((sum - 20.0).abs() < 1e-9, "sum of areas={}", sum);
    }

    #[test]
    fn closed_rectangle_ring_is_gap_free() {
        let t = 0.3;
        let half = t / 2.0;
        let mut model = ArchModel::default();
        // Bottom: (0,0)→(10,0)
        model.add_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), t));
        // Right: (10,0)→(10,8)
        model.add_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), t));
        // Top: (10,8)→(0,8)
        model.add_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), t));
        // Left: (0,8)→(0,0)
        model.add_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), t));

        let shapes = solve_joins(&model);
        assert_eq!(shapes.len(), 4);

        // Every shape has two Join::L
        for s in &shapes {
            let l_count = s.joins.iter().filter(|j| matches!(j, Join::L(_))).count();
            assert_eq!(l_count, 2, "each rectangle wall should have 2 L joins, got {}", l_count);
        }

        // Check corner points of the outer rectangle appear in exactly 2 shapes
        // Outer corners: (0,0), (10,0), (10,8), (0,8)
        // The "corner" of each wall's ring at the joint should be at specific points.

        // The outer rectangle corners in terms of wall rings:
        // Bottom wall's Start: at (0,0) → corners at (0,-half) and (0,half)
        // Bottom wall's End: at (10,0) → mitred
        // Right wall's Start: at (10,0) → mitred
        // etc.

        // Instead, verify: each of the 4 outer corners (±half offsets) appears in exactly 2 polygons.
        // Outer corner (0, 0):
        //   - Bottom wall left face start: (0, -half) [bottom-left outer corner]
        //   - Left wall right face end: (0, -half) [same point]
        // Actually the outer corners of the rectangle are at:
        //   (−half, −half) is NOT a corner... let me think differently.

        // The outer boundary of the 4-wall rectangle:
        // Bottom wall: left=(0,-h), right=(0,h), end_left=(10,0±h mitred), ...
        // Actually, let's just check that the shared corners between adjacent walls match.

        let h = half;
        // The 4 outer "square" corners of the border:
        // These are the points where the outer face lines of adjacent walls meet.
        // For a proper closed rectangle with mitres, the outer corners should be shared.

        // Check that wall 0's End left corner == wall 1's Start left corner (shared at bottom-right)
        // Wall 0 (bottom): End left = polygon[2]
        // Wall 1 (right): Start left = polygon[0]
        assert!(shapes[0].polygon[2].near(shapes[1].polygon[0], 1e-9), "bottom-right outer corner not shared between wall 0 End and wall 1 Start");

        // Wall 1 (right): End left = polygon[2]
        // Wall 2 (top): Start left = polygon[0]
        assert!(shapes[1].polygon[2].near(shapes[2].polygon[0], 1e-9), "top-right outer corner not shared between wall 1 End and wall 2 Start");

        // Wall 2 (top): End left = polygon[2]
        // Wall 3 (left): Start left = polygon[0]
        assert!(shapes[2].polygon[2].near(shapes[3].polygon[0], 1e-9), "top-left outer corner not shared between wall 2 End and wall 3 Start");

        // Wall 3 (left): End left = polygon[2]
        // Wall 0 (bottom): Start left = polygon[0]
        assert!(shapes[3].polygon[2].near(shapes[0].polygon[0], 1e-9), "bottom-left outer corner not shared between wall 3 End and wall 0 Start");
    }
}
