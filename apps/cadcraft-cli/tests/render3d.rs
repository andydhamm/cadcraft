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

/// A temp directory unique to this test (no cross-test clobbering): the tests run in
/// parallel in one process, so the process id and a timestamp alone are shared by all of them.
fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cadcraft-cli-render-{}", std::process::id())).join(test);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `sample floorplan` -> the wall plan the render tests draw on.
fn floorplan(dir: &Path) -> PathBuf {
    let in_dxf = dir.join("floorplan.dxf");
    assert!(run_cli(&["sample".into(), "floorplan".into(), in_dxf.to_string_lossy().into()]).status.success());
    assert!(in_dxf.exists(), "floorplan.dxf not written");
    in_dxf
}

#[test]
fn render_writes_a_png_or_skips_without_an_adapter() {
    let dir = temp_dir("render");
    let in_dxf = floorplan(&dir);
    let out_png = dir.join("view.png");

    let out = run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), "orbit".into()]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // Every outcome exits 0. (The old check expected a no-adapter run to both fail and exit
    // 0, and a successful run to always write a PNG, so it failed on every GPU-less machine.)
    assert_eq!(out.status.code(), Some(0), "render must exit 0 (written or SKIP), stderr: {stderr}");
    if stderr.contains("render: no GPU adapter available") || stderr.contains("not available") {
        // SKIP: no adapter, or an adapter but no draw pass yet. Then nothing may be
        // written: an uninitialised target used to be saved as if it were the view.
        assert!(!out_png.exists(), "a PNG was written although nothing was rendered: {stderr}");
    } else {
        assert!(is_png(&out_png), "render did not write a valid PNG: {}", out_png.display());
        assert!(fs::metadata(&out_png).unwrap().len() > 8, "{:?}", out_png);
    }
}

#[test]
fn render_rejects_sizes_wgpu_cannot_allocate() {
    let dir = temp_dir("size");
    let in_dxf = floorplan(&dir);
    let out_png = dir.join("view.png");
    for size in ["0x0", "100000x100000", "8193x8193"] {
        let out = run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--size".into(), size.into()]);
        assert!(!out.status.success(), "size `{size}` must be refused, stderr={}", String::from_utf8_lossy(&out.stderr));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("panicked"), "size `{size}` panicked: {stderr}");
    }
}

#[test]
fn render_rejects_a_malformed_camera() {
    let dir = temp_dir("malformed");
    let in_dxf = floorplan(&dir);
    let out_png = dir.join("view.png");

    for camera in ["orbit,azimuth=NaN", "orbit,elevation=inf", "orbit,distance=-1", "orbit,fov=abc", "not-a-camera,fov=1"] {
        let out =
            run_cli(&["render".into(), in_dxf.to_string_lossy().into(), out_png.to_string_lossy().into(), "--camera".into(), camera.to_string()]);
        assert!(!out.status.success(), "malformed camera `{camera}` should be rejected: stderr={}", String::from_utf8_lossy(&out.stderr));
        assert!(!out_png.exists(), "nothing should be written for malformed camera `{camera}`");
    }
}
