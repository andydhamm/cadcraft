//! Utility for computing a point strictly inside a polygon (with optional holes)
//! for use as a label placement position.

use cadcraft_geom::{Vec2, point_in_polygon};

/// Minimum distance from any polygon edge that a label point must maintain.
const MIN_EDGE_DIST: f64 = 1e-9;
/// Minimum polygon area below which the polygon is considered degenerate.
const MIN_AREA: f64 = 1e-9;
/// Maximum number of vertex-pair chords to probe for the longest interior chord.
const MAX_PAIRS: u32 = 4096;

/// Find a point strictly inside `outer` (and outside every `holes` polygon) that
/// is suitable for placing a text label.
///
/// Returns `None` if `outer` is degenerate (fewer than 3 vertices, any non-finite
/// coordinate, or area < 1e-9), or if no valid point can be found.
///
/// The search proceeds through progressively more aggressive strategies:
/// 1. Vertex-average centroid, if it is strictly inside.
/// 2. Midpoint of the longest interior vertex-vertex chord.
/// 3. Midpoint of each vertex with the centroid, then with each other vertex.
pub fn label_point(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Option<Vec2> {
    if outer.len() < 3 {
        return None;
    }
    if !outer.iter().all(|p| p.is_finite()) {
        return None;
    }
    for hole in holes {
        if !hole.iter().all(|p| p.is_finite()) {
            return None;
        }
    }
    let area = crate::join::polygon_area(outer);
    if area.abs() < MIN_AREA {
        return None;
    }

    // 1. Vertex-average centroid.
    let centroid = vertex_centroid(outer);
    if is_strictly_inside(&centroid, outer, holes) {
        return Some(centroid);
    }

    // 2. Longest interior chord midpoint.
    if let Some(mid) = longest_interior_chord(outer, holes) {
        return Some(mid);
    }

    // 3. Vertex-centroid and vertex-vertex fallbacks, in index order.
    let n = outer.len();
    for i in 0..n {
        let v = outer[i];
        let p = Vec2::new((v.x + centroid.x) * 0.5, (v.y + centroid.y) * 0.5);
        if is_strictly_inside(&p, outer, holes) {
            return Some(p);
        }
        for j in 0..n {
            if i == j {
                continue;
            }
            let p = Vec2::new((v.x + outer[j].x) * 0.5, (v.y + outer[j].y) * 0.5);
            if is_strictly_inside(&p, outer, holes) {
                return Some(p);
            }
        }
    }

    None
}

/// The average of all vertices (vertex-average, not area-weighted).
fn vertex_centroid(outer: &[Vec2]) -> Vec2 {
    let n = outer.len() as f64;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for v in outer {
        sx += v.x;
        sy += v.y;
    }
    Vec2::new(sx / n, sy / n)
}

/// Check that `p` is strictly inside `outer`, outside every hole, and at least
/// `MIN_EDGE_DIST` away from every edge of `outer` and every hole.
fn is_strictly_inside(p: &Vec2, outer: &[Vec2], holes: &[Vec<Vec2>]) -> bool {
    if !point_in_polygon(outer, *p) {
        return false;
    }
    for hole in holes {
        if hole.len() >= 3 && point_in_polygon(hole, *p) {
            return false;
        }
    }
    if min_edge_dist(p, outer) <= MIN_EDGE_DIST {
        return false;
    }
    for hole in holes {
        if min_edge_dist(p, hole) <= MIN_EDGE_DIST {
            return false;
        }
    }
    true
}

/// Minimum distance from `p` to any edge of `poly`.
fn min_edge_dist(p: &Vec2, poly: &[Vec2]) -> f64 {
    let n = poly.len();
    if n < 2 {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let d = dist_to_segment(*p, a, b);
        if d < best {
            best = d;
        }
    }
    best
}

/// Euclidean distance from `p` to the segment `[a, b]`.
fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let ab_len_sq = abx * abx + aby * aby;
    if ab_len_sq < 1e-30 {
        let dx = p.x - a.x;
        let dy = p.y - a.y;
        return (dx * dx + dy * dy).sqrt();
    }
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let t = (apx * abx + apy * aby) / ab_len_sq;
    let tc = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    let cx = a.x + abx * tc;
    let cy = a.y + aby * tc;
    let dx = p.x - cx;
    let dy = p.y - cy;
    (dx * dx + dy * dy).sqrt()
}

