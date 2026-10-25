//! Ray-cast probing of 3D meshes: determines which part of the scene
//! a point is contained in, and reports the owning object, part name
//! and level elevation.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use crate::mesh::Mesh;
use crate::ray::{box_tris, crossings_upward};
use cadraft_doc::ArchId;
use cadraft_geom::Vec3;

/// Maximum number of triangles to scan for a single probe.
///
/// Probes scan the first N triangles and use their parity; this caps
/// the work for pathological meshes while keeping the common case
/// (small architectural meshes) exact.
pub(crate) const MAX_PROBE_TRIANGLES: usize = 1_000_000;

/// Classify a mesh by its material name.
///
/// Performs a case-insensitive substring match against a fixed priority
/// list. Unrecognised materials map to `"wall"`.
pub fn part_of(mesh: &Mesh) -> &'static str {
    let m = mesh.material.to_lowercase();
    if m.contains("glass") {
        return "glass";
    }
    if m.contains("frame") {
        return "frame";
    }
    if m.contains("casing") {
        return "casing";
    }
    if m.contains("leaf") {
        return "leaf";
    }
    if m.contains("sill") {
        return "sill";
    }
    if m.contains("header") {
        return "header";
    }
    if m.contains("roof") {
        return "roof";
    }
    if m.contains("ceiling") {
        return "ceiling";
    }
    if m.contains("floor") {
        return "floor";
    }
    if m.contains("wall") {
        return "wall";
    }
    "wall"
}

/// Whether a mesh can be probed (i.e. it is a closed volume).
///
/// Ceilings are single-sided quads and would report a hit for every
/// point below them; they are excluded. Meshes with negligible z-extent
/// (e.g. flat decals) are also excluded.
pub fn is_probeable(mesh: &Mesh) -> bool {
    if part_of(mesh) == "ceiling" {
        return false;
    }
    let b = mesh.bounds();
    let z_extent = b.max[2] - b.min[2];
    z_extent >= 1e-6
}

/// A hit reported by a probe: the owning object id, the part name,
/// and the level elevation.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    pub part: String,
    pub level: f64,
}

/// Probe a single mesh at point `p`.
///
/// Returns `true` when `p` is inside the mesh's volume (odd number of
/// upward ray crossings). Short-circuited by the bounding box and by
/// [`is_probeable`]. When the mesh has more than
/// [`MAX_PROBE_TRIANGLES`] triangles, only the first N are scanned and
/// their parity is used.
pub fn probe_mesh(mesh: &Mesh, p: Vec3) -> bool {
    if !is_probeable(mesh) {
        return false;
    }
    if !p.is_finite() {
        return false;
    }
    let b = mesh.bounds();
    if p.x < b.min[0] || p.x > b.max[0] || p.y < b.min[1] || p.y > b.max[1] || p.z < b.min[2] || p.z > b.max[2] {
        return false;
    }
    let tris = box_tris(mesh);
    let scan_count = tris.len().min(MAX_PROBE_TRIANGLES);
    crossings_upward(p, &tris[..scan_count]) % 2 == 1
}

/// A human-readable label for a mesh, based on its owning object.
///
/// Falls back to the `ArchId` display string; never returns an empty string.
pub fn mesh_label(mesh: &Mesh) -> String {
    let label = format!("{}", mesh.object);
    if label.is_empty() { format!("object-{}", mesh.object.0) } else { label }
}

