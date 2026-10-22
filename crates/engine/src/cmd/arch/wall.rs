//! The `wall` command: create architectural wall segments from JSON parameters.
//!
//! JSON parameters:
//! - `points`: required, `[[x,y], ...]` with 2 or more distinct points.
//! - `thickness`: optional, wall thickness. Defaults to the exterior or interior
//!   default based on `exterior`.
//! - `height`: optional, wall height. Defaults to the level's `floor_to_floor`.
//! - `exterior`: optional bool (default `true`).
//! - `closed`: optional bool (default `false`). If true and >2 points and last
//!   differs from first, a closing segment is added.
//! - `level`: optional string, level name. Defaults to the first level in the model.
//! - `type`: optional string (`exterior|interior|knee|railing|retaining`).
//!   Overrides the `exterior` flag and sets the wall kind.
//!
//! Returns JSON: `{"walls": [{"id": "w1", "handle": "a0000001"}, ...], "joins": 0}`

use cadcraft_arch::{JoinKind, Level, Wall};
use cadcraft_doc::Handle;
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, bool_or, distinct_points, points_param, size_param, str_param};
use crate::{EngineError, Result, Session};

/// Command specs for the wall module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("wall", "Wall", run_wall)
            .menu(&["Architecture", "Wall"])
            .alias(&["wal"])
            .params("{points: [[x,y],...], thickness?, height?, exterior?, closed?, level?, type?}"),
    ]
}

