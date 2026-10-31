//! The site mesh: the modified surface triangulated with every breakline as an edge and an
//! elevation per triangle corner, so a vertical step is two triangles' corners at one point with
//! different elevations. Filled in by `p9-arch-terrain-mesh`.

use std::cmp::Ordering;
use std::f64::consts::PI;

use crate::poly::offset_polygon;
use cadcraft_geom::{Vec2, Vec3, point_in_polygon};

use super::TerrainError;
use super::pad::{PadKind, PadRegion};
use super::surface::SiteSurface;

/// Most triangles a `SiteMesh` may hold. More than this returns
/// `Err(TerrainError("terrain mesh too large".into()))`.
const MAX_MESH_TRIANGLES: usize = 200_000;
/// Vertices closer than this are considered the same vertex.
const DEDUP_EPS: f64 = 1e-9;
/// Triangles with area below this are dropped (degenerate).
const MIN_TRIANGLE_AREA: f64 = 1e-12;
/// Spacing of the rings, and of the points along each ring, where a pad's slope runs back to the ground.
const PAD_SLOPE_STEP: f64 = 0.25;
/// At most this many slope rings per pad (the step widens on a tall site).
const MAX_PAD_SLOPE_RINGS: usize = 32;
/// At most this many slope points over all pads.
const MAX_PAD_SLOPE_POINTS: usize = 20_000;

/// Triangles in plan with an elevation per corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteMesh {
    pub xy: Vec<Vec2>,
    /// Counter-clockwise corner indices into `xy`.
    pub triangles: Vec<[usize; 3]>,
    /// The modified elevation at each triangle's three corners, seen from inside that triangle.
    pub z: Vec<[f64; 3]>,
}

impl SiteMesh {
    /// The smallest corner z, `None` when empty or every z is non-finite.
    pub fn lowest(&self) -> Option<f64> {
        let mut best: Option<f64> = None;
        for tri in &self.z {
            for z in tri {
                if z.is_finite() {
                    best = Some(match best {
                        Some(b) => b.min(*z),
                        None => *z,
                    });
                }
            }
        }
        best
    }
}

/// Even-odd containment; a point within `DEDUP_EPS` of the ring counts as inside.
fn inside_or_on(ring: &[Vec2], p: Vec2) -> bool {
    ring.len() >= 3 && p.is_finite() && (point_in_polygon(ring, p) || ring_dist(p, ring) <= DEDUP_EPS)
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 <= 1e-18 {
        return p.dist(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

fn ring_dist(p: Vec2, ring: &[Vec2]) -> f64 {
    let n = ring.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        if let Some(b) = ring.get((i + 1) % n) {
            best = best.min(seg_dist(p, ring[i], *b));
        }
    }
    best
}

/// Clip segment `a`-`b` to the inside of `ring`. Returns the inside sub-segments.
fn clip_to_poly(a: Vec2, b: Vec2, ring: &[Vec2]) -> Vec<[Vec2; 2]> {
    if ring.len() < 3 || !a.is_finite() || !b.is_finite() {
        return vec![];
    }
    let ab = b - a;
    let mut ts: Vec<f64> = vec![0.0, 1.0];
    for i in 0..ring.len() {
        let p = ring[i];
        let q = ring[(i + 1) % ring.len()];
        if !p.is_finite() || !q.is_finite() {
            continue;
        }
        let pq = q - p;
        let d = ab.cross(pq);
        if d.abs() < 1e-14 {
            continue;
        }
        let ap = p - a;
        let t = ap.cross(pq) / d;
        if t >= -1e-9 && t <= 1.0 + 1e-9 {
            ts.push(t.clamp(0.0, 1.0));
        }
    }
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(Ordering::Equal));
    let mut deduped: Vec<f64> = Vec::new();
    for t in ts {
        if deduped.is_empty() || (t - deduped.last().copied().unwrap_or(0.0)).abs() > DEDUP_EPS {
            deduped.push(t);
        }
    }
    let mut result: Vec<[Vec2; 2]> = vec![];
    let n = deduped.len();
    for i in 0..n.saturating_sub(1) {
        let dt = deduped[i + 1] - deduped[i];
        if dt < 1e-12 {
            continue;
        }
        let t_mid = (deduped[i] + deduped[i + 1]) / 2.0;
        if inside_or_on(ring, a + ab * t_mid) {
            let p0 = a + ab * deduped[i];
            let p1 = a + ab * deduped[i + 1];
            if p0.dist(p1) > DEDUP_EPS {
                result.push([p0, p1]);
            }
        }
    }
    result
}

/// Sort by (x, y) then remove adjacent points closer than `eps`.
fn dedup_pts(raw: Vec<Vec2>, eps: f64) -> Vec<Vec2> {
    let mut pts = raw;
    pts.sort_by(|a, b| {
        let xo = a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal);
        if xo != Ordering::Equal {
            return xo;
        }
        a.y.partial_cmp(&b.y).unwrap_or(Ordering::Equal)
    });
    let mut out: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in &pts {
        if out.is_empty() || out.last().map_or(false, |q| p.dist(*q) > eps) {
            out.push(p);
        }
    }
    out
}

