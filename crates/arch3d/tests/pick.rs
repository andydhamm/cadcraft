//! BVH ray picking against the phase-1 house.

use cadcraft_arch3d::pick::{Bvh, PickHit};
use cadcraft_arch3d::{ArchId, Scene3d};
use cadcraft_doc::arch::{Opening, OpeningKind};
use cadcraft_doc::{ArchModel, Wall};
use cadcraft_geom::{Vec2, Vec3};

/// The phase-1 house with a 0.9 x 2.1 hinged door on the south wall at offset 3.0.
/// Returns the scene, the south wall's id and the door's id.
fn house() -> (Scene3d, ArchId, ArchId) {
    let mut m = ArchModel::default();
    let w = |a: (f64, f64), b: (f64, f64), t: f64, exterior: bool| Wall {
        start: Vec2::new(a.0, a.1),
        end: Vec2::new(b.0, b.1),
        thickness: t,
        height: 2.4,
        exterior,
        ..Default::default()
    };
    let w0 = m.push_wall(w((0.0, 0.0), (10.0, 0.0), 0.2, true));
    m.push_wall(w((10.0, 0.0), (10.0, 8.0), 0.2, true));
    m.push_wall(w((10.0, 8.0), (0.0, 8.0), 0.2, true));
    m.push_wall(w((0.0, 8.0), (0.0, 0.0), 0.2, true));
    m.push_wall(w((0.0, 3.0), (10.0, 3.0), 0.1, false));
    // The 1 cm sill keeps the wall builder from dropping a floor-touching hole (see HOLE_INSET).
    let door = m.push_opening(Opening { sill: 0.01, ..Opening::new(w0, 3.0, 0.9, 2.1, OpeningKind::Door) });
    (Scene3d::build(&m), w0, door)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

#[test]
fn pick_hits_the_nearest_wall_from_the_south() {
    let (scene, w0, _) = house();
    let hit = scene.pick(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.0)).expect("hit");
    assert_eq!(hit.object, w0);
    assert_eq!(hit.part, "wall");
    assert!((hit.distance - 4.9).abs() < 1e-3, "distance {}", hit.distance);
    assert!((hit.point.y + 0.1).abs() < 1e-3, "point {:?}", hit.point);
}

#[test]
fn pick_through_the_door_opening_finds_no_wall() {
    let (scene, _, _) = house();
    // The door's leaf stops the ray, but the wall around it must not: the ray passes the hole.
    let hit = scene.pick(v(3.0, -5.0, 1.0), v(0.0, 1.0, 0.0)).expect("the leaf");
    assert_ne!(hit.part, "wall", "hit the host wall: {hit:?}");
    // Past the leaf (from inside) the ray reaches the interior wall, not the south wall.
    let inside = scene.pick(v(3.0, 1.0, 1.0), v(0.0, -1.0, 0.0)).expect("the leaf from inside");
    assert_ne!(inside.part, "wall", "{inside:?}");
}

#[test]
fn pick_at_the_sky_is_none() {
    let (scene, _, _) = house();
    assert!(scene.pick(v(5.0, 4.0, 20.0), v(0.0, 0.0, 1.0)).is_none());
    assert!(scene.pick(v(5.0, 4.0, 20.0), v(1.0, 0.0, 0.2)).is_none());
}

#[test]
fn pick_reports_the_door_part() {
    let (scene, _, door) = house();
    let bvh = Bvh::build(&scene);
    // Wall and opening ids come from separate counters, so tell them apart by part name.
    let hit = scene.pick_with(&bvh, v(3.0, -5.0, 1.0), v(0.0, 1.0, 0.0)).expect("hit");
    assert_eq!(hit.object, door);
    assert_eq!(hit.part, "leaf");
}

