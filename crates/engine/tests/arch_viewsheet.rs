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

/// Create a session with a metric drawing (paper units are mm).
fn metric_session() -> Session {
    let mut s = Session::new();
    s.new_drawing(true);
    s
}

#[test]
fn viewsheet_puts_each_view_in_its_own_viewport_at_1_to_50() {
    let mut s = metric_session();

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
    let mut s = metric_session();

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
    let mut s = metric_session();

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
    let mut s = metric_session();
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
    let mut s = metric_session();
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
fn viewsheet_uses_imperial_scale_for_non_metric_drawings() {
    let mut s = Session::new();
    s.doc_mut().unwrap().arch.views.push(make_view(0, "v0", 50.0, 11.2, 5.7));

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "ImperialSheet",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet failed on imperial drawing: {:?}", res.err());
    let val = res.unwrap();
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 1);
    // Imperial: k = 1000 / 25.4 / 50 = 0.7874015748031497
    let k = 1000.0 / 25.4 / 50.0;
    assert!((vps[0]["scale"].as_f64().unwrap() - k).abs() < 1e-9);
}

#[test]
fn viewsheet_viewport_positions_are_deterministic() {
    let mut s = metric_session();
    for i in 0..4 {
        s.doc_mut().unwrap().arch.views.push(make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7));
    }

    let res = s
        .execute(
            "viewsheet",
            &json!({
                "name": "PosSheet",
                "views": ["v0", "v1", "v2", "v3"],
                "paper": "ARCH D",
                "landscape": true
            }),
        )
        .unwrap();

    let vps = res["viewports"].as_array().unwrap();

    // Row 0: v0, v1 (same y, v0 left of v1)
    // Row 1: v2, v3 (same y, v2 left of v3)
    // Row 0 above Row 1
    let y0 = vps[0]["center"][1].as_f64().unwrap();
    let y1 = vps[1]["center"][1].as_f64().unwrap();
    let y2 = vps[2]["center"][1].as_f64().unwrap();
    let y3 = vps[3]["center"][1].as_f64().unwrap();
    assert!((y0 - y1).abs() < 0.001, "v0 and v1 should be on the same row");
    assert!((y2 - y3).abs() < 0.001, "v2 and v3 should be on the same row");
    assert!(y0 > y2, "row 0 should be above row 1");

    let x0 = vps[0]["center"][0].as_f64().unwrap();
    let x1 = vps[1]["center"][0].as_f64().unwrap();
    let x2 = vps[2]["center"][0].as_f64().unwrap();
    let x3 = vps[3]["center"][0].as_f64().unwrap();
    assert!(x0 < x1, "v0 should be left of v1");
    assert!(x2 < x3, "v2 should be left of v3");
    assert!((x0 - x2).abs() < 0.001, "v0 and v2 should be in the same column");
    assert!((x1 - x3).abs() < 0.001, "v1 and v3 should be in the same column");
}

#[test]
fn viewsheet_viewport_dimensions_match_grown_bounds() {
    let mut s = metric_session();
    // View with extent 10 x 5 m, scale 50 -> k = 20 mm/m
    // Grown bounds: (10 + 3) x (5 + 3) = 13 x 8 m
    // Viewport: 13 * 20 = 260 mm wide, 8 * 20 = 160 mm tall
    s.doc_mut().unwrap().arch.views.push(make_view(0, "v0", 50.0, 10.0, 5.0));

    let res = s
        .execute(
            "viewsheet",
            &json!({
                "name": "DimSheet",
                "views": ["v0"],
                "paper": "ARCH D",
                "landscape": true
            }),
        )
        .unwrap();

    let vps = res["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 1);
    let w = vps[0]["width"].as_f64().unwrap();
    let h = vps[0]["height"].as_f64().unwrap();
    assert!((w - 260.0).abs() < 0.001, "viewport width should be 260 mm, got {}", w);
    assert!((h - 160.0).abs() < 0.001, "viewport height should be 160 mm, got {}", h);
}

#[test]
fn viewsheet_works_with_a_single_view() {
    let mut s = metric_session();
    s.doc_mut().unwrap().arch.views.push(make_view(0, "v0", 50.0, 11.2, 5.7));

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Single",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet failed with single view: {:?}", res.err());
    let val = res.unwrap();
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 1);
    assert_eq!(vps[0]["view"], "v0");
}

#[test]
fn viewsheet_lays_out_three_views() {
    let mut s = metric_session();
    for i in 0..3 {
        s.doc_mut().unwrap().arch.views.push(make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7));
    }

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "ThreeViews",
            "views": ["v0", "v1", "v2"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet failed with three views: {:?}", res.err());
    let val = res.unwrap();
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 3);

    // v0 and v1 on first row, v2 on second row
    let y0 = vps[0]["center"][1].as_f64().unwrap();
    let y1 = vps[1]["center"][1].as_f64().unwrap();
    let y2 = vps[2]["center"][1].as_f64().unwrap();
    assert!((y0 - y1).abs() < 0.001, "v0 and v1 should be on the same row");
    assert!(y1 > y2, "v2 should be below v0 and v1");
}