/// The level elevation for a mesh.
///
/// Falls back to the mesh bounding box `min.z` when no level is
/// recorded on the mesh.
pub fn mesh_level(mesh: &Mesh) -> f64 {
    mesh.bounds().min[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a closed box (cube) mesh from `(x0, y0, z0)` with side `size`.
    fn box_mesh(x0: f64, y0: f64, z0: f64, size: f64, material: &str, object: ArchId) -> Mesh {
        let x0 = x0 as f32;
        let y0 = y0 as f32;
        let z0 = z0 as f32;
        let s = size as f32;

        let positions: Vec<[f32; 3]> = vec![
            [x0, y0, z0],
            [x0 + s, y0, z0],
            [x0 + s, y0 + s, z0],
            [x0, y0 + s, z0],
            [x0, y0, z0 + s],
            [x0 + s, y0, z0 + s],
            [x0 + s, y0 + s, z0 + s],
            [x0, y0 + s, z0 + s],
        ];

        let normals: Vec<[f32; 3]> = vec![
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ];

        let uvs: Vec<[f32; 2]> = (0..8).map(|_| [0.0, 0.0]).collect();

        let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2, 7, 6, 0, 3, 7, 0, 7, 4, 1, 5, 6, 1, 6, 2];

        Mesh { positions, normals, uvs, indices, material: material.to_string(), object }
    }

    /// A flat ceiling quad at z = `z0`, spanning `(x0, y0)` to `(x0+s, y0+s)`.
    fn ceiling_quad(x0: f64, y0: f64, z0: f64, size: f64, object: ArchId) -> Mesh {
        let x0 = x0 as f32;
        let y0 = y0 as f32;
        let z0 = z0 as f32;
        let s = size as f32;

        let positions: Vec<[f32; 3]> = vec![[x0, y0, z0], [x0 + s, y0, z0], [x0 + s, y0 + s, z0], [x0, y0 + s, z0]];

        let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0]];

        let uvs: Vec<[f32; 2]> = (0..4).map(|_| [0.0, 0.0]).collect();

        let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3];

        Mesh { positions, normals, uvs, indices, material: "ceiling".to_string(), object }
    }

    #[test]
    fn part_of_maps_material_names() {
        let base = box_mesh(0.0, 0.0, 0.0, 1.0, "wall", ArchId(1));
        assert_eq!(part_of(&base), "wall");

        let glass = box_mesh(0.0, 0.0, 0.0, 1.0, "glass-pane", ArchId(2));
        assert_eq!(part_of(&glass), "glass");

        let frame = box_mesh(0.0, 0.0, 0.0, 1.0, "door-frame", ArchId(3));
        assert_eq!(part_of(&frame), "frame");

        let casing = box_mesh(0.0, 0.0, 0.0, 1.0, "window-casing", ArchId(4));
        assert_eq!(part_of(&casing), "casing");

        let leaf = box_mesh(0.0, 0.0, 0.0, 1.0, "door-leaf", ArchId(5));
        assert_eq!(part_of(&leaf), "leaf");

        let sill = box_mesh(0.0, 0.0, 0.0, 1.0, "window-sill", ArchId(6));
        assert_eq!(part_of(&sill), "sill");

        let header = box_mesh(0.0, 0.0, 0.0, 1.0, "door-header", ArchId(7));
        assert_eq!(part_of(&header), "header");

        let roof = box_mesh(0.0, 0.0, 0.0, 1.0, "roof-deck", ArchId(8));
        assert_eq!(part_of(&roof), "roof");

        let ceil = box_mesh(0.0, 0.0, 0.0, 1.0, "ceiling-tile", ArchId(9));
        assert_eq!(part_of(&ceil), "ceiling");

        let floor = box_mesh(0.0, 0.0, 0.0, 1.0, "floor-slab", ArchId(10));
        assert_eq!(part_of(&floor), "floor");

        // Unrecognised -> "wall"
        let unknown = box_mesh(0.0, 0.0, 0.0, 1.0, "mystery-material", ArchId(11));
        assert_eq!(part_of(&unknown), "wall");

        // Priority: "glass" wins over "frame" in "glass-frame"
        let mixed = box_mesh(0.0, 0.0, 0.0, 1.0, "glass-frame", ArchId(12));
        assert_eq!(part_of(&mixed), "glass");
    }

    #[test]
    fn probe_mesh_inside_a_closed_box_hits() {
        let mesh = box_mesh(0.0, 0.0, 0.0, 1.0, "wall", ArchId(1));
        // Point strictly inside the box.
        assert!(probe_mesh(&mesh, Vec3::new(0.5, 0.5, 0.5)));
    }

    #[test]
    fn probe_mesh_below_and_above_a_closed_box_misses() {
        let mesh = box_mesh(0.0, 0.0, 0.0, 1.0, "wall", ArchId(1));
        // Below the box.
        assert!(!probe_mesh(&mesh, Vec3::new(0.5, 0.5, -0.1)));
        // Above the box.
        assert!(!probe_mesh(&mesh, Vec3::new(0.5, 0.5, 1.1)));
    }

    #[test]
    fn probe_mesh_skips_single_sided_surfaces() {
        let mesh = ceiling_quad(0.0, 0.0, 1.0, 1.0, ArchId(1));
        // A point "inside" where a ceiling would naively hit.
        assert!(!probe_mesh(&mesh, Vec3::new(0.5, 0.5, 0.5)));
    }

    #[test]
    fn probe_mesh_rejects_points_outside_its_bounds() {
        let mesh = box_mesh(0.0, 0.0, 0.0, 1.0, "wall", ArchId(1));
        // Far outside in x.
        assert!(!probe_mesh(&mesh, Vec3::new(10.0, 0.5, 0.5)));
        // Far outside in y.
        assert!(!probe_mesh(&mesh, Vec3::new(0.5, 10.0, 0.5)));
    }

    #[test]
    fn probe_mesh_ignores_non_finite_points() {
        let mesh = box_mesh(0.0, 0.0, 0.0, 1.0, "wall", ArchId(1));
        assert!(!probe_mesh(&mesh, Vec3::new(f64::NAN, 0.5, 0.5)));
        assert!(!probe_mesh(&mesh, Vec3::new(0.5, f64::INFINITY, 0.5)));
        assert!(!probe_mesh(&mesh, Vec3::new(0.5, 0.5, f64::NEG_INFINITY)));
    }
}
