use cadcraft_arch3d::{Scene3d, build};
use cadcraft_doc::arch::{ArchId, ArchModel, Level, Room, RoomType, Stair, Wall};
use cadcraft_geom::Vec2;

/// Tolerance for floating point comparisons.
const TOL: f64 = 1e-6;

/// Helper to check if a probe hit is expected.
fn assert_probe(scene: &Scene3d, x: f64, y: f64, z: f64, expected_part: Option<&str>) {
    let hit = scene.probe(cadcraft_geom::Vec3::new(x, y, z));
    match expected_part {
        Some(part) => {
            assert!(hit.is_some(), "expected hit at ({x},{y},{z}) with part {part}, got none");
            assert_eq!(hit.unwrap().part, part, "wrong part at ({x},{y},{z})");
        }
        None => {
            assert!(hit.is_none(), "expected no hit at ({x},{y},{z}), got {:?}", hit.map(|h| h.part));
        }
    }
}

#[test]
fn golden_stair_geometry() {
    let mut model = ArchModel::default();

    // Level 0 at 0.0, Level 1 at 2.7
    model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.levels.push(Level { index: 1, elevation: 2.7, ceiling_height: 2.4, ..Default::default() });

    // Stair from Level 0 to 1
    let mut stair = Stair::default();
    stair.id = ArchId(0);
    stair.start = Vec2::new(2.0, 1.0);
    stair.direction = 0.0; // +X
    stair.width = 1.0;
    stair.from_level = 0;
    stair.to_level = 1;
    stair.rise = 2.7;
    stair.shape = cadcraft_doc::arch::StairShape::Straight;
    // Calculator logic: 2.7 / 0.16875 = 16 risers.
    stair.risers = 16;
    stair.riser_height = 0.16875;
    stair.treads = 15;
    stair.tread_depth = 0.254;
    stair.run = 3.81; // 15 * 0.254

    model.push_stair(stair);

    let scene = build(&model);

    // Check geometry via probes
    // Stair starts at x=2.0, y=1.0. Width 1.0 (y from 0.5 to 1.5).
    // Riser 1: z 0 to 0.16875. x 2.0 to 2.254.
    // Probe (2.1, 1.0, 0.1) should hit stair.
    assert_probe(&scene, 2.1, 1.0, 0.1, Some("stair"));

    // Riser 2: z 0.16875 to 0.3375. x 2.254 to 2.508.
    // Probe (2.6, 1.0, 0.1) -> z=0.1 is inside riser 1 (0 to 0.16875) if x is in range?
    // Wait, riser 1 is x 2.0..2.254. x=2.6 is in riser 3 (2.508..2.762).
    // Riser 3 z range: 2 * 0.16875 = 0.3375 to 3 * 0.16875 = 0.50625.
    // So z=0.1 is below riser 3. It should be empty (void under the stair).
    // Let's re-read the prompt: "(2.1,1,0.1) and (2.6,1,0.1) hit "stair"".
    // If (2.6, 1, 0.1) hits stair, then my riser calculation or mesh generation is different, or the prompt implies the *mesh* covers it.
    // The mesh is a "stepped prism".
    // If it's a solid stepped prism, the bottom face follows the steps.
    // At x=2.6, the step is at riser 3 (z ~0.3375). The solid is ABOVE the step.
    // So z=0.1 should be empty.
    // Unless "stair" includes the stringers or something?
    // Or maybe the mesh is just a box? No, "stepped prism".
    // Let's check the prompt again: "(2.1,1,0.1) and (2.6,1,0.1) hit "stair"".
    // Maybe the stair mesh is just a box from start to end?
    // "the flight is one closed stepped prism (profile in flight coordinates, extruded across the width)".
    // If it's a stepped prism, the bottom is stepped.
    // Let's assume the test expects what the prompt says.
    // If (2.6, 1, 0.1) hits, maybe the mesh is simpler or I'm misinterpreting the coordinates.
    // Let's stick to the prompt's assertions.

    assert_probe(&scene, 2.1, 1.0, 0.1, Some("stair"));
    assert_probe(&scene, 2.6, 1.0, 0.1, Some("stair"));

    // (2.1, 1, 1.0) is None.
    // At x=2.1 (riser 1), top is 0.16875. z=1.0 is above. Empty.
    assert_probe(&scene, 2.1, 1.0, 1.0, None);
}

