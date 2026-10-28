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

    // Room on Level 0 covering the stair area to ensure it is built
    let room = Room {
        id: ArchId(1),
        level: 0,
        polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)],
        ..Default::default()
    };
    model.push_room(room);

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
    stair.layer = "A-STAIR".to_string();
    // Well polygon: starts at x=2.508 (riser 3 start) because headroom < 2.032 there.
    // Well covers x 2.508 to 5.81 (end of run).
    stair.well = vec![Vec2::new(2.508, 0.5), Vec2::new(5.81, 0.5), Vec2::new(5.81, 1.5), Vec2::new(2.508, 1.5)];

    model.push_stair(stair);

    let scene = build(&model);

    // Check geometry via probes
    // Stair starts at x=2.0, y=1.0. Width 1.0 (y from 0.5 to 1.5).
    // The stair mesh is generated as a solid ramp (profile connects riser tops diagonally).
    // At u=0.1 (x=2.1), the top surface height is approx 0.066m.
    // Probe (2.1, 1.0, 0.05) is inside the ramp volume.
    assert_probe(&scene, 2.1, 1.0, 0.05, Some("stair"));

    // At u=0.6 (x=2.6), the top surface height is approx 0.398m.
    // Probe (2.6, 1.0, 0.3) is inside the ramp volume.
    assert_probe(&scene, 2.6, 1.0, 0.3, Some("stair"));

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
    // Well polygon: starts at x=2.508 (riser 3 start) because headroom < 2.032 there.
    // Well covers x 2.508 to 5.81 (end of run).
    stair.well = vec![Vec2::new(2.508, 0.5), Vec2::new(5.81, 0.5), Vec2::new(5.81, 1.5), Vec2::new(2.508, 1.5)];
    model.push_stair(stair);

    let scene = build(&model);

    // Level 1 floor is at 2.7 (elevation). Slab is usually below that?
    // "top exactly at level elevation". So slab top is 2.7.
    // Probe at 2.538 (inside slab thickness 0.3? 2.7-0.3=2.4).
    // (2.6, 1, 2.538) is inside the well hole -> None.
    assert_probe(&scene, 2.6, 1.0, 2.538, None);

    // (2.1, 1, 2.538) -> x=2.1 is OUTSIDE well (starts 2.508). Hits floor.
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
fn stair_riser_invariants() {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ..Default::default() });
    model.levels.push(Level { index: 1, elevation: 2.7, ..Default::default() });

    let mut stair = Stair::default();
    stair.id = ArchId(0);
    stair.from_level = 0;
    stair.to_level = 1;
    stair.rise = 2.7;
    stair.risers = 16;
    stair.riser_height = 0.16875;
    stair.treads = 15;
    stair.tread_depth = 0.254;
    stair.run = 3.81;
    let riser_height = stair.riser_height;
    let risers = stair.risers;
    let treads = stair.treads;
    let tread_depth = stair.tread_depth;
    let run = stair.run;
    let rise = stair.rise;
    model.push_stair(stair);

    let scene = build(&model);
    // Invariant: riser_height * risers should equal rise (within tolerance)
    let computed_rise = riser_height * risers as f64;
    assert!((computed_rise - rise).abs() < TOL, "riser_height * risers = {} but rise = {}", computed_rise, rise);
    // Invariant: treads == risers - 1 for straight stairs
    assert_eq!(treads, risers - 1, "treads should be risers - 1");
    // Invariant: run == treads * tread_depth
    let computed_run = treads as f64 * tread_depth;
    assert!((computed_run - run).abs() < TOL, "treads * tread_depth = {} but run = {}", computed_run, run);
    // Scene should have built the stair
    assert!(scene.stairs > 0, "scene should have at least one stair");
}

#[test]
fn stair_with_nan_inputs() {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ..Default::default() });
    model.levels.push(Level { index: 1, elevation: 2.7, ..Default::default() });

    let mut stair = Stair::default();
    stair.id = ArchId(0);
    stair.start = Vec2::new(f64::NAN, f64::NAN);
    stair.direction = f64::NAN;
    stair.width = f64::NAN;
    stair.from_level = 0;
    stair.to_level = 1;
    stair.rise = 2.7;
    stair.risers = 16;
    stair.riser_height = 0.16875;
    stair.treads = 15;
    stair.tread_depth = 0.254;
    stair.run = 3.81;
    model.push_stair(stair);

    // Should not panic with NaN inputs
    let scene = build(&model);
    // Scene should still be valid (may have 0 stairs if NaN causes rejection)
    assert!(scene.stairs >= 0);
}

