use cadcraft_doc::arch::{ArchView, ViewGeometry, ViewLine};
use cadcraft_engine::Session;
use serde_json::json;

/// Helper to create a mock ArchView with generated geometry.
fn make_view(name: &str, scale: f64, w: f64, h: f64) -> ArchView {
    let mut v = ArchView { name: name.into(), scale, ..Default::default() };

    // Create generated geometry with bounds [0, 0, w, h]
    let mut geometry = ViewGeometry::default();
    geometry.extent = [0.0, 0.0, w, h];

    // Add a line to ensure it's not empty, though extent is what matters for bounds()
    // Assuming bounds() uses extent. If bounds() calculates from lines, we need lines.
    // Let's add a line covering the bounds.
    geometry.lines.push(ViewLine { a: [0.0, 0.0], b: [w, h], kind: cadcraft_doc::arch::ArchKind::View, object: cadcraft_doc::arch::ArchId(0) });

    v.generated = Some(geometry);
    v
}

#[test]
fn viewsheet_puts_each_view_in_its_own_viewport_at_1_to_50() {
    let mut s = Session::new();

    // Setup 4 views
    for i in 0..4 {
        let v = make_view(&format!("v{i}"), 50.0, 11.2, 5.7);
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
        assert_eq!(vp["scale"], 20.0); // 1:50 metric -> 20 mm/m
        assert!(vp["handle"].is_string());
    }
}

#[test]
fn viewsheet_lays_out_two_per_row_from_the_top_left() {
    let mut s = Session::new();

    // Setup 4 views
    for i in 0..4 {
        let v = make_view(&format!("v{i}"), 50.0, 11.2, 5.7);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    let res = s
        .execute(
            "viewsheet",
            &json!({
                "name": "Layout1",
                "views": ["v0", "v1", "v2", "v3"],
                "paper": "ARCH D",
                "landscape": true
            }),
        )
        .unwrap();

    let vps = res["viewports"].as_array().unwrap();

    // Check positions
    // v0: Top Left
    // v1: Top Right
    // v2: Bottom Left
    // v3: Bottom Right

    // Y of v0 should be > Y of v2
    // X of v0 should be < X of v1

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
        let v = make_view(&format!("v{i}"), 50.0, 11.2, 5.7);
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

    // Check layout entities
    let layout = s.doc().unwrap().layouts.iter().find(|l| l.name == "NoDefault").unwrap();

    // Count viewports
    // layout.entities is an EntityStore, we need to iterate over handles or entities
    // EntityStore iterates over &Arc<Entity>
    let vp_count = layout.entities.iter().filter(|e| matches!(e.kind, cadcraft_doc::EntityKind::Viewport(_))).count();

    assert_eq!(vp_count, 4, "Should have exactly 4 viewports, no default");
}

#[test]
fn viewsheet_refuses_bad_input_and_changes_nothing() {
    let mut s = Session::new();

    // Add 1 view
    let v = make_view("v0", 50.0, 11.2, 5.7);
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();

    // Empty views
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

    // Unknown view
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

    // Duplicate views
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

    // Unknown paper
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
