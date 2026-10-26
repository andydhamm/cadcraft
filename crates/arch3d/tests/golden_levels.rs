//! Two-storey floor-slab probe golden test.
//!
//! Builds a two-storey `ArchModel` and asserts that the 3D probe
//! correctly identifies the floor slabs of both storeys at the expected
//! points.

use cadcraft_arch::area::refresh_areas;
use cadcraft_arch::room::detect_rooms;
use cadcraft_arch3d::Mesh;
use cadcraft_arch3d::build;
use cadcraft_doc::arch::room_label;
use cadcraft_doc::{ArchModel, Level, Wall};
use cadcraft_geom::point_in_polygon;
use cadcraft_geom::{Vec2, Vec3};

/// Build the phase-1 house with a 0.1 m loop wall, detect rooms, and build the 3D scene.
fn house_with_loop() -> (ArchModel, cadcraft_arch3d::Scene3d) {
    let mut model = ArchModel::default();

    // Exterior rectangle centre lines.
    let exterior = [
        (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
        (Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)),
        (Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)),
        (Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)),
    ];

    for (start, end) in exterior.iter() {
        model.push_wall(Wall {
            start: start.clone(),
            end: end.clone(),
            thickness: 0.2,
            height: 2.7,
            base_level: 0,
            exterior: true,
            ..Wall::default()
        });
    }

    // Interior walls.
    model.push_wall(Wall { start: Vec2::new(0.0, 3.0), end: Vec2::new(10.0, 3.0), thickness: 0.1, height: 2.7, base_level: 0, ..Wall::default() });
    model.push_wall(Wall { start: Vec2::new(6.0, 3.0), end: Vec2::new(6.0, 8.0), thickness: 0.1, height: 2.7, base_level: 0, ..Wall::default() });

    // Closed 0.1 m loop wall.
    let loop_walls = [
        (Vec2::new(2.5, 1.0), Vec2::new(3.5, 1.0)),
        (Vec2::new(3.5, 1.0), Vec2::new(3.5, 2.0)),
        (Vec2::new(3.5, 2.0), Vec2::new(2.5, 2.0)),
        (Vec2::new(2.5, 2.0), Vec2::new(2.5, 1.0)),
    ];
    for (start, end) in loop_walls.iter() {
        model.push_wall(Wall { start: start.clone(), end: end.clone(), thickness: 0.1, height: 2.7, base_level: 0, ..Wall::default() });
    }

    let rooms = detect_rooms(&model);
    for r in rooms {
        model.push_room(r);
    }
    refresh_areas(&mut model);

    let scene = build(&model);
    (model, scene)
}

/// Sum the areas of triangles in `mesh` whose three vertices all sit at height `z`.
fn cap_area(mesh: &Mesh, z: f64) -> f64 {
    let mut total = 0.0;
    for chunk in mesh.indices.chunks(3) {
        let a = mesh.positions[chunk[0] as usize];
        let b = mesh.positions[chunk[1] as usize];
        let c = mesh.positions[chunk[2] as usize];
        if (a[2] as f64 - z).abs() < 1e-6 && (b[2] as f64 - z).abs() < 1e-6 && (c[2] as f64 - z).abs() < 1e-6 {
            let ab = Vec2::new(b[0] as f64 - a[0] as f64, b[1] as f64 - a[1] as f64);
            let ac = Vec2::new(c[0] as f64 - a[0] as f64, c[1] as f64 - a[1] as f64);
            total += (ab.x * ac.y - ab.y * ac.x).abs() * 0.5;
        }
    }
    total
}

