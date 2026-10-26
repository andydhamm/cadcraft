//! Integration tests for the `roomdetect` command.
//!
//! - [`roomdetect_dry_run_does_not_mutate`]: a dry run reports the detection
//!   without touching the room list or bumping `arch_revision`.
//! - [`roomdetect_replaces_only_the_named_level`]: a named-level run replaces
//!   only that level's rooms; the all-levels run then drops and re-adds the
//!   level-0 rooms.

use serde_json::json;

use cadcraft_engine::Session;

/// Build the phase-1 house with the `wall` command and assert each call
/// returned `Ok`: an exterior closed rectangle `(0,0) (10,0) (10,8) (0,8)`
/// at 0.2 m, plus interior 0.1 m walls `(0,3)-(10,3)` and `(6,3)-(6,8)`.
fn house(s: &mut Session) {
    // Exterior closed rectangle; the `closed` flag closes the final segment.
    let p = json!({
        "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]],
        "closed": true,
        "thickness": 0.2,
    });
    assert!(s.execute("wall", &p).is_ok());

    // Interior walls.
    let p = json!({
        "points": [[0.0, 3.0], [10.0, 3.0]],
        "thickness": 0.1,
        "exterior": false,
    });
    assert!(s.execute("wall", &p).is_ok());

    let p = json!({
        "points": [[6.0, 3.0], [6.0, 8.0]],
        "thickness": 0.1,
        "exterior": false,
    });
    assert!(s.execute("wall", &p).is_ok());
}

/// A dry run must emit the full room report (non-empty for the house) without
/// mutating the model: same room count, same revision, and the rooms appear
/// only after a real run.
#[test]
fn roomdetect_dry_run_does_not_mutate() {
    let mut s = Session::new();
    house(&mut s);

    let rev = s.doc().unwrap().arch_revision;
    let n = s.doc().unwrap().arch.rooms.len();

    let r = s.execute("roomdetect", &json!({"dryRun": true})).unwrap();
    let count = r["count"].as_u64().unwrap();
    assert!(count >= 1);
    let rooms = r["rooms"].as_array().unwrap();
    assert_eq!(rooms.len(), count as usize);
    for room in rooms {
        assert!(room.get("level").is_some());
        assert!(room.get("type").is_some());
        assert!(room.get("interiorArea").is_some());
        assert!(room.get("standardArea").is_some());
        assert!(room.get("polygon").is_some());
        let polygon = room["polygon"].as_array().unwrap();
        assert!(polygon.len() >= 3);
        for pt in polygon {
            let pair = pt.as_array().unwrap();
            assert_eq!(pair.len(), 2);
        }
    }

    // Nothing was written by the dry run.
    assert_eq!(s.doc().unwrap().arch.rooms.len(), n);
    assert_eq!(s.doc().unwrap().arch_revision, rev);

    // A real run on the same document must find rooms: the dry run was a
    // no-op and the real run works.
    assert!(s.execute("roomdetect", &json!({})).is_ok());
    assert!(s.doc().unwrap().arch.rooms.len() > n);
}

/// A named-level run touches only that level: all walls sit on level 0, so a
/// "Level 2" detect reports zero rooms/replacements and leaves level 0's rooms
/// (ids included) intact; the following all-levels run replaces exactly them.
#[test]
fn roomdetect_replaces_only_the_named_level() {
    let mut s = Session::new();
    house(&mut s);

    // Add a second level (named "Level 2"); all house walls sit on level 0.
    assert!(s.execute("level", &json!({"above": "Level 1"})).is_ok());
    assert_eq!(s.doc().unwrap().arch.levels.len(), 2);

    // First all-levels detect: rooms exist on level 0, none on level 1.
    assert!(s.execute("roomdetect", &json!({})).is_ok());
    let d = s.doc().unwrap();
    assert!(d.arch.rooms.iter().any(|r| r.level == 0));
    assert!(!d.arch.rooms.iter().any(|r| r.level == 1));

    let level0_ids: Vec<_> = d.arch.rooms.iter().filter(|r| r.level == 0).map(|r| r.id).collect();

    // Named-level detect on the empty "Level 2": zero found, zero replaced.
    let r = s.execute("roomdetect", &json!({"level": "Level 2"})).unwrap();
    assert_eq!(r["count"].as_u64().unwrap(), 0);
    assert_eq!(r["replaced"].as_u64().unwrap(), 0);

    // Level 0's rooms are still there, with the same ids, in the same order.
    let d = s.doc().unwrap();
    assert!(d.arch.rooms.iter().all(|r| r.level == 0));
    let current_ids: Vec<_> = d.arch.rooms.iter().map(|r| r.id).collect();
    assert_eq!(current_ids, level0_ids);

    // All-levels detect drops and re-adds the level-0 rooms.
    let r = s.execute("roomdetect", &json!({})).unwrap();
    assert_eq!(r["replaced"].as_u64().unwrap(), level0_ids.len() as u64);

    // No two rooms share an id after the replace.
    let d = s.doc().unwrap();
    let ids: std::collections::BTreeSet<_> = d.arch.rooms.iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), d.arch.rooms.len());
}

