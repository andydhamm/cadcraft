//! ROOMDETECT: rebuild the model's rooms from the solved wall poché.
use cadcraft_doc::arch::{MAX_ROOMS, RoomType};
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, Session, bad, bool_or};
use crate::{EngineError, Result};

/// Command specs for the `roomdetect` module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("roomdetect", "Room Detect", run_roomdetect)
            .menu(&["Architecture", "Room Detect"])
            .alias(&["rd"])
            .params("{level?: name|index, dryRun?: bool}"),
    ]
}

/// Human-readable name for a [`RoomType`].
fn room_type_name(t: RoomType) -> &'static str {
    match t {
        RoomType::Living => "living",
        RoomType::Bedroom => "bedroom",
        RoomType::Kitchen => "kitchen",
        RoomType::Bath => "bath",
        RoomType::Closet => "closet",
        RoomType::Utility => "utility",
        RoomType::Garage => "garage",
        RoomType::Porch => "porch",
        RoomType::Deck => "deck",
        RoomType::OpenBelow => "openBelow",
        RoomType::Other => "other",
    }
}

/// Resolve the `level` JSON parameter to an optional level index.
///
/// Returns `Ok(None)` when `v` is absent or `Value::Null` (detect all levels),
/// otherwise an index into `arch.levels`.
fn resolve_level(arch: &cadcraft_doc::arch::ArchModel, v: &Value) -> Result<Option<u32>> {
    match v {
        Value::String(s) => {
            let trimmed = s.trim();
            if let Ok(n) = trimmed.parse::<u64>() {
                if n < arch.levels.len() as u64 {
                    return Ok(Some(n as u32));
                }
            }
            for (i, l) in arch.levels.iter().enumerate() {
                if l.name == trimmed {
                    return Ok(Some(i as u32));
                }
            }
            Err(bad("roomdetect", format!("unknown level `{s}`")))
        }
        Value::Number(n) => {
            if let Some(int) = n.as_i64() {
                if int >= 0 && int < arch.levels.len() as i64 {
                    return Ok(Some(int as u32));
                }
            }
            Err(bad("roomdetect", "`level` must be a level name or index"))
        }
        Value::Null => Ok(None),
        _ => Err(bad("roomdetect", "`level` must be a level name or index")),
    }
}

/// Build the JSON object for a single detected room.
fn room_json(level: u32, t: RoomType, interior: f64, standard: f64, polygon: &[cadcraft_geom::Vec2]) -> Value {
    json!({
        "level": level,
        "type": room_type_name(t),
        "interiorArea": interior,
        "standardArea": standard,
        "polygon": polygon
            .iter()
            .map(|p| json!([p.x, p.y]))
            .collect::<Vec<_>>()
    })
}