/// Linear scan for the index of `target` in `pts` (within `DEDUP_EPS`).
fn point_in_list(pts: &[Vec2], target: Vec2) -> Option<usize> {
    pts.iter().position(|p| p.dist(target) <= DEDUP_EPS)
}

/// Points on the rings where a pad's 1:2 slope runs from its margin back to the ground, `step`
/// apart along each ring, so the mesh follows the slope instead of one triangle spanning from the
/// margin ring to the boundary. Points only: as constraints the rings cross the original TIN's
/// edges at hundreds of points the triangulation cannot keep apart, and the mesh tears.
fn pad_slope_points(surf: &SiteSurface, region: &PadRegion, budget: &mut usize) -> Vec<Vec2> {
    let mut out = Vec::new();
    let e = region.elevation;
    if region.kind != PadKind::Pad || !e.is_finite() || region.ring.len() < 3 || region.ring.iter().any(|p| !p.is_finite()) {
        return out;
    }
    let Some((mut lo, mut hi)) = surf.original().elevation_range() else { return out };
    for m in surf.modifiers() {
        match &m.kind {
            cadcraft_doc::arch::ModifierKind::Flat { elevation, .. } if elevation.is_finite() => {
                lo = lo.min(*elevation);
                hi = hi.max(*elevation);
            }
            cadcraft_doc::arch::ModifierKind::Hill { height, .. } if height.is_finite() => {
                hi += height.max(0.0);
                lo += height.min(0.0);
            }
            _ => {}
        }
    }
    let s_max = (hi - e).max(e - lo).max(0.0) / cadcraft_doc::arch::PAD_SLOPE;
    if !s_max.is_finite() || s_max <= 1e-9 {
        return out;
    }
    let n = ((s_max / PAD_SLOPE_STEP).ceil() as usize).clamp(1, MAX_PAD_SLOPE_RINGS);
    let step = s_max / n as f64;
    let m = if region.margin.is_finite() && region.margin > 0.0 { region.margin } else { 0.0 };
    for k in 1..=n {
        let Some(ring) = offset_polygon(&region.ring, &vec![m + step * k as f64; region.ring.len()], true) else { continue };
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if !a.is_finite() || !b.is_finite() {
                continue;
            }
            let pieces = ((a.dist(b) / step.min(1.0).max(PAD_SLOPE_STEP)).ceil() as usize).clamp(1, 256);
            for j in 0..pieces {
                if *budget == 0 {
                    return out;
                }
                *budget -= 1;
                out.push(a + (b - a) * (j as f64 / pieces as f64));
            }
        }
    }
    out
}

