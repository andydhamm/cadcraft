//! The `arch.wall.stretch` command: stretch one end of a wall and transitively
//! move every neighbour end that shared that point (one undo step).

use cadcraft_arch::join;
use cadcraft_doc::ArchId;
use serde_json::{Value, json};

use crate::cmd::arch::MIN_WALL_LENGTH;
use crate::cmd::{CommandSpec, bad, point_req, str_param};
use crate::{EngineError, Result, Session};

/// Command specs for the arch wall stretch module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("arch.wall.stretch", "Stretch Wall", run_stretch)
            .menu(&["Architecture", "Stretch Wall"])
            .params("{wall: \"w1\", end: \"start\"|\"end\", to: [x,y]}"),
    ]
}

/// Run the `arch.wall.stretch` command from JSON parameters.
pub(crate) fn run_stretch(s: &mut Session, p: &Value) -> Result<Value> {
    // `wall` (or `id`) names the wall to stretch.
    let wall_str = str_param(p, "wall").or_else(|| str_param(p, "id"));
    let wall_str = match wall_str {
        Some(s) => s,
        None => return Err(bad("arch.wall.stretch", "`wall` (e.g. \"w1\") is required")),
    };
    let id: ArchId = match wall_str.parse() {
        Ok(v) => v,
        Err(_) => return Err(bad("arch.wall.stretch", format!("invalid wall id `{wall_str}`"))),
    };

    // `end` selects which end of the wall to move.
    let end = match str_param(p, "end").map(|s| s.to_ascii_lowercase()) {
        Some(e) if e == "start" => join::End::Start,
        Some(e) if e == "end" => join::End::End,
        _ => return Err(bad("arch.wall.stretch", "`end` must be \"start\" or \"end\"")),
    };

    // `to` is the new world position of the moved end.
    let to = point_req("arch.wall.stretch", p, "to")?;

    // Stretch a copy and validate it before touching the drawing: a refused drag then
    // leaves no (empty) undo step behind.
    let mut arch = s.doc()?.arch.clone();
    let w = arch.wall(id).ok_or_else(|| bad("arch.wall.stretch", format!("no wall {id}")))?;

    // Refuse to drag an end onto the wall's other end (would collapse it).
    let other = if end == join::End::Start { w.end } else { w.start };
    if to.near(other, MIN_WALL_LENGTH) {
        return Err(bad("arch.wall.stretch", "the dragged end would collapse the wall"));
    }

    let moved = join::stretch_wall(&mut arch, id, end, to).map_err(|e| EngineError::Other(e.to_string()))?;

    // The ends that follow the drag can collapse a neighbour (drag a corner onto the far
    // end of the next wall): refuse that too, or the neighbour silently drops out of the
    // plan and the 3D model while staying in the drawing.
    if let Some(w) = moved.iter().filter_map(|m| arch.wall(*m)).find(|w| w.start.near(w.end, MIN_WALL_LENGTH)) {
        return Err(bad("arch.wall.stretch", format!("the drag would collapse wall {}", w.id)));
    }

    let d = s.doc_mut()?;
    d.arch = arch;
    d.touch_arch();

    Ok(json!({
        "moved": moved.iter().map(|m| format!("w{}", m.0)).collect::<Vec<String>>(),
        "revision": d.arch_revision,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A session with an L-shape: w0 = (0,0)-(10,0), w1 = (10,0)-(10,10).
    fn make_l() -> Session {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]})).expect("wall command should succeed");
        s
    }

    #[test]
    fn arch_wall_stretch_moves_the_neighbour() {
        let mut s = make_l();
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [12.0, 0.0]}));
        assert!(res.is_ok(), "stretch should succeed: {:?}", res);
        let val = res.unwrap();
        let moved: Vec<String> = val["moved"]
            .as_array()
            .expect("moved should be an array")
            .iter()
            .map(|v| v.as_str().expect("moved entries must be strings"))
            .map(str::to_owned)
            .collect();
        assert!(moved.contains(&"w0".to_string()), "w0 should be in moved: {moved:?}");
        assert!(moved.contains(&"w1".to_string()), "w1 should be in moved: {moved:?}");

        let d = s.doc_mut().expect("doc_mut should succeed");
        let w0 = d.arch.wall(ArchId(0)).expect("w0 should exist");
        let w1 = d.arch.wall(ArchId(1)).expect("w1 should exist");
        assert!((w0.end.x - 12.0).abs() < 1e-6, "w0.end.x = {}", w0.end.x);
        assert!((w1.start.x - 12.0).abs() < 1e-6, "w1.start.x = {}", w1.start.x);
    }

    #[test]
    fn arch_wall_stretch_bad_id_is_an_error() {
        let mut s = make_l();

        // Unknown wall id.
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w99", "end": "end", "to": [1.0, 1.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "unknown id should be BadParams: {:?}", r);

        // Missing `wall` param.
        let r = s.execute("arch.wall.stretch", &json!({"end": "end", "to": [1.0, 1.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "missing wall should be BadParams: {:?}", r);

        // Invalid `end` value.
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "middle", "to": [1.0, 1.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "bad end should be BadParams: {:?}", r);
    }

    #[test]
    fn arch_wall_stretch_refuses_to_collapse_a_neighbour() {
        let mut s = make_l();
        let undo_before = s.state().expect("state").undo.len();
        // Dragging w0's end onto w1's far end would make w1 zero-length.
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [10.0, 10.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "collapsing a neighbour must be refused: {r:?}");
        let d = s.doc().expect("doc");
        let w1 = d.arch.wall(ArchId(1)).expect("w1 should exist");
        assert_eq!(w1.start, cadcraft_geom::Vec2::new(10.0, 0.0), "w1 must be untouched");
        assert_eq!(s.state().expect("state").undo.len(), undo_before, "a refused drag must not leave an undo step");
    }

    #[test]
    fn arch_wall_stretch_rejects_degenerate_drag() {
        let mut s = make_l();
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [0.0, 0.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "degenerate drag should be BadParams: {:?}", r);

        // Model unchanged: w0.end is still (10, 0).
        let d = s.doc_mut().expect("doc_mut should succeed");
        let w0 = d.arch.wall(ArchId(0)).expect("w0 should exist");
        assert!((w0.end.x - 10.0).abs() < 1e-6, "w0.end.x should still be 10: {}", w0.end.x);
    }
}
