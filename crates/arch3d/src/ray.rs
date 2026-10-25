//! Ray / triangle crossing primitive for the upward CPU probe.
//!
//! The previous probe tested containment as "inside the XY projection and
//! `p.z` in the triangle's min/max z". That counts vertical wall triangles but
//! *never* a horizontal face (a single `z` value), so a point inside a floor
//! slab was reported as empty space. This module replaces that test with a
//! true Möller–Trumbore ray/triangle intersection specialised to a +Z ray.
//!
//! Pure geometry: no allocation in the hot path, no `unwrap`/`panic`, finite
//! checks up front so hostile input (NaN/Inf) yields `false` and never panics.

use cadcraft_geom::Vec3;

/// Tolerance used for coplanarity / inside tests.
pub(crate) const RAY_EPS: f64 = 1e-9;

/// Whether every coordinate of the four points is finite.
fn all_finite3(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    p.is_finite() && a.is_finite() && b.is_finite() && c.is_finite()
}

/// 2D cross product (scalar).
fn cross2(x: Vec3, y: Vec3) -> f64 {
    x.x * y.y - x.y * y.x
}

/// Cross product of two 3D vectors.
fn cross3(x: Vec3, y: Vec3) -> Vec3 {
    Vec3::new(x.y * y.z - x.z * y.y, x.z * y.x - x.x * y.z, x.x * y.y - x.y * y.x)
}

/// Whether the point is exactly on the 2D segment from `v` to `w` (within eps).
fn on_segment2(p: Vec3, v: Vec3, w: Vec3) -> bool {
    let dx = w.x - v.x;
    let dy = w.y - v.y;
    let cross = dx * (p.y - v.y) - dy * (p.x - v.x);
    let seg = dx.abs() + dy.abs();
    if cross.abs() > seg * RAY_EPS + RAY_EPS {
        return false;
    }
    (p.x - v.x).abs() <= dx.abs() + RAY_EPS && (p.y - v.y).abs() <= dy.abs() + RAY_EPS
}

/// 2D barycentric sign test, inclusive, with the standard half-open rule for
/// points landing exactly on a shared edge of two triangles of the same face.
fn point_in_triangle2(pp: Vec3, pa: Vec3, pb: Vec3, pc: Vec3) -> bool {
    let d1 = cross2(pb - pa, pp - pa);
    let d2 = cross2(pc - pb, pp - pb);
    let d3 = cross2(pa - pc, pp - pc);

    // Strictly outside: signs disagree.
    let neg = d1 < -RAY_EPS || d2 < -RAY_EPS || d3 < -RAY_EPS;
    let pos = d1 > RAY_EPS || d2 > RAY_EPS || d3 > RAY_EPS;
    if neg && pos {
        return false;
    }

    // Strictly inside (no boundary edge hit) → in.
    if !on_segment2(pp, pa, pb) && !on_segment2(pp, pb, pc) && !on_segment2(pp, pc, pa) {
        return true;
    }

    // Exactly on a shared edge → half-open: count only the triangle whose edge
    // is oriented "upward" (v_i.y < v_j.y). Two same-face triangles sharing the
    // diagonal are oppositely wound, so exactly one counts.
    if on_segment2(pp, pa, pb) {
        return pa.y < pb.y;
    }
    if on_segment2(pp, pb, pc) {
        return pb.y < pc.y;
    }
    if on_segment2(pp, pc, pa) {
        return pc.y < pa.y;
    }

    false
}

/// True iff the half-line `o + t*(0,0,1)` for `t > 0` crosses triangle `abc`.
///
/// Möller–Trumbore specialised to dir = +Z:
/// - `n = (b-a) x (c-a)`. Vertical triangles (`|n.z| < RAY_EPS`) are never
///   crossings (the ray is parallel to / in their plane) → 0 contribution.
/// - Plane hit `t = n · (a-o) / n.z` must be `> RAY_EPS` (ahead of origin).
/// - The hit point must be coplanar-inside the triangle.
pub(crate) fn crosses_upward(o: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    if !all_finite3(o, a, b, c) {
        return false;
    }
    let n = cross3(b - a, c - a);
    if n.z.abs() < RAY_EPS {
        return false;
    }
    let t = n.dot(a - o) / n.z;
    if !(t > RAY_EPS) {
        return false;
    }
    let h = Vec3::new(o.x, o.y, o.z + t);
    point_in_triangle(h, a, b, c)
}

