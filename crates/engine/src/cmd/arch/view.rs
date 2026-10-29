//! Architecture 3D view commands.
//!
//! `view3d` builds and reports the architectural 3D scene. `camera`
//! validates and echoes the orbit camera parameters used by the 3D pane.

use serde_json::{Value, json};

use cadcraft_doc::arch::{Camera3d, CameraKind};

use crate::{
    EngineError, Session,
    cmd::{CommandSpec, bad},
};

/// Largest accepted orbit distance, in metres (matches the 3D pane's limit).
const MAX_DISTANCE: f64 = 1_000_000.0;
/// Walkthrough inputs at or beyond this magnitude are refused.
const MAX_WALK: f64 = 1e6;

/// `view3d`, `camera` and `walkthrough` command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("view3d", "3D View", run_view3d).menu(&["Architecture", "3D View"]).params("{}").noundo(),
        CommandSpec::new("camera", "Camera", run_camera).params(
            "{save:name, kind?:orbit|full, target?, azimuth?, elevation?, distance?, eye?, eyeHeight?, fov?} | {list:true} | {use:name} | {delete:name} | {azimuth?, elevation?, distance?, target?}",
        ),
        CommandSpec::new("walkthrough", "Walkthrough", run_walkthrough)
            .menu(&["Architecture", "Walkthrough"])
            .params("{eye,yaw,forward?,strafe?,turn?,step?}")
            .noundo(),
    ]
}

/// Build the 3D scene from the arch model and report its state.
///
/// Returns `{"ok":true,"meshes":N,"revision":rev}`. When the drawing carries
/// no arch walls, `meshes` is 0. The full scene build is deferred to the
/// render3d pane which re-renders when the revision changes.
fn run_view3d(s: &mut Session, _params: &Value) -> Result<Value, EngineError> {
    let doc = s.doc()?;
    let revision = doc.arch_revision;
    // Count meshes by building the scene (empty ArchModel yields 0).
    let meshes: usize = {
        let arch = &doc.arch;
        if arch.walls.is_empty() { 0 } else { cadcraft_arch3d::build(arch).meshes.len() }
    };
    Ok(json!({ "ok": true, "meshes": meshes, "revision": revision }))
}

/// Validate and echo the orbit camera.
///
/// Accepts optional `azimuth`, `elevation`, `distance` (radians / meters)
/// and an optional `target` (`[x,y,z]`). A present, non-null field must be
/// a finite number (`distance` in addition `> 0`), and `target` must be an
/// array of exactly three finite numbers; otherwise a `BadParams` error is
/// returned. Omitted or null fields take defaults (azimuth `0.0`, elevation
/// `0.35`, distance `10.0`, target the origin). Returns
/// `{"ok":true,"azimuth":..,"elevation":..,"distance":..,"target":[x,y,z]}`.
fn run_camera(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let present = |k: &str| !matches!(params.get(k), None | Some(Value::Null));
    if present("save") {
        return camera_save(s, params);
    }
    if present("use") {
        let name = name_param(params, "use")?;
        if s.doc()?.arch.camera_by_name(&name).is_none() {
            return Err(bad("camera", format!("no camera named `{name}`")));
        }
        let doc = s.doc_mut()?;
        doc.arch.active_camera = Some(name);
        doc.touch_arch();
        return cameras_json(s);
    }
    if present("delete") {
        let name = name_param(params, "delete")?;
        if s.doc()?.arch.camera_by_name(&name).is_none() {
            return Err(bad("camera", format!("no camera named `{name}`")));
        }
        let doc = s.doc_mut()?;
        doc.arch.remove_camera(&name);
        doc.touch_arch();
        return cameras_json(s);
    }
    if params.get("list").and_then(Value::as_bool) == Some(true) {
        return cameras_json(s);
    }
    let azimuth = match params.get("azimuth") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad("camera", "`azimuth` must be a finite number"))?,
    };
    let elevation = match params.get("elevation") {
        None | Some(Value::Null) => 0.35,
        Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad("camera", "`elevation` must be a finite number"))?,
    };
    let distance = match params.get("distance") {
        None | Some(Value::Null) => 10.0,
        Some(v) => {
            let d = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad("camera", "`distance` must be a finite number"))?;
            if d > 0.0 {
                d
            } else {
                return Err(bad("camera", "`distance` must be greater than 0"));
            }
        }
    };
    let target = match params.get("target") {
        None | Some(Value::Null) => (0.0, 0.0, 0.0),
        Some(v) => {
            let a = v.as_array().ok_or_else(|| bad("camera", "`target` must be an array of three numbers"))?;
            if a.len() != 3 {
                return Err(bad("camera", format!("`target` must have exactly 3 components (got {})", a.len())));
            }
            (
                a.get(0)
                    .and_then(Value::as_f64)
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| bad("camera", "`target` components must be finite numbers"))?,
                a.get(1)
                    .and_then(Value::as_f64)
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| bad("camera", "`target` components must be finite numbers"))?,
                a.get(2)
                    .and_then(Value::as_f64)
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| bad("camera", "`target` components must be finite numbers"))?,
            )
        }
    };

    // The command does not store the camera (the 3D pane owns it); it only
    // validates the parameters and echoes the result so scripts and the CLI
    // can query it.

    Ok(json!({
        "ok": true,
        "azimuth": azimuth,
        "elevation": elevation,
        "distance": distance,
        "target": [target.0, target.1, target.2],
    }))
}

