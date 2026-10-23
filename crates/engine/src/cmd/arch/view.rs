//! Architecture 3D view commands.
//!
//! `view3d` builds and reports the architectural 3D scene. `camera` sets
//! (or reads) the orbit camera parameters used by the 3D pane.

use serde_json::{Value, json};

use crate::EngineError;
use crate::cmd::CommandSpec;

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
    let revision = doc.arch_revision();
    // Count meshes by building the scene (empty ArchModel yields 0).
    let meshes: usize = {
        let arch = &doc.arch;
        if arch.walls().is_empty() { 0 } else { crate::arch3d::build(arch).meshes.len() }
    };
    Ok(json!({ "ok": true, "meshes": meshes, "revision": revision }))
}

/// Set (or read) the orbit camera.
///
/// Accepts optional `azimuth`, `elevation`, `distance` (radians / meters)
/// and an optional `target` (`[x,y,z]`). Omitted fields keep their current
/// values. Always returns `{"ok":true,"azimuth":..,"elevation":..,
/// "distance":..,"target":[x,y,z]}`.
fn run_camera(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let azimuth = params.get("azimuth").and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or_default();
    let elevation = params.get("elevation").and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(0.35);
    let distance = params.get("distance").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(10.0);
    let target = params
        .get("target")
        .and_then(|t| {
            t.as_array().and_then(|a| {
                a.get(0)
                    .and_then(Value::as_f64)
                    .filter(|x| x.is_finite())
                    .zip(a.get(1).and_then(Value::as_f64).filter(|y| y.is_finite()))
                    .zip(a.get(2).and_then(Value::as_f64).filter(|z| z.is_finite()))
                    .map(|((x, y), z)| (x, y, z))
            })
        })
        .unwrap_or((0.0, 0.0, 0.0));

    // Persist the camera on the drawing so the pane picks it up on next build.
    s.doc()?.set_arch_camera(azimuth, elevation, distance, target);

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

    /// `camera` with explicit parameters returns them and sets the pane's
    /// orbit; omitted fields fall back to the defaults.
    #[test]
    fn camera_command_updates_the_orbit() {
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
}
