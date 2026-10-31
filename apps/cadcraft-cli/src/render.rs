//! `render`: the architectural 3D scene of a drawing as a PNG, through the headless GPU path.

use super::args::RenderArgs;
use super::open;
use super::output::{OutputError, Streams, write_atomic};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use cadcraft_render3d::adapter::AdapterChoice;
use cadcraft_render3d::camera::{Camera, MAX_DISTANCE, Orbit};
use cadcraft_render3d::light::{Light, Sun};
use cadcraft_render3d::offscreen::{Offscreen, RenderOptions, Style};

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
    #[error("camera spec must begin with `orbit` or `full`")]
    CameraStart,
    #[error("camera field must be `name=value`, got `{0}`")]
    CameraField(String),
    #[error("camera {name}: {value}")]
    CameraValue { name: String, value: String },
    #[error("unknown camera field `{0}`")]
    CameraUnknown(String),
    #[error("full camera: {0}")]
    CameraFull(String),
    #[error("--sun must be lat=DEG,date=YYYY-MM-DD,time=solar-noon|HH:MM, got `{0}`")]
    Sun(String),
    #[error("--sun needs lat=DEG when the drawing has no site location")]
    NoSiteLat,
    #[error("render: no GPU adapter")]
    NoAdapter,
    #[error("render: {0}")]
    Gpu(String),
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

/// Parse `--style`: exactly `shaded`, `shaded-edges`, `hidden-line` or `glass-house`.
pub(crate) fn parse_style(s: &str) -> Option<Style> {
    match s {
        "shaded" => Some(Style::Shaded),
        "shaded-edges" => Some(Style::ShadedEdges),
        "hidden-line" => Some(Style::HiddenLine),
        "glass-house" => Some(Style::GlassHouse),
        _ => None,
    }
}

/// Parse `--adapter`: `auto` or `fallback`.
pub(crate) fn parse_adapter(s: &str) -> Option<AdapterChoice> {
    AdapterChoice::parse(s)
}

/// Parse `date=YYYY-MM-DD,time=solar-noon|HH:MM[,lat=DEG]` into `(lat, date, time)`. `date`
/// and `time` are always required; `lat` is optional (render falls back to the site latitude
/// when it is missing). When `lat` is given the sun must be computable for the triple; unknown
/// or repeated fields are refused and a non-finite latitude is rejected.
pub(crate) fn parse_sun(s: &str) -> Option<(Option<f64>, String, String)> {
    let (mut lat, mut date, mut time) = (None, None, None);
    for t in s.split(',').map(str::trim) {
        let (k, v) = t.split_once('=').map(|(k, v)| (k.trim(), v.trim()))?;
        match k {
            "lat" if lat.is_none() => lat = Some(v.parse::<f64>().ok().filter(|x| x.is_finite())?),
            "date" if date.is_none() => date = Some(v.to_string()),
            "time" if time.is_none() => time = Some(v.to_string()),
            _ => return None,
        }
    }
    let (date, time) = (date?, time?);
    if let Some(lat) = lat {
        cadcraft_arch::sun::sun_position(lat, &date, &time)?;
    }
    Some((lat, date, time))
}

