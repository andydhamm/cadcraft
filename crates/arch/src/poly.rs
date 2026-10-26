//! Pure 2D polygon helpers for arch geometry.

use cadcraft_geom::Vec2;

use crate::join::polygon_area;

/// Absolute area of a closed polygon ring.
///
/// Returns `0.0` for degenerate rings (`< 3` points) or rings containing a non-finite point.
pub fn ring_area(pts: &[Vec2]) -> f64 {
    if pts.len() < 3 || pts.iter().any(|p| !p.is_finite()) {
        return 0.0;
    }
    polygon_area(pts).abs()
}

/// Perimeter of a closed polygon ring.
///
/// The ring is closed implicitly: the last point connects back to the first. Returns `0.0` for
/// degenerate rings (`< 2` points) or rings containing a non-finite point.
pub fn ring_perimeter(pts: &[Vec2]) -> f64 {
    let n = pts.len();
    if n < 2 || pts.iter().any(|p| !p.is_finite()) {
        return 0.0;
    }

    let mut total = 0.0;
    for i in 0..n {
        let Some(a) = pts.get(i) else {
            return 0.0;
        };
        let Some(b) = pts.get((i + 1) % n) else {
            return 0.0;
        };
        total += a.dist(*b);
    }
    total
}

/// Area of a polygonal region: outer ring area minus the sum of hole areas.
///
/// The result is clamped to `0.0`.
pub fn region_area(outer: &[Vec2], holes: &[Vec<Vec2>]) -> f64 {
    let mut area = ring_area(outer);
    for hole in holes {
        area -= ring_area(hole);
    }
    area.max(0.0)
}

/// Perimeter of a polygonal region: outer ring perimeter plus every hole perimeter.
pub fn region_perimeter(outer: &[Vec2], holes: &[Vec<Vec2>]) -> f64 {
    let mut total = ring_perimeter(outer);
    for hole in holes {
        total += ring_perimeter(hole);
    }
    total
}

/// Return `pts` oriented counter-clockwise.
///
/// If the signed area is already non-negative, the ring is returned unchanged; otherwise it is
/// reversed.
pub fn ensure_ccw(pts: &[Vec2]) -> Vec<Vec2> {
    if polygon_area(pts) >= 0.0 {
        pts.to_vec()
    } else {
        let mut out = pts.to_vec();
        out.reverse();
        out
    }
}

/// Distance from point `p` to the closed segment `a -> b`.
///
/// Returns `f64::MAX` for non-finite inputs or degenerate segments.
pub fn point_segment_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    if !p.is_finite() || !a.is_finite() || !b.is_finite() {
        return f64::MAX;
    }

    let ab = Vec2::new(b.x - a.x, b.y - a.y);
    let len2 = ab.dot(ab);
    if len2 < 1e-24 || !len2.is_finite() {
        return f64::MAX;
    }

    let pa = Vec2::new(p.x - a.x, p.y - a.y);
    let t = pa.dot(ab) / len2;
    if !t.is_finite() {
        return f64::MAX;
    }
    let t = t.clamp(0.0, 1.0);

    let closest = Vec2::new(a.x + ab.x * t, a.y + ab.y * t);
    closest.dist(p)
}

/// Tolerance used by `segs_cross` for floating-point comparisons.
const TOL: f64 = 1e-12;

/// Minimum squared length used to treat a segment as degenerate.
const MIN_LEN2: f64 = 1e-24;

