//! Ray-cast probing of 3D meshes: determines which part of the scene
//! a point is contained in, and reports the owning object, part name
//! and level elevation.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use crate::Scene3d;
use crate::mesh::Mesh;
use crate::ray::mesh_crossings_upward;
use cadcraft_doc::ArchId;
use cadcraft_geom::Vec3;

/// Maximum number of triangles to scan for a single probe.
///
/// Probes scan the first N triangles of the mesh and use their parity; this caps
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
    if m.contains("roof") || m.contains("fascia") || m.contains("soffit") || m.contains("shingle") {
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
    // Nudge the point slightly upward so that points on the top face are outside.
    let p = Vec3::new(p.x, p.y, p.z + 1e-6);
    mesh_crossings_upward(p, &mesh.positions, &mesh.indices, MAX_PROBE_TRIANGLES) % 2 == 1
}

/// A human-readable label for a mesh, based on its material and owning object.
///
/// `floor`/`ceiling` materials use the room label; opening-part materials use
/// the opening label; everything else uses the wall label. Never returns an
/// empty string.
pub fn mesh_label(mesh: &Mesh) -> String {
    let m = mesh.material.to_lowercase();
    let label = if m.contains("floor") || m.contains("ceiling") {
        cadcraft_doc::arch::room_label(mesh.object)
    } else if m.contains("door")
        || m.contains("window")
        || m.contains("glass")
        || m.contains("frame")
        || m.contains("casing")
        || m.contains("leaf")
        || m.contains("sill")
        || m.contains("header")
    {
        cadcraft_doc::arch::opening_label(mesh.object)
    } else {
        cadcraft_doc::arch::wall_label(mesh.object)
    };
    if label.is_empty() { format!("object-{}", mesh.object.0) } else { label }
}

/// The level elevation for a mesh.
///
/// Prefers the recorded `level_elevation`; falls back to the mesh
/// bounding box `min.z` when unknown.
pub fn mesh_level(mesh: &Mesh) -> f64 {
    if mesh.level_elevation.is_finite() {
        return mesh.level_elevation;
    }
    mesh.bounds().min[2]
}

