//! `pick3d`: cast a viewport pixel ray into the architectural 3D scene.
//!
//! Read-only: the scene is built from the arch model and never written back.

use serde_json::{Value, json};

use cadcraft_arch3d::Scene3d;
use cadcraft_arch3d::camera::{Camera, FullCamera, MAX_DISTANCE, Orbit, ray_at};
use cadcraft_doc::arch::{handle_json, part_handle};
use cadcraft_doc::{ArchId, opening_label, railing_label, roof_label, room_label, stair_label, wall_label};
use cadcraft_geom::Vec3;

use crate::{
    EngineError, Session,
    cmd::{CommandSpec, bad},
};

/// Largest accepted viewport edge, in pixels.
const MAX_SIZE: u64 = 8192;
/// Coordinates at or beyond this magnitude are refused.
const MAX_COORD: f64 = 1e9;

/// `pick3d` command spec.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("pick3d", "Pick 3D", run_pick3d).menu(&["Architecture", "Pick 3D"]).params("{camera,size:[w,h],pixel:[x,y]}").noundo()]
}

fn finite(v: &Value, what: &str) -> Result<f64, EngineError> {
    match v.as_f64() {
        Some(x) if x.is_finite() && x.abs() <= MAX_COORD => Ok(x),
        _ => Err(bad("pick3d", format!("`{what}` must be a finite number"))),
    }
}

/// Optional finite number field; absent/null yields `default`.
fn num(obj: &Value, key: &str, default: f64) -> Result<f64, EngineError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => finite(v, key),
    }
}

/// Parse a 3-element number array (or 2 when `allow2`).
fn vec_n(v: &Value, what: &str, allow2: bool) -> Result<(f64, f64, Option<f64>), EngineError> {
    let arr = v.as_array().ok_or_else(|| bad("pick3d", format!("`{what}` must be an array")))?;
    match arr.as_slice() {
        [x, y] if allow2 => Ok((finite(x, what)?, finite(y, what)?, None)),
        [x, y, z] => Ok((finite(x, what)?, finite(y, what)?, Some(finite(z, what)?))),
        _ => Err(bad("pick3d", format!("`{what}` must have {} numbers", if allow2 { "2 or 3" } else { "3" }))),
    }
}

fn vec3_field(obj: &Value, key: &str, default: Vec3) -> Result<Vec3, EngineError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let (x, y, z) = vec_n(v, key, false)?;
            Ok(Vec3::new(x, y, z.unwrap_or(0.0)))
        }
    }
}

fn parse_camera(cam: &Value) -> Result<Camera, EngineError> {
    if !cam.is_object() {
        return Err(bad("pick3d", "`camera` must be an object"));
    }
    let kind = match cam.get("kind") {
        None | Some(Value::Null) => "orbit",
        Some(v) => v.as_str().ok_or_else(|| bad("pick3d", "`camera.kind` must be a string"))?,
    };
    let fov = num(cam, "fov", 45.0)?;
    if fov <= 0.0 || fov >= 180.0 {
        return Err(bad("pick3d", "`fov` must be in (0, 180) degrees"));
    }
    match kind {
        "orbit" => {
            let distance = num(cam, "distance", 10.0)?;
            if distance <= 0.0 || distance > MAX_DISTANCE {
                return Err(bad("pick3d", format!("`distance` must be in (0, {MAX_DISTANCE}]")));
            }
            let target = vec3_field(cam, "target", Vec3::new(0.0, 0.0, 0.0))?;
            Ok(Camera::Orbit(Orbit::new(target, num(cam, "azimuth", 0.0)?, num(cam, "elevation", 30.0)?, distance, fov)))
        }
        "full" => {
            let target = vec3_field(cam, "target", Vec3::new(0.0, 0.0, 0.0))?;
            let eye_v = cam.get("eye").filter(|v| !v.is_null()).ok_or_else(|| bad("pick3d", "`full` camera needs `eye`"))?;
            let (x, y, z) = vec_n(eye_v, "eye", true)?;
            let z = match z {
                Some(z) => z,
                None => num(cam, "eyeHeight", 1.6)?,
            };
            let eye = Vec3::new(x, y, z);
            let (dx, dy, dz) = (eye.x - target.x, eye.y - target.y, eye.z - target.z);
            if (dx * dx + dy * dy + dz * dz).sqrt() < 1e-9 {
                return Err(bad("pick3d", "`eye` and `target` must differ"));
            }
            Ok(Camera::Full(FullCamera::new(eye, target, fov)))
        }
        other => Err(bad("pick3d", format!("unknown camera kind `{other}`"))),
    }
}