/// Midpoint of the longest vertex-vertex chord whose midpoint is strictly inside.
///
/// Iterates pairs `(i, j)` with `i < j` in index order, stopping after
/// `MAX_PAIRS` pairs. Returns the midpoint that maximises the vertex-vertex
/// distance among valid (strictly-inside) candidates.
fn longest_interior_chord(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Option<Vec2> {
    let n = outer.len();
    let mut best_mid: Option<Vec2> = None;
    let mut best_dist_sq: f64 = -1.0;
    let mut pairs = 0u32;

    for i in 0..n {
        for j in (i + 1)..n {
            pairs += 1;
            if pairs > MAX_PAIRS {
                return best_mid;
            }
            let vi = outer[i];
            let vj = outer[j];
            let dx = vi.x - vj.x;
            let dy = vi.y - vj.y;
            let d2 = dx * dx + dy * dy;
            if d2 <= best_dist_sq {
                continue;
            }
            let mid = Vec2::new((vi.x + vj.x) * 0.5, (vi.y + vj.y) * 0.5);
            if is_strictly_inside(&mid, outer, holes) {
                best_mid = Some(mid);
                best_dist_sq = d2;
            }
        }
        if pairs > MAX_PAIRS {
            break;
        }
    }

    best_mid
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::Vec2;

    fn pt(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn label_point_is_strictly_inside_its_polygon() {
        // Square: the vertex-average centroid is the centre, returned directly.
        let outer = vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 4.0), pt(0.0, 4.0)];
        let p = label_point(&outer, &[]).expect("square should yield a label point");
        assert!((p.x - 2.0).abs() < 1e-9);
        assert!((p.y - 2.0).abs() < 1e-9);

        // L-shape: the vertex-average centroid falls in the missing corner,
        // so the function must fall back to a chord or vertex-vertex point.
        let lshape = vec![pt(0.0, 0.0), pt(6.0, 0.0), pt(6.0, 2.0), pt(2.0, 2.0), pt(2.0, 6.0), pt(0.0, 6.0)];
        let p = label_point(&lshape, &[]).expect("L-shape should yield a label point");
        assert!(point_in_polygon(&lshape, p));
        // Must be more than 1e-6 from every edge.
        for i in 0..lshape.len() {
            let a = lshape[i];
            let b = lshape[(i + 1) % lshape.len()];
            let d = dist_to_segment(p, a, b);
            assert!(d > 1e-6, "distance to edge {i} is {d}");
        }
    }

    #[test]
    fn label_point_avoids_holes() {
        let outer = vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)];
        let hole = vec![pt(4.0, 4.0), pt(6.0, 4.0), pt(6.0, 6.0), pt(4.0, 6.0)];
        let p = label_point(&outer, &[hole.clone()]).expect("should yield a point");
        assert!(point_in_polygon(&outer, p));
        assert!(!point_in_polygon(&hole, p));
    }

    #[test]
    fn label_point_returns_none_for_degenerate() {
        // Empty outer.
        assert!(label_point(&[], &[]).is_none());
        // Two vertices.
        assert!(label_point(&vec![pt(0.0, 0.0), pt(1.0, 1.0)], &[]).is_none());
        // Collinear (zero area).
        let collinear = vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(2.0, 0.0)];
        assert!(label_point(&collinear, &[]).is_none());
        // NaN vertex.
        let nan = vec![pt(f64::NAN, 0.0), pt(1.0, 1.0), pt(2.0, 2.0)];
        assert!(label_point(&nan, &[]).is_none());
        // Inf vertex.
        let inf = vec![pt(f64::INFINITY, 0.0), pt(1.0, 1.0), pt(2.0, 2.0)];
        assert!(label_point(&inf, &[]).is_none());
    }

    #[test]
    fn label_point_is_deterministic() {
        let outer = vec![pt(0.0, 0.0), pt(6.0, 0.0), pt(6.0, 2.0), pt(2.0, 2.0), pt(2.0, 6.0), pt(0.0, 6.0)];
        let p1 = label_point(&outer, &[]).expect("should yield a point");
        let p2 = label_point(&outer, &[]).expect("should yield a point");
        assert_eq!(p1.x, p2.x);
        assert_eq!(p1.y, p2.y);
    }
}
