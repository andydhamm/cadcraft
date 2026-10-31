//! The site mesh: the modified surface triangulated with every breakline as an edge and an
//! elevation per triangle corner, so a vertical step is two triangles' corners at one point with
//! different elevations. Filled in by `p9-arch-terrain-mesh`.

use std::cmp::Ordering;
use std::f64::consts::PI;

use crate::poly::offset_polygon;
use cadcraft_geom::{Vec2, Vec3, point_in_polygon};

use super::TerrainError;
use super::pad::PadKind;
use super::surface::SiteSurface;

/// Most triangles a `SiteMesh` may hold. More than this returns
/// `Err(TerrainError("terrain mesh too large".into()))`.
const MAX_MESH_TRIANGLES: usize = 200_000;
/// Vertices closer than this are considered the same vertex.
const DEDUP_EPS: f64 = 1e-9;
/// Triangles with area below this are dropped (degenerate).
const MIN_TRIANGLE_AREA: f64 = 1e-12;

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

    // Pad ring edges.
    for region in surf.pads() {
        for i in 0..region.ring.len() {
            let a = region.ring[i];
            let b = region.ring[(i + 1) % region.ring.len()];
            if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                raw_constraints.push([a, b]);
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
    use cadcraft_doc::arch::{ArchModel, ModifierKind, Terrain};
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
}
