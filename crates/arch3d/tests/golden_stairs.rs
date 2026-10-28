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
    // Riser 1: z 0 to 0.16875. x 2.0 to 2.254.
    // Probe (2.1, 1.0, 0.15) should hit stair (inside Riser 1 volume).
    assert_probe(&scene, 2.1, 1.0, 0.15, Some("stair"));

    // Riser 2: z 0.16875 to 0.3375. x 2.254 to 2.508.
    // Riser 3: z 0.3375 to 0.50625. x 2.508 to 2.762.
    // Probe (2.6, 1.0, 0.3) is inside Riser 3 volume.
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