#[test]
fn viewsheet_handles_different_scales() {
    let mut s = metric_session();
    s.doc_mut().unwrap().arch.views.push(make_view(0, "v0", 50.0, 11.2, 5.7));
    s.doc_mut().unwrap().arch.views.push(make_view(1, "v1", 100.0, 11.2, 5.7));

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "MixedScales",
            "views": ["v0", "v1"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet failed with mixed scales: {:?}", res.err());
    let val = res.unwrap();
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 2);
    assert_eq!(vps[0]["scale"], 20.0); // 1000/50
    assert_eq!(vps[1]["scale"], 10.0); // 1000/100
}

#[test]
fn viewsheet_refuses_a_view_without_generated_geometry() {
    let mut s = metric_session();
    let v = ArchView { id: ArchId(0), name: "v0".into(), scale: 50.0, ..Default::default() };
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "NoGeom",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse a view without generated geometry");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_more_than_sixteen_views() {
    let mut s = metric_session();
    for i in 0..17 {
        s.doc_mut().unwrap().arch.views.push(make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7));
    }

    let views: Vec<String> = (0..17).map(|i| format!("v{i}")).collect();
    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "TooMany",
            "views": views,
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse more than 16 views");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_takes_the_id_an_elevation_command_replies() {
    let mut s = metric_session();
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
fn viewsheet_refuses_when_views_do_not_fit_sheet_width() {
    let mut s = metric_session();
    // Create 4 very wide views that won't fit side by side on ARCH D
    for i in 0..4 {
        let v = make_view(i, &format!("v{i}"), 50.0, 50.0, 5.7);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "TooWide",
            "views": ["v0", "v1", "v2", "v3"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse when views don't fit width");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_when_views_do_not_fit_sheet_height() {
    let mut s = metric_session();
    // Create 4 very tall views that won't fit stacked on ARCH D
    for i in 0..4 {
        let v = make_view(i, &format!("v{i}"), 50.0, 11.2, 30.0);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "TooTall",
            "views": ["v0", "v1", "v2", "v3"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse when views don't fit height");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_when_layout_name_already_in_use() {
    let mut s = metric_session();
    s.doc_mut().unwrap().arch.views.push(make_view(0, "v0", 50.0, 11.2, 5.7));

    // Create a layout with the same name first
    s.execute("layout.new", &json!({"name": "Existing", "viewport": false})).unwrap();

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "Existing",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse duplicate layout name");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_works_in_portrait_orientation() {
    let mut s = metric_session();
    for i in 0..2 {
        let v = make_view(i, &format!("v{i}"), 50.0, 11.2, 5.7);
        s.doc_mut().unwrap().arch.views.push(v);
    }

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "PortraitSheet",
            "views": ["v0", "v1"],
            "paper": "ARCH D",
            "landscape": false
        }),
    );

    assert!(res.is_ok(), "viewsheet failed in portrait: {:?}", res.err());
    let val = res.unwrap();
    assert_eq!(val["landscape"], false);
    let vps = val["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 2);
}

#[test]
fn viewsheet_works_with_different_paper_size() {
    let mut s = metric_session();
    // Smaller view that fits on Letter (279.4 mm wide in landscape) at 1:50.
    // 5 m + 3 m growth = 8 m * 20 mm/m = 160 mm viewport + 50 mm margins = 210 mm < 279.4 mm.
    s.doc_mut().unwrap().arch.views.push(make_view(0, "v0", 50.0, 5.0, 3.0));

    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "LetterSheet",
            "views": ["v0"],
            "paper": "Letter",
            "landscape": true
        }),
    );

    assert!(res.is_ok(), "viewsheet failed with Letter paper: {:?}", res.err());
    let val = res.unwrap();
    assert_eq!(val["paper"], "Letter");
}

#[test]
fn viewsheet_refuses_view_with_zero_size_bounds() {
    let mut s = metric_session();
    // Create a view with zero extent
    let mut v = make_view(0, "v0", 50.0, 0.0, 0.0);
    v.generated.as_mut().unwrap().extent = [0.0, 0.0, 0.0, 0.0];
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "ZeroBounds",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse view with zero bounds");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_view_with_non_finite_scale() {
    let mut s = metric_session();
    let mut v = make_view(0, "v0", f64::NAN, 11.2, 5.7);
    v.scale = f64::NAN;
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "NanScale",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse a view with a non-finite scale");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_view_with_non_finite_bounds() {
    let mut s = metric_session();
    let mut v = make_view(0, "v0", 50.0, 11.2, 5.7);
    v.generated.as_mut().unwrap().extent = [f64::NAN, 0.0, 11.2, 5.7];
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "NanBounds",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse a view with non-finite bounds");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_view_with_zero_scale() {
    let mut s = metric_session();
    let mut v = make_view(0, "v0", 0.0, 11.2, 5.7);
    v.scale = 0.0;
    s.doc_mut().unwrap().arch.views.push(v);

    let layouts_before = s.doc().unwrap().layouts.len();
    let res = s.execute(
        "viewsheet",
        &json!({
            "name": "ZeroScale",
            "views": ["v0"],
            "paper": "ARCH D",
            "landscape": true
        }),
    );

    assert!(res.is_err(), "viewsheet should refuse a view with a zero scale");
    assert_eq!(s.doc().unwrap().layouts.len(), layouts_before);
}

#[test]
fn viewsheet_refuses_bad_input_and_changes_nothing() {
    let mut s = metric_session();

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