/// Probe the scene at point `p`, returning the first mesh that contains it.
///
/// Returns `None` immediately if any coordinate of `p` is non-finite.
/// Otherwise iterates `scene.meshes` in build order and returns the first
/// mesh for which [`probe_mesh`] is true, as a [`Hit`] with the mesh's
/// label, part name and level elevation. First match wins; no merging or
/// sorting.
///
/// `Scene3d::probe` (in `lib.rs`) delegates to this function.
pub fn probe(scene: &Scene3d, p: Vec3) -> Option<Hit> {
    if !p.is_finite() {
        return None;
    }
    for mesh in &scene.meshes {
        if probe_mesh(mesh, p) {
            return Some(Hit { id: mesh_label(mesh), part: part_of(mesh).to_string(), level: mesh_level(mesh) });
        }
    }
    None
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

        Mesh { positions, normals, uvs, indices, ..Mesh::new(material, object) }
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

        Mesh { positions, normals, uvs, indices, ..Mesh::new("ceiling", object) }
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

    use crate::Scene3d;
    use cadcraft_doc::{ArchModel, Level, Room, Wall};
    use cadcraft_geom::Vec2;

    /// South-room geometry from `loop_room_slabs_have_distinct_owners`: a wide
    /// shallow polygon with the loop hole near the bottom-left corner.
    #[test]
    fn probe_mesh_south_room_excludes_hole_near_corner() {
        let mut m = Mesh::new("floor", ArchId(1));
        // Exact south-room polygon from the diagnostic:
        let outer = vec![Vec2::new(9.9, 0.1), Vec2::new(9.9, 2.95), Vec2::new(0.1, 2.95), Vec2::new(0.1, 0.1)];
        // Exact hole polygon from the diagnostic (CCW as stored in the room):
        let hole = vec![Vec2::new(3.55, 0.95), Vec2::new(3.55, 2.05), Vec2::new(2.45, 2.05), Vec2::new(2.45, 0.95)];
        m.push_prism_with_holes(&outer, &[hole], -0.3, 0.0);

        // Point inside the hole (should be excluded from the solid slab):
        assert!(!probe_mesh(&m, Vec3::new(3.0, 1.5, -0.15)), "point in hole should not be inside south-room floor slab");
        // Point in the open south-room area (should be inside):
        assert!(probe_mesh(&m, Vec3::new(1.0, 1.0, -0.15)), "point in solid should be inside south-room floor slab");
    }

    #[test]
    fn probe_mesh_is_not_its_bounding_box() {
        let mut m = Mesh::new("floor", ArchId(1));
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let hole = vec![Vec2::new(1.5, 1.5), Vec2::new(2.5, 1.5), Vec2::new(2.5, 2.5), Vec2::new(1.5, 2.5)];
        m.push_prism_with_holes(&outer, &[hole], -0.3, 0.0);

        // Inside the hole (but inside AABB)
        assert!(!probe_mesh(&m, Vec3::new(2.0, 2.0, -0.15)), "point in hole should not be inside");
        // Inside solid part
        assert!(probe_mesh(&m, Vec3::new(0.5, 0.5, -0.15)), "point in solid should be inside");
    }

    #[test]
    fn probe_mesh_ignores_a_point_above_a_thin_slab() {
        let mut m = Mesh::new("floor", ArchId(1));
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        m.push_prism(&outer, 0.0, 0.02);

        // Above the slab
        assert!(!probe_mesh(&m, Vec3::new(2.0, 2.0, 0.03)), "point above slab should not be inside");
    }

    #[test]
    fn probe_mesh_counts_a_point_in_solid_as_inside() {
        let mut m = Mesh::new("floor", ArchId(1));
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        m.push_prism(&outer, 0.0, 0.02);

        // Inside the slab
        assert!(probe_mesh(&m, Vec3::new(2.0, 2.0, 0.01)), "point inside slab should be inside");
    }

    /// Build the phase-1 house model and its 3D scene.
    ///
    /// Exterior centre lines (0,0)-(10,0)-(10,8)-(0,8), 0.2 m thick;
    /// interior (0,3)-(10,3) and (6,3)-(6,8), 0.1 m thick;
    /// Level 1 at elevation 0, ceiling 2.4, structure 0.3, so a Level-2
    /// slab occupies z 2.4…2.7. Outer faces span x -0.1…10.1, y -0.1…8.1.
    fn house_scene() -> Scene3d {
        let mut model = ArchModel::default();

        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
        model.levels.push(Level { index: 1, elevation: 2.7, ceiling_height: 2.4, ..Default::default() });

        let wall = |id: u32, a: (f64, f64), b: (f64, f64), t: f64| -> Wall {
            Wall { id: ArchId(id), start: Vec2::new(a.0, a.1), end: Vec2::new(b.0, b.1), thickness: t, height: 2.7, ..Default::default() }
        };

        // Exterior walls.
        model.walls.push(wall(1, (0.0, 0.0), (10.0, 0.0), 0.2));
        model.walls.push(wall(2, (10.0, 0.0), (10.0, 8.0), 0.2));
        model.walls.push(wall(3, (10.0, 8.0), (0.0, 8.0), 0.2));
        model.walls.push(wall(4, (0.0, 8.0), (0.0, 0.0), 0.2));

        // Interior walls.
        model.walls.push(wall(5, (0.0, 3.0), (10.0, 3.0), 0.1));
        model.walls.push(wall(6, (6.0, 3.0), (6.0, 8.0), 0.1));

        // Level-2 room providing the floor slab at z 2.4…2.7.
        model.rooms.push(Room {
            id: ArchId(100),
            level: 1,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)],
            ..Default::default()
        });

        Scene3d::build(&model)
    }

    /// Assert the scene actually contains wall and floor meshes before probing,
    /// so a missing floor mesh fails loudly as a scene-builder bug rather than
    /// looking like a probe bug.
    fn assert_scene_has_meshes(scene: &Scene3d) {
        assert!(scene.meshes.iter().any(|m| part_of(m) == "wall"), "scene has no wall mesh");
        assert!(scene.meshes.iter().any(|m| part_of(m) == "floor"), "scene has no floor mesh");
    }

    #[test]
    fn probe_inside_a_wall_hits_the_wall() {
        let scene = house_scene();
        assert_scene_has_meshes(&scene);

        let hit = probe(&scene, Vec3::new(5.0, 0.0, 1.0));
        assert!(hit.is_some(), "probe(5,0,1) should hit the south wall");
        let hit = hit.unwrap();
        assert_eq!(hit.part, "wall");
        assert!(!hit.id.is_empty(), "hit id is empty");
    }

    #[test]
    fn probe_in_the_house_volume_hits_nothing() {
        let scene = house_scene();
        assert_scene_has_meshes(&scene);

        // Inside the house volume; catches ceilings being probed as solids.
        assert!(probe(&scene, Vec3::new(5.0, 4.0, 1.0)).is_none(), "probe(5,4,1) should be None");
    }

    #[test]
    fn probe_inside_a_floor_slab_hits_the_floor() {
        let scene = house_scene();
        assert_scene_has_meshes(&scene);

        // Inside the Level-2 slab (z 2.4…2.7).
        let hit = probe(&scene, Vec3::new(2.0, 1.0, 2.55));
        assert!(hit.is_some(), "probe(2,1,2.55) should hit the floor slab");
        assert_eq!(hit.unwrap().part, "floor");

        // Below the slab: the +z ray crosses bottom (2.4) and top (2.7) —
        // two crossings, even, so not inside.
        assert!(probe(&scene, Vec3::new(2.0, 1.0, 1.0)).is_none(), "probe(2,1,1) should be None");
    }

    #[test]
    fn probe_outside_the_building_hits_nothing() {
        let scene = house_scene();
        assert_scene_has_meshes(&scene);

        assert!(probe(&scene, Vec3::new(-5.0, -5.0, 1.0)).is_none(), "probe(-5,-5,1) should be None");
        assert!(probe(&scene, Vec3::new(5.0, 4.0, 50.0)).is_none(), "probe(5,4,50) should be None");
    }

    #[test]
    fn probe_survives_nan_and_huge_points() {
        let scene = house_scene();
        assert_scene_has_meshes(&scene);

        let all: [f64; 5] = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300, -1e300];

        for &v in &all {
            let non_finite = !v.is_finite();

            // x
            let r = probe(&scene, Vec3::new(v, 0.0, 0.0));
            if non_finite {
                assert!(r.is_none(), "probe(x={v}) should be None");
            }
            // y
            let r = probe(&scene, Vec3::new(0.0, v, 0.0));
            if non_finite {
                assert!(r.is_none(), "probe(y={v}) should be None");
            }
            // z
            let r = probe(&scene, Vec3::new(0.0, 0.0, v));
            if non_finite {
                assert!(r.is_none(), "probe(z={v}) should be None");
            }
        }

        // Finite but far away: no panic, and not inside anything.
        assert!(probe(&scene, Vec3::new(1e6, 1e6, 1e6)).is_none(), "probe(1e6,1e6,1e6) should be None");
    }

    /// Build a minimal scene: Level 0 room (4x4) with walls.
    ///
    /// Level 0: elevation 0.0, structure 0.3, ceiling_height 2.4.
    /// Floor slab spans z -0.3..0.0; ceiling slab spans z 2.38..2.4.
    fn level0_room_scene() -> Scene3d {
        let mut model = ArchModel::default();

        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, structure: 0.3, ..Default::default() });

        let wall = |id: u32, a: (f64, f64), b: (f64, f64)| -> Wall {
            Wall { id: ArchId(id), start: Vec2::new(a.0, a.1), end: Vec2::new(b.0, b.1), thickness: 0.2, height: 2.4, ..Default::default() }
        };

        model.walls.push(wall(1, (0.0, 0.0), (4.0, 0.0)));
        model.walls.push(wall(2, (4.0, 0.0), (4.0, 4.0)));
        model.walls.push(wall(3, (4.0, 4.0), (0.0, 4.0)));
        model.walls.push(wall(4, (0.0, 4.0), (0.0, 0.0)));

        model.rooms.push(Room {
            id: ArchId(100),
            level: 0,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)],
            ..Default::default()
        });

        Scene3d::build(&model)
    }

    #[test]
    fn probe_hits_a_ceiling_slab() {
        let scene = level0_room_scene();
        // Ceiling slab spans z 2.38..2.4; probe at 2.39 (inside).
        let hit = probe(&scene, Vec3::new(2.0, 2.0, 2.39));
        assert!(hit.is_some(), "probe(2,2,2.39) should hit the ceiling slab");
        let hit = hit.unwrap();
        assert_eq!(hit.part, "ceiling");
        assert!(!hit.id.is_empty());
    }

    #[test]
    fn probe_reports_the_level_elevation_not_the_slab_bottom() {
        let scene = level0_room_scene();
        // Ceiling slab spans 2.38..2.4 but level.elevation is 0.0.
        let hit = probe(&scene, Vec3::new(2.0, 2.0, 2.39)).unwrap();
        assert_eq!(hit.level, 0.0, "level should be 0.0 (level elevation), not 2.38 (slab bottom)");
    }

    #[test]
    fn floor_probe_id_starts_with_r() {
        let scene = level0_room_scene();
        // Floor slab spans z -0.3..0.0; probe at -0.15 (inside).
        let hit = probe(&scene, Vec3::new(2.0, 2.0, -0.15)).expect("floor slab hit");
        assert!(hit.id.starts_with('r'), "floor probe id should start with 'r', got {:?}", hit.id);
    }

    #[test]
    fn probe_level_one_slab_reports_zero() {
        let scene = level0_room_scene();
        // Level-1 (elevation 0.0) floor slab bottom is -0.3.
        // The probe must report level.elevation = 0.0, not bounds.min[2] = -0.3.
        let hit = probe(&scene, Vec3::new(2.0, 2.0, -0.15)).expect("floor slab hit");
        assert_eq!(hit.level, 0.0, "Level-0 slab must report 0.0, not -0.3");
    }

    #[test]
    fn mesh_label_uses_the_material_prefix() {
        // Floor material → room label (starts with 'r').
        let floor_mesh = Mesh {
            positions: vec![],
            normals: vec![],
            uvs: Vec::new(),
            indices: Vec::new(),
            material: "floor-slab".to_string(),
            object: ArchId(42),
            ..Mesh::new("", ArchId(42))
        };
        assert!(mesh_label(&floor_mesh).starts_with('r'), "floor mesh label should be a room label");

        // Ceiling material → room label (starts with 'r').
        let ceil_mesh = Mesh {
            positions: vec![],
            normals: vec![],
            uvs: Vec::new(),
            indices: Vec::new(),
            material: "ceiling".to_string(),
            object: ArchId(42),
            ..Mesh::new("", ArchId(42))
        };
        assert!(mesh_label(&ceil_mesh).starts_with('r'), "ceiling mesh label should be a room label");

        // Door material → opening label.
        let door_mesh = Mesh {
            positions: vec![],
            normals: vec![],
            uvs: Vec::new(),
            indices: Vec::new(),
            material: "door-frame".to_string(),
            object: ArchId(42),
            ..Mesh::new("", ArchId(42))
        };
        let door_label = mesh_label(&door_mesh);
        assert!(!door_label.is_empty());

        // Wall material → wall label.
        let wall_mesh = Mesh {
            positions: vec![],
            normals: vec![],
            uvs: Vec::new(),
            indices: Vec::new(),
            material: "wall".to_string(),
            object: ArchId(42),
            ..Mesh::new("", ArchId(42))
        };
        let wall_label = mesh_label(&wall_mesh);
        assert!(!wall_label.is_empty());

        // Unknown material → wall label (fallback).
        let mystery_mesh = Mesh {
            positions: vec![],
            normals: vec![],
            uvs: Vec::new(),
            indices: Vec::new(),
            material: "mystery-thing".to_string(),
            object: ArchId(42),
            ..Mesh::new("", ArchId(42))
        };
        let mystery_label = mesh_label(&mystery_mesh);
        assert!(!mystery_label.is_empty());
    }

    #[test]
    fn probe_mesh_empty_mesh_misses() {
        let m = Mesh::new("wall", ArchId(1));
        assert!(!probe_mesh(&m, Vec3::new(0.0, 0.0, 0.0)), "empty mesh should never contain a point");
    }

    #[test]
    fn probe_mesh_multiple_holes() {
        let mut m = Mesh::new("floor", ArchId(1));
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let hole1 = vec![Vec2::new(1.0, 1.0), Vec2::new(3.0, 1.0), Vec2::new(3.0, 3.0), Vec2::new(1.0, 3.0)];
        let hole2 = vec![Vec2::new(7.0, 7.0), Vec2::new(9.0, 7.0), Vec2::new(9.0, 9.0), Vec2::new(7.0, 9.0)];
        m.push_prism_with_holes(&outer, &[hole1, hole2], -0.3, 0.0);
        assert!(!probe_mesh(&m, Vec3::new(2.0, 2.0, -0.15)), "point in hole1 should not be inside");
        assert!(!probe_mesh(&m, Vec3::new(8.0, 8.0, -0.15)), "point in hole2 should not be inside");
        assert!(probe_mesh(&m, Vec3::new(5.0, 5.0, -0.15)), "point in solid should be inside");
    }

    #[test]
    fn probe_mesh_huge_coordinates() {
        let mut m = Mesh::new("wall", ArchId(1));
        let outer = vec![Vec2::new(1e6, 1e6), Vec2::new(1e6 + 4.0, 1e6), Vec2::new(1e6 + 4.0, 1e6 + 4.0), Vec2::new(1e6, 1e6 + 4.0)];
        m.push_prism(&outer, 0.0, 3.0);
        assert!(probe_mesh(&m, Vec3::new(1e6 + 2.0, 1e6 + 2.0, 1.5)), "point inside huge-coordinate prism should be inside");
        assert!(!probe_mesh(&m, Vec3::new(1e6 + 10.0, 1e6 + 2.0, 1.5)), "point outside huge-coordinate prism should not be inside");
    }

    #[test]
    fn probe_mesh_thin_prism_below_probeable_threshold() {
        let mut m = Mesh::new("floor", ArchId(1));
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        m.push_prism(&outer, 0.0, 1e-7);
        assert!(!probe_mesh(&m, Vec3::new(2.0, 2.0, 5e-8)), "prism thinner than 1e-6 should not be probed");
    }

    #[test]
    fn mesh_level_falls_back_to_bounds_when_nan() {
        let mut m = Mesh::new("wall", ArchId(1));
        m.push_prism(&[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)], 2.0, 3.0);
        // level_elevation is NaN by default from Mesh::new
        assert!(m.level_elevation.is_nan());
        assert_eq!(mesh_level(&m), 2.0);
    }

    #[test]
    fn ceiling_slab_with_z_extent_is_probeable() {
        let mut m = Mesh::new("ceiling", ArchId(1));
        m.push_prism(&[Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)], 2.38, 2.4);
        assert!(is_probeable(&m), "ceiling slab with z extent >= 1e-6 must be probeable");
        assert!(probe_mesh(&m, Vec3::new(2.0, 2.0, 2.39)), "point inside ceiling slab must hit");
    }

    #[test]
    fn probe_mesh_point_on_top_face_is_outside() {
        let mut m = Mesh::new("floor", ArchId(1));
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        m.push_prism(&outer, 0.0, 0.3);
        assert!(!probe_mesh(&m, Vec3::new(2.0, 2.0, 0.3)), "point on top face should not be inside");
    }
}
