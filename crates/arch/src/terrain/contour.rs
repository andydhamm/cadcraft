//! Contours of the site mesh at minor and major intervals. Filled in by
//! `p9-arch-terrain-contours`.
//!
//! Every `k·minor` elevation strictly between the mesh's lowest and highest corner
//! `z` produces contour segments, extracted per triangle exactly as
//! [`cadcraft_geom::tin::contours`] does and chained into polylines. The `minor` set
//! is every level; the `major` set is only the levels whose elevation is a whole
//! multiple of `major` (and therefore also present in `minor`).

use cadcraft_doc::arch::{MAX_CONTOUR_LEVELS, MIN_CONTOUR_INTERVAL};
use cadcraft_geom::Vec2;
use cadcraft_geom::tin::ContourSegment;
use cadcraft_geom::tin_ops::{ContourLine, chain_contours};

use super::mesh::SiteMesh;

/// The contours of a [`SiteMesh`], split into `minor` (one polyline per level) and
/// `major` (only the whole-multiple levels, which are also present in `minor`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteContours {
    pub minor: Vec<ContourLine>,
    pub major: Vec<ContourLine>,
}

/// The lowest and highest finite corner elevations of `mesh`, else `None`.
fn elevation_range(mesh: &SiteMesh) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for tri in &mesh.z {
        for z in tri {
            if z.is_finite() {
                lo = lo.min(*z);
                hi = hi.max(*z);
            }
        }
    }
    if lo.is_finite() && hi.is_finite() { Some((lo, hi)) } else { None }
}

/// The single segment where `level` crosses triangle `tri_index`'s two "transition" edges, or
/// `None` when `level` is not strictly between the triangle's corner elevations. Mirrors the
/// reference `cadcraft_geom::tin` crossing exactly: a corner counts as below when `z < level`,
/// and the crossing is interpolated on the two crossing edges using this triangle's own corner
/// `z` values (so a vertical step carries per-side elevations and no level between the sides
/// crosses it).
fn triangle_crossing(mesh: &SiteMesh, tri_index: usize, level: f64) -> Option<ContourSegment> {
    if !level.is_finite() {
        return None;
    }
    let (tri, zs) = (mesh.triangles.get(tri_index)?, mesh.z.get(tri_index)?);
    let zmin = zs[0].min(zs[1]).min(zs[2]);
    let zmax = zs[0].max(zs[1]).max(zs[2]);
    if level <= zmin || level >= zmax {
        return None;
    }
    let pts: [Vec2; 3] = [mesh.xy.get(tri[0])?, mesh.xy.get(tri[1])?, mesh.xy.get(tri[2])?];
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
        let t = ((level - za) / denom).clamp(0.0, 1.0);
        crossings.push(pts[i] + (pts[j] - pts[i]) * t);
    }
    match crossings.as_slice() {
        [start, end] => Some(ContourSegment { elevation: level, start: *start, end: *end }),
        // Level landing on a vertex or otherwise degenerate: skip rather than guess.
        _ => None,
    }
}

/// Whether `level` is a major contour: `level / major` within 1e-9 of an integer.
fn is_major_level(level: f64, major: f64) -> bool {
    if !level.is_finite() || !major.is_finite() || major <= 0.0 {
        return false;
    }
    let v = level / major;
    if !v.is_finite() {
        return false;
    }
    (v - v.round()).abs() <= 1e-9
}

