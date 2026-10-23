//! JSON-only opening commands (`door`, `window`, `garagedoor`).
//!
//! These commands place an [`Opening`](cadcraft_doc::arch::Opening) on an existing
//! wall and are reachable through `Session::execute`, the MCP `execute` tool and
//! the CLI.
//!
//! The interactive machines (`arch.opening.move`, grip-drag along the wall) land
//! in follow-up segments; nothing interactive is wired up in this file yet.

use serde_json::Value;
use serde_json::json;
use std::str::FromStr;

use crate::EngineError;
use crate::Session;
use crate::cmd::CommandSpec;

use cadcraft_arch::opening::{place_opening, project_to_wall, validate_placement};
use cadcraft_doc::ArchId;
use cadcraft_doc::arch::{Hinge, MAX_OPENINGS, Opening, OpeningKind, Swing, arch_handle};

/// Allowed door styles.
const DOOR_STYLES: [&str; 4] = ["hinged", "sliding", "pocket", "bifold"];

/// Allowed window styles.
const WINDOW_STYLES: [&str; 10] = ["single-hung", "double-hung", "slider", "casement", "picture", "bay", "box", "bow", "awning", "fixed"];

/// Allowed garage-door styles.
const GARAGE_STYLES: [&str; 2] = ["one-piece", "sectional"];

/// Parse a user-supplied id string into an [`ArchId`](cadcraft_doc::ArchId).
///
/// Accepts an optional leading `w`/`W`/`o`/`O` followed by a decimal; `Display`
/// prints `w{n}` but the bare `n` is also accepted.
fn parse_arch_id(s: &str) -> Result<ArchId, String> {
    let s = s.trim();
    let start = match s.as_bytes().first() {
        Some(b) if *b == b'w' || *b == b'W' || *b == b'o' || *b == b'O' => 1,
        _ => 0,
    };
    let digits = &s[start..];
    if digits.is_empty() {
        return Err(format!("empty id `{s}`"));
    }
    let n: ArchId = digits.parse().map_err(|_| format!("invalid id `{s}`"))?;
    Ok(n)
}

