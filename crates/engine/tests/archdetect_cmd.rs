//! Integration tests for the `archdetect` command: dry run, accept, units and refusals.

use serde_json::{Value, json};

use cadcraft_doc::{Arc, Common, EntityKind, Line, Space};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;

fn add(s: &mut Session, layer: &str, kind: EntityKind) {
    let d = s.doc_mut().expect("doc");
    d.add(&Space::Model, Common { layer: layer.into(), ..Common::default() }, kind).expect("add");
}

fn line(s: &mut Session, layer: &str, a: (f64, f64), b: (f64, f64), k: f64) {
    add(s, layer, EntityKind::Line(Line { a: Vec3::new(a.0 * k, a.1 * k, 0.0), b: Vec3::new(b.0 * k, b.1 * k, 0.0) }));
}

/// A 10 x 5 m plan, 0.2 m exterior walls and a 0.1 m partition at x = 5 with a door gap (y 2..3)
/// and its swing arc; every coordinate is multiplied by `k` (1000 for a millimetre drawing).
fn two_rooms(k: f64, insunits: i64) -> Session {
    let mut s = Session::empty();
    s.new_drawing(false);
    s.doc_mut().expect("doc").header.set_i64("INSUNITS", insunits);
    let w = "A-WALL";
    for (o, rect) in [(0.1, (-0.1, -0.1, 10.1, 5.1)), (-0.1, (0.1, 0.1, 9.9, 4.9))] {
        let _ = o;
        let (x0, y0, x1, y1) = rect;
        line(&mut s, w, (x0, y0), (x1, y0), k);
        line(&mut s, w, (x1, y0), (x1, y1), k);
        line(&mut s, w, (x1, y1), (x0, y1), k);
        line(&mut s, w, (x0, y1), (x0, y0), k);
    }
    for x in [4.95, 5.05] {
        line(&mut s, w, (x, 0.1), (x, 2.0), k);
        line(&mut s, w, (x, 3.0), (x, 4.9), k);
    }
    line(&mut s, w, (4.95, 2.0), (5.05, 2.0), k);
    line(&mut s, w, (4.95, 3.0), (5.05, 3.0), k);
    line(&mut s, "A-DOOR", (5.0, 2.0), (5.9, 2.0), k);
    add(
        &mut s,
        "A-DOOR",
        EntityKind::Arc(Arc { center: Vec3::new(5.0 * k, 2.0 * k, 0.0), radius: 0.9 * k, start: 0.0, end: std::f64::consts::FRAC_PI_2 }),
    );
    s
}

fn dry(s: &mut Session, extra: Value) -> Value {
    let mut p = json!({"dryRun": true});
    if let (Some(m), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    s.execute("archdetect", &p).expect("dry run")
}

fn arr<'a>(v: &'a Value, k: &str) -> &'a Vec<Value> {
    v[k].as_array().expect("array")
}

#[test]
fn a_dry_run_changes_nothing() {
    let mut s = two_rooms(1.0, 6);
    let rev = s.doc().expect("doc").arch_revision;
    let r = s.execute("archdetect", &json!({})).expect("dry run");
    assert_eq!(r["units"], "m");
    assert_eq!(r["scale"], 1.0);
    assert!(!arr(&r, "walls").is_empty(), "walls: {r}");
    assert!(!arr(&r, "rooms").is_empty(), "rooms: {r}");
    let walls = arr(&r, "walls").len();
    for (i, w) in arr(&r, "walls").iter().enumerate() {
        assert_eq!(w["id"], format!("c{i}"));
        assert!(w["confidence"].as_f64().is_some());
        assert!(w["evidence"]["rules"].is_array() && w["evidence"]["handles"].is_array() && w["evidence"]["layers"].is_array());
    }
    for (i, o) in arr(&r, "openings").iter().enumerate() {
        assert_eq!(o["id"], format!("c{}", walls + i));
        assert!(o["host"].as_str().is_some_and(|h| h.starts_with('c')));
    }
    assert!(r["stats"]["entities"].as_u64().is_some_and(|n| n > 0));
    let d = s.doc().expect("doc");
    assert_eq!(d.arch_revision, rev);
    assert!(d.arch.walls.is_empty() && d.arch.rooms.is_empty());
    assert_eq!(s.undo().expect("undo"), None, "a dry run leaves no undo step");
}

