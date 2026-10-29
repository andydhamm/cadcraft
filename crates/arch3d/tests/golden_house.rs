//! Golden-scene test for the 10×8 m demo house.
//!
//! Loads `fixtures/house_10x8.json`, builds a `Scene3d`, and asserts:
//! - mesh / triangle counts,
//! - world-space bounds,
//! - a stable FNV-1a hash over the concatenated mesh bytes,
//! - that `build_dirty` on a single wall reproduces a full `build`.
//!
//! The expected hash constant is written in by the developer after the first
//! successful run.  Update it if the geometry changes intentionally.

use cadcraft_arch3d::{Scene3d, build, build_dirty};
use cadcraft_doc::geom::Vec2;
use cadcraft_doc::{ArchId, ArchModel, Wall};

/// FNV-1a 64-bit hash (no external dependency).
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x10000_0001b3);
    }
    hash
}

/// Concatenate all mesh position/UV/index bytes into one
/// deterministic byte sequence for hashing.
fn scene_bytes(scene: &Scene3d) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    for mesh in &scene.meshes {
        // object id (4 bytes)
        buf.extend_from_slice(&mesh.object.0.to_le_bytes());
        // per-vertex: position (3 × f32)
        for p in &mesh.positions {
            buf.extend_from_slice(&p[0].to_le_bytes());
            buf.extend_from_slice(&p[1].to_le_bytes());
            buf.extend_from_slice(&p[2].to_le_bytes());
        }
        // per-vertex UVs
        for uv in &mesh.uvs {
            buf.extend_from_slice(&uv[0].to_le_bytes());
            buf.extend_from_slice(&uv[1].to_le_bytes());
        }
        // index count
        buf.extend_from_slice(&mesh.indices.len().to_le_bytes());
        for i in &mesh.indices {
            buf.extend_from_slice(&i.to_le_bytes());
        }
    }
    buf
}

/// Build the 10×8 m demo house model programmatically (matches the fixture JSON).
fn programmatic_house() -> ArchModel {
    let mut model = ArchModel::default();

    // Exterior walls: 4 walls forming a 10×8 m rectangle
    let exterior_walls = [
        (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
        (Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)),
        (Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)),
        (Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)),
    ];
    for (start, end) in exterior_walls {
        model.add_wall(Wall { start, end, thickness: 0.3, height: 2.7, exterior: true, wall_type: "Default".to_string(), ..Default::default() });
    }

    // Interior walls: 2 walls dividing the space
    let interior_walls = [(Vec2::new(0.0, 4.0), Vec2::new(10.0, 4.0)), (Vec2::new(5.0, 0.0), Vec2::new(5.0, 8.0))];
    for (start, end) in interior_walls {
        model.add_wall(Wall { start, end, thickness: 0.15, height: 2.7, exterior: false, wall_type: "Default".to_string(), ..Default::default() });
    }

    model
}

/// Build the 10×8 m demo house by parsing `fixtures/house_10x8.json`.
///
/// Walls are added through [`ArchModel::add_wall`] in file order so that
/// their assigned ids match those produced by [`programmatic_house`].
fn load_fixture() -> ArchModel {
    let json = include_str!("fixtures/house_10x8.json");
    let parsed: ArchModel = serde_json::from_str(json).expect("fixtures/house_10x8.json must be valid JSON");
    let mut model = ArchModel::default();
    for wall in parsed.walls {
        model.add_wall(wall);
    }
    model
}

/// The JSON fixture must produce exactly the same wall data as the programmatic
/// builder so that the two paths stay in sync.
#[test]
fn fixture_json_matches_the_programmatic_house() {
    assert_eq!(load_fixture().walls, programmatic_house().walls);
}