/// Two-storey model:
/// - Level 1 (index 0, elevation 0.0): 10×8 exterior, 1 room.
/// - Level 2 (index 1, elevation 2.7): same exterior + interior wall at x = 5, 2 rooms.
///
/// Probes three points:
/// - (2.0, 1.0, 2.55)  → Level-2 floor slab (z ∈ [2.4, 2.7]), hits a level-1 room.
/// - (2.0, 1.0, 1.0)   → empty space between storeys, hits nothing.
/// - (2.0, 1.0, -0.15) → Level-1 floor slab (z ∈ [-0.3, 0.0]), hits the level-0 room.
#[test]
fn two_storey_probes_match_the_brief() {
    // --- Build the model ---
    let mut model = ArchModel::default();

    // Level 1 (index 0) is the default: elevation 0.0, ceiling 2.4, structure 0.3.
    // Add Level 2 at elevation 2.7 (0.0 + 2.4 + 0.3).
    model.levels.push(Level { index: 1, name: "Level 2".into(), elevation: 2.7, ..Default::default() });

    // Exterior rectangle centre lines.
    let exterior = [
        (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
        (Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)),
        (Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)),
        (Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)),
    ];

    // Level 1: exterior rectangle only (4 walls, base_level 0, to_ceiling).
    for (start, end) in exterior.iter() {
        model.push_wall(Wall {
            start: start.clone(),
            end: end.clone(),
            thickness: 0.2,
            height: 2.4,
            base_level: 0,
            to_ceiling: true,
            ..Wall::default()
        });
    }

    // Level 2: same exterior (4 walls) + one interior wall at x = 5 (base_level 1).
    for (start, end) in exterior.iter() {
        model.push_wall(Wall {
            start: start.clone(),
            end: end.clone(),
            thickness: 0.2,
            height: 2.4,
            base_level: 1,
            to_ceiling: true,
            ..Wall::default()
        });
    }
    model.push_wall(Wall {
        start: Vec2::new(5.0, 0.0),
        end: Vec2::new(5.0, 8.0),
        thickness: 0.1,
        height: 2.4,
        base_level: 1,
        to_ceiling: true,
        ..Wall::default()
    });

    // Detect rooms and push them into the model (preserving their `level`).
    let detected = detect_rooms(&model);
    assert_eq!(detected.len(), 3, "expected 3 detected rooms (1 on level 0 + 2 on level 1), got {}", detected.len());
    for room in detected {
        model.push_room(room);
    }

    // Build the 3D scene from the model.
    let scene = build(&model);

    // --- Probe 1: (2.0, 1.0, 2.55) → Level-2 floor slab ---
    // z = 2.55 ∈ [2.4, 2.7] (Level-2 floor slab, elevation 2.7, structure 0.3).
    // x = 2.0 < 5 → inside the left half of the Level-2 interior.
    match scene.probe(Vec3::new(2.0, 1.0, 2.55)) {
        Some(hit) => {
            assert_eq!(hit.part, "floor", "probe(2.0, 1.0, 2.55).part = `{}` (expected `floor`)", hit.part);
            let level1_ids: Vec<String> = model.rooms.iter().filter(|r| r.level == 1).map(|r| room_label(r.id)).collect();
            assert!(level1_ids.contains(&hit.id), "probe(2.0, 1.0, 2.55).id = `{}` not in level-1 room ids: {level1_ids:?}", hit.id);
        }
        None => assert!(false, "probe(2.0, 1.0, 2.55) returned None (expected a Level-2 floor hit)"),
    }

    // --- Probe 2: (2.0, 1.0, 1.0) → empty space between storeys ---
    // z = 1.0 is above the Level-1 slab (top 0.0) and below the Level-2 slab
    // (bottom 2.4), and well clear of every wall at this x/y.
    match scene.probe(Vec3::new(2.0, 1.0, 1.0)) {
        Some(hit) => assert!(false, "probe(2.0, 1.0, 1.0) returned {hit:?} (expected None)"),
        None => {}
    }

    // --- Probe 3: (2.0, 1.0, -0.15) → Level-1 floor slab ---
    // z = -0.15 ∈ [-0.3, 0.0] (Level-1 floor slab, elevation 0.0, structure 0.3).
    // x = 2.0, y = 1.0 is within the single Level-1 room.
    match scene.probe(Vec3::new(2.0, 1.0, -0.15)) {
        Some(hit) => {
            assert_eq!(hit.part, "floor", "probe(2.0, 1.0, -0.15).part = `{}` (expected `floor`)", hit.part);
            let level0_ids: Vec<String> = model.rooms.iter().filter(|r| r.level == 0).map(|r| room_label(r.id)).collect();
            assert!(level0_ids.contains(&hit.id), "probe(2.0, 1.0, -0.15).id = `{}` not in level-0 room ids: {level0_ids:?}", hit.id);
        }
        None => assert!(false, "probe(2.0, 1.0, -0.15) returned None (expected a Level-1 floor hit)"),
    }
}

