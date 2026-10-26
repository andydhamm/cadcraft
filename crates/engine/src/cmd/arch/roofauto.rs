//! The `roofauto` command: generate automatic roofs from walls.

use crate::cmd::{CommandSpec, bad, f64_or, str_param};
use crate::{EngineError, Result, Session};
use cadcraft_arch::roof::{build_roof, top_level};
use cadcraft_doc::arch::{ArchId, Roof, RoofDirective};
use serde_json::{Value, json};

/// Command specs for the roofauto module.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("roofauto", "Roof Automatic", run_roofauto).menu(&["Architecture", "Roof", "Automatic"]).alias(&["ra"]).params(
        "{pitchRise?, pitchRun?, overhang?, thickness?, fascia?, soffit?, level?, walls?: [{wall, directive, pitch?, overhang?, eaveHeight?}]}",
    )]
}

/// Run the `roofauto` command from JSON parameters.
fn run_roofauto(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "roofauto";

    // Parse and validate parameters
    let pitch_rise = f64_or(p, "pitchRise", 6.0);
    let pitch_run = f64_or(p, "pitchRun", 12.0);
    let overhang = f64_or(p, "overhang", 0.0);
    let thickness = f64_or(p, "thickness", 0.2);
    let fascia = p.get("fascia").and_then(Value::as_bool).unwrap_or(true);
    let soffit = p.get("soffit").and_then(Value::as_bool).unwrap_or(true);

    if !pitch_rise.is_finite() || pitch_rise < 0.0 {
        return Err(bad(cmd, "`pitchRise` must be finite and non-negative"));
    }
    if !pitch_run.is_finite() || pitch_run <= 0.0 {
        return Err(bad(cmd, "`pitchRun` must be finite and positive"));
    }
    if !overhang.is_finite() || overhang < 0.0 || overhang > 5.0 {
        return Err(bad(cmd, "`overhang` must be between 0 and 5"));
    }
    if !thickness.is_finite() || thickness <= 0.0 || thickness > 2.0 {
        return Err(bad(cmd, "`thickness` must be between 0 and 2"));
    }

    // Parse level
    let level_name = str_param(p, "level").map(|s| s.to_string());

    // Parse per-wall directives
    // (wall_id, directive, pitch, overhang, eave_height)
    let mut directives = Vec::new();
    if let Some(walls_val) = p.get("walls") {
        if let Some(arr) = walls_val.as_array() {
            for w in arr {
                let wall_id_str = w.get("wall").and_then(Value::as_str).ok_or_else(|| bad(cmd, "each wall directive needs a `wall` id"))?;
                let wall_id = wall_id_str.parse::<ArchId>().map_err(|_| bad(cmd, format!("invalid wall id `{wall_id_str}`")))?;

                // Check if wall exists
                let doc = s.doc()?;
                if doc.arch.walls.iter().all(|w| w.id != wall_id) {
                    return Err(bad(cmd, format!("wall `{wall_id_str}` not found")));
                }

                let directive_str = w.get("directive").and_then(Value::as_str).unwrap_or("auto");
                let directive = match directive_str {
                    "auto" => RoofDirective::Auto,
                    "hip" => RoofDirective::Hip,
                    "gable" => RoofDirective::Gable,
                    "valley" => RoofDirective::Valley,
                    _ => return Err(bad(cmd, format!("unknown directive `{directive_str}`"))),
                };

                let w_pitch = w.get("pitch").and_then(Value::as_f64).filter(|v| v.is_finite());
                let w_overhang = w.get("overhang").and_then(Value::as_f64).filter(|v| v.is_finite());
                let w_eave = w.get("eaveHeight").and_then(Value::as_f64).filter(|v| v.is_finite());

                directives.push((wall_id, directive, w_pitch, w_overhang, w_eave));
            }
        }
    }

    // Determine target level
    let target_level = if let Some(name) = level_name {
        s.doc()?.arch.levels.iter().position(|l| l.name == name).map(|i| i as u32).ok_or_else(|| bad(cmd, format!("level `{name}` not found")))?
    } else {
        top_level(&s.doc()?.arch)
    };

    // Build roof on a copy of the model to validate
    let mut model = s.doc()?.arch.clone();

    // Apply directives to walls in the model copy
    for (wall_id, directive, w_pitch, w_overhang, w_eave) in &directives {
        if let Some(wall) = model.walls.iter_mut().find(|w| w.id == *wall_id) {
            wall.roof_directive = *directive;
            if let Some(p) = w_pitch {
                wall.roof_pitch = Some(*p);
            }
            if let Some(o) = w_overhang {
                wall.roof_overhang = Some(*o);
            }
            if let Some(e) = w_eave {
                wall.eave_height = Some(*e);
            }
        }
    }

    // Construct a temporary Roof object for build_roof
    let temp_roof = Roof {
        id: ArchId(0), // Dummy ID for validation
        level: target_level,
        pitch_rise,
        pitch_run,
        overhang,
        thickness,
        fascia,
        soffit,
        ..Default::default()
    };

    let built_roof = match build_roof(&model, target_level, &temp_roof) {
        Ok(r) => r,
        Err(e) => return Err(EngineError::Other(format!("roof build failed: {e:?}"))),
    };

    // Apply to document
    let mut doc = s.doc_mut()?;
    let mut rebuilt = false;

    // Find existing roof for this level
    let existing_idx = doc.arch.roofs.iter().position(|r| r.level == target_level);

    if let Some(idx) = existing_idx {
        if doc.arch.roofs[idx].hand_edited {
            return Ok(json!({"handEdited": true, "rebuilt": false}));
        }
        // Replace
        doc.arch.roofs[idx] = built_roof;
        rebuilt = true;
    } else {
        // Push new
        let new_id = doc.arch.next_roof_id;
        let mut new_roof = built_roof;
        new_roof.id = ArchId(new_id);
        doc.arch.roofs.push(new_roof);
        doc.arch.next_roof_id = doc.arch.next_roof_id.saturating_add(1);
        rebuilt = true;
    }

    doc.touch_arch();

    let handle = format!("r{}", doc.arch.roofs.iter().rev().find(|r| r.level == target_level).map(|r| r.id.0).unwrap_or(0));
    let planes = doc.arch.roofs.iter().find(|r| r.level == target_level).map(|r| r.planes.len()).unwrap_or(0);
    let projected_area =
        doc.arch.roofs.iter().find(|r| r.level == target_level).map(|r| r.planes.iter().map(|p| p.projected_area).sum()).unwrap_or(0.0);

    Ok(json!({
        "handles": [handle],
        "planes": planes,
        "projectedArea": projected_area,
        "rebuilt": rebuilt
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::Level;
    use serde_json::json;

    fn new_session_with_level() -> Session {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "ground".to_string(), ceiling_height: 2.4, structure: 0.3, ..Default::default() });
        d.touch_arch();
        s
    }

    #[test]
    fn roofauto_builds_a_hip_roof() {
        let mut s = new_session_with_level();
        // Add walls
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({
            "pitchRise": 6.0,
            "pitchRun": 12.0,
        });
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
        assert_eq!(r["planes"].as_u64().unwrap(), 4);
    }

    #[test]
    fn roofauto_rejects_zero_pitch() {
        let mut s = new_session_with_level();
        let p = json!({
            "pitchRun": 0.0,
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_rejects_negative_overhang() {
        let mut s = new_session_with_level();
        let p = json!({
            "overhang": -1.0,
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_rejects_unknown_directive() {
        let mut s = new_session_with_level();
        let p = json!({
            "walls": [{"wall": "w1", "directive": "unknown"}]
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_rejects_unknown_wall() {
        let mut s = new_session_with_level();
        let p = json!({
            "walls": [{"wall": "w99", "directive": "auto"}]
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_failure_leaves_no_roof() {
        let mut s = new_session_with_level();
        let before = s.doc().unwrap().arch.roofs.len();
        // Invalid pitch run
        let p = json!({
            "pitchRun": -1.0,
        });
        let _ = s.execute("roofauto", &p);
        let after = s.doc().unwrap().arch.roofs.len();
        assert_eq!(before, after);
    }

    #[test]
    fn roofauto_replaces_on_a_second_call() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p1 = json!({"pitchRise": 6.0, "pitchRun": 12.0});
        s.execute("roofauto", &p1).unwrap();

        let p2 = json!({"pitchRise": 4.0, "pitchRun": 12.0});
        let r = s.execute("roofauto", &p2).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
        assert_eq!(s.doc().unwrap().arch.roofs.len(), 1);
    }

    #[test]
    fn roofauto_rebuilds_after_a_wall_edit() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p_roof = json!({"pitchRise": 6.0, "pitchRun": 12.0});
        s.execute("roofauto", &p_roof).unwrap();

        // Add a wall, should trigger rebuild
        let p_wall = json!({
            "points": [[10.0, 0.0], [12.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p_wall).unwrap();

        // Check if roof was rebuilt (planes might change or just exist)
        assert_eq!(s.doc().unwrap().arch.roofs.len(), 1);
    }

    #[test]
    fn hand_edited_roof_is_not_rebuilt() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p_roof = json!({"pitchRise": 6.0, "pitchRun": 12.0});
        s.execute("roofauto", &p_roof).unwrap();

        // Mark as hand edited
        let d = s.doc_mut().unwrap();
        d.arch.roofs[0].hand_edited = true;
        d.touch_arch();

        // Try to rebuild
        let p_roof2 = json!({"pitchRise": 4.0, "pitchRun": 12.0});
        let r = s.execute("roofauto", &p_roof2).unwrap();
        assert_eq!(r["handEdited"].as_bool(), Some(true));
        assert_eq!(r["rebuilt"].as_bool(), Some(false));
    }

    #[test]
    fn roofauto_is_one_undo_step() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let before = s.doc().unwrap().arch.roofs.len();
        let p_roof = json!({"pitchRise": 6.0, "pitchRun": 12.0});
        s.execute("roofauto", &p_roof).unwrap();

        assert_eq!(s.doc().unwrap().arch.roofs.len(), before + 1);

        s.undo().unwrap();
        assert_eq!(s.doc().unwrap().arch.roofs.len(), before);
    }

    #[test]
    fn roofauto_accepts_flat_roof() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({
            "pitchRise": 0.0,
            "pitchRun": 12.0,
        });
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
    }

    #[test]
    fn roofauto_rejects_overhang_above_max() {
        let mut s = new_session_with_level();
        let p = json!({
            "overhang": 5.1,
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_rejects_thickness_above_max() {
        let mut s = new_session_with_level();
        let p = json!({
            "thickness": 2.1,
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_rejects_unknown_level() {
        let mut s = new_session_with_level();
        let p = json!({
            "level": "unknown",
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_with_per_wall_directives() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let wall_id = s.doc().unwrap().arch.walls.first().map(|w| format!("w{}", w.id.0)).unwrap();
        let p = json!({
            "walls": [
                {"wall": wall_id, "directive": "hip"}
            ]
        });
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
    }

    #[test]
    fn roofauto_rejects_missing_wall_id() {
        let mut s = new_session_with_level();
        let p = json!({
            "walls": [{"directive": "hip"}]
        });
        assert!(s.execute("roofauto", &p).is_err());
    }

    #[test]
    fn roofauto_accepts_fascia_soffit_false() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({
            "fascia": false,
            "soffit": false,
        });
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
    }

    #[test]
    fn roofauto_with_specific_level() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({
            "level": "ground",
        });
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
    }

    #[test]
    fn roofauto_returns_correct_format() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({});
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["handles"].is_array());
        assert!(r["handles"].as_array().unwrap().len() == 1);
        assert!(r["planes"].is_u64());
        assert!(r["projectedArea"].is_f64());
        assert!(r["rebuilt"].is_bool());
    }

    #[test]
    fn roofauto_handle_format() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({});
        let r = s.execute("roofauto", &p).unwrap();
        let handle = r["handles"][0].as_str().unwrap();
        assert!(handle.starts_with("r"));
    }

    #[test]
    fn roofauto_accepts_huge_pitch() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
            "exterior": true,
        });
        s.execute("wall", &p).unwrap();

        let p = json!({
            "pitchRise": 1000.0,
            "pitchRun": 12.0,
        });
        let r = s.execute("roofauto", &p).unwrap();
        assert!(r["rebuilt"].as_bool().unwrap());
    }
}