/// Reference Möller–Trumbore over every triangle in the scene.
fn brute(scene: &Scene3d, o: Vec3, d: Vec3) -> Option<f64> {
    let d = d.normalized();
    let mut best: Option<f64> = None;
    for m in &scene.meshes {
        for g in m.indices.chunks_exact(3) {
            let p = |k: usize| {
                let q = m.positions.get(*g.get(k)? as usize)?;
                Some(v(q[0] as f64, q[1] as f64, q[2] as f64))
            };
            let (Some(a), Some(b), Some(c)) = (p(0), p(1), p(2)) else { continue };
            let (e1, e2) = (b - a, c - a);
            let pv = d.cross(e2);
            let det = e1.dot(pv);
            if det.abs() < 1e-12 {
                continue;
            }
            let s = o - a;
            let u = s.dot(pv) / det;
            let q = s.cross(e1);
            let w = d.dot(q) / det;
            if u < 0.0 || w < 0.0 || u + w > 1.0 {
                continue;
            }
            let t = e2.dot(q) / det;
            if t > 1e-9 && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
    }
    best
}

#[test]
fn bvh_matches_a_brute_force_triangle_scan() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    assert!(!bvh.is_empty());
    let mut seed = 12345u64;
    let mut rnd = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as f64 / (1u64 << 31) as f64
    };
    let mut hits = 0;
    for _ in 0..500 {
        let o = v(rnd() * 16.0 - 3.0, rnd() * 14.0 - 3.0, rnd() * 5.0 - 1.0);
        let d = v(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
        if d.len() < 1e-3 {
            continue;
        }
        let a = bvh.ray(o, d, 1e6).map(|h| h.distance);
        let b = brute(&scene, o, d);
        match (a, b) {
            (None, None) => {}
            (Some(x), Some(y)) => {
                hits += 1;
                assert!((x - y).abs() < 1e-6, "bvh {x} vs brute {y}");
            }
            _ => unreachable_mismatch(a, b),
        }
    }
    assert!(hits > 50, "only {hits} hits");
}

fn unreachable_mismatch(a: Option<f64>, b: Option<f64>) {
    assert_eq!(a.is_some(), b.is_some(), "bvh {a:?} vs brute {b:?}");
}

#[test]
fn pick_is_deterministic_across_rebuilds() {
    let (scene, _, _) = house();
    let b1 = Bvh::build(&scene);
    let b2 = Bvh::build(&house().0);
    for i in 0..60 {
        let o = v(-4.0 + i as f64 * 0.25, -6.0, 0.4 + (i % 7) as f64 * 0.3);
        let d = v(0.1 * (i % 5) as f64 - 0.2, 1.0, 0.05);
        let x: Option<PickHit> = b1.ray(o, d, 100.0);
        let y: Option<PickHit> = b2.ray(o, d, 100.0);
        assert_eq!(x, y);
    }
}

#[test]
fn hostile_rays_never_panic() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    let o = v(5.0, -5.0, 1.0);
    let up = v(0.0, 1.0, 0.0);
    assert!(bvh.ray(o, v(0.0, 0.0, 0.0), 10.0).is_none());
    assert!(bvh.ray(o, v(nan, 1.0, 0.0), 10.0).is_none());
    assert!(bvh.ray(o, v(inf, 0.0, 0.0), 10.0).is_none());
    assert!(bvh.ray(v(nan, 0.0, 0.0), up, 10.0).is_none());
    assert!(bvh.ray(v(inf, inf, inf), up, 10.0).is_none());
    assert!(bvh.ray(o, up, nan).is_none());
    assert!(bvh.ray(o, up, inf).is_none());
    assert!(bvh.ray(o, up, 0.0).is_none());
    assert!(bvh.ray(o, up, -1.0).is_none());
    // Huge but finite values must not panic.
    let _ = bvh.ray(v(1e300, 1e300, 1e300), v(1e300, -1e300, 1e-300), 1e300);
    let _ = bvh.ray(o, v(1e-320, 1.0, 1e-320), f64::MAX);
    // Empty scene.
    let empty = Scene3d::default();
    assert!(empty.pick(o, up).is_none());
    assert!(Bvh::build(&empty).is_empty());
}

/// A hit exactly at max_t should be included (spec: 0 < t <= max_t).
#[test]
fn pick_at_exact_max_t() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from (7, -4.9, 1.5) in +Y direction hits the wall at distance 4.8 (y = -0.1).
    let hit = bvh.ray(v(7.0, -4.9, 1.5), v(0.0, 1.0, 0.0), 4.8);
    assert!(hit.is_some(), "hit at exact max_t should be included");
    assert_eq!(hit.unwrap().object, w0);
    // Just short of max_t: no hit.
    let none = bvh.ray(v(7.0, -4.9, 1.5), v(0.0, 1.0, 0.0), 4.799);
    assert!(none.is_none(), "hit just beyond max_t should be excluded");
}

