//! The `roofplane` command: create or edit a single manual roof plane.

use cadcraft_arch::poly::is_simple;
use cadcraft_doc::arch::{Roof, RoofEdge, RoofEdgeKind, RoofPlane};
use cadcraft_geom::Vec2;
use serde_json::Value;

use crate::Session;
use crate::cmd::{Result, bad, f64_or, points_param};

/// Parse a roof plane ID like "r0p1" into (roof_index, plane_index).
fn parse_roof_plane_id(id: &str) -> Option<(usize, usize)> {
    let rest = id.strip_prefix('r')?;
    let (r_str, p_str) = rest.split_once('p')?;
    let r = r_str.parse::<usize>().ok()?;
    let p = p_str.parse::<usize>().ok()?;
    Some((r, p))
}

/// Create a new roof plane from points and a base edge.
fn create_roof_plane(
    s: &mut Session,
    points: Vec<Vec2>,
    base: (Vec2, Vec2),
    pitch_rise: f64,
    pitch_run: f64,
    thickness: f64,
    fascia: f64,
    soffit: f64,
    level: Option<usize>,
) -> Result<Value> {
    if points.len() < 3 || points.len() > 256 {
        return Err(bad("roofplane", "points must be between 3 and 256"));
    }
    if !points.iter().all(|p| p.is_finite()) {
        return Err(bad("roofplane", "points must be finite"));
    }
    if !is_simple(&points) {
        return Err(bad("roofplane", "polygon is self-crossing"));
    }
    if !base.0.is_finite() || !base.1.is_finite() {
        return Err(bad("roofplane", "base points must be finite"));
    }
    if base.0.near(base.1, 1e-6) {
        return Err(bad("roofplane", "base edge is degenerate"));
    }
    if pitch_run <= 0.0 || !pitch_run.is_finite() {
        return Err(bad("roofplane", "pitchRun must be positive and finite"));
    }
    if !pitch_rise.is_finite() {
        return Err(bad("roofplane", "pitchRise must be finite"));
    }
    if thickness < 0.0 || !thickness.is_finite() {
        return Err(bad("roofplane", "thickness must be non-negative and finite"));
    }
    if fascia < 0.0 || !fascia.is_finite() {
        return Err(bad("roofplane", "fascia must be non-negative and finite"));
    }
    if soffit < 0.0 || !soffit.is_finite() {
        return Err(bad("roofplane", "soffit must be non-negative and finite"));
    }

    // Check if base lies on one of the polygon's edges (1e-6 tolerance)
    let mut on_edge = false;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let ab = b - a;
        let len_sq = ab.len2();
        if len_sq < 1e-12 {
            continue;
        }

        let is_on_segment = |p: Vec2| -> bool {
            let ap = p - a;
            let t = ap.dot(ab) / len_sq;
            if t < -1e-6 || t > 1.0 + 1e-6 {
                return false;
            }
            let proj = a + ab * t.clamp(0.0, 1.0);
            p.near(proj, 1e-6)
        };

        if is_on_segment(base.0) && is_on_segment(base.1) {
            on_edge = true;
            break;
        }
    }

    if !on_edge {
        return Err(bad("roofplane", "base edge does not lie on a polygon edge"));
    }

    let slope = pitch_rise / pitch_run;

    let mut area = 0.0;
    for i in 0..points.len() {
        let j = (i + 1) % points.len();
        area += points[i].x * points[j].y - points[j].x * points[i].y;
    }
    let projected_area = area.abs() / 2.0;

    let mut ring = points.clone();
    if area < 0.0 {
        ring.reverse();
    }

    let base_edge = [base.0.x, base.0.y, base.1.x, base.1.y];
    let plane = RoofPlane { ring, slope, projected_area, base_edge, thickness, ..Default::default() };

    let edge = RoofEdge { kind: RoofEdgeKind::Eave, a: base.0, b: base.1, ..Default::default() };

    let mut doc = s.doc_mut()?;
    let arch = &mut doc.arch;
    let id = arch.next_roof_id;
    arch.next_roof_id += 1;

    let roof = Roof {
        id: cadcraft_doc::ArchId(id),
        level: level.unwrap_or(0) as u32,
        pitch_rise,
        pitch_run,
        thickness,
        fascia: fascia > 0.0,
        soffit: soffit > 0.0,
        manual: true,
        hand_edited: true,
        planes: vec![plane],
        edges: vec![edge],
        ..Default::default()
    };

    arch.roofs.push(roof);
    doc.touch_arch();

    let handle = format!("r{}", id);
    Ok(serde_json::json!({
        "handles": [handle],
        "plane": 0,
        "projectedArea": projected_area
    }))
}

