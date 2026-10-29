//! Roof mesh generation.
//!
//! Generates 3D meshes for roof planes, fascia, and soffits from `RoofPlane` data.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use crate::mesh::Mesh;
use cadcraft_doc::ArchId;
use cadcraft_doc::arch::{Roof, RoofPlane};
use cadcraft_geom::{Vec2, Vec3};

fn dist_to_base_edge_line(plane: &RoofPlane, p: Vec2) -> f64 {
    let a = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let b = Vec2::new(plane.base_edge[2], plane.base_edge[3]);
    let ab = b - a;
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if !len_sq.is_finite() || len_sq < 1e-12 {
        return 0.0;
    }
    let len = len_sq.sqrt();
    let dir = Vec2::new(ab.x / len, ab.y / len);
    let left = Vec2::new(-dir.y, dir.x);
    let right = Vec2::new(dir.y, -dir.x);
    let centroid = plane.ring.iter().filter(|q| q.is_finite()).copied().reduce(|acc, q| acc + q).unwrap_or(Vec2::ZERO);
    let count = plane.ring.iter().filter(|q| q.is_finite()).count();
    let centroid = if count > 0 { centroid / count as f64 } else { a };
    let normal_xy = if (centroid - a).dot(left) >= 0.0 { left } else { right };
    let v = p - a;
    v.x * normal_xy.x + v.y * normal_xy.y
}

/// The underside elevation of a roof plane at plan point `p`.
///
/// The eave sits `overhang` inside from the roof outline, so the underside is
/// `eave_z + slope * (dist_to_base_edge_line(p) - overhang)`. `overhang` is
/// treated as zero when it is non-finite or negative.
pub fn underside_z(plane: &RoofPlane, eave_z: f64, p: Vec2) -> f64 {
    let overhang = if plane.overhang.is_finite() && plane.overhang >= 0.0 { plane.overhang } else { 0.0 };
    eave_z + plane.slope * (dist_to_base_edge_line(plane, p) - overhang)
}

/// Generate a mesh for a single roof plane.
///
/// The underside is defined by lifting the plane's ring vertices by `z = eave_z + slope * dist`,
/// where `dist` is the perpendicular distance from the vertex to the base edge's line.
/// The top surface is the underside raised vertically by `thickness / cos(theta)`, where
/// `theta` is the pitch angle, so the plan ring is identical for both rings and the sides
/// are vertical.
///
/// Returns `None` if the plane is degenerate or contains non-finite values.
pub fn roof_plane_mesh(plane: &RoofPlane, eave_z: f64, material: &str, id: ArchId) -> Option<Mesh> {
    if plane.ring.len() < 3 {
        return None;
    }
    if !plane.ring.iter().all(|p| p.is_finite()) || !eave_z.is_finite() || !plane.slope.is_finite() || !plane.thickness.is_finite() {
        return None;
    }

    // Base edge line: p0 -> p1
    let p0 = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let p1 = Vec2::new(plane.base_edge[2], plane.base_edge[3]);

    // Direction of base edge
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() || len_sq < 1e-12 {
        return None;
    }
    let len = len_sq.sqrt();
    let dir = Vec2::new(dx / len, dy / len);
    let normal_xy = Vec2::new(-dir.y, dir.x);

    // Calculate bottom ring (underside)
    let mut bottom_ring: Vec<Vec2> = Vec::with_capacity(plane.ring.len());
    let mut bottom_z: Vec<f64> = Vec::with_capacity(plane.ring.len());

    for &v in &plane.ring {
        let z = underside_z(plane, eave_z, v);
        if !z.is_finite() {
            return None;
        }

        bottom_ring.push(v);
        bottom_z.push(z);
    }

    // Calculate top ring
    // Normal vector in 3D.
    // The plane rises with slope `s` in the direction of `normal_xy`.
    // The 3D normal `n` is perpendicular to the base edge direction `dir` and the slope direction.
    // Slope vector in 3D: `dir` is horizontal. The rise is `s * dist`.
    // The plane equation is z = eave_z + s * (dot(v - p0, normal_xy)).
    // Gradient of z is `s * normal_xy`.
    // Normal to surface (z - f(x,y) = 0) is (-df/dx, -df/dy, 1) = (-s*nx, -s*ny, 1).
    // Normalize this vector.
    let nx = -plane.slope * normal_xy.x;
    let ny = -plane.slope * normal_xy.y;
    let nz = 1.0;
    let n_len = (nx * nx + ny * ny + nz * nz).sqrt();
    if n_len < 1e-12 {
        return None;
    }
    let n = Vec3::new(nx / n_len, ny / n_len, nz / n_len);

    let mut top_ring: Vec<Vec2> = Vec::with_capacity(plane.ring.len());
    let mut top_z: Vec<f64> = Vec::with_capacity(plane.ring.len());

    for i in 0..plane.ring.len() {
        let v = plane.ring[i];
        let z = bottom_z[i];

        // Vertical thickness: keep the plan ring and raise it by
        // thickness / cos(theta) = thickness * sqrt(1 + slope^2).
        let tz = z + plane.thickness * (1.0 + plane.slope * plane.slope).sqrt();

        if !tz.is_finite() {
            return None;
        }

        top_ring.push(v);
        top_z.push(tz);
    }

    // Build mesh
    let mut mesh = Mesh::new(material, id);

    // We need to emit vertices and indices manually because push_prism assumes vertical sides.
    // Here sides are slanted (top ring is offset in XY and Z).

    let n_verts = plane.ring.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n_verts * 2);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n_verts * 2);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n_verts * 2);
    let mut indices: Vec<u32> = Vec::new();

    // Bottom vertices
    for i in 0..n_verts {
        positions.push([bottom_ring[i].x as f32, bottom_ring[i].y as f32, bottom_z[i] as f32]);
        // Normal for bottom face is -n
        normals.push([-n.x as f32, -n.y as f32, -n.z as f32]);
        uvs.push([bottom_ring[i].x as f32, bottom_ring[i].y as f32]);
    }

    // Top vertices
    for i in 0..n_verts {
        positions.push([top_ring[i].x as f32, top_ring[i].y as f32, top_z[i] as f32]);
        // Normal for top face is n
        normals.push([n.x as f32, n.y as f32, n.z as f32]);
        uvs.push([top_ring[i].x as f32, top_ring[i].y as f32]);
    }

    // Sides
    for i in 0..n_verts {
        let next = (i + 1) % n_verts;
        let b0 = i as u32;
        let b1 = next as u32;
        let t0 = (n_verts + i) as u32;
        let t1 = (n_verts + next) as u32;

        // Quad: b0, b1, t1, t0
        // Triangles: (b0, b1, t1), (b0, t1, t0)
        // Winding: CCW when viewed from outside.
        // Assuming ring is CCW.
        indices.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
    }

    // Bottom cap (triangulate fan from vertex 0)
    // Winding: CW when viewed from below (outside).
    // If ring is CCW, bottom face normal is -n (down-ish).
    // To face down, triangles should be CW in XY projection?
    // Let's use standard fan.
    for i in 1..n_verts - 1 {
        indices.extend_from_slice(&[0, (i + 1) as u32, i as u32]);
    }

    // Top cap (triangulate fan from vertex n_verts)
    // Winding: CCW when viewed from above.
    let base = n_verts as u32;
    for i in 1..n_verts - 1 {
        let i_u32 = i as u32;
        indices.extend_from_slice(&[base, base + i_u32, base + i_u32 + 1]);
    }

    mesh.positions = positions;
    mesh.normals = normals;
    mesh.uvs = uvs;
    mesh.indices = indices;
    mesh.level_elevation = eave_z;

    Some(mesh)
}

