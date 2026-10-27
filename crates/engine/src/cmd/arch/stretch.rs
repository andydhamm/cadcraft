//! The `arch.wall.stretch` command: stretch one end of a wall and transitively
//! move every neighbour end that shared that point (one undo step).

use cadcraft_arch::join;
use cadcraft_arch::opening::{project_to_wall, validate_placement};
use cadcraft_doc::ArchId;
use cadcraft_geom::Vec2;
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

    // Validate on a clone so a refused drag never changes the document Arc
    // (Session::execute pushes an undo step whenever the Arc changes, even on Err).
    let (arch_after, moved) = {
        let doc = s.doc()?;
        let w = doc.arch.wall(id).ok_or_else(|| bad("arch.wall.stretch", format!("no wall {id}")))?;

        // Refuse to drag an end onto the wall's other end (would collapse it).
        let other = if end == join::End::Start { w.end } else { w.start };
        if to.near(other, MIN_WALL_LENGTH) {
            return Err(bad("arch.wall.stretch", "the dragged end would collapse the wall"));
        }

        let mut arch = doc.arch.clone();

        // Capture old starts and ends (owned values) before the stretch so the
        // openings' old world positions survive the in-place rewrite.
        let old_walls: Vec<(ArchId, Vec2, Vec2)> =
            arch.walls.iter().map(|w| (w.id, w.start, w.end)).map(|(wid, start, end)| (wid, start, end)).collect();

        let moved = join::stretch_wall(&mut arch, id, end, to).map_err(|e| EngineError::Other(e.to_string()))?;

        // Refuse if the drag would collapse any neighbour wall.
        for &mid in &moved {
            if let Some(w) = arch.wall(mid) {
                if w.start.near(w.end, MIN_WALL_LENGTH) {
                    return Err(bad("arch.wall.stretch", format!("the drag would collapse wall w{}", mid.0)));
                }
            }
        }

        // First pass: for every wall whose start point changed (the stretched
        // wall when `start` was moved, plus any neighbour in `moved`), re-project
        // each opening it hosts onto the new wall axis so the opening's world
        // start point stays put. The loop holds an immutable borrow of
        // `arch.openings`, so the new offsets are collected first and applied
        // once that borrow has ended.
        let adjustments: Vec<usize> = {
            let mut reoffsets: Vec<(usize, f64)> = Vec::new();
            for (oi, o) in arch.openings.iter().enumerate() {
                let Some((_, old_start, old_end)) = old_walls.iter().find(|(wid, _, _)| *wid == o.host) else {
                    continue;
                };
                let new_start = match arch.wall(o.host) {
                    Some(w) => w.start,
                    None => *old_start,
                };
                if new_start == *old_start {
                    continue;
                }
                let old_dir = (*old_end - *old_start).normalized();
                let old_world = *old_start + old_dir * o.offset;
                let new_offset = project_to_wall(&arch, o.host, old_world)
                    .ok_or_else(|| bad("arch.wall.stretch", format!("cannot project opening o{} onto wall w{}", o.id.0, o.host.0)))?;
                reoffsets.push((oi, new_offset));
            }
            for (oi, new_offset) in &reoffsets {
                arch.openings[*oi].offset = *new_offset;
            }
            reoffsets.iter().map(|(oi, _)| *oi).collect()
        };

        // Second pass: validate every adjusted opening against the fully
        // adjusted clone (ignoring itself). Any failure refuses the whole
        // stretch, so a partial state is never committed.
        for oi in &adjustments {
            let o = &arch.openings[*oi];
            match validate_placement(&arch, o.host, o.offset, o.width, o.sill, o.height, o.kind, &o.style, Some(o.id)) {
                Ok(_) => (),
                Err(e) => {
                    return Err(bad("arch.wall.stretch", format!("the stretch would push o{} off w{}", o.id.0, o.host.0)));
                }
            }
        }

        (arch, moved)
    };

    // All checks passed — commit to the live document.
    let d = s.doc_mut()?;
    d.arch = arch_after;
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

    /// An L-shape with one extra wall: w0 = (0,0)-(10,0), w1 = (10,0)-(10,10),
    /// w2 = (10,10)-(20,10).
    fn make_l3() -> Session {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]})).expect("wall command should succeed");
        s
    }

    #[test]
    fn arch_wall_stretch_moves_the_neighbour() {
        let mut s = make_l3();
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [12.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);
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
        let mut s = make_l3();

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
        let mut s = make_l3();
        let undo_before = s.state().expect("should have state").undo.len();

        // Drag w0's end to (10,10): w1.start is at (10,0) and also moves to (10,10),
        // collapsing w1 (start == end).
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [10.0, 10.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "collapsing a neighbour should be BadParams: {:?}", r);

        // No undo step should have been pushed for the refused drag.
        assert_eq!(s.state().expect("should have state").undo.len(), undo_before, "failed stretch must not push an undo step");

        // w1.start must be unchanged.
        let d = s.doc_mut().expect("doc_mut should succeed");
        let w1 = d.arch.wall(ArchId(1)).expect("w1 should exist");
        assert!((w1.start.x - 10.0).abs() < 1e-6 && w1.start.y.abs() < 1e-6, "w1.start should still be (10, 0): ({}, {})", w1.start.x, w1.start.y);
    }

    #[test]
    fn arch_wall_stretch_rejects_degenerate_drag() {
        let mut s = make_l3();
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [0.0, 0.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "degenerate drag should be BadParams: {:?}", r);

        // Model unchanged: w0.end is still (10, 0).
        let d = s.doc_mut().expect("doc_mut should succeed");
        let w0 = d.arch.wall(ArchId(0)).expect("w0 should exist");
        assert!((w0.end.x - 10.0).abs() < 1e-6, "w0.end.x should still be 10: {}", w0.end.x);
    }

    /// A session with a single 5000-unit wall and a window on it.
    fn make_long_wall_with_window() -> Session {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [5000.0, 0.0]]})).expect("wall command");
        s.execute("window", &json!({"wall": "w0", "offset": 2000.0, "width": 900.0, "height": 2.0, "sill": 0.6})).expect("window command");
        s
    }

    #[test]
    fn stretching_a_walls_start_keeps_its_openings_in_place() {
        let mut s = make_long_wall_with_window();
        let rev_before = s.doc().expect("doc").arch_revision;

        // Stretch w0's start from (0, 0) to (-1000, 0): window's offset should
        // grow by 1000 while its world start stays at (2000, 0).
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [-1000.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.start.x + 1000.0).abs() < 1e-6, "w0.start.x should be -1000: {}", w0.start.x);
        assert!((w0.end.x - 5000.0).abs() < 1e-6, "w0.end.x should be unchanged: {}", w0.end.x);

        let window = d.arch.opening(ArchId(0)).expect("window");
        assert!((window.offset - 3000.0).abs() < 1e-6, "window offset should be 3000 (2000 + 1000 shift): {}", window.offset);
        assert!((w0.start + Vec2::new(window.offset, 0.0) - Vec2::new(2000.0, 0.0)).len() < 1e-6, "window world start should still be (2000, 0)");
        assert!(d.arch_revision != rev_before, "a successful stretch must bump arch_revision");
    }

    #[test]
    fn a_stretch_that_leaves_an_opening_off_its_wall_is_refused() {
        let mut s = make_long_wall_with_window();
        let rev_before = s.doc().expect("doc").arch_revision;

        // Stretch start to (2500, 0): the window (world 2000) would land at
        // offset -500, before the wall start.
        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [2500.0, 0.0]}));
        let err = r.expect_err("stretch that leaves the window off the wall should fail");
        assert!(err.to_string().contains("o0") && err.to_string().contains("w0"), "error should name the opening and wall, got: {}", err);

        // Drawing untouched: start, offset and revision all unchanged.
        assert_eq!(s.doc().expect("doc").arch_revision, rev_before, "revision must be unchanged");
        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.start.x - 0.0).abs() < 1e-6 && w0.start.y.abs() < 1e-6, "w0.start should still be (0, 0): ({}, {})", w0.start.x, w0.start.y);
        assert!((d.arch.opening(ArchId(0)).expect("window").offset - 2000.0).abs() < 1e-6, "offset should still be 2000");
    }

    #[test]
    fn a_neighbour_whose_start_moves_keeps_its_openings() {
        let mut s = Session::new();
        // w0 = (0,0)-(10,0), w1 = (10,0)-(10,10), w2 = (10,10)-(10,20): w2 is a
        // collinear continuation of w1, so sliding w2's start along the shared
        // axis does not rotate the wall.
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [10.0, 20.0]]})).expect("wall command");
        s.execute("door", &json!({"wall": "w2", "offset": 5.0, "width": 1.0, "height": 2.1})).expect("door on w2");

        // Door's world start is (10, 15). Stretch w1's end (10, 10) along w1 to
        // (10, 12); w2's start follows to (10, 12) and the door must keep its
        // world position.
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w1", "end": "end", "to": [10.0, 12.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let door = d.arch.opening(ArchId(0)).expect("door");
        assert!((d.arch.wall(ArchId(2)).expect("w2").start - Vec2::new(10.0, 12.0)).len() < 1e-6, "w2.start should have moved to (10, 12)");
        let door_world = d.arch.wall(ArchId(2)).expect("w2").start
            + (d.arch.wall(ArchId(2)).expect("w2").end - d.arch.wall(ArchId(2)).expect("w2").start).normalized() * door.offset;
        assert!((door_world - Vec2::new(10.0, 15.0)).len() < 1e-5, "door world start should still be (10, 15): ({}, {})", door_world.x, door_world.y);
    }

    #[test]
    fn stretching_the_end_keeps_offsets() {
        let mut s = make_long_wall_with_window();

        // Stretching the end grip must keep existing offsets (unshifted wall).
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [6000.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.end.x - 6000.0).abs() < 1e-6, "w0.end.x should be 6000: {}", w0.end.x);
        assert!((w0.start.x).abs() < 1e-6, "w0.start must stay at (0, 0)");
        assert!((d.arch.opening(ArchId(0)).expect("window").offset - 2000.0).abs() < 1e-6, "offset should still be 2000");
    }

    #[test]
    fn stretching_start_with_multiple_openings() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [100.0, 0.0]]})).expect("wall command");
        s.execute("window", &json!({"wall": "w0", "offset": 10.0, "width": 2.0, "height": 1.5, "sill": 0.9})).expect("window 1");
        s.execute("door", &json!({"wall": "w0", "offset": 50.0, "width": 1.0, "height": 2.1})).expect("door");
        s.execute("window", &json!({"wall": "w0", "offset": 80.0, "width": 2.0, "height": 1.5, "sill": 0.9})).expect("window 2");

        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [-5.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.start.x + 5.0).abs() < 1e-6, "w0.start.x should be -5: {}", w0.start.x);

        let w1 = d.arch.opening(ArchId(0)).expect("window 1");
        assert!((w1.offset - 15.0).abs() < 1e-6, "window 1 offset should be 15: {}", w1.offset);
        let door = d.arch.opening(ArchId(1)).expect("door");
        assert!((door.offset - 55.0).abs() < 1e-6, "door offset should be 55: {}", door.offset);
        let w2 = d.arch.opening(ArchId(2)).expect("window 2");
        assert!((w2.offset - 85.0).abs() < 1e-6, "window 2 offset should be 85: {}", w2.offset);
    }

    #[test]
    fn stretching_start_with_nan_to_is_an_error() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]})).expect("wall command");

        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [f64::NAN, 0.0]}));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "NaN in to should be BadParams: {:?}", r);
    }

    #[test]
    fn stretching_start_with_huge_coordinates() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [1e9, 0.0]]})).expect("wall command");
        s.execute("window", &json!({"wall": "w0", "offset": 5e8, "width": 900.0, "height": 2.0, "sill": 0.6})).expect("window");

        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [-1e8, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.start.x + 1e8).abs() < 1e-3, "w0.start.x should be -1e8: {}", w0.start.x);

        let window = d.arch.opening(ArchId(0)).expect("window");
        assert!((window.offset - 6e8).abs() < 1e-3, "window offset should be 6e8: {}", window.offset);
    }

    #[test]
    fn stretching_start_of_vertical_wall_keeps_openings() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [0.0, 10.0]]})).expect("wall command");
        s.execute("window", &json!({"wall": "w0", "offset": 3.0, "width": 2.0, "height": 1.5, "sill": 0.9})).expect("window");

        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [0.0, -2.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.start.y + 2.0).abs() < 1e-6, "w0.start.y should be -2: {}", w0.start.y);

        let window = d.arch.opening(ArchId(0)).expect("window");
        assert!((window.offset - 5.0).abs() < 1e-6, "window offset should be 5: {}", window.offset);
    }

    #[test]
    fn stretching_start_past_an_opening_is_refused() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]})).expect("wall command");
        s.execute("window", &json!({"wall": "w0", "offset": 1.0, "width": 1.0, "height": 1.5, "sill": 0.9})).expect("window");

        let r = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [1.5, 0.0]}));
        let err = r.expect_err("stretch that pushes window off the wall should fail");
        assert!(err.to_string().contains("o0") && err.to_string().contains("w0"), "error should name the opening and wall, got: {}", err);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!(w0.start.x.abs() < 1e-6 && w0.start.y.abs() < 1e-6, "w0.start should still be (0, 0)");
        assert!((d.arch.opening(ArchId(0)).expect("window").offset - 1.0).abs() < 1e-6, "offset should still be 1.0");
    }

    #[test]
    fn stretching_start_with_opening_at_start() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]})).expect("wall command");
        // Door near the start
        s.execute("door", &json!({"wall": "w0", "offset": 0.2, "width": 1.0, "height": 2.1})).expect("door at start");

        // Stretch start to (-1, 0): door offset should become 1.2
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "start", "to": [-1.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let door = d.arch.opening(ArchId(0)).expect("door");
        assert!((door.offset - 1.2).abs() < 1e-6, "door offset should be 1.2: {}", door.offset);
    }

    #[test]
    fn stretching_end_with_opening_at_end() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]})).expect("wall command");
        // Door near the end
        s.execute("door", &json!({"wall": "w0", "offset": 8.8, "width": 1.0, "height": 2.1})).expect("door at end");

        // Stretch end to (12, 0): door offset should stay 8.8
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [12.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let door = d.arch.opening(ArchId(0)).expect("door");
        assert!((door.offset - 8.8).abs() < 1e-6, "door offset should be 8.8: {}", door.offset);
    }

    #[test]
    fn stretching_t_junction_stem_with_opening() {
        let mut s = Session::new();
        // w0 = (0,0)-(10,0), w1 = (5,0)-(5,10) (T-junction)
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]})).expect("wall w0");
        s.execute("wall", &json!({"points": [[5.0, 0.0], [5.0, 10.0]]})).expect("wall w1");
        // Door on w1 at offset 5
        s.execute("door", &json!({"wall": "w1", "offset": 5.0, "width": 1.0, "height": 2.1})).expect("door on w1");

        // Stretch w0's end to (12, 0): w1 is not affected (it's not connected to w0's end)
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [12.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let door = d.arch.opening(ArchId(0)).expect("door");
        // w1 is unchanged, so the door offset should be unchanged
        assert!((door.offset - 5.0).abs() < 1e-6, "door offset should be 5.0: {}", door.offset);
    }

    #[test]
    fn arch_wall_stretch_isolated_wall() {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]})).expect("wall command");

        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [15.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        assert!((w0.end.x - 15.0).abs() < 1e-6, "w0.end.x should be 15: {}", w0.end.x);
    }

    #[test]
    fn arch_wall_stretch_collinear_walls() {
        let mut s = Session::new();
        // w0 = (0,0)-(10,0), w1 = (10,0)-(20,0): collinear
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]})).expect("wall command");

        // Stretch w0's end to (12, 0): w1's start should move to (12, 0)
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [12.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        let w1 = d.arch.wall(ArchId(1)).expect("w1");
        assert!((w0.end.x - 12.0).abs() < 1e-6, "w0.end.x should be 12: {}", w0.end.x);
        assert!((w1.start.x - 12.0).abs() < 1e-6, "w1.start.x should be 12: {}", w1.start.x);
    }

    #[test]
    fn arch_wall_stretch_with_door_at_shared_point() {
        let mut s = Session::new();
        // w0 = (0,0)-(10,0), w1 = (10,0)-(10,10)
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]})).expect("wall command");
        // Door near the shared point (10,0) on w1, offset 0.5 (so the rough span fits)
        s.execute("door", &json!({"wall": "w1", "offset": 0.5, "width": 1.0, "height": 2.1})).expect("door on w1");

        // Stretch w0's end to (12, 0): w1's start moves to (12, 0), door should stay at (12, 0.5)
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [12.0, 0.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w1 = d.arch.wall(ArchId(1)).expect("w1");
        let door = d.arch.opening(ArchId(0)).expect("door");
        // Door's old world position (10, 0.5) is projected onto the new wall line (12,0)-(10,10),
        // giving (615/52, 45/52) = (11.8269..., 0.8653...).
        let door_world = w1.start + (w1.end - w1.start).normalized() * door.offset;
        assert!(
            (door_world - Vec2::new(11.826923076923077, 0.8653846153846155)).len() < 1e-6,
            "door world start should be (11.8269..., 0.8653...): ({}, {})",
            door_world.x,
            door_world.y
        );
    }

    #[test]
    fn arch_wall_stretch_with_door_far_from_shared_point() {
        let mut s = Session::new();
        // w0 = (0,0)-(10,0), w1 = (10,0)-(10,10)
        s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]})).expect("wall command");
        // Door at offset 5 on w1 (middle of the wall)
        s.execute("door", &json!({"wall": "w1", "offset": 5.0, "width": 1.0, "height": 2.1})).expect("door on w1");

        // Stretch w0's end to (10, 2): w1's start moves to (10, 2), w1 stays vertical,
        // door's world position should stay at (10, 5) with offset adjusted to 3.
        let res = s.execute("arch.wall.stretch", &json!({"wall": "w0", "end": "end", "to": [10.0, 2.0]}));
        assert!(matches!(res, Ok(_)), "stretch should succeed: {:?}", res);

        let d = s.doc().expect("doc");
        let w1 = d.arch.wall(ArchId(1)).expect("w1");
        let door = d.arch.opening(ArchId(0)).expect("door");
        // Door's world position is preserved at (10, 5); offset adjusted from 5 to 3
        let door_world = w1.start + (w1.end - w1.start).normalized() * door.offset;
        assert!((door_world - Vec2::new(10.0, 5.0)).len() < 1e-6, "door world start should be (10, 5): ({}, {})", door_world.x, door_world.y);
        assert!((door.offset - 3.0).abs() < 1e-6, "door offset should be 3.0: {}", door.offset);
    }
}
