//! Phase 4 acceptance: rough openings cut holes in walls.
//!
//! These tests verify that `build` correctly cuts the wall elevation profile with the rough opening
//! rectangle before extruding, so that probes inside the rough opening find no wall material.

use cadcraft_arch3d::{Scene3d, build};
use cadcraft_doc::arch::{Opening, OpeningKind, wall_label};
use cadcraft_doc::{ArchId, ArchModel, Wall};
use cadcraft_geom::{Vec2, Vec3};

/// The phase-1 house: centre lines (0,0) (10,0) (10,8) (0,8), 0.2 m thick, 2.4 m high.
/// Returns the model and the wall ids south, east, north, west.
fn house() -> (ArchModel, [ArchId; 4]) {
    let mut m = ArchModel::default();
    let w = |a: (f64, f64), b: (f64, f64)| Wall {
        start: Vec2::new(a.0, a.1),
        end: Vec2::new(b.0, b.1),
        thickness: 0.2,
        height: 2.4,
        ..Default::default()
    };
    let s = m.push_wall(w((0.0, 0.0), (10.0, 0.0)));
    let e = m.push_wall(w((10.0, 0.0), (10.0, 8.0)));
    let n = m.push_wall(w((10.0, 8.0), (0.0, 8.0)));
    let wst = m.push_wall(w((0.0, 8.0), (0.0, 0.0)));
    (m, [s, e, n, wst])
}

#[test]
fn rough_opening_is_a_hole_in_the_wall() {
    let (mut m, [_, _, n, _]) = house();
    // North wall runs (10,8) to (0,8). Offset 6.0 from start (10,8) is x=4.
    let mut o = Opening::new(n, 6.0, 0.9, 1.2, OpeningKind::Window);
    o.sill = 0.9;
    m.push_opening(o);
    let scene = build(&m);

    // Rough opening spans x 3.49..4.51 and z 0.9..2.22 (sill 0.9 + height 1.2 + rough allowance).
    // Probe inside the rough opening: should NOT hit wall.
    let p_in = scene.probe(Vec3::new(4.0, 8.0, 1.5));
    assert!(p_in.is_none() || p_in.as_ref().unwrap().part != "wall", "probe inside rough opening hit wall: {:?}", p_in);

    // Probe below sill: should hit wall.
    let p_below = scene.probe(Vec3::new(4.0, 8.0, 0.5));
    assert!(p_below.is_some() && p_below.as_ref().unwrap().part == "wall", "probe below sill did not hit wall: {:?}", p_below);

    // Probe above rough opening: should hit wall.
    let p_above = scene.probe(Vec3::new(4.0, 8.0, 2.3));
    assert!(p_above.is_some() && p_above.as_ref().unwrap().part == "wall", "probe above rough opening did not hit wall: {:?}", p_above);

    // Probe beside rough opening: should hit wall.
    let p_side = scene.probe(Vec3::new(5.0, 8.0, 1.5));
    assert!(p_side.is_some() && p_side.as_ref().unwrap().part == "wall", "probe beside rough opening did not hit wall: {:?}", p_side);
}

#[test]
fn hole_below_the_sill_keeps_the_wall() {
    let (mut m, [_, _, n, _]) = house();
    let mut o = Opening::new(n, 6.0, 0.9, 1.2, OpeningKind::Window);
    o.sill = 0.9;
    m.push_opening(o);
    let scene = build(&m);

    // Probe below sill (z=0.5 < 0.9): should hit wall.
    let p = scene.probe(Vec3::new(4.0, 8.0, 0.5));
    assert!(p.is_some(), "probe below sill returned None");
    let hit = p.unwrap();
    assert_eq!(hit.part, "wall", "probe below sill hit non-wall part: {:?}", hit);
    assert_eq!(hit.id, wall_label(n), "probe below sill hit wrong object: {:?}", hit);
}

#[test]
fn wall_without_openings_is_solid_at_the_same_points() {
    let (m, _) = house();
    let scene = build(&m);

    // All four points from the first test should hit wall when no opening exists.
    let points = [Vec3::new(4.0, 8.0, 1.5), Vec3::new(4.0, 8.0, 0.5), Vec3::new(4.0, 8.0, 2.3), Vec3::new(5.0, 8.0, 1.5)];

    for p in points {
        let hit = scene.probe(p);
        assert!(hit.is_some(), "probe at {:?} returned None", p);
        let h = hit.unwrap();
        assert_eq!(h.part, "wall", "probe at {:?} hit non-wall part: {:?}", p, h);
    }
}
