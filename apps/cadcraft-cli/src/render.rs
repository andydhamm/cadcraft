//! `render`: the architectural 3D scene of a drawing as a PNG, through the headless GPU path.

use super::args::RenderArgs;
use super::open;
use super::output::{OutputError, Streams, write_atomic};
use cadcraft_engine::Session;
use cadcraft_render3d::camera::{Camera, MAX_DISTANCE, Orbit};

/// Default pixel size for `render`.
const DEFAULT_SIZE: (u32, u32) = (2400, 1600);

/// The largest side `render` honours: wgpu's default `max_texture_dimension_2d`. A larger request
/// would be shrunk silently, so the `PROJECTED` line would stop describing the PNG.
const MAX_SIZE: u32 = 8192;

/// Exit status when `render` finds no GPU adapter.
pub(crate) const RENDER_NO_ADAPTER_EXIT: i32 = 3;

/// Why `render` fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RenderError {
    #[error("--size needs WxH, got `{0}`")]
    SizeFormat(String),
    #[error("--size {name} must be a whole number in 1..={MAX_SIZE}, got `{value}`")]
    SizeRange { name: &'static str, value: String },
    #[error("camera spec must begin with `orbit`")]
    CameraStart,
    #[error("camera field must be `name=value`, got `{0}`")]
    CameraField(String),
    #[error("camera {name}: {value}")]
    CameraValue { name: String, value: String },
    #[error("unknown camera field `{0}`")]
    CameraUnknown(String),
    #[error("render: no GPU adapter")]
    NoAdapter,
    #[error("render: {0}")]
    Gpu(String),
    #[error("render: tokio runtime: {0}")]
    Runtime(std::io::Error),
    #[error(transparent)]
    Open(Box<super::error::CliError>),
    #[error(transparent)]
    Output(#[from] OutputError),
}

/// Parse `WxH` into a pixel size (whole numbers in `1..=MAX_SIZE`).
pub(crate) fn parse_size(v: &str) -> Result<(u32, u32), RenderError> {
    let parts: Vec<&str> = v.split(['x', 'X']).map(str::trim).collect();
    let [w, h] = parts.as_slice() else { return Err(RenderError::SizeFormat(v.to_string())) };
    let num = |name: &'static str, t: &str| match t.parse::<u32>() {
        Ok(n) if (1..=MAX_SIZE).contains(&n) => Ok(n),
        _ => Err(RenderError::SizeRange { name, value: t.to_string() }),
    };
    Ok((num("width", w)?, num("height", h)?))
}

/// Parse `orbit[,azimuth=A][,elevation=E][,distance=D][,fov=F]` into a camera seeded from `base`.
/// Values must be finite and in range, so a bad `--camera` never reaches the GPU.
fn parse_camera(spec: &str, base: Orbit) -> Result<Camera, RenderError> {
    let mut tokens = spec.split(',').map(str::trim);
    if tokens.next() != Some("orbit") {
        return Err(RenderError::CameraStart);
    }
    let mut o = base;
    for t in tokens.filter(|t| !t.is_empty()) {
        let (name, val) = t.split_once('=').map(|(k, v)| (k.trim(), v.trim())).ok_or_else(|| RenderError::CameraField(t.to_string()))?;
        let bad = |why: String| RenderError::CameraValue { name: name.to_string(), value: why };
        let v = val.parse::<f64>().map_err(|e| bad(e.to_string()))?;
        if !v.is_finite() {
            return Err(bad(format!("must be finite, got `{val}`")));
        }
        match name {
            "azimuth" => o.azimuth = v,
            "elevation" if (-89.0..=89.0).contains(&v) => o.elevation = v,
            "elevation" => return Err(bad(format!("must be in [-89, 89] degrees, got `{val}`"))),
            "distance" if v > 0.0 && v <= MAX_DISTANCE => o.distance = v,
            "distance" => return Err(bad(format!("must be in (0, {MAX_DISTANCE}], got `{val}`"))),
            "fov" if 0.0 < v && v < 180.0 => o.fov_deg = v,
            "fov" => return Err(bad(format!("must be in (0, 180) degrees, got `{val}`"))),
            other => return Err(RenderError::CameraUnknown(other.to_string())),
        }
    }
    Ok(Camera::Orbit(o))
}