/// The south room's floor slab must have the loop hole cut out of it.
#[test]
fn south_room_slab_has_the_loop_hole() {
    let (model, scene) = house_with_loop();

    // Find the south room (contains point (1.0, 1.0)).
    let south = model.rooms.iter().find(|r| point_in_polygon(&r.polygon, Vec2::new(1.0, 1.0))).expect("south room not found");
    assert_eq!(south.holes.len(), 1, "south room should have 1 hole");
    assert!((south.interior_area - 26.72).abs() < 1e-3, "south interior area = {}, expected 26.72", south.interior_area);

    // Find the inner room (the small room inside the loop, area ≈ 0.81).
    let inner = model.rooms.iter().find(|r| (r.interior_area - 0.81).abs() < 1e-3).expect("inner room not found");
    assert!(inner.holes.is_empty(), "inner room should have no holes");

    // Find the floor mesh for each room.
    let south_mesh = scene.meshes.iter().filter(|m| m.object == south.id && m.material == "floor").collect::<Vec<_>>();
    assert_eq!(south_mesh.len(), 1, "south room should have exactly 1 floor mesh");
    let south_cap = cap_area(south_mesh[0], 0.0);
    assert!((south_cap - 31.89).abs() < 1e-2, "south slab top cap area = {}, expected 31.89", south_cap);

    let inner_mesh = scene.meshes.iter().filter(|m| m.object == inner.id && m.material == "floor").collect::<Vec<_>>();
    assert_eq!(inner_mesh.len(), 1, "inner room should have exactly 1 floor mesh");
    let inner_cap = cap_area(inner_mesh[0], 0.0);
    assert!((inner_cap - 0.81).abs() < 1e-3, "inner slab top cap area = {}, expected 0.81", inner_cap);
}

/// The loop hole in the south room's slab must result in two distinct floor slabs
/// with different owners: the inner room and the surrounding south room.
#[test]
fn slab_hole_gives_two_room_slabs() {
    let (_model, scene) = house_with_loop();

    // Probe inside the loop's hole (3.0, 1.5) at floor level.
    // This should hit the inner room's own floor slab.
    let inner = scene.probe(Vec3::new(3.0, 1.5, -0.15));
    let inner = inner.expect("probe(3.0, 1.5, -0.15) returned None (expected inner room floor)");
    assert_eq!(inner.part, "floor", "inner probe part = `{}` (expected `floor`)", inner.part);
    assert!(inner.id.starts_with('r'), "inner probe id `{}` is not a room label", inner.id);

    // Probe in the south room, outside the loop (1.0, 1.0) at floor level.
    // This should hit the south room's floor slab.
    let south = scene.probe(Vec3::new(1.0, 1.0, -0.15));
    let south = south.expect("probe(1.0, 1.0, -0.15) returned None (expected south room floor)");
    assert_eq!(south.part, "floor", "south probe part = `{}` (expected `floor`)", south.part);
    assert!(south.id.starts_with('r'), "south probe id `{}` is not a room label", south.id);

    // The inner room and the south room must be different objects.
    assert_ne!(inner.id, south.id, "the inner and surrounding room slabs must have different owners, both reported {}", inner.id);

    // Probe inside the loop's hole (3.0, 1.5) near the ceiling.
    // The inner room has a ceiling; the south room's ceiling has a hole here.
    let ceil = scene.probe(Vec3::new(3.0, 1.5, 2.39));
    let ceil = ceil.expect("probe(3.0, 1.5, 2.39) returned None (expected inner room ceiling)");
    assert_eq!(ceil.part, "ceiling", "ceiling probe part = `{}` (expected `ceiling`)", ceil.part);
    assert_eq!(ceil.id, inner.id, "ceiling probe id `{}` should match inner room id `{}`", ceil.id, inner.id);
}