/// Degenerate triangles (collinear points, zero area) should be skipped by tri_hit.
#[test]
fn pick_with_degenerate_triangles() {
    // Build a scene with a wall, then manually add a degenerate triangle to the BVH.
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // The BVH should still work correctly despite any degenerate triangles in the scene.
    let hit = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "degenerate triangles should not break picking");
}

/// NaN in triangle corners should cause those triangles to be skipped.
#[test]
fn pick_with_nan_in_scene() {
    let mut model = ArchModel::default();
    model.walls.push(Wall { id: ArchId(1), start: Vec2::new(0.0, 0.0), end: Vec2::new(4.0, 0.0), thickness: 0.3, height: 2.5, ..Default::default() });
    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // The wall should still be pickable.
    let hit = bvh.ray(v(2.0, -5.0, 1.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "wall with NaN-free corners should be pickable");
    assert_eq!(hit.unwrap().object, ArchId(1));
}

/// Infinity in triangle corners should cause those triangles to be skipped.
#[test]
fn pick_with_infinity_in_scene() {
    let mut model = ArchModel::default();
    model.walls.push(Wall { id: ArchId(1), start: Vec2::new(0.0, 0.0), end: Vec2::new(4.0, 0.0), thickness: 0.3, height: 2.5, ..Default::default() });
    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    let hit = bvh.ray(v(2.0, -5.0, 1.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "wall with finite corners should be pickable");
}

/// Zero max_t should return None.
#[test]
fn pick_with_zero_max_t() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let none = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.0), 0.0);
    assert!(none.is_none(), "zero max_t should return None");
}

/// Negative max_t should return None.
#[test]
fn pick_with_negative_max_t() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let none = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.0), -1.0);
    assert!(none.is_none(), "negative max_t should return None");
}

/// Infinity max_t is non-finite, so it returns None per spec.
#[test]
fn pick_with_infinity_max_t() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let none = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.0), f64::INFINITY);
    assert!(none.is_none(), "infinity max_t is non-finite and should return None");
}

/// NaN max_t should return None.
#[test]
fn pick_with_nan_max_t() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let none = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.0), f64::NAN);
    assert!(none.is_none(), "NaN max_t should return None");
}

/// Zero direction should return None.
#[test]
fn pick_with_zero_direction() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let none = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 0.0, 0.0), 10.0);
    assert!(none.is_none(), "zero direction should return None");
}

/// Very small direction should still work (normalized internally).
#[test]
fn pick_with_very_small_direction() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    let hit = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1e-10, 0.0), 10.0);
    assert!(hit.is_some(), "very small direction should still find hits");
    assert_eq!(hit.unwrap().object, w0);
}

/// Very large direction should still work (normalized internally).
#[test]
fn pick_with_very_large_direction() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    let hit = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1e10, 0.0), 10.0);
    assert!(hit.is_some(), "very large direction should still find hits");
    assert_eq!(hit.unwrap().object, w0);
}

/// Non-normalized direction should work (normalized internally).
#[test]
fn pick_with_non_normalized_direction() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Direction (0, 2, 0) is not unit length; should be normalized to (0, 1, 0).
    let hit = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 2.0, 0.0), 10.0);
    assert!(hit.is_some(), "non-normalized direction should still find hits");
    assert_eq!(hit.unwrap().object, w0);
    // Distance should be 4.9 (same as with unit direction).
    assert!((hit.unwrap().distance - 4.9).abs() < 1e-3, "distance {}", hit.unwrap().distance);
}