/// `PROJECTED x0,y0,x1,y1` at fixed precision.
fn format_projected(pb: [f32; 4]) -> String {
    format!("PROJECTED {:.6},{:.6},{:.6},{:.6}", pb[0], pb[1], pb[2], pb[3])
}

pub(crate) fn render(a: &RenderArgs, io: &mut Streams) -> Result<(), RenderError> {
    let size = a.size.unwrap_or(DEFAULT_SIZE);
    let mut s = Session::empty();
    open(&mut s, &a.file).map_err(|e| RenderError::Open(Box::new(e)))?;
    let d = s.doc().map_err(|e| RenderError::Open(Box::new(e.into())))?;
    let scene = cadcraft_arch3d::build(&d.arch);
    let base = Orbit::fit(&scene.bounds);
    let camera = match &a.camera {
        Some(spec) => parse_camera(spec, base)?,
        None => Camera::Orbit(base),
    };
    // The mesh vertices, not the bounding box: room slabs leave its lower corners empty.
    let pb = cadcraft_render3d::camera::projected_scene_bounds(&camera, &scene, size);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(RenderError::Runtime)?;
    let png = rt.block_on(cadcraft_render3d::offscreen::headless_png(&scene, &camera, size)).map_err(|e| match e {
        cadcraft_render3d::Render3dError::NoAdapter => RenderError::NoAdapter,
        e => RenderError::Gpu(e.to_string()),
    })?;
    write_atomic(&a.output, &png)?;
    io.errln(format_args!("wrote {} ({} bytes)", a.output.display(), png.len()));
    let projected = format_projected(pb);
    io.outln(format_args!("{projected}"));
    io.errln(format_args!("{projected}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Orbit {
        Orbit::fit(&cadcraft_arch3d::mesh::Bounds3::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]))
    }

    #[test]
    fn projected_is_fixed_width() {
        assert_eq!(format_projected([1.0, 2.0, 3.0, 4.0]), "PROJECTED 1.000000,2.000000,3.000000,4.000000");
    }

    #[test]
    fn size_must_fit_a_wgpu_texture() {
        assert_eq!(parse_size(" 100 X 200 ").ok(), Some((100, 200)));
        assert_eq!(parse_size("8192x8192").ok(), Some((8192, 8192)));
        for bad in ["0x10", "10x0", "8193x10", "4294967295x1", "x", "10", "-5x5", "100.5x100", "1x2x3", ""] {
            assert!(parse_size(bad).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn camera_fields_are_validated() {
        assert!(parse_camera("orbit,,azimuth=1.5, elevation = 0.5", base()).is_ok());
        for bad in [
            "",
            "perspective",
            "orbit,azimuth",
            "orbit,azimuth=NaN",
            "orbit,distance=0",
            "orbit,distance=1e300",
            "orbit,fov=180",
            "orbit,fov=0",
            "orbit,elevation=90",
            "orbit,unknown=1",
        ] {
            assert!(parse_camera(bad, base()).is_err(), "`{bad}` should be rejected");
        }
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,azimuth=10,azimuth=20,elevation=30,distance=25,fov=60", base()) else {
            return;
        };
        assert_eq!((o.azimuth, o.elevation, o.distance, o.fov_deg), (20.0, 30.0, 25.0, 60.0));
    }

    fn orbit_of(spec: &str) -> Orbit {
        match parse_camera(spec, base()) {
            Ok(Camera::Orbit(o)) => o,
            _ => panic!("`{spec}` should parse to an orbit"),
        }
    }

    #[test]
    fn format_projected_is_fixed_width() {
        assert_eq!(format_projected([1.0, 2.0, 3.0, 4.0]), "PROJECTED 1.000000,2.000000,3.000000,4.000000");
    }

    #[test]
    fn no_adapter_exits_three() {
        assert_eq!(RENDER_NO_ADAPTER_EXIT, 3);
        assert!(matches!(
            Err::<(), _>(cadcraft_render3d::Render3dError::NoAdapter).map_err(|e| match e {
                cadcraft_render3d::Render3dError::NoAdapter => RenderError::NoAdapter,
                e => RenderError::Gpu(e.to_string()),
            }),
            Err(RenderError::NoAdapter)
        ));
    }

    #[test]
    fn render_no_adapter_exit_constant() {
        assert_eq!(RENDER_NO_ADAPTER_EXIT, 3);
    }

    #[test]
    fn format_projected_edge_cases() {
        assert_eq!(format_projected([0.0; 4]), "PROJECTED 0.000000,0.000000,0.000000,0.000000");
        assert_eq!(format_projected([-1.5, -2.5, -3.5, -4.5]), "PROJECTED -1.500000,-2.500000,-3.500000,-4.500000");
        assert_eq!(format_projected([1e6, 2e6, 3e6, 4e6]), "PROJECTED 1000000.000000,2000000.000000,3000000.000000,4000000.000000");
        assert_eq!(format_projected([1.0000008, 2.0000008, 3.0000008, 4.0000008]), "PROJECTED 1.000001,2.000001,3.000001,4.000001");
        assert_eq!(format_projected([1.0000003, 2.0000003, 3.0000003, 4.0000003]), "PROJECTED 1.000000,2.000000,3.000000,4.000000");
    }

    #[test]
    fn format_projected_rounds_half_up_at_seventh_decimal() {
        assert_eq!(format_projected([1.0000008, 2.0000008, 3.0000008, 4.0000008]), "PROJECTED 1.000001,2.000001,3.000001,4.000001");
    }

    #[test]
    fn format_projected_very_small_values_round_to_zero() {
        assert_eq!(format_projected([1e-10, 2e-10, 3e-10, 4e-10]), "PROJECTED 0.000000,0.000000,0.000000,0.000000");
    }

    #[test]
    fn format_projected_f32_max_is_finite() {
        let s = format_projected([f32::MAX; 4]);
        let parts: Vec<&str> = s.strip_prefix("PROJECTED ").unwrap_or("").split(',').collect();
        assert_eq!(parts.len(), 4);
        for p in parts {
            assert!(p.parse::<f64>().is_ok_and(f64::is_finite), "{p}");
        }
    }

    #[test]
    fn projected_output_round_trips() {
        let pb = [1.234567, -2.345678, 3.456789, -4.567890];
        let s = format_projected(pb);
        let parts: Vec<&str> = s.strip_prefix("PROJECTED ").unwrap_or("").split(',').collect();
        assert_eq!(parts.len(), 4);
        for (i, p) in parts.iter().enumerate() {
            let v: f64 = p.parse().unwrap_or(f64::NAN);
            assert!((v - f64::from(pb[i])).abs() < 1e-6, "round-trip failed for index {i}: {v} vs {}", pb[i]);
        }
    }

    #[test]
    fn parse_size_valid() {
        assert_eq!(parse_size("2400x1600").ok(), Some((2400, 1600)));
        assert_eq!(parse_size("1x1").ok(), Some((1, 1)));
        assert_eq!(parse_size("1920x1080").ok(), Some((1920, 1080)));
        assert_eq!(parse_size(" 100 x 200 ").ok(), Some((100, 200)));
        assert_eq!(parse_size("100X200").ok(), Some((100, 200)));
        assert_eq!(parse_size("8192x8192").ok(), Some((8192, 8192)));
        assert!(parse_size("8193x8193").is_err());
        assert!(parse_size("4294967295x4294967295").is_err());
    }

    #[test]
    fn parse_size_invalid() {
        for bad in [
            "",
            "100",
            "100x",
            "x100",
            "0x100",
            "100x0",
            "abcxdef",
            "100x200x300",
            "100 x 200 x 300",
            "8193x100",
            "4294967295x1",
            "-1x100",
            "100.5x100",
        ] {
            assert!(parse_size(bad).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn parse_size_asymmetric_boundaries() {
        assert_eq!(parse_size("1x8192").ok(), Some((1, 8192)));
        assert_eq!(parse_size("8192x1").ok(), Some((8192, 1)));
        for bad in ["0x8192", "8192x0", "8193x8192", "8192x8193"] {
            assert!(parse_size(bad).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn parse_size_with_whitespace() {
        assert_eq!(parse_size("  100  x  200  ").ok(), Some((100, 200)));
        assert_eq!(parse_size("\t100\tX\t200\t").ok(), Some((100, 200)));
    }

    #[test]
    fn parse_camera_valid() {
        assert!(parse_camera("orbit", base()).is_ok());
        let o = orbit_of("orbit,azimuth=1.5,elevation=0.5,distance=10,fov=45");
        assert_eq!((o.azimuth, o.elevation, o.distance, o.fov_deg), (1.5, 0.5, 10.0, 45.0));
        assert!(parse_camera("orbit,,azimuth=1.5,,", base()).is_ok());
        assert!(parse_camera("orbit , azimuth = 1.5 , elevation = 0.5", base()).is_ok());
    }

    #[test]
    fn parse_camera_invalid() {
        for bad in [
            "",
            "perspective",
            "orbit,azimuth",
            "orbit,azimuth=abc",
            "orbit,azimuth=NaN",
            "orbit,distance=0",
            "orbit,distance=-1",
            "orbit,fov=0",
            "orbit,fov=-1",
            "orbit,unknown=1",
        ] {
            assert!(parse_camera(bad, base()).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn camera_fields_outside_a_finite_projection_are_refused() {
        for bad in ["orbit,fov=180", "orbit,fov=360", "orbit,elevation=90", "orbit,elevation=-120", "orbit,distance=1e300"] {
            assert!(parse_camera(bad, base()).is_err(), "`{bad}` should be rejected");
        }
        let o = orbit_of("orbit,azimuth=35,elevation=30,distance=25,fov=60");
        assert_eq!((o.azimuth, o.elevation, o.distance, o.fov_deg), (35.0, 30.0, 25.0, 60.0));
    }

    #[test]
    fn parse_camera_boundary_angles() {
        for ok in ["orbit,fov=0.1", "orbit,fov=179.999", "orbit,elevation=89", "orbit,elevation=-89"] {
            assert!(parse_camera(ok, base()).is_ok(), "`{ok}` should be accepted");
        }
        let o = orbit_of("orbit,azimuth=0,elevation=0,distance=1,fov=90");
        assert_eq!((o.azimuth, o.elevation, o.distance, o.fov_deg), (0.0, 0.0, 1.0, 90.0));
    }

    #[test]
    fn parse_camera_with_duplicate_fields() {
        assert_eq!(orbit_of("orbit,azimuth=10,azimuth=20").azimuth, 20.0);
    }

    #[test]
    fn offscreen_render_of_a_two_room_model_has_lit_pixels() {
        let d = cadcraft_engine::sample::floor_plan();
        let scene = cadcraft_arch3d::build(&d.arch);
        if cadcraft_render3d::offscreen::device_within(std::time::Duration::from_secs(5)).is_none() {
            eprintln!("SKIP: no GPU adapter available");
            return;
        }
        let camera = Camera::Orbit(Orbit::fit(&scene.bounds));
        let Ok(png) = cadcraft_render3d::offscreen::render_png(&scene, &camera, &cadcraft_render3d::light::Light::DEFAULT, (320, 200)) else {
            panic!("render failed");
        };
        let Ok(rgba) = cadcraft_render3d::offscreen::png_rgba(&png) else { panic!("decode failed") };
        let clear = [5u8, 6, 9, 255];
        let different = rgba.chunks_exact(4).filter(|p| p.iter().zip(clear.iter()).any(|(a, b)| (i32::from(*a) - i32::from(*b)).abs() > 10)).count();
        let fraction = different as f64 / (rgba.len() / 4) as f64;
        assert!(fraction > 0.02, "only {}% of pixels differ from clear color (need > 2%)", fraction * 100.0);
    }
}