/// The contours of `mesh`.
pub fn site_contours(mesh: &SiteMesh, minor: f64, major: f64) -> SiteContours {
    // Hostile input: refuse non-finite values, intervals below the contour minimum, and a
    // major finer than the minor.
    if minor.is_finite().is(false) || major.is_finite().is(false) || minor < MIN_CONTOUR_INTERVAL || major < MIN_CONTOUR_INTERVAL || major < minor {
        return SiteContours::default();
    }

    let Some((lo, hi)) = elevation_range(mesh) else {
        return SiteContours::default();
    };

    // Candidate levels: every k·minor strictly above `lo`, up to `hi`; endpoints contribute
    // no segment (they are skipped in `triangle_crossing`). The count is capped — more than
    // MAX_CONTOUR_LEVELS levels returns empty, so a pathological near-zero interval can't
    // allocate unboundedly on hostile input.
    let mut levels: Vec<f64> = Vec::new();
    let mut level = (lo / minor).floor() * minor + minor;
    const MAX_ITER: usize = MAX_CONTOUR_LEVELS * 4;
    for _ in 0..MAX_ITER {
        if !level.is_finite() || level >= hi {
            break;
        }
        if levels.len() >= MAX_CONTOUR_LEVELS {
            return SiteContours::default();
        }
        levels.push(level);
        level += minor;
    }
    if levels.is_empty() {
        return SiteContours::default();
    }

    let mut minor_lines: Vec<ContourLine> = Vec::new();
    let mut major_lines: Vec<ContourLine> = Vec::new();
    for &level in &levels {
        let mut segs: Vec<ContourSegment> = Vec::with_capacity(mesh.triangles.len() * 2);
        for (i, _tri) in mesh.triangles.iter().enumerate() {
            if let Some(seg) = triangle_crossing(mesh, i, level) {
                segs.push(seg);
            }
        }
        if segs.is_empty() {
            continue;
        }
        let lines = chain_contours(&segs);
        if is_major_level(level, major) {
            major_lines.extend(lines.clone());
        }
        major_lines.extend(lines);
        minor_lines.extend(lines);
    }

    SiteContours { minor: minor_lines, major: major_lines }
}

/// A contour's label: its elevation with two decimals ("1.00", "-0.50"), where a rounding to
/// "-0.00" is printed as "0.00".
pub fn label_text(elevation: f64) -> String {
    let s = format!("{elevation:.2}");
    if s == "-0.00" { "0.00".to_string() } else { s }
}

