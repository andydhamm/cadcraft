use cadcraft_doc::arch::{ArchId, ArchView, ViewGeometry, ViewLine};
use cadcraft_engine::Session;
use serde_json::json;

/// Helper to create a mock ArchView with generated geometry.
fn make_view(id: u32, name: &str, scale: f64, w: f64, h: f64) -> ArchView {
    let mut v = ArchView { id: ArchId(id), name: name.into(), scale, ..Default::default() };

    let mut geometry = ViewGeometry::default();
    geometry.extent = [0.0, 0.0, w, h];
    geometry.lines.push(ViewLine { a: [0.0, 0.0], b: [w, h], ..Default::default() });

    v.generated = Some(geometry);
    v
}

#[test]
fn viewsheet_puts_each_view_in_its_own_viewport_at_1_to_50() {
    let mut s = Session::new();

    for i in 0..4 {
        let v = make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "TestSheet",
            "views": ["v0", "v1", "v2", "v3"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet failed: {:?}", res.err());
    let val = res.unwrap();

    assert_eq!(val["layout"], "TestSheet");
    assert_eq!(val["paper"], "ARCH D");
    assert_eq!(val["landscape"], true);

    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 4);

    for vp in vps {
        assert_eq!(vp["scale"], 20.0);
        assert!(vp["handle"].is_string());
    }
}

#[test]
fn viewsheet_lays_out_two_per_row_from_the_top_left() {
    let mut s = Session::new();

    for i in 0..4 {
        let v = make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    let res = s
        .execute(
            "viewsheet",
            &json!({
                "name": "TestSheet2",
                "views": ["v0", "v1", "v2", "v3"],
                "paper": "ARCH D",
                "landscape": true
            }),
        )
        .unwrap();

    let vps = res["viewports"].as_array().unwrap();

    let y0 = vps[0]["center"][1].as_f64().unwrap();
    let y2 = vps[2]["center"][1].as_f64().unwrap();
    assert!(y0 > y2, "v0 should be above v2");

    let x0 = vps[0]["center"][0].as_f64().unwrap();
    let x1 = vps[1]["center"][0].as_f64().unwrap();
    assert!(x0 < x1, "v0 should be left of v1");
}

#[test]
fn viewsheet_adds_no_default_viewport() {
    let mut s = Session::new();

    for i in 0..4 {
        let v = make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    s.execute(
        "viewsheet",
        &json!({
            "name": "NoDefault",
            "views": ["v0", "v1", "v2", "v3"],
            "paper": "ARCH D",
            "landscape": true
        }),
    )
    .unwrap();

    let layout = s.doc().unwrap().layouts.iter().find(|l| l.name == "NoDefault").unwrap();
    let vp_count = layout.entities.iter().filter(|e| matches!(e.kind, cadcraft_doc::EntityKind::Viewport(_))).count();

    assert_eq!(vp_count, 4, "Should have exactly 4 viewports, no default");
}

#[test]
fn viewsheet_finds_views_by_id_when_names_differ() {
    let mut s = Session::new();
    let names = ["South Elevation", "East Elevation", "North Elevation", "West Elevation"];
    for (i, name) in names.iter().enumerate() {
        s.doc_mut().unwrap().arch.views.push(make_view(i as u32, name, 50.0, 11.2, 5.7));
    }

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "IDSheet",
            "views": ["v0", "v1", "v2", "v3"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet did not match ids: {:?}", res.err());
    let val = res.unwrap();
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 4);
    assert_eq!(vps[0]["view"], "v0");
    assert_eq!(vps[1]["view"], "v1");
    assert_eq!(vps[2]["view"], "v2");
    assert_eq!(vps[3]["view"], "v3");
}

#[test]
fn viewsheet_refuses_a_view_name_that_is_not_an_id() {
    let mut s = Session::new();
    let names = ["South Elevation", "East Elevation", "North Elevation", "West Elevation"];
    for (i, name) in names.iter().enumerate() {
        s.doc_mut().unwrap().arch.views.push(make_view(i as u32, name, 50.0, 11.2, 5.7));
    }

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "NameOnlySheet",
            "views": ["South Elevation"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err());
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_takes_the_id_an_elevation_command_replies() {
    let mut s = Session::new();
    s.execute(
        "wall",
        &json!({
            "points": [[0,0],[10,0],[10,8],[0,8],[0,0]],
            "thickness": 0.2,
            "height": 2.4,
            "exterior": true
        }),
    )
    .unwrap();

    let elev = s.execute("elevation", &json!({"side": "south", "insert": [0, -20]})).unwrap();
    let view_id = elev["view"].as_str().unwrap();

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Sheet",
            "views": [view_id],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet rejected elevation reply: {:?}", res.err());
    let val = res.unwrap();
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 1);
    assert_eq!(vps[0]["view"], view_id);
    assert_eq!(vps[0]["scale"], 20.0);
}

#[test]
fn viewsheet_refuses_bad_input_and_changes_nothing() {
    let mut s = Session::new();

    let v = make_view(0, "v0", 50.0, 11.2, 5.7);
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Bad",
            "views": [],
            "paper": "ARCH D"
        }),
    );
    assert!(res.is_err());
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Bad",
            "views": ["unknown"],
            "paper": "ARCH D"
        }),
    );
    assert!(res.is_err());
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Bad",
            "views": ["v0", "v0"],
            "paper": "ARCH D"
        }),
    );
    assert!(res.is_err());
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Bad",
            "views": ["v0"],
            "paper": "UNKNOWN_PAPER"
        }),
    );
    assert!(res.is_err());
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}