/// A required string parameter (non-empty after trimming).
fn name_param(params: &Value, key: &str) -> Result<String, EngineError> {
    match params.get(key).and_then(Value::as_str) {
        Some(n) if !n.trim().is_empty() => Ok(n.to_string()),
        _ => Err(bad("camera", format!("`{key}` must be a non-empty camera name"))),
    }
}

/// An optional finite number.
fn opt_f64(params: &Value, key: &str) -> Result<Option<f64>, EngineError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|n| n.is_finite()).map(Some).ok_or_else(|| bad("camera", format!("`{key}` must be a finite number"))),
    }
}

/// An optional array of exactly `N` finite numbers.
fn opt_vec<const N: usize>(params: &Value, key: &str) -> Result<Option<[f64; N]>, EngineError> {
    let Some(v) = params.get(key).filter(|v| !v.is_null()) else { return Ok(None) };
    let msg = || bad("camera", format!("`{key}` must be an array of {N} finite numbers"));
    let a = v.as_array().filter(|a| a.len() == N).ok_or_else(msg)?;
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_f64().filter(|n| n.is_finite()).ok_or_else(msg)?;
    }
    Ok(Some(out))
}

/// Validate and store a named camera, and make it active.
fn camera_save(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = name_param(params, "save")?;
    let kind = match params.get("kind") {
        None | Some(Value::Null) => CameraKind::Orbit,
        Some(Value::String(k)) if k == "orbit" => CameraKind::Orbit,
        Some(Value::String(k)) if k == "full" => CameraKind::Full,
        Some(_) => return Err(bad("camera", "`kind` must be \"orbit\" or \"full\"")),
    };
    let mut cam = Camera3d { name, kind, ..Camera3d::default() };
    if let Some(t) = opt_vec::<3>(params, "target")? {
        cam.target = t;
    }
    if let Some(a) = opt_f64(params, "azimuth")? {
        cam.azimuth_deg = a;
    }
    if let Some(e) = opt_f64(params, "elevation")? {
        if !(-89.0..=89.0).contains(&e) {
            return Err(bad("camera", "`elevation` must be within [-89, 89] degrees"));
        }
        cam.elevation_deg = e;
    }
    if let Some(d) = opt_f64(params, "distance")? {
        if !(d > 0.0 && d <= MAX_DISTANCE) {
            return Err(bad("camera", format!("`distance` must be in (0, {MAX_DISTANCE}]")));
        }
        cam.distance = d;
    }
    let eye = opt_vec::<2>(params, "eye")?;
    if let Some(e) = eye {
        cam.eye = e;
    }
    if let Some(h) = opt_f64(params, "eyeHeight")? {
        cam.eye_height = h;
    }
    if let Some(f) = opt_f64(params, "fov")? {
        if !(f > 0.0 && f < 180.0) {
            return Err(bad("camera", "`fov` must be within (0, 180) degrees"));
        }
        cam.fov_deg = f;
    }
    if kind == CameraKind::Full {
        let Some(e) = eye else { return Err(bad("camera", "a `full` camera needs `eye`")) };
        if e == [cam.target[0], cam.target[1]] {
            return Err(bad("camera", "`eye` must differ from `target`"));
        }
    }
    let name = cam.name.clone();
    let doc = s.doc_mut()?;
    doc.arch.set_camera(cam).map_err(|e| bad("camera", format!("cannot save camera: {e}")))?;
    doc.arch.active_camera = Some(name);
    doc.touch_arch();
    cameras_json(s)
}

