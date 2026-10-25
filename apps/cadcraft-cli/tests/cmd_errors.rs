//! `--cmd` failures are reported on stderr and do not abort the run, except when the *last*
//! command fails (which sets the exit status) or the failure comes from the `--script` path
//! (which keeps its own message and is still fatal).

use std::process::{Command, Output};

/// Run the binary with the given args, capturing stdout and stderr.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cadcraft-cli"))
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("run cadcraft-cli")
}

/// The last JSON line printed on stdout (command results are one JSON value per line).
fn last_json(stdout: &str) -> serde_json::Value {
    let line = stdout
        .lines()
        .rev()
        .find(|l| {
            let t = l.trim_start();
            t.starts_with('{') || t.starts_with('[')
        })
        .unwrap_or_else(|| panic!("no JSON result in {stdout:?}"));
    serde_json::from_str(line).expect("command result is JSON")
}

/// A failed `--cmd` is reported on stderr as `error: <cmd>:` and does not stop the run.
#[test]
fn cmd_error_is_reported_on_stderr() {
    let out = run(&["run", "--metric", "--cmd", "no.such.command {}", "--cmd", "drawing.inspect {}"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {:?}\nstdout: {stdout}\nstderr: {stderr}", out.status);
    assert!(stderr.lines().any(|l| l.starts_with("error: no.such.command:")), "stderr: {stderr}");
    assert!(stdout.contains("drawing.inspect"), "stdout: {stdout}");
    assert!(!stdout.contains("no.such.command"), "stdout: {stdout}");
}

/// A failed `--cmd` in the middle of the run does not abort it.
#[test]
fn cmd_error_does_not_abort_the_run() {
    let out = run(&[
        "run",
        "--metric",
        "--cmd",
        "no.such.command {}",
        "--cmd",
        r#"line {"points":[[0,0],[10,0]]}"#,
        "--cmd",
        r#"entities {"type":"LINE"}"#,
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "exit {:?}\nstdout: {stdout}", out.status);
    let r = last_json(&stdout);
    assert_eq!(r["count"].as_u64(), Some(1), "stdout: {stdout}");
}

/// A `--cmd` with invalid JSON is a parse error, reported on stderr, and does not abort the run.
#[test]
fn cmd_json_parse_error_does_not_abort_the_run() {
    let out = run(&[
        "run",
        "--metric",
        "--cmd",
        "line {\"points\": [[",
        "--cmd",
        r#"line {"points":[[0,0],[5,0]]}"#,
        "--cmd",
        r#"entities {"type":"LINE"}"#,
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {:?}\nstdout: {stdout}\nstderr: {stderr}", out.status);
    assert!(stderr.lines().any(|l| l.starts_with("error: line:")), "stderr: {stderr}");
    let r = last_json(&stdout);
    assert_eq!(r["count"].as_u64(), Some(1), "stdout: {stdout}");
}

/// The phase-3 hostile shape: an earlier command fails, but a good last step still exits 0.
#[test]
fn earlier_cmd_error_with_a_good_last_step_exits_0() {
    let out = run(&["run", "--metric", "--cmd", r#"room {"points":[[0,0],[1,0],[1,1]]}"#, "--cmd", r#"arch.inspect {"rooms":true}"#]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "exit {:?}\nstdout: {stdout}", out.status);
    let r = last_json(&stdout);
    assert!(r["rooms"].is_array(), "stdout: {stdout}");
    assert_eq!(r["rooms"].as_array().map(|a| a.len()), Some(0), "stdout: {stdout}");
}

/// When the *last* `--cmd` fails, the run exits 1 with an `error:` line.
#[test]
fn last_cmd_error_sets_the_exit_status() {
    let out = run(&["run", "--metric", "--cmd", r#"line {"points":[[0,0],[1,0]]}"#, "--cmd", r#"door {"wall":"w0","offset":5.0,"width":12.0}"#]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr: {stderr}");
    assert!(stderr.lines().any(|l| l.starts_with("error: door:")), "stderr: {stderr}");

    // A failed command followed by a good one exits 0.
    let out2 = run(&["run", "--metric", "--cmd", r#"door {"wall":"w0","offset":5.0,"width":12.0}"#, "--cmd", "drawing.inspect {}"]);
    assert!(out2.status.success(), "exit {:?}", out2.status);
}

/// A failed `--cmd` does not clear the remembered `{last}` handle.
#[test]
fn failed_cmd_leaves_last_handle_untouched() {
    let out = run(&[
        "run",
        "--metric",
        "--cmd",
        r#"line {"points":[[0,0],[10,0]]}"#,
        "--cmd",
        "no.such.command {}",
        "--cmd",
        r#"erase {"handles":["{last}"]}"#,
        "--cmd",
        r#"entities {"type":"LINE"}"#,
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "exit {:?}\nstdout: {stdout}", out.status);
    let r = last_json(&stdout);
    assert_eq!(r["count"].as_u64(), Some(0), "stdout: {stdout}");
}

/// A failed `--script` is still fatal, but its message is not prefixed with `error:`.
#[test]
fn script_failure_is_still_fatal() {
    let out = run(&["run", "--metric", "--script", "no.such.command"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr: {stderr}");
    assert!(!stderr.starts_with("error:"), "stderr: {stderr}");
}
