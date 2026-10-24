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
        assert!(close(point_segment_dist(Vec2::new(5.0, 12.0), a, b), 13.0));

        assert!(point_segment_dist(Vec2::new(1.0, 1.0), a, a) >= f64::MAX);
        assert!(point_segment_dist(Vec2::new(f64::NAN, 0.0), a, b) >= f64::MAX);
    }
}