/// Place an opening on a wall from JSON parameters and return a summary.
fn place_from_params(cmd: &'static str, kind: OpeningKind, s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let d = s.doc_mut()?;

    // 1. Host wall.
    let wall_s = crate::cmd::str_param(p, "wall").ok_or_else(|| crate::cmd::bad(cmd, "`wall` (string id) is required"))?;
    let host: ArchId = parse_arch_id(wall_s).map_err(|e| crate::cmd::bad(cmd, e))?;
    if d.arch.wall(host).is_none() {
        return Err(crate::cmd::bad(cmd, format!("no wall {host}")));
    }

    // 2. Offset: explicit number or projection of a point.
    let offset: f64 = if let Some(off) = p.get("offset").and_then(Value::as_f64) {
        off
    } else if let Some(pt) = crate::cmd::point_param(p, "point") {
        project_to_wall(&d.arch, host, pt).ok_or_else(|| crate::cmd::bad(cmd, "point does not project onto the wall"))?
    } else {
        return Err(crate::cmd::bad(cmd, "`offset` (number) or `point` ([x,y]) is required"));
    };

    // 3. Sizes with per-kind defaults.
    let (def_width, def_height, def_sill) = match kind {
        OpeningKind::Door => (0.9, 2.1, 0.0),
        OpeningKind::Window | OpeningKind::CasedOpening => (1.2, 1.2, 0.9),
        OpeningKind::GarageDoor => (2.4, 2.1, 0.0),
    };
    let width = crate::cmd::size_param(cmd, p, "width", false)?.unwrap_or(def_width);
    let height = crate::cmd::size_param(cmd, p, "height", false)?.unwrap_or(def_height);
    let sill = crate::cmd::size_param(cmd, p, "sill", true)?.unwrap_or(def_sill);
    let recess = crate::cmd::size_param(cmd, p, "recess", true)?.unwrap_or(0.0);
    let casing = crate::cmd::size_param(cmd, p, "casing", true)?.unwrap_or(0.0);

    // 4. Style (validated against the per-kind whitelist).
    let mut style = String::new();
    if let Some(st) = crate::cmd::str_param(p, "style") {
        let lower = st.to_ascii_lowercase();
        let valid: &[&str] = match kind {
            OpeningKind::Door => DOOR_STYLES.as_slice(),
            OpeningKind::Window | OpeningKind::CasedOpening => WINDOW_STYLES.as_slice(),
            OpeningKind::GarageDoor => GARAGE_STYLES.as_slice(),
        };
        if !valid.contains(&lower.as_str()) {
            return Err(crate::cmd::bad(cmd, format!("unknown style `{lower}` for {kind:?}")));
        }
        style = lower;
    }

    // 5. Hinge and swing (case-insensitive; unknown is a bad-params error).
    let mut hinge = Hinge::Left;
    if let Some(h) = crate::cmd::str_param(p, "hinge") {
        hinge = match h.to_ascii_lowercase().as_str() {
            "left" => Hinge::Left,
            "right" => Hinge::Right,
            _ => return Err(crate::cmd::bad(cmd, format!("unknown hinge `{h}`"))),
        };
    }
    let mut swing = Swing::In;
    if let Some(sv) = crate::cmd::str_param(p, "swing") {
        swing = match sv.to_ascii_lowercase().as_str() {
            "in" => Swing::In,
            "out" => Swing::Out,
            _ => return Err(crate::cmd::bad(cmd, format!("unknown swing `{sv}`"))),
        };
    }

    // 6. Level: accepted and validated against `d.arch.levels`; not yet
    // stored on the opening (follow-up segment).
    if let Some(lv) = crate::cmd::str_param(p, "level") {
        if !d.arch.levels.iter().any(|l| l.name == lv) {
            return Err(crate::cmd::bad(cmd, format!("unknown level `{lv}`")));
        }
    }

    let fit = crate::cmd::bool_or(p, "fit", false);

    // 7. Capacity.
    if d.arch.openings.len() >= MAX_OPENINGS {
        return Err(crate::cmd::bad(cmd, "max number of openings reached"));
    }

    // 8. Construct, place, persist.
    let requested_width = width;
    let mut o = Opening::new(host, offset, width, height, kind);
    o.style = style;
    o.sill_depth = sill;
    o.casing_width = casing;
    o.hinge = hinge;
    o.swing = swing;

    let placed = place_opening(&d.arch, o, fit).map_err(|e| crate::cmd::bad(cmd, e.to_string()))?;

    let placed_width = placed.width;
    let resized = (placed_width - requested_width).abs() > 1e-9;
    let id = d.arch.push_opening(placed);
    d.touch_arch();

    Ok(json!({
        "openings": [
            {
                "id": id.to_string(),
                "handle": arch_handle(id).hex(),
                "host": host.to_string(),
                "offset": offset,
                "width": placed_width,
                "resized": resized,
            }
        ],
        "revision": d.arch_revision,
    }))
}

fn run_door(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    place_from_params("door", OpeningKind::Door, s, p)
}

fn run_window(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    place_from_params("window", OpeningKind::Window, s, p)
}

fn run_garagedoor(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    place_from_params("garagedoor", OpeningKind::GarageDoor, s, p)
}