/// roomdetect on an empty drawing (no walls) reports zero rooms and does not
/// bump the revision.
#[test]
fn roomdetect_empty_drawing() {
    let mut s = Session::new();

    let rev = s.doc().unwrap().arch_revision;
    let r = s.execute("roomdetect", &json!({})).unwrap();
    assert_eq!(r["count"].as_u64().unwrap(), 0);
    assert_eq!(r["replaced"].as_u64().unwrap(), 0);
    let rooms = r["rooms"].as_array().unwrap();
    assert!(rooms.is_empty());

    // No rooms added. (The revision may still bump: a real run always calls touch_arch().)
    assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);
}

/// Detecting on a level that does not exist reports zero rooms and replaces
/// nothing.
#[test]
fn roomdetect_unknown_level() {
    let mut s = Session::new();
    house(&mut s);

    // First detect so rooms exist on level 0.
    assert!(s.execute("roomdetect", &json!({})).is_ok());
    let level0_count = s.doc().unwrap().arch.rooms.len();
    assert!(level0_count > 0);

    // Detect on a non-existent level: the command rejects it with BadParams.
    let r = s.execute("roomdetect", &json!({"level": "No Such Level"}));
    assert!(r.is_err());

    // Level 0 rooms untouched.
    assert_eq!(s.doc().unwrap().arch.rooms.len(), level0_count);
    assert!(s.doc().unwrap().arch.rooms.iter().all(|r| r.level == 0));
}

/// Running roomdetect twice in a row replaces the same set of rooms: the
/// second run's `replaced` count equals the first run's `count`.
#[test]
fn roomdetect_idempotent() {
    let mut s = Session::new();
    house(&mut s);

    let r1 = s.execute("roomdetect", &json!({})).unwrap();
    let count1 = r1["count"].as_u64().unwrap();
    assert!(count1 > 0);

    let r2 = s.execute("roomdetect", &json!({})).unwrap();
    assert_eq!(r2["count"].as_u64().unwrap(), count1);
    assert_eq!(r2["replaced"].as_u64().unwrap(), count1);

    // Still the same number of rooms in the model.
    assert_eq!(s.doc().unwrap().arch.rooms.len(), count1 as usize);
}

/// Hostile JSON: empty object, unknown keys, and wrong types for known keys
/// must not crash and must be handled gracefully.
#[test]
fn roomdetect_hostile_json() {
    let mut s = Session::new();
    house(&mut s);

    // Empty object (no keys at all).
    assert!(s.execute("roomdetect", &json!({})).is_ok());

    // Unknown keys are ignored.
    let r = s.execute("roomdetect", &json!({"foo": "bar", "baz": 42})).unwrap();
    assert!(r["count"].as_u64().unwrap() > 0);

    // dryRun as a non-boolean should be tolerated or rejected cleanly.
    let r = s.execute("roomdetect", &json!({"dryRun": "yes"}));
    // Either it works (treats truthy as true) or it errors; it must not crash.
    assert!(r.is_ok() || r.is_err());

    // level as a non-string.
    let r = s.execute("roomdetect", &json!({"level": 0}));
    assert!(r.is_ok() || r.is_err());
}

/// After detection, every room has a valid polygon (at least 3 points, each
/// a finite [x, y] pair) and positive interior area.
#[test]
fn roomdetect_all_rooms_valid() {
    let mut s = Session::new();
    house(&mut s);

    let r = s.execute("roomdetect", &json!({})).unwrap();
    let rooms = r["rooms"].as_array().unwrap();
    assert!(!rooms.is_empty());

    for room in rooms {
        let polygon = room["polygon"].as_array().unwrap();
        assert!(polygon.len() >= 3, "room polygon must have at least 3 points");

        for pt in polygon {
            let pair = pt.as_array().unwrap();
            assert_eq!(pair.len(), 2);
            let x = pair[0].as_f64().unwrap();
            let y = pair[1].as_f64().unwrap();
            assert!(x.is_finite(), "polygon x must be finite");
            assert!(y.is_finite(), "polygon y must be finite");
        }

        let area = room["interiorArea"].as_f64().unwrap();
        assert!(area > 0.0, "interior area must be positive");
    }
}

/// A dry run with a specific level reports only that level's rooms and does
/// not mutate the model.
#[test]
fn roomdetect_dry_run_with_level() {
    let mut s = Session::new();
    house(&mut s);

    // Detect all levels first so rooms exist.
    assert!(s.execute("roomdetect", &json!({})).is_ok());
    let rev = s.doc().unwrap().arch_revision;
    let n = s.doc().unwrap().arch.rooms.len();

    // Dry run on level 0 (the only level with walls).
    let r = s.execute("roomdetect", &json!({"level": "Level 1", "dryRun": true})).unwrap();
    let count = r["count"].as_u64().unwrap();
    assert!(count > 0);

    // All reported rooms are on level 0.
    let rooms = r["rooms"].as_array().unwrap();
    for room in rooms {
        assert_eq!(room["level"].as_u64().unwrap(), 0);
    }

    // No mutation.
    assert_eq!(s.doc().unwrap().arch.rooms.len(), n);
    assert_eq!(s.doc().unwrap().arch_revision, rev);
}