fn parse_size(v: Option<&Value>) -> Result<(u32, u32), EngineError> {
    let arr = v.and_then(Value::as_array).ok_or_else(|| bad("pick3d", "`size` must be [w,h]"))?;
    let dim = |x: Option<&Value>| match x.and_then(Value::as_u64) {
        Some(n) if (1..=MAX_SIZE).contains(&n) => Ok(n as u32),
        _ => Err(bad("pick3d", format!("`size` entries must be integers in 1..={MAX_SIZE}"))),
    };
    if arr.len() != 2 {
        return Err(bad("pick3d", "`size` must be [w,h]"));
    }
    Ok((dim(arr.first())?, dim(arr.get(1))?))
}

fn parse_pixel(v: Option<&Value>, size: (u32, u32)) -> Result<[f32; 2], EngineError> {
    let arr = v.and_then(Value::as_array).ok_or_else(|| bad("pick3d", "`pixel` must be [x,y]"))?;
    let (x, y) = match arr.as_slice() {
        [x, y] => (finite(x, "pixel")?, finite(y, "pixel")?),
        _ => return Err(bad("pick3d", "`pixel` must be [x,y]")),
    };
    if x < -1.0 || y < -1.0 || x > f64::from(size.0) + 1.0 || y > f64::from(size.1) + 1.0 {
        return Err(bad("pick3d", "`pixel` lies outside the viewport"));
    }
    Ok([x as f32, y as f32])
}

/// Label and proxy handle (lower-case hex, as `arch.inspect` reports) of a hit object.
fn label_and_handle(id: ArchId, part: &str) -> (String, String) {
    let label = match part {
        "floor" | "ceiling" | "deck" => room_label(id),
        "leaf" | "frame" | "glass" | "casing" | "sill" | "header" => opening_label(id),
        "roof" => roof_label(id),
        "stair" => stair_label(id),
        "railing" => railing_label(id),
        _ => wall_label(id),
    };
    (label, handle_json(part_handle(id, part)))
}

