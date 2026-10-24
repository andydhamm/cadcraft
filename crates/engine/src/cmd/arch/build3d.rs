//! `build3d` — build the 3D scene for the current drawing's arch model.
//!
//! View-only command (`.noundo()`): reads `Drawing::arch`, runs the
//! `arch3d` builder, and reports the result as JSON. It must not mutate the
//! document or bump `arch_revision`.

use std::time::Instant;

use cadcraft_arch3d::build;
use serde_json::{Value, json};

use crate::Session;

/// Run body. Returns `{"meshes": ..., "triangles": ..., "rebuildMs": ...,
/// "bounds": {"min": [x,y,z], "max": [x,y,z]}, "floors": ..., "ceilings": ...,
/// "openings": ..., "revision": ...}`.
fn run_build3d(s: &mut Session, _params: &Value) -> crate::Result<Value> {
    let d = s.doc()?;
    let t = Instant::now();
    let scene = build(&d.arch);
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
        "openings": scene.openings_built,
        "revision": d.arch_revision
    }))
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
    v.push(crate::cmd::CommandSpec::new("build3d", "Build 3D", run_build3d).menu(&["Architecture", "Build 3D"]).noundo().params("{}"));
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
}