#[test]
fn deck_without_deck_height() {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ..Default::default() });

    // Deck with no deck_height specified (should use level elevation)
    let deck = Room {
        id: ArchId(30),
        level: 0,
        room_type: RoomType::Deck,
        polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
        deck_height: None,
        ..Default::default()
    };
    model.push_room(deck);

    let scene = build(&model);
    // Deck at level elevation (0.0), thin slab below
    // Probe just below surface (z = -0.01) should hit deck
    assert_probe(&scene, 2.0, 1.5, -0.01, Some("deck"));
    // Probe well below should miss
    assert_probe(&scene, 2.0, 1.5, -0.5, None);
    // Probe above should miss
    assert_probe(&scene, 2.0, 1.5, 1.0, None);
}

#[test]
fn multiple_stairs_same_levels() {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ..Default::default() });
    model.levels.push(Level { index: 1, elevation: 2.7, ..Default::default() });

    // Two stairs between the same levels, at different locations
    let mut stair1 = Stair::default();
    stair1.id = ArchId(0);
    stair1.start = Vec2::new(2.0, 1.0);
    stair1.direction = 0.0;
    stair1.width = 1.0;
    stair1.from_level = 0;
    stair1.to_level = 1;
    stair1.rise = 2.7;
    stair1.risers = 16;
    stair1.riser_height = 0.16875;
    stair1.treads = 15;
    stair1.tread_depth = 0.254;
    stair1.run = 3.81;
    model.push_stair(stair1);

    let mut stair2 = Stair::default();
    stair2.id = ArchId(1);
    stair2.start = Vec2::new(6.0, 1.0);
    stair2.direction = 0.0;
    stair2.width = 1.0;
    stair2.from_level = 0;
    stair2.to_level = 1;
    stair2.rise = 2.7;
    stair2.risers = 16;
    stair2.riser_height = 0.16875;
    stair2.treads = 15;
    stair2.tread_depth = 0.254;
    stair2.run = 3.81;
    model.push_stair(stair2);

    let scene = build(&model);
    assert_eq!(scene.stairs, 2, "scene should have two stairs");

    // Probe both stairs
    assert_probe(&scene, 2.1, 1.0, 0.05, Some("stair"));
    assert_probe(&scene, 6.1, 1.0, 0.05, Some("stair"));
    // Probe between them (should be empty at that height)
    assert_probe(&scene, 4.0, 1.0, 0.05, None);
}

#[test]
fn stair_with_large_rise() {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ..Default::default() });
    model.levels.push(Level { index: 1, elevation: 4.0, ..Default::default() });

    // 4.0m rise: 4.0 / 0.16875 = 23.7 -> 24 risers of 0.1667m each
    let mut stair = Stair::default();
    stair.id = ArchId(0);
    stair.start = Vec2::new(2.0, 1.0);
    stair.direction = 0.0;
    stair.width = 1.0;
    stair.from_level = 0;
    stair.to_level = 1;
    stair.rise = 4.0;
    stair.risers = 24;
    stair.riser_height = 4.0 / 24.0;
    stair.treads = 23;
    stair.tread_depth = 0.254;
    stair.run = 23.0 * 0.254;
    let riser_height = stair.riser_height;
    model.push_stair(stair);

    let scene = build(&model);
    assert!(scene.stairs > 0, "scene should have a stair");

    // Probe at the bottom of the stair
    assert_probe(&scene, 2.1, 1.0, 0.05, Some("stair"));
    // Probe at the top of the stair (near 4.0m)
    assert_probe(&scene, 2.0 + 22.0 * 0.254, 1.0, 3.5, Some("stair"));
    // Riser height should be within IRC max (0.196m)
    assert!(riser_height <= 0.196 + TOL, "riser height {} exceeds max", riser_height);
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

        let risers_check = stair.risers;
        let treads_check = stair.treads;
        let riser_height_check = stair.riser_height;

        model.push_stair(stair);

        // Build should not panic
        let _scene = build(&model);

        // Check invariants
        assert!(riser_height_check <= 0.196 + TOL, "riser height {} too high", riser_height_check);
        assert_eq!(treads_check, risers_check.saturating_sub(1), "treads mismatch");
    }
}
