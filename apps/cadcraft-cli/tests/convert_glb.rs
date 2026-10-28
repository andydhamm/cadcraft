use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// The built `cadcraft-cli` binary. Under `cargo test` the integration test executable
/// lives in `<root>/target/<profile>/deps/<hash>`; the CLI binary sits at
/// `<root>/target/<profile>/cadcraft-cli`.
fn binary() -> PathBuf {
    match std::env::current_exe() {
        Ok(exe) => {
            let base = exe.parent().and_then(Path::parent);
            base.map_or_else(|| PathBuf::from("target/debug/cadcraft-cli"), |b| b.join("cadcraft-cli"))
        }
        Err(_) => PathBuf::from("target/debug/cadcraft-cli"),
    }
}

/// Run the CLI binary with the given arguments and return its full output.
fn run_cli(args: &[String]) -> Output {
    let mut cmd = Command::new(binary());
    for a in args {
        cmd.arg(a);
    }
    cmd.output().expect("run cadcraft-cli")
}

/// True when `path` is a valid binary GLB (glTF container magic `glTF`), regardless of content.
fn is_glb(path: &PathBuf) -> bool {
    match fs::read(path) {
        Ok(bytes) => bytes.len() >= 4 && bytes[..4] == *b"glTF",
        Err(_) => false,
    }
}

/// A temp directory unique to this test process and test name (no cross-test clobbering).
fn temp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cadcraft-cli-{}-{}", std::process::id(), test_name));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Build a DXF with 5 arch walls (a closed rectangle of 4 walls + 1 interior partition)
/// by running `cadcraft-cli run --metric --cmd 'wall ...' --save walls.dxf`.
/// Build an empty DXF (no walls, no linework) by running `cadcraft-cli run --save`.
fn empty_plan(dir: &Path) -> PathBuf {
    let out_dxf = dir.join("empty.dxf");
    let out = run_cli(&["run".into(), "--metric".into(), "--save".into(), out_dxf.to_string_lossy().into()]);
    assert!(out.status.success(), "empty_plan failed: {}", String::from_utf8_lossy(&out.stderr));
    out_dxf
}

fn wall_plan(dir: &Path) -> PathBuf {
    let out_dxf = dir.join("walls.dxf");
    let out = run_cli(&[
        "run".into(),
        "--metric".into(),
        "--cmd".into(),
        r#"wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4}"#.into(),
        "--cmd".into(),
        r#"wall {"points":[[0,3],[10,3]],"thickness":0.1}"#.into(),
        "--save".into(),
        out_dxf.to_string_lossy().into(),
    ]);
    assert!(out.status.success(), "wall_plan failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(out_dxf.exists(), "walls.dxf not written");
    out_dxf
}

/// Parse the glTF JSON chunk from a GLB file and return it as a `Value`.
///
/// GLB layout: 12-byte header | 4-byte JSON-chunk-length | 4-byte type "JSON" | JSON bytes (padded with 0x20).
fn glb_json(path: &Path) -> Value {
    let bytes = fs::read(path).expect("read GLB");
    assert!(bytes.len() >= 20, "GLB too small to contain a JSON chunk: {} bytes", bytes.len());
    let chunk_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    assert!(bytes.len() >= 20 + chunk_len, "GLB truncated before JSON chunk end: file {} bytes, chunk needs {}", bytes.len(), 20 + chunk_len);
    serde_json::from_slice(&bytes[20..20 + chunk_len]).expect("parse GLB JSON chunk")
}

/// Return the names of all `wall-w*` meshes found in a GLB's JSON chunk.
fn wall_mesh_names(json: &Value) -> Vec<String> {
    json["meshes"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|m| m["name"].as_str()).filter(|n| n.starts_with("wall-w")).map(|n| n.to_string()).collect())
        .unwrap_or_default()
}

#[test]
fn convert_writes_a_glb_from_a_wall_plan() {
    let dir = temp_dir("convert_writes_a_glb_from_a_wall_plan");
    let in_dxf = wall_plan(&dir);
    let out_glb = dir.join("house.glb");

    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "convert did not write a valid GLB: {}", out_glb.to_string_lossy());

    let json = glb_json(&out_glb);
    let names = wall_mesh_names(&json);
    assert_eq!(names.len(), 5, "expected 5 wall meshes named wall-w*, got {}; names: {:?}", names.len(), names);
    for name in &names {
        assert!(name.starts_with("wall-w"), "mesh name {name:?} does not start with wall-w");
    }
}