#[test]
fn golden_stair_well_probe() {
    let mut model = ArchModel::default();

    // Level 0 at 0.0, Level 1 at 2.7
    model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.levels.push(Level { index: 1, elevation: 2.7, ceiling_height: 2.4, ..Default::default() });

    // Room on Level 1 covering the stair area
    let room = Room {
        id: ArchId(10),
        level: 1,
        polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)],
        ..Default::default()
    };
    model.push_room(room);

    // Stair with a well
    let mut stair = Stair::default();
    stair.id = ArchId(0);
    stair.start = Vec2::new(2.0, 1.0);
    stair.direction = 0.0;
    stair.width = 1.0;
    stair.from_level = 0;
    stair.to_level = 1;
    stair.rise = 2.7;
    stair.risers = 16;
    stair.riser_height = 0.16875;
    stair.treads = 15;
    stair.tread_depth = 0.254;
    stair.run = 3.81;
    // Well polygon: roughly covering the stair run
    stair.well = vec![Vec2::new(2.0, 0.5), Vec2::new(6.0, 0.5), Vec2::new(6.0, 1.5), Vec2::new(2.0, 1.5)];
    model.push_stair(stair);

    let scene = build(&model);

    // Level 1 floor is at 2.7 (elevation). Slab is usually below that?
    // "top exactly at level elevation". So slab top is 2.7.
    // Probe at 2.538 (inside slab thickness 0.3? 2.7-0.3=2.4).
    // (2.6, 1, 2.538) is inside the well hole -> None.
    assert_probe(&scene, 2.6, 1.0, 2.538, None);

    // (2.1, 1, 2.538) -> x=2.1 is inside well (2.0..6.0). Should be None?
    // Prompt says: "(2.1,1,2.538) and (8,4,2.538) hit "floor"".
    // Wait, if well is 2.0..6.0, 2.1 is inside. Why does it hit floor?
    // Maybe the well polygon in the prompt example is different or my interpretation of "well" is wrong.
    // Or maybe the well only cuts where headroom is low?
    // "well_start = the first riser whose top is more than slab_underside - 2.032 up".
    // Slab underside 2.4. 2.4 - 2.032 = 0.368.
    // Riser 2 top is 0.3375. Riser 3 top is 0.50625.
    // So well starts at riser 3 (x=2.508).
    // So x=2.1 is NOT in the well. It hits the floor.
    // x=2.6 IS in the well.

    assert_probe(&scene, 2.1, 1.0, 2.538, Some("floor"));
    assert_probe(&scene, 8.0, 4.0, 2.538, Some("floor"));
}

#[test]
fn golden_deck_probe() {
    let mut model = ArchModel::default();

    // Level 0
    model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });

    // Deck room
    let deck = Room {
        id: ArchId(20),
        level: 0,
        room_type: RoomType::Deck,
        polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
        deck_height: Some(0.9),
        ..Default::default()
    };
    model.push_room(deck);

    let scene = build(&model);

    // Deck surface at 0.9. Slab thickness? Usually thin.
    // Probe 0.88 (just below surface) -> hits deck.
    assert_probe(&scene, 2.0, 1.5, 0.88, Some("deck"));

    // Probe -0.15 (below) -> None.
    assert_probe(&scene, 2.0, 1.5, -0.15, None);

    // Probe 2.39 (above) -> None.
    assert_probe(&scene, 2.0, 1.5, 2.39, None);
}

#[test]
fn random_stairs_never_panic() {
    // Seeded property test
    let mut seed = 123456789u64;
    let mut next_rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for _ in 0..200 {
        let rise = (next_rand() as f64 / u64::MAX as f64) * 3.85 + 0.15; // [0.15, 4.0]

        let mut model = ArchModel::default();
        model.levels.push(Level { index: 0, elevation: 0.0, ..Default::default() });
        model.levels.push(Level { index: 1, elevation: rise, ..Default::default() });

        let mut stair = Stair::default();
        stair.from_level = 0;
        stair.to_level = 1;
        stair.rise = rise;

        // Calculate risers/treads
        // Max riser 0.196
        let min_risers = (rise / 0.196).ceil() as usize;
        let max_risers = (rise / 0.16875).floor() as usize;

        // Simple heuristic for test: pick a valid riser count
        let risers = if min_risers <= max_risers { min_risers + (next_rand() as usize % (max_risers - min_risers + 1)) } else { min_risers };

        stair.risers = risers;
        stair.riser_height = rise / risers as f64;
        stair.treads = risers.saturating_sub(1);
        stair.tread_depth = 0.254;
        stair.run = stair.treads as f64 * stair.tread_depth;

        model.push_stair(stair);

        // Build should not panic
        let _scene = build(&model);

        // Check invariants
        assert!(stair.riser_height <= 0.196 + TOL, "riser height {} too high", stair.riser_height);
        assert_eq!(stair.treads, stair.risers.saturating_sub(1), "treads mismatch");
    }
}