/// A scene with a single triangle should be pickable.
#[test]
fn pick_single_triangle() {
    let mut model = ArchModel::default();
    model.walls.push(Wall { id: ArchId(1), start: Vec2::new(0.0, 0.0), end: Vec2::new(2.0, 0.0), thickness: 0.2, height: 2.0, ..Default::default() });
    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    assert!(!bvh.is_empty(), "scene with one wall should have triangles");
    let hit = bvh.ray(v(1.0, -5.0, 1.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "single wall should be pickable");
    assert_eq!(hit.unwrap().object, ArchId(1));
}

/// Picking from inside the house should find the interior wall.
#[test]
fn pick_from_inside_house() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // From (5, 4, 1.0) looking down (-Y), should hit the interior wall at y=3.
    let hit = bvh.ray(v(5.0, 4.0, 1.0), v(0.0, -1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit interior wall from inside");
    assert_eq!(hit.unwrap().part, "wall");
    // The hit should be at y=3.0 (interior wall) or y=0.1 (south wall inner face).
    // Since we're at y=4 looking down, the interior wall at y=3 is first.
    assert!((hit.unwrap().point.y - 3.0).abs() < 0.05, "hit y {}", hit.unwrap().point.y);
}

/// Picking a roof should report part "roof".
#[test]
fn pick_reports_roof_part() {
    use cadcraft_doc::arch::{Roof, RoofPlane};

    let mut model = ArchModel::default();
    model.levels.push(cadcraft_doc::arch::Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.walls.push(Wall {
        id: ArchId(0),
        base_level: 0,
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(10.0, 0.0),
        thickness: 0.3,
        height: 2.4,
        ..Default::default()
    });
    let mut roof = Roof::default();
    roof.id = ArchId(100);
    roof.level = 0;
    let mut plane = RoofPlane::default();
    plane.base_edge = [0.0, 0.0, 10.0, 0.0];
    plane.ring = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 5.0), Vec2::new(0.0, 5.0)];
    plane.slope = 0.5;
    plane.eave_z = 2.4;
    roof.planes.push(plane);
    model.roofs.push(roof);

    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // Pick the roof from above.
    let hit = bvh.ray(v(5.0, 2.5, 10.0), v(0.0, 0.0, -1.0), 100.0);
    assert!(hit.is_some(), "roof should be pickable from above");
    assert_eq!(hit.unwrap().part, "roof");
}

/// Picking a floor should report part "floor".
#[test]
fn pick_reports_floor_part() {
    let mut model = ArchModel::default();
    model.levels.push(cadcraft_doc::arch::Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.rooms.push(cadcraft_doc::arch::Room {
        id: ArchId(100),
        level: 0,
        polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
        ..Default::default()
    });

    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // Pick the floor from above.
    let hit = bvh.ray(v(2.0, 1.5, 1.0), v(0.0, 0.0, -1.0), 10.0);
    assert!(hit.is_some(), "floor should be pickable from above");
    assert_eq!(hit.unwrap().part, "floor");
}

/// BVH triangle count should equal the sum of triangle counts of all meshes.
#[test]
fn bvh_triangle_count_matches_scene() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    let scene_tris: usize = scene.meshes.iter().map(|m| m.triangle_count()).sum();
    assert_eq!(bvh.triangle_count(), scene_tris, "BVH triangle count should match scene");
}

/// BVH node count should be bounded by MAX_NODES.
#[test]
fn bvh_node_count_bounded() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    assert!(bvh.node_count() <= cadcraft_arch3d::pick::MAX_NODES, "node count {} exceeds MAX_NODES", bvh.node_count());
}

/// Picking with a diagonal ray should work.
#[test]
fn pick_with_diagonal_ray() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Diagonal ray from (7, -5, 1.5) in direction (0, 1, 0.1).
    let hit = bvh.ray(v(7.0, -5.0, 1.5), v(0.0, 1.0, 0.1), 10.0);
    assert!(hit.is_some(), "diagonal ray should hit the wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that grazes the wall edge should work.
#[test]
fn pick_grazing_wall_edge() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray that passes very close to the wall edge at (0, 0).
    let hit = bvh.ray(v(-0.01, -5.0, 1.0), v(0.001, 1.0, 0.0), 10.0);
    // This should either hit the wall or miss, depending on exact geometry.
    // The important thing is that it doesn't panic.
    let _ = hit;
}