/// `{"ok":true,"cameras":[…],"active":name|null}` for the current drawing.
fn cameras_json(s: &Session) -> Result<Value, EngineError> {
    let arch = &s.doc()?.arch;
    let cams: Vec<Value> = arch
        .cameras
        .iter()
        .map(|c| {
            let kind = if c.kind == CameraKind::Full { "full" } else { "orbit" };
            json!({
                "name": c.name,
                "kind": kind,
                "target": c.target,
                "azimuth": c.azimuth_deg,
                "elevation": c.elevation_deg,
                "distance": c.distance,
                "eye": c.eye,
                "eyeHeight": c.eye_height,
                "fov": c.fov_deg,
            })
        })
        .collect();
    Ok(json!({ "ok": true, "cameras": cams, "active": arch.active_camera }))
}

/// A finite number with magnitude below [`MAX_WALK`]; `default` when absent.
fn walk_num(params: &Value, key: &str, default: Option<f64>) -> Result<f64, EngineError> {
    let v = match params.get(key) {
        None | Some(Value::Null) => default.ok_or_else(|| bad("walkthrough", format!("`{key}` is required")))?,
        Some(v) => v.as_f64().ok_or_else(|| bad("walkthrough", format!("`{key}` must be a number")))?,
    };
    if !v.is_finite() || v.abs() >= MAX_WALK {
        return Err(bad("walkthrough", format!("`{key}` must be finite with magnitude below {MAX_WALK}")));
    }
    Ok(v)
}