/// Coplanar inside test: project the point and triangle onto the plane
/// perpendicular to the axis with the largest normal component, then run three
/// sign-of-cross-product tests (inclusive, allowing zero).
pub(crate) fn point_in_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    if !all_finite3(p, a, b, c) {
        return false;
    }
    let n = cross3(b - a, c - a);
    // Degenerate (zero-area) triangle.
    if n.x.abs() < RAY_EPS && n.y.abs() < RAY_EPS && n.z.abs() < RAY_EPS {
        return false;
    }
    // Drop the axis with the largest |component|; project onto the other two.
    // Horizontal triangles → XY; near-vertical → XZ or YZ.
    let nabs = [n.x.abs(), n.y.abs(), n.z.abs()];
    let drop = if nabs[1] >= nabs[0] && nabs[1] >= nabs[2] {
        1
    } else if nabs[2] >= nabs[0] && nabs[2] >= nabs[1] {
        2
    } else {
        0
    };
    let map = |v: Vec3| match drop {
        0 => Vec3::new(v.y, v.z, 0.0),
        1 => Vec3::new(v.x, v.z, 0.0),
        _ => Vec3::new(v.x, v.y, 0.0),
    };
    point_in_triangle2(map(p), map(a), map(b), map(c))
}

/// Count upward (+Z) crossings of `tris` by the ray starting at `o`.
///
/// Parity (`crossings & 1`) gives even-odd inside/outside; we return the raw
/// count so the caller can keep it observable.
pub(crate) fn crossings_upward(o: Vec3, tris: &[[Vec3; 3]]) -> usize {
    tris.iter().filter(|t| crosses_upward(o, t[0], t[1], t[2])).count()
}

