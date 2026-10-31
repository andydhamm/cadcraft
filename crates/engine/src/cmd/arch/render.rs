//! `render`: a 3D still of the drawing (phase 10) through the renderer the app installs with
//! [`set_render_hooks`] (the CLI and the desktop app; the engine has no GPU). One renderer lives
//! for the whole session, so its mesh upload and shadow maps are reused from one `render` to the
//! next: a call that only moves the camera builds no shadow map. `sunstudy` uses the same hook.
//!
//! Filled here: the hook types, [`set_render_hooks`], [`render_hooks`], [`stats`]. The command
//! itself is filled by `p10-cmd-render` (spec stub below).

use std::path::PathBuf;
use std::sync::OnceLock;

use cadcraft_arch::site::true_to_drawing;
use cadcraft_arch::sun::{sun_dir, sun_position};
use cadcraft_arch3d::camera::{Camera, FullCamera, Orbit};
use cadcraft_doc::Drawing;
use cadcraft_doc::arch::{Camera3d, CameraKind};
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

use crate::cmd::arch::pick::parse_camera;
use crate::cmd::{CommandSpec, bad};
use crate::{EngineError, Session};

/// What lights the still besides the fixtures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SunChoice {
    /// The default light (`render3d::Light::DEFAULT`), no sun shadow.
    Default,
    /// Night: the sun is off.
    Off,
    /// The sun from this direction (toward the sun, drawing coordinates, +Z up), with its shadow.
    Dir([f64; 3]),
}

/// One `render` (or one `sunstudy` still).
#[derive(Clone, Debug)]
pub struct RenderRequest<'a> {
    pub drawing: &'a Drawing,
    /// `None`: the active saved camera, else the orbit fitted to the scene.
    pub camera: Option<Camera>,
    /// Pixels, each side 1..=8192.
    pub size: (u32, u32),
    /// Where to write the PNG; `None` renders without writing (the reply still reports).
    pub out: Option<PathBuf>,
    /// Finalize Render instead of the interactive view.
    pub final_render: bool,
    pub sun: SunChoice,
    /// `shaded`, `shaded-edges`, `hidden-line` or `glass-house`.
    pub style: String,
    /// Draw the ground plane.
    pub ground: bool,
    /// Lights shadowed at most in the interactive view (Finalize shadows every light).
    pub budget: usize,
    /// World points whose pixel and colour the reply reports (at most 64).
    pub samples: Vec<[f64; 3]>,
}

/// What a session's renderer has done so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub shadow_maps_built: u64,
    pub final_renders: u64,
}

/// The app's renderer: `render` returns the reply JSON of one still (see `p10-cli-render-hook`
/// for its keys) or an error message; `stats` reports the counters.
pub struct RenderHooks {
    pub render: fn(&RenderRequest) -> Result<Value, String>,
    pub stats: fn() -> RenderStats,
}

static HOOKS: OnceLock<RenderHooks> = OnceLock::new();

/// Install the app's renderer (the first call wins).
pub fn set_render_hooks(h: RenderHooks) {
    let _ = HOOKS.set(h);
}

/// The installed renderer, if any.
pub fn render_hooks() -> Option<&'static RenderHooks> {
    HOOKS.get()
}

/// The installed renderer's counters (zeros without one): `build3d` reports them.
pub fn stats() -> RenderStats {
    HOOKS.get().map(|h| (h.stats)()).unwrap_or_default()
}

fn parse_size(v: Option<&Value>) -> Result<(u32, u32), EngineError> {
    const MAX: u64 = 8192;
    match v {
        None => Ok((1200, 800)),
        Some(Value::Array(arr)) => {
            if arr.len() != 2 {
                return Err(bad("render", "`size` must be [w,h]"));
            }
            let dim = |idx: usize| -> Result<u32, EngineError> {
                arr.get(idx)
                    .and_then(Value::as_u64)
                    .and_then(|n| (1..=MAX).contains(&n).then_some(n as u32))
                    .ok_or_else(|| bad("render", format!("`size[{}]` must be a whole number in 1..={MAX}", idx)))
            };
            Ok((dim(0)?, dim(1)?))
        }
        _ => Err(bad("render", "`size` must be [w,h]")),
    }
}

