//! A triangulated irregular network (TIN): Delaunay triangulation in the plane with each point's Z
//! carried along, linear elevation queries and per-triangle contour segments.
//!
//! Lifted from the `cadcraft-terrain` crate on `zorgglubb/main` of the zorgglubb/cadcraft fork
//! (MIT OR Apache-2.0, credited in `ATTRIBUTION.md`): `triangulate`, `Tin::elevation_at`,
//! `Tin::elevation_range` and `contours` are its code (Bowyer-Watson triangulation, per-triangle contour segments). Changes
//! here: one module instead of a crate, a plain error enum (geom has no `thiserror`), no indexing
//! with `[]` on input, and a cap on the number of points.
//!
//! Built with the Bowyer-Watson incremental algorithm: a point at a time, remove every triangle
//! whose circumcircle contains it, and retriangulate the hole that leaves. The triangulation works
//! in the plane (X, Y); each point's Z rides along for elevation queries and contour extraction,
//! not for choosing the triangulation itself — the usual convention for a terrain TIN.

use crate::{Vec2, Vec3};

/// The most points [`triangulate`] accepts: insertion scans every triangle, so the cost grows
/// with the square of the point count.
pub const MAX_TIN_POINTS: usize = 20_000;

/// Why a TIN could not be built.
#[derive(Clone, Debug, PartialEq)]
pub enum TinError {
    /// Fewer than three points.
    NotEnoughPoints(usize),
    /// More than [`MAX_TIN_POINTS`] points.
    TooManyPoints(usize),
    /// Point `index` has a non-finite coordinate.
    NonFinitePoint { index: usize },
    /// All points collinear or coincident: no triangle could be formed.
    Degenerate,
}

impl std::fmt::Display for TinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TinError::NotEnoughPoints(n) => write!(f, "a triangulation needs at least 3 points, got {n}"),
            TinError::TooManyPoints(n) => write!(f, "a triangulation takes at most {MAX_TIN_POINTS} points, got {n}"),
            TinError::NonFinitePoint { index } => write!(f, "point {index} has a non-finite coordinate"),
            TinError::Degenerate => write!(f, "the points are degenerate (collinear, or all coincident): no triangle could be formed"),
        }
    }
}

impl std::error::Error for TinError {}

/// A triangulated irregular network: the input points plus the triangles connecting them, each
/// stored as the indices of three of those points (counter-clockwise).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tin {
    pub points: Vec<Vec3>,
    pub triangles: Vec<[usize; 3]>,
}

impl Tin {
    /// The lowest and highest elevation among the TIN's points, if it has any.
    pub fn elevation_range(&self) -> Option<(f64, f64)> {
        let mut it = self.points.iter().map(|p| p.z);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), z| (lo.min(z), hi.max(z))))
    }

    /// Linearly interpolates the elevation at `(x, y)` from whichever triangle contains it
    /// (barycentric interpolation of its three corners' Z), or `None` outside the TIN.
    pub fn elevation_at(&self, x: f64, y: f64) -> Option<f64> {
        let p = Vec2::new(x, y);
        for tri in &self.triangles {
            let [ia, ib, ic] = *tri;
            let (a, b, c) = (self.points.get(ia)?, self.points.get(ib)?, self.points.get(ic)?);
            if let Some((u, v, w)) = barycentric(Vec2::new(a.x, a.y), Vec2::new(b.x, b.y), Vec2::new(c.x, c.y), p) {
                return Some(u * a.z + v * b.z + w * c.z);
            }
        }
        None
    }
}

