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

/// True when `path` is a valid PNG (magic bytes `89 50 4E 47`).
fn is_png(path: &Path) -> bool {
    match fs::read(path) {
        Ok(bytes) => bytes.len() >= 4 && bytes[0..4] == [0x89, 0x50, 0x4e, 0x47],
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

/// `sample floorplan` -> the wall plan the render tests draw on.
fn floorplan(dir: &PathBuf) -> PathBuf {
    let in_dxf = dir.join("floorplan.dxf");
    assert!(run_cli(&["sample".into(), "floorplan".into(), in_dxf.to_string_lossy().into()]).status.success());
    assert!(in_dxf.exists(), "floorplan.dxf not written");
    in_dxf
}

#[test]
fn render_writes_a_png() {
    let dir = temp_dir();
    let in_dxf = floorplan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit".into()]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        // Adapter available: a real PNG must be written, and both the rendered-bounds and
        // "wrote" lines are reported to stderr.
        assert!(is_png(&out_png), "render did not write a valid PNG: {}", out_png.display());
        assert!(fs::metadata(&out_png).unwrap().len() > 8, "{:?}", out_png);
        assert!(stderr.contains("PROJECTED "), "render did not print the PROJECTED bounds to stderr: {stderr}");
        assert!(stderr.contains("wrote "), "render did not print the wrote line to stderr: {stderr}");
    } else {
        // No adapter (headless CI): exit 3 with the NO_ADAPTER message on stderr.
        assert_eq!(out.status.code(), Some(3), "no-adapter must exit 3, stderr: {stderr}");
        assert!(stderr.contains("NO_ADAPTER"), "no-adapter case did not report 'NO_ADAPTER': {stderr}");
    }
}

#[test]
fn render_writes_a_png_or_skips_without_an_adapter() {
    let dir = temp_dir();
    let in_dxf = floorplan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit".into()]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        // Adapter available: a real PNG must be written, and both the rendered-bounds and
        // "wrote" lines are reported to stderr.
        assert!(is_png(&out_png), "render did not write a valid PNG: {}", out_png.display());
        assert!(fs::metadata(&out_png).unwrap().len() > 8, "{:?}", out_png);
        assert!(stderr.contains("PROJECTED "), "render did not print the PROJECTED bounds to stderr: {stderr}");
        assert!(stderr.contains("wrote "), "render did not print the wrote line to stderr: {stderr}");
    } else {
        // No adapter (headless CI): exit 3 with the NO_ADAPTER message on stderr.
        assert_eq!(out.status.code(), Some(3), "no-adapter must exit 3, stderr: {stderr}");
        assert!(stderr.contains("NO_ADAPTER"), "no-adapter case did not report 'NO_ADAPTER': {stderr}");
    }
}

#[test]
fn render_rejects_a_malformed_camera() {
    let dir = temp_dir();
    let in_dxf = floorplan(&dir);
    let out_png = dir.join("view.png");

    for camera in ["orbit,azimuth=NaN", "orbit,elevation=inf", "orbit,distance=-1", "orbit,fov=abc", "not-a-camera,fov=1"] {
        let out =
            run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), camera.to_string()]);
        assert!(!out.status.success(), "malformed camera `{camera}` should be rejected: stderr={}", String::from_utf8_lossy(&out.stderr));
        assert!(!out_png.exists(), "nothing should be written for malformed camera `{camera}`");
    }
}