/// Edit an existing roof plane.
fn edit_roof_plane(
    s: &mut Session,
    id_str: &str,
    pitch_rise: Option<f64>,
    pitch_run: Option<f64>,
    thickness: Option<f64>,
    fascia: Option<f64>,
    soffit: Option<f64>,
) -> Result<Value> {
    let (r_idx, p_idx) = parse_roof_plane_id(id_str).ok_or_else(|| bad("roofplane", format!("invalid roof plane id: {}", id_str)))?;

    let mut doc = s.doc_mut()?;
    let arch = &mut doc.arch;

    let roof = arch.roofs.get_mut(r_idx).ok_or_else(|| bad("roofplane", format!("roof r{} not found", r_idx)))?;

    let plane = roof.planes.get_mut(p_idx).ok_or_else(|| bad("roofplane", format!("plane p{} not found", p_idx)))?;

    let mut changed = false;

    if let Some(v) = pitch_rise {
        if !v.is_finite() {
            return Err(bad("roofplane", "pitchRise must be finite"));
        }
        if roof.pitch_rise != v {
            roof.pitch_rise = v;
            changed = true;
        }
    }
    if let Some(v) = pitch_run {
        if v <= 0.0 || !v.is_finite() {
            return Err(bad("roofplane", "pitchRun must be positive and finite"));
        }
        if roof.pitch_run != v {
            roof.pitch_run = v;
            changed = true;
        }
    }

    if pitch_rise.is_some() || pitch_run.is_some() {
        let new_slope = roof.pitch_rise / roof.pitch_run;
        if (plane.slope - new_slope).abs() > 1e-9 {
            plane.slope = new_slope;
            changed = true;
        }
    }

    if let Some(v) = thickness {
        if v < 0.0 || !v.is_finite() {
            return Err(bad("roofplane", "thickness must be non-negative and finite"));
        }
        if plane.thickness != v {
            plane.thickness = v;
            roof.thickness = v;
            changed = true;
        }
    }
    if let Some(v) = fascia {
        if v < 0.0 || !v.is_finite() {
            return Err(bad("roofplane", "fascia must be non-negative and finite"));
        }
        let has_fascia = v > 0.0;
        if roof.fascia != has_fascia {
            roof.fascia = has_fascia;
            changed = true;
        }
    }
    if let Some(v) = soffit {
        if v < 0.0 || !v.is_finite() {
            return Err(bad("roofplane", "soffit must be non-negative and finite"));
        }
        let has_soffit = v > 0.0;
        if roof.soffit != has_soffit {
            roof.soffit = has_soffit;
            changed = true;
        }
    }

    if !changed {
        return Err(bad("roofplane", "no changes specified"));
    }

    roof.hand_edited = true;
    let projected_area = plane.projected_area;
    doc.touch_arch();

    let handle = format!("r{}", r_idx);
    Ok(serde_json::json!({
        "handles": [handle],
        "plane": p_idx,
        "projectedArea": projected_area
    }))
}

/// Run the `roofplane` command.
pub fn run_roofplane(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(edit_id) = p.get("edit").and_then(Value::as_str) {
        let pitch_rise = p.get("pitchRise").and_then(Value::as_f64);
        let pitch_run = p.get("pitchRun").and_then(Value::as_f64);
        let thickness = p.get("thickness").and_then(Value::as_f64);
        let fascia = p.get("fascia").and_then(Value::as_f64);
        let soffit = p.get("soffit").and_then(Value::as_f64);
        return edit_roof_plane(s, edit_id, pitch_rise, pitch_run, thickness, fascia, soffit);
    }

    let points = points_param(p, "points").ok_or_else(|| bad("roofplane", "`points` is required"))?;

    let base_val = p.get("base").ok_or_else(|| bad("roofplane", "`base` is required"))?;

    let base_arr = base_val.as_array().ok_or_else(|| bad("roofplane", "`base` must be an array of two points"))?;

    if base_arr.len() != 2 {
        return Err(bad("roofplane", "`base` must have exactly two points"));
    }

    let base_0 = crate::cmd::point_value(&base_arr[0]).ok_or_else(|| bad("roofplane", "invalid base point 0"))?;
    let base_1 = crate::cmd::point_value(&base_arr[1]).ok_or_else(|| bad("roofplane", "invalid base point 1"))?;

    let pitch_rise = p.get("pitchRise").and_then(Value::as_f64).unwrap_or(6.0);
    let pitch_run = p.get("pitchRun").and_then(Value::as_f64).unwrap_or(12.0);
    let thickness = p.get("thickness").and_then(Value::as_f64).unwrap_or(0.1);
    let fascia = p.get("fascia").and_then(Value::as_f64).unwrap_or(0.1);
    let soffit = p.get("soffit").and_then(Value::as_f64).unwrap_or(0.1);
    let level = p.get("level").and_then(Value::as_u64).map(|v| v as usize);

    create_roof_plane(s, points, (base_0, base_1), pitch_rise, pitch_run, thickness, fascia, soffit, level)
}

