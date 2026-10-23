//! Architecture 3D view commands.
//!
//! `view3d` builds and reports the architectural 3D scene. `camera` sets
//! (or reads) the orbit camera parameters used by the 3D pane.

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

/// Validate and echo orbit camera parameters.
///
/// Accepts optional `azimuth`, `elevation`, `distance` (radians / meters) and an optional
/// `target` (`[x,y,z]`); omitted fields take the defaults (0, 0.35, 10, origin). A field
/// that is present but unusable (not a finite number, a non-positive distance, a target
/// that is not three finite numbers) is an error, never silently replaced by the default.
/// Returns `{"ok":true,"azimuth":..,"elevation":..,"distance":..,"target":[x,y,z]}`.
///
/// The engine holds no camera: the 3D pane owns its orbit in `ui-egui`, so this does not
/// move the pane's view yet (see docs/arch3d/REVIEW-2026-10-23.md).
fn run_camera(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let num = |key: &str, default: f64| -> Result<f64, EngineError> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("camera", format!("`{key}` must be a finite number"))),
        }
    };
    let azimuth = num("azimuth", 0.0)?;
    let elevation = num("elevation", 0.35)?;
    let distance = num("distance", 10.0)?;
    if distance <= 0.0 {
        return Err(bad("camera", "`distance` must be positive"));
    }
    let target = match params.get("target") {
        None | Some(Value::Null) => [0.0, 0.0, 0.0],
        Some(t) => {
            let xs: Option<Vec<f64>> =
                t.as_array().filter(|a| a.len() == 3).map(|a| a.iter().filter_map(Value::as_f64).filter(|x| x.is_finite()).collect());
            match xs.as_deref() {
                Some([x, y, z]) => [*x, *y, *z],
                _ => return Err(bad("camera", "`target` must be [x, y, z] of finite numbers")),
            }
        }
    };

    Ok(json!({
        "ok": true,
        "azimuth": azimuth,
        "elevation": elevation,
        "distance": distance,
        "target": target,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

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

    /// `camera` with explicit parameters echoes them; omitted fields fall back to the
    /// defaults. (It does not reach the pane's orbit: the engine holds no camera.)
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

    /// A present but unusable value is an error, not a silent default (it used to turn
    /// `distance: -5` into 10 and `target: [1, 2]` into the origin).
    #[test]
    fn camera_command_rejects_unusable_values() {
        let mut s = Session::new();
        for p in [
            json!({ "distance": -5.0 }),
            json!({ "distance": 0.0 }),
            json!({ "azimuth": "east" }),
            json!({ "elevation": [1] }),
            json!({ "target": [1.0, 2.0] }),
            json!({ "target": [1.0, 2.0, "z"] }),
        ] {
            assert!(matches!(s.execute("camera", &p), Err(EngineError::BadParams { .. })), "{p} must be refused");
        }
        let r = s.execute("camera", &json!({ "target": [1.0, 2.0, 3.0] })).unwrap();
        assert_eq!(r["target"], json!([1.0, 2.0, 3.0]));
    }
}
