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
        let rooms: Vec<Value> = detected.iter().map(|r| room_json(r.level, r.room_type, r.interior_area, r.standard_area, &r.polygon)).collect();
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

        let rooms: Vec<Value> = detected.iter().map(|r| room_json(r.level, r.room_type, r.interior_area, r.standard_area, &r.polygon)).collect();

        for r in detected {
            d.arch.push_room(r);
        }
        d.touch_arch();

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
}
