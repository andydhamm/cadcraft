//! Regression tests from the arch3d review (round 9): arch walls through the generic
//! `grip.*` commands and the `wall` command's validation.

use cadcraft_doc::{ArchId, arch_handle};
use cadcraft_engine::{EngineError, Session};
use serde_json::json;

/// A session with the L-shaped wall w0 = (0,0)-(10,0), w1 = (10,0)-(10,10).
fn l_shape() -> Session {
    let mut s = Session::new();
    s.execute("wall", &json!({"points": [[0, 0], [10, 0], [10, 10]]})).unwrap();
    s
}

#[test]
fn grip_stretch_of_an_arch_wall_is_one_undo_step() {
    let mut s = l_shape();
    let h = arch_handle(ArchId(0)).hex();
    s.execute("grip.stretch", &json!({"handle": h, "index": 1, "to": [12, 0]})).unwrap();
    assert_eq!(s.doc().unwrap().arch.wall(ArchId(1)).unwrap().start.x, 12.0);
    // One undo reverts the drag, the next one the `wall` command: the drag used to record
    // two steps (the nested `arch.wall.stretch` and `grip.stretch` itself).
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().arch.wall(ArchId(0)).unwrap().end.x, 10.0);
    s.undo().unwrap();
    assert!(s.doc().unwrap().arch.walls.is_empty(), "the second undo must remove the walls");
}

#[test]
fn grip_stretch_of_an_arch_wall_rejects_a_third_grip() {
    let mut s = l_shape();
    let h = arch_handle(ArchId(0)).hex();
    let r = s.execute("grip.stretch", &json!({"handle": h, "index": 7, "to": [3, 3]}));
    assert!(matches!(r, Err(EngineError::BadParams { .. })), "{r:?}");
    assert_eq!(s.doc().unwrap().arch.wall(ArchId(0)).unwrap().end.x, 10.0, "grip 7 must not move the end");
}