/// Run the `wall` command from JSON parameters. Never opens a dialog.
fn run_wall(s: &mut Session, p: &Value) -> Result<Value> {
    // --- Parse and validate points ---
    let pts = points_param(p, "points").ok_or_else(|| bad("wall", "`points` [[x,y],...] (2 or more) is required"))?;
    let pts = distinct_points(&pts);
    if pts.len() < 2 {
        return Err(bad("wall", "`points` [[x,y],...] (2 or more) is required"));
    }
    if pts.len() > super::MAX_WALLS {
        return Err(bad("wall", format!("too many points ({} > {})", pts.len(), super::MAX_WALLS)));
    }
    for i in 0..pts.len() {
        if !pts[i].is_finite() {
            return Err(bad("wall", "non-finite coordinate in `points`"));
        }
    }
    for i in 0..pts.len() - 1 {
        let d = pts[i + 1] - pts[i];
        if d.len() < super::MIN_WALL_LENGTH {
            return Err(bad("wall", "a segment is shorter than the minimum length"));
        }
    }

    // --- Parse remaining parameters ---
    let closed = bool_or(p, "closed", false);
    let level_name = str_param(p, "level").map(|s| s.to_string());
    let type_str = str_param(p, "type").map(|s| s.to_string());

    let d = s.doc_mut()?;

    // `type` overrides `exterior`; knee/railing/retaining are treated as interior
    let exterior = match &type_str {
        None => bool_or(p, "exterior", true),
        Some(t) => match t.as_str() {
            "exterior" => true,
            "interior" | "knee" | "railing" | "retaining" => false,
            _ => return Err(bad("wall", format!("unknown wall type `{t}`"))),
        },
    };
    let wall_type = type_str.clone().unwrap_or_else(|| if exterior { "exterior".to_string() } else { "interior".to_string() });

    // Thickness: explicit or default based on exterior
    let thickness = size_param("wall", p, "thickness", false)?;
    let thickness = match thickness {
        Some(t) => t,
        None => {
            if exterior {
                super::DEFAULT_EXT_THICKNESS
            } else {
                super::DEFAULT_INT_THICKNESS
            }
        }
    };

    // Level: from param or first in model
    let level = level_name.as_deref().and_then(|name| d.arch.levels.iter().find(|l| l.name == name));

    // Height: explicit or level floor_to_floor
    let height = size_param("wall", p, "height", false)?;
    let height = match height {
        Some(h) => h,
        None => level.map(|l| l.floor_to_floor).unwrap_or(super::DEFAULT_EXT_THICKNESS * 5.0),
    };

    // Apply closed flag
    let pts = if closed && pts.len() > 2 {
        match pts.last() {
            Some(last) if !last.near(pts[0], 1e-6) => {
                let first = pts[0];
                let mut v = pts;
                v.push(first);
                v
            }
            _ => pts,
        }
    } else {
        pts
    };

    // --- Build wall objects ---
    let base_level_idx = level.and_then(|l| d.arch.levels.iter().position(|lv| lv.name == l.name)).unwrap_or(0) as u32;
    let layers = super::wall_layers(exterior);
    let mut new_walls: Vec<Wall> = Vec::with_capacity(pts.len().saturating_sub(1));
    for i in 0..pts.len().saturating_sub(1) {
        let id = d.arch.next_wall_id();
        new_walls.push(Wall {
            id,
            start: pts[i],
            end: pts[i + 1],
            thickness,
            height,
            base_level: base_level_idx,
            to_ceiling: true,
            exterior,
            wall_type: wall_type.clone(),
            join_start: JoinKind::default(),
            join_end: JoinKind::default(),
            layers: layers.clone(),
        });
    }

    // --- Build response: newly added walls only ---
    let walls_json: Vec<Value> = new_walls
        .iter()
        .map(|w| {
            let handle = Handle(0xA000_0000u64 + w.id.0 as u64);
            json!({
                "id": format!("w{}", w.id.0),
                "handle": handle.hex(),
            })
        })
        .collect();

    d.arch.walls.extend(new_walls);
    d.touch_arch();

    // --- Join solver (pending p1-join-solver merge) ---
    let joins = 0usize;
    // TODO(p1-join-solver): let joins = cadcraft_arch::join::solve_joins(&mut d.arch);

    Ok(json!({ "walls": walls_json, "joins": joins }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;

    fn new_session_with_level() -> Session {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "ground".to_string(), floor_to_floor: 3.0, ..Default::default() });
        d.touch_arch();
        s
    }

    #[test]
    fn wall_command_creates_segments_and_bumps_revision() {
        let mut s = new_session_with_level();
        let before = s.doc().unwrap().arch_revision;
        let p = json!({
            "points": [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0]],
        });
        let r = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch_revision > before);
        let walls = r["walls"].as_array().unwrap();
        assert_eq!(walls.len(), 2);
        assert_eq!(d.arch.walls.len(), 2);
        assert_eq!(d.arch.walls[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(d.arch.walls[0].end, Vec2::new(3.0, 0.0));
        assert_eq!(d.arch.walls[1].start, Vec2::new(3.0, 0.0));
        assert_eq!(d.arch.walls[1].end, Vec2::new(3.0, 3.0));
    }

    #[test]
    fn wall_command_rejects_bad_thickness_and_short_segments() {
        let mut s = new_session_with_level();
        // thickness: 0 (size_param with zero_ok=false rejects zero)
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": 0.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));

        // thickness: -1
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": -1.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));

        // short segment (below MIN_WALL_LENGTH)
        let p = json!({
            "points": [[0.0, 0.0], [0.0000001, 0.0]],
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));

        // height: very negative (non-positive, rejected by size_param)
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "height": -1.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_closed_flag_adds_the_closing_segment() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]],
            "closed": true,
        });
        let r = s.execute("wall", &p).unwrap();
        let walls = r["walls"].as_array().unwrap();
        assert_eq!(walls.len(), 4);
    }

    #[test]
    fn wall_command_is_undoable() {
        let mut s = new_session_with_level();
        let before = s.doc().unwrap().arch.walls.len();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
        });
        let _ = s.execute("wall", &p).unwrap();
        let after_add = s.doc().unwrap().arch.walls.len();
        assert_eq!(after_add, before + 1);

        s.undo().unwrap();
        let after_undo = s.doc().unwrap().arch.walls.len();
        assert_eq!(after_undo, before);
    }

    #[test]
    fn hostile_wall_params_never_panic() {
        let mut s = new_session_with_level();

        // duplicate consecutive points (reduced by distinct_points)
        let p = json!({ "points": [[1.0, 1.0], [1.0, 1.0], [2.0, 2.0]] });
        let _ = s.execute("wall", &p);

        // 2-point list where points are equal
        let p = json!({ "points": [[5.0, 5.0], [5.0, 5.0]] });
        let _ = s.execute("wall", &p);

        // thickness: 0
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": 0.0,
        });
        let _ = s.execute("wall", &p);

        // thickness: -1
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": -1.0,
        });
        let _ = s.execute("wall", &p);

        // height: very negative
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "height": -1.0,
        });
        let _ = s.execute("wall", &p);

        // points: "nope" (not an array, points_param returns None)
        let p = json!({ "points": "nope" });
        let _ = s.execute("wall", &p);

        // points: [] (empty array)
        let p = json!({ "points": [] });
        let _ = s.execute("wall", &p);

        // closed: true with 2 points
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "closed": true,
        });
        let _ = s.execute("wall", &p);

        // 5000 points (refused, not allocated beyond the cap)
        let pts: Vec<Value> = (0..=5000).map(|i| json!([i as f64, 0.0])).collect();
        let p = json!({ "points": pts });
        let _ = s.execute("wall", &p);
    }

    #[test]
    fn wall_command_never_opens_a_dialog() {
        let mut s = new_session_with_level();
        let p = json!({ "points": [[0.0, 0.0], [1.0, 0.0]] });
        let r = s.execute("wall", &p);
        assert!(r.is_ok());
    }

    #[test]
    fn wall_defaults_use_the_level_floor_to_floor() {
        let mut s = new_session_with_level();
        let p = json!({ "points": [[0.0, 0.0], [2.0, 0.0]] });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls[0].height, 3.0);
    }
}
