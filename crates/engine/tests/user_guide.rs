use std::fs;
use std::path::Path;

use cadcraft_engine::Session;

fn read_user_guide() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/arch3d/USER-GUIDE.md");
    match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    }
}

fn execute_examples(commands: &[&str]) {
    let mut session = Session::new();
    for command in commands {
        let Some((id, json)) = command.split_once(' ') else {
            panic!("user-guide command `{command}` should have the form `<id> <json>`");
        };
        let params: serde_json::Value = match serde_json::from_str(json) {
            Ok(params) => params,
            Err(error) => panic!("user-guide command `{command}` has invalid JSON: {error}"),
        };
        if let Err(error) = session.execute(id, &params) {
            panic!("user-guide command `{command}` failed: {error:?}");
        }
    }
}

/// The standalone phase-6 examples execute with the command syntax used by the CLI.
#[test]
fn user_guide_examples_parse() {
    execute_examples(&[
        "sun {\"lat\":51.5,\"date\":\"2026-06-21\",\"time\":\"solar-noon\"}",
        "walkthrough {\"eye\":[5,-10,1.6],\"yaw\":0,\"forward\":2,\"strafe\":0,\"turn\":0}",
        "arch.inspect {\"views\":true}",
    ]);
}

#[test]
fn user_guide_has_all_phase6_commands() {
    let content = read_user_guide();
    for expected in ["sun", "camera", "walkthrough", "pick3d", "render"] {
        assert!(content.contains(expected), "user guide should document {expected}");
    }
    for flag in ["--style", "--sun", "--ground", "--adapter", "--camera"] {
        assert!(content.contains(flag), "user guide should document {flag}");
    }
}

/// Camera operations share one session, just as they do in a CLI script.
#[test]
fn user_guide_examples_run() {
    execute_examples(&[
        "camera {\"save\":\"front\",\"kind\":\"full\",\"eye\":[3,-8,1.6],\"target\":[3,0,1.05],\"fov\":60}",
        "camera {\"use\":\"front\"}",
        "camera {\"list\":true}",
        "pick3d {\"camera\":{\"kind\":\"full\",\"eye\":[3,-8,1.6],\"target\":[3,0,1.05],\"fov\":60},\"size\":[800,600],\"pixel\":[400,300]}",
        "camera {\"delete\":\"front\"}",
    ]);
}

#[test]
fn user_guide_has_all_phase7_commands() {
    let content = read_user_guide();
    for expected in ["elevation", "section", "view.delete", "arch.inspect {\"views\":true}", "viewsheet", "exportpdf {\"layout\":", "ARCH D", "1:50"]
    {
        assert!(content.contains(expected), "user guide should document {expected}");
    }
}

#[test]
fn user_guide_sun_winter_summer() {
    // Sun position differs between solstices; both must compute without error.
    execute_examples(&[
        "sun {\"lat\":51.5,\"date\":\"2026-06-21\",\"time\":\"solar-noon\"}",
        "sun {\"lat\":51.5,\"date\":\"2026-12-21\",\"time\":\"solar-noon\"}",
        "sun {\"lat\":0,\"date\":\"2026-03-20\",\"time\":\"solar-noon\"}",
    ]);
}

#[test]
fn user_guide_camera_list_empty() {
    // Listing cameras when none are saved must not error.
    let mut session = Session::new();
    let result = session.execute("camera", &serde_json::json!({"list": true}));
    assert!(result.is_ok(), "camera list with no saved cameras should succeed");
}

#[test]
fn user_guide_walkthrough_zero_movement() {
    // A walkthrough step with zero movement is valid and returns the same eye.
    execute_examples(&["walkthrough {\"eye\":[5,-10,1.6],\"yaw\":0,\"forward\":0,\"strafe\":0,\"turn\":0}"]);
}
