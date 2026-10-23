use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// A temp directory unique to this test (no cross-test clobbering): the tests run in
/// parallel in one process, so the process id and a timestamp alone are shared by all of them.
fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cadcraft-cli-{}", std::process::id())).join(test);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A DXF holding arch walls: the 10×8 m rectangle plus one interior wall (5 walls).
/// (`sample floorplan` is plain LINE work with no arch model, so a GLB made from it is an
/// empty scene and proves nothing about walls.)
fn wall_plan(dir: &Path) -> PathBuf {
    let dxf = dir.join("walls.dxf");
    let out = run_cli(&[
        "run".into(),
        "--metric".into(),
        "--cmd".into(),
        r#"wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4}"#.into(),
        "--cmd".into(),
        r#"wall {"points":[[0,3],[10,3]],"thickness":0.1}"#.into(),
        "--save".into(),
        dxf.to_string_lossy().into(),
    ]);
    assert!(out.status.success(), "wall plan: {}", String::from_utf8_lossy(&out.stderr));
    dxf
}

/// The JSON chunk of a GLB file.
fn glb_json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).unwrap();
    let len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}

#[test]
fn convert_writes_a_glb_from_a_wall_plan() {
    let dir = temp_dir("convert");
    let in_dxf = wall_plan(&dir);
    let out_glb = dir.join("house.glb");

    // Convert the wall plan to a GLB and confirm a valid GLB container came out.
    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "convert did not write a valid GLB: {}", out_glb.to_string_lossy());
    // One named mesh per wall, each with triangles.
    let gl = glb_json(&out_glb);
    let meshes = gl["meshes"].as_array().cloned().unwrap_or_default();
    assert_eq!(meshes.len(), 5, "expected one mesh per wall: {}", gl["meshes"]);
    assert!(meshes.iter().all(|m| m["name"].as_str().is_some_and(|n| n.starts_with("wall-w"))), "{}", gl["meshes"]);
}

#[test]
fn run_export_glb_matches_convert() {
    let dir = temp_dir("run-export");
    let in_dxf = wall_plan(&dir);
    let glb_convert = dir.join("via_convert.glb");
    let glb_run = dir.join("via_run.glb");

    // Both paths open the same drawing and build the same scene.
    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), glb_convert.to_string_lossy().into()]).status.success());
    assert!(run_cli(&["run".into(), in_dxf.to_string_lossy().into(), "--export".into(), glb_run.to_string_lossy().into()]).status.success());

    assert!(is_glb(&glb_convert), "convert GLB invalid: {}", glb_convert.to_string_lossy());
    assert!(is_glb(&glb_run), "run GLB invalid: {}", glb_run.to_string_lossy());

    // Deterministic build -> byte-for-byte identical containers (of a non-empty scene).
    assert_eq!(glb_json(&glb_run)["meshes"].as_array().map_or(0, Vec::len), 5);
    assert_eq!(fs::read(glb_convert).unwrap(), fs::read(glb_run).unwrap(), "convert and run --export must produce identical GLB bytes");
}

#[test]
fn glb_conversion_works_on_a_dxf_reopened_from_disk() {
    let dir = temp_dir("reopened");
    let on_disk = dir.join("saved.dxf");
    let out_glb = dir.join("saved.glb");

    // A DXF that was *written* to disk (not a fresh in-memory drawing).
    assert!(run_cli(&["sample".into(), "floorplan".into(), on_disk.to_string_lossy().into()]).status.success());
    assert!(on_disk.exists(), "saved.dxf not written");

    // Reopen that on-disk DXF and convert it; the GLB must be a valid container.
    assert!(run_cli(&["convert".into(), on_disk.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "reopened-from-disk GLB invalid: {}", out_glb.to_string_lossy());

    // The round-tripped file is non-empty and still a GLB.
    let bytes = fs::read(out_glb).unwrap();
    assert!(bytes.len() > 16, "GLB is too small to hold a scene: {} bytes", bytes.len());
}
