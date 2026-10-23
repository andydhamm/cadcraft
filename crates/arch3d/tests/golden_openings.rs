//! Golden scene and property tests for openings.
//!
//! Loads `fixtures/house_openings.json` (a 10 × 8 m house, one interior wall, a
//! hinged door, a sliding door and two windows) and asserts:
//!
//! * the built `Scene3d` is byte-stable across two runs;
//! * every mesh is a closed 2-manifold;
//! * the host wall's mesh has strictly fewer triangles than the same wall with
//!   no openings (the hole really cuts);
//! * the scene bounds still match the 10.2 × 8.2 × 2.4 envelope;
//! * the GLB hash of `cadcraft_io::glb::write(&scene)` matches a stored constant;
//! * `build_dirty` after moving one opening equals a full `build`.
//!
//! No external dependencies: the GLB hash is a dependency-free FNV-1a checksum.

use std::collections::BTreeSet;
use std::fs;

use cadcraft_arch3d::{build, build_dirty, Bounds3, Mesh, Scene3d};
use cadcraft_doc::arch::OpeningKind;
use cadcraft_doc::{ArchId, ArchModel, Wall};
use cadcraft_geom::Vec2;

/// The stored GLB hash of the golden scene.
///
//! Filled in by running `cargo test -p cadcraft_arch3d golden_opening_scene_is_deterministic`
/// once and pasting the printed value here. The test prints the real hash to stderr
/// whenever it mismatches.
const GLB_HASH: u64 = 0x0000_0000_0000_0000; // <-- paste the printed hash here

/// A 64-bit FNV-1a checksum (dependency-free).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

/// Load the golden `ArchModel` from `fixtures/house_openings.json`.
fn load_model() -> ArchModel {
    let path = std::path::Path::new("crates/arch3d/tests/fixtures/house_openings.json");
    let src = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("cannot read {}: {}", path.display(), e)
    });
    serde_json::from_str(&src).unwrap_or_else(|e| {
        panic!("cannot parse {}: {}", path.display(), e)
    })
}

/// Build the golden scene.
fn golden_scene() -> Scene3d {
    build(&load_model())
}

/// The built `Scene3d` is byte-stable across two runs.
#[test]
fn golden_opening_scene_is_deterministic() {
    let s1 = golden_scene();
    let s2 = golden_scene();

    // Byte-stable: every mesh field matches across the two builds.
    assert_eq!(s1.meshes.len(), s2.meshes.len());
    for (a, b) in s1.meshes.iter().zip(s2.meshes.iter()) {
        assert_eq!(a.object, b.object);
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.normals, b.normals);
        assert_eq!(a.uvs, b.uvs);
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.material, b.material);
    }
    assert_eq!(s1.bounds, s2.bounds);

    // Stored GLB hash matches.
    let h1 = fnv1a(&cadcraft_io::glb::write(&s1));
    let h2 = fnv1a(&cadcraft_io::glb::write(&s2));
    assert_eq!(h1, h2, "GLB is not deterministic across two builds");
    if h1 != GLB_HASH {
        eprintln!(
            "GLB_HASH mismatch: computed {h1:#018x}, stored {GLB_HASH:#018x}. Run the test once and paste {h1:#018x} into GLB_HASH."
        );
    }
    assert_eq!(h1, GLB_HASH, "GLB hash drift: computed {h1:#018x} != stored {GLB_HASH:#018x}");
}

/// The host wall's mesh has strictly fewer triangles than the same wall with no
/// openings (the hole really cuts).
#[test]
fn cutting_an_opening_reduces_the_wall_triangle_count() {
    let model = load_model();
    let scene = build(&model);

    // Find the host wall that carries the most openings (the south wall has the
    // hinged door; the north wall has two windows).
    let mut host: Option<ArchId> = None;
    let mut max_openings: usize = 0;
    for o in &model.openings {
        let n = model.openings_on(o.host).len();
        if n > max_openings {
            max_openings = n;
            host = Some(o.host);
        }
    }
    let host = host.expect("golden model has no openings");
    let host_wall = model.wall(host).unwrap();

    // Build the same wall *without* any openings and count its triangles.
    let no_openings: ArchModel = {
        let mut m = model.clone();
        m.openings.clear();
        m
    };
    let no_open_scene = build(&no_openings);
    let wall_mesh = scene.mesh_for(&host).expect("no mesh for the host wall");
    let no_open_mesh = no_open_scene.mesh_for(&host).expect("no mesh for the host wall without openings");
    let with_tri = wall_mesh.triangle_count();
    let without_tri = no_open_mesh.triangle_count();
    assert!(
        with_tri < without_tri,
        "host wall {} with openings ({with_tri} tris) should have fewer triangles than without ({without_tri})",
        host.0
    );
}

/// Every opening mesh is a closed 2-manifold.
#[test]
fn all_opening_meshes_are_manifold() {
    let scene = golden_scene();
    for m in &scene.meshes {
        assert!(m.is_manifold(), "mesh {} (object {}) is not a closed manifold", m.material, m.object.0);
    }
}

/// `build_dirty` after moving one opening equals a full `build`.
#[test]
fn build_dirty_after_an_opening_move_equals_build() {
    let model = load_model();
    let full = build(&model);

    // Move the first opening along its host wall (a small offset change).
    let mut moved = model.clone();
    if let Some(o) = moved.openings.first_mut() {
        o.offset += 0.25;
    }
    let dirty: BTreeSet<ArchId> = BTreeSet::from([
        moved.openings.iter().map(|o| o.id).collect(),
    ]);
    let partial = build_dirty(&moved, &dirty, &full);

    // A full rebuild of the moved model must equal the dirty rebuild.
    let full_moved = build(&moved);
    assert_eq!(partial.meshes.len(), full_moved.meshes.len());
    for (a, b) in partial.meshes.iter().zip(full_moved.meshes.iter()) {
        assert_eq!(a.object, b.object);
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.normals, b.normals);
        assert_eq!(a.uvs, b.uvs);
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.material, b.material);
    }
    assert_eq!(partial.bounds, full_moved.bounds);
}

/// The scene bounds still match the 10.2 × 8.2 × 2.4 envelope.
#[test]
fn golden_scene_bounds_match_envelope() {
    let scene = golden_scene();
    let b = scene.bounds;
    // 10.2 × 8.2 in plan (10 × 8 walls + 0.2 thickness), 2.4 m tall.
    assert!((b.max[0] - 10.2).abs() < 1e-4, "x max {} != 10.2", b.max[0]);
    assert!((b.max[1] - 8.2).abs() < 1e-4, "y max {} != 8.2", b.max[1]);
    assert!((b.max[2] - 2.4).abs() < 1e-4, "z max {} != 2.4", b.max[2]);
    assert!((b.min[0] + 0.1).abs() < 1e-4, "x min {} != -0.1", b.min[0]);
    assert!((b.min[1] + 0.1).abs() < 1e-4, "y min {} != -0.1", b.min[1]);
    assert!((b.min[2] + 0.0).abs() < 1e-4, "z min {} != 0.0", b.min[2]);
}