fn parse_samples(v: Option<&Value>) -> Result<Vec<[f64; 3]>, EngineError> {
    match v {
        None => Ok(Vec::new()),
        Some(Value::Array(arr)) => {
            if arr.len() > 64 {
                return Err(bad("render", "`samples` must have at most 64 points"));
            }
            let mut out = Vec::with_capacity(arr.len());
            for (i, p) in arr.iter().enumerate() {
                let inner = p.as_array().ok_or_else(|| bad("render", format!("`samples[{}]` must be [x,y,z]", i)))?;
                if inner.len() != 3 {
                    return Err(bad("render", format!("`samples[{}]` must be [x,y,z]", i)));
                }
                let f = |idx: usize| -> Result<f64, EngineError> {
                    inner
                        .get(idx)
                        .and_then(Value::as_f64)
                        .filter(|x| x.is_finite())
                        .ok_or_else(|| bad("render", format!("`samples[{}][{}]` must be a finite number", i, idx)))
                };
                out.push([f(0)?, f(1)?, f(2)?]);
            }
            Ok(out)
        }
        _ => Err(bad("render", "`samples` must be an array of [x,y,z] points")),
    }
}

fn parse_sun(v: Option<&Value>, doc: &Drawing) -> Result<SunChoice, EngineError> {
    match v {
        None => Ok(SunChoice::Default),
        Some(Value::Bool(false)) => Ok(SunChoice::Off),
        Some(Value::String(s)) => {
            if s == "default" {
                Ok(SunChoice::Default)
            } else {
                Err(bad("render", format!("unknown sun value `{s}`")))
            }
        }
        Some(Value::Object(obj)) => {
            let date = obj.get("date").and_then(Value::as_str).ok_or_else(|| bad("render", "sun needs a `date` field"))?;
            let time = obj.get("time").and_then(Value::as_str).ok_or_else(|| bad("render", "sun needs a `time` field"))?;
            let site = doc.arch.site.as_ref().ok_or_else(|| bad("render", "sun needs a site location: run site first"))?;
            let sun = sun_position(site.latitude, date, time).ok_or_else(|| bad("render", "invalid sun date or time"))?;
            if sun.elevation_deg <= 0.0 {
                return Err(bad("render", "sun is below the horizon at the given date and time"));
            }
            let dir = sun_dir(&sun);
            let drawing_dir = true_to_drawing(Some(site), dir);
            Ok(SunChoice::Dir(drawing_dir))
        }
        _ => Err(bad("render", "sun must be false, \"default\" or {date, time}")),
    }
}

fn camera_from_saved(c: &Camera3d) -> Result<Camera, EngineError> {
    match c.kind {
        CameraKind::Orbit => {
            let target = c.target.unwrap_or([0.0; 3]);
            let azimuth = c.azimuth_deg.unwrap_or(0.0);
            let elevation = c.elevation_deg.unwrap_or(30.0);
            let distance = c.distance.unwrap_or(10.0);
            let fov = c.fov_deg.unwrap_or(45.0);
            let target_vec = Vec3::new(target[0], target[1], target[2]);
            Ok(Camera::Orbit(Orbit::new(target_vec, azimuth, elevation, distance, fov)))
        }
        CameraKind::Full => {
            let eye_x = c.eye.as_ref().map_or(0.0, |e| e[0]);
            let eye_y = c.eye.as_ref().map_or(0.0, |e| e[1]);
            let eye = Vec3::new(eye_x, eye_y, c.eye_height.unwrap_or(1.6));
            let target = c.target.unwrap_or([0.0; 3]);
            let fov = c.fov_deg.unwrap_or(45.0);
            let target_vec = Vec3::new(target[0], target[1], target[2]);
            Ok(Camera::Full(FullCamera::new(eye, target_vec, fov)))
        }
    }
}

fn camera_from_params(v: Option<&Value>, doc: &Drawing) -> Result<Option<Camera>, EngineError> {
    match v {
        None | Some(Value::Null) => match &doc.arch.active_camera {
            Some(name) => match doc.arch.camera_by_name(name) {
                Some(c) => Ok(Some(camera_from_saved(c)?)),
                None => Ok(None),
            },
            None => Ok(None),
        },
        Some(cv) => Ok(Some(parse_camera(cv)?)),
    }
}

