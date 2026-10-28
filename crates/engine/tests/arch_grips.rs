//! Integration tests for arch-object grip editing.

use cadcraft_doc::{ArchId, arch_handle, opening_handle};
use cadcraft_engine::{EngineError, Session, grips::GripMode};
use cadcraft_geom::Vec2;
use serde_json::json;

/// L-shape: w0 = (0,0)-(10,0), w1 = (10,0)-(10,10).
fn make_l() -> Session {
    let mut s = Session::new();
    s.execute("wall", &json!({"points": [[0,0],[10,0],[10,10]]})).expect("wall");
    s
}

/// One wall w0 = (0,0)-(10,0) with door o0 at offset 3.
fn make_wall_with_door() -> Session {
    let mut s = Session::new();
    s.execute("wall", &json!({"points": [[0,0],[10,0]]})).expect("wall");
    s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9})).expect("door");
    s
}

/// `grip.stretch` on an arch wall must be exactly one undo step: one undo
/// reverts the stretch, a second undo removes the wall command.
#[test]
fn grip_stretch_of_an_arch_wall_is_one_undo_step() {
    let mut s = make_l();
    let steps_after_wall = s.state().expect("state").undo.len();

    // Drag w0's end grip (index 1) from (10,0) to (12,0).
    let h0 = arch_handle(ArchId(0)).hex();
    s.execute("grip.stretch", &json!({"handle": h0, "index": 1, "to": [12.0, 0.0]})).expect("grip.stretch");

    // Must have added exactly one undo step.
    assert_eq!(s.state().expect("state").undo.len(), steps_after_wall + 1, "grip.stretch must be one undo step");

    // w1.start must have moved with w0.end.
    {
        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0");
        let w1 = d.arch.wall(ArchId(1)).expect("w1");
        assert!((w0.end.x - 12.0).abs() < 1e-9, "w0.end.x should be 12, got {}", w0.end.x);
        assert!((w1.start.x - 12.0).abs() < 1e-9, "w1.start.x should be 12, got {}", w1.start.x);
    }

    // First undo restores both walls to their original positions.
    s.undo().expect("undo");
    {
        let d = s.doc().expect("doc");
        let w0 = d.arch.wall(ArchId(0)).expect("w0 after undo");
        let w1 = d.arch.wall(ArchId(1)).expect("w1 after undo");
        assert!((w0.end.x - 10.0).abs() < 1e-9, "w0.end.x should be 10 after undo, got {}", w0.end.x);
        assert!((w1.start.x - 10.0).abs() < 1e-9, "w1.start.x should be 10 after undo, got {}", w1.start.x);
    }

    // Second undo removes the wall command entirely.
    s.undo().expect("undo wall");
    assert!(s.doc().expect("doc").arch.walls.is_empty(), "walls should be empty after undoing the wall command");
}

/// `grip.stretch` on an arch opening must be exactly one undo step.
#[test]
fn grip_stretch_of_an_arch_opening_is_one_undo_step() {
    let mut s = make_wall_with_door();
    let steps_after_door = s.state().expect("state").undo.len();

    // Drag opening o0's center grip (index 0) to offset 6.
    let oh = opening_handle(ArchId(0)).hex();
    s.execute("grip.stretch", &json!({"handle": oh, "index": 0, "to": [6.0, 0.0]})).expect("grip.stretch on opening");

    assert_eq!(s.state().expect("state").undo.len(), steps_after_door + 1, "grip.stretch on opening must be one undo step");

    {
        let d = s.doc().expect("doc");
        assert!((d.arch.opening(ArchId(0)).expect("o0").offset - 6.0).abs() < 1e-9, "offset should be 6.0 after stretch");
    }

    // First undo reverts the offset.
    s.undo().expect("undo stretch");
    {
        let d = s.doc().expect("doc");
        assert!((d.arch.opening(ArchId(0)).expect("o0 after undo").offset - 3.0).abs() < 1e-9, "offset should be 3.0 after undo");
    }

    // Second undo removes the door (undoes the door command).
    s.undo().expect("undo door");
    assert!(s.doc().expect("doc").arch.openings.is_empty(), "openings should be empty after undoing the door command");
}

/// `grip.stretch` with index >= 2 on an arch wall must return `BadParams`.
#[test]
fn grip_stretch_of_an_arch_wall_rejects_a_third_grip() {
    let mut s = make_l();
    let h0 = arch_handle(ArchId(0)).hex();

    let r = s.execute("grip.stretch", &json!({"handle": h0, "index": 7, "to": [12.0, 0.0]}));
    assert!(matches!(r, Err(EngineError::BadParams { .. })), "index 7 should be BadParams, got {:?}", r);

    // Wall must be unchanged.
    let d = s.doc().expect("doc");
    let w0 = d.arch.wall(ArchId(0)).expect("w0");
    assert!((w0.end.x - 10.0).abs() < 1e-9, "w0.end.x should still be 10 after rejection, got {}", w0.end.x);
}

/// `Session::grip_edit` with index >= 2 on an arch wall must return `Err`.
#[test]
fn grip_edit_of_an_arch_wall_rejects_a_third_grip() {
    let mut s = make_l();
    let h = arch_handle(ArchId(0));

    let r = s.grip_edit(h, 7, Vec2::new(12.0, 0.0), GripMode::Stretch);
    assert!(r.is_err(), "grip_edit with index 7 on a wall should return Err");

    // Wall must be unchanged.
    let d = s.doc().expect("doc");
    let w0 = d.arch.wall(ArchId(0)).expect("w0");
    assert!((w0.end.x - 10.0).abs() < 1e-9, "w0.end.x should still be 10 after rejection, got {}", w0.end.x);
}