/// Run the `roomdetect` command: detect rooms from the solved poché and replace
/// the existing rooms for the targeted level(s).
fn run_roomdetect(s: &mut Session, p: &Value) -> Result<Value> {
    // Resolve the level filter before any mutation so a bad level leaves the doc untouched.
    let level_filter = resolve_level(&s.doc()?.arch, &p.get("level").cloned().unwrap_or(Value::Null))?;
    let dry = bool_or(p, "dryRun", false);

    if dry {
        // Dry run: read-only. Must not call `doc_mut()` or `touch_arch()`.
        let d = s.doc()?;
        let all = cadcraft_arch::room::detect_rooms(&d.arch);
        let detected: Vec<cadcraft_arch::Room> = match level_filter {
            Some(l) => all.into_iter().filter(|r| r.level == l).collect(),
            None => all,
        };
        let rooms: Vec<Value> = detected
            .iter()
            .map(|r| {
                room_json(r.level, r.room_type, cadcraft_arch::area::interior_area(r), cadcraft_arch::area::standard_area(&d.arch, r), &r.polygon)
            })
            .collect();
        Ok(json!({"rooms": rooms, "count": detected.len()}))
    } else {
        let d = s.doc_mut()?;
        let all = cadcraft_arch::room::detect_rooms(&d.arch);
        let mut detected: Vec<cadcraft_arch::Room> = match level_filter {
            Some(l) => all.into_iter().filter(|r| r.level == l).collect(),
            None => all,
        };

        // Replace-per-level: remove rooms for the target level, then insert the
        // detected ones. One `touch_arch()` → one undo step.
        let before = d.arch.rooms.len();
        d.arch.rooms.retain(|r| match level_filter {
            Some(l) => r.level != l,
            None => false,
        });
        let replaced = before - d.arch.rooms.len();

        let cap = MAX_ROOMS.saturating_sub(d.arch.rooms.len());
        let truncated = detected.len() > cap;
        detected.truncate(cap);
        let count = detected.len();

        for r in detected {
            d.arch.push_room(r);
        }
        cadcraft_arch::area::refresh_areas(&mut d.arch);
        d.touch_arch();

        let rooms: Vec<Value> = d
            .arch
            .rooms
            .iter()
            .filter(|r| match level_filter {
                Some(l) => r.level == l,
                None => true,
            })
            .map(|r| room_json(r.level, r.room_type, r.interior_area, r.standard_area, &r.polygon))
            .collect();

        Ok(json!({
            "rooms": rooms,
            "count": count,
            "replaced": replaced,
            "truncated": truncated
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;

    /// Building a 10×8 closed exterior wall must produce at least one room.
    fn build_test_wall(s: &mut Session) {
        let wall_params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]],
            "closed": true,
            "thickness": 0.2,
            "exterior": true
        });
        s.execute("wall", &wall_params).expect("wall must succeed");
    }

    /// `roomdetect` must be a single undo step: one revision bump, one labelled
    /// undo that restores both the rooms and the revision.
    #[test]
    fn roomdetect_is_one_undo_step() {
        let mut s = Session::new();
        build_test_wall(&mut s);

        let rev = s.doc().unwrap().arch_revision;
        let rooms_before = s.doc().unwrap().arch.rooms.len();

        let res = s.execute("roomdetect", &json!({}));
        assert!(res.is_ok(), "roomdetect on a valid model must succeed: {:?}", res);

        let rooms_after = s.doc().unwrap().arch.rooms.len();
        assert!(rooms_after > rooms_before, "rooms: {} -> {}", rooms_before, rooms_after);
        assert_eq!(s.doc().unwrap().arch_revision, rev + 1, "exactly one revision bump");

        let label = s.undo().unwrap();
        assert_eq!(label, Some("Room Detect".to_string()), "undo label must match the command name");

        assert_eq!(s.doc().unwrap().arch.rooms.len(), rooms_before, "undo must restore the room count");
        assert_eq!(s.doc().unwrap().arch_revision, rev, "undo must restore the revision");
    }

    /// `roomdetect` must never panic on hostile or malformed parameters, and a
    /// bad `level` must leave the document state untouched.
    #[test]
    fn roomdetect_never_panics_on_garbage() {
        let garbage =
            [json!({}), json!({"level": "nope"}), json!({"level": 99}), json!({"level": []}), json!({"dryRun": "yes"}), json!({"dryRun": null})];
        for g in &garbage {
            let mut s = Session::new();
            let rev_before = s.doc().unwrap().arch_revision;
            let rooms_before = s.doc().unwrap().arch.rooms.len();

            // The execution must complete without panicking.
            let result = s.execute("roomdetect", g);
            let is_err = result.is_err();

            // On an empty model with no `level` key, must be Ok with count == 0.
            if g == &json!({}) {
                assert!(!is_err, "empty model, no level: expected Ok, got {:?}", result);
                let val = result.unwrap();
                assert_eq!(val["count"], 0, "expected count 0 on empty model");
            }
            // A bad `level` must be Err and must not mutate the document.
            else if g == &json!({"level": "nope"}) || g == &json!({"level": 99}) {
                assert!(is_err, "bad level {:?} must be Err", g);
                assert_eq!(s.doc().unwrap().arch.rooms.len(), rooms_before, "rooms must be unchanged after Err");
                assert_eq!(s.doc().unwrap().arch_revision, rev_before, "arch_revision must be unchanged after Err");
            }
        }
    }

    /// `roomdetect` stores non-zero standard areas on the model's rooms.
    #[test]
    fn roomdetect_stores_standard_areas() {
        let mut s = Session::new();
        build_test_wall(&mut s);
        let res = s.execute("roomdetect", &json!({}));
        assert!(res.is_ok(), "roomdetect must succeed");
        let d = s.doc().unwrap();
        assert!(d.arch.rooms.len() > 0, "expected at least one room after detection");
        for r in &d.arch.rooms {
            assert!(r.standard_area > 0.0, "stored standard_area should be > 0, got {}", r.standard_area);
        }
        let val = res.unwrap();
        for room in val["rooms"].as_array().expect("rooms array").iter() {
            let standard = room["standardArea"].as_f64().unwrap();
            assert!(standard > 0.0, "reported standardArea should be > 0, got {}", standard);
        }
    }

    /// A dry-run `roomdetect` reports areas without mutating the model.
    #[test]
    fn roomdetect_dry_run_reports_areas_without_mutating() {
        let mut s = Session::new();
        build_test_wall(&mut s);
        let rooms_before = s.doc().unwrap().arch.rooms.len();
        let rev_before = s.doc().unwrap().arch_revision;
        let res = s.execute("roomdetect", &json!({"dryRun": true}));
        assert!(res.is_ok(), "dry-run roomdetect must succeed");
        assert_eq!(s.doc().unwrap().arch.rooms.len(), rooms_before, "dry run must not add rooms");
        assert_eq!(s.doc().unwrap().arch_revision, rev_before, "dry run must not bump revision");
        let val = res.unwrap();
        for room in val["rooms"].as_array().expect("rooms array").iter() {
            let standard = room["standardArea"].as_f64().unwrap();
            assert!(standard > 0.0, "dry-run standardArea should be > 0, got {}", standard);
        }
    }

    /// Temporary diagnostic: prints rooms + holes + floor-mesh vertex counts.
    #[test]
    fn diag_loop_rooms_and_holes() {
        let mut s = Session::new();
        s.execute(
            "wall",
            &json!({"points": [[0.0,0.0],[10.0,0.0],[10.0,8.0],[0.0,8.0],[0.0,0.0]], "thickness": 0.2, "height": 2.7, "exterior": true}),
        )
        .unwrap();
        s.execute("wall", &json!({"points": [[0.0,3.0],[10.0,3.0]], "thickness": 0.1})).unwrap();
        s.execute("wall", &json!({"points": [[6.0,3.0],[6.0,8.0]], "thickness": 0.1})).unwrap();
        s.execute("wall", &json!({"points": [[2.5,1.0],[3.5,1.0],[3.5,2.0],[2.5,2.0],[2.5,1.0]], "thickness": 0.1})).unwrap();
        s.execute("roomdetect", &json!({})).unwrap();
        let d = s.doc().unwrap();
        eprintln!("rooms after roomdetect: {}", d.arch.rooms.len());
        for r in &d.arch.rooms {
            eprintln!("  room {:?} area={:.3} holes={} poly_verts={}", r.id, r.interior_area, r.holes.len(), r.polygon.len());
            eprintln!("    polygon: {:?}", r.polygon.iter().map(|p| (format!("{:.3}", p.x), format!("{:.3}", p.y))).collect::<Vec<_>>());
            for (i, h) in r.holes.iter().enumerate() {
                eprintln!("    hole[{}]: {:?}", i, h.iter().map(|p| (format!("{:.3}", p.x), format!("{:.3}", p.y))).collect::<Vec<_>>());
            }
        }
        // Build 3D and inspect floor mesh for room 0
        let scene = cadcraft_arch3d::build(&d.arch);
        for mesh in &scene.meshes {
            if mesh.material == "floor" {
                let in_loop = cadcraft_arch3d::probe::probe_mesh(mesh, cadcraft_geom::Vec3::new(3.0, 1.5, -0.15));
                let in_south = cadcraft_arch3d::probe::probe_mesh(mesh, cadcraft_geom::Vec3::new(1.0, 1.0, -0.15));
                eprintln!(
                    "  floor mesh obj={:?} verts={} tris={} probe(3,1.5)={} probe(1,1)={}",
                    mesh.object,
                    mesh.positions.len(),
                    mesh.indices.len() / 3,
                    in_loop,
                    in_south
                );
            }
        }
        // Not an assert — just diagnostics. Always "pass".
    }

    /// Regression (acceptance check 05 phase-3 block b3): a closed interior loop
    /// must yield its own room and its own floor slab, distinct from the surrounding
    /// south room's floor slab.
    ///
    /// Setup: 10×8 exterior, a horizontal split at y=3, a vertical split at x=6
    /// (south room = 0..10 × 0..3), plus a 1×1 closed loop at (2.5..3.5, 1..2).
    /// After `roomdetect` + `build3d`, two probe points — one inside the loop
    /// interior, one in the open south room — must each hit a floor slab but must
    /// hit *different* slabs (different `id` values).
    #[test]
    fn loop_room_slabs_have_distinct_owners() {
        let mut s = Session::new();
        // Phase-1 house: closed 10x8 exterior (0.2) plus two interior walls (0.1).
        s.execute(
            "wall",
            &json!({"points": [[0.0,0.0],[10.0,0.0],[10.0,8.0],[0.0,8.0],[0.0,0.0]], "thickness": 0.2, "height": 2.7, "exterior": true}),
        )
        .unwrap();
        s.execute("wall", &json!({"points": [[0.0,3.0],[10.0,3.0]], "thickness": 0.1})).unwrap();
        s.execute("wall", &json!({"points": [[6.0,3.0],[6.0,8.0]], "thickness": 0.1})).unwrap();
        // A closed 0.1 m loop inside the south room (centre lines 2.5..3.5 x 1.0..2.0).
        s.execute("wall", &json!({"points": [[2.5,1.0],[3.5,1.0],[3.5,2.0],[2.5,2.0],[2.5,1.0]], "thickness": 0.1})).unwrap();
        s.execute("roomdetect", &json!({})).unwrap();
        let r = s.execute("build3d", &json!({"probe": [[3.0,1.5,-0.15],[1.0,1.0,-0.15]]})).unwrap();
        let probe = r["probe"].as_array().expect("probe array");
        assert_eq!(probe.len(), 2);
        let p0 = &probe[0];
        let p1 = &probe[1];
        assert!(p0.is_object() && p0["part"] == "floor", "inner-room probe should hit a floor: {p0:?}");
        assert!(p1.is_object() && p1["part"] == "floor", "south-room probe should hit a floor: {p1:?}");
        assert_ne!(p0["id"], p1["id"], "the loop interior and the south room must own different floor slabs: {probe:?}");
    }

    /// The slab hole is the loop wall's outer footprint (2.45..3.55 x 0.95..2.05).
    /// Probe points just outside the loop wall's outer faces must hit the south
    /// room's floor slab, not the inner room's.
    #[test]
    fn probe_at_hole_boundary_returns_floor() {
        let mut s = Session::new();
        s.execute(
            "wall",
            &json!({"points": [[0.0,0.0],[10.0,0.0],[10.0,8.0],[0.0,8.0],[0.0,0.0]], "thickness": 0.2, "height": 2.7, "exterior": true}),
        )
        .unwrap();
        s.execute("wall", &json!({"points": [[0.0,3.0],[10.0,3.0]], "thickness": 0.1})).unwrap();
        s.execute("wall", &json!({"points": [[6.0,3.0],[6.0,8.0]], "thickness": 0.1})).unwrap();
        s.execute("wall", &json!({"points": [[2.5,1.0],[3.5,1.0],[3.5,2.0],[2.5,2.0],[2.5,1.0]], "thickness": 0.1})).unwrap();
        s.execute("roomdetect", &json!({})).unwrap();

        // Reference: open south room floor slab id.
        let ref_r = s.execute("build3d", &json!({"probe": [[1.0,1.0,-0.15]]})).unwrap();
        let ref_probe = ref_r["probe"].as_array().expect("probe array");
        let south_id = &ref_probe[0]["id"];

        // Points just outside the loop wall's outer faces (0.05 m clearance).
        let r = s.execute("build3d", &json!({"probe": [[2.40,1.5,-0.15],[3.60,1.5,-0.15],[3.0,0.90,-0.15],[3.0,2.10,-0.15]]})).unwrap();
        let probe = r["probe"].as_array().expect("probe array");
        assert_eq!(probe.len(), 4);
        for (i, p) in probe.iter().enumerate() {
            assert!(p.is_object() && p["part"] == "floor", "boundary probe {i} should hit a floor: {p:?}");
            assert_eq!(p["id"], south_id, "boundary probe {i} must hit the south room slab: {p:?} vs {south_id:?}");
        }
    }

    /// A probe inside a wall must return the wall, not the floor slab beneath.
    #[test]
    fn probe_inside_wall_returns_wall_not_floor() {
        let mut s = Session::new();
        s.execute(
            "wall",
            &json!({"points": [[0.0,0.0],[10.0,0.0],[10.0,8.0],[0.0,8.0],[0.0,0.0]], "thickness": 0.2, "height": 2.7, "exterior": true}),
        )
        .unwrap();
        s.execute("roomdetect", &json!({})).unwrap();

        // Mid-height probe inside the left exterior wall (x=0.1, wall spans 0..0.2).
        let r = s.execute("build3d", &json!({"probe": [[0.1,4.0,1.0]]})).unwrap();
        let probe = r["probe"].as_array().expect("probe array");
        assert_eq!(probe.len(), 1);
        let p = &probe[0];
        assert!(p.is_object() && p["part"] == "wall", "probe inside wall should hit a wall: {p:?}");

        // Probe at floor level inside the same wall should still hit the wall.
        let r2 = s.execute("build3d", &json!({"probe": [[0.1,4.0,-0.15]]})).unwrap();
        let probe2 = r2["probe"].as_array().expect("probe array");
        let p2 = &probe2[0];
        assert!(p2.is_object() && p2["part"] == "wall", "floor-level probe inside wall should hit a wall: {p2:?}");
    }

    /// A probe outside the house footprint must return null (no hit).
    #[test]
    fn probe_outside_house_returns_null() {
        let mut s = Session::new();
        s.execute(
            "wall",
            &json!({"points": [[0.0,0.0],[10.0,0.0],[10.0,8.0],[0.0,8.0],[0.0,0.0]], "thickness": 0.2, "height": 2.7, "exterior": true}),
        )
        .unwrap();
        s.execute("roomdetect", &json!({})).unwrap();

        let r = s.execute("build3d", &json!({"probe": [[15.0,4.0,0.0],[-5.0,4.0,0.0],[5.0,12.0,0.0]]})).unwrap();
        let probe = r["probe"].as_array().expect("probe array");
        assert_eq!(probe.len(), 3);
        for (i, p) in probe.iter().enumerate() {
            assert!(p.is_null(), "outside probe {i} should be null: {p:?}");
        }
    }

    /// Two separate loops in the south room must each become their own room with
    /// their own floor slab, all distinct from the surrounding south room slab.
    #[test]
    fn two_loops_produce_three_distinct_floor_slabs() {
        let mut s = Session::new();
        s.execute(
            "wall",
            &json!({"points": [[0.0,0.0],[10.0,0.0],[10.0,8.0],[0.0,8.0],[0.0,0.0]], "thickness": 0.2, "height": 2.7, "exterior": true}),
        )
        .unwrap();
        s.execute("wall", &json!({"points": [[0.0,3.0],[10.0,3.0]], "thickness": 0.1})).unwrap();
        // Two 1x1 loops in the south room.
        s.execute("wall", &json!({"points": [[1.5,0.5],[2.5,0.5],[2.5,1.5],[1.5,1.5],[1.5,0.5]], "thickness": 0.1})).unwrap();
        s.execute("wall", &json!({"points": [[7.5,0.5],[8.5,0.5],[8.5,1.5],[7.5,1.5],[7.5,0.5]], "thickness": 0.1})).unwrap();
        s.execute("roomdetect", &json!({})).unwrap();

        // Probe: inside loop 1, inside loop 2, open south room.
        let r = s.execute("build3d", &json!({"probe": [[2.0,1.0,-0.15],[8.0,1.0,-0.15],[5.0,1.0,-0.15]]})).unwrap();
        let probe = r["probe"].as_array().expect("probe array");
        assert_eq!(probe.len(), 3);
        for p in probe.iter() {
            assert!(p.is_object() && p["part"] == "floor", "probe should hit a floor: {p:?}");
        }
        assert_ne!(probe[0]["id"], probe[1]["id"], "the two loops must own different slabs: {probe:?}");
        assert_ne!(probe[0]["id"], probe[2]["id"], "loop 1 and south room must differ: {probe:?}");
        assert_ne!(probe[1]["id"], probe[2]["id"], "loop 2 and south room must differ: {probe:?}");
    }
}