/// Move an existing opening along its host wall.
///
/// `opening` is the id of the opening to move (accepts `w{n}`, `o{n}` or a bare
/// number); `offset` gives the new distance from the wall start, or `point`
/// ([x,y]) which is projected onto the wall. The opening is re-validated against
/// the host wall (a position that runs past either end, leaving less than the
/// minimum wall segment, is rejected). Undoable by default.
fn run_opening_move(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let cmd = "arch.opening.move";

    // 1. Host opening id.
    let id_s = crate::cmd::str_param(p, "opening").ok_or_else(|| crate::cmd::bad(cmd, "`opening` (string id) is required"))?;
    let id: ArchId = parse_arch_id(id_s).map_err(|e| crate::cmd::bad(cmd, e))?;

    // 2. Copy the opening's data into locals before touching `d.arch` again:
    // `d.arch.opening(id)` is a borrow that would conflict with the later
    // `d.arch.openings[i].offset = ...` mutation, so nothing is kept alive.
    let d = s.doc_mut()?;
    let (host, width, sill, height, kind, style) = match d.arch.opening(id) {
        Some(o) => (o.host, o.width, o.sill, o.height, o.kind, o.style.clone()),
        None => return Err(crate::cmd::bad(cmd, format!("no opening {id}"))),
    };

    // 3. New offset: explicit number, or a point projected onto the host wall.
    let new_offset = if let Some(off) = crate::cmd::f64_req(p, "offset") {
        off
    } else if let Some(pt) = crate::cmd::point_param(p, "point") {
        cadcraft_arch::opening::project_to_wall(&d.arch, host, pt).ok_or_else(|| crate::cmd::bad(cmd, "point does not project onto the wall"))?
    } else {
        return Err(crate::cmd::bad(cmd, "`offset` (number) or `point` ([x,y]) is required"));
    };

    // 4. Re-validate on the host wall. `ignore = Some(id)` so the opening is
    // not compared against itself.
    cadcraft_arch::opening::validate_placement(&d.arch, host, new_offset, width, sill, height, kind, &style, Some(id))
        .map_err(|e| crate::cmd::bad(cmd, e.to_string()))?;

    // 5. Write the new offset back through the index (there is no setter).
    if let Some(i) = d.arch.opening_index(id) {
        d.arch.openings[i].offset = new_offset;
    }
    d.touch_arch();

    // 6. Summary.
    Ok(json!({
        "moved": id.to_string(),
        "offset": new_offset,
        "revision": d.arch_revision,
    }))
}