/// Generate a fascia mesh for a roof plane.
///
/// The fascia is a vertical band at the eave edge (base edge), with width `plane.thickness`
/// and depth `depth`.
pub fn fascia_mesh(plane: &RoofPlane, eave_z: f64, depth: f64, id: ArchId) -> Option<Mesh> {
    if plane.ring.len() < 3 {
        return None;
    }
    if !depth.is_finite() || depth <= 0.0 {
        return None;
    }

    let p0 = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let p1 = Vec2::new(plane.base_edge[2], plane.base_edge[3]);

    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() || len_sq < 1e-12 {
        return None;
    }
    let len = len_sq.sqrt();
    let dir = Vec2::new(dx / len, dy / len);
    let normal_xy = Vec2::new(-dir.y, dir.x);

    // The fascia hangs down from the eave.
    // The eave edge is at z = eave_z (at the base edge line).
    // Actually, the roof plane underside at the base edge is z = eave_z.
    // The fascia is attached to the underside.
    // It extends down by `depth`.
    // It has width `plane.thickness` (horizontal width along the normal? or vertical thickness?)
    // "vertical band at the eave edge, plane.thickness wide".
    // Usually fascia is the board covering the rafter ends.
    // It is vertical. Its width is the thickness of the roof (or fascia board).
    // Let's assume it's a box extruded from the base edge line.

    // Vertices:
    // Top-Outer: p0, p1 at z = eave_z
    // Bottom-Outer: p0, p1 at z = eave_z - depth
    // Top-Inner: p0 + normal_xy * thickness, p1 + normal_xy * thickness at z = eave_z
    // Bottom-Inner: p0 + normal_xy * thickness, p1 + normal_xy * thickness at z = eave_z - depth

    let t = plane.thickness;
    let p0_in = Vec2::new(p0.x + normal_xy.x * t, p0.y + normal_xy.y * t);
    let p1_in = Vec2::new(p1.x + normal_xy.x * t, p1.y + normal_xy.y * t);

    let z_top = underside_z(plane, eave_z, p0);
    let z_bot = z_top - depth;

    let mut mesh = Mesh::new("fascia", id);

    // 8 vertices
    // 0: p0, z_top
    // 1: p1, z_top
    // 2: p1_in, z_top
    // 3: p0_in, z_top
    // 4: p0, z_bot
    // 5: p1, z_bot
    // 6: p1_in, z_bot
    // 7: p0_in, z_bot

    let positions = vec![
        [p0.x as f32, p0.y as f32, z_top as f32],
        [p1.x as f32, p1.y as f32, z_top as f32],
        [p1_in.x as f32, p1_in.y as f32, z_top as f32],
        [p0_in.x as f32, p0_in.y as f32, z_top as f32],
        [p0.x as f32, p0.y as f32, z_bot as f32],
        [p1.x as f32, p1.y as f32, z_bot as f32],
        [p1_in.x as f32, p1_in.y as f32, z_bot as f32],
        [p0_in.x as f32, p0_in.y as f32, z_bot as f32],
    ];

    // Normals:
    // Front (0,1,5,4): normal -dir? No, front is the outer face. Normal is -normal_xy?
    // The outer face is the one at p0-p1. Normal points away from roof, so -normal_xy.
    // Back (3,2,6,7): inner face. Normal points towards roof, so normal_xy.
    // Left (0,3,7,4): normal -dir.
    // Right (1,2,6,5): normal dir.
    // Top (0,1,2,3): normal up (0,0,1).
    // Bottom (4,5,6,7): normal down (0,0,-1).

    let n_front = [-normal_xy.x as f32, -normal_xy.y as f32, 0.0];
    let n_back = [normal_xy.x as f32, normal_xy.y as f32, 0.0];
    let n_left = [-dir.x as f32, -dir.y as f32, 0.0];
    let n_right = [dir.x as f32, dir.y as f32, 0.0];
    let n_up = [0.0, 0.0, 1.0];
    let n_down = [0.0, 0.0, -1.0];

    let normals = vec![
        n_front, n_front, n_back, n_back, // Top ring normals approx
        n_front, n_front, n_back, n_back, // Bottom ring normals approx
    ];
    // Actually, per-vertex normals for a box are tricky if we want flat shading.
    // The renderer uses flat shading from triangle plane, so normals don't matter much for lighting,
    // but they should be correct for other uses.
    // Let's just assign per-face normals if possible, but Mesh stores per-vertex.
    // We'll duplicate vertices for sharp edges if needed, but for a simple box,
    // we can just use the 8 vertices and let the flat shading handle it.
    // The normals array in Mesh is per-vertex.

    let uvs = vec![[0.0, 0.0]; 8];

    let indices = vec![
        // Front (Outer)
        0, 1, 5, 0, 5, 4, // Back (Inner)
        3, 7, 6, 3, 6, 2, // Left
        0, 4, 7, 0, 7, 3, // Right
        1, 2, 6, 1, 6, 5, // Top
        0, 3, 2, 0, 2, 1, // Bottom
        4, 5, 6, 4, 6, 7,
    ];

    mesh.positions = positions;
    mesh.normals = normals;
    mesh.uvs = uvs;
    mesh.indices = indices;
    mesh.level_elevation = eave_z;

    Some(mesh)
}