/// Barycentric coordinates of `p` in triangle `(a, b, c)`, or `None` when `p` falls outside it
/// (any coordinate strictly negative beyond a small tolerance) or the triangle is degenerate.
pub fn barycentric(a: Vec2, b: Vec2, c: Vec2, p: Vec2) -> Option<(f64, f64, f64)> {
    let v0 = b - a;
    let v1 = c - a;
    let v2 = p - a;
    let d00 = v0.dot(v0);
    let d01 = v0.dot(v1);
    let d11 = v1.dot(v1);
    let d20 = v2.dot(v0);
    let d21 = v2.dot(v1);
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-12 {
        return None;
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    const EPS: f64 = 1e-9;
    (u >= -EPS && v >= -EPS && w >= -EPS).then_some((u, v, w))
}

/// Whether `p` lies strictly inside the circumcircle of the (any-orientation) triangle `(a, b, c)`,
/// via the in-circle determinant, normalised to a counter-clockwise triangle first.
pub fn in_circumcircle(a: Vec2, b: Vec2, c: Vec2, p: Vec2) -> bool {
    let (b, c) = if (b - a).cross(c - a) < 0.0 { (c, b) } else { (b, c) };
    let (ax, ay) = (a.x - p.x, a.y - p.y);
    let (bx, by) = (b.x - p.x, b.y - p.y);
    let (cx, cy) = (c.x - p.x, c.y - p.y);
    let det = (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay) + (cx * cx + cy * cy) * (ax * by - bx * ay);
    det > 1e-9
}

/// Builds the Delaunay triangulation of `points` (their X, Y; Z carries over unused by the
/// triangulation itself). Never panics: too few or too many points, a non-finite coordinate, or
/// points too degenerate to form any triangle are reported as a [`TinError`]. A point that
/// coincides with an earlier one is skipped (it is kept in `points` but used by no triangle).
pub fn triangulate(points: &[Vec3]) -> Result<Tin, TinError> {
    let n = points.len();
    if n < 3 {
        return Err(TinError::NotEnoughPoints(n));
    }
    if n > MAX_TIN_POINTS {
        return Err(TinError::TooManyPoints(n));
    }
    for (i, p) in points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
            return Err(TinError::NonFinitePoint { index: i });
        }
    }

    let first = points.first().ok_or(TinError::NotEnoughPoints(0))?;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let dx = (max_x - min_x).max(1.0);
    let dy = (max_y - min_y).max(1.0);
    let margin = dx.max(dy) * 20.0 + 10.0;
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;

    // A super-triangle generously enclosing every input point, appended after them so its vertex
    // indices (n, n+1, n+2) are easy to recognise and strip out at the end.
    let mut pts2: Vec<Vec2> = points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
    pts2.push(Vec2::new(cx - 2.0 * margin, cy - margin));
    pts2.push(Vec2::new(cx, cy + 2.0 * margin));
    pts2.push(Vec2::new(cx + 2.0 * margin, cy - margin));
    let (s0, s1, s2) = (n, n + 1, n + 2);

    let mut triangles: Vec<[usize; 3]> = vec![[s0, s1, s2]];

    for (i, &p) in pts2.iter().enumerate().take(n) {
        let mut bad = Vec::new();
        for (ti, tri) in triangles.iter().enumerate() {
            let (Some(a), Some(b), Some(c)) = (pts2.get(tri[0]).copied(), pts2.get(tri[1]).copied(), pts2.get(tri[2]).copied()) else { continue };
            if in_circumcircle(a, b, c, p) {
                bad.push(ti);
            }
        }
        if bad.is_empty() {
            // p coincides with an already-placed point, or falls exactly on an edge under this
            // algorithm's tolerance: skip it rather than corrupt the triangulation.
            continue;
        }

        // Directed edges of every bad triangle; one that doesn't also appear reversed among them
        // bounds the hole, in the orientation that keeps new triangles counter-clockwise.
        let mut edges: Vec<(usize, usize)> = Vec::with_capacity(bad.len() * 3);
        for &ti in &bad {
            let Some(tri) = triangles.get(ti) else { continue };
            edges.push((tri[0], tri[1]));
            edges.push((tri[1], tri[2]));
            edges.push((tri[2], tri[0]));
        }
        let boundary: Vec<(usize, usize)> = edges.iter().copied().filter(|&(u, v)| !edges.iter().any(|&(x, y)| x == v && y == u)).collect();

        let mut kept = Vec::with_capacity(triangles.len());
        for (ti, tri) in triangles.into_iter().enumerate() {
            if !bad.contains(&ti) {
                kept.push(tri);
            }
        }
        triangles = kept;
        for (u, v) in boundary {
            triangles.push([u, v, i]);
        }
    }

    triangles.retain(|t| t[0] < n && t[1] < n && t[2] < n);
    if triangles.is_empty() {
        return Err(TinError::Degenerate);
    }

    Ok(Tin { points: points.to_vec(), triangles })
}

/// One straight segment of a contour line at a single elevation, crossing one triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourSegment {
    pub elevation: f64,
    pub start: Vec2,
    pub end: Vec2,
}

/// Extracts contour segments for every multiple of `interval` that crosses the TIN's elevation
/// range. Returns an empty vector (not an error) for a non-positive interval, an interval coarser
/// than the whole elevation range, or a TIN with no elevation range (no points) - none of those
/// are malformed input, they just produce no contours. Segments are emitted per triangle and not
/// chained into polylines; each carries its elevation so a caller can group and chain them.
pub fn contours(tin: &Tin, interval: f64) -> Vec<ContourSegment> {
    let mut out = Vec::new();
    if !interval.is_finite() || interval <= 0.0 {
        return out;
    }
    let Some((lo, hi)) = tin.elevation_range() else { return out };
    if !lo.is_finite() || !hi.is_finite() {
        return out;
    }

    let first_level = (lo / interval).ceil() * interval;
    let mut level = first_level;
    // Cap the number of levels so a pathological (near-zero) interval can't loop unboundedly on
    // hostile input; this is generous for any real terrain (thousands of contour levels).
    const MAX_LEVELS: usize = 100_000;
    let mut emitted_levels = 0usize;
    while level <= hi + 1e-9 && emitted_levels < MAX_LEVELS {
        for tri in &tin.triangles {
            if let Some(seg) = triangle_crossing(tin, tri, level) {
                out.push(seg);
            }
        }
        level += interval;
        emitted_levels += 1;
    }
    out
}