/// Whether the closed segments `a1 -> a2` and `b1 -> b2` touch or cross.
///
/// Uses the signed cross-product test with `TOL` for robustness. Returns
/// `false` when any endpoint is non-finite.
fn segs_cross(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    if !a1.is_finite() || !a2.is_finite() || !b1.is_finite() || !b2.is_finite() {
        return false;
    }

    let ab = Vec2::new(a2.x - a1.x, a2.y - a1.y);
    let cd = Vec2::new(b2.x - b1.x, b2.y - b1.y);

    let d1 = ab.cross(Vec2::new(b1.x - a1.x, b1.y - a1.y));
    let d2 = ab.cross(Vec2::new(b2.x - a1.x, b2.y - a1.y));
    let d3 = cd.cross(Vec2::new(a1.x - b1.x, a1.y - b1.y));
    let d4 = cd.cross(Vec2::new(a2.x - b1.x, a2.y - b1.y));

    let straddle = |d: f64, e: f64| (d > TOL && e < -TOL) || (d < -TOL && e > TOL);

    // Proper crossing: both pairs of signed distances straddle zero.
    if straddle(d1, d2) && straddle(d3, d4) {
        return true;
    }

    // Collinear overlap: all four cross-products are within tolerance.
    // Project both segments onto the longer axis of `ab` and check for overlap.
    if d1.abs() <= TOL && d2.abs() <= TOL && d3.abs() <= TOL && d4.abs() <= TOL {
        let (lo1, hi1): (f64, f64) = if ab.x.abs() >= ab.y.abs() { (a1.x.min(a2.x), a1.x.max(a2.x)) } else { (a1.y.min(a2.y), a1.y.max(a2.y)) };
        let (lo2, hi2): (f64, f64) = if ab.x.abs() >= ab.y.abs() { (b1.x.min(b2.x), b1.x.max(b2.x)) } else { (b1.y.min(b2.y), b1.y.max(b2.y)) };
        if hi1.min(hi2) - lo1.max(lo2) > TOL {
            return true;
        }
    }

    // Endpoint touching: an endpoint within tolerance of the other segment's
    // supporting line AND inside the other segment's bounding box (with `TOL`
    // slack on both axes).
    if d1.abs() <= TOL && in_bbox(b1, a1, a2) {
        return true;
    }
    if d2.abs() <= TOL && in_bbox(b2, a1, a2) {
        return true;
    }
    if d3.abs() <= TOL && in_bbox(a1, b1, b2) {
        return true;
    }
    if d4.abs() <= TOL && in_bbox(a2, b1, b2) {
        return true;
    }

    false
}

/// Whether `p` is inside the axis-aligned bounding box of the segment
/// `a -> b` (with `TOL` slack on both axes).
fn in_bbox(p: Vec2, a: Vec2, b: Vec2) -> bool {
    p.x >= a.x.min(b.x) - TOL && p.x <= a.x.max(b.x) + TOL && p.y >= a.y.min(b.y) - TOL && p.y <= a.y.max(b.y) + TOL
}

/// Whether `pts` form a simple (non-self-intersecting) closed polygon ring.
///
/// Returns `false` when `pts` has fewer than three points, when any point is
/// non-finite, when the enclosed area is at most `TOL`, or when any pair of
/// non-adjacent edges crosses. Edge `i` is `pts[i] -> pts[(i + 1) % n]`.
pub fn is_simple(pts: &[Vec2]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    if pts.iter().any(|p| !p.is_finite()) {
        return false;
    }
    if ring_area(pts) <= TOL {
        return false;
    }
    for i in 0..n {
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segs_cross(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]) {
                return false;
            }
        }
    }
    true
}

