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
    let mut out: Vec<(String, Vec<Vec2>)> = Vec::new();
    let mut consumed = 0.0;
    for (name, t) in &w.layers {
        // Skip bands whose thickness is not a finite number (NaN/inf) rather than feeding
        // them to arithmetic that could produce a NaN offset.
        if !t.is_finite() {
            continue;
        }
        // Remaining budget is floored at zero: rounding can leave `thickness - consumed`
        // one ulp below zero, and `f64::clamp` panics when its bounds cross.
        let remaining = (w.thickness - consumed).max(0.0);
        let t = t.min(remaining).max(0.0);
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
    fn layer_bands_thicker_than_the_wall_are_clipped_without_panicking() {
        for &thickness in &[0.1, 0.103, 0.0999, 0.3 - 0.2, 1e-4] {
            let w = Wall {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(10.0, 0.0),
                thickness,
                height: 3.0,
                layers: vec![("outer".into(), 0.013), ("core".into(), 0.09), ("inner".into(), 0.013)],
                ..Wall::default()
            };
            let polys = wall_layer_polygons(&w);
            let dir = (w.end - w.start).normalized();
            let perp = dir.perp();
            // Summed band widths never exceed the wall thickness (within tolerance).
            let mut total = 0.0;
            for (_, pts) in &polys {
                let width = (pts[0] - pts[3]).dot(perp).abs();
                assert!(width.is_finite());
                total += width;
            }
            assert!(total <= thickness + 1e-12, "total {total} exceeds thickness {thickness}");
        }
        // The exact rounding case from the report: thickness one ulp below the band sum.
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.937076180931465,
            height: 3.0,
            layers: vec![("a".into(), 0.395546415468281), ("b".into(), 5.0), ("c".into(), 0.01)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 2, "band b (5.0) is clipped away, leaving a and c");
    }

    #[test]
    fn hostile_layer_band_thicknesses_are_skipped() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 3.0,
            layers: vec![("nan".into(), f64::NAN), ("neg".into(), -1.0), ("inf".into(), f64::INFINITY), ("ok".into(), 0.05)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 1, "only the finite band survives");
        assert_eq!(polys[0].0, "ok");
        for (_, pts) in &polys {
            assert!(pts.iter().all(|p| p.is_finite()));
        }
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

    #[test]
    fn wall_layer_polygons_with_no_layers_returns_empty() {
        let w = wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        assert!(wall_layer_polygons(&w).is_empty());
    }

    #[test]
    fn layer_bands_with_zero_and_tiny_thickness_are_skipped() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("zero".into(), 0.0), ("tiny".into(), 1e-15), ("ok".into(), 0.5)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 1, "only the 0.5 band should survive");
        assert_eq!(polys[0].0, "ok");
    }

    #[test]
    fn layer_bands_fill_wall_exactly() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0), ("c".into(), 0.5)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 3);
        // The outermost band should start at the outer face (y = -1.0).
        let a0 = polys[0].1[0];
        assert!((a0.y - (-1.0)).abs() < 1e-9, "first band starts at outer face");
        // The innermost band should end at the inner face (y = 1.0).
        let c2 = polys[2].1[2];
        assert!((c2.y - 1.0).abs() < 1e-9, "last band ends at inner face");
    }

    #[test]
    fn layer_bands_with_empty_and_duplicate_names() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 3.0,
            height: 3.0,
            layers: vec![("".into(), 0.5), ("dup".into(), 1.0), ("dup".into(), 0.5), ("last".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 4, "all bands including empty and duplicate names survive");
        assert_eq!(polys[0].0, "");
        assert_eq!(polys[1].0, "dup");
        assert_eq!(polys[2].0, "dup");
        assert_eq!(polys[3].0, "last");
    }

    #[test]
    fn wall_layer_polygons_on_very_thin_wall() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 1e-10,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 0.5)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        // Both bands are thicker than the wall, so the first is clamped to the wall
        // thickness and the second is skipped (no remaining budget).
        assert_eq!(polys.len(), 1, "only one band fits in a 1e-10 wall");
        assert_eq!(polys[0].0, "a");
    }

    #[test]
    fn layer_bands_on_diagonal_wall() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 10.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 2);
        for (_, pts) in &polys {
            assert_eq!(pts.len(), 4);
            assert!(pts.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn layer_band_exactly_fills_wall_thickness() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("full".into(), 2.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 1);
        let pts = &polys[0].1;
        let width = (pts[0] - pts[3]).y.abs();
        assert!((width - 2.0).abs() < 1e-9, "band should span full thickness");
    }

    #[test]
    fn layer_bands_with_floating_point_sum() {
        // 0.1 + 0.2 + 0.3 in f64 is 0.6000000000000001, not exactly 0.6.
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.6,
            height: 3.0,
            layers: vec![("a".into(), 0.1), ("b".into(), 0.2), ("c".into(), 0.3)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        // All three bands should fit despite floating-point rounding.
        assert_eq!(polys.len(), 3);
    }

    #[test]
    fn layer_bands_on_wall_with_negative_coordinates() {
        let w = Wall {
            start: Vec2::new(-5.0, -3.0),
            end: Vec2::new(5.0, -3.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 2);
        for (_, pts) in &polys {
            assert!(pts.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn layer_bands_on_very_long_wall() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(1e6, 0.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 2);
        for (_, pts) in &polys {
            assert!(pts.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn layer_bands_on_wall_with_huge_thickness() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 1e15,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 2);
        for (_, pts) in &polys {
            assert!(pts.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn layer_bands_summing_to_less_than_thickness_do_not_reach_inner_face() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 2.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        assert_eq!(polys.len(), 1);
        let pts = &polys[0].1;
        // Band starts at outer face (y = -1.0) and ends at y = -0.5, not at inner face.
        assert!((pts[0].y - (-1.0)).abs() < 1e-9, "band starts at outer face");
        assert!((pts[3].y - (-0.5)).abs() < 1e-9, "band ends at y = -0.5");
    }

    #[test]
    fn wall_dir_returns_none_for_zero_length() {
        let w = wall(Vec2::ZERO, Vec2::ZERO, 2.0);
        assert!(wall_dir(&w).is_none());
    }

    #[test]
    fn wall_dir_returns_correct_direction() {
        let w = wall(Vec2::new(0.0, 0.0), Vec2::new(3.0, 4.0), 2.0);
        let dir = wall_dir(&w).unwrap();
        assert!((dir.x - 0.6).abs() < 1e-9, "x component should be 0.6");
        assert!((dir.y - 0.8).abs() < 1e-9, "y component should be 0.8");
        // Should be a unit vector
        assert!((dir.len() - 1.0).abs() < 1e-9, "should be unit length");
    }

    #[test]
    fn wall_dir_returns_none_for_non_finite_points() {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        assert!(wall_dir(&wall(Vec2::new(nan, 0.0), Vec2::new(1.0, 0.0), 2.0)).is_none());
        assert!(wall_dir(&wall(Vec2::new(0.0, 0.0), Vec2::new(inf, 0.0), 2.0)).is_none());
    }

    #[test]
    fn wall_length_returns_correct_length() {
        let w = wall(Vec2::new(0.0, 0.0), Vec2::new(3.0, 4.0), 2.0);
        assert!((wall_length(&w) - 5.0).abs() < 1e-9, "length should be 5.0");
    }

    #[test]
    fn wall_length_with_zero_length_wall() {
        let w = wall(Vec2::ZERO, Vec2::ZERO, 2.0);
        assert!((wall_length(&w) - 0.0).abs() < 1e-9, "length should be 0.0");
    }

    #[test]
    fn wall_length_with_non_finite_points() {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        // NaN coordinates should give NaN length
        let w = wall(Vec2::new(nan, 0.0), Vec2::new(1.0, 0.0), 2.0);
        assert!(wall_length(&w).is_nan(), "length should be NaN with NaN input");
        // Infinite coordinates should give infinite length
        let w = wall(Vec2::new(0.0, 0.0), Vec2::new(inf, 0.0), 2.0);
        assert!(wall_length(&w).is_infinite(), "length should be infinite with inf input");
    }

    #[test]
    fn offset_line_with_zero_distance_returns_original_line() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(4.0, 6.0);
        let (o1, o2) = offset_line(a, b, 0.0).unwrap();
        assert!(o1.near(a, 1e-9), "offset by 0 should return original start");
        assert!(o2.near(b, 1e-9), "offset by 0 should return original end");
    }

    #[test]
    fn wall_layer_polygons_with_degenerate_wall_returns_empty() {
        let w = Wall {
            start: Vec2::ZERO,
            end: Vec2::ZERO,
            thickness: 2.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0)],
            ..Wall::default()
        };
        assert!(wall_layer_polygons(&w).is_empty(), "degenerate wall should have no layer polygons");
    }

    #[test]
    fn wall_layer_polygons_with_negative_thickness_wall_returns_empty() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: -1.0,
            height: 3.0,
            layers: vec![("a".into(), 0.5)],
            ..Wall::default()
        };
        assert!(wall_layer_polygons(&w).is_empty(), "negative thickness wall should have no layer polygons");
    }

    #[test]
    fn wall_layer_polygons_with_nan_thickness_wall_returns_empty() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: f64::NAN,
            height: 3.0,
            layers: vec![("a".into(), 0.5)],
            ..Wall::default()
        };
        assert!(wall_layer_polygons(&w).is_empty(), "NaN thickness wall should have no layer polygons");
    }

    #[test]
    fn wall_bounds_with_degenerate_wall_returns_empty() {
        let w = wall(Vec2::ZERO, Vec2::ZERO, 2.0);
        assert_eq!(wall_bounds(&w), Bounds2::EMPTY, "degenerate wall should have empty bounds");
    }

    #[test]
    fn wall_polygon_with_very_large_coordinates() {
        let w = wall(Vec2::new(1e15, 1e15), Vec2::new(1e15 + 10.0, 1e15), 2.0);
        let p = wall_polygon(&w).unwrap();
        assert_eq!(p.len(), 4);
        assert!(p.iter().all(|pt| pt.is_finite()), "all points should be finite");
    }

    #[test]
    fn wall_polygon_with_very_small_coordinates() {
        let w = wall(Vec2::new(1e-15, 1e-15), Vec2::new(1e-15 + 1e-10, 1e-15), 1e-12);
        let p = wall_polygon(&w).unwrap();
        assert_eq!(p.len(), 4);
        assert!(p.iter().all(|pt| pt.is_finite()), "all points should be finite");
    }

    #[test]
    fn wall_layer_polygons_on_wall_with_infinite_thickness() {
        let w = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: f64::INFINITY,
            height: 3.0,
            layers: vec![("a".into(), 0.5), ("b".into(), 1.0)],
            ..Wall::default()
        };
        let polys = wall_layer_polygons(&w);
        // With infinite thickness, all bands should fit
        assert_eq!(polys.len(), 2, "both bands should fit in infinite-thickness wall");
        for (_, pts) in &polys {
            assert!(pts.iter().all(|p| p.is_finite()), "all points should be finite");
        }
    }

    #[test]
    fn wall_dir_on_vertical_wall() {
        let w = wall(Vec2::new(0.0, 0.0), Vec2::new(0.0, 5.0), 2.0);
        let dir = wall_dir(&w).unwrap();
        assert!((dir.x - 0.0).abs() < 1e-9, "x component should be 0.0");
        assert!((dir.y - 1.0).abs() < 1e-9, "y component should be 1.0");
    }

    #[test]
    fn wall_dir_on_downward_wall() {
        let w = wall(Vec2::new(0.0, 5.0), Vec2::new(0.0, 0.0), 2.0);
        let dir = wall_dir(&w).unwrap();
        assert!((dir.x - 0.0).abs() < 1e-9, "x component should be 0.0");
        assert!((dir.y - (-1.0)).abs() < 1e-9, "y component should be -1.0");
    }
}