/// When the A-WALL layer is locked, both `grip_edit` and `grip.stretch` on a
/// wall must be refused with the locked-layer error, and the wall is unchanged.
#[test]
fn grip_edit_of_an_arch_wall_on_a_locked_layer_is_refused() {
    let mut s = make_l();

    // Lock the A-WALL layer.
    s.doc_mut().expect("doc_mut").layer_mut("A-WALL").expect("A-WALL layer").locked = true;

    let h = arch_handle(ArchId(0));

    // grip_edit must refuse.
    let r = s.grip_edit(h, 1, Vec2::new(12.0, 0.0), GripMode::Stretch);
    assert!(r.is_err(), "grip_edit on a locked wall should return Err");

    // grip.stretch must refuse.
    let h0 = h.hex();
    let r = s.execute("grip.stretch", &json!({"handle": h0, "index": 1, "to": [12.0, 0.0]}));
    assert!(r.is_err(), "grip.stretch on a locked wall should return Err");

    // Wall must be unchanged.
    let d = s.doc().expect("doc");
    let w0 = d.arch.wall(ArchId(0)).expect("w0");
    assert!((w0.end.x - 10.0).abs() < 1e-9, "w0.end.x should still be 10, got {}", w0.end.x);
    let w1 = d.arch.wall(ArchId(1)).expect("w1");
    assert!((w1.start.x - 10.0).abs() < 1e-9, "w1.start.x should still be 10, got {}", w1.start.x);
}

/// When the opening's layer (A-DOOR) is locked, `grip.stretch` on the opening
/// must be refused and the offset unchanged.
#[test]
fn grip_stretch_of_an_arch_opening_on_a_locked_layer_is_refused() {
    let mut s = make_wall_with_door();

    // Lock the A-DOOR layer.
    s.doc_mut().expect("doc_mut").layer_mut("A-DOOR").expect("A-DOOR layer").locked = true;

    let oh = opening_handle(ArchId(0)).hex();
    let r = s.execute("grip.stretch", &json!({"handle": oh, "index": 0, "to": [6.0, 0.0]}));
    assert!(r.is_err(), "grip.stretch on a locked opening should return Err");

    // Offset must be unchanged.
    let d = s.doc().expect("doc");
    let o = d.arch.opening(ArchId(0)).expect("o0");
    assert!((o.offset - 3.0).abs() < 1e-9, "offset should still be 3.0, got {}", o.offset);
}

/// When the opening's layer is locked, `grip_edit` on the opening must be refused.
#[test]
fn grip_edit_of_an_arch_opening_on_a_locked_layer_is_refused() {
    let mut s = make_wall_with_door();

    // Lock the A-DOOR layer.
    s.doc_mut().expect("doc_mut").layer_mut("A-DOOR").expect("A-DOOR layer").locked = true;

    let h = opening_handle(ArchId(0));
    let r = s.grip_edit(h, 0, Vec2::new(6.0, 0.0), GripMode::Stretch);
    assert!(r.is_err(), "grip_edit on a locked opening should return Err");

    // Offset must be unchanged.
    let d = s.doc().expect("doc");
    let o = d.arch.opening(ArchId(0)).expect("o0");
    assert!((o.offset - 3.0).abs() < 1e-9, "offset should still be 3.0, got {}", o.offset);
}

/// `grip_edit` on a wall with a non-finite point must return Err.
#[test]
fn grip_edit_of_an_arch_wall_with_nan_point_is_refused() {
    let mut s = make_l();
    let h = arch_handle(ArchId(0));

    let nan = Vec2::new(f64::NAN, f64::NAN);
    let r = s.grip_edit(h, 1, nan, GripMode::Stretch);
    assert!(r.is_err(), "grip_edit with NaN point should return Err");

    let inf = Vec2::new(f64::INFINITY, f64::NEG_INFINITY);
    let r = s.grip_edit(h, 1, inf, GripMode::Stretch);
    assert!(r.is_err(), "grip_edit with inf point should return Err");

    // Wall must be unchanged.
    let d = s.doc().expect("doc");
    let w0 = d.arch.wall(ArchId(0)).expect("w0");
    assert!((w0.end.x - 10.0).abs() < 1e-9, "w0.end.x should still be 10, got {}", w0.end.x);
}

/// `grip.stretch` on an opening with index >= 3 must return BadParams.
#[test]
fn grip_stretch_of_an_arch_opening_rejects_a_fourth_grip() {
    let mut s = make_wall_with_door();
    let oh = opening_handle(ArchId(0)).hex();

    let r = s.execute("grip.stretch", &json!({"handle": oh, "index": 3, "to": [6.0, 0.0]}));
    assert!(matches!(r, Err(EngineError::BadParams { .. })), "index 3 should be BadParams, got {:?}", r);

    // Offset must be unchanged.
    let d = s.doc().expect("doc");
    let o = d.arch.opening(ArchId(0)).expect("o0");
    assert!((o.offset - 3.0).abs() < 1e-9, "offset should still be 3.0, got {}", o.offset);
}

/// `grip_edit` on an opening with index >= 3 must return Err.
#[test]
fn grip_edit_of_an_arch_opening_rejects_a_fourth_grip() {
    let mut s = make_wall_with_door();
    let h = opening_handle(ArchId(0));

    let r = s.grip_edit(h, 3, Vec2::new(6.0, 0.0), GripMode::Stretch);
    assert!(r.is_err(), "grip_edit with index 3 on an opening should return Err");

    // Offset must be unchanged.
    let d = s.doc().expect("doc");
    let o = d.arch.opening(ArchId(0)).expect("o0");
    assert!((o.offset - 3.0).abs() < 1e-9, "offset should still be 3.0, got {}", o.offset);
}