pub fn specs() -> Vec<crate::cmd::CommandSpec> {
    vec![
        crate::cmd::CommandSpec::new("roofplane", "Roof Plane", run_roofplane)
            .menu(&["Architecture", "Roof", "Plane"])
            .alias(&["rp"])
            .params("{points: [[x,y],...], base: [x0,y0,x1,y1], pitchRise?, pitchRun?, thickness?, fascia?, soffit?, level?} | {edit: \"r0p1\", pitchRise?, pitchRun?, thickness?, fascia?, soffit?}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;

    #[test]
    fn roofplane_makes_one_plane() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        assert_eq!(res["handles"][0], "r0");
        assert_eq!(res["plane"], 0);
        assert!((res["projectedArea"].as_f64().unwrap() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn roofplane_rejects_a_non_edge_base() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [5.0, 5.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("base edge does not lie on a polygon edge"));
    }

    #[test]
    fn roofplane_rejects_a_self_crossing_polygon() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 10.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("self-crossing"));
    }

    #[test]
    fn roofplane_rejects_over_256_points() {
        let mut s = Session::new();
        let mut pts = Vec::new();
        for i in 0..257 {
            let angle = (i as f64) * std::f64::consts::PI * 2.0 / 257.0;
            pts.push(vec![angle.cos() * 10.0, angle.sin() * 10.0]);
        }

        let params = json!({
            "points": pts,
            "base": [pts[0], pts[1]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("between 3 and 256"));
    }

    #[test]
    fn roofplane_rejects_degenerate_base() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [0.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("degenerate"));
    }

    #[test]
    fn roofplane_edit_marks_hand_edited() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        run_roofplane(&mut s, &params).unwrap();

        let edit_params = json!({
            "edit": "r0p0",
            "pitchRise": 8.0
        });
        let res = run_roofplane(&mut s, &edit_params).unwrap();
        assert_eq!(res["handles"][0], "r0");

        let doc = s.doc().unwrap();
        let roof = &doc.arch.roofs[0];
        assert!(roof.hand_edited);
    }

    #[test]
    fn roofplane_edit_of_unknown_id_is_refused() {
        let mut s = Session::new();
        let params = json!({
            "edit": "r99p0",
            "pitchRise": 8.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn roofplane_is_one_undo_step() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        s.execute("roofplane", &params).unwrap();

        s.undo().unwrap();
        let doc = s.doc().unwrap();
        assert!(doc.arch.roofs.is_empty());
    }

    #[test]
    fn roofplane_rejects_nan_pitch() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": serde_json::Value::from(f64::NAN),
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("finite"));
    }

    #[test]
    fn roofplane_rejects_zero_pitch_run() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 0.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("positive"));
    }

    #[test]
    fn roofplane_edit_refuses_noop() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        run_roofplane(&mut s, &params).unwrap();

        let edit_params = json!({
            "edit": "r0p0",
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &edit_params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("no changes"));
    }

    #[test]
    fn roofplane_accepts_three_points() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        assert_eq!(res["handles"][0], "r0");
        assert!((res["projectedArea"].as_f64().unwrap() - 50.0).abs() < 1e-6);
    }

    #[test]
    fn roofplane_base_on_last_edge() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 10.0], [0.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        assert_eq!(res["handles"][0], "r0");
    }

    #[test]
    fn roofplane_edit_pitch_updates_slope() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        run_roofplane(&mut s, &params).unwrap();

        let edit_params = json!({
            "edit": "r0p0",
            "pitchRise": 12.0,
            "pitchRun": 12.0
        });
        run_roofplane(&mut s, &edit_params).unwrap();

        let doc = s.doc().unwrap();
        let roof = &doc.arch.roofs[0];
        assert!((roof.planes[0].slope - 1.0).abs() < 1e-9);
    }

    #[test]
    fn roofplane_rejects_negative_thickness() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0,
            "thickness": -0.1
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("non-negative"));
    }
}