/// Axis-aligned closed box, 12 triangles, consistent outward winding, each
/// face split by the diagonal from its lowest-x (then lowest-y) corner.
///
/// Used by tests and by the `p3-probe-parts-and-mesh` segment.
pub(crate) fn box_tris(min: Vec3, max: Vec3) -> Vec<[Vec3; 3]> {
    let x0 = min.x;
    let x1 = max.x;
    let y0 = min.y;
    let y1 = max.y;
    let z0 = min.z;
    let z1 = max.z;
    let c000 = Vec3::new(x0, y0, z0);
    let c100 = Vec3::new(x1, y0, z0);
    let c110 = Vec3::new(x1, y1, z0);
    let c010 = Vec3::new(x0, y1, z0);
    let c001 = Vec3::new(x0, y0, z1);
    let c101 = Vec3::new(x1, y0, z1);
    let c111 = Vec3::new(x1, y1, z1);
    let c011 = Vec3::new(x0, y1, z1);
    let mut tris = Vec::with_capacity(12);
    // Top (normal +z), diagonal c001 -> c111.
    tris.push([c001, c101, c111]);
    tris.push([c001, c111, c011]);
    // Bottom (normal -z), diagonal c000 -> c110.
    tris.push([c000, c010, c110]);
    tris.push([c000, c110, c100]);
    // Front (y0, normal -y), diagonal c000 -> c101.
    tris.push([c000, c100, c101]);
    tris.push([c000, c101, c001]);
    // Back (y1, normal +y), diagonal c010 -> c111.
    tris.push([c010, c011, c111]);
    tris.push([c010, c111, c110]);
    // Left (x0, normal -x), diagonal c000 -> c011.
    tris.push([c000, c001, c011]);
    tris.push([c000, c011, c010]);
    // Right (x1, normal +x), diagonal c100 -> c111.
    tris.push([c100, c110, c111]);
    tris.push([c100, c111, c101]);
    tris
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossings_upward_counts_a_closed_box_from_inside() {
        let tris = box_tris(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 3.0));
        // Point inside the box on the +z ray: odd crossing count (the top
        // face only) → "inside" by the even-odd rule.
        let n = crossings_upward(Vec3::new(1.0, 1.0, 1.0), &tris);
        assert_eq!(n, 1, "inside box should give 1 crossing, got {n}");
    }

    #[test]
    fn crossings_upward_is_zero_below_and_two_above_a_box() {
        let tris = box_tris(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 3.0));
        // A point below the box: the +z ray passes through the bottom and top
        // faces → 2 (even). A point above the box: the ray misses → 0 (even).
        let below = crossings_upward(Vec3::new(1.0, 1.0, -1.0), &tris);
        let above = crossings_upward(Vec3::new(1.0, 1.0, 4.0), &tris);
        assert_eq!(below, 2, "below the box should be 2, got {below}");
        assert_eq!(above, 0, "above the box should be 0, got {above}");
    }

    #[test]
    fn horizontal_triangle_above_the_point_counts() {
        // Up-facing triangle at z=3; point below at z=1 → 1 crossing.
        let o = Vec3::new(0.5, 0.5, 1.0);
        let a = Vec3::new(0.0, 0.0, 3.0);
        let b = Vec3::new(2.0, 0.0, 3.0);
        let c = Vec3::new(1.0, 2.0, 3.0);
        assert!(crosses_upward(o, a, b, c));
    }

    #[test]
    fn horizontal_triangle_below_the_point_does_not_count() {
        // Triangle below the point (z=1); ray from z=3 going up misses it → 0.
        let o = Vec3::new(0.5, 0.5, 3.0);
        let a = Vec3::new(0.0, 0.0, 1.0);
        let b = Vec3::new(2.0, 0.0, 1.0);
        let c = Vec3::new(1.0, 2.0, 1.0);
        assert!(!crosses_upward(o, a, b, c));
    }

    #[test]
    fn vertical_triangle_never_counts() {
        // A wall triangle (normal along +x); the +z ray is parallel to it.
        let o = Vec3::new(1.0, 1.0, 1.0);
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 2.0, 0.0);
        let c = Vec3::new(1.0, 0.0, 3.0);
        assert!(!crosses_upward(o, a, b, c));
    }

    #[test]
    fn ray_through_a_face_diagonal_counts_one_crossing() {
        // A horizontal (top) face split by a diagonal; a ray through the exact
        // midpoint of the diagonal must count exactly ONE crossing for that face.
        // Top face at z=3, diagonal (0,0)->(2,2); the ray at x=1,y=1 is on it.
        let z = 3.0;
        let p00 = Vec3::new(0.0, 0.0, z);
        let p10 = Vec3::new(2.0, 0.0, z);
        let p11 = Vec3::new(2.0, 2.0, z);
        let p01 = Vec3::new(0.0, 2.0, z);
        let ts = vec![[p00, p10, p11], [p00, p11, p01]];
        let o = Vec3::new(1.0, 1.0, 1.0);
        let n = crossings_upward(o, &ts[..]);
        assert_eq!(n, 1, "diagonal of one face must be 1 crossing, got {n}");
    }

    #[test]
    fn crossings_survive_nan_and_infinite_inputs() {
        let a = Vec3::new(0.0, 0.0, 3.0);
        let b = Vec3::new(2.0, 0.0, 3.0);
        let c = Vec3::new(1.0, 2.0, 3.0);
        // NaN in the ray origin → false, no panic.
        let o = Vec3::new(f64::NAN, 1.0, 1.0);
        assert!(!crosses_upward(o, a, b, c));
        let ts = vec![[a, b, c]];
        assert_eq!(crossings_upward(o, &ts[..]), 0);
        // NaN/Inf in triangle vertices → false.
        let bad = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        assert!(!crosses_upward(Vec3::new(1.0, 1.0, 1.0), bad, b, c));
        // Degenerate (repeated vertices) → false.
        assert!(!crosses_upward(Vec3::new(1.0, 1.0, 1.0), a, a, b));
    }
}
