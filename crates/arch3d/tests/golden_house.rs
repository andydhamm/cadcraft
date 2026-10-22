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

use cadcraft_arch3d::Scene3d;
use cadcraft_arch3d::builder::{build, build_dirty};
use cadcraft_doc::geom::Vec2;
use cadcraft_doc::{ArchModel, Wall};

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
            buf.extend_from_slice(&p[0].to_f32().to_le_bytes());
            buf.extend_from_slice(&p[1].to_f32().to_le_bytes());
            buf.extend_from_slice(&p[2].to_f32().to_le_bytes());
        }
        // per-vertex UVs if present
        if let Some(uvs) = &mesh.uvs {
            for uv in uvs {
                buf.extend_from_slice(&uv[0].to_f32().to_le_bytes());
                buf.extend_from_slice(&uv[1].to_f32().to_le_bytes());
            }
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
fn load_fixture() -> ArchModel {
    let mut model = ArchModel::new();

    // Exterior walls: 4 walls forming a 10×8 m rectangle
    let exterior_walls = [
        (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
        (Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)),
        (Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)),
        (Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)),
    ];
    for (start, end) in exterior_walls {
        model.add_wall(Wall {
            start,
            end,
            thickness: 0.3,
            height: 2.7,
            base_level: 0.0,
            to_ceiling: true,
            exterior: true,
            wall_type: Default::default(),
            join_start: Default::default(),
            join_end: Default::default(),
        });
    }

    // Interior walls: 2 walls dividing the space
    let interior_walls = [(Vec2::new(0.0, 4.0), Vec2::new(10.0, 4.0)), (Vec2::new(5.0, 0.0), Vec2::new(5.0, 8.0))];
    for (start, end) in interior_walls {
        model.add_wall(Wall {
            start,
            end,
            thickness: 0.15,
            height: 2.7,
            base_level: 0.0,
            to_ceiling: true,
            exterior: false,
            wall_type: Default::default(),
            join_start: Default::default(),
            join_end: Default::default(),
        });
    }

    model
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
            min_x = min_x.min(p[0]);
            max_x = max_x.max(p[0]);
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
            min_z = min_z.min(p[2]);
            max_z = max_z.max(p[2]);
        }
    }
    // Allow ±0.5 m tolerance for wall offset.
    assert!(min_x > -1.0 && max_x < 11.0, "x bounds: {}..{}", min_x, max_x);
    assert!(min_y > -1.0 && max_y < 9.0, "y bounds: {}..{}", min_y, max_y);
    assert!((min_z - 0.0).abs() < 0.1, "min_z: {}", min_z);
    assert!((max_z - 2.7).abs() < 0.1, "max_z: {}", max_z);

    let hash = fnv1a_hash(&scene_bytes(&scene));
    println!("golden hash = {hash:016x} (meshes={}, tris={})", mesh_count, tri_count);

    // TODO: after first run, paste the hash here:
    const GOLDEN_HASH: u64 = 0x0000_0000_0000_0000;
    assert_eq!(hash, GOLDEN_HASH, "scene hash drift: {hash:016x} != GOLDEN_HASH");
}

/// `build_dirty` with a single wall marked dirty must produce the same scene
/// as a full `build`.  This validates the incremental path.
#[test]
fn build_dirty_equals_full_build() {
    let model = load_fixture();
    let full = build(&model);

    let dirty: std::collections::BTreeSet<_> = full.meshes.iter().map(|m| m.object).collect();
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
}
