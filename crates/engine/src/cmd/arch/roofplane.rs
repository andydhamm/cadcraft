//! The `roofplane` command: create or edit a single manual roof plane.

use cadcraft_arch::poly::is_simple;
use cadcraft_doc::arch::{ArchModel, Level, Roof, RoofEdge, RoofEdgeKind, RoofPlane, Wall};
use cadcraft_geom::Vec2;
use serde_json::Value;

use crate::Session;
use crate::cmd::{Result, bad, f64_or, points_param};

/// Height of the wall sitting under the eave `base` segment.
///
/// A wall matches when its centre line is parallel to the base segment (cosine within
/// 0.99) and its infinite line is within half the wall thickness plus a small tolerance
/// of the base midpoint. Returns the matching wall's height (closest wins); `None` when
/// no wall is under the base.
fn wall_height_under(base_a: Vec2, base_b: Vec2, model: &ArchModel) -> Option<f64> {
    let base = base_b - base_a;
    let base_len = base.len();
    if base_len <= 1e-9 || !base_len.is_finite() {
        return None;
    }
    let base_dir = base * (1.0 / base_len);
    let base_mid = base_a + base * 0.5;
    let mut best: Option<(f64, f64)> = None; // (distance, height)
    for w in &model.walls {
        if !w.start.is_finite() || !w.end.is_finite() {
            continue;
        }
        let wl = w.start.dist(w.end);
        if wl <= 1e-12 || !wl.is_finite() {
            continue;
        }
        if w.height <= 0.0 || !w.height.is_finite() {
            continue;
        }
        if !w.thickness.is_finite() || w.thickness < 0.0 {
            continue;
        }
        let w_dir = (w.end - w.start) * (1.0 / wl);
        let dot = base_dir.dot(w_dir);
        if dot.abs() < 0.99 {
            continue;
        }
        let ap = base_mid - w.start;
        let t = ap.dot(w_dir);
        let closest = w.start + w_dir * t;
        let d = base_mid.dist(closest);
        let max_d = w.thickness * 0.5 + 0.05;
        if d <= max_d {
            let cand = (d, w.height);
            match &best {
                Some(b) if b.0 <= cand.0 => {}
                _ => best = Some(cand),
            }
        }
    }
    best.map(|(_, h)| h)
}

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
    mut points: Vec<Vec2>,
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
    // Closed rings commonly repeat the first vertex as the last; drop the redundant
    // trailing point before any simplicity or edge check so the ring is well formed.
    if points.len() >= 2 && points.last().is_some_and(|last| points.first().is_some_and(|first| last.near(*first, 1e-6))) {
        points.pop();
    }
    if points.len() < 3 {
        return Err(bad("roofplane", "points must be between 3 and 256"));
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

    let mut doc = s.doc_mut()?;
    // Roofs add their save-time proxy layer on first use; a fresh drawing carries no
    // arch layers.
    doc.ensure_layer("A-ROOF");
    let arch = &mut doc.arch;

    // Resolve the roof level: the requested index when it exists, otherwise the top level.
    let top = arch.levels.iter().map(|l| l.index).max().unwrap_or(0);
    let level_idx = match level {
        Some(l) if arch.level(l as u32).is_some() => l as u32,
        Some(_) | None => top,
    };
    let lev = arch.level(level_idx).cloned().unwrap_or_else(Level::default);

    // Eave height: level elevation + the wall height under the base edge, falling back
    // to the level's ceiling height when no wall lies under the base.
    let eave_z = match wall_height_under(base.0, base.1, &arch) {
        Some(h) => lev.elevation + h,
        None => lev.elevation + lev.ceiling_height,
    };

    let plane =
        RoofPlane { ring, slope, projected_area, base_edge, thickness, eave_z, fascia_depth: fascia, soffit_depth: soffit, ..Default::default() };

    let edge = RoofEdge { kind: RoofEdgeKind::Eave, a: base.0, b: base.1, ..Default::default() };

    let id = arch.next_roof_id;
    arch.next_roof_id += 1;

    let roof = Roof {
        id: cadcraft_doc::ArchId(id),
        level: level_idx,
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
    // Roofs add their save-time proxy layer on first use; a fresh drawing carries no
    // arch layers.
    doc.ensure_layer("A-ROOF");
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

    let pitch_rise = match p.get("pitchRise") {
        None => 6.0,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("roofplane", "`pitchRise` must be a finite number"))?,
    };
    let pitch_run = match p.get("pitchRun") {
        None => 12.0,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("roofplane", "`pitchRun` must be a finite number"))?,
    };
    let thickness = match p.get("thickness") {
        None => 0.1,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("roofplane", "`thickness` must be a finite number"))?,
    };
    let fascia = match p.get("fascia") {
        None => 0.1,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("roofplane", "`fascia` must be a finite number"))?,
    };
    let soffit = match p.get("soffit") {
        None => 0.1,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("roofplane", "`soffit` must be a finite number"))?,
    };
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
        let mut params = serde_json::Map::new();
        params.insert("points".to_string(), json!([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]));
        params.insert("base".to_string(), json!([[0.0, 0.0], [10.0, 0.0]]));
        params.insert("pitchRise".to_string(), serde_json::Value::from(f64::NAN));
        params.insert("pitchRun".to_string(), json!(12.0));
        let params = serde_json::Value::Object(params);
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

    /// `roofplane` creates the A-ROOF save-time proxy layer on first use (exactly once).
    #[test]
    fn roofplane_creates_roof_layer() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        run_roofplane(&mut s, &params).unwrap();
        let d = s.doc().unwrap();
        assert!(d.layers.iter().any(|l| l.name == "A-ROOF"), "A-ROOF not created by roofplane");
        assert_eq!(d.layers.iter().filter(|l| l.name == "A-ROOF").count(), 1, "A-ROOF created more than once");
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

    /// A closed ring that repeats its first vertex as the last is accepted and the
    /// stored ring has the redundant point dropped.
    #[test]
    fn roofplane_accepts_a_closed_ring() {
        let mut s = Session::new();
        let params = json!({
            "points": [[-0.1, -0.1], [10.1, -0.1], [10.1, 4.0], [-0.1, 4.0], [-0.1, -0.1]],
            "base": [[-0.1, -0.1], [10.1, -0.1]],
            "pitchRise": 6.0,
            "pitchRun": 12.0,
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_ok(), "closed ring rejected: {res:?}");
        let v = res.unwrap();
        assert_eq!(v["handles"][0], "r0");
        assert_eq!(v["plane"], 0);
        let area = v["projectedArea"].as_f64().unwrap();
        assert!((area - 41.82).abs() < 1e-6, "projectedArea {area} != 41.82");
        let doc = s.doc().unwrap();
        assert_eq!(doc.arch.roofs.len(), 1);
        let plane = &doc.arch.roofs[0].planes[0];
        assert_eq!(plane.ring.len(), 4, "ring should drop the repeated closing point");
        assert_eq!(plane.base_edge, [-0.1, -0.1, 10.1, -0.1]);
        assert!((plane.slope - 0.5).abs() < 1e-9);
    }

    /// Numeric `fascia`/`soffit` depths become the roof booleans and are stored as
    /// `fascia_depth`/`soffit_depth` on the plane.
    #[test]
    fn roofplane_numeric_fascia_and_soffit() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0,
            "fascia": 0.2,
            "soffit": 0.15,
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_ok(), "numeric fascia/soffit rejected: {res:?}");
        let roof = &s.doc().unwrap().arch.roofs[0];
        assert!(roof.fascia, "fascia bool should be true for fascia 0.2");
        assert!(roof.soffit, "soffit bool should be true for soffit 0.15");
        let p = &roof.planes[0];
        assert!((p.fascia_depth - 0.2).abs() < 1e-9, "fascia_depth {}", p.fascia_depth);
        assert!((p.soffit_depth - 0.15).abs() < 1e-9, "soffit_depth {}", p.soffit_depth);

        let mut s2 = Session::new();
        let res2 = run_roofplane(
            &mut s2,
            &json!({
                "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                "base": [[0.0, 0.0], [10.0, 0.0]],
                "fascia": 0.0,
                "soffit": 0.0,
            }),
        );
        assert!(res2.is_ok(), "zero fascia/soffit rejected: {res2:?}");
        let roof2 = &s2.doc().unwrap().arch.roofs[0];
        assert!(!roof2.fascia, "fascia bool should be false for fascia 0.0");
        assert!(!roof2.soffit, "soffit bool should be false for soffit 0.0");
        let p2 = &roof2.planes[0];
        assert_eq!(p2.fascia_depth, 0.0, "fascia_depth should be 0.0");
        assert_eq!(p2.soffit_depth, 0.0, "soffit_depth should be 0.0");
    }

    /// A roof created via `roofplane` is reported by `arch.inspect` under `roofs`.
    #[test]
    fn roofplane_reports_the_roof_in_inspect() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0,
            "fascia": 0.2,
            "soffit": 0.15,
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_ok(), "roofplane rejected: {res:?}");

        let inspect = s.execute("arch.inspect", &json!({ "roofs": true }));
        assert!(inspect.is_ok(), "arch.inspect rejected: {inspect:?}");
        let v = inspect.unwrap();
        let roofs = v["roofs"].as_array().expect("arch.inspect should report `roofs`");
        assert_eq!(roofs.len(), 1, "expected exactly one roof");
        let r = &roofs[0];
        assert_eq!(r["id"].as_str().unwrap(), "r0");
        assert!(r["planes"].as_array().unwrap().len() == 1, "roof should have one plane");
        assert!(r["manual"].as_bool().unwrap(), "manual roof should report manual:true");
        assert_eq!(r["planes"][0].as_object().unwrap()["fasciaDepth"], json!(0.2));
        assert_eq!(r["planes"][0].as_object().unwrap()["soffitDepth"], json!(0.15));
    }

    /// NaN in any point coordinate is rejected as non-finite input.
    #[test]
    fn roofplane_rejects_nan_points() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, f64::NAN], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("finite"));
    }

    /// Creating two roofs assigns sequential IDs r0 and r1.
    #[test]
    fn roofplane_two_roofs_get_sequential_ids() {
        let mut s = Session::new();
        let params1 = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res1 = run_roofplane(&mut s, &params1).unwrap();
        assert_eq!(res1["handles"][0], "r0");

        let params2 = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[10.0, 0.0], [10.0, 10.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res2 = run_roofplane(&mut s, &params2).unwrap();
        assert_eq!(res2["handles"][0], "r1");
    }

    /// A negative pitch rise produces a negative slope (downward plane).
    #[test]
    fn roofplane_negative_pitch_rise() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": -6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        let doc = s.doc().unwrap();
        let roof = &doc.arch.roofs[0];
        assert!((roof.planes[0].slope - (-0.5)).abs() < 1e-9);
        assert!((roof.pitch_rise - (-6.0)).abs() < 1e-9);
    }

    /// Editing only the thickness updates both the plane and the roof record.
    #[test]
    fn roofplane_edit_thickness_only() {
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
            "thickness": 0.25
        });
        run_roofplane(&mut s, &edit_params).unwrap();

        let doc = s.doc().unwrap();
        let roof = &doc.arch.roofs[0];
        assert!((roof.planes[0].thickness - 0.25).abs() < 1e-9);
        assert!((roof.thickness - 0.25).abs() < 1e-9);
    }

    /// A closed ring with the minimum 3 points (triangle) is accepted.
    #[test]
    fn roofplane_closed_ring_three_points() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [0.0, 0.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        let doc = s.doc().unwrap();
        let plane = &doc.arch.roofs[0].planes[0];
        assert_eq!(plane.ring.len(), 3);
        assert!((res["projectedArea"].as_f64().unwrap() - 50.0).abs() < 1e-6);
    }

    /// Very large coordinates stay finite and produce a correct projected area.
    #[test]
    fn roofplane_huge_coordinates() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [1e6, 0.0], [1e6, 1e6], [0.0, 1e6]],
            "base": [[0.0, 0.0], [1e6, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        let area = res["projectedArea"].as_f64().unwrap();
        assert!(area.is_finite());
        assert!((area - 1e12).abs() < 1e6);
    }

    /// An unknown level index falls back to the top level (level 0 in a new drawing).
    #[test]
    fn roofplane_unknown_level_falls_back_to_top() {
        let mut s = Session::new();
        let params = json!({
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "base": [[0.0, 0.0], [10.0, 0.0]],
            "pitchRise": 6.0,
            "pitchRun": 12.0,
            "level": 99
        });
        let res = run_roofplane(&mut s, &params).unwrap();
        let doc = s.doc().unwrap();
        let roof = &doc.arch.roofs[0];
        assert_eq!(roof.level, 0);
    }
}
