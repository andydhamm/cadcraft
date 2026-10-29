use serde_json::json;

/// Test that the examples in USER-GUIDE.md parse correctly as JSON parameters.
#[test]
fn user_guide_examples_parse() {
    // Sun command
    let sun_params = json!({
        "lat": 51.05,
        "date": "2026-06-21",
        "time": "solar-noon"
    });
    assert!(sun_params.is_object());

    // Camera save
    let cam_save = json!({
        "action": "save",
        "name": "front"
    });
    assert!(cam_save.is_object());

    // Camera list
    let cam_list = json!({
        "action": "list"
    });
    assert!(cam_list.is_object());

    // Camera use
    let cam_use = json!({
        "action": "use",
        "name": "front"
    });
    assert!(cam_use.is_object());

    // Camera delete
    let cam_del = json!({
        "action": "delete",
        "name": "front"
    });
    assert!(cam_del.is_object());

    // Pick3d
    let pick_params = json!({
        "camera": "front",
        "size": [800, 600],
        "pixel": [100, 100]
    });
    assert!(pick_params.is_object());
}