/// Parse `eye=x,y,z[,eyeHeight=h],target=x,y,z[,fov=f]` (or `eye=x,y` with `eyeHeight`), the part
/// of a `full` camera after its name. Numbers must be finite, `fov` in (0, 180) degrees and the
/// eye must differ from the target.
fn parse_full_camera<'a>(tokens: impl Iterator<Item = &'a str>) -> Result<Camera, RenderError> {
    let err = RenderError::CameraFull;
    // Fields are `name=v[,v...]`: a token without `=` continues the previous field's numbers.
    let mut fields: Vec<(&str, Vec<f64>)> = Vec::new();
    for t in tokens.filter(|t| !t.is_empty()) {
        let (name, val) = match t.split_once('=') {
            Some((k, v)) => {
                fields.push((k.trim(), Vec::new()));
                (k.trim(), v.trim())
            }
            None if fields.is_empty() => return Err(RenderError::CameraField(t.to_string())),
            None => (fields.last().map_or("", |f| f.0), t),
        };
        let v = val.parse::<f64>().map_err(|e| err(format!("{name}: {e}")))?;
        if !v.is_finite() {
            return Err(err(format!("{name} must be finite, got `{val}`")));
        }
        if let Some(f) = fields.last_mut() {
            f.1.push(v);
        }
    }
    let (mut eye, mut eye_height, mut target, mut fov) = (None, None, None, 60.0);
    for (name, v) in fields {
        match (name, v.as_slice()) {
            ("eye", [x, y]) => eye = Some((*x, *y, None)),
            ("eye", [x, y, z]) => eye = Some((*x, *y, Some(*z))),
            ("eye", _) => return Err(err("eye needs x,y or x,y,z".into())),
            ("eyeHeight", [h]) => eye_height = Some(*h),
            ("eyeHeight", _) => return Err(err("eyeHeight needs one number".into())),
            ("target", [x, y, z]) => target = Some(Vec3::new(*x, *y, *z)),
            ("target", _) => return Err(err("target needs x,y,z".into())),
            ("fov", [f]) if 0.0 < *f && *f < 180.0 => fov = *f,
            ("fov", _) => return Err(err("fov must be one number in (0, 180) degrees".into())),
            (other, _) => return Err(RenderError::CameraUnknown(other.to_string())),
        }
    }
    let (ex, ey, ez) = eye.ok_or_else(|| err("eye is required".into()))?;
    let ez = eye_height.or(ez).ok_or_else(|| err("eye=x,y needs eyeHeight".into()))?;
    let target = target.ok_or_else(|| err("target is required".into()))?;
    let eye = Vec3::new(ex, ey, ez);
    if eye == target {
        return Err(err("eye and target must differ".into()));
    }
    Ok(Camera::Full(cadcraft_arch3d::camera::FullCamera::new(eye, target, fov)))
}