/// Per-edge offset of a closed polygon ring, with miter re-intersection.
///
/// Each edge `i` (`ccw[i] -> ccw[(i + 1) % n]`) is offset by `dist[i]` along its normal.
/// The offset is outward when `outward` is true (for a CCW ring, outward is the left-hand
/// side of the traversal, i.e. the `-dir.perp()` side). Consecutive offset lines are
/// intersected to form the new vertices; when two consecutive lines are parallel or
/// collinear (the common case after collinear merging) the start point of the offset
/// line is used instead.
///
/// Returns `None` when the ring is degenerate (`< 3` points), when `dist` does not match
/// the ring length, when any distance is non-finite or negative, or when the resulting
/// ring is not simple (self-intersecting) or contains a non-finite vertex.
pub fn offset_polygon(pts: &[Vec2], dist: &[f64], outward: bool) -> Option<Vec<Vec2>> {
    let ccw = ensure_ccw(pts);
    let n = ccw.len();
    if n < 3 || dist.len() != n {
        return None;
    }

    let mut lines: Vec<(Vec2, Vec2)> = Vec::with_capacity(n);
    for i in 0..n {
        let a = ccw[i];
        let b = ccw[(i + 1) % n];
        let ab = Vec2::new(b.x - a.x, b.y - a.y);
        let len2 = ab.dot(ab);
        if len2 < MIN_LEN2 {
            return None;
        }
        let len = len2.sqrt();
        let dir = Vec2::new(ab.x / len, ab.y / len);
        if !dir.is_finite() {
            return None;
        }
        let d = dist[i];
        if !d.is_finite() || d < 0.0 {
            return None;
        }
        let ni = if outward { -dir.perp() } else { dir.perp() };
        let p = Vec2::new(a.x + ni.x * d, a.y + ni.y * d);
        lines.push((p, dir));
    }

    let mut out: Vec<Vec2> = Vec::with_capacity(n);
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let vertex = line_line(lines[prev].0, lines[prev].1, lines[i].0, lines[i].1).unwrap_or(lines[i].0);
        if !vertex.is_finite() {
            return None;
        }
        out.push(vertex);
    }

    if !is_simple(&out) {
        return None;
    }
    Some(out)
}

