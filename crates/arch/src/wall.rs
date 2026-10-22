//! Wall geometry: poché (2D cross-section) polygons, offsets, layer bands, and bounds.
//!
//! A wall is a centre-line segment with a thickness and a height. The poché ring is the
//! 4-corner rectangle perpendicular to the centre line. Layer bands split that rectangle
//! into nested sub-rectangles from `wall_type.layers`.

pub use cadcraft_doc::Wall;
use cadcraft_geom::{Bounds2, Vec2};

/// The 4-corner CCW poché ring of a wall (no repeated first point).
///
/// Returns `None` when the wall is degenerate (zero length, non-finite points, or
/// non-positive thickness).
pub fn wall_polygon(w: &Wall) -> Option<Vec<Vec2>> {
    if !w.start.is_finite() || !w.end.is_finite() {
        return None;
    }
    let len = w.start.dist(w.end);
    if len < 1e-12 || !len.is_finite() {
        return None;
    }
    if w.thickness <= 0.0 || !w.thickness.is_finite() {
        return None;
    }
    let half = w.thickness / 2.0;
    let dir = (w.end - w.start).normalized();
    let perp = dir.perp();
    let a = w.start - perp * half;
    let b = w.end - perp * half;
    let c = w.end + perp * half;
    let d = w.start + perp * half;
    // CCW: a → b → c → d
    Some(vec![a, b, c, d])
}

/// Unit direction from `start` to `end`.
pub fn wall_dir(w: &Wall) -> Option<Vec2> {
    let d = w.end - w.start;
    if d.len() < 1e-12 || !d.is_finite() {
        return None;
    }
    Some(d.normalized())
}

/// Centre-line length of the wall.
pub fn wall_length(w: &Wall) -> f64 {
    w.start.dist(w.end)
}

/// Bounding box of the wall's poché ring.
pub fn wall_bounds(w: &Wall) -> Bounds2 {
    match wall_polygon(w) {
        Some(pts) => Bounds2::from_points(pts.iter().copied()),
        None => Bounds2::EMPTY,
    }
}

/// Perpendicular offset of the segment `a→b` by distance `d` (positive = left of the
/// direction). Returns `None` when the segment is degenerate or `d` is non-finite.
pub fn offset_line(a: Vec2, b: Vec2, d: f64) -> Option<(Vec2, Vec2)> {
    if !a.is_finite() || !b.is_finite() || !d.is_finite() {
        return None;
    }
    let dir = (b - a).normalized();
    if dir == Vec2::ZERO {
        return None;
    }
    let perp = dir.perp();
    Some((a + perp * d, b + perp * d))
}