/// The 10×8 m house: 4 exterior walls + 2 interior walls produce a fixed,
/// deterministic scene.  Assert counts, bounds, and a stable FNV-1a hash.
///
/// After the first successful build, paste the printed hash into `GOLDEN_HASH`
/// and re-run.  Any subsequent change to the geometry will surface as a hash
/// mismatch.
#[test]
fn golden_house_scene_hash() {
    let model = load_fixture();
    let scene = build(&model);

    // Deterministic sanity checks on the counts.
    let mesh_count = scene.meshes.len();
    let tri_count: usize = scene.meshes.iter().map(|m| m.indices.len() / 3).sum();
    assert!(mesh_count > 0, "scene has no meshes");
    assert!(tri_count > 0, "scene has no triangles");

    // Bounds: a 10×8 m house with 0.3 m exterior walls should span roughly
    // x ∈ [-0.15, 10.15], y ∈ [-0.15, 8.15], z ∈ [0, 2.7].
    let (mut min_x, mut min_y, mut min_z) = (f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y, mut max_z) = (f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for mesh in &scene.meshes {
        for p in &mesh.positions {
            min_x = min_x.min(p[0] as f64);
            max_x = max_x.max(p[0] as f64);
            min_y = min_y.min(p[1] as f64);
            max_y = max_y.max(p[1] as f64);
            min_z = min_z.min(p[2] as f64);
            max_z = max_z.max(p[2] as f64);
        }
    }
    // Allow ±0.5 m tolerance for wall offset.
    assert!(min_x > -1.0 && max_x < 11.0, "x bounds: {}..{}", min_x, max_x);
    assert!(min_y > -1.0 && max_y < 9.0, "y bounds: {}..{}", min_y, max_y);
    assert!((min_z - 0.0).abs() < 0.1, "min_z: {}", min_z);
    assert!((max_z - 2.7).abs() < 0.1, "max_z: {}", max_z);

    let hash = fnv1a_hash(&scene_bytes(&scene));
    println!("golden hash = {hash:016x} (meshes={}, tris={})", mesh_count, tri_count);

    // Golden value recorded on the first successful build; update this if the
    // geometry changes intentionally.
    const GOLDEN_HASH: u64 = 0x9ca6417af0059c50;
    assert_eq!(hash, GOLDEN_HASH, "scene hash drift: {hash:016x} != GOLDEN_HASH");
}

/// `build_dirty` with a single wall marked dirty must produce the same scene
/// as a full `build`.  This validates the incremental path.
#[test]
fn build_dirty_equals_full_build() {
    let model = load_fixture();
    let full = build(&model);

    // Keep only the first wall marked dirty; recompute from a "previous"
    // scene that already has all other walls.
    let first_id = full.meshes.first().map(|m| m.object).expect("no meshes");
    let mut dirty_set: std::collections::BTreeSet<cadcraft_arch::ArchId> = std::collections::BTreeSet::new();
    dirty_set.insert(first_id);

    let dirty_scene = build_dirty(&model, &dirty_set, &full);

    // The resulting mesh count and total triangle count must match.
    assert_eq!(dirty_scene.meshes.len(), full.meshes.len(), "build_dirty changed mesh count");
    let tri_full: usize = full.meshes.iter().map(|m| m.indices.len() / 3).sum();
    let tri_dirty: usize = dirty_scene.meshes.iter().map(|m| m.indices.len() / 3).sum();
    assert_eq!(tri_full, tri_dirty, "build_dirty changed triangle count");
    assert_eq!(scene_bytes(&dirty_scene), scene_bytes(&full), "build_dirty scene bytes differ from full build");
}

/// The two interior walls cross at (5, 4); after the X-crossing fix each keeps
/// its full length (the crossing joins nothing).
#[test]
fn crossing_interior_walls_keep_their_full_length() {
    let model = load_fixture();
    let scene = build(&model);

    let mesh4 = scene.meshes.iter().find(|m| m.object == ArchId(4)).expect("no mesh for ArchId(4)");
    let mesh5 = scene.meshes.iter().find(|m| m.object == ArchId(5)).expect("no mesh for ArchId(5)");

    // Wall 4 (0,4)-(10,4) spans x 0..10.
    let (mut min_x4, mut max_x4) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &mesh4.positions {
        min_x4 = min_x4.min(p[0] as f64);
        max_x4 = max_x4.max(p[0] as f64);
    }
    assert!((min_x4 - 0.0).abs() < 0.2, "wall 4 min.x: {}", min_x4);
    assert!((max_x4 - 10.0).abs() < 0.2, "wall 4 max.x: {}", max_x4);

    // Wall 5 (5,0)-(5,8) spans y 0..8.
    let (mut min_y5, mut max_y5) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &mesh5.positions {
        min_y5 = min_y5.min(p[1] as f64);
        max_y5 = max_y5.max(p[1] as f64);
    }
    assert!((min_y5 - 0.0).abs() < 0.2, "wall 5 min.y: {}", min_y5);
    assert!((max_y5 - 8.0).abs() < 0.2, "wall 5 max.y: {}", max_y5);
}

/// build_dirty with all walls marked dirty must equal a full build.
#[test]
fn build_dirty_with_all_walls_dirty_equals_full_build() {
    let model = load_fixture();
    let full = build(&model);

    let dirty_set: std::collections::BTreeSet<cadcraft_arch::ArchId> = full.meshes.iter().map(|m| m.object).collect();
    let dirty_scene = build_dirty(&model, &dirty_set, &full);

    assert_eq!(scene_bytes(&dirty_scene), scene_bytes(&full), "build_dirty(all) differs from full build");
}

/// build_dirty with an empty dirty set must return the previous scene unchanged.
#[test]
fn build_dirty_with_empty_dirty_set_returns_previous() {
    let model = load_fixture();
    let full = build(&model);

    let empty: std::collections::BTreeSet<cadcraft_arch::ArchId> = std::collections::BTreeSet::new();
    let unchanged = build_dirty(&model, &empty, &full);

    assert_eq!(scene_bytes(&unchanged), scene_bytes(&full), "build_dirty(empty) changed the scene");
}

/// A single wall builds a scene with exactly one mesh.
#[test]
fn single_wall_builds_one_mesh() {
    let mut model = ArchModel::default();
    model.add_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(3.0, 0.0),
        thickness: 0.2,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = build(&model);
    assert_eq!(scene.meshes.len(), 1, "single wall should produce exactly one mesh");
    let tri_count = scene.meshes[0].indices.len() / 3;
    assert!(tri_count > 0, "single wall mesh has no triangles");
}

/// A zero-length wall (start == end) must not crash and must not produce a mesh.
#[test]
fn zero_length_wall_is_skipped() {
    let mut model = ArchModel::default();
    model.add_wall(Wall {
        start: Vec2::new(1.0, 1.0),
        end: Vec2::new(1.0, 1.0),
        thickness: 0.2,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = build(&model);
    assert!(scene.meshes.is_empty(), "zero-length wall should produce no mesh");
}

/// Building the same model twice must produce byte-identical scenes.
#[test]
fn scene_bytes_are_deterministic_across_builds() {
    let model = load_fixture();
    let scene1 = build(&model);
    let scene2 = build(&model);

    assert_eq!(scene_bytes(&scene1), scene_bytes(&scene2), "two builds of the same model differ");
}

/// Two collinear walls on the same line build without error.
#[test]
fn collinear_walls_build_without_error() {
    let mut model = ArchModel::default();
    model.add_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(5.0, 0.0),
        thickness: 0.2,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });
    model.add_wall(Wall {
        start: Vec2::new(5.0, 0.0),
        end: Vec2::new(10.0, 0.0),
        thickness: 0.2,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = build(&model);
    assert_eq!(scene.meshes.len(), 2, "two collinear walls should produce two meshes");
}

/// A wall with huge coordinates builds without overflow.
#[test]
fn huge_coordinate_wall_builds_without_error() {
    let mut model = ArchModel::default();
    model.add_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(1e6, 1e6),
        thickness: 0.2,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = build(&model);
    assert_eq!(scene.meshes.len(), 1, "huge wall should produce one mesh");
}

/// A wall with zero thickness builds without crashing (degenerate but valid).
#[test]
fn zero_thickness_wall_builds_without_crash() {
    let mut model = ArchModel::default();
    model.add_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(3.0, 0.0),
        thickness: 0.0,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = build(&model);
    // A zero-thickness wall may or may not produce a mesh depending on the
    // implementation, but it must not crash.
    assert!(scene.meshes.len() <= 1, "zero-thickness wall produced too many meshes");
}

/// A wall with NaN coordinates is handled gracefully (no mesh, no crash).
#[test]
fn nan_coordinate_wall_is_handled() {
    let mut model = ArchModel::default();
    model.add_wall(Wall {
        start: Vec2::new(f64::NAN, 0.0),
        end: Vec2::new(3.0, 0.0),
        thickness: 0.2,
        height: 2.5,
        exterior: true,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = build(&model);
    assert!(scene.meshes.is_empty(), "NaN wall should produce no mesh");
}