/// The single segment where `level` crosses triangle `tri`'s two "transition" edges, if the
/// level actually lies strictly between the triangle's min and max elevation.
fn triangle_crossing(tin: &Tin, tri: &[usize; 3], level: f64) -> Option<ContourSegment> {
    let pts: [Vec3; 3] = [*tin.points.get(tri[0])?, *tin.points.get(tri[1])?, *tin.points.get(tri[2])?];
    let zs = [pts[0].z, pts[1].z, pts[2].z];
    let (zmin, zmax) = (zs[0].min(zs[1]).min(zs[2]), zs[0].max(zs[1]).max(zs[2]));
    if level <= zmin || level >= zmax {
        return None;
    }

    let mut crossings: Vec<Vec2> = Vec::with_capacity(2);
    for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
        let (za, zb) = (zs[i], zs[j]);
        let crosses = (za < level) != (zb < level);
        if !crosses {
            continue;
        }
        let denom = zb - za;
        if denom.abs() < 1e-12 {
            continue;
        }
        let t = (level - za) / denom;
        let t = t.clamp(0.0, 1.0);
        let a = Vec2::new(pts[i].x, pts[i].y);
        let b = Vec2::new(pts[j].x, pts[j].y);
        crossings.push(a + (b - a) * t);
    }

    match crossings.as_slice() {
        [start, end] => Some(ContourSegment { elevation: level, start: *start, end: *end }),
        // A level landing exactly on a vertex, or other degenerate edge case: skip rather than
        // guess, this is never hostile-input-unsafe since we only ever read, never panic.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Vec3> {
        vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0), Vec3::new(0.0, 10.0, 10.0)]
    }

    #[test]
    fn rejects_too_few_points() {
        assert_eq!(triangulate(&[Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)]), Err(TinError::NotEnoughPoints(2)));
    }

    #[test]
    fn rejects_non_finite_coordinates() {
        let pts = [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(f64::NAN, 1.0, 0.0)];
        assert_eq!(triangulate(&pts), Err(TinError::NonFinitePoint { index: 2 }));
    }

    #[test]
    fn rejects_collinear_points() {
        let pts = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0)];
        assert_eq!(triangulate(&pts), Err(TinError::Degenerate));
    }

    #[test]
    fn rejects_too_many_points() {
        let pts = vec![Vec3::ZERO; MAX_TIN_POINTS + 1];
        assert_eq!(triangulate(&pts), Err(TinError::TooManyPoints(MAX_TIN_POINTS + 1)));
    }

    #[test]
    fn a_square_makes_two_triangles() {
        let tin = triangulate(&square()).unwrap_or_default();
        assert_eq!(tin.triangles.len(), 2);
    }

    #[test]
    fn elevation_interpolates_a_plane_exactly() {
        let tin = triangulate(&square()).unwrap_or_default();
        assert_eq!(tin.elevation_at(5.0, 5.0), Some(5.0));
        assert_eq!(tin.elevation_at(2.5, 7.5), Some(7.5));
        assert_eq!(tin.elevation_at(11.0, 5.0), None);
    }

    #[test]
    fn a_jittered_grid_is_delaunay() {
        let mut pts = Vec::new();
        for i in 0..12 {
            for j in 0..12 {
                let jit = ((i * 7 + j * 13) % 10) as f64 * 0.01;
                pts.push(Vec3::new(i as f64 + jit, j as f64 - jit, (i + j) as f64));
            }
        }
        let tin = triangulate(&pts).unwrap_or_default();
        assert!(!tin.triangles.is_empty());
        for t in &tin.triangles {
            let p = |k: usize| tin.points.get(k).map(|v| Vec2::new(v.x, v.y)).unwrap_or(Vec2::ZERO);
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            for q in &tin.points {
                assert!(!in_circumcircle(a, b, c, Vec2::new(q.x, q.y)), "triangle {t:?} has a point inside its circumcircle");
            }
        }
    }

    #[test]
    fn contours_of_a_plane_are_straight_and_skip_the_range_ends() {
        let tin = triangulate(&square()).unwrap_or_default();
        let segs = contours(&tin, 2.5);
        let mut levels: Vec<f64> = segs.iter().map(|s| s.elevation).collect();
        levels.dedup();
        assert_eq!(levels, vec![2.5, 5.0, 7.5]);
        for s in &segs {
            assert!((s.start.y - s.elevation).abs() < 1e-9 && (s.end.y - s.elevation).abs() < 1e-9, "{s:?}");
        }
    }

    #[test]
    fn contours_refuse_a_bad_interval() {
        let tin = triangulate(&square()).unwrap_or_default();
        assert!(contours(&tin, 0.0).is_empty());
        assert!(contours(&tin, f64::NAN).is_empty());
    }
}