/// Cast the ray through `pixel` of a `size` viewport and report the nearest object hit.
fn run_pick3d(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let cam = params.get("camera").filter(|v| !v.is_null()).ok_or_else(|| bad("pick3d", "`camera` is required"))?;
    let camera = parse_camera(cam)?;
    let size = parse_size(params.get("size"))?;
    let pixel = parse_pixel(params.get("pixel"), size)?;
    let Some((origin, dir)) = ray_at(&camera, size, pixel) else {
        return Ok(json!({ "ok": true, "hit": null }));
    };
    let doc = s.doc()?;
    let scene = Scene3d::build(&doc.arch);
    Ok(match scene.pick(origin, dir) {
        None => json!({ "ok": true, "hit": null }),
        Some(h) => {
            let (id, handle) = label_and_handle(h.object, h.part);
            json!({ "ok": true, "hit": {
                "id": id, "handle": handle, "part": h.part, "distance": h.distance,
                "point": [h.point.x, h.point.y, h.point.z],
            } })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{Opening, OpeningKind, Wall};
    use cadcraft_geom::Vec2;

    fn session() -> Session {
        let mut s = Session::new();
        if let Ok(d) = s.doc_mut() {
            let wid =
                d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
            d.arch.push_opening(Opening::new(wid, 3.0, 0.9, 2.0, OpeningKind::Door));
            d.touch_arch();
        }
        s
    }

    fn params(camera: Value, pixel: Value) -> Value {
        json!({ "camera": camera, "size": [100, 100], "pixel": pixel })
    }

    fn front() -> Value {
        json!({ "kind": "full", "eye": [3.45, -5.0, 1.0], "target": [3.45, 0.0, 1.0], "fov": 45 })
    }

    #[test]
    fn pick3d_returns_the_door_id_and_handle() {
        let mut s = session();
        let r = s.execute("pick3d", &params(front(), json!([50, 50])));
        assert!(r.is_ok(), "{r:?}");
        let v = r.unwrap_or(Value::Null);
        let hit = &v["hit"];
        assert_eq!(hit["id"], "o0", "{v}");
        assert!(hit["handle"].as_str().is_some_and(|h| h.starts_with("a000")), "{v}");
        assert!(hit["distance"].as_f64().is_some_and(|d| d > 0.0));
        assert_eq!(hit["point"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn pick3d_on_the_door_reports_the_opening_handle_not_the_walls() {
        let mut s = session();
        let v = s.execute("pick3d", &params(front(), json!([50, 50]))).unwrap_or(Value::Null);
        let hit = &v["hit"];
        assert_eq!(hit["id"], "o0", "{v}");
        assert_eq!(hit["handle"], "a0008000", "{v}");
        assert_ne!(hit["handle"], "a0000000", "{v}");
    }

    #[test]
    fn pick3d_at_the_sky_returns_null() {
        let mut s = session();
        let cam = json!({ "kind": "full", "eye": [3.45, -5.0, 1.0], "target": [3.45, -4.0, 50.0] });
        let r = s.execute("pick3d", &params(cam, json!([50, 50])));
        assert!(r.is_ok(), "{r:?}");
        let v = r.unwrap_or(Value::Null);
        assert_eq!(v["ok"], true);
        assert!(v["hit"].is_null(), "{v}");
    }

    #[test]
    fn pick3d_refuses_a_degenerate_camera() {
        let mut s = session();
        let bads = [
            Value::Null,
            json!({ "kind": "full", "eye": [1, 1, 1], "target": [1, 1, 1] }),
            json!({ "kind": "orbit", "fov": 0 }),
            json!({ "kind": "orbit", "fov": 180 }),
            json!({ "kind": "orbit", "distance": 0 }),
            json!({ "kind": "orbit", "distance": 1e7 }),
            json!({ "kind": "orbit", "azimuth": "x" }),
            json!({ "kind": "spiral" }),
        ];
        for c in bads {
            assert!(s.execute("pick3d", &params(c.clone(), json!([50, 50]))).is_err(), "accepted {c}");
        }
    }

    #[test]
    fn pick3d_refuses_pixels_outside_the_viewport() {
        let mut s = session();
        for px in [json!([-5, 50]), json!([50, 200]), json!([101.5, 50]), json!([50]), json!(["a", 1])] {
            assert!(s.execute("pick3d", &params(front(), px.clone())).is_err(), "accepted {px}");
        }
        for sz in [json!([0, 10]), json!([10, 9000]), json!([1.5, 10]), json!([10])] {
            let p = json!({ "camera": front(), "size": sz, "pixel": [1, 1] });
            assert!(s.execute("pick3d", &p).is_err());
        }
    }

    #[test]
    fn pick3d_never_mutates_the_document() {
        let mut s = session();
        let rev = s.doc().map(|d| d.arch_revision).unwrap_or(0);
        let walls = s.doc().map(|d| d.arch.walls.len()).unwrap_or(0);
        let _ = s.execute("pick3d", &params(front(), json!([50, 50])));
        let _ = s.execute("pick3d", &params(front(), json!([500, 50])));
        assert_eq!(s.doc().map(|d| d.arch_revision).unwrap_or(1), rev);
        assert_eq!(s.doc().map(|d| d.arch.walls.len()).unwrap_or(9), walls);
    }
}