/// Parse `orbit[,azimuth=A][,elevation=E][,distance=D][,fov=F][,target=X,Y,Z]` into a camera
/// seeded from `base` (the fitted target is replaced when `target` is given), or the
/// `full,eye=..,target=..` form. Values must be finite and in range, so a bad `--camera`
/// never reaches the GPU.
fn parse_camera(spec: &str, base: Orbit) -> Result<Camera, RenderError> {
    let mut tokens = spec.split(',').map(str::trim);
    match tokens.next() {
        Some("orbit") => {}
        Some("full") => return parse_full_camera(tokens),
        _ => return Err(RenderError::CameraStart),
    }
    let mut it = tokens.filter(|t| !t.is_empty());
    let mut o = base;
    while let Some(t) = it.next() {
        let (name, val) = t.split_once('=').map(|(k, v)| (k.trim(), v.trim())).ok_or_else(|| RenderError::CameraField(t.to_string()))?;
        let bad = |why: String| RenderError::CameraValue { name: name.to_string(), value: why };
        let v = val.parse::<f64>().map_err(|e| bad(e.to_string()))?;
        if !v.is_finite() {
            return Err(bad(format!("must be finite, got `{val}`")));
        }
        match name {
            "target" => {
                // `target` spans three comma tokens: `target=X`, then the bare `Y` and `Z`
                // tokens that follow it (the way `parse_full_camera` reads `target=`).
                let ty = it.next().and_then(|y| y.parse::<f64>().ok()).ok_or_else(|| bad("target needs x,y,z".into()))?;
                let tz = it.next().and_then(|z| z.parse::<f64>().ok()).ok_or_else(|| bad("target needs x,y,z".into()))?;
                if ty.is_finite() && tz.is_finite() {
                    o.target = Vec3::new(v, ty, tz);
                } else {
                    return Err(bad(format!("target must be finite, got y={ty},z={tz}")));
                }
            }
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

/// The sun direction, in drawing coordinates, that `render` uses for a given `--sun` triple.
/// An explicit latitude (from `--sun`) wins over the site latitude; with neither a site
/// location nor an explicit latitude the `NoSiteLat` refusal fires. The resulting direction
/// is rotated by the site's true north so shadows fall along the site's true north (a no-op
/// when there is no site, which leaves the raw direction).
fn sun_light_dir(
    explicit_lat: Option<f64>,
    date: &str,
    time: &str,
    site: Option<&cadcraft_engine::doc::arch::SiteLocation>,
) -> Result<[f64; 3], RenderError> {
    let lat = explicit_lat.or_else(|| site.map(|s| s.latitude)).ok_or(RenderError::NoSiteLat)?;
    let sun = cadcraft_arch::sun::sun_position(lat, date, time).ok_or(RenderError::Sun(format!("lat={lat},date={date},time={time}")))?;
    let dir = cadcraft_arch::sun::sun_dir(&sun);
    Ok(cadcraft_arch::site::true_to_drawing(site, dir))
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
    let light = match &a.sun {
        Some((lat, date, time)) => {
            // An explicit `lat` wins; otherwise the site latitude stands in. With no site
            // location the existing refusal (reworded `NoSiteLat`) fires. The direction is
            // rotated by the site's true north.
            let [dx, dy, dz] = sun_light_dir(*lat, date, time, d.arch.site.as_ref())?;
            Light::from_sun(&Sun { dir: [dx as f32, dy as f32, dz as f32], ..Sun::default() }, Light::DEFAULT.ambient)
        }
        None => Light::DEFAULT,
    };
    let gpu = |e: cadcraft_render3d::Render3dError| match e {
        cadcraft_render3d::Render3dError::NoAdapter => RenderError::NoAdapter,
        e => RenderError::Gpu(e.to_string()),
    };
    let mut off = Offscreen::create_with(a.adapter, size).map_err(gpu)?;
    io.errln(format_args!("{}", off.device_info().line()));
    let map = if a.sun.is_some() || a.ground { off.build_shadow(&scene, scene.revision, &light, a.ground) } else { None };
    let opts = RenderOptions { shadow: map.as_ref(), style: a.style, ground: a.ground, ..RenderOptions::default() };
    off.render_with(&scene, scene.revision, &camera, &light, &opts);
    let png = off.read_png().map_err(gpu)?;
    if map.is_some() {
        io.errln(format_args!("SHADOW size={} built={}", cadcraft_render3d::shadow::SHADOW_SIZE, off.shadow_maps_built()));
    }
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
    fn parse_style_accepts_only_the_four_names() {
        assert_eq!(parse_style("shaded"), Some(Style::Shaded));
        assert_eq!(parse_style("shaded-edges"), Some(Style::ShadedEdges));
        assert_eq!(parse_style("hidden-line"), Some(Style::HiddenLine));
        assert_eq!(parse_style("glass-house"), Some(Style::GlassHouse));
        for bad in ["", "Shaded", "SHADED", "shaded_edges", "wireframe", " shaded", "glass"] {
            assert_eq!(parse_style(bad), None, "`{bad}`");
        }
    }

    #[test]
    fn orbit_camera_spec_accepts_a_target() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=5,4,0", base()) else {
            panic!("target camera");
        };
        assert_eq!(o.target, Vec3::new(5.0, 4.0, 0.0));
        assert!(parse_camera("orbit,target=5,4,0,azimuth=0", base()).is_ok());
    }

    #[test]
    fn orbit_target_needs_three_finite_numbers() {
        for bad in [
            "orbit,target=5,4",
            "orbit,target=5,abc,0",
            "orbit,target=abc,4,0",
            "orbit,target=5,inf,0",
            "orbit,target=5,4,inf",
            "orbit,target=NaN,4,0",
            "orbit,target=5,4,0,0",
        ] {
            assert!(parse_camera(bad, base()).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn parse_camera_accepts_the_full_form() {
        let Ok(Camera::Full(c)) = parse_camera("full,eye=1,2,3,target=4,5,6,fov=50", base()) else { panic!("full camera") };
        assert_eq!((c.eye, c.target, c.fov_deg), (Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0), 50.0));
        let Ok(Camera::Full(c)) = parse_camera("full,eye=1,2,eyeHeight=1.6,target=4,5,1", base()) else { panic!("eye height") };
        assert_eq!((c.eye, c.fov_deg), (Vec3::new(1.0, 2.0, 1.6), 60.0));
        for bad in [
            "full",
            "full,eye=1,2,3",
            "full,target=1,2,3",
            "full,eye=1,2,target=3,4,5",
            "full,eye=1,2,3,target=1,2,3",
            "full,eye=1,2,3,target=4,5,6,fov=0",
            "full,eye=1,2,3,target=4,5,6,fov=180",
            "full,eye=NaN,2,3,target=4,5,6",
            "full,eye=1,2,3,target=4,5,inf",
            "full,eye=1,2,3,4,target=4,5,6",
            "full,eye=1,2,3,target=4,5,6,bogus=1",
            "full,1,2,3",
        ] {
            assert!(parse_camera(bad, base()).is_err(), "`{bad}` should be rejected");
        }
        assert!(matches!(parse_camera("orbit,azimuth=5", base()), Ok(Camera::Orbit(_))));
    }

    #[test]
    fn parse_sun_requires_lat_date_and_time() {
        assert_eq!(parse_sun("lat=51.05,date=2026-06-21,time=solar-noon"), Some((Some(51.05), "2026-06-21".to_string(), "solar-noon".to_string())));
        assert_eq!(parse_sun(" lat = -33.9 , date=2026-12-21 , time=15:30 ").map(|s| s.2), Some("15:30".to_string()));
        for bad in [
            "",
            "lat=51",
            "lat=51,date=2026-06-21",
            "lat=51,date=2026-06-21,time=12:00,extra=1",
            "lat=51,lat=52,date=2026-06-21,time=12:00",
            "lat=abc,date=2026-06-21,time=12:00",
            "lat=NaN,date=2026-06-21,time=12:00",
            "lat=95,date=2026-06-21,time=12:00",
            "lat=51,date=2026-13-40,time=12:00",
            "lat=51,date=2026-06-21,time=noon",
        ] {
            assert_eq!(parse_sun(bad), None, "`{bad}`");
        }
    }

    #[test]
    fn parse_sun_allows_missing_lat() {
        assert_eq!(parse_sun("date=2026-06-21,time=solar-noon"), Some((None, "2026-06-21".to_string(), "solar-noon".to_string())),);
        assert_eq!(parse_sun("date=2026-12-21,time=09:30"), Some((None, "2026-12-21".to_string(), "09:30".to_string())),);
    }

    #[test]
    fn parse_adapter_accepts_auto_and_fallback() {
        assert_eq!(parse_adapter("auto"), Some(AdapterChoice::Auto));
        assert_eq!(parse_adapter("fallback"), Some(AdapterChoice::Fallback));
        for bad in ["", "gpu", "software", "auto,fallback"] {
            assert_eq!(parse_adapter(bad), None, "`{bad}`");
        }
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

    #[test]
    fn orbit_target_with_other_fields_after() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=5,4,0,azimuth=10,elevation=20", base()) else {
            panic!("target with other fields after");
        };
        assert_eq!(o.target, Vec3::new(5.0, 4.0, 0.0));
        assert_eq!(o.azimuth, 10.0);
        assert_eq!(o.elevation, 20.0);
    }

    #[test]
    fn orbit_target_as_only_field() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=1,2,3", base()) else {
            panic!("target as only field");
        };
        assert_eq!(o.target, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn orbit_duplicate_target_second_wins() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=1,2,3,target=4,5,6", base()) else {
            panic!("duplicate target");
        };
        assert_eq!(o.target, Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn orbit_target_negative_and_zero() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=-5,-4,0", base()) else {
            panic!("negative target");
        };
        assert_eq!(o.target, Vec3::new(-5.0, -4.0, 0.0));
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=0,0,0", base()) else {
            panic!("zero target");
        };
        assert_eq!(o.target, Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn orbit_target_inf_x_rejected() {
        assert!(parse_camera("orbit,target=inf,4,0", base()).is_err(), "inf x should be rejected");
        assert!(parse_camera("orbit,target=-inf,4,0", base()).is_err(), "-inf x should be rejected");
    }

    #[test]
    fn orbit_target_very_large_and_small() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=1e300,1e300,1e300", base()) else {
            panic!("large target");
        };
        assert_eq!(o.target, Vec3::new(1e300, 1e300, 1e300));
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target=1e-300,1e-300,1e-300", base()) else {
            panic!("small target");
        };
        assert_eq!(o.target, Vec3::new(1e-300, 1e-300, 1e-300));
    }

    #[test]
    fn orbit_target_with_whitespace() {
        let Ok(Camera::Orbit(o)) = parse_camera("orbit,target= 5 , 4 , 0", base()) else {
            panic!("target with whitespace");
        };
        assert_eq!(o.target, Vec3::new(5.0, 4.0, 0.0));
    }

    fn dirs_close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    fn dirs_not_close(a: [f64; 3], b: [f64; 3]) -> bool {
        !dirs_close(a, b)
    }

    #[test]
    fn render_sun_uses_site_latitude_and_true_north() {
        let date = "2026-06-21";
        let time = "solar-noon";
        let site = cadcraft_engine::doc::arch::SiteLocation { latitude: 51.05, true_north: 0.0, ..Default::default() };
        let site_north90 = cadcraft_engine::doc::arch::SiteLocation { latitude: 51.05, true_north: 90.0, ..Default::default() };

        // 1) No explicit latitude: the site latitude must stand in, and the result must equal
        //    the direction computed by explicitly asking for that same latitude.
        let via_site = sun_light_dir(None, date, time, Some(&site)).unwrap();
        let via_explicit = sun_light_dir(Some(51.05), date, time, Some(&site)).unwrap();
        assert!(dirs_close(via_site, via_explicit), "site latitude must equal an explicit 51.05 latitude");

        // 2) An explicit latitude wins over the site latitude (and gives a different answer).
        let explicit_win = sun_light_dir(Some(-20.0), date, time, Some(&site)).unwrap();
        assert!(dirs_not_close(via_site, explicit_win), "an explicit latitude must win over the site latitude");

        // 3) The site's true north rotates the shadow direction; true north 0 is a no-op.
        let via_north90 = sun_light_dir(None, date, time, Some(&site_north90)).unwrap();
        assert!(dirs_not_close(via_site, via_north90), "true north must rotate the shadow direction");

        // 4) No site and no explicit latitude -> the NoSiteLat refusal fires.
        assert!(matches!(sun_light_dir(None, date, time, None), Err(RenderError::NoSiteLat)));
    }

    #[test]
    fn sun_light_dir_equator_equinox_noon_is_overhead() {
        // At the equator on the March equinox at solar noon, the sun is directly overhead:
        // elevation 90°, so the direction vector should be (0, 0, 1).
        let site = cadcraft_engine::doc::arch::SiteLocation { latitude: 0.0, true_north: 0.0, ..Default::default() };
        let dir = sun_light_dir(None, "2026-03-20", "solar-noon", Some(&site)).unwrap();
        assert!((dir[0]).abs() < 1e-6, "x should be ~0 at equator equinox noon, got {}", dir[0]);
        assert!((dir[1]).abs() < 1e-6, "y should be ~0 at equator equinox noon, got {}", dir[1]);
        assert!((dir[2] - 1.0).abs() < 1e-6, "z should be ~1 at equator equinox noon, got {}", dir[2]);
    }

    #[test]
    fn sun_light_dir_true_north_360_equals_zero() {
        // A full 360° rotation of true north must give the same direction as 0°.
        let site0 = cadcraft_engine::doc::arch::SiteLocation { latitude: 40.0, true_north: 0.0, ..Default::default() };
        let site360 = cadcraft_engine::doc::arch::SiteLocation { latitude: 40.0, true_north: 360.0, ..Default::default() };
        let dir0 = sun_light_dir(None, "2026-06-21", "solar-noon", Some(&site0)).unwrap();
        let dir360 = sun_light_dir(None, "2026-06-21", "solar-noon", Some(&site360)).unwrap();
        assert!(dirs_close(dir0, dir360), "true_north=360 must equal true_north=0");
    }

    #[test]
    fn parse_sun_boundary_latitudes() {
        // Latitude boundaries: ±90° and 0° must all parse successfully.
        assert!(parse_sun("lat=90,date=2026-06-21,time=solar-noon").is_some(), "lat=90 must parse");
        assert!(parse_sun("lat=-90,date=2026-06-21,time=solar-noon").is_some(), "lat=-90 must parse");
        assert!(parse_sun("lat=0,date=2026-06-21,time=solar-noon").is_some(), "lat=0 must parse");
        // Just outside the boundaries must fail.
        assert!(parse_sun("lat=90.1,date=2026-06-21,time=solar-noon").is_none(), "lat=90.1 must fail");
        assert!(parse_sun("lat=-90.1,date=2026-06-21,time=solar-noon").is_none(), "lat=-90.1 must fail");
    }

    #[test]
    fn sun_light_dir_site_reference_does_not_affect_direction() {
        // The site's reference point (easting/northing/CRS) must not affect the sun direction.
        let site_no_ref = cadcraft_engine::doc::arch::SiteLocation { latitude: 51.5, true_north: 15.0, ..Default::default() };
        let site_with_ref = cadcraft_engine::doc::arch::SiteLocation {
            latitude: 51.5,
            true_north: 15.0,
            reference: Some(cadcraft_engine::doc::arch::SiteReference {
                point: cadcraft_geom::Vec2::new(100.0, 200.0),
                easting: 500000.0,
                northing: 5700000.0,
                crs: "EPSG:27700".to_string(),
            }),
            ..Default::default()
        };
        let dir_no_ref = sun_light_dir(None, "2026-06-21", "solar-noon", Some(&site_no_ref)).unwrap();
        let dir_with_ref = sun_light_dir(None, "2026-06-21", "solar-noon", Some(&site_with_ref)).unwrap();
        assert!(dirs_close(dir_no_ref, dir_with_ref), "reference point must not affect sun direction");
    }

    #[test]
    fn parse_sun_whitespace_and_trailing_comma() {
        // Various whitespace patterns must parse correctly.
        assert_eq!(
            parse_sun("  lat = 51.05 , date = 2026-06-21 , time = solar-noon  "),
            Some((Some(51.05), "2026-06-21".to_string(), "solar-noon".to_string()))
        );
        // Trailing comma should be tolerated (empty token after split is skipped by filter).
        // Actually, parse_sun doesn't filter empty tokens, so a trailing comma produces an
        // empty token that fails split_once('='). This is a real edge case.
        // Let's verify the current behavior: trailing comma should fail.
        assert!(parse_sun("lat=51.05,date=2026-06-21,time=solar-noon,").is_none(), "trailing comma should fail");
        // But extra whitespace around the comma is fine.
        assert_eq!(parse_sun("lat=51.05, date=2026-06-21, time=solar-noon"), Some((Some(51.05), "2026-06-21".to_string(), "solar-noon".to_string())));
    }
}
