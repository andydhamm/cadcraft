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
