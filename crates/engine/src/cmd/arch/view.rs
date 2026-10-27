//! Architecture 3D view commands.
//!
//! `view3d` builds and reports the architectural 3D scene. `camera`
//! validates and echoes the orbit camera parameters used by the 3D pane.

use serde_json::{Value, json};

use crate::{
    EngineError, Session,
    cmd::{CommandSpec, bad},
};

/// `view3d` and `camera` command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("view3d", "3D View", run_view3d).menu(&["Architecture", "3D View"]).params("{}").noundo(),
        CommandSpec::new("camera", "Camera", run_camera).params("{azimuth?, elevation?, distance?, target?}").noundo(),
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
fn run_camera(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineError, Session};

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
