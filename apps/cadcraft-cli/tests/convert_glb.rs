use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The built `cadcraft-cli` binary. Under `cargo test` the integration test executable
/// lives in `<root>/target/<profile>/deps/<hash>`; the CLI binary sits at
/// `<root>/target/<profile>/cadcraft-cli`.
fn binary() -> PathBuf {
    match std::env::current_exe() {
        Ok(exe) => {
            let base = exe.parent().and_then(Path::parent).and_then(Path::parent);
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

/// A temp directory unique to this test (no cross-test clobbering).
fn temp_dir() -> PathBuf {
    let base = std::env::temp_dir().join(format!("cadcraft-cli-{}", std::process::id()));
    let dir = base.join(format!("{:08}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn convert_writes_a_glb_from_a_wall_plan() {
    let dir = temp_dir();
    let in_dxf = dir.join("floorplan.dxf");
    let out_glb = dir.join("house.glb");

    // `sample floorplan` writes a floor plan (walls) to disk; that is our "wall plan".
    assert!(run_cli(&["sample".into(), "floorplan".into(), in_dxf.to_string_lossy().into()]).status.success());
    assert!(in_dxf.exists(), "floorplan.dxf not written");

    // Convert the wall plan to a GLB and confirm a valid GLB container came out.
    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), out_glb.to_string_lossy().into()]).status.success());
    assert!(is_glb(&out_glb), "convert did not write a valid GLB: {}", out_glb.to_string_lossy());
    assert!(fs::metadata(&out_glb).unwrap().len() > 0, "{:?}", out_glb);
}

#[test]
fn run_export_glb_matches_convert() {
    let dir = temp_dir();
    let in_dxf = dir.join("floorplan.dxf");
    let glb_convert = dir.join("via_convert.glb");
    let glb_run = dir.join("via_run.glb");

    assert!(run_cli(&["sample".into(), "floorplan".into(), in_dxf.to_string_lossy().into()]).status.success());
    assert!(in_dxf.exists(), "floorplan.dxf not written");

    // Both paths open the same drawing and build the same scene.
    assert!(run_cli(&["convert".into(), in_dxf.to_string_lossy().into(), glb_convert.to_string_lossy().into()]).status.success());
    assert!(run_cli(&["run".into(), in_dxf.to_string_lossy().into(), "--export".into(), glb_run.to_string_lossy().into()]).status.success());

    assert!(is_glb(&glb_convert), "convert GLB invalid: {}", glb_convert.to_string_lossy());
    assert!(is_glb(&glb_run), "run GLB invalid: {}", glb_run.to_string_lossy());

    // Deterministic build -> byte-for-byte identical containers.
    assert_eq!(fs::read(glb_convert).unwrap(), fs::read(glb_run).unwrap(), "convert and run --export must produce identical GLB bytes");
}

#[test]
fn glb_conversion_works_on_a_dxf_reopened_from_disk() {
    let dir = temp_dir();
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