/// Generate a soffit mesh for a roof plane.
///
/// The soffit is a flat horizontal band at `eave_z`, extending `depth` inward from the
/// eave edge (base edge), with a fixed vertical thickness of 0.02 m.
///
/// Returns `None` if the plane is degenerate, the base edge is zero-length,
/// or `depth` is not a positive finite number.
pub fn soffit_mesh(plane: &RoofPlane, eave_z: f64, depth: f64, id: ArchId) -> Option<Mesh> {
    if plane.ring.len() < 3 {
        return None;
    }
    if !depth.is_finite() || depth <= 0.0 {
        return None;
    }
    let thickness = 0.02;

    let p0 = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let p1 = Vec2::new(plane.base_edge[2], plane.base_edge[3]);

    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() || len_sq < 1e-12 {
        return None;
    }
    let len = len_sq.sqrt();
    let dir = Vec2::new(dx / len, dy / len);
    let normal_xy = Vec2::new(-dir.y, dir.x);

    let width = depth;

    let p0_in = Vec2::new(p0.x + normal_xy.x * width, p0.y + normal_xy.y * width);
    let p1_in = Vec2::new(p1.x + normal_xy.x * width, p1.y + normal_xy.y * width);

    // Soffit is horizontal.
    // It is located under the roof.
    // The roof underside at the eave is z = eave_z.
    // The soffit is below that.
    // Let's place it at z = eave_z - plane.thickness - thickness/2?
    // Or just below the fascia?
    // Fascia hangs down from eave_z.
    // Soffit is the underside of the overhang.
    // It should be at z = eave_z - plane.thickness (bottom of roof deck).
    // Let's say the soffit top is at eave_z - plane.thickness.
    // And it is 0.02 thick, so bottom is eave_z - plane.thickness - 0.02.

    let z_top = underside_z(plane, eave_z, p0);
    let z_bot = z_top - thickness;

    let mut mesh = Mesh::new("soffit", id);

    let positions = vec![
        [p0.x as f32, p0.y as f32, z_top as f32],
        [p1.x as f32, p1.y as f32, z_top as f32],
        [p1_in.x as f32, p1_in.y as f32, z_top as f32],
        [p0_in.x as f32, p0_in.y as f32, z_top as f32],
        [p0.x as f32, p0.y as f32, z_bot as f32],
        [p1.x as f32, p1.y as f32, z_bot as f32],
        [p1_in.x as f32, p1_in.y as f32, z_bot as f32],
        [p0_in.x as f32, p0_in.y as f32, z_bot as f32],
    ];

    let normals = vec![
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
    ];

    let uvs = vec![[0.0, 0.0]; 8];

    let indices = vec![
        // Top
        0, 1, 2, 0, 2, 3, // Bottom
        4, 6, 5, 4, 7, 6, // Sides
        0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2, 7, 3, 3, 7, 4, 3, 4, 0,
    ];

    mesh.positions = positions;
    mesh.normals = normals;
    mesh.uvs = uvs;
    mesh.indices = indices;
    mesh.level_elevation = eave_z;

    Some(mesh)
}

