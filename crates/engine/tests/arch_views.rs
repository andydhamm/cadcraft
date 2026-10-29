//! Elevation / section / view.delete commands, linked regeneration and the `views` report of `arch.inspect`, driven
//! through `Session::execute` on the phase-1 house with its gable roof.

use serde_json::{Value, json};

use cadcraft_doc::ArchId;
use cadcraft_doc::arch::view_handle;
use cadcraft_engine::Session;
use cadcraft_engine::grips::{ArchTarget, arch_id_from_handle};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p} failed: {e}"))
}

/// The house, door at offset 3.0, rooms and a 6/12 gable roof with a 0.5 m overhang.
fn house() -> Session {
    let mut s = Session::new();
    run(&mut s, "wall", json!({"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}));
    run(&mut s, "door", json!({"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}));
    run(&mut s, "roomdetect", json!({}));
    run(
        &mut s,
        "roofauto",
        json!({"pitchRise":6,"pitchRun":12,"overhang":0.5,"walls":[{"wall":"w0","directive":"gable"},{"wall":"w2","directive":"gable"}]}),
    );
    s
}

fn views(s: &mut Session) -> Vec<Value> {
    run(s, "arch.inspect", json!({"views": true}))["views"].as_array().cloned().unwrap_or_default()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

/// u-span of the lines of opening `id` in view `v`.
fn span(v: &Value, id: &str) -> (f64, f64) {
    let us: Vec<f64> = v["lines"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|l| l["kind"] == "opening" && l["id"] == id)
        .flat_map(|l| [f(&l["a"][0]), f(&l["b"][0])])
        .collect();
    assert!(!us.is_empty(), "no lines of {id}");
    (us.iter().cloned().fold(f64::INFINITY, f64::min), us.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
}

#[test]
fn elevation_creates_a_linked_view_with_lines_and_dims() {
    let mut s = house();
    let r = run(&mut s, "elevation", json!({"side":"south","insert":[0,-20]}));
    assert_eq!(r["view"], "v0");
    assert_eq!(r["handle"], "a0030000");
    assert_eq!(r["name"], "South Elevation");
    assert!(r["lines"].as_u64().unwrap_or(0) >= 20, "{r}");
    let vs = views(&mut s);
    assert_eq!(vs.len(), 1);
    let v = &vs[0];
    assert_eq!(v["kind"], "elevation");
    assert_eq!(v["layer"], "A-ELEV");
    assert!(f(&v["look"][0]).abs() < 1e-12 && (f(&v["look"][1]) - 1.0).abs() < 1e-12);
    let rev = run(&mut s, "arch.inspect", json!({}))["revision"].clone();
    assert_eq!(v["revision"], rev);
    let e: Vec<f64> = v["extent"].as_array().unwrap_or(&vec![]).iter().map(f).collect();
    assert_eq!(e.len(), 4);
    assert!((e[2] - e[0] - 11.2).abs() < 0.005, "outline width {}", e[2] - e[0]);
    let all = v["lines"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<&str> = all.iter().filter_map(|l| l["kind"].as_str()).collect();
    for k in ["wall", "opening", "roof"] {
        assert!(kinds.contains(&k), "no {k} lines");
    }
    let b: Vec<f64> = v["bounds"].as_array().unwrap_or(&vec![]).iter().map(f).collect();
    assert!((b[0] - 0.0).abs() < 1e-9 && (b[1] + 20.0).abs() < 1e-9);
    assert!((b[2] - (e[2] - e[0])).abs() < 1e-6 && (b[3] - (-20.0 + e[3] - e[1])).abs() < 1e-6);
    let dims = v["dims"].as_array().cloned().unwrap_or_default();
    assert_eq!(dims.len(), 2);
    let w = dims.iter().find(|d| d["kind"] == "width");
    assert_eq!(w.map(|d| d["text"].clone()), Some(json!("11200")));
}

#[test]
fn section_defaults_to_the_left_normal_and_a_numbered_name() {
    let mut s = house();
    let r = run(&mut s, "section", json!({"from":[-1,4],"to":[11,4]}));
    assert_eq!(r["view"], "v0");
    assert_eq!(r["name"], "Section 1");
    let r2 = run(&mut s, "section", json!({"from":[5,-1],"to":[5,9],"name":"Cross"}));
    assert_eq!(r2["name"], "Cross");
    let r3 = run(&mut s, "section", json!({"from":[5,-1],"to":[5,9]}));
    assert_eq!(r3["name"], "Section 3");
    let vs = views(&mut s);
    assert_eq!(vs[0]["kind"], "section");
    assert_eq!(vs[0]["layer"], "A-SECT");
    assert!(f(&vs[0]["look"][0]).abs() < 1e-12 && (f(&vs[0]["look"][1]) - 1.0).abs() < 1e-12, "{}", vs[0]["look"]);
    assert_eq!(vs[0]["cut"], json!([[-1.0, 4.0], [11.0, 4.0]]));
    assert!((f(&vs[1]["look"][0]) + 1.0).abs() < 1e-12, "left normal of a north-pointing cut is -x: {}", vs[1]["look"]);
    let d = s.doc().map(|d| (d.layer("A-SECT").is_some(), d.layer("A-SECT-MCUT").is_some()));
    assert_eq!(d.ok(), Some((true, true)));
}

#[test]
fn moving_a_window_regenerates_the_view_with_no_other_command() {
    let mut s = house();
    run(&mut s, "window", json!({"wall":"w0","offset":6.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}));
    run(&mut s, "elevation", json!({"side":"south","insert":[0,-20]}));
    let before = views(&mut s);
    let (b0, b1) = span(&before[0], "o1");
    let door = span(&before[0], "o0");
    run(&mut s, "arch.opening.move", json!({"opening":"o1","offset":7.0}));
    let after = views(&mut s);
    let (a0, a1) = span(&after[0], "o1");
    assert!((a0 - b0 - 1.0).abs() < 1e-6 && (a1 - b1 - 1.0).abs() < 1e-6, "{b0}..{b1} -> {a0}..{a1}");
    assert_eq!(span(&after[0], "o0"), door);
    assert_ne!(after[0]["revision"], before[0]["revision"]);
}

#[test]
fn undo_restores_the_previous_view_geometry() {
    let mut s = house();
    run(&mut s, "window", json!({"wall":"w0","offset":6.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}));
    run(&mut s, "elevation", json!({"side":"south","insert":[0,-20]}));
    let before = views(&mut s);
    run(&mut s, "arch.opening.move", json!({"opening":"o1","offset":7.0}));
    assert_ne!(span(&views(&mut s)[0], "o1"), span(&before[0], "o1"));
    run(&mut s, "u", json!({}));
    let back = views(&mut s);
    assert_eq!(span(&back[0], "o1"), span(&before[0], "o1"));
    assert_eq!(back[0]["revision"], before[0]["revision"]);
}

#[test]
fn views_regenerate_after_opening_a_drawing() {
    let mut s = house();
    run(&mut s, "elevation", json!({"side":"south"}));
    let want = views(&mut s);
    assert!(!want[0]["lines"].as_array().map_or(true, |l| l.is_empty()));
    let mut d = s.doc().map(|d| d.clone()).ok().unwrap_or_default();
    for v in &mut d.arch.views {
        v.generated = None;
    }
    let mut s2 = Session::new();
    s2.open_drawing(d, "reopened", None);
    let got = views(&mut s2);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0]["lines"], want[0]["lines"]);
    assert_eq!(got[0]["revision"], run(&mut s2, "arch.inspect", json!({}))["revision"]);
}

#[test]
fn hostile_view_params_are_refused_and_change_nothing() {
    let mut s = house();
    run(&mut s, "elevation", json!({"side":"south","insert":[0,-20]}));
    let base = (views(&mut s), run(&mut s, "arch.inspect", json!({})));
    let bad: Vec<(&str, Value)> = vec![
        ("elevation", json!({})),
        ("elevation", json!({"side":"up"})),
        ("elevation", json!({"look":[0,0]})),
        ("elevation", json!({"look":[1e308,1e308]})),
        ("elevation", json!({"look":["a",1]})),
        ("elevation", json!({"side":"south","insert":[1e308,0]})),
        ("elevation", json!({"side":"south","scale":0})),
        ("elevation", json!({"side":"south","scale":-50})),
        ("elevation", json!({"side":"south","scale":1e9})),
        ("elevation", json!({"side":"south","look":[0,1]})),
        ("elevation", json!({"side":"south","name":5})),
        ("elevation", json!({"side":"south","dimensions":"yes"})),
        ("section", json!({"from":[1,1],"to":[1,1]})),
        ("section", json!({"from":[-1,4],"to":[11,4],"look":[1,0]})),
        ("section", json!({"from":[0,0]})),
        ("section", json!({"from":[-1,4],"to":[11,4],"depth":-1})),
        ("section", json!({"from":[-1,4],"to":[1e308,4]})),
        ("view.delete", json!({"view":"v9"})),
        ("view.delete", json!({})),
    ];
    for (c, p) in bad {
        assert!(s.execute(c, &p).is_err(), "accepted: {c} {p}");
        assert_eq!(views(&mut s), base.0, "{c} {p} changed the views");
        assert_eq!(run(&mut s, "arch.inspect", json!({})), base.1, "{c} {p} changed the model");
    }
}

#[test]
fn view_delete_removes_the_view() {
    let mut s = house();
    run(&mut s, "elevation", json!({"side":"south"}));
    run(&mut s, "elevation", json!({"look":[3,4]}));
    let vs = views(&mut s);
    assert_eq!(vs[1]["name"], "Elevation 2");
    assert!((f(&vs[1]["look"][0]) - 0.6).abs() < 1e-12 && (f(&vs[1]["look"][1]) - 0.8).abs() < 1e-12);
    assert_eq!(run(&mut s, "view.delete", json!({"view":"v0"})), json!({"deleted":"v0"}));
    let vs = views(&mut s);
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0]["id"], "v1");
}

#[test]
fn inspect_views_reports_the_documented_fields() {
    let mut s = house();
    run(&mut s, "section", json!({"from":[-1,4],"to":[11,4],"depth":30.0,"scope":[[-1,-1],[11,9]],"scale":100,"dimensions":false}));
    run(&mut s, "elevation", json!({"side":"east"}));
    assert!(run(&mut s, "arch.inspect", json!({})).get("views").is_none());
    let vs = views(&mut s);
    assert_eq!(vs.len(), 2);
    let v = &vs[0];
    for k in [
        "id",
        "handle",
        "name",
        "kind",
        "look",
        "cut",
        "depth",
        "scope",
        "scale",
        "insert",
        "dimensions",
        "layer",
        "revision",
        "extent",
        "bounds",
        "lines",
        "rings",
        "dims",
        "error",
    ] {
        assert!(v.get(k).is_some(), "missing {k}");
    }
    assert_eq!(v["depth"], json!(30.0));
    assert_eq!(v["scope"], json!([[-1.0, -1.0], [11.0, 9.0]]));
    assert_eq!(v["scale"], json!(100.0));
    assert_eq!(v["dimensions"], json!(false));
    assert_eq!(v["dims"], json!([]));
    assert_eq!(v["error"], Value::Null);
    assert!(v["rings"].as_array().is_some_and(|r| !r.is_empty()), "a section has poché rings");
    let ring = &v["rings"][0];
    for k in ["kind", "id", "material", "points", "area"] {
        assert!(ring.get(k).is_some(), "ring missing {k}");
    }
    assert!(f(&ring["area"]) > 0.0);
    let l = &v["lines"][0];
    for k in ["a", "b", "kind", "id"] {
        assert!(l.get(k).is_some(), "line missing {k}");
    }
    assert_eq!(vs[1]["id"], "v1");
    assert_eq!(vs[1]["handle"], "a0030001");
    assert_eq!(vs[1]["cut"], Value::Null);
    assert_eq!(vs[1]["depth"], Value::Null);
    assert_eq!(vs[1]["insert"], json!([0.0, -30.0]));
}

#[test]
fn a_view_handle_resolves_to_the_view_target() {
    assert_eq!(arch_id_from_handle(view_handle(ArchId(0))), Some(ArchTarget::View(ArchId(0))));
    assert_eq!(arch_id_from_handle(view_handle(ArchId(7))), Some(ArchTarget::View(ArchId(7))));
}