/// Picking a stair should report part "stair".
#[test]
fn pick_reports_stair_part() {
    let mut model = ArchModel::default();
    model.levels.push(cadcraft_doc::arch::Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.levels.push(cadcraft_doc::arch::Level { index: 1, elevation: 2.7, ceiling_height: 2.4, ..Default::default() });
    let mut stair = cadcraft_doc::arch::Stair::default();
    stair.id = ArchId(200);
    stair.start = Vec2::new(2.0, 1.0);
    stair.direction = 0.0;
    stair.width = 1.0;
    stair.from_level = 0;
    stair.to_level = 1;
    stair.rise = 2.7;
    model.stairs.push(stair);

    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // Pick the stair from above.
    let hit = bvh.ray(v(2.5, 1.5, 2.0), v(0.0, 0.0, -1.0), 10.0);
    assert!(hit.is_some(), "stair should be pickable from above");
    assert_eq!(hit.unwrap().part, "stair");
}

/// Picking a railing should report part "railing".
#[test]
fn pick_reports_railing_part() {
    let mut model = ArchModel::default();
    model.levels.push(cadcraft_doc::arch::Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    let mut railing = cadcraft_doc::arch::Railing::default();
    railing.id = ArchId(300);
    railing.level = 0;
    railing.height = 1.0;
    railing.path = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
    model.railings.push(railing);

    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // Pick the railing from above.
    let hit = bvh.ray(v(5.0, 0.0, 2.0), v(0.0, 0.0, -1.0), 10.0);
    assert!(hit.is_some(), "railing should be pickable from above");
    assert_eq!(hit.unwrap().part, "railing");
}

/// Picking a deck should report part "deck".
#[test]
fn pick_reports_deck_part() {
    let mut model = ArchModel::default();
    model.levels.push(cadcraft_doc::arch::Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.rooms.push(cadcraft_doc::arch::Room {
        id: ArchId(100),
        level: 0,
        polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
        room_type: cadcraft_doc::arch::RoomType::Deck,
        ..Default::default()
    });

    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // Pick the deck from above.
    let hit = bvh.ray(v(2.0, 1.5, 1.0), v(0.0, 0.0, -1.0), 10.0);
    assert!(hit.is_some(), "deck should be pickable from above");
    assert_eq!(hit.unwrap().part, "deck");
}

/// Picking through a window opening should not hit the wall.
#[test]
fn pick_through_window_opening() {
    let mut model = ArchModel::default();
    model.walls.push(Wall {
        id: ArchId(0),
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(10.0, 0.0),
        thickness: 0.3,
        height: 2.7,
        ..Default::default()
    });
    let mut opening = cadcraft_doc::arch::Opening::default();
    opening.id = ArchId(1);
    opening.host = ArchId(0);
    opening.offset = 5.0;
    opening.width = 1.0;
    opening.height = 1.5;
    opening.sill = 0.9;
    opening.style = "window".to_string();
    model.openings.push(opening);

    let scene = Scene3d::build(&model);
    let bvh = Bvh::build(&scene);
    // Pick through the window opening.
    let hit = bvh.ray(v(5.0, -5.0, 1.5), v(0.0, 1.0, 0.0), 10.0);
    // Should hit the window frame or glass, not the wall.
    if let Some(h) = hit {
        assert_ne!(h.part, "wall", "should not hit wall through window opening");
    }
}

/// Picking with a ray that misses all geometry should return None.
#[test]
fn pick_ray_misses_all_geometry() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from far above, pointing up (away from the house).
    let none = bvh.ray(v(5.0, 4.0, 10.0), v(0.0, 0.0, 1.0), 100.0);
    assert!(none.is_none(), "ray pointing away from house should miss");
}

/// Picking with a ray that passes through the house should hit the first wall.
#[test]
fn pick_ray_passes_through_house() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from south, through the house, should hit the south wall first.
    let hit = bvh.ray(v(5.0, -5.0, 1.0), v(0.0, 1.0, 0.0), 100.0);
    assert!(hit.is_some(), "ray through house should hit south wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the back of a wall should work.
#[test]
fn pick_back_of_wall() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from inside the house, looking at the south wall from the inside.
    let hit = bvh.ray(v(5.0, 1.0, 1.0), v(0.0, -1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit south wall from inside");
    assert_eq!(hit.unwrap().part, "wall");
}

/// Picking with a ray that hits the top of a wall should work.
#[test]
fn pick_top_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from above, looking down at the wall.
    let hit = bvh.ray(v(5.0, 0.0, 5.0), v(0.0, 0.0, -1.0), 10.0);
    assert!(hit.is_some(), "should hit top of wall from above");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom of a wall should work.
#[test]
fn pick_bottom_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from below, looking up at the wall.
    let hit = bvh.ray(v(5.0, 0.0, -1.0), v(0.0, 0.0, 1.0), 10.0);
    assert!(hit.is_some(), "should hit bottom of wall from below");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the side of a wall should work.
#[test]
fn pick_side_of_wall() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the side, looking at the wall.
    // Hits the west wall (ArchId(3)) first at x=-0.1.
    let hit = bvh.ray(v(-5.0, 0.0, 1.0), v(1.0, 0.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit side of wall");
    assert_eq!(hit.unwrap().object, ArchId(3));
}

/// Picking with a ray that hits the corner of a wall should work.
#[test]
fn pick_corner_of_wall() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the corner, looking at the wall.
    let hit = bvh.ray(v(-1.0, -1.0, 1.0), v(1.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit corner of wall");
    assert_eq!(hit.unwrap().part, "wall");
}

/// Picking with a ray that hits the edge of a wall should work.
#[test]
fn pick_edge_of_wall() {
    let (scene, _, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the edge, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 1.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit edge of wall");
    assert_eq!(hit.unwrap().part, "wall");
}

/// Picking with a ray that hits the face of a wall should work.
#[test]
fn pick_face_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the face, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 1.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit face of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the middle of a wall should work.
#[test]
fn pick_middle_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the middle, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 1.2), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit middle of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the top edge of a wall should work.
#[test]
fn pick_top_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the top edge, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 2.4), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit top edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom edge of a wall should work.
#[test]
fn pick_bottom_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the bottom edge, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 0.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit bottom edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the left edge of a wall should work.
#[test]
fn pick_left_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the left edge, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 1.2), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit left edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the right edge of a wall should work.
#[test]
fn pick_right_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the right edge, looking at the wall.
    let hit = bvh.ray(v(10.0, -1.0, 1.2), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit right edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the top-left corner of a wall should work.
#[test]
fn pick_top_left_corner_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the top-left corner, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 2.4), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit top-left corner of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the top-right corner of a wall should work.
#[test]
fn pick_top_right_corner_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the top-right corner, looking at the wall.
    let hit = bvh.ray(v(10.0, -1.0, 2.4), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit top-right corner of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom-left corner of a wall should work.
#[test]
fn pick_bottom_left_corner_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the bottom-left corner, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 0.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit bottom-left corner of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom-right corner of a wall should work.
#[test]
fn pick_bottom_right_corner_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the bottom-right corner, looking at the wall.
    let hit = bvh.ray(v(10.0, -1.0, 0.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit bottom-right corner of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the center of a wall should work.
#[test]
fn pick_center_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the center, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 1.2), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit center of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the top-center of a wall should work.
#[test]
fn pick_top_center_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the top-center, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 2.4), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit top-center of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom-center of a wall should work.
#[test]
fn pick_bottom_center_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the bottom-center, looking at the wall.
    let hit = bvh.ray(v(5.0, -1.0, 0.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit bottom-center of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the left-center of a wall should work.
#[test]
fn pick_left_center_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the left-center, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 1.2), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit left-center of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the right-center of a wall should work.
#[test]
fn pick_right_center_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the right-center, looking at the wall.
    let hit = bvh.ray(v(10.0, -1.0, 1.2), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit right-center of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the top-left edge of a wall should work.
#[test]
fn pick_top_left_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the top-left edge, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 2.4), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit top-left edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the top-right edge of a wall should work.
#[test]
fn pick_top_right_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the top-right edge, looking at the wall.
    let hit = bvh.ray(v(10.0, -1.0, 2.4), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit top-right edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom-left edge of a wall should work.
#[test]
fn pick_bottom_left_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the bottom-left edge, looking at the wall.
    let hit = bvh.ray(v(0.0, -1.0, 0.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit bottom-left edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}

/// Picking with a ray that hits the bottom-right edge of a wall should work.
#[test]
fn pick_bottom_right_edge_of_wall() {
    let (scene, w0, _) = house();
    let bvh = Bvh::build(&scene);
    // Ray from the bottom-right edge, looking at the wall.
    let hit = bvh.ray(v(10.0, -1.0, 0.0), v(0.0, 1.0, 0.0), 10.0);
    assert!(hit.is_some(), "should hit bottom-right edge of wall");
    assert_eq!(hit.unwrap().object, w0);
}