/// Pure walkthrough calculator: move `forward`/`strafe` metres (times `step`)
/// from `eye` facing `yaw` (degrees, 0 = +Y, increasing toward +X), apply
/// `turn`, and return the new `eye` and normalised `yaw` in (-180, 180].
/// Never reads or writes the drawing.
fn run_walkthrough(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let eye = match params.get("eye").and_then(Value::as_array) {
        Some(a) if a.len() == 3 => {
            let mut e = [0.0; 3];
            for (o, x) in e.iter_mut().zip(a) {
                *o = x
                    .as_f64()
                    .filter(|n| n.is_finite() && n.abs() < MAX_WALK)
                    .ok_or_else(|| bad("walkthrough", "`eye` components must be finite numbers below 1e6"))?;
            }
            e
        }
        _ => return Err(bad("walkthrough", "`eye` must be an array of three numbers")),
    };
    let yaw = walk_num(params, "yaw", None)?;
    let forward = walk_num(params, "forward", Some(0.0))?;
    let strafe = walk_num(params, "strafe", Some(0.0))?;
    let turn = walk_num(params, "turn", Some(0.0))?;
    let step = walk_num(params, "step", Some(1.0))?;
    if !(step > 0.0 && step <= 100.0) {
        return Err(bad("walkthrough", "`step` must be in (0, 100]"));
    }
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (f, r) = (forward * step, strafe * step);
    let x = eye[0] + f * sy + r * cy;
    let y = eye[1] + f * cy - r * sy;
    let mut n = (yaw + turn) % 360.0;
    if n > 180.0 {
        n -= 360.0;
    } else if n <= -180.0 {
        n += 360.0;
    }
    Ok(json!({ "ok": true, "eye": [x, y, eye[2]], "yaw": n }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineError, Session};

    fn cam_count(s: &Session) -> usize {
        s.doc().unwrap().arch.cameras.len()
    }

    #[test]
    fn camera_save_list_use_and_delete() {
        let mut s = Session::new();
        let r = s.execute("camera", &json!({ "save": "front", "azimuth": 10.0, "distance": 20.0 })).unwrap();
        assert_eq!(r["active"], json!("front"));
        assert_eq!(r["cameras"][0]["distance"], json!(20.0));
        s.execute("camera", &json!({ "save": "plan", "kind": "full", "eye": [5.0, 5.0], "fov": 60.0 })).unwrap();
        let l = s.execute("camera", &json!({ "list": true })).unwrap();
        assert_eq!(l["cameras"].as_array().unwrap().len(), 2);
        assert_eq!(l["cameras"][1]["kind"], json!("full"));
        assert_eq!(l["active"], json!("plan"));
        let u = s.execute("camera", &json!({ "use": "front" })).unwrap();
        assert_eq!(u["active"], json!("front"));
        assert!(s.execute("camera", &json!({ "use": "nope" })).is_err());
        let d = s.execute("camera", &json!({ "delete": "front" })).unwrap();
        assert_eq!(d["active"], json!(null));
        assert_eq!(cam_count(&s), 1);
        assert!(s.execute("camera", &json!({ "delete": "front" })).is_err());
        s.execute("u", &json!({})).unwrap();
        assert_eq!(cam_count(&s), 2);
    }

    #[test]
    fn camera_list_does_not_mutate_the_document() {
        let mut s = Session::new();
        s.execute("camera", &json!({ "save": "a" })).unwrap();
        let rev = s.doc().unwrap().arch_revision;
        let cams = s.doc().unwrap().arch.cameras.clone();
        s.execute("camera", &json!({ "list": true })).unwrap();
        assert_eq!(s.doc().unwrap().arch_revision, rev);
        assert_eq!(s.doc().unwrap().arch.cameras, cams);
    }

    #[test]
    fn camera_save_refuses_duplicate_and_non_finite() {
        let mut s = Session::new();
        s.execute("camera", &json!({ "save": "a" })).unwrap();
        for p in [
            json!({ "save": "a" }),
            json!({ "save": "  " }),
            json!({ "save": "b", "distance": 0.0 }),
            json!({ "save": "b", "distance": 1e7 }),
            json!({ "save": "b", "elevation": 90.0 }),
            json!({ "save": "b", "fov": 180.0 }),
            json!({ "save": "b", "fov": 0.0 }),
            json!({ "save": "b", "azimuth": "x" }),
            json!({ "save": "b", "target": [1.0, "nan", 0.0] }),
            json!({ "save": "b", "kind": "full" }),
            json!({ "save": "b", "kind": "full", "eye": [0.0, 0.0] }),
            json!({ "save": "b", "kind": "weird" }),
        ] {
            let r = s.execute("camera", &p);
            assert!(matches!(r, Err(EngineError::BadParams { .. })), "{p:?} -> {r:?}");
        }
        assert_eq!(cam_count(&s), 1);
    }

    #[test]
    fn bare_camera_still_echoes_the_orbit() {
        let mut s = Session::new();
        let r = s.execute("camera", &json!({ "azimuth": 2.0 })).unwrap();
        assert_eq!(r["azimuth"], json!(2.0));
        assert_eq!(r["distance"], json!(10.0));
        assert_eq!(cam_count(&s), 0);
    }

    #[test]
    fn walkthrough_forward_and_strafe_are_orthogonal() {
        let mut s = Session::new();
        let r = s.execute("walkthrough", &json!({ "eye": [1.0, 2.0, 1.6], "yaw": 0.0, "forward": 2.0 })).unwrap();
        assert!((r["eye"][0].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert!((r["eye"][1].as_f64().unwrap() - 4.0).abs() < 1e-9);
        assert_eq!(r["eye"][2], json!(1.6));
        let r = s.execute("walkthrough", &json!({ "eye": [0.0, 0.0, 0.0], "yaw": 0.0, "strafe": 3.0 })).unwrap();
        assert!((r["eye"][0].as_f64().unwrap() - 3.0).abs() < 1e-9);
        assert!(r["eye"][1].as_f64().unwrap().abs() < 1e-9);
        let r = s.execute("walkthrough", &json!({ "eye": [0.0, 0.0, 0.0], "yaw": 90.0, "forward": 1.0, "step": 2.0 })).unwrap();
        assert!((r["eye"][0].as_f64().unwrap() - 2.0).abs() < 1e-9);
        assert!(r["eye"][1].as_f64().unwrap().abs() < 1e-9);
    }

    #[test]
    fn walkthrough_turn_adds_to_yaw_and_normalises() {
        let mut s = Session::new();
        let r = s.execute("walkthrough", &json!({ "eye": [0, 0, 0], "yaw": 170.0, "turn": 20.0 })).unwrap();
        assert!((r["yaw"].as_f64().unwrap() + 170.0).abs() < 1e-9);
        let r = s.execute("walkthrough", &json!({ "eye": [0, 0, 0], "yaw": -170.0, "turn": -10.0 })).unwrap();
        assert!((r["yaw"].as_f64().unwrap() - 180.0).abs() < 1e-9);
    }

    #[test]
    fn walkthrough_refuses_huge_steps() {
        let mut s = Session::new();
        for p in [
            json!({ "eye": [0, 0, 0], "yaw": 0.0, "forward": 1e6 }),
            json!({ "eye": [0, 0, 0], "yaw": 0.0, "strafe": -1e9 }),
            json!({ "eye": [0, 0, 0], "yaw": 0.0, "step": 101.0 }),
            json!({ "eye": [0, 0, 0], "yaw": 0.0, "step": 0.0 }),
            json!({ "eye": [1e6, 0, 0], "yaw": 0.0 }),
            json!({ "eye": [0, 0], "yaw": 0.0 }),
            json!({ "eye": [0, 0, 0] }),
            json!({ "eye": [0, 0, 0], "yaw": 1e6 }),
        ] {
            let r = s.execute("walkthrough", &p);
            assert!(matches!(r, Err(EngineError::BadParams { .. })), "{p:?} -> {r:?}");
        }
    }

    /// `view3d` on a fresh drawing (no walls) reports `ok` with zero meshes
    /// and revision 0.
    #[test]
    fn view3d_command_reports_the_scene() {
        let mut s = Session::new();
        let r = s.execute("view3d", &json!({})).unwrap();
        assert!(r["ok"].as_bool().unwrap());
        assert_eq!(r["meshes"].as_u64().unwrap(), 0);
        assert_eq!(r["revision"].as_u64().unwrap(), 0);
    }

    /// `camera` with explicit parameters echoes them; omitted fields fall
    /// back to the defaults.
    #[test]
    fn camera_command_echoes_the_orbit() {
        let mut s = Session::new();
        let r = s.execute("camera", &json!({ "azimuth": 1.5, "elevation": 0.3, "distance": 25.0 })).unwrap();
        assert!(r["ok"].as_bool().unwrap());
        assert_eq!(r["azimuth"].as_f64().unwrap(), 1.5);
        assert_eq!(r["elevation"].as_f64().unwrap(), 0.3);
        assert_eq!(r["distance"].as_f64().unwrap(), 25.0);

        // Defaults when no parameters are given.
        let r2 = s.execute("camera", &json!({})).unwrap();
        assert_eq!(r2["azimuth"].as_f64().unwrap(), 0.0);
        assert_eq!(r2["elevation"].as_f64().unwrap(), 0.35);
        assert_eq!(r2["distance"].as_f64().unwrap(), 10.0);
    }

    /// The command does not mutate any stored camera state (the 3D pane owns
    /// the orbit); it validates the request and echoes the effective orbit
    /// back to the caller. Supplied fields are returned verbatim; omitted or
    /// null fields fall back to the defaults (azimuth `0.0`, elevation `0.35`,
    /// distance `10.0`, target the origin).
    #[test]
    fn camera_command_updates_the_orbit() {
        let mut s = Session::new();

        // Supplied fields are echoed verbatim, including a three-component target.
        let r = s.execute("camera", &json!({ "azimuth": 0.9, "elevation": 0.5, "distance": 5.0, "target": [1.0, 2.0, 3.0] })).unwrap();
        assert!(r["ok"].as_bool().unwrap());
        assert_eq!(r["azimuth"].as_f64().unwrap(), 0.9);
        assert_eq!(r["elevation"].as_f64().unwrap(), 0.5);
        assert_eq!(r["distance"].as_f64().unwrap(), 5.0);
        assert_eq!(r["target"], json!([1.0, 2.0, 3.0]));

        // Null fields fall back to the defaults too.
        let r2 = s.execute("camera", &json!({ "distance": null })).unwrap();
        assert_eq!(r2["azimuth"].as_f64().unwrap(), 0.0);
        assert_eq!(r2["elevation"].as_f64().unwrap(), 0.35);
        assert_eq!(r2["distance"].as_f64().unwrap(), 10.0);
        assert_eq!(r2["target"], json!([0.0, 0.0, 0.0]));
    }

    /// `camera` rejects unusable values instead of defaulting: a non-positive or
    /// non-numeric `distance`, a non-numeric `azimuth`/`elevation`, and a
    /// `target` that is not an array of exactly three finite numbers each
    /// return `EngineError::BadParams`. A valid three-component `target` is
    /// echoed back.
    #[test]
    fn camera_command_rejects_unusable_values() {
        let mut s = Session::new();

        for p in [
            json!({ "distance": -5.0 }),
            json!({ "distance": 0.0 }),
            json!({ "azimuth": "east" }),
            json!({ "elevation": [1] }),
            json!({ "target": [1, 2] }),
            json!({ "target": [1, 2, "z"] }),
        ] {
            let r = s.execute("camera", &p);
            assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })), "camera({p:?}) should be rejected, got {r:?}");
        }

        let r = s.execute("camera", &json!({ "target": [1.0, 2.0, 3.0] })).unwrap();
        assert!(r["ok"].as_bool().unwrap());
        assert_eq!(r["target"], json!([1.0, 2.0, 3.0]));
    }

    /// Empty or oversized target arrays and non-numeric scalars (booleans,
    /// objects, string numbers) are rejected for every field.
    #[test]
    fn camera_command_rejects_malformed_targets_and_scalars() {
        let mut s = Session::new();

        for p in [
            json!({ "target": [] }),
            json!({ "target": [1.0, 2.0, 3.0, 4.0] }),
            json!({ "azimuth": true }),
            json!({ "elevation": false }),
            json!({ "distance": {} }),
            json!({ "azimuth": "1.5" }),
        ] {
            let r = s.execute("camera", &p);
            assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })), "camera({p:?}) should be rejected, got {r:?}");
        }
    }

    /// Zero is valid for azimuth and elevation (but not distance); very large
    /// finite values are accepted and echoed back.
    #[test]
    fn camera_command_accepts_zero_and_large_values() {
        let mut s = Session::new();

        let r = s.execute("camera", &json!({ "azimuth": 0.0, "elevation": 0.0, "distance": 5.0 })).unwrap();
        assert!(r["ok"].as_bool().unwrap());
        assert_eq!(r["azimuth"].as_f64().unwrap(), 0.0);
        assert_eq!(r["elevation"].as_f64().unwrap(), 0.0);

        let r2 = s.execute("camera", &json!({ "azimuth": 1e308, "elevation": 1e308, "distance": 1e308 })).unwrap();
        assert!(r2["ok"].as_bool().unwrap());
        assert_eq!(r2["azimuth"].as_f64().unwrap(), 1e308);
        assert_eq!(r2["distance"].as_f64().unwrap(), 1e308);
    }

    /// When any field is bad the whole call is rejected, even if the other
    /// fields are valid; a fully valid call with all four fields succeeds.
    #[test]
    fn camera_command_rejects_when_any_field_is_bad() {
        let mut s = Session::new();

        let r = s.execute("camera", &json!({ "azimuth": 0.5, "distance": -1.0 }));
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));

        let r2 = s.execute("camera", &json!({ "distance": 5.0, "target": [1.0] }));
        assert!(matches!(r2, Err(EngineError::BadParams { cmd: _, msg: _ })));

        let r3 = s.execute("camera", &json!({ "azimuth": 0.5, "elevation": 0.3, "distance": 5.0, "target": [1.0, 2.0, 3.0] })).unwrap();
        assert!(r3["ok"].as_bool().unwrap());
        assert_eq!(r3["azimuth"].as_f64().unwrap(), 0.5);
        assert_eq!(r3["elevation"].as_f64().unwrap(), 0.3);
        assert_eq!(r3["distance"].as_f64().unwrap(), 5.0);
        assert_eq!(r3["target"], json!([1.0, 2.0, 3.0]));
    }
}