/// Where a contour's label goes: the point at half the polyline's length. Closed lines include
/// the closing edge (last point back to the first); fewer than two points has no halfway point.
pub fn label_point(line: &ContourLine) -> Option<Vec2> {
    let pts = &line.points;
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len();
    let num_segs = if line.closed { n } else { n - 1 };
    let lens: Vec<f64> = (0..num_segs)
        .map(|i| {
            let a = &pts[i];
            let b = if line.closed { &pts[(i + 1) % n] } else { &pts[i + 1] };
            b.dist(*a)
        })
        .collect();
    let total: f64 = lens.iter().sum();
    if !total.is_finite() || total <= 1e-12 {
        return Some(pts[0]);
    }
    let mut target = total / 2.0;
    for (i, &len) in lens.iter().enumerate() {
        if target <= len {
            if len <= 1e-12 {
                return Some(pts[i]);
            }
            let t = (target / len).clamp(0.0, 1.0);
            let a = pts[i];
            let b = if line.closed { pts[(i + 1) % n] } else { pts[i + 1] };
            return Some(a + (b - a) * t);
        }
        target -= len;
    }
    // The target always lands in the last segment; safety net for floating-point residue.
    Some(pts[n - 1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::mesh::SiteMesh;
    use cadcraft_geom::Vec2;

    /// A 40 m × 30 m site with `z = 0.05 x`, triangulated into two counter-clockwise triangles:
    /// `z` runs from 0 (west) to 2 (east), so the minor 0.25 / major 1.0 contour set has seven
    /// minor lines and one major line.
    fn sloped_mesh() -> SiteMesh {
        SiteMesh {
            xy: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            z: vec![[0.0, 2.0, 2.0], [0.0, 2.0, 0.0]],
        }
    }

    /// A 10 m × 10 m site with a vertical step along the diagonal: the lower triangle is sloped
    /// (z 0 → 2 on the east edge) and the upper is flat at z = 2. A contour never crosses the
    /// flat top, so every line terminates on the step edge.
    fn step_mesh() -> SiteMesh {
        SiteMesh {
            xy: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            z: vec![[0.0, 2.0, 2.0], [2.0, 2.0, 2.0]],
        }
    }

    #[test]
    fn contours_of_the_sloped_site_are_seven_minor_one_major() {
        let contours = site_contours(&sloped_mesh(), 0.25, 1.0);
        assert_eq!(contours.minor.len(), 7, "minor lines: {}", contours.minor.len());
        assert_eq!(contours.major.len(), 1, "major lines: {}", contours.major.len());
        assert!((contours.major[0].elevation - 1.0).abs() < 1e-9);
        let mut got: Vec<f64> = contours.minor.iter().map(|l| l.elevation).collect();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let expected = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75];
        for (g, e) in got.iter().zip(expected) {
            assert!((*g - e).abs() < 1e-9, "minor elevation {g}");
        }
        assert!(contours.minor.iter().any(|l| (l.elevation - 1.0).abs() < 1e-9));
    }

    #[test]
    fn contours_skip_the_range_ends() {
        let contours = site_contours(&sloped_mesh(), 0.25, 1.0);
        for line in &contours.minor {
            // No line at the lowest (z=0) or highest (z=2) corner elevation.
            assert!((line.elevation - 0.0).abs() > 1e-9, "low endpoint {}", line.elevation);
            assert!((line.elevation - 2.0).abs() > 1e-9, "high endpoint {}", line.elevation);
        }
    }

    #[test]
    fn contours_chain_across_triangles() {
        let contours = site_contours(&sloped_mesh(), 0.25, 1.0);
        for line in &contours.minor {
            // Each level crosses both triangles; the shared-knot segments chain into one
            // polyline of three knots (not two disconnected lines).
            assert_eq!(line.points.len(), 3, "line at {:.3}", line.elevation);
        }
    }

    #[test]
    fn contours_stop_at_a_step() {
        let contours = site_contours(&step_mesh(), 0.25, 1.0);
        assert_eq!(contours.minor.len(), 7);
        for line in &contours.minor {
            // The flat top (z=2) crosses no level, so every line's far end lands on the step
            // edge x=y (the (0,0)-(10,10) diagonal), never past it.
            assert!(
                (line.points[0].x - line.points[0].y).abs() < 1e-9 || (line.points[1].x - line.points[1].y).abs() < 1e-9,
                "no endpoint on step edge {line:?}"
            );
        }
    }

    #[test]
    fn label_text_has_two_decimals() {
        assert_eq!(label_text(1.0), "1.00");
        assert_eq!(label_text(-0.5), "-0.50");
        assert_eq!(label_text(0.0), "0.00");
        assert_eq!(label_text(-0.0), "0.00");
        assert_eq!(label_text(-0.001), "0.00");
        assert_eq!(label_text(0.004), "0.00");
        assert_eq!(label_text(1.005), "1.01");
        assert_eq!(label_text(12.345), "12.35");
    }

    #[test]
    fn label_point_is_halfway_along() {
        // Open line, two points: (0,0)-(2,0); total length 2, halfway at (1,0).
        let open_2 = ContourLine { elevation: 1.0, points: vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0)], closed: false };
        assert_eq!(label_point(&open_2), Some(Vec2::new(1.0, 0.0)));

        // Open line, uneven segments: (0,0)-(1,0)-(4,0); lengths 1 and 3, halfway (2,0).
        let open_3 = ContourLine { elevation: 1.0, points: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(4.0, 0.0)], closed: false };
        assert_eq!(label_point(&open_3), Some(Vec2::new(2.0, 0.0)));

        // Closed line includes the closing edge: (0,0)-(4,0)-(0,6); arc halfway falls inside
        // the hypotenuse, at roughly (1.445, 3.832).
        let closed = ContourLine { elevation: 1.0, points: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(0.0, 6.0)], closed: true };
        let p = label_point(&closed).expect("closed line has a halfway point");
        assert!((p.x - 1.445).abs() < 1e-3 && (p.y - 3.832).abs() < 1e-3, "{p:?}");

        // Fewer than two points: None.
        let short = ContourLine { elevation: 1.0, points: vec![Vec2::new(0.0, 0.0)], closed: false };
        assert_eq!(label_point(&short), None);
    }
}
