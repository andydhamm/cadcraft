//! Two-storey floor-slab probe golden test.
//!
//! Builds a two-storey `ArchModel` and asserts that the 3D probe
//! correctly identifies the floor slabs of both storeys at the expected
//! points.

use cadcraft_arch::room::detect_rooms;
use cadcraft_arch3d::build;
use cadcraft_doc::{ArchModel, Level, Wall};
use cadcraft_geom::{Vec2, Vec3};

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
            let level1_ids: Vec<String> = model.rooms.iter().filter(|r| r.level == 1).map(|r| format!("{}", r.id)).collect();
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
            let level0_ids: Vec<String> = model.rooms.iter().filter(|r| r.level == 0).map(|r| format!("{}", r.id)).collect();
            assert!(level0_ids.contains(&hit.id), "probe(2.0, 1.0, -0.15).id = `{}` not in level-0 room ids: {level0_ids:?}", hit.id);
        }
        None => assert!(false, "probe(2.0, 1.0, -0.15) returned None (expected a Level-1 floor hit)"),
    }
}
