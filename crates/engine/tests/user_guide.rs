use std::fs;
use std::path::Path;

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};

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

/// The guide as compiled into the test binary, so a change to it rebuilds and reruns these tests.
const GUIDE: &str = include_str!("../../../docs/arch3d/USER-GUIDE.md");

/// The ```sh blocks of the guide (as acceptance check 02 finds them), each as logical lines: a line ending in `\`
/// continues on the next.
fn sh_blocks(guide: &str) -> Vec<Vec<String>> {
    let mut blocks = Vec::new();
    let mut rest = guide;
    while let Some(start) = rest.find("```sh\n") {
        let body = &rest[start + "```sh\n".len()..];
        let Some(end) = body.find("```") else {
            panic!("the user guide has an unterminated ```sh block");
        };
        let mut lines = Vec::new();
        let mut line = String::new();
        for part in body[..end].lines() {
            match part.strip_suffix('\\') {
                Some(head) => {
                    line.push_str(head);
                    line.push(' ');
                }
                None => {
                    line.push_str(part);
                    lines.push(std::mem::take(&mut line));
                }
            }
        }
        if !line.trim().is_empty() {
            lines.push(line);
        }
        blocks.push(lines);
        rest = &body[end + 3..];
    }
    blocks
}

/// The `--cmd '<id> <json>'` arguments of one logical line, in order.
fn cmd_args(line: &str) -> Vec<String> {
    let mut cmds = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("--cmd ") {
        let arg = rest[at + "--cmd ".len()..].trim_start();
        let Some(body) = arg.strip_prefix('\'') else {
            panic!("a user-guide --cmd must be single-quoted (`--cmd '<id> <json>'`): {line}");
        };
        let Some(end) = body.find('\'') else {
            panic!("unterminated --cmd quote in the user guide: {line}");
        };
        cmds.push(body[..end].to_string());
        rest = &body[end + 1..];
    }
    cmds
}

/// The CLI installs the real PDF plotter; here a stand-in answers so `exportpdf` still checks its layout and params.
fn stand_in_plot(_: &cadcraft_doc::Drawing, _: &cadcraft_doc::Space, _: &serde_json::Value) -> Result<Vec<u8>, String> {
    Ok(b"%PDF-1.4 user guide".to_vec())
}

/// Every `run` line of every ```sh block executes through the engine as the CLI runs it: a fresh session per line,
/// a new metric drawing for `--metric`, then each `--cmd` in order. `render` and `convert` lines need a GPU adapter
/// or a saved file and are left to acceptance check 02 (which also plots the real PDF). A relative `path` (exportpdf)
/// is written to a temp dir.
#[test]
fn user_guide_sh_blocks_execute_through_the_engine() {
    set_io(IoHooks {
        read: |_, _| Err("no reader in this test".into()),
        write: |_, _| Err("no writer in this test".into()),
        plot: Some(stand_in_plot),
    });
    let out_dir = std::env::temp_dir().join(format!("cadcraft-user-guide-{}", std::process::id()));
    if let Err(error) = fs::create_dir_all(&out_dir) {
        panic!("cannot create {}: {error}", out_dir.display());
    }
    let blocks = sh_blocks(GUIDE);
    let (mut runs, mut ids) = (0usize, Vec::new());
    for (b, lines) in blocks.iter().enumerate() {
        for line in lines {
            if !line.contains("cadcraft-cli --") {
                continue;
            }
            if line.contains("-- render ") || line.contains("-- convert ") {
                continue;
            }
            assert!(line.contains("-- run --metric"), "sh block {}: every `run` line starts a new metric drawing: {line}", b + 1);
            let cmds = cmd_args(line);
            assert_eq!(cmds.len(), line.matches("--cmd ").count(), "sh block {}: every --cmd is read: {line}", b + 1);
            let mut session = Session::empty();
            session.new_drawing(true);
            for command in &cmds {
                let Some((id, json)) = command.split_once(' ') else {
                    panic!("sh block {}: user-guide command `{command}` should have the form `<id> <json>`", b + 1);
                };
                let mut params: serde_json::Value = match serde_json::from_str(json) {
                    Ok(params) => params,
                    Err(error) => panic!("sh block {}: user-guide command `{command}` has invalid JSON: {error}", b + 1),
                };
                if let Some(path) = params.get("path").and_then(serde_json::Value::as_str) {
                    if Path::new(path).is_relative() {
                        params["path"] = serde_json::Value::String(out_dir.join(path).to_string_lossy().into_owned());
                    }
                }
                if let Err(error) = session.execute(id, &params) {
                    panic!("sh block {}: user-guide command `{command}` failed: {error:?}", b + 1);
                }
                ids.push(id.to_string());
            }
            runs += 1;
        }
    }
    let _ = fs::remove_dir_all(&out_dir);
    assert!(runs >= 7, "the guide's sh blocks should hold at least 7 `run` examples, found {runs}");
    for id in ["sun", "camera", "walkthrough", "pick3d", "elevation", "section", "view.delete", "viewsheet", "exportpdf"] {
        assert!(ids.iter().any(|x| x == id), "no sh block in the user guide runs `{id}`");
    }
}

/// The guide is titled for arch3d (not "Phase 6"), puts 51.05 N in Calgary (London is 51.5 N) and uses the real
/// CLI syntax, not an invented `--command ... --params` form.
#[test]
fn user_guide_title_and_latitude_are_right() {
    let title = GUIDE.trim_start().lines().next().unwrap_or_default();
    assert_eq!(title, "# arch3d user guide", "the user guide's title");
    for line in GUIDE.lines() {
        assert!(
            !(line.contains("51.05") && line.to_lowercase().contains("london")),
            "51.05 N is Calgary, not London: {line}"
        );
    }
    assert!(GUIDE.contains("Calgary, 51.05"), "the guide names Calgary, 51.05° N, as the examples' latitude");
    assert!(!GUIDE.contains("--command") && !GUIDE.contains("--params"), "the guide uses an invented `--command ... --params` syntax");
}