/// Command specs for the opening module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("door", "Door", run_door)
            .menu(&["Architecture", "Door"])
            .alias(&["dr"])
            .params("{wall:\"w0\", offset?|point?, width?, height?, style?, hinge?, swing?, recess?, casing?, fit?, level?}"),
        CommandSpec::new("window", "Window", run_window)
            .menu(&["Architecture", "Window"])
            .alias(&["wn"])
            .params("{wall:\"w0\", offset?|point?, width?, height?, style?, recess?, casing?, fit?, level?}"),
        CommandSpec::new("garagedoor", "Garage Door", run_garagedoor)
            .menu(&["Architecture", "Garage Door"])
            .alias(&["gd"])
            .params("{wall:\"w0\", offset?|point?, width?, height?, style?, recess?, casing?, fit?, level?}"),
        CommandSpec::new("arch.opening.move", "Move Opening", run_opening_move).params("{opening:\"w0\", offset?|point?}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A session with one 10-unit wall at the origin (host `w0`).
    fn session_with_wall() -> Session {
        let mut s = Session::new();
        let r = s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]}));
        r.expect("create wall");
        s
    }

    #[test]
    fn door_command_places_an_opening_and_bumps_revision() {
        let mut s = session_with_wall();
        let rev_before = s.doc().expect("doc").arch_revision;
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("door execute");
        let rev_after = s.doc().expect("doc").arch_revision;
        assert_ne!(rev_before, rev_after, "arch revision did not change");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert_eq!(one["id"], json!("w1"));
        assert_eq!(one["host"], json!("w0"));
        assert_eq!(one["offset"], json!(3.0));
        assert_eq!(one["width"], json!(0.9));
        assert_eq!(one["resized"], json!(false));
        assert_eq!(r["revision"], json!(rev_after));
    }

    #[test]
    fn window_command_defaults_the_sill() {
        let mut s = session_with_wall();
        let r = s.execute("window", &json!({"wall": "w0", "offset": 2.0})).expect("window execute");
        let _ = r["openings"].as_array().expect("openings");
        let d = s.doc().expect("doc");
        let opening = &d.arch.openings[0];
        assert!((opening.sill_depth - 0.9).abs() < 1e-9, "window sill should default to 0.9, got {}", opening.sill_depth);
    }

    #[test]
    fn garagedoor_command_enforces_the_minimum_width() {
        let mut s = session_with_wall();
        let r = s.execute("garagedoor", &json!({"wall": "w0", "offset": 2.0, "width": 1.0}));
        let err = r.expect_err("garage door with width 1.0 < 1.8 m min should fail");
        assert!(err.to_string().contains("1.8"), "expected min-width error, got: {err}");
    }

    #[test]
    fn door_command_accepts_a_point_instead_of_an_offset() {
        let mut s = session_with_wall();
        // Wall w0 runs (0,0) -> (10,0); (3,0) projects to offset 3.0.
        let r = s.execute("door", &json!({"wall": "w0", "point": [3.0, 0.0], "width": 0.9, "height": 2.1})).expect("door with point execute");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert_eq!(one["offset"], json!(3.0));
    }

    #[test]
    fn unknown_style_is_a_bad_params_error() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 1.0, "width": 0.9, "height": 2.1, "style": "spinning"}));
        let err = r.expect_err("unknown door style should be rejected");
        assert!(err.to_string().contains("unknown style"), "expected unknown-style error, got: {err}");
    }

    #[test]
    fn missing_offset_is_a_bad_params_error() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "width": 0.9, "height": 2.1}));
        let err = r.expect_err("missing offset/point should be rejected");
        assert!(err.to_string().contains("`offset` (number) or `point`"), "expected missing-offset error, got: {err}");
    }

    #[test]
    fn door_command_is_undoable() {
        let mut s = session_with_wall();
        let before = s.doc().expect("doc").arch.openings.len();
        s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("door execute");
        let mid = s.doc().expect("doc").arch.openings.len();
        assert_eq!(mid, before + 1, "opening should be added");
        s.undo().expect("undo after door");
        let after = s.doc().expect("doc").arch.openings.len();
        assert_eq!(after, before, "opening should be removed by undo");
    }

    /// A session with the 10 m wall `w0` and one 0.9 m door `w1` at offset 3.0.
    fn session_with_door() -> Session {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("create door");
        s
    }

    #[test]
    fn opening_move_updates_the_offset() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "w1", "offset": 6.5})).expect("move opening");
        assert_eq!(r["moved"], json!("w1"), "moved should be the opening id");
        assert_eq!(r["offset"], json!(6.5), "moved offset should be echoed");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(1)).map(|o| o.offset), Some(6.5), "stored offset should be 6.5");
    }

    #[test]
    fn opening_move_rejects_a_position_past_the_wall() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "w1", "offset": 9.5}));
        let err = r.expect_err("moving the door past the wall end should be rejected");
        assert!(!err.to_string().is_empty(), "expected a placement error");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(1)).map(|o| o.offset), Some(3.0), "stored offset must be unchanged");
    }

    #[test]
    fn opening_move_rejects_an_unknown_id() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "w99"}));
        let err = r.expect_err("moving a non-existent opening should be rejected");
        assert!(err.to_string().contains("no opening"), "expected 'no opening' error, got: {err}");
    }

    #[test]
    fn opening_move_accepts_a_point_instead_of_an_offset() {
        let mut s = session_with_door();
        // Wall w0 runs (0,0) -> (10,0); (6,0) projects to offset 6.0.
        let r = s.execute("arch.opening.move", &json!({"opening": "w1", "point": [6.0, 0.0]})).expect("move opening via point");
        assert_eq!(r["offset"], json!(6.0), "projected offset should be 6.0");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(1)).map(|o| o.offset), Some(6.0), "stored offset should be 6.0");
    }
}