/// Intersection of the two infinite lines `(p, r)` and `(q, s)`.
///
/// Returns `None` when the lines are parallel (cross-product of direction vectors is within
/// `TOL`) or when the intersection point is non-finite.
fn line_line(p: Vec2, r: Vec2, q: Vec2, s: Vec2) -> Option<Vec2> {
    let den = r.cross(s);
    if den.abs() < TOL {
        return None;
    }
    let t = Vec2::new(q.x - p.x, q.y - p.y).cross(s) / den;
    let x = p.x + r.x * t;
    let y = p.y + r.y * t;
    if x.is_finite() && y.is_finite() { Some(Vec2::new(x, y)) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::Vec2;

    fn sq(xs: &[(f64, f64)]) -> Vec<Vec2> {
        xs.iter().map(|&(x, y)| Vec2::new(x, y)).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn ring_area_of_unit_square() {
        let ccw = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let cw = sq(&[(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]);

        assert!(close(ring_area(&ccw), 1.0));
        assert!(close(ring_area(&cw), 1.0));
        assert!(close(ring_area(&[]), 0.0));
        assert!(close(ring_area(&[Vec2::ZERO, Vec2::X]), 0.0));

        let bad = vec![Vec2::ZERO, Vec2::X, Vec2::new(f64::NAN, 0.0)];
        assert!(close(ring_area(&bad), 0.0));
    }

    #[test]
    fn ring_perimeter_of_square() {
        let square = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);

        assert!(close(ring_perimeter(&square), 4.0));
        assert!(close(ring_perimeter(&[Vec2::ZERO, Vec2::X]), 2.0));
        assert!(close(ring_perimeter(&[]), 0.0));
    }

    #[test]
    fn region_area_subtracts_holes() {
        let outer = sq(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let holes = vec![sq(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)])];

        assert!(close(region_area(&outer, &holes), 96.0));
    }

    #[test]
    fn region_perimeter_adds_hole_rings() {
        let outer = sq(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let holes = vec![sq(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)])];

        assert!(close(region_perimeter(&outer, &holes), 48.0));
    }

    #[test]
    fn ensure_ccw_reverses_cw_square() {
        let ccw = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let cw = sq(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)]);

        let out = ensure_ccw(&cw);
        assert!(polygon_area(&out) > 0.0);
        assert_eq!(out.len(), 4);
        assert_eq!(out, vec![Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(0.0, 0.0),]);

        assert_eq!(ensure_ccw(&ccw), ccw);
        assert!(ensure_ccw(&[]).is_empty());
    }

    #[test]
    fn point_segment_dist_clamps_to_endpoints() {
        let a = Vec2::ZERO;
        let b = Vec2::X;

        assert!(close(point_segment_dist(Vec2::new(0.5, 2.0), a, b), 2.0));
        assert!(close(point_segment_dist(Vec2::new(-3.0, 4.0), a, b), 5.0));
        // (5, 12) projects past the end (1, 0); distance to the clamped end is sqrt(4^2 + 12^2).
        assert!(close(point_segment_dist(Vec2::new(5.0, 12.0), a, b), 160.0_f64.sqrt()));

        assert!(point_segment_dist(Vec2::new(1.0, 1.0), a, a) >= f64::MAX);
        assert!(point_segment_dist(Vec2::new(f64::NAN, 0.0), a, b) >= f64::MAX);
    }

    #[test]
    fn is_simple_accepts_square() {
        let square = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert!(is_simple(&square));

        let pentagon = sq(&[(0.0, 0.0), (2.0, 0.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 2.0)]);
        assert!(is_simple(&pentagon));
    }

    #[test]
    fn is_simple_rejects_bowtie() {
        let bowtie = sq(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)]);
        assert!(!is_simple(&bowtie));
    }

    #[test]
    fn is_simple_rejects_collinear_overlap() {
        let spike = sq(&[(-1.0, -1.0), (4.0, -1.0), (4.0, 4.0), (1.0, 4.0), (1.0, 2.0), (2.0, 2.0), (2.0, 4.0), (-1.0, 4.0)]);
        assert!(!is_simple(&spike));
    }

    #[test]
    fn is_simple_rejects_degenerate_and_short() {
        assert!(!is_simple(&[]));

        let two_points = vec![Vec2::ZERO, Vec2::X];
        assert!(!is_simple(&two_points));

        let zero_area = sq(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]);
        assert!(!is_simple(&zero_area));

        let inf = vec![Vec2::ZERO, Vec2::X, Vec2::new(f64::INFINITY, 0.0)];
        assert!(!is_simple(&inf));
    }

    #[test]
    fn offset_rectangle_outward_by_uniform_distance() {
        let pts = sq(&[(0.1, 0.1), (9.9, 0.1), (9.9, 2.95), (0.1, 2.95)]);
        let dist = [0.1, 0.2, 0.1, 0.2];
        let out = offset_polygon(&pts, &dist, true).unwrap_or_else(|| {
            assert!(false, "expected a valid outward offset");
            vec![]
        });
        assert!(close(out[0].x, -0.1) && close(out[0].y, 0.0));
        assert!(close(out[1].x, 10.1) && close(out[1].y, 0.0));
        assert!(close(out[2].x, 10.1) && close(out[2].y, 3.05));
        assert!(close(out[3].x, -0.1) && close(out[3].y, 3.05));
        assert!(close(ring_area(&out), 10.2 * 3.05));
    }

    #[test]
    fn offset_hole_inward_shrinks_it() {
        let pts = sq(&[(2.45, 0.95), (3.55, 0.95), (3.55, 2.05), (2.45, 2.05)]);
        let dist = [0.05, 0.05, 0.05, 0.05];
        let out = offset_polygon(&pts, &dist, false).unwrap_or_else(|| {
            assert!(false, "expected a valid inward offset");
            vec![]
        });
        assert!(close(ring_area(&out), 1.0));
    }

    #[test]
    fn offset_uses_per_edge_distances() {
        let pts = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let dist = [0.0, 0.5, 0.0, 0.5];
        let out = offset_polygon(&pts, &dist, true).unwrap_or_else(|| {
            assert!(false, "expected a valid per-edge offset");
            vec![]
        });
        assert!(close(out[0].x, -0.5) && close(out[0].y, 0.0));
        assert!(close(out[1].x, 1.5) && close(out[1].y, 0.0));
        assert!(close(out[2].x, 1.5) && close(out[2].y, 1.0));
        assert!(close(out[3].x, -0.5) && close(out[3].y, 1.0));
        assert!(close(ring_area(&out), 2.0));
    }

    #[test]
    fn offset_returns_none_for_degenerate_or_bad_input() {
        let pts = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);

        // Length mismatch.
        let dist = vec![0.1, 0.2, 0.3];
        assert!(offset_polygon(&pts, &dist, true).is_none());

        // Two-point ring.
        let two = vec![Vec2::ZERO, Vec2::X];
        assert!(offset_polygon(&two, &[0.1; 2], true).is_none());

        // Non-finite distance.
        let nan_dist = vec![0.1, f64::NAN, 0.1, 0.1];
        assert!(offset_polygon(&pts, &nan_dist, true).is_none());

        // A slot open at the top: the two slot walls offset onto the same line,
        // so the resulting ring has overlapping (self-intersecting) edges and
        // fails `is_simple`.
        let notch = sq(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (7.0, 10.0), (7.0, 4.0), (5.0, 4.0), (5.0, 10.0), (0.0, 10.0)]);
        assert!(offset_polygon(&notch, &[1.0; 8], true).is_none());
    }

    #[test]
    fn offset_zero_distance_is_identity() {
        let pts = sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let out = offset_polygon(&pts, &[0.0; 4], true).unwrap_or_else(|| {
            assert!(false, "zero distance should be the identity");
            vec![]
        });
        assert_eq!(out.len(), 4);
        for (i, p) in pts.iter().enumerate() {
            assert!(close(out[i].x, p.x));
            assert!(close(out[i].y, p.y));
        }
        assert!(close(ring_area(&out), 1.0));
    }

    fn bounds_of(r: &[Vec2]) -> (f64, f64, f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in r {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        (min_x, min_y, max_x, max_y)
    }

    fn unit_square_ccw() -> Vec<Vec2> {
        sq(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])
    }

    #[test]
    fn offset_rectangle_outward_grows() {
        let square = unit_square_ccw();
        let d = [0.5, 0.5, 0.5, 0.5];
        let Some(out) = offset_polygon(&square, &d, true) else {
            assert!(false, "offset_polygon returned None for outward unit square");
            return;
        };
        assert!(close(ring_area(&out), 4.0));
        let b = bounds_of(&out);
        assert!(close(b.0, -0.5) && close(b.1, -0.5) && close(b.2, 1.5) && close(b.3, 1.5));
        assert!(close(ring_perimeter(&out), 8.0));
    }

    #[test]
    fn offset_rectangle_inward_shrinks() {
        let square = sq(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let d = [0.25, 0.25, 0.25, 0.25];
        let Some(out) = offset_polygon(&square, &d, false) else {
            assert!(false, "offset_polygon returned None for inward 2x2 square");
            return;
        };
        assert!(close(ring_area(&out), 2.25));
        let b = bounds_of(&out);
        assert!(close(b.0, 0.25) && close(b.1, 0.25) && close(b.2, 1.75) && close(b.3, 1.75));
    }

    #[test]
    fn offset_polygon_parallel_edges_is_finite() {
        let ring = sq(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (2.0, 1.0), (0.0, 1.0)]);
        let d = [0.1, 0.1, 0.1, 0.1, 0.1];
        let Some(out) = offset_polygon(&ring, &d, true) else {
            assert!(false, "offset_polygon returned None for ring with parallel edges");
            return;
        };
        assert!(close(ring_area(&out), 2.64));
        let b = bounds_of(&out);
        assert!(close(b.0, -0.1) && close(b.1, -0.1) && close(b.2, 2.1) && close(b.3, 1.1));
    }

    #[test]
    fn offset_polygon_rejects_self_intersection() {
        let u = sq(&[(0.0, 0.0), (3.0, 0.0), (3.0, 4.0), (2.0, 4.0), (2.0, 1.0), (1.0, 1.0), (1.0, 3.0), (0.0, 3.0)]);
        let d = [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0];
        assert!(offset_polygon(&u, &d, true).is_none());
        let small = [0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2];
        assert!(offset_polygon(&u, &small, true).is_some());
    }

    #[test]
    fn offset_polygon_rejects_bad_dist_len() {
        let square = unit_square_ccw();
        let bad_len3 = [0.1, 0.1, 0.1];
        assert!(offset_polygon(&square, &bad_len3, true).is_none());
        let bad_len0 = [0.0];
        assert!(offset_polygon(&square, &[], true).is_none());
    }

    #[test]
    fn offset_polygon_rejects_negative_dist() {
        let square = unit_square_ccw();
        let negative = [0.1, -0.1, 0.1, 0.1];
        assert!(offset_polygon(&square, &negative, true).is_none());
        let nan_dist = [0.1, f64::NAN, 0.1, 0.1];
        assert!(offset_polygon(&square, &nan_dist, true).is_none());
    }

    #[test]
    fn offset_polygon_rejects_degenerate_ring() {
        let two_points = vec![Vec2::ZERO, Vec2::X];
        let two_dist = [0.1, 0.1];
        assert!(offset_polygon(&two_points, &two_dist, true).is_none());
        let repeated = sq(&[(0.0, 0.0), (0.0, 0.0), (1.0, 1.0)]);
        let repeated_dist = [0.1, 0.1, 0.1];
        assert!(offset_polygon(&repeated, &repeated_dist, true).is_none());
    }

    #[test]
    fn offset_polygon_rejects_nan_points() {
        let square = unit_square_ccw();
        let with_nan = vec![Vec2::ZERO, Vec2::X, Vec2::new(1.0, f64::NAN), Vec2::new(0.0, 1.0)];
        let d = [0.1, 0.1, 0.1, 0.1];
        assert!(offset_polygon(&with_nan, &d, true).is_none());
    }

    #[test]
    fn offset_polygon_rejects_inf_points() {
        let with_inf = vec![Vec2::ZERO, Vec2::X, Vec2::new(f64::INFINITY, 1.0), Vec2::new(0.0, 1.0)];
        let d = [0.1, 0.1, 0.1, 0.1];
        assert!(offset_polygon(&with_inf, &d, true).is_none());
    }

    #[test]
    fn offset_polygon_triangle() {
        let tri = sq(&[(0.0, 0.0), (2.0, 0.0), (0.0, 2.0)]);
        let d = [1.0, 1.0, 1.0];
        let out = offset_polygon(&tri, &d, true).unwrap_or_else(|| {
            assert!(false, "triangle offset failed");
            vec![]
        });
        assert_eq!(out.len(), 3);
        assert!(ring_area(&out) > ring_area(&tri));
        assert!(is_simple(&out));
    }

    #[test]
    fn offset_polygon_cw_input() {
        let cw = sq(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)]);
        let d = [0.5, 0.5, 0.5, 0.5];
        let out = offset_polygon(&cw, &d, true).unwrap_or_else(|| {
            assert!(false, "CW square offset failed");
            vec![]
        });
        assert!(close(ring_area(&out), 4.0));
        let b = bounds_of(&out);
        assert!(close(b.0, -0.5) && close(b.1, -0.5) && close(b.2, 1.5) && close(b.3, 1.5));
    }

    #[test]
    fn offset_polygon_large_coords() {
        let big = sq(&[(1e6, 1e6), (1e6 + 1.0, 1e6), (1e6 + 1.0, 1e6 + 1.0), (1e6, 1e6 + 1.0)]);
        let d = [0.5, 0.5, 0.5, 0.5];
        let out = offset_polygon(&big, &d, true).unwrap_or_else(|| {
            assert!(false, "large-coordinate square offset failed");
            vec![]
        });
        assert!(close(ring_area(&out), 4.0));
        let b = bounds_of(&out);
        assert!(close(b.0, 1e6 - 0.5) && close(b.1, 1e6 - 0.5));
        assert!(close(b.2, 1e6 + 1.5) && close(b.3, 1e6 + 1.5));
    }

    #[test]
    fn offset_polygon_tiny_distance() {
        let square = unit_square_ccw();
        let d = [1e-9, 1e-9, 1e-9, 1e-9];
        let out = offset_polygon(&square, &d, true).unwrap_or_else(|| {
            assert!(false, "tiny distance offset failed");
            vec![]
        });
        assert!(is_simple(&out));
        assert!(ring_area(&out) > 1.0);
    }
}