#[test]
fn run_export_glb_matches_convert() {
    let dir = temp_dir("run_export_glb_matches_convert");
    let in_dxf = wall_plan(&dir);
    let glb_convert = dir.join("via_convert.glb");
    let glb_run = dir.join("via_run.glb");

    // Both paths open the same drawing and build the same scene.
    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), glb_convert.to_string_lossy().into()]).status.success());
    assert!(run_cli(&["run".into(), in_dxf.to_string_lossy().into(), "--export".into(), glb_run.to_string_lossy().into()]).status.success());

    assert!(is_glb(&glb_convert), "convert GLB invalid: {}", glb_convert.to_string_lossy());
    assert!(is_glb(&glb_run), "run GLB invalid: {}", glb_run.to_string_lossy());

    // Both GLBs must contain exactly 5 wall meshes.
    for (label, path) in [("convert", &glb_convert), ("run --export", &glb_run)] {
        let json = glb_json(path);
        let names = wall_mesh_names(&json);
        assert_eq!(names.len(), 5, "{label}: expected 5 wall meshes named wall-w*, got {}; names: {:?}", names.len(), names);
    }

    // Deterministic build -> byte-for-byte identical containers.
    assert_eq!(fs::read(&glb_convert).unwrap(), fs::read(&glb_run).unwrap(), "convert and run --export must produce identical GLB bytes");
}

#[test]
fn glb_conversion_works_on_a_dxf_reopened_from_disk() {
    let dir = temp_dir("glb_conversion_works_on_a_dxf_reopened_from_disk");
    let on_disk = wall_plan(&dir);
    let out_glb = dir.join("saved.glb");

    // Reopen the on-disk DXF and convert it; the GLB must be a valid container.
    assert!(run_cli(&["convert".into(), on_disk.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "reopened-from-disk GLB invalid: {}", out_glb.to_string_lossy());

    // The round-tripped file is non-empty and still a GLB.
    let bytes = fs::read(out_glb).unwrap();
    assert!(bytes.len() > 16, "GLB is too small to hold a scene: {} bytes", bytes.len());
}

#[test]
fn convert_empty_plan_produces_valid_glb() {
    let dir = temp_dir("convert_empty_plan_produces_valid_glb");
    let in_dxf = empty_plan(&dir);
    let out_glb = dir.join("empty.glb");

    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "convert did not write a valid GLB for empty plan");

    let json = glb_json(&out_glb);
    let names = wall_mesh_names(&json);
    assert_eq!(names.len(), 0, "expected 0 wall meshes for empty plan, got {}", names.len());
}

#[test]
fn convert_single_wall_produces_one_mesh() {
    let dir = temp_dir("convert_single_wall_produces_one_mesh");
    let out_dxf = dir.join("single.dxf");
    let out = run_cli(&[
        "run".into(),
        "--metric".into(),
        "--cmd".into(),
        r#"wall {"points":[[0,0],[10,0]],"thickness":0.2,"height":2.4}"#.into(),
        "--save".into(),
        out_dxf.to_string_lossy().into(),
    ]);
    assert!(out.status.success(), "single wall plan failed: {}", String::from_utf8_lossy(&out.stderr));

    let out_glb = dir.join("single.glb");
    assert!(run_cli(&["convert".into(), out_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "convert did not write a valid GLB for single wall");

    let json = glb_json(&out_glb);
    let names = wall_mesh_names(&json);
    assert_eq!(names.len(), 1, "expected 1 wall mesh, got {}", names.len());
}

#[test]
fn convert_rejects_missing_input() {
    let dir = temp_dir("convert_rejects_missing_input");
    let missing = dir.join("does_not_exist.dxf");
    let out_glb = dir.join("out.glb");

    let out = run_cli(&["convert".into(), missing.to_string_lossy().into(), out_glb.to_string_lossy().into()]);
    assert!(!out.status.success(), "convert should fail for missing input");
    assert!(!out_glb.exists(), "no output should be written for missing input");
}

#[test]
fn wall_mesh_names_are_unique() {
    let dir = temp_dir("wall_mesh_names_are_unique");
    let in_dxf = wall_plan(&dir);
    let out_glb = dir.join("house.glb");

    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());

    let json = glb_json(&out_glb);
    let names = wall_mesh_names(&json);
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "wall mesh names must be unique: {:?}", names);
}

#[test]
fn run_export_rejects_missing_input() {
    let dir = temp_dir("run_export_rejects_missing_input");
    let missing = dir.join("missing.dxf");
    let out_glb = dir.join("out.glb");

    let out = run_cli(&["run".into(), missing.to_string_lossy().into(), "--export".into(), out_glb.to_string_lossy().into()]);
    assert!(!out.status.success(), "run --export should fail for missing input");
    assert!(!out_glb.exists(), "no output should be written for missing input");
}

#[test]
fn convert_rejects_non_dxf_input() {
    let dir = temp_dir("convert_rejects_non_dxf_input");
    let not_dxf = dir.join("not_a_dxf.txt");
    fs::write(&not_dxf, "this is not a DXF file").expect("write non-dxf file");
    let out_glb = dir.join("out.glb");

    let out = run_cli(&["convert".into(), not_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]);
    assert!(!out.status.success(), "convert should fail for non-DXF input");
    assert!(!out_glb.exists(), "no output should be written for non-DXF input");
}

#[test]
fn run_export_without_input_file() {
    let dir = temp_dir("run_export_without_input_file");
    let out_glb = dir.join("out.glb");

    let out = run_cli(&["run".into(), "--export".into(), out_glb.to_string_lossy().into()]);
    assert!(!out.status.success(), "run --export without input should fail");
    assert!(!out_glb.exists(), "no output should be written without input");
}