/// `render`: render a 3D still through the installed render hook.
fn run_render(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let hooks = render_hooks().ok_or_else(|| bad("render", "render needs the 3D renderer: run it from cadcraft-cli run or the app"))?;

    let doc = s.doc()?;

    let size = parse_size(params.get("size"))?;
    let camera = camera_from_params(params.get("camera"), doc)?;
    let final_render = params.get("final").and_then(Value::as_bool).unwrap_or(false);
    let sun = parse_sun(params.get("sun"), doc)?;
    let style = params
        .get("style")
        .and_then(Value::as_str)
        .and_then(|s| matches!(s, "shaded" | "shaded-edges" | "hidden-line" | "glass-house").then(|| s.to_string()))
        .ok_or_else(|| bad("render", "`style` must be one of: shaded, shaded-edges, hidden-line, glass-house"))?;
    let ground = params.get("ground").and_then(Value::as_bool).unwrap_or(false);
    let budget = params
        .get("budget")
        .and_then(Value::as_u64)
        .and_then(|n| (0..=64).contains(&n).then_some(n as usize))
        .ok_or_else(|| bad("render", "`budget` must be a whole number in 0..=64"))?;
    let samples = parse_samples(params.get("samples"))?;
    let out = params.get("out").and_then(Value::as_str).map(PathBuf::from);

    let request = RenderRequest { drawing: &doc, camera, size, out, final_render, sun, style, ground, budget, samples };

    (hooks.render)(&request).map_err(|e| bad("render", e))
}

/// Command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("render", "Render 3D", run_render)
            .menu(&["Architecture", "Render 3D"])
            .params("{out?: path, size?: [w,h], camera?, final?: bool, sun?: false | {date, time} | \"default\", style?, ground?: bool, budget?, samples?: [[x,y,z], ...]}")
            .noundo(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn render_without_a_renderer_is_refused() {
        let mut s = Session::new();
        let r = s.execute("render", &json!({}));
        let e = r.unwrap_err().to_string();
        assert!(e.contains("render needs the 3D renderer"), "{e}");
    }

    #[test]
    fn render_parses_sun_off_and_site_sun() {
        // No hook is installed, so parse the helpers directly.
        let doc = cadcraft_doc::Drawing::new_metric();
        // sun false -> Off
        let off: serde_json::Value = json!(false);
        assert!(matches!(parse_sun(Some(&off), &doc), Ok(SunChoice::Off)), "sun false should parse to Off");
        // sun {date, time} without a site is refused
        let sun: serde_json::Value = json!({"date": "2026-06-21", "time": "12:00"});
        let r = parse_sun(Some(&sun), &doc);
        let e = r.unwrap_err().to_string();
        assert!(e.contains("sun needs a site location"), "{e}");
        // with a site the site sun is accepted
        let mut doc_with_site = doc.clone();
        doc_with_site.arch.site =
            Some(cadcraft_doc::arch::site::SiteLocation { latitude: 51.5, ..cadcraft_doc::arch::site::SiteLocation::default() });
        let r = parse_sun(Some(&sun), &doc_with_site);
        assert!(matches!(r, Ok(SunChoice::Dir(_))), "site sun should parse to a direction");
    }

    #[test]
    fn render_refuses_bad_size_and_too_many_samples() {
        // bad size: 0
        let s0: serde_json::Value = json!([0, 800]);
        assert!(matches!(parse_size(Some(&s0)), Err(_)), "size 0 should be refused");
        // bad size: > 8192
        let s1: serde_json::Value = json!([9000, 800]);
        assert!(matches!(parse_size(Some(&s1)), Err(_)), "size > 8192 should be refused");
        // default size
        assert!(matches!(parse_size(None), Ok((1200, 800))), "default size should be [1200, 800]");
        // too many samples
        let samples: Vec<serde_json::Value> = (0..65).map(|_| json!([0.0, 0.0, 0.0])).collect();
        let samples_json: serde_json::Value = json!(samples);
        assert!(matches!(parse_samples(Some(&samples_json)), Err(_)), "too many samples should be refused");
        // ok samples
        let ok: serde_json::Value = json!([[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]]);
        assert!(matches!(parse_samples(Some(&ok)), Ok(_)), "valid samples should be accepted");
    }
}
