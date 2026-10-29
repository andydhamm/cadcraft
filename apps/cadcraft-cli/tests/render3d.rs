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

#[test]
fn render_writes_a_png() {
    let dir = temp_dir("render_writes_a_png");
    let in_dxf = wall_plan(&dir);
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
    let dir = temp_dir("render_writes_a_png_or_skips_without_an_adapter");
    let in_dxf = wall_plan(&dir);
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
    let dir = temp_dir("render_rejects_a_malformed_camera");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");

    for camera in ["orbit,azimuth=NaN", "orbit,elevation=inf", "orbit,distance=-1", "orbit,fov=abc", "not-a-camera,fov=1"] {
        let out =
            run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), camera.to_string()]);
        assert!(!out.status.success(), "malformed camera `{camera}` should be rejected: stderr={}", String::from_utf8_lossy(&out.stderr));
        assert!(!out_png.exists(), "nothing should be written for malformed camera `{camera}`");
    }
}

#[test]
fn render_rejects_missing_input() {
    let dir = temp_dir("render_rejects_missing_input");
    let missing = dir.join("does_not_exist.dxf");
    let out_png = dir.join("view.png");

    let out = run_cli(&["render".into(), missing.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit".into()]);
    assert!(!out.status.success(), "render should fail for missing input");
    assert!(!out_png.exists(), "no output should be written for missing input");
}

#[test]
fn render_empty_plan_succeeds_or_skips() {
    let dir = temp_dir("render_empty_plan_succeeds_or_skips");
    let in_dxf = empty_plan(&dir);
    let out_png = dir.join("empty.png");

    let out = run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit".into()]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        assert!(is_png(&out_png), "render did not write a valid PNG for empty plan");
    } else {
        assert_eq!(out.status.code(), Some(3), "no-adapter must exit 3");
        assert!(stderr.contains("NO_ADAPTER"));
    }
}

#[test]
fn render_projected_bounds_are_finite() {
    let dir = temp_dir("render_projected_bounds_are_finite");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit".into()]);
    if !out.status.success() {
        // No adapter; nothing to check.
        return;
    }

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let line = stderr.lines().find(|l| l.starts_with("PROJECTED ")).expect("PROJECTED line");
    let parts: Vec<&str> = line.strip_prefix("PROJECTED ").unwrap().trim().split(',').collect();
    assert_eq!(parts.len(), 4, "PROJECTED must have 4 components: {}", line);
    for p in parts {
        let v: f64 = p.trim().parse().expect("PROJECTED component must be a number");
        assert!(v.is_finite(), "PROJECTED component must be finite: {}", v);
    }
}

#[test]
fn render_with_explicit_camera_params() {
    let dir = temp_dir("render_with_explicit_camera_params");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&[
        "render".into(),
        in_dxf.to_string_lossy().into(),
        out_png.to_string_lossy().into(),
        "--camera".into(),
        "orbit,azimuth=45,elevation=30,distance=20,fov=60".into(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        assert!(is_png(&out_png), "render did not write a valid PNG with explicit camera params");
        assert!(stderr.contains("PROJECTED "), "render did not print PROJECTED with explicit camera: {stderr}");
    } else {
        assert_eq!(out.status.code(), Some(3), "no-adapter must exit 3");
        assert!(stderr.contains("NO_ADAPTER"));
    }
}

#[test]
fn render_with_size_option() {
    let dir = temp_dir("render_with_size_option");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&[
        "render".into(),
        in_dxf.to_string_lossy().into(),
        out_png.to_string_lossy().into(),
        "--camera".into(),
        "orbit".into(),
        "--size".into(),
        "640x480".into(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        assert!(is_png(&out_png), "render did not write a valid PNG with --size");
        assert!(stderr.contains("PROJECTED "), "render did not print PROJECTED with --size: {stderr}");
    } else {
        assert_eq!(out.status.code(), Some(3), "no-adapter must exit 3");
        assert!(stderr.contains("NO_ADAPTER"));
    }
}

#[test]
fn render_rejects_bad_style_sun_and_adapter() {
    let dir = temp_dir("render_rejects_bad_style_sun_and_adapter");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");
    let base = ["render".to_string(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into()];
    for extra in
        [["--style", "wireframe"], ["--style", "Shaded"], ["--sun", "lat=51"], ["--sun", "lat=51,date=2026-06-21,time=noon"], ["--adapter", "gpu"]]
    {
        let mut args = base.to_vec();
        args.extend(extra.iter().map(|s| s.to_string()));
        let out = run_cli(&args);
        assert!(!out.status.success(), "{extra:?} should be a usage error");
        assert_ne!(out.status.code(), Some(3), "{extra:?} is a usage error, not NO_ADAPTER");
        assert!(!out_png.exists(), "nothing should be written for {extra:?}");
    }
    let mut args = base.to_vec();
    args.extend(["--camera".to_string(), "full,eye=1,1,1,target=1,1,1".to_string()]);
    assert!(!run_cli(&args).status.success(), "eye == target should be rejected");
}

#[test]
fn render_prints_device_and_shadow_lines() {
    let dir = temp_dir("render_prints_device_and_shadow_lines");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&[
        "render".into(),
        in_dxf.to_string_lossy().into(),
        out_png.to_string_lossy().into(),
        "--adapter".into(),
        "fallback".into(),
        "--style".into(),
        "shaded-edges".into(),
        "--ground".into(),
        "--sun".into(),
        "lat=51.05,date=2026-06-21,time=solar-noon".into(),
        "--camera".into(),
        "full,eye=-8,-10,6,target=5,4,1,fov=50".into(),
        "--size".into(),
        "320x200".into(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.code() == Some(3) {
        assert!(stderr.contains("NO_ADAPTER"), "{stderr}");
        eprintln!("SKIP: no fallback adapter");
        return;
    }
    assert!(out.status.success(), "render failed: {stderr}");
    assert!(is_png(&out_png), "no PNG written");
    assert!(stderr.lines().any(|l| l.starts_with("PROJECTED ")), "{stderr}");
    assert!(stderr.lines().any(|l| l.starts_with("DEVICE name=")), "{stderr}");
    assert!(stderr.lines().any(|l| l.starts_with("SHADOW size=1024 built=")), "{stderr}");
}

#[test]
fn render_camera_unknown_param_rejected() {
    let dir = temp_dir("render_camera_unknown_param_rejected");
    let in_dxf = wall_plan(&dir);
    let out_png = dir.join("view.png");

    let out =
        run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit,bogus=5".into()]);
    assert!(!out.status.success(), "unknown camera parameter should be rejected");
    assert!(!out_png.exists(), "no output should be written for unknown camera parameter");
}