/// Build the site mesh of `surf`.
pub fn site_mesh(surf: &SiteSurface) -> Result<SiteMesh, TerrainError> {
    let boundary = surf.boundary().to_vec();
    if boundary.len() < 3 {
        return Err(TerrainError("boundary needs 3+ points".into()));
    }
    let original = surf.original();

    // --- 1. Raw points ---
    let mut raw_pts: Vec<Vec2> = Vec::new();

    // Original TIN vertices used by a triangle.
    for tri in &original.triangles {
        for &i in tri {
            if let Some(p) = original.points.get(i) {
                if p.x.is_finite() && p.y.is_finite() {
                    raw_pts.push(Vec2::new(p.x, p.y));
                }
            }
        }
    }

    // Boundary.
    for &p in &boundary {
        if p.is_finite() {
            raw_pts.push(p);
        }
    }

    // Modifier points.
    for m in surf.modifiers() {
        match &m.kind {
            cadcraft_doc::arch::ModifierKind::Flat { polygon, margin, .. } => {
                for &p in polygon {
                    if p.is_finite() {
                        raw_pts.push(p);
                    }
                }
                if margin.is_finite() && *margin > 0.0 {
                    if let Some(ring) = offset_polygon(polygon, &vec![*margin; polygon.len()], true) {
                        for &p in &ring {
                            if p.is_finite() {
                                raw_pts.push(p);
                            }
                        }
                    }
                }
            }
            cadcraft_doc::arch::ModifierKind::Path { polygon, .. } => {
                for &p in polygon {
                    if p.is_finite() {
                        raw_pts.push(p);
                    }
                }
            }
            cadcraft_doc::arch::ModifierKind::Hill { center, radius, .. } => {
                if center.is_finite() && radius.is_finite() && *radius > 0.0 {
                    raw_pts.push(*center);
                    for k in 1..=6 {
                        let r = radius * (k as f64) / 6.0;
                        for j in 0..24 {
                            let theta = 2.0 * PI * j as f64 / 24.0;
                            let p = Vec2::new(center.x + r * theta.cos(), center.y + r * theta.sin());
                            raw_pts.push(p);
                        }
                    }
                }
            }
        }
    }

    // Pad points.
    let mut slope_budget = MAX_PAD_SLOPE_POINTS;
    for region in surf.pads() {
        for &p in &region.ring {
            if p.is_finite() {
                raw_pts.push(p);
            }
        }
        if region.kind == PadKind::Pad && region.margin.is_finite() && region.margin > 0.0 {
            if let Some(ring) = offset_polygon(&region.ring, &vec![region.margin; region.ring.len()], true) {
                for &p in &ring {
                    if p.is_finite() {
                        raw_pts.push(p);
                    }
                }
            }
        }
        raw_pts.extend(pad_slope_points(surf, region, &mut slope_budget));
    }

    // Breakline endpoints.
    for seg in surf.breaklines() {
        if seg[0].is_finite() {
            raw_pts.push(seg[0]);
        }
        if seg[1].is_finite() {
            raw_pts.push(seg[1]);
        }
    }

    // --- 2. Raw constraint segments (before clipping) ---
    let mut raw_constraints: Vec<[Vec2; 2]> = Vec::new();

    // Boundary edges.
    for i in 0..boundary.len() {
        let a = boundary[i];
        let b = boundary[(i + 1) % boundary.len()];
        if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
            raw_constraints.push([a, b]);
        }
    }

    // Original TIN triangle edges.
    for tri in &original.triangles {
        let edges = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])];
        for (ia, ib) in edges {
            let pa = original.points.get(ia).map(|p| Vec2::new(p.x, p.y));
            let pb = original.points.get(ib).map(|p| Vec2::new(p.x, p.y));
            if let (Some(a), Some(b)) = (pa, pb) {
                if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                    raw_constraints.push([a, b]);
                }
            }
        }
    }

    // Modifier edges.
    for m in surf.modifiers() {
        match &m.kind {
            cadcraft_doc::arch::ModifierKind::Flat { polygon, margin, .. } => {
                for i in 0..polygon.len() {
                    let a = polygon[i];
                    let b = polygon[(i + 1) % polygon.len()];
                    if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                        raw_constraints.push([a, b]);
                    }
                }
                if margin.is_finite() && *margin > 0.0 {
                    if let Some(ring) = offset_polygon(polygon, &vec![*margin; polygon.len()], true) {
                        for i in 0..ring.len() {
                            let a = ring[i];
                            let b = ring[(i + 1) % ring.len()];
                            if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                                raw_constraints.push([a, b]);
                            }
                        }
                    }
                }
            }
            cadcraft_doc::arch::ModifierKind::Path { polygon, .. } => {
                for i in 0..polygon.len() {
                    let a = polygon[i];
                    let b = polygon[(i + 1) % polygon.len()];
                    if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                        raw_constraints.push([a, b]);
                    }
                }
            }
            cadcraft_doc::arch::ModifierKind::Hill { center, radius, .. } => {
                if center.is_finite() && radius.is_finite() && *radius > 0.0 {
                    for k in 1..=6 {
                        let r = radius * (k as f64) / 6.0;
                        for j in 0..24 {
                            let theta = 2.0 * PI * j as f64 / 24.0;
                            let a = Vec2::new(center.x + r * theta.cos(), center.y + r * theta.sin());
                            let theta_next = 2.0 * PI * (j as f64 + 1.0) / 24.0;
                            let b = Vec2::new(center.x + r * theta_next.cos(), center.y + r * theta_next.sin());
                            if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                                raw_constraints.push([a, b]);
                            }
                        }
                    }
                }
            }
        }
    }

    // Pad ring edges, and a pad's margin ring (where the flat apron meets the slope).
    for region in surf.pads() {
        let margin_ring = (region.kind == PadKind::Pad && region.margin.is_finite() && region.margin > 0.0)
            .then(|| offset_polygon(&region.ring, &vec![region.margin; region.ring.len()], true))
            .flatten();
        for ring in std::iter::once(&region.ring).chain(margin_ring.as_ref()) {
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                    raw_constraints.push([a, b]);
                }
            }
        }
    }

    // Breakline edges.
    for seg in surf.breaklines() {
        if seg[0].is_finite() && seg[1].is_finite() && seg[0].dist(seg[1]) > 1e-12 {
            raw_constraints.push([seg[0], seg[1]]);
        }
    }

    // --- 3. Clip raw constraints to the boundary ---
    let mut clipped_constraints: Vec<[Vec2; 2]> = Vec::new();
    for [a, b] in &raw_constraints {
        for sub in clip_to_poly(*a, *b, &boundary) {
            clipped_constraints.push(sub);
            raw_pts.push(sub[0]);
            raw_pts.push(sub[1]);
        }
    }

    // --- 4. Dedup and drop outside-boundary points ---
    let mut pts = dedup_pts(raw_pts, DEDUP_EPS);
    pts.retain(|p| inside_or_on(&boundary, *p));

    if pts.len() < 3 {
        return Ok(SiteMesh::default());
    }

    // --- 5. Convert clipped constraints to index pairs ---
    let mut constraints: Vec<[usize; 2]> = Vec::new();
    for [a, b] in &clipped_constraints {
        if let (Some(ia), Some(ib)) = (point_in_list(&pts, *a), point_in_list(&pts, *b)) {
            if ia != ib {
                constraints.push([ia, ib]);
            }
        }
    }

    // --- 6. Build CDT ---
    let pts3: Vec<Vec3> = pts.iter().map(|p| Vec3::new(p.x, p.y, 0.0)).collect();
    let cdt = cadcraft_geom::cdt::triangulate_constrained(&pts3, &constraints).map_err(|e| TerrainError(e.to_string()))?;
    let mut tin = cdt.tin;

    // --- 7. Retain inside boundary ---
    cadcraft_geom::tin_ops::retain_inside(&mut tin, &boundary);

    // --- 8. Drop degenerate triangles ---
    tin.triangles.retain(|tri| {
        let a = tin.points.get(tri[0]).map(|p| Vec2::new(p.x, p.y)).unwrap_or_default();
        let b = tin.points.get(tri[1]).map(|p| Vec2::new(p.x, p.y)).unwrap_or_default();
        let c = tin.points.get(tri[2]).map(|p| Vec2::new(p.x, p.y)).unwrap_or_default();
        if !a.is_finite() || !b.is_finite() || !c.is_finite() {
            return false;
        }
        ((b - a).cross(c - a)).abs() * 0.5 >= MIN_TRIANGLE_AREA
    });

    if tin.triangles.len() > MAX_MESH_TRIANGLES {
        return Err(TerrainError("terrain mesh too large".into()));
    }

    // --- 9. Assign corner elevations ---
    // `xy` is the triangulation's point set (shared vertices: a step point is one entry
    // referenced by each incident triangle). `triangles` index into it; `z[ti]` holds the
    // elevation of each of triangle `ti`'s corners, computed toward the triangle's centroid so a
    // corner on a step carries its own triangle's side.
    let xy: Vec<Vec2> = tin.points.iter().map(|p| p.xy()).collect();
    let z: Vec<[f64; 3]> = tin
        .triangles
        .iter()
        .map(|tri| {
            let p0 = tin.points[tri[0]];
            let p1 = tin.points[tri[1]];
            let p2 = tin.points[tri[2]];
            let c0 = p0.xy();
            let c1 = p1.xy();
            let c2 = p2.xy();
            let centroid = Vec2::new((c0.x + c1.x + c2.x) / 3.0, (c0.y + c1.y + c2.y) / 3.0);
            [
                surf.modified_toward(c0, centroid).or(surf.modified_at(c0)).unwrap_or(0.0),
                surf.modified_toward(c1, centroid).or(surf.modified_at(c1)).unwrap_or(0.0),
                surf.modified_toward(c2, centroid).or(surf.modified_at(c2)).unwrap_or(0.0),
            ]
        })
        .collect();

    Ok(SiteMesh { xy, triangles: tin.triangles, z })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{ArchId, ArchModel, Level, ModifierKind, Terrain, Wall};
    use cadcraft_geom::Vec3;

    fn make_model(boundary: Vec<Vec2>, points: Vec<Vec3>, modifiers: Vec<ModifierKind>) -> ArchModel {
        let terrain = Terrain {
            boundary,
            points,
            modifiers: modifiers.into_iter().enumerate().map(|(i, kind)| cadcraft_doc::arch::TerrainModifier { id: i as u32 + 1, kind }).collect(),
            pad: false,
            ..Terrain::default()
        };
        ArchModel { terrain: Some(terrain), ..ArchModel::default() }
    }

    fn flat_poly() -> Vec<Vec2> {
        vec![Vec2::new(15.0, 10.0), Vec2::new(25.0, 10.0), Vec2::new(25.0, 20.0), Vec2::new(15.0, 20.0)]
    }

    fn flat_step_poly() -> Vec<Vec2> {
        vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)]
    }

    #[test]
    fn site_mesh_of_a_plane_matches_the_plane() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let model = make_model(boundary, points, vec![]);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            for (ki, &ci) in tri.iter().enumerate() {
                let p = mesh.xy[ci];
                let z = mesh.z[ti][ki];
                assert!((z - 0.05 * p.x).abs() < 1e-4, "corner {p:?} z={z} expected 0.05*x");
            }
        }
    }

    #[test]
    fn site_mesh_keeps_flat_region_edges_as_steps() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let modifiers = vec![ModifierKind::Flat { polygon: flat_poly(), elevation: 1.0, margin: 0.0 }];
        let model = make_model(boundary, points, modifiers);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            let cx = mesh.xy[tri[0]].x + mesh.xy[tri[1]].x + mesh.xy[tri[2]].x;
            let cy = mesh.xy[tri[0]].y + mesh.xy[tri[1]].y + mesh.xy[tri[2]].y;
            let centroid = Vec2::new(cx / 3.0, cy / 3.0);
            let in_poly = inside_or_on(&flat_poly(), centroid);
            for (ki, &ci) in tri.iter().enumerate() {
                let p = mesh.xy[ci];
                let z = mesh.z[ti][ki];
                if in_poly {
                    assert!((z - 1.0).abs() < 1e-3, "in poly {p:?} z={z}");
                } else {
                    let ground = 0.05 * p.x;
                    assert!((z - ground).abs() < 1e-3, "out poly {p:?} z={z} ground={ground}");
                }
            }
        }
    }

    #[test]
    fn site_mesh_corner_z_comes_from_its_own_side() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let modifiers = vec![ModifierKind::Flat { polygon: flat_poly(), elevation: 1.0, margin: 0.0 }];
        let model = make_model(boundary, points, modifiers);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        let idx = point_in_list(&mesh.xy, Vec2::new(15.0, 10.0)).unwrap();
        let mut inside_seen = false;
        let mut outside_seen = false;
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            for (ki, &ci) in tri.iter().enumerate() {
                if ci == idx {
                    let z = mesh.z[ti][ki];
                    if z > 0.9 {
                        inside_seen = true;
                        assert!((z - 1.0).abs() < 1e-4, "corner (15,10) inside z={z}");
                    } else {
                        outside_seen = true;
                        assert!((z - 0.75).abs() < 1e-3, "corner (15,10) outside z={z}");
                    }
                }
            }
        }
        assert!(inside_seen, "no inside triangle at corner (15,10)");
        assert!(outside_seen, "no outside triangle at corner (15,10)");
    }

    #[test]
    fn site_mesh_refines_the_original_tin() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let modifiers = vec![ModifierKind::Flat { polygon: flat_poly(), elevation: 1.0, margin: 0.0 }];
        let model = make_model(boundary, points, modifiers);
        let surf = SiteSurface::new(&model).unwrap();
        let original = surf.original();
        let mesh = site_mesh(&surf).unwrap();
        assert!(mesh.triangles.len() >= original.triangles.len());
        let mesh_area: f64 = mesh
            .triangles
            .iter()
            .map(|tri| {
                let a = mesh.xy[tri[0]];
                let b = mesh.xy[tri[1]];
                let c = mesh.xy[tri[2]];
                ((b - a).cross(c - a)).abs() * 0.5
            })
            .sum();
        assert!((mesh_area - 1200.0).abs() < 0.1, "area {mesh_area}");
        let max_orig: f64 = original
            .triangles
            .iter()
            .map(|tri| {
                let a = original.points[tri[0]];
                let b = original.points[tri[1]];
                let c = original.points[tri[2]];
                ((b.xy() - a.xy()).cross(c.xy() - a.xy())).abs() * 0.5
            })
            .fold(0.0f64, f64::max);
        for tri in &mesh.triangles {
            let a = mesh.xy[tri[0]];
            let b = mesh.xy[tri[1]];
            let c = mesh.xy[tri[2]];
            let area = ((b - a).cross(c - a)).abs() * 0.5;
            assert!(area <= max_orig + 1e-6, "mesh tri {area} > orig max {max_orig}");
        }
    }

    #[test]
    fn site_mesh_samples_hills_on_rings() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let hill = ModifierKind::Hill { center: Vec2::new(10.0, 20.0), height: 2.0, radius: 5.0 };
        let model = make_model(boundary, points, vec![hill]);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        let center_idx = point_in_list(&mesh.xy, Vec2::new(10.0, 20.0)).unwrap();
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            if tri.contains(&center_idx) {
                let ki = tri.iter().position(|&c| c == center_idx).unwrap();
                let z = mesh.z[ti][ki];
                assert!((z - 2.5).abs() < 1e-3, "hill center z={z}");
            }
        }
        let ring_pt_idx = point_in_list(&mesh.xy, Vec2::new(12.5, 20.0)).unwrap();
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            if tri.contains(&ring_pt_idx) {
                let ki = tri.iter().position(|&c| c == ring_pt_idx).unwrap();
                let z = mesh.z[ti][ki];
                assert!((z - 1.75).abs() < 0.01, "hill ring z={z}");
            }
        }
    }

    #[test]
    fn site_mesh_stays_inside_the_boundary() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let model = make_model(boundary.clone(), points, vec![]);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        for &p in &mesh.xy {
            assert!(inside_or_on(&boundary, p), "vertex {p:?} outside");
        }
        let area: f64 = mesh
            .triangles
            .iter()
            .map(|tri| {
                let a = mesh.xy[tri[0]];
                let b = mesh.xy[tri[1]];
                let c = mesh.xy[tri[2]];
                ((b - a).cross(c - a)).abs() * 0.5
            })
            .sum();
        assert!((area - 1200.0).abs() < 0.1, "coverage area {area}");
    }

    #[test]
    fn site_mesh_lowest_returns_minimum_z() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let model = make_model(boundary, points, vec![]);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        let lowest = mesh.lowest().unwrap();
        assert!((lowest - 0.0).abs() < 1e-4, "lowest z={lowest} expected 0.0");
    }

    #[test]
    fn site_mesh_lowest_empty_returns_none() {
        let mesh = SiteMesh::default();
        assert!(mesh.lowest().is_none(), "empty mesh should have no lowest");
    }

    #[test]
    fn site_mesh_requires_three_boundary_points() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0)];
        let model = make_model(boundary, points, vec![]);
        let result = SiteSurface::new(&model);
        assert!(result.is_err(), "expected error for 2-point boundary");
    }

    #[test]
    fn site_mesh_deduplicates_coincident_points() {
        let boundary = flat_step_poly();
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(40.0, 0.0, 2.0),
            Vec3::new(40.0, 30.0, 2.0),
            Vec3::new(0.0, 30.0, 0.0),
        ];
        let model = make_model(boundary, points, vec![]);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        for i in 0..mesh.xy.len() {
            for j in (i + 1)..mesh.xy.len() {
                assert!(mesh.xy[i].dist(mesh.xy[j]) > DEDUP_EPS, "duplicate vertices at {} and {}", i, j);
            }
        }
    }

    #[test]
    fn site_mesh_handles_nan_points() {
        let boundary = flat_step_poly();
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(40.0, 0.0, 2.0),
            Vec3::new(40.0, 30.0, 2.0),
            Vec3::new(0.0, 30.0, 0.0),
            Vec3::new(f64::NAN, f64::NAN, f64::NAN),
        ];
        let model = make_model(boundary, points, vec![]);
        let result = SiteSurface::new(&model);
        assert!(result.is_err(), "expected error for NaN point");
    }

    #[test]
    fn site_mesh_flat_region_with_margin() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let modifiers = vec![ModifierKind::Flat { polygon: flat_poly(), elevation: 1.0, margin: 0.5 }];
        let model = make_model(boundary, points, modifiers);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        let mut found_offset = false;
        for p in &mesh.xy {
            if (p.x - 14.5).abs() < 0.1 && (p.y - 9.5).abs() < 0.1 {
                found_offset = true;
                break;
            }
        }
        assert!(found_offset, "no vertex near offset ring corner (14.5, 9.5)");
    }

    #[test]
    fn site_mesh_hill_at_boundary() {
        let boundary = flat_step_poly();
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let hill = ModifierKind::Hill { center: Vec2::new(20.0, 0.0), height: 2.0, radius: 10.0 };
        let model = make_model(boundary, points, vec![hill]);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        let center_idx = point_in_list(&mesh.xy, Vec2::new(20.0, 0.0)).unwrap();
        let mut found_center = false;
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            if tri.contains(&center_idx) {
                let ki = tri.iter().position(|&c| c == center_idx).unwrap();
                let z = mesh.z[ti][ki];
                assert!((z - 3.0).abs() < 0.1, "hill center z={z} expected ~3.0");
                found_center = true;
                break;
            }
        }
        assert!(found_center, "hill center not in any triangle");
    }
    /// One-level house: 40x30 boundary, z=0.05*x corners, four exterior walls
    /// around (15,11)-(25,19), pad enabled (pad_margin defaults to 0.3).
    fn make_house_site(boundary: Vec<Vec2>, points: Vec<Vec3>) -> ArchModel {
        let level = Level { index: 0, name: "Level 1".into(), elevation: 0.0, ..Level::default() };
        let walls = vec![
            Wall {
                id: ArchId(1),
                start: Vec2::new(15.0, 11.0),
                end: Vec2::new(25.0, 11.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: ArchId(2),
                start: Vec2::new(25.0, 11.0),
                end: Vec2::new(25.0, 19.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: ArchId(3),
                start: Vec2::new(25.0, 19.0),
                end: Vec2::new(15.0, 19.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: ArchId(4),
                start: Vec2::new(15.0, 19.0),
                end: Vec2::new(15.0, 11.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
        ];
        let terrain = Terrain { boundary, points, modifiers: Vec::new(), pad: true, ..Terrain::default() };
        ArchModel { levels: vec![level], walls, terrain: Some(terrain), ..ArchModel::default() }
    }

    /// Barycentric corner-elevation interpolation at `p` over `mesh`.
    fn mesh_z_at(mesh: &SiteMesh, p: Vec2) -> Option<f64> {
        for (ti, tri) in mesh.triangles.iter().enumerate() {
            let a = mesh.xy[tri[0]];
            let b = mesh.xy[tri[1]];
            let c = mesh.xy[tri[2]];
            let v0 = b - a;
            let v1 = c - a;
            let v2 = p - a;
            let den = v0.cross(v1);
            if den.abs() < 1e-14 {
                continue;
            }
            let u = v2.cross(v1) / den;
            let v = v0.cross(v2) / den;
            if u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9 {
                let z = (1.0 - u - v) * mesh.z[ti][0] + u * mesh.z[ti][1] + v * mesh.z[ti][2];
                return Some(z);
            }
        }
        None
    }

    #[test]
    fn site_mesh_follows_the_pad_slope() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let model = make_house_site(boundary, points);
        let surf = SiteSurface::new(&model).unwrap();
        let pads = surf.pads();
        assert_eq!(pads.len(), 1, "expected exactly one pad");
        let ring = pads[0].ring.clone();
        let pad_dist = pads[0].margin + 0.05;
        let mesh = site_mesh(&surf).unwrap();
        for x in 0..40 {
            for y in 0..30 {
                let p = Vec2::new(x as f64 + 0.5, y as f64 + 0.5);
                if inside_or_on(&ring, p) {
                    continue;
                }
                if ring_dist(p, &ring) <= pad_dist {
                    continue;
                }
                if let (Some(z), Some(expected)) = (mesh_z_at(&mesh, p), surf.modified_at(p)) {
                    assert!((z - expected).abs() < 0.07, "at {p:?} mesh z={z} expected {expected}");
                }
            }
        }
        assert!(mesh_z_at(&mesh, Vec2::new(18.0, 5.0)).is_some(), "no triangle at (18, 5)");
        let z_18_5 = mesh_z_at(&mesh, Vec2::new(18.0, 5.0)).unwrap();
        assert!((z_18_5 - 0.9).abs() < 0.01, "at (18, 5) z={z_18_5} expected ~0.9",);
    }

    #[test]
    fn site_mesh_pad_slope_rings_stay_inside_the_boundary() {
        let boundary = vec![Vec2::new(12.0, 8.0), Vec2::new(28.0, 8.0), Vec2::new(28.0, 22.0), Vec2::new(12.0, 22.0)];
        let points = vec![Vec3::new(12.0, 8.0, 0.6), Vec3::new(28.0, 8.0, 1.4), Vec3::new(28.0, 22.0, 1.4), Vec3::new(12.0, 22.0, 0.6)];
        let model = make_house_site(boundary.clone(), points);
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        for &p in &mesh.xy {
            assert!(inside_or_on(&boundary, p), "vertex {p:?} outside boundary");
        }
    }

    #[test]
    fn site_mesh_without_a_pad_keeps_its_vertices() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let terrain = Terrain { boundary, points, modifiers: Vec::new(), pad: false, ..Terrain::default() };
        let model = ArchModel { terrain: Some(terrain), ..ArchModel::default() };
        let surf = SiteSurface::new(&model).unwrap();
        let mesh = site_mesh(&surf).unwrap();
        assert_eq!(mesh.xy.len(), 4, "expected 4 vertices, got {}", mesh.xy.len());
        assert_eq!(mesh.triangles.len(), 2, "expected 2 triangles, got {}", mesh.triangles.len(),);
    }

    /// The same house with a basement level under it (the pad sits on level 1, the basement cuts
    /// the ground down inside the footprint).
    fn make_house_site_with_basement() -> ArchModel {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let mut model = make_house_site(boundary, points);
        model.levels = vec![
            Level { index: 0, name: "Basement".into(), elevation: -2.7, ..Level::default() },
            Level { index: 1, name: "Level 1".into(), elevation: 0.0, ..Level::default() },
        ];
        let upper: Vec<Wall> = model.walls.iter().map(|w| Wall { id: ArchId(w.id.0 + 10), base_level: 1, ..w.clone() }).collect();
        model.walls.extend(upper);
        model
    }

    /// Every edge of `mesh` is shared by two triangles running opposite ways, except edges on the
    /// site boundary, which have exactly one: the closed top the 3D terrain solid is built from.
    fn assert_closed(mesh: &SiteMesh, boundary: &[Vec2], what: &str) {
        let mut directed = std::collections::HashMap::new();
        for (t, tri) in mesh.triangles.iter().enumerate() {
            for k in 0..3 {
                let e = (tri[k], tri[(k + 1) % 3]);
                assert!(directed.insert(e, t).is_none(), "{what}: edge {:?} -> {:?} used twice the same way", mesh.xy[e.0], mesh.xy[e.1]);
            }
        }
        for &(a, b) in directed.keys() {
            if !directed.contains_key(&(b, a)) {
                let (pa, pb) = (mesh.xy[a], mesh.xy[b]);
                let mid = Vec2::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0);
                assert!(ring_dist(mid, boundary) <= 1e-9, "{what}: open edge {pa:?} -> {pb:?} inside the site");
            }
        }
    }

    #[test]
    fn site_mesh_with_a_pad_is_closed() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let one = SiteSurface::new(&make_house_site(boundary.clone(), points)).unwrap();
        assert_closed(&site_mesh(&one).unwrap(), &boundary, "pad");
        let two = SiteSurface::new(&make_house_site_with_basement()).unwrap();
        assert_eq!(two.pads().len(), 2, "a pad and a basement");
        assert_closed(&site_mesh(&two).unwrap(), &boundary, "pad and basement");
    }
}
