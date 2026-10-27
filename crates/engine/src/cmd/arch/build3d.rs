//! `build3d` — build the 3D scene for the current drawing's arch model.
//!
//! View-only command (`.noundo()`): reads `Drawing::arch`, runs the
//! `arch3d` builder, and reports the result as JSON. It must not mutate the
//! document or bump `arch_revision`.

use std::time::Instant;

use cadcraft_arch3d::build;
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

use crate::Session;

/// Run body. Returns `{"meshes": ..., "triangles": ..., "rebuildMs": ...,
/// "bounds": {"min": [x,y,z], "max": [x,y,z]}, "floors": ..., "ceilings": ..., "roofs": ...,
/// "openings": ..., "revision": ..., "probe": [{id, part, level} | null, ...]}`.
/// The optional `probe` key is a list of at most 1024 `[x, y, z]` points;
/// each element of the returned `probe` array is the hit (or `null`) for the
/// corresponding input point, in the same order.
fn run_build3d(s: &mut Session, params: &Value) -> crate::Result<Value> {
    let d = s.doc()?;
    let t = Instant::now();
    let scene = build(&d.arch);

    let probe_points = parse_probe_points(params)?;
    let probe_results: Vec<Value> = probe_points
        .iter()
        .map(|p| {
            scene
                .probe(*p)
                .map(|h| {
                    json!({
                        "id": h.id,
                        "part": h.part,
                        "level": h.level,
                    })
                })
                .unwrap_or(Value::Null)
        })
        .collect();

    Ok(json!({
        "meshes": scene.meshes.len(),
        "triangles": scene.meshes.iter().map(|m| m.indices.len() / 3).sum::<usize>(),
        "rebuildMs": elapsed_ms(t),
        "bounds": {
            "min": [scene.bounds.min[0], scene.bounds.min[1], scene.bounds.min[2]],
            "max": [scene.bounds.max[0], scene.bounds.max[1], scene.bounds.max[2]]
        },
        "floors": scene.floors,
        "ceilings": scene.ceilings,
        "roofs": scene.roofs,
        "openings": scene.openings_built,
        "revision": d.arch_revision,
        "probe": probe_results,
    }))
}

/// Parse the optional `probe` key — a JSON array of `[x, y, z]` arrays — into a
/// capped, finite list of 3D points.
///
/// Absent or `null` yields an empty list (no probing). A non-array value, an
/// array longer than 1024 entries, an element that is not a 3-number array,
/// or any non-finite coordinate is rejected with `bad`.
fn parse_probe_points(params: &Value) -> crate::Result<Vec<Vec3>> {
    match params.get("probe") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(v) => {
            let arr = v.as_array().ok_or_else(|| crate::cmd::bad("build3d", "`probe` must be an array of [x, y, z] arrays"))?;
            if arr.len() > 1024 {
                return Err(crate::cmd::bad("build3d", "probe: at most 1024 points allowed"));
            }
            let mut pts = Vec::with_capacity(arr.len());
            for (i, item) in arr.iter().enumerate() {
                let sub = item.as_array().ok_or_else(|| crate::cmd::bad("build3d", format!("probe[{}] must be [x, y, z]", i)))?;
                if sub.len() != 3 {
                    return Err(crate::cmd::bad("build3d", format!("probe[{}] must be an array of 3 numbers, got {}", i, sub.len())));
                }
                let x = sub.get(0).and_then(Value::as_f64).ok_or_else(|| crate::cmd::bad("build3d", format!("probe[{}][0] is not a number", i)))?;
                let y = sub.get(1).and_then(Value::as_f64).ok_or_else(|| crate::cmd::bad("build3d", format!("probe[{}][1] is not a number", i)))?;
                let z = sub.get(2).and_then(Value::as_f64).ok_or_else(|| crate::cmd::bad("build3d", format!("probe[{}][2] is not a number", i)))?;
                if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                    return Err(crate::cmd::bad("build3d", format!("probe[{}] has a non-finite coordinate", i)));
                }
                pts.push(Vec3::new(x, y, z));
            }
            Ok(pts)
        }
    }
}