/// Nested layer-band polygons: each band is a sub-rectangle of the poché ring, split
/// from the outer edge inward. Bands are clamped so their total thickness does not
/// exceed the wall thickness. Returns an empty vec when the wall is degenerate.
pub fn wall_layer_polygons(w: &Wall) -> Vec<(String, Vec<Vec2>)> {
    let Some(poly) = wall_polygon(w) else {
        return Vec::new();
    };
    let dir = (w.end - w.start).normalized();
    let perp = dir.perp();
    let half = w.thickness / 2.0;
    let outer = half;
    let mut out: Vec<(String, Vec<Vec2>)> = Vec::new();
    let mut consumed = 0.0;
    for (name, t) in &w.layers {
        let t = t.clamp(0.0, w.thickness - consumed);
        if t < 1e-12 {
            continue;
        }
        let outer = half - consumed;
        let inner = half - consumed - t;
        consumed += t;
        let a = w.start - perp * outer;
        let b = w.end - perp * outer;
        let c = w.end - perp * inner;
        let d = w.start - perp * inner;
        out.push((name.clone(), vec![a, b, c, d]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::Vec2;

    fn wall(start: Vec2, end: Vec2, thickness: f64) -> Wall {
        Wall { start, end, thickness, height: 3.0, layers: Vec::new(), ..Wall::default() }
    }

    #[test]
    fn wall_polygon_is_a_closed_ccw_ring() {
        let w = wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let p = wall_polygon(&w).unwrap();
        assert_eq!(p.len(), 4);
        // CCW: the signed area should be positive.
        let area = 0.5
            * (p[0].x * p[1].y - p[1].x * p[0].y + p[1].x * p[2].y - p[2].x * p[1].y + p[2].x * p[3].y - p[3].x * p[2].y + p[3].x * p[0].y
                - p[0].x * p[3].y);
        assert!(area > 0.0, "area should be positive for CCW: {area}");
        // No repeated first point.
        assert!(!p[0].near(p[3], 1e-9));
    }

    #[test]
    fn wall_polygon_is_none_for_zero_length() {
        let w = wall(Vec2::ZERO, Vec2::ZERO, 2.0);
        assert!(wall_polygon(&w).is_none());
    }

    #[test]
    fn offset_line_offsets_perpendicular_both_sides() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        // Positive offset = left of direction (0,0)→(10,0) = up.
        let (o1, o2) = offset_line(a, b, 1.0).unwrap();
        assert!(o1.near(Vec2::new(0.0, 1.0), 1e-9));
        assert!(o2.near(Vec2::new(10.0, 1.0), 1e-9));
        // Negative offset = down.
        let (o3, o4) = offset_line(a, b, -1.0).unwrap();
        assert!(o3.near(Vec2::new(0.0, -1.0), 1e-9));
        assert!(o4.near(Vec2::new(10.0, -1.0), 1e-9));
    }

    #[test]
    fn wall_layer_polygons_sum_to_the_wall_thickness() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 4.0,
            height: 3.0,
            layers: vec![("outer".into(), 1.0), ("core".into(), 2.0), ("inner".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 3);
        // Each band's thickness = distance between its two parallel edges.
        for (name, pts) in &polys {
            let dir = (w.end - w.start).normalized();
            let perp = dir.perp();
            // The band spans from outer edge to inner edge; its thickness is the
            // perpendicular distance between the two long edges.
            let a = pts[0];
            let b = pts[1];
            let c = pts[2];
            let d = pts[3];
            // Width of the band = |perp component of (a - d)|.
            let width = (a - d).dot(perp).abs();
            let expected = w.layers.iter().find(|(n, _)| n == name).map(|(_, t)| *t).unwrap();
            assert!((width - expected).abs() < 1e-9, "band {name}: {width} != {expected}");
        }
        // Sum of band thicknesses = wall thickness.
        let sum: f64 = w.layers.iter().map(|(_, t)| t).sum();
        assert!((sum - w.thickness).abs() < 1e-9);
    }

    #[test]
    fn wall_bounds_matches_the_poché_ring() {
        let w = wall(Vec2::new(1.0, 2.0), Vec2::new(11.0, 2.0), 3.0);
        let b = wall_bounds(&w);
        let p = wall_polygon(&w).unwrap();
        let pb = Bounds2::from_points(p.iter().copied());
        assert_eq!(b, pb);
        assert!((b.min.x - 1.0).abs() < 1e-9);
        assert!((b.max.x - 11.0).abs() < 1e-9);
        assert!((b.min.y - 0.5).abs() < 1e-9);
        assert!((b.max.y - 3.5).abs() < 1e-9);
    }

    #[test]
    fn non_finite_input_returns_none_never_panics() {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        // Non-finite start.
        assert!(wall_polygon(&wall(Vec2::new(nan, 0.0), Vec2::new(10.0, 0.0), 2.0)).is_none());
        // Non-finite end.
        assert!(wall_polygon(&wall(Vec2::new(0.0, 0.0), Vec2::new(inf, 0.0), 2.0)).is_none());
        // Non-finite thickness.
        assert!(wall_polygon(&wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), nan)).is_none());
        // Zero thickness.
        assert!(wall_polygon(&wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.0)).is_none());
        // Negative thickness.
        assert!(wall_polygon(&wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), -1.0)).is_none());
        // Degenerate offset_line.
        assert!(offset_line(Vec2::ZERO, Vec2::ZERO, 1.0).is_none());
        assert!(offset_line(Vec2::new(nan, 0.0), Vec2::new(1.0, 0.0), 1.0).is_none());
        assert!(offset_line(Vec2::ZERO, Vec2::new(1.0, 0.0), inf).is_none());
        // wall_layer_polygons on degenerate wall.
        let w = wall(Vec2::ZERO, Vec2::ZERO, 2.0);
        assert!(wall_layer_polygons(&w).is_empty());
        // wall_bounds on degenerate wall.
        assert!(wall_bounds(&w).is_empty());
    }
}