#[test]
fn accept_all_creates_the_confident_candidates_in_one_undo_step() {
    let mut s = two_rooms(1.0, 6);
    let r = dry(&mut s, json!({}));
    let ok = |v: &Value| v["confidence"].as_f64().is_some_and(|c| c >= 0.5);
    let walls = arr(&r, "walls").iter().filter(|v| ok(v)).count();
    let rooms = arr(&r, "rooms").iter().filter(|v| ok(v)).count();
    assert!(walls > 0);

    let a = s.execute("archdetect", &json!({"accept": "all"})).expect("accept");
    assert_eq!(a["walls"].as_u64(), Some(walls as u64));
    assert_eq!(a["rooms"].as_u64(), Some(rooms as u64));
    assert!(a["converted"].is_null());
    let d = s.doc().expect("doc");
    assert_eq!(d.arch.walls.len(), walls);
    assert_eq!(d.arch.rooms.len(), rooms);
    assert_eq!(a["accepted"].as_object().map(|m| m.len()), Some(walls + rooms + a["openings"].as_u64().unwrap_or(0) as usize));
    assert!(a["accepted"]["c0"].as_str().is_some_and(|l| l.starts_with('w')));

    assert!(s.undo().expect("undo").is_some());
    let d = s.doc().expect("doc");
    assert!(d.arch.walls.is_empty() && d.arch.openings.is_empty() && d.arch.rooms.is_empty());
    assert_eq!(s.undo().expect("undo"), None, "everything was one undo step");
}

#[test]
fn accepting_in_millimetres_converts_the_drawing() {
    let mut s = two_rooms(1000.0, 4);
    let before = s.doc().expect("doc").clone();
    let r = dry(&mut s, json!({}));
    assert_eq!(r["units"], "mm");
    assert_eq!(r["scale"], 0.001);
    let far = arr(&r, "walls").iter().flat_map(|w| [w["start"][0].as_f64(), w["end"][0].as_f64()]).flatten().fold(0.0_f64, f64::max);
    assert!(far > 5.0 && far < 20.0, "candidates are in metres: {far}");

    let a = s.execute("archdetect", &json!({"accept": "all"})).expect("accept");
    assert_eq!(a["converted"], json!({"from": "mm", "scale": 0.001}));
    let d = s.doc().expect("doc");
    assert_eq!(d.header.i64("INSUNITS", 0), 6);
    assert!(!d.arch.walls.is_empty());

    assert!(s.undo().expect("undo").is_some());
    let d = s.doc().expect("doc");
    assert_eq!(d.header.i64("INSUNITS", 0), 4);
    assert!(d.arch.walls.is_empty());
    assert_eq!(*d, before);
}

#[test]
fn an_opening_without_its_wall_is_refused() {
    let mut s = two_rooms(1.0, 6);
    let r = dry(&mut s, json!({}));
    let o = arr(&r, "openings").first().expect("the door in the partition is detected");
    let (id, host) = (o["id"].as_str().expect("id").to_string(), o["host"].as_str().expect("host").to_string());
    let before = s.doc().expect("doc").clone();
    let e = s.execute("archdetect", &json!({"accept": [id]})).expect_err("host missing");
    let msg = e.to_string();
    assert!(msg.contains(&id) && msg.contains(&host), "{msg}");
    assert_eq!(*s.doc().expect("doc"), before);
    assert_eq!(s.undo().expect("undo"), None);
}

#[test]
fn bad_params_are_refused_and_change_nothing() {
    let mut s = two_rooms(1.0, 6);
    let before = s.doc().expect("doc").clone();
    let bads = [
        json!({"dryRun": true, "accept": "all"}),
        json!({"dryRun": "yes"}),
        json!({"accept": "some"}),
        json!({"accept": [1]}),
        json!({"accept": ["c999"]}),
        json!({"accept": ["nope"]}),
        json!({"minConfidence": 1.5}),
        json!({"minConfidence": -0.1}),
        json!({"minConfidence": "high"}),
        json!({"units": "furlong"}),
        json!({"units": 5}),
        json!({"layers": "A-WALL"}),
        json!({"layers": [3]}),
    ];
    for p in bads {
        assert!(s.execute("archdetect", &p).is_err(), "{p}");
    }
    assert!(s.execute("archdetect", &json!([1])).is_err());
    assert_eq!(*s.doc().expect("doc"), before);
    assert_eq!(s.undo().expect("undo"), None);

    // An unknown $INSUNITS code is refused unless `units` is passed.
    let mut u = two_rooms(1.0, 99);
    assert!(u.execute("archdetect", &json!({})).is_err());
    assert!(u.execute("archdetect", &json!({"units": "m"})).is_ok());
}

#[test]
fn dry_runs_are_deterministic() {
    let mut s = two_rooms(1.0, 6);
    let a = dry(&mut s, json!({}));
    let b = dry(&mut s, json!({}));
    for k in ["walls", "openings", "rooms", "units", "scale"] {
        assert_eq!(a[k], b[k], "{k}");
    }
}