/// Elapsed ms since `t`. On wasm32 this is `0.0` — no `web-time` dependency so
/// the web build keeps compiling.
#[cfg(not(target_arch = "wasm32"))]
fn elapsed_ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
#[cfg(target_arch = "wasm32")]
fn elapsed_ms(_t: Instant) -> f64 {
    0.0
}

/// Command spec for `build3d`.
pub fn specs() -> Vec<crate::cmd::CommandSpec> {
    let mut v = Vec::new();
    v.push(
        crate::cmd::CommandSpec::new("build3d", "Build 3D", run_build3d)
            .menu(&["Architecture", "Build 3D"])
            .noundo()
            .params("{probe?: [[x, y, z], ...]}"),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    use cadcraft_doc::arch::{Level, Opening, OpeningKind, Room, RoomType};
    use cadcraft_doc::{ArchId, Wall};
    use cadcraft_geom::Vec2;
    use serde_json::json;

    /// A session with a fresh empty drawing.
    fn fresh_session() -> Session {
        let mut s = Session::empty();
        s.new_drawing(false);
        s
    }

    /// A session whose arch model holds a single extrudable wall.
    fn wall_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            ..Wall::default()
        });
        d.touch_arch();
        s
    }

    #[test]
    fn build3d_reports_mesh_and_triangle_counts() {
        let mut s = wall_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["meshes"].as_u64(), Some(1), "one wall -> one mesh");
        let tri = r["triangles"].as_u64().expect("missing triangles");
        assert!(tri > 0, "expected triangles > 0, got {tri}");
    }

    #[test]
    fn build3d_on_an_empty_model_is_not_an_error() {
        let mut s = fresh_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["meshes"].as_u64(), Some(0));
        assert_eq!(r["triangles"].as_u64(), Some(0));
        // All zeros: bounds at [0,0,0], never an error.
        for axis in 0..3 {
            assert_eq!(r["bounds"]["min"][axis].as_f64(), Some(0.0));
            assert_eq!(r["bounds"]["max"][axis].as_f64(), Some(0.0));
        }
    }

    #[test]
    fn build3d_does_not_bump_the_revision() {
        let mut s = wall_session();
        let before = s.doc().unwrap().arch_revision;
        let _ = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().arch_revision, before, "build3d must not bump arch_revision");
    }

    /// A session whose arch model holds a level and two `Living` rooms.
    fn rooms_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Living,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Living,
            polygon: vec![Vec2::new(5.0, 0.0), Vec2::new(9.0, 0.0), Vec2::new(9.0, 5.0), Vec2::new(5.0, 5.0)],
            ..Room::default()
        });
        d.touch_arch();
        s
    }

    /// A session whose arch model holds a level and one `OpenBelow` room.
    fn open_below_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::OpenBelow,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.touch_arch();
        s
    }

    /// A session whose arch model holds one 10 m wall with a single hinged door on it.
    fn wall_with_opening_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        let wid = d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.arch.push_opening(Opening::new(wid, 3.0, 0.9, 2.0, OpeningKind::Door));
        d.touch_arch();
        s
    }

    #[test]
    fn build3d_reports_the_openings_it_cut() {
        let mut s = wall_with_opening_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["openings"].as_u64(), Some(1), "expected 1 cut opening");
    }

    #[test]
    fn build3d_reports_floor_and_ceiling_counts() {
        let mut s = rooms_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["floors"].as_u64(), Some(2), "two Living rooms -> 2 floors");
        assert_eq!(r["ceilings"].as_u64(), Some(2), "two Living rooms -> 2 ceilings");
    }

    #[test]
    fn build3d_reports_zero_floors_for_open_below() {
        let mut s = open_below_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["floors"].as_u64(), Some(0), "OpenBelow rooms get no floor slab");
        assert_eq!(r["ceilings"].as_u64(), Some(1), "OpenBelow room still gets a ceiling");
    }

    #[test]
    fn build3d_still_reports_meshes_and_openings() {
        let mut s = wall_with_opening_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert!(r["meshes"].is_number(), "missing `meshes` key");
        assert!(r["triangles"].is_number(), "missing `triangles` key");
        assert!(r["openings"].is_number(), "missing `openings` key");
        assert!(r["bounds"].is_object(), "missing `bounds` key");
        assert!(r["revision"].is_number(), "missing `revision` key");
    }

    /// A session whose arch model holds a level and a `Deck` room.
    fn deck_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Deck,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.touch_arch();
        s
    }

    /// A session whose arch model holds a level and a `Porch` room.
    fn porch_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Porch,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.touch_arch();
        s
    }

    /// A session with mixed room types on one level.
    fn mixed_rooms_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Living,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Bedroom,
            polygon: vec![Vec2::new(5.0, 0.0), Vec2::new(9.0, 0.0), Vec2::new(9.0, 5.0), Vec2::new(5.0, 5.0)],
            ..Room::default()
        });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::OpenBelow,
            polygon: vec![Vec2::new(0.0, 6.0), Vec2::new(4.0, 6.0), Vec2::new(4.0, 11.0), Vec2::new(0.0, 11.0)],
            ..Room::default()
        });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Deck,
            polygon: vec![Vec2::new(5.0, 6.0), Vec2::new(9.0, 6.0), Vec2::new(9.0, 11.0), Vec2::new(5.0, 11.0)],
            ..Room::default()
        });
        d.touch_arch();
        s
    }

    /// A session with rooms on two levels.
    fn two_level_rooms_session() -> Session {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.levels.push(Level { ..Level::default() });
        d.arch.rooms.push(Room {
            level: 0,
            room_type: RoomType::Living,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.arch.rooms.push(Room {
            level: 1,
            room_type: RoomType::Bedroom,
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 5.0), Vec2::new(0.0, 5.0)],
            ..Room::default()
        });
        d.touch_arch();
        s
    }

    #[test]
    fn build3d_deck_room_has_floor_no_ceiling() {
        let mut s = deck_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["floors"].as_u64(), Some(1), "Deck room gets a floor slab");
        assert_eq!(r["ceilings"].as_u64(), Some(0), "Deck room gets no ceiling");
    }

    #[test]
    fn build3d_porch_room_has_floor_no_ceiling() {
        let mut s = porch_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["floors"].as_u64(), Some(1), "Porch room gets a floor slab");
        assert_eq!(r["ceilings"].as_u64(), Some(0), "Porch room gets no ceiling");
    }

    #[test]
    fn build3d_mixed_room_types_counts() {
        let mut s = mixed_rooms_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        // Living (floor+ceiling) + Bedroom (floor+ceiling) + OpenBelow (ceiling only) + Deck (floor only)
        assert_eq!(r["floors"].as_u64(), Some(3), "Living + Bedroom + Deck = 3 floors");
        assert_eq!(r["ceilings"].as_u64(), Some(3), "Living + Bedroom + OpenBelow = 3 ceilings");
    }

    #[test]
    fn build3d_rooms_on_two_levels_both_build() {
        let mut s = two_level_rooms_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["floors"].as_u64(), Some(2), "one room per level -> 2 floors");
        assert_eq!(r["ceilings"].as_u64(), Some(2), "one room per level -> 2 ceilings");
    }

    #[test]
    fn build3d_probe_answers_every_point() {
        let mut s = wall_session();
        let params = json!({
            "probe": [
                [5.0, 0.0, 1.0],
                [5.0, 1.0, 1.0],
                [-1.0, 0.0, 1.0],
            ],
        });
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 3, "expected 3 probe results, got {}", probe.len());

        // [5, 0, 1] is inside the 10 m wall (x 0..10, y -0.1..0.1, z 0..2.7).
        let hit = &probe[0];
        assert!(hit.is_object(), "probe[0] should be a hit object (inside wall)");
        assert!(hit["id"].is_string(), "probe[0].id should be a string");
        assert!(hit["part"].is_string(), "probe[0].part should be a string");
        assert!(hit["level"].is_number(), "probe[0].level should be a number");

        // [5, 1, 1] is well outside in y.
        assert!(probe[1].is_null(), "probe[1] should be null (outside wall)");

        // [-1, 0, 1] is well outside in x.
        assert!(probe[2].is_null(), "probe[2] should be null (outside wall)");
    }

    #[test]
    fn build3d_probe_rejects_non_finite() {
        let mut s = wall_session();
        // JSON has no NaN/Infinity, so a real non-finite `Number` is unrepresentable;
        // the textual form `"NaN"` is the closest input, and the command must reject it
        // (non-number) rather than coerce it.
        let params = json!({"probe": [[5.0, 0.0, "NaN"]]});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject a non-finite probe coordinate");
    }

    #[test]
    fn wall_on_level_two_builds_at_its_elevation() {
        let mut s = fresh_session();
        s.execute("level", &json!({"above": "Level 1"})).unwrap(); // creates Level 2 at elevation 2.7, index 1
        s.execute("wall", &json!({"points": [[0.0,0.0],[10.0,0.0]], "thickness": 0.2, "level": "Level 2"})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        let min_z = r["bounds"]["min"][2].as_f64().expect("min z");
        let max_z = r["bounds"]["max"][2].as_f64().expect("max z");
        assert!((min_z - 2.7).abs() < 1e-3, "Level-2 wall must start at its level elevation z=2.7, got {min_z}");
        assert!((max_z - 5.4).abs() < 1e-3, "Level-2 wall top must be 2.7+2.7=5.4, got {max_z}");
    }

    #[test]
    fn wall_to_ceiling_tops_at_ceiling_height() {
        let mut s = fresh_session();
        s.execute("wall", &json!({"points": [[0.0,0.0],[10.0,0.0]], "thickness": 0.2, "toCeiling": true})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        let max_z = r["bounds"]["max"][2].as_f64().expect("max z");
        assert!((max_z - 2.4).abs() < 1e-3, "to_ceiling wall must top at Level 1 ceiling height 2.4, got {max_z}");
    }

    #[test]
    fn wall_on_level_one_default_height() {
        let mut s = fresh_session();
        s.execute("wall", &json!({"points": [[0.0,0.0],[10.0,0.0]], "thickness": 0.2})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        let min_z = r["bounds"]["min"][2].as_f64().expect("min z");
        let max_z = r["bounds"]["max"][2].as_f64().expect("max z");
        assert!((min_z - 0.0).abs() < 1e-3, "Level-1 wall must start at z=0.0, got {min_z}");
        assert!((max_z - 2.7).abs() < 1e-3, "Level-1 wall default height must be 2.7, got {max_z}");
    }

    #[test]
    fn wall_to_ceiling_on_level_two() {
        let mut s = fresh_session();
        s.execute("level", &json!({"above": "Level 1"})).unwrap();
        s.execute("wall", &json!({"points": [[0.0,0.0],[10.0,0.0]], "thickness": 0.2, "level": "Level 2", "toCeiling": true})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        let min_z = r["bounds"]["min"][2].as_f64().expect("min z");
        let max_z = r["bounds"]["max"][2].as_f64().expect("max z");
        assert!((min_z - 2.7).abs() < 1e-3, "Level-2 to-ceiling wall must start at z=2.7, got {min_z}");
        assert!((max_z - 5.1).abs() < 1e-3, "Level-2 to-ceiling wall must top at 2.7+2.4=5.1, got {max_z}");
    }

    #[test]
    fn wall_custom_height_on_level_two() {
        let mut s = fresh_session();
        s.execute("level", &json!({"above": "Level 1"})).unwrap();
        s.execute("wall", &json!({"points": [[0.0,0.0],[10.0,0.0]], "thickness": 0.2, "level": "Level 2", "height": 3.0})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        let min_z = r["bounds"]["min"][2].as_f64().expect("min z");
        let max_z = r["bounds"]["max"][2].as_f64().expect("max z");
        assert!((min_z - 2.7).abs() < 1e-3, "Level-2 wall must start at z=2.7, got {min_z}");
        assert!((max_z - 5.7).abs() < 1e-3, "Level-2 wall with height 3.0 must top at 2.7+3.0=5.7, got {max_z}");
    }

    #[test]
    fn wall_on_level_three_builds_at_its_elevation() {
        let mut s = fresh_session();
        s.execute("level", &json!({"above": "Level 1"})).unwrap();
        s.execute("level", &json!({"above": "Level 2"})).unwrap();
        s.execute("wall", &json!({"points": [[0.0,0.0],[10.0,0.0]], "thickness": 0.2, "level": "Level 3"})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        let min_z = r["bounds"]["min"][2].as_f64().expect("min z");
        let max_z = r["bounds"]["max"][2].as_f64().expect("max z");
        assert!((min_z - 5.4).abs() < 1e-3, "Level-3 wall must start at z=5.4, got {min_z}");
        assert!((max_z - 8.1).abs() < 1e-3, "Level-3 wall top must be 5.4+2.7=8.1, got {max_z}");
    }

    #[test]
    fn build3d_probe_empty_array() {
        let mut s = wall_session();
        let params = json!({"probe": []});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 0, "expected 0 probe results for empty input");
    }

    #[test]
    fn build3d_probe_cap_exceeded() {
        let mut s = wall_session();
        let mut points = Vec::new();
        for i in 0..1025 {
            points.push(json!([i as f64, 0.0, 1.0]));
        }
        let params = json!({"probe": points});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject more than 1024 probe points");
    }

    #[test]
    fn build3d_probe_cap_exact() {
        let mut s = wall_session();
        let mut points = Vec::new();
        for i in 0..1024 {
            points.push(json!([i as f64, 0.0, 1.0]));
        }
        let params = json!({"probe": points});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 1024, "expected 1024 probe results for 1024 input points");
    }

    #[test]
    fn build3d_probe_wrong_element_count() {
        let mut s = wall_session();
        let params = json!({"probe": [[1.0, 2.0]]});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject a probe point with 2 elements");
    }

    #[test]
    fn build3d_probe_extra_element() {
        let mut s = wall_session();
        let params = json!({"probe": [[1.0, 2.0, 3.0, 4.0]]});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject a probe point with 4 elements");
    }

    #[test]
    fn build3d_probe_non_array_element() {
        let mut s = wall_session();
        let params = json!({"probe": [1]});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject a non-array probe element");
    }

    #[test]
    fn build3d_probe_non_number_element() {
        let mut s = wall_session();
        let params = json!({"probe": [["a", "b", "c"]]});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject a probe point with non-number elements");
    }

    #[test]
    fn build3d_probe_null_element() {
        let mut s = wall_session();
        let params = json!({"probe": [null]});
        let r = s.execute("build3d", &params);
        assert!(r.is_err(), "build3d must reject a null probe element");
    }

    #[test]
    fn build3d_probe_huge_values() {
        let mut s = wall_session();
        let params = json!({"probe": [[1e300, 1e300, 1e300]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 1);
        assert!(probe[0].is_null(), "probe at huge coordinates should miss");
    }

    #[test]
    fn build3d_probe_above_wall() {
        let mut s = wall_session();
        // Wall is at z 0..2.7, so z=3.0 is above
        let params = json!({"probe": [[5.0, 0.0, 3.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_null(), "probe above wall should miss");
    }

    #[test]
    fn build3d_probe_below_wall() {
        let mut s = wall_session();
        // Wall is at z 0..2.7, so z=-0.1 is below
        let params = json!({"probe": [[5.0, 0.0, -0.1]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_null(), "probe below wall should miss");
    }

    #[test]
    fn build3d_probe_through_opening() {
        let mut s = wall_with_opening_session();
        // Door at offset 3.0, width 0.9, so x 3.0..3.9, y 0, z 0..2.0
        // Probe through the door opening at z=1.0
        let params = json!({"probe": [[3.5, 0.0, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_null(), "probe through door opening should miss wall");
    }

    #[test]
    fn build3d_probe_multiple_walls() {
        let mut s = fresh_session();
        let d = s.doc_mut().unwrap();
        d.arch.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            ..Wall::default()
        });
        d.arch.walls.push(Wall {
            id: ArchId(2),
            start: Vec2::new(0.0, 5.0),
            end: Vec2::new(10.0, 5.0),
            thickness: 0.2,
            height: 2.7,
            ..Wall::default()
        });
        d.touch_arch();
        let params = json!({"probe": [[5.0, 0.0, 1.0], [5.0, 5.0, 1.0], [5.0, 2.5, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_object(), "probe at first wall should hit");
        assert!(probe[1].is_object(), "probe at second wall should hit");
        assert!(probe[2].is_null(), "probe between walls should miss");
    }

    #[test]
    fn build3d_probe_order_preserved() {
        let mut s = wall_session();
        let params = json!({"probe": [[5.0, 0.0, 1.0], [5.0, 1.0, 1.0], [5.0, 0.0, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 3);
        assert!(probe[0].is_object(), "first probe should hit");
        assert!(probe[1].is_null(), "second probe should miss");
        assert!(probe[2].is_object(), "third probe should hit");
    }

    #[test]
    fn build3d_probe_mixed_hits_and_misses() {
        let mut s = wall_session();
        let params = json!({"probe": [
            [5.0, 0.0, 1.0],   // hit
            [5.0, 1.0, 1.0],   // miss
            [5.0, 0.0, 3.0],   // miss (above)
            [5.0, 0.0, -0.1],  // miss (below)
            [5.0, 0.0, 0.5],   // hit
        ]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 5);
        assert!(probe[0].is_object(), "probe 0 should hit");
        assert!(probe[1].is_null(), "probe 1 should miss");
        assert!(probe[2].is_null(), "probe 2 should miss (above)");
        assert!(probe[3].is_null(), "probe 3 should miss (below)");
        assert!(probe[4].is_object(), "probe 4 should hit");
    }

    #[test]
    fn build3d_probe_wall_at_origin() {
        let mut s = wall_session();
        // Wall starts at origin, probe just inside the wall from the start should hit
        let params = json!({"probe": [[0.1, 0.0, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_object(), "probe just inside wall start should hit");
    }

    #[test]
    fn build3d_probe_wall_at_end() {
        let mut s = wall_session();
        // Wall ends at x=10, probe at end should hit
        let params = json!({"probe": [[10.0, 0.0, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_object(), "probe at wall end should hit");
    }

    #[test]
    fn build3d_probe_just_outside_wall_end() {
        let mut s = wall_session();
        // Wall ends at x=10, probe just past should miss
        let params = json!({"probe": [[10.5, 0.0, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert!(probe[0].is_null(), "probe just past wall end should miss");
    }

    /// A session holding the phase-1 wall chain: a closed 10 x 8 m perimeter,
    /// one `wall` command per side, on the default level.
    fn wall_chain_session() -> Session {
        let mut s = fresh_session();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[10.0, 0.0], [10.0, 8.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[10.0, 8.0], [0.0, 8.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[0.0, 8.0], [0.0, 0.0]], "thickness": 0.2})).unwrap();
        s
    }

    /// `roofauto` on the phase-1 chain produces one roof, which `build3d` must
    /// report in `roofs`.
    #[test]
    fn build3d_reports_roof_count() {
        let mut s = wall_chain_session();
        s.execute("roofauto", &json!({"pitchRise": 6, "pitchRun": 12})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["roofs"].as_u64(), Some(1), "roofauto'd phase-1 chain must report 1 roof");
    }

    /// Walls without a roof must report zero.
    #[test]
    fn build3d_reports_zero_roofs_without_roofs() {
        let mut s = wall_session();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["roofs"].as_u64(), Some(0), "walls only must report 0 roofs");
    }

    /// Two separate wall chains, each with its own roof, must report two roofs.
    #[test]
    fn build3d_reports_multiple_roofs() {
        let mut s = fresh_session();
        // First building: 10x8 m
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[10.0, 0.0], [10.0, 8.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[10.0, 8.0], [0.0, 8.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[0.0, 8.0], [0.0, 0.0]], "thickness": 0.2})).unwrap();
        // Second building: 6x6 m, offset to not overlap
        s.execute("wall", &json!({"points": [[20.0, 0.0], [26.0, 0.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[26.0, 0.0], [26.0, 6.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[26.0, 6.0], [20.0, 6.0]], "thickness": 0.2})).unwrap();
        s.execute("wall", &json!({"points": [[20.0, 6.0], [20.0, 0.0]], "thickness": 0.2})).unwrap();
        // Roof on both
        s.execute("roofauto", &json!({"pitchRise": 6, "pitchRun": 12})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["roofs"].as_u64(), Some(2), "two roofed buildings must report 2 roofs");
    }

    /// Roof count must be reported correctly when probe is also used.
    #[test]
    fn build3d_roof_count_with_probe() {
        let mut s = wall_chain_session();
        s.execute("roofauto", &json!({"pitchRise": 6, "pitchRun": 12})).unwrap();
        let params = json!({"probe": [[5.0, 4.0, 1.0]]});
        let r = s.execute("build3d", &params).unwrap();
        assert_eq!(r["roofs"].as_u64(), Some(1), "roof count must be correct with probe");
        let probe = r["probe"].as_array().expect("missing `probe` array");
        assert_eq!(probe.len(), 1);
    }

    /// Calling build3d twice must give the same roof count (idempotent).
    #[test]
    fn build3d_roof_count_stable_across_calls() {
        let mut s = wall_chain_session();
        s.execute("roofauto", &json!({"pitchRise": 6, "pitchRun": 12})).unwrap();
        let r1 = s.execute("build3d", &json!({})).unwrap();
        let r2 = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r1["roofs"].as_u64(), r2["roofs"].as_u64(), "roof count must be stable");
        assert_eq!(r1["roofs"].as_u64(), Some(1));
    }

    /// Roof count must be correct in a scene with walls, openings, and rooms.
    #[test]
    fn build3d_roof_count_in_complex_scene() {
        let mut s = wall_chain_session();
        // Add a door
        s.execute("door", &json!({"wall": "wall-w1", "offset": 3.0, "width": 0.9})).unwrap();
        // Add a room
        s.execute(
            "room",
            &json!({
                "points": [[0.5, 0.5], [9.5, 0.5], [9.5, 7.5], [0.5, 7.5]],
                "name": "Living Room",
                "type": "Living"
            }),
        )
        .unwrap();
        // Add a roof
        s.execute("roofauto", &json!({"pitchRise": 6, "pitchRun": 12})).unwrap();
        let r = s.execute("build3d", &json!({})).unwrap();
        assert_eq!(r["roofs"].as_u64(), Some(1), "roof count must be correct in complex scene");
        assert!(r["meshes"].as_u64().unwrap() > 0, "scene must have meshes");
        assert!(r["triangles"].as_u64().unwrap() > 0, "scene must have triangles");
    }
}