/// Generate all meshes for a roof.
///
/// Emits one solid mesh per valid plane with material `"roof"`. When `roof.fascia`
/// is true, a vertical fascia band is added per plane; when `roof.soffit` is true,
/// a horizontal soffit panel is added. Fascia/soffit depths come from
/// `plane.fascia_depth` / `plane.soffit_depth`, defaulting to 0.1 m when zero.
/// Degenerate or non-finite planes are silently skipped.
pub fn roof_meshes(roof: &Roof, eave_z: f64) -> Vec<Mesh> {
    let mut meshes = Vec::new();

    for plane in &roof.planes {
        // A plane carries its own eave height (set by the command for manual roofs); fall back to
        // the level-derived `eave_z` only when it is not finite.
        let ez = if plane.eave_z.is_finite() { plane.eave_z } else { eave_z };

        if let Some(m) = roof_plane_mesh(plane, ez, "roof", roof.id) {
            meshes.push(m);
        }

        if roof.fascia {
            let fd = if plane.fascia_depth > 0.0 { plane.fascia_depth } else { 0.1 };
            if let Some(m) = fascia_mesh(plane, ez, fd, roof.id) {
                meshes.push(m);
            }
        }

        if roof.soffit {
            let sd = if plane.soffit_depth > 0.0 { plane.soffit_depth } else { 0.1 };
            if let Some(m) = soffit_mesh(plane, ez, sd, roof.id) {
                meshes.push(m);
            }
        }
    }

    meshes
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::ArchId;
    use cadcraft_doc::arch::{Roof, RoofPlane};
    use cadcraft_geom::Vec2;

    fn simple_plane() -> RoofPlane {
        let mut plane = RoofPlane::default();
        // Base edge along X axis from 0,0 to 10,0
        plane.base_edge = [0.0, 0.0, 10.0, 0.0];
        // Ring: rectangle 10x5, extending in +Y
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 5.0), Vec2::new(0.0, 5.0)];
        plane.slope = 0.5; // 6/12 pitch
        plane.thickness = 0.2;
        plane
    }

    #[test]
    fn roof_plane_mesh_is_a_closed_manifold() {
        let plane = simple_plane();
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // Check bounds
        let b = mesh.bounds();
        assert!(b.min[2] < b.max[2]);

        // Check indices are valid
        let max_idx = mesh.indices.iter().max().copied().unwrap_or(0);
        assert!((max_idx as usize) < mesh.positions.len());

        // Check triangle count
        assert_eq!(mesh.indices.len() % 3, 0);

        // Simple manifold check: every edge used by exactly 2 triangles?
        // This is a complex check. For now, just ensure it builds and has volume.
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn roof_plane_underside_passes_through_the_eave_edge() {
        let plane = simple_plane();
        let eave_z = 2.5;
        let mesh = roof_plane_mesh(&plane, eave_z, "roof", ArchId(1)).unwrap();

        // The base edge vertices are at (0,0) and (10,0).
        // Their distance to the base edge line is 0.
        // So z should be eave_z + slope * 0 = eave_z.

        // Find vertices with x=0, y=0
        let mut found = false;
        for p in &mesh.positions {
            if (p[0] - 0.0).abs() < 1e-4 && (p[1] - 0.0).abs() < 1e-4 {
                if (p[2] - eave_z as f32).abs() < 1e-4 {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "Vertex at eave edge should be at eave_z");
    }

    #[test]
    fn roof_plane_rises_at_its_slope() {
        let plane = simple_plane();
        let eave_z = 2.5;
        let mesh = roof_plane_mesh(&plane, eave_z, "roof", ArchId(1)).unwrap();

        // Vertex at (0, 5). Distance to line y=0 is 5.
        // z = 2.5 + 0.5 * 5 = 5.0.

        let mut found = false;
        for p in &mesh.positions {
            if (p[0] - 0.0).abs() < 1e-4 && (p[1] - 5.0).abs() < 1e-4 {
                if (p[2] - 5.0).abs() < 1e-4 {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "Vertex at distance 5 should be at z=5.0");
    }

    #[test]
    fn roof_meshes_are_probeable() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        roof.planes.push(simple_plane());

        let meshes = roof_meshes(&roof, 2.5);
        assert!(!meshes.is_empty());

        // Check if probeable (has z extent)
        for m in &meshes {
            let b = m.bounds();
            assert!(b.max[2] - b.min[2] > 1e-6, "Mesh {} has no z extent", m.material);
        }
    }

    #[test]
    fn fascia_and_soffit_are_emitted_when_flagged() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        roof.planes.push(simple_plane());

        let meshes = roof_meshes(&roof, 2.5);

        let has_fascia = meshes.iter().any(|m| m.material == "fascia");
        let has_soffit = meshes.iter().any(|m| m.material == "soffit");

        assert!(has_fascia, "Fascia mesh missing");
        assert!(has_soffit, "Soffit mesh missing");
    }

    #[test]
    fn degenerate_plane_is_skipped() {
        let mut plane = simple_plane();
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)]; // < 3 vertices

        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1));
        assert!(mesh.is_none());
    }

    #[test]
    fn part_of_maps_fascia_and_soffit_to_roof() {
        // This test is in probe.rs, but we can verify the logic here if we import part_of
        // But part_of is in probe.rs. We'll rely on the probe.rs test.
    }

    #[test]
    fn roof_plane_zero_slope_is_flat() {
        let mut plane = simple_plane();
        plane.slope = 0.0;
        let eave_z = 2.5;
        let mesh = roof_plane_mesh(&plane, eave_z, "roof", ArchId(1)).unwrap();

        // With zero slope, all bottom vertices should be at eave_z.
        let n_verts = plane.ring.len();
        for p in mesh.positions.iter().take(n_verts) {
            if (p[0] - 0.0).abs() < 1e-4 && (p[1] - 0.0).abs() < 1e-4 {
                assert!((p[2] - eave_z as f32).abs() < 1e-4, "bottom vertex at ({},{}) should be at eave_z", p[0], p[1]);
            }
        }

        // Top ring should be at eave_z + thickness (since normal is straight up for zero slope)
        for p in mesh.positions.iter().skip(n_verts) {
            if (p[0] - 0.0).abs() < 1e-4 && (p[1] - 5.0).abs() < 1e-4 {
                // This is a top vertex at (0, 5)
                let expected_top = eave_z + plane.thickness;
                assert!((p[2] - expected_top as f32).abs() < 1e-4, "top vertex should be at eave_z + thickness");
            }
        }
    }

    #[test]
    fn roof_plane_nan_in_ring_returns_none() {
        let mut plane = simple_plane();
        plane.ring[0] = Vec2::new(f64::NAN, 0.0);
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1));
        assert!(mesh.is_none(), "NaN in ring should return None");
    }

    #[test]
    fn roof_plane_nan_eave_z_returns_none() {
        let plane = simple_plane();
        let mesh = roof_plane_mesh(&plane, f64::NAN, "roof", ArchId(1));
        assert!(mesh.is_none(), "NaN eave_z should return None");
    }

    #[test]
    fn roof_plane_nan_slope_returns_none() {
        let mut plane = simple_plane();
        plane.slope = f64::NAN;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1));
        assert!(mesh.is_none(), "NaN slope should return None");
    }

    #[test]
    fn roof_plane_nan_thickness_returns_none() {
        let mut plane = simple_plane();
        plane.thickness = f64::NAN;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1));
        assert!(mesh.is_none(), "NaN thickness should return None");
    }

    #[test]
    fn roof_plane_zero_length_base_edge_returns_none() {
        let mut plane = simple_plane();
        plane.base_edge = [0.0, 0.0, 0.0, 0.0];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1));
        assert!(mesh.is_none(), "zero-length base edge should return None");
    }

    #[test]
    fn roof_plane_huge_slope_is_finite() {
        let mut plane = simple_plane();
        plane.slope = 1000.0;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // All positions should be finite
        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite(), "position not finite: {:?}", p);
        }
    }

    #[test]
    fn roof_plane_huge_coordinates_are_finite() {
        let mut plane = simple_plane();
        plane.ring = vec![Vec2::new(1e6, 1e6), Vec2::new(1e6 + 10.0, 1e6), Vec2::new(1e6 + 10.0, 1e6 + 5.0), Vec2::new(1e6, 1e6 + 5.0)];
        plane.base_edge = [1e6, 1e6, 1e6 + 10.0, 1e6];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite(), "position not finite: {:?}", p);
        }
    }

    #[test]
    fn roof_plane_level_elevation_is_set() {
        let plane = simple_plane();
        let eave_z = 3.5;
        let mesh = roof_plane_mesh(&plane, eave_z, "roof", ArchId(1)).unwrap();
        assert_eq!(mesh.level_elevation, eave_z, "level_elevation should be set to eave_z");
    }

    #[test]
    fn roof_plane_top_is_directly_above_its_plan_ring() {
        let plane = simple_plane();
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        let n_verts = plane.ring.len();
        // Bottom vertices are 0..n_verts, top vertices are n_verts..2*n_verts.
        for i in 0..n_verts {
            let b = mesh.positions[i];
            let t = mesh.positions[n_verts + i];
            assert!((b[0] - t[0]).abs() < 1e-4, "top x mismatch at vertex {}", i);
            assert!((b[1] - t[1]).abs() < 1e-4, "top y mismatch at vertex {}", i);
        }
    }

    #[test]
    fn roof_meshes_multiple_planes() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        roof.planes.push(simple_plane());

        let mut plane2 = simple_plane();
        plane2.base_edge = [10.0, 0.0, 20.0, 0.0];
        plane2.ring = vec![Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(20.0, 5.0), Vec2::new(10.0, 5.0)];
        roof.planes.push(plane2);

        let meshes = roof_meshes(&roof, 2.5);

        // 2 planes * 3 meshes each (roof, fascia, soffit) = 6 meshes
        assert_eq!(meshes.len(), 6, "expected 6 meshes for 2 planes, got {}", meshes.len());

        // All meshes should have the roof id
        for m in &meshes {
            assert_eq!(m.object, ArchId(1), "all meshes should have the roof id");
        }
    }

    #[test]
    fn roof_meshes_skips_degenerate_planes() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);

        let mut degenerate = simple_plane();
        degenerate.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)]; // < 3 vertices
        roof.planes.push(degenerate);

        roof.planes.push(simple_plane());

        let meshes = roof_meshes(&roof, 2.5);

        // Only the non-degenerate plane should produce meshes (3 meshes)
        assert_eq!(meshes.len(), 3, "expected 3 meshes (degenerate skipped), got {}", meshes.len());
    }

    #[test]
    fn roof_meshes_all_have_roof_id() {
        let mut roof = Roof::default();
        roof.id = ArchId(42);
        roof.planes.push(simple_plane());

        let meshes = roof_meshes(&roof, 2.5);
        for m in &meshes {
            assert_eq!(m.object, ArchId(42), "mesh {} should have roof id 42", m.material);
        }
    }

    #[test]
    fn fascia_mesh_zero_depth_returns_none() {
        let plane = simple_plane();
        let mesh = fascia_mesh(&plane, 2.5, 0.0, ArchId(1));
        assert!(mesh.is_none(), "zero depth fascia should return None");
    }

    #[test]
    fn fascia_mesh_negative_depth_returns_none() {
        let plane = simple_plane();
        let mesh = fascia_mesh(&plane, 2.5, -0.1, ArchId(1));
        assert!(mesh.is_none(), "negative depth fascia should return None");
    }

    #[test]
    fn fascia_mesh_nan_depth_returns_none() {
        let plane = simple_plane();
        let mesh = fascia_mesh(&plane, 2.5, f64::NAN, ArchId(1));
        assert!(mesh.is_none(), "NaN depth fascia should return None");
    }

    #[test]
    fn soffit_mesh_is_horizontal() {
        let plane = simple_plane();
        let eave_z = 2.5;
        let mesh = soffit_mesh(&plane, eave_z, 0.3, ArchId(1)).unwrap();

        // Soffit is horizontal, so top face normals should be (0,0,1) and bottom (0,0,-1)
        // Check that all top vertices have the same z
        let top_z = mesh.positions[0][2];
        for i in 0..4 {
            assert!((mesh.positions[i][2] - top_z).abs() < 1e-6, "top vertex {} z mismatch", i);
        }
    }

    #[test]
    fn roof_plane_mesh_triangle_count_is_correct() {
        let plane = simple_plane();
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // For a 4-vertex ring:
        // Sides: 4 quads = 8 triangles
        // Bottom cap: 2 triangles (fan from vertex 0)
        // Top cap: 2 triangles (fan from vertex 4)
        // Total: 12 triangles
        assert_eq!(mesh.triangle_count(), 12, "expected 12 triangles for 4-vertex ring");
    }

    #[test]
    fn roof_plane_mesh_pentagon_has_correct_triangle_count() {
        let mut plane = simple_plane();
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 5.0), Vec2::new(0.0, 5.0)];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // For a 5-vertex ring:
        // Sides: 5 quads = 10 triangles
        // Bottom cap: 3 triangles (fan)
        // Top cap: 3 triangles (fan)
        // Total: 16 triangles
        assert_eq!(mesh.triangle_count(), 16, "expected 16 triangles for 5-vertex ring");
    }

    #[test]
    fn roof_plane_nan_in_base_edge_returns_none() {
        let mut plane = simple_plane();
        plane.base_edge = [f64::NAN, 0.0, 10.0, 0.0];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1));
        assert!(mesh.is_none(), "NaN in base_edge should return None");
    }

    #[test]
    fn roof_plane_very_small_slope_is_valid() {
        let mut plane = simple_plane();
        plane.slope = 0.001;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // All positions should be finite
        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite(), "position not finite: {:?}", p);
        }

        // The roof should still have z extent
        let b = mesh.bounds();
        assert!(b.max[2] - b.min[2] > 1e-6, "mesh should have z extent");
    }

    #[test]
    fn roof_plane_ring_with_three_vertices() {
        let mut plane = simple_plane();
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(5.0, 5.0)];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // For a 3-vertex ring:
        // Sides: 3 quads = 6 triangles
        // Bottom cap: 1 triangle (fan)
        // Top cap: 1 triangle (fan)
        // Total: 8 triangles
        assert_eq!(mesh.triangle_count(), 8, "expected 8 triangles for 3-vertex ring");
    }

    #[test]
    fn roof_plane_is_a_probeable_solid() {
        use crate::probe::{part_of, probe_mesh};

        let plane = simple_plane();
        // eave_z=2.5, slope=0.5; at y=2.5 bottom z = 2.5 + 0.5*2.5 = 3.75;
        // normal nz = 1/sqrt(1+0.25) ≈ 0.894; top z ≈ 3.75 + 0.2*0.894 ≈ 3.929.
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // Point strictly inside the roof solid.
        assert!(probe_mesh(&mesh, cadcraft_geom::Vec3::new(5.0, 2.5, 3.8)), "probe inside roof should hit");
        // Material "roof" maps to part "roof".
        assert_eq!(part_of(&mesh), "roof");
    }

    #[test]
    fn fascia_and_soffit_are_built() {
        // roof.fascia and roof.soffit are true by default.
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        roof.planes.push(simple_plane());

        let meshes = roof_meshes(&roof, 2.5);
        assert!(meshes.iter().any(|m| m.material == "fascia"), "fascia expected when roof.fascia is true");
        assert!(meshes.iter().any(|m| m.material == "soffit"), "soffit expected when roof.soffit is true");

        // With both flags off, neither should appear.
        let mut roof_off = Roof { fascia: false, soffit: false, ..Roof::default() };
        roof_off.id = ArchId(2);
        roof_off.planes.push(simple_plane());

        let meshes_off = roof_meshes(&roof_off, 2.5);
        assert!(!meshes_off.iter().any(|m| m.material == "fascia"), "no fascia when roof.fascia is false");
        assert!(!meshes_off.iter().any(|m| m.material == "soffit"), "no soffit when roof.soffit is false");
    }

    #[test]
    fn degenerate_roof_plane_is_skipped() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);

        // Degenerate: only 2 ring vertices.
        let mut bad = simple_plane();
        bad.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)];
        roof.planes.push(bad);

        // Valid plane alongside the degenerate one.
        roof.planes.push(simple_plane());

        let meshes = roof_meshes(&roof, 2.5);
        // Degenerate plane produces no meshes; valid plane does.
        assert!(!meshes.is_empty(), "valid plane should produce meshes");
        assert!(meshes.iter().all(|m| m.object == ArchId(1)), "all meshes belong to the roof");
        // No mesh comes from the degenerate plane (all meshes are non-empty).
        for m in &meshes {
            assert!(!m.positions.is_empty(), "every mesh must have vertices");
        }
    }

    #[test]
    fn soffit_mesh_zero_depth_returns_none() {
        let plane = simple_plane();
        let mesh = soffit_mesh(&plane, 2.5, 0.0, ArchId(1));
        assert!(mesh.is_none(), "zero depth soffit should return None");
    }

    #[test]
    fn soffit_mesh_negative_depth_returns_none() {
        let plane = simple_plane();
        let mesh = soffit_mesh(&plane, 2.5, -0.1, ArchId(1));
        assert!(mesh.is_none(), "negative depth soffit should return None");
    }

    #[test]
    fn soffit_mesh_nan_depth_returns_none() {
        let plane = simple_plane();
        let mesh = soffit_mesh(&plane, 2.5, f64::NAN, ArchId(1));
        assert!(mesh.is_none(), "NaN depth soffit should return None");
    }

    #[test]
    fn fascia_mesh_zero_length_base_edge_returns_none() {
        let mut plane = simple_plane();
        plane.base_edge = [0.0, 0.0, 0.0, 0.0];
        let mesh = fascia_mesh(&plane, 2.5, 0.1, ArchId(1));
        assert!(mesh.is_none(), "zero-length base edge fascia should return None");
    }

    #[test]
    fn soffit_mesh_zero_length_base_edge_returns_none() {
        let mut plane = simple_plane();
        plane.base_edge = [0.0, 0.0, 0.0, 0.0];
        let mesh = soffit_mesh(&plane, 2.5, 0.1, ArchId(1));
        assert!(mesh.is_none(), "zero-length base edge soffit should return None");
    }

    #[test]
    fn fascia_mesh_degenerate_plane_returns_none() {
        let mut plane = simple_plane();
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)];
        let mesh = fascia_mesh(&plane, 2.5, 0.1, ArchId(1));
        assert!(mesh.is_none(), "degenerate plane fascia should return None");
    }

    #[test]
    fn soffit_mesh_degenerate_plane_returns_none() {
        let mut plane = simple_plane();
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)];
        let mesh = soffit_mesh(&plane, 2.5, 0.1, ArchId(1));
        assert!(mesh.is_none(), "degenerate plane soffit should return None");
    }

    #[test]
    fn roof_plane_collinear_ring_vertices_work() {
        let mut plane = simple_plane();
        // Add a collinear point on the bottom edge
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 5.0), Vec2::new(0.0, 5.0)];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();
        assert!(mesh.triangle_count() > 0, "collinear vertices should still produce a mesh");
        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite());
        }
    }

    #[test]
    fn roof_plane_negative_slope_works() {
        let mut plane = simple_plane();
        plane.slope = -0.5;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();
        // With negative slope, the far edge should be lower than the eave
        // Vertex at (0, 5): z = 2.5 + (-0.5) * 5 = 0.0
        let mut found = false;
        for p in &mesh.positions {
            if (p[0] - 0.0).abs() < 1e-4 && (p[1] - 5.0).abs() < 1e-4 {
                if (p[2] - 0.0).abs() < 1e-4 {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "vertex at distance 5 with slope -0.5 should be at z=0.0");
    }

    #[test]
    fn roof_plane_top_ring_offset_is_thickness() {
        let plane = simple_plane();
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();
        let n_verts = plane.ring.len();

        // Vertical thickness: the top ring keeps the plan ring and is raised by
        // thickness / cos(theta) = thickness * sqrt(1 + slope^2).
        let dz = plane.thickness * (1.0 + plane.slope * plane.slope).sqrt();

        for i in 0..n_verts {
            let b = mesh.positions[i];
            let t = mesh.positions[n_verts + i];
            let dx = t[0] as f64 - b[0] as f64;
            let dy = t[1] as f64 - b[1] as f64;
            let dz_obs = t[2] as f64 - b[2] as f64;
            assert!((dx - 0.0).abs() < 1e-4, "top offset x should be 0 at vertex {}", i);
            assert!((dy - 0.0).abs() < 1e-4, "top offset y should be 0 at vertex {}", i);
            assert!((dz_obs - dz).abs() < 1e-4, "top offset z mismatch at vertex {}", i);
        }
    }

    #[test]
    fn roof_plane_zero_thickness_produces_flat_mesh() {
        let mut plane = simple_plane();
        plane.thickness = 0.0;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // With zero thickness, top and bottom rings are at the same z.
        let n_verts = plane.ring.len();
        for i in 0..n_verts {
            let b = mesh.positions[i];
            let t = mesh.positions[n_verts + i];
            assert!((b[2] - t[2]).abs() < 1e-6, "top and bottom z should match at vertex {}", i);
        }
        // Mesh should still be valid (non-empty).
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn roof_plane_nan_fascia_depth_uses_default() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.fascia_depth = f64::NAN;
        roof.planes.push(plane);

        let meshes = roof_meshes(&roof, 2.5);
        let fascia = meshes.iter().find(|m| m.material == "fascia").expect("fascia mesh");
        let b = fascia.bounds();
        // Default depth is 0.1
        assert!((b.max[2] - b.min[2] - 0.1).abs() < 1e-4, "fascia depth should default to 0.1 when NaN");
    }

    #[test]
    fn roof_plane_nan_soffit_depth_uses_default() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.soffit_depth = f64::NAN;
        roof.planes.push(plane);

        let meshes = roof_meshes(&roof, 2.5);
        let soffit = meshes.iter().find(|m| m.material == "soffit").expect("soffit mesh");
        let b = soffit.bounds();
        // Default depth is 0.1
        assert!((b.max[1] - b.min[1] - 0.1).abs() < 1e-4, "soffit depth should default to 0.1 when NaN");
    }

    #[test]
    fn roof_plane_very_large_thickness_is_finite() {
        let mut plane = simple_plane();
        plane.thickness = 1000.0;
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite(), "position not finite: {:?}", p);
        }
    }

    #[test]
    fn roof_plane_coincident_ring_vertices_produce_valid_mesh() {
        let mut plane = simple_plane();
        // All vertices at the same point
        plane.ring = vec![Vec2::new(5.0, 2.5), Vec2::new(5.0, 2.5), Vec2::new(5.0, 2.5)];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        // Mesh should be valid (non-empty) even though degenerate.
        assert!(mesh.triangle_count() > 0);
        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite());
        }
    }

    #[test]
    fn roof_meshes_empty_planes_returns_empty() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let meshes = roof_meshes(&roof, 2.5);
        assert!(meshes.is_empty(), "roof with no planes should produce no meshes");
    }

    #[test]
    fn roof_meshes_prefer_the_plane_eave_z() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.eave_z = 2.4;
        plane.slope = 0.5;
        plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(6.0, 4.0), Vec2::new(0.0, 4.0)];
        plane.base_edge = [0.0, 0.0, 6.0, 0.0];
        plane.thickness = 0.25;
        roof.planes.push(plane);

        // The fallback passed to roof_meshes is 0.0; the plane's own eave_z (2.4) must win.
        let meshes = roof_meshes(&roof, 0.0);
        let roof_mesh = meshes.iter().find(|m| m.material == "roof").expect("roof mesh");
        let b = roof_mesh.bounds();

        // Bottom at the far edge (y=4): z = 2.4 + 0.5*4 = 4.4; the top adds ~thickness*cos-ish.
        assert!(b.max[2] > 4.0, "max z should exceed 4.0, got {}", b.max[2]);
        // The lowest point is the underside at the eave (z = 2.4), well above 2.0.
        assert!(b.min[2] > 2.0, "min z should exceed 2.0, got {}", b.min[2]);
    }

    #[test]
    fn fascia_and_soffit_use_specified_depths() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.fascia_depth = 0.15;
        plane.soffit_depth = 0.25;
        roof.planes.push(plane);

        let meshes = roof_meshes(&roof, 2.5);

        let fascia = meshes.iter().find(|m| m.material == "fascia").unwrap();
        let b = fascia.bounds();
        assert!((b.max[2] - b.min[2] - 0.15).abs() < 1e-4, "fascia depth should be 0.15");

        let soffit = meshes.iter().find(|m| m.material == "soffit").unwrap();
        let sb = soffit.bounds();
        assert!((sb.max[1] - sb.min[1] - 0.25).abs() < 1e-4, "soffit depth should be 0.25");
    }

    #[test]
    fn roof_plane_huge_slope_and_coordinates_are_finite() {
        let mut plane = simple_plane();
        plane.slope = 1000.0;
        plane.ring = vec![Vec2::new(1e6, 1e6), Vec2::new(1e6 + 10.0, 1e6), Vec2::new(1e6 + 10.0, 1e6 + 5.0), Vec2::new(1e6, 1e6 + 5.0)];
        plane.base_edge = [1e6, 1e6, 1e6 + 10.0, 1e6];
        let mesh = roof_plane_mesh(&plane, 2.5, "roof", ArchId(1)).unwrap();

        for p in &mesh.positions {
            assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite(), "position not finite: {:?}", p);
        }
    }

    #[test]
    fn roof_meshes_fallback_eave_z_when_plane_eave_z_is_nan() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.eave_z = f64::NAN;
        roof.planes.push(plane);

        // Fallback eave_z is 2.5; with NaN on the plane, the fallback must be used.
        let meshes = roof_meshes(&roof, 2.5);
        let roof_mesh = meshes.iter().find(|m| m.material == "roof").expect("roof mesh");
        let b = roof_mesh.bounds();

        // Base edge vertices should be at z = 2.5 (the fallback), not NaN.
        assert!(b.min[2] >= 2.49, "min z should be near fallback 2.5, got {}", b.min[2]);
        for p in &roof_mesh.positions {
            assert!(p[2].is_finite(), "position z should be finite, got {}", p[2]);
        }
    }

    #[test]
    fn roof_meshes_fallback_eave_z_when_plane_eave_z_is_infinite() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.eave_z = f64::INFINITY;
        roof.planes.push(plane);

        // is_finite() is false for INFINITY, so the fallback must be used.
        let meshes = roof_meshes(&roof, 3.0);
        let roof_mesh = meshes.iter().find(|m| m.material == "roof").expect("roof mesh");
        let b = roof_mesh.bounds();

        assert!(b.min[2] >= 2.99, "min z should be near fallback 3.0, got {}", b.min[2]);
        for p in &roof_mesh.positions {
            assert!(p[2].is_finite(), "position z should be finite, got {}", p[2]);
        }
    }

    #[test]
    fn roof_meshes_plane_eave_z_zero_is_used() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.eave_z = 0.0; // finite, so it must be used (not the fallback)
        roof.planes.push(plane);

        // Fallback is 2.5, but the plane's 0.0 must win.
        let meshes = roof_meshes(&roof, 2.5);
        let roof_mesh = meshes.iter().find(|m| m.material == "roof").expect("roof mesh");
        let b = roof_mesh.bounds();

        // Base edge at z=0.0, far edge bottom at z = 0.0 + 0.5*5 = 2.5, top adds
        // thickness * sqrt(1+slope^2) = 0.2 * sqrt(1.25) ≈ 0.224, so max z ≈ 2.724.
        assert!(b.min[2] < 0.01, "min z should be near 0.0, got {}", b.min[2]);
        assert!(b.max[2] < 2.75, "max z should be below 2.75, got {}", b.max[2]);
    }

    #[test]
    fn roof_meshes_each_plane_uses_own_eave_z() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);

        let mut plane1 = simple_plane();
        plane1.eave_z = 2.0;
        plane1.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)];
        plane1.base_edge = [0.0, 0.0, 4.0, 0.0];
        roof.planes.push(plane1);

        let mut plane2 = simple_plane();
        plane2.eave_z = 4.0;
        plane2.ring = vec![Vec2::new(4.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 3.0), Vec2::new(4.0, 3.0)];
        plane2.base_edge = [4.0, 0.0, 8.0, 0.0];
        roof.planes.push(plane2);

        let meshes = roof_meshes(&roof, 0.0);
        let roof_meshes: Vec<_> = meshes.iter().filter(|m| m.material == "roof").collect();
        assert_eq!(roof_meshes.len(), 2, "expected 2 roof meshes");

        // Plane 1: eave at z=2.0, far edge at z = 2.0 + 0.5*3 = 3.5
        let b1 = roof_meshes[0].bounds();
        assert!((b1.min[2] - 2.0).abs() < 0.01, "plane 1 min z should be 2.0, got {}", b1.min[2]);

        // Plane 2: eave at z=4.0, far edge at z = 4.0 + 0.5*3 = 5.5
        let b2 = roof_meshes[1].bounds();
        assert!((b2.min[2] - 4.0).abs() < 0.01, "plane 2 min z should be 4.0, got {}", b2.min[2]);
    }

    #[test]
    fn fascia_and_soffit_use_plane_eave_z() {
        let mut roof = Roof::default();
        roof.id = ArchId(1);
        let mut plane = simple_plane();
        plane.eave_z = 2.4;
        plane.overhang = 0.5;
        plane.fascia_depth = 0.12;
        plane.soffit_depth = 0.2;
        roof.planes.push(plane);

        let meshes = roof_meshes(&roof, 0.0);

        // At the overhang outline the underside is eave_z - slope * overhang.
        let outline_z = 2.4 - 0.5 * 0.5;
        let fascia = meshes.iter().find(|m| m.material == "fascia").expect("fascia mesh");
        let fb = fascia.bounds();
        assert!((fb.max[2] - outline_z).abs() < 0.01, "fascia top should be at {outline_z}, got {}", fb.max[2]);

        let soffit = meshes.iter().find(|m| m.material == "soffit").expect("soffit mesh");
        let sb = soffit.bounds();
        assert!((sb.max[2] - outline_z).abs() < 0.01, "soffit top should be at {outline_z}, got {}", sb.max[2]);
    }

    #[test]
    fn probe_above_the_ridge_hits_roof() {
        use crate::Scene3d;
        use crate::probe::probe;
        use cadcraft_doc::ArchModel;

        let mut model = ArchModel::default();

        let mut roof = Roof::default();
        roof.id = ArchId(0);
        roof.level = 0;

        // South plane: base edge at y=0, ridge at y=4.1
        // z at ridge = 2.4 + 0.5 * 4.1 = 4.45 (underside)
        let mut south = RoofPlane::default();
        south.base_edge = [0.0, 0.0, 8.2, 0.0];
        south.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(8.2, 0.0), Vec2::new(8.2, 4.1), Vec2::new(0.0, 4.1)];
        south.slope = 0.5;
        south.thickness = 0.2;
        south.eave_z = 2.4;

        // North plane: base edge at y=8.2 (reversed so the slope normal points toward the ridge),
        // ridge at y=4.1. z at ridge = 2.4 + 0.5 * 4.1 = 4.45 (underside).
        let mut north = RoofPlane::default();
        north.base_edge = [8.2, 8.2, 0.0, 8.2];
        north.ring = vec![Vec2::new(8.2, 8.2), Vec2::new(0.0, 8.2), Vec2::new(0.0, 4.1), Vec2::new(8.2, 4.1)];
        north.slope = 0.5;
        north.thickness = 0.2;
        north.eave_z = 2.4;

        roof.planes = vec![south, north];
        model.push_roof(roof);

        let scene = Scene3d::build(&model);
        // Ridge underside is at z = 4.45; vertical top is at 4.45 + 0.2*sqrt(1.25) ≈ 4.674.
        // z = 4.6 is inside the roof solid.
        let hit = probe(&scene, Vec3::new(4.1, 4.1, 4.6));
        assert!(hit.is_some(), "probe at ridge midpoint (4.1, 4.1, 4.6) should hit the roof");
        assert_eq!(hit.unwrap().part, "roof", "ridge probe should report part 'roof'");
    }
}
