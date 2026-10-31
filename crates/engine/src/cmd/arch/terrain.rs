//! `terrain` and `terrain.delete`: the site boundary and its elevation data (phase 9).

use serde_json::{Value, json};

use cadcraft_doc::arch::{ElevationLine, Terrain};
use cadcraft_geom::{Vec2, Vec3};

use crate::cmd::{CommandSpec, bad, bool_or, point_value};
use crate::{Result, Session};

/// Command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("terrain", "Terrain", run_terrain)
            .menu(&["Architecture", "Terrain"])
            .params("{boundary: [[x,y],...], elevation?, points?: [[x,y,z],...], lines?: [{points: [[x,y],...], elevation, closed?}], csv?: path, csvText?: text, subtractReference?: bool, contours?: [minor, major], pad?: bool, padMargin?}"),
        CommandSpec::new("terrain.delete", "Delete Terrain", run_delete).menu(&["Architecture", "Delete Terrain"]),
    ]
}

fn finite_number(v: &Value, key: &str, cmd: &str) -> Result<f64> {
    v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad(cmd, format!("`{key}` must be a finite number")))
}

fn parse_point_xy(v: &Value, cmd: &str, key: &str) -> Result<Vec2> {
    point_value(v).ok_or_else(|| bad(cmd, format!("`{key}` must be [x, y] with finite numbers")))
}

fn parse_point_xyz(v: &Value, cmd: &str, key: &str) -> Result<Vec3> {
    let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| bad(cmd, format!("`{key}` must be [x, y, z] with three finite numbers")))?;
    let x = a
        .first()
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| bad(cmd, format!("`{key}` must be [x, y, z] with three finite numbers")))?;
    let y = a
        .get(1)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| bad(cmd, format!("`{key}` must be [x, y, z] with three finite numbers")))?;
    let z = a
        .get(2)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| bad(cmd, format!("`{key}` must be [x, y, z] with three finite numbers")))?;
    Ok(Vec3::new(x, y, z))
}

fn parse_boundary(cmd: &str, v: &Value) -> Result<Vec<Vec2>> {
    let arr = v.as_array().ok_or_else(|| bad(cmd, "`boundary` must be an array of [x, y] points"))?;
    let mut pts = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let p = parse_point_xy(item, cmd, &format!("boundary[{i}]"))?;
        pts.push(p);
    }
    if pts.len() >= 2 && (pts[0].x - pts[pts.len() - 1].x).hypot(pts[0].y - pts[pts.len() - 1].y) <= 1e-12 {
        pts.pop();
    }
    // Deduplicate consecutive coincident points.
    let mut deduped = Vec::with_capacity(pts.len());
    for p in pts {
        if deduped.last().is_some_and(|last: &Vec2| (last.x - p.x).hypot(last.y - p.y) <= 1e-12) {
            continue;
        }
        deduped.push(p);
    }
    if deduped.len() >= 2 && (deduped[0].x - deduped[deduped.len() - 1].x).hypot(deduped[0].y - deduped[deduped.len() - 1].y) <= 1e-12 {
        deduped.pop();
    }
    pts = deduped;
    if pts.len() < 3 {
        return Err(bad(cmd, "`boundary` must contain at least 3 points after dropping a closing point"));
    }
    if pts.len() > cadcraft_doc::arch::MAX_RING_POINTS {
        return Err(bad(cmd, format!("`boundary` exceeds the {} point cap", cadcraft_doc::arch::MAX_RING_POINTS)));
    }
    let area = signed_area(&pts);
    if area < 0.0 {
        pts.reverse();
    }
    Ok(pts)
}

fn signed_area(ring: &[Vec2]) -> f64 {
    let mut area = 0.0;
    for i in 0..ring.len() {
        let next = (i + 1) % ring.len();
        area += ring[i].x * ring[next].y - ring[next].x * ring[i].y;
    }
    0.5 * area
}

fn parse_points_list(cmd: &str, key: &str, v: &Value) -> Result<Vec<Vec3>> {
    let arr = v.as_array().ok_or_else(|| bad(cmd, format!("`{key}` must be an array of [x, y, z] points")))?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        out.push(parse_point_xyz(item, cmd, &format!("{key}[{i}]"))?);
    }
    Ok(out)
}

fn parse_lines(cmd: &str, v: &Value) -> Result<Vec<ElevationLine>> {
    let arr = v.as_array().ok_or_else(|| bad(cmd, "`lines` must be an array of elevation-line objects"))?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| bad(cmd, format!("`lines[{i}]` must be an object")))?;
        let points_v = obj.get("points").ok_or_else(|| bad(cmd, format!("`lines[{i}].points` is required")))?;
        let pts_arr = points_v.as_array().ok_or_else(|| bad(cmd, format!("`lines[{i}].points` must be an array of [x, y] points")))?;
        let mut points = Vec::with_capacity(pts_arr.len());
        for (j, p) in pts_arr.iter().enumerate() {
            points.push(parse_point_xy(p, cmd, &format!("lines[{i}].points[{j}]"))?);
        }
        let elevation = obj
            .get("elevation")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .ok_or_else(|| bad(cmd, format!("`lines[{i}].elevation` must be a finite number")))?;
        let closed = obj.get("closed").and_then(Value::as_bool).unwrap_or(false);
        out.push(ElevationLine { points, elevation, closed });
    }
    Ok(out)
}

fn parse_contours(cmd: &str, v: &Value) -> Result<(f64, f64)> {
    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(cmd, "`contours` must be [minor, major]"))?;
    let minor = finite_number(a.first().unwrap_or(&Value::Null), "minor", cmd)?;
    let major = finite_number(a.get(1).unwrap_or(&Value::Null), "major", cmd)?;
    Ok((minor, major))
}

fn run_terrain(s: &mut Session, params: &Value) -> Result<Value> {
    let cmd = "terrain";
    let obj = params.as_object().ok_or_else(|| bad(cmd, "params must be an object"))?;
    let site = s.doc()?.arch.site.clone();
    let existing = s.doc()?.arch.terrain.clone();

    let boundary = match obj.get("boundary") {
        Some(v) => parse_boundary(cmd, v)?,
        None => return Err(bad(cmd, "`boundary` is required")),
    };

    let mut points = match obj.get("points") {
        Some(v) if !v.is_null() => parse_points_list(cmd, "points", v)?,
        Some(_) | None => Vec::new(),
    };
    let offset = if bool_or(params, "subtractReference", false) {
        let Some(site) = site.as_ref() else {
            return Err(bad(cmd, "`subtractReference` requires a site reference"));
        };
        let Some(reference) = site.reference.as_ref() else {
            return Err(bad(cmd, "`subtractReference` requires a site reference"));
        };
        Some((reference.easting - reference.point.x, reference.northing - reference.point.y))
    } else {
        None
    };
    if let Some(v) = obj.get("csvText") {
        let text = v.as_str().ok_or_else(|| bad(cmd, "`csvText` must be a string"))?;
        let parsed = cadcraft_arch::terrain::csv::parse_points_csv(text, offset).map_err(|e| bad(cmd, e.to_string()))?;
        points.extend(parsed);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(v) = obj.get("csv") {
        let path = v.as_str().ok_or_else(|| bad(cmd, "`csv` must be a path string"))?;
        let text = std::fs::read_to_string(path).map_err(|e| bad(cmd, format!("could not read `{path}`: {e}")))?;
        let parsed = cadcraft_arch::terrain::csv::parse_points_csv(&text, offset).map_err(|e| bad(cmd, e.to_string()))?;
        points.extend(parsed);
    }
    #[cfg(target_arch = "wasm32")]
    if obj.get("csv").is_some() {
        return Err(bad(cmd, "csv files are not available in the web build"));
    }

    let lines = match obj.get("lines") {
        Some(v) if !v.is_null() => parse_lines(cmd, v)?,
        Some(_) | None => Vec::new(),
    };

    let default_elevation = match obj.get("elevation") {
        Some(v) => finite_number(v, "elevation", cmd)?,
        None => existing.as_ref().map(|t| t.default_elevation).unwrap_or(0.0),
    };
    let contours = match obj.get("contours") {
        Some(v) if !v.is_null() => parse_contours(cmd, v)?,
        None => existing
            .as_ref()
            .map(|t| (t.contour_minor, t.contour_major))
            .unwrap_or((cadcraft_doc::arch::DEFAULT_CONTOUR_MINOR, cadcraft_doc::arch::DEFAULT_CONTOUR_MAJOR)),
        Some(_) => return Err(bad(cmd, "`contours` must be [minor, major] or null")),
    };
    let (contour_minor, contour_major) = contours;
    let pad = obj.get("pad").and_then(Value::as_bool).unwrap_or_else(|| existing.as_ref().map(|t| t.pad).unwrap_or(true));
    let pad_margin = match obj.get("padMargin") {
        Some(v) => finite_number(v, "padMargin", cmd)?,
        None => existing.as_ref().map(|t| t.pad_margin).unwrap_or(cadcraft_doc::arch::DEFAULT_PAD_MARGIN),
    };

    let terrain = Terrain {
        boundary,
        default_elevation,
        points,
        lines,
        modifiers: existing.as_ref().map(|t| t.modifiers.clone()).unwrap_or_default(),
        next_modifier_id: existing.as_ref().map(|t| t.next_modifier_id).unwrap_or(0),
        contour_minor,
        contour_major,
        pad,
        pad_margin,
        ..Default::default()
    };

    cadcraft_arch::terrain::validate_terrain(&terrain).map_err(|e| bad(cmd, e.0))?;
    let tin = cadcraft_arch::terrain::original_tin(&terrain).map_err(|e| bad(cmd, e.0))?;
    let range = tin.elevation_range().unwrap_or((0.0, 0.0));

    let d = s.doc_mut()?;
    d.arch.terrain = Some(terrain);
    d.touch_arch();

    Ok(
        json!({ "terrain": "t0", "points": d.arch.terrain.as_ref().map(|t| t.points.len()).unwrap_or(0), "lines": d.arch.terrain.as_ref().map(|t| t.lines.len()).unwrap_or(0), "triangles": tin.triangles.len(), "range": [range.0, range.1] }),
    )
}

fn run_delete(s: &mut Session, _params: &Value) -> Result<Value> {
    let cmd = "terrain.delete";
    let has_terrain = s.doc()?.arch.terrain.is_some();
    if !has_terrain {
        return Err(bad(cmd, "there is no terrain to delete"));
    }
    let d = s.doc_mut()?;
    d.arch.terrain = None;
    d.touch_arch();
    Ok(json!({ "deleted": "t0" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{ModifierKind, TerrainModifier};
    use cadcraft_geom::Vec2;

    #[test]
    fn terrain_command_builds_from_points_and_lines() {
        let mut s = Session::new();
        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]],
                    "points": [[0.0, 0.0, 0.0], [40.0, 0.0, 2.0], [40.0, 30.0, 2.0], [0.0, 30.0, 0.0]],
                    "lines": [{"points": [[10.0, 0.0], [10.0, 20.0]], "elevation": 1.0}]
                }),
            )
            .unwrap();
        assert_eq!(r["terrain"], "t0");
        assert_eq!(r["points"], 4);
        assert_eq!(r["lines"], 1);
        let range = r["range"].as_array().unwrap();
        assert!((range[0].as_f64().unwrap() - 0.0).abs() < 1e-9);
        assert!((range[1].as_f64().unwrap() - 2.0).abs() < 1e-9);
        assert!(r["triangles"].as_u64().unwrap() > 0);
    }

    #[test]
    fn terrain_command_imports_csv_text_with_reference_offset() {
        let mut s = Session::new();
        s.execute(
            "site",
            &json!({
                "reference": { "point": [0.0, 0.0], "easting": 1000.0, "northing": 2000.0, "crs": "EPSG:32611" }
            }),
        )
        .unwrap();

        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]],
                    "csvText": "1000,2000,3\n1100,2100,4\n",
                    "subtractReference": true
                }),
            )
            .unwrap();
        assert_eq!(r["points"], 2);
        let terrain = s.doc().unwrap().arch.terrain.as_ref().unwrap();
        assert_eq!(terrain.points.len(), 2);
        assert_eq!(terrain.points[0], cadcraft_geom::Vec3::new(0.0, 0.0, 3.0));
        assert_eq!(terrain.points[1], cadcraft_geom::Vec3::new(100.0, 100.0, 4.0));
    }

    #[test]
    fn terrain_command_refuses_bad_input_and_writes_nothing() {
        let mut s = Session::new();
        let before = s.doc().unwrap().arch.terrain.clone();
        let rev = s.doc().unwrap().arch_revision;
        let err = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]],
                    "csvText": "x,y,z\n1,2,3\n1,2\n"
                }),
            )
            .unwrap_err();
        assert!(matches!(err, crate::EngineError::BadParams { cmd: _, msg } if msg.contains("CSV line 3")));
        assert_eq!(s.doc().unwrap().arch.terrain, before);
        assert_eq!(s.doc().unwrap().arch_revision, rev);
    }

    #[test]
    fn terrain_command_keeps_modifiers_on_replace() {
        let mut s = Session::new();
        let mut terrain = Terrain::default();
        terrain.boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        terrain.modifiers = vec![TerrainModifier { id: 5, kind: ModifierKind::Hill { center: Vec2::new(5.0, 5.0), height: 1.5, radius: 8.0 } }];
        terrain.next_modifier_id = 7;
        s.doc_mut().unwrap().arch.terrain = Some(terrain);

        s.execute(
            "terrain",
            &json!({
                "boundary": [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]],
                "points": [[0.0, 0.0, 0.0], [20.0, 20.0, 0.0]]
            }),
        )
        .unwrap();

        let terrain = s.doc().unwrap().arch.terrain.as_ref().unwrap();
        assert_eq!(terrain.modifiers.len(), 1);
        assert_eq!(terrain.modifiers[0].id, 5);
        assert_eq!(terrain.next_modifier_id, 7);
    }

    #[test]
    fn terrain_delete_removes_it() {
        let mut s = Session::new();
        s.execute(
            "terrain",
            &json!({
                "boundary": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                "points": [[0.0, 0.0, 0.0], [10.0, 10.0, 0.0]]
            }),
        )
        .unwrap();
        assert!(s.doc().unwrap().arch.terrain.is_some());

        let r = s.execute("terrain.delete", &json!({})).unwrap();
        assert_eq!(r["deleted"], "t0");
        assert!(s.doc().unwrap().arch.terrain.is_none());
    }

    #[test]
    fn terrain_delete_refuses_when_none_exists() {
        let mut s = Session::new();
        let err = s.execute("terrain.delete", &json!({})).unwrap_err();
        assert!(matches!(
            err,
            crate::EngineError::BadParams { cmd, msg }
                if cmd == "terrain.delete" && msg.contains("no terrain")
        ));
        assert!(s.doc().unwrap().arch.terrain.is_none());
    }

    #[test]
    fn terrain_command_three_point_boundary() {
        let mut s = Session::new();
        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [10.0, 0.0], [5.0, 10.0]],
                    "points": [[0.0, 0.0, 0.0], [10.0, 0.0, 1.0], [5.0, 10.0, 0.5]]
                }),
            )
            .unwrap();
        assert_eq!(r["terrain"], "t0");
        assert_eq!(r["points"], 3);
        let triangles = r["triangles"].as_u64().unwrap();
        assert!(triangles >= 1, "a three-point boundary must produce at least one triangle");
    }

    #[test]
    fn terrain_command_drops_repeated_closing_point() {
        let mut s = Session::new();
        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]],
                    "points": [[0.0, 0.0, 0.0], [10.0, 10.0, 0.0]]
                }),
            )
            .unwrap();
        assert_eq!(r["terrain"], "t0");
        let terrain = s.doc().unwrap().arch.terrain.as_ref().unwrap();
        assert_eq!(terrain.boundary.len(), 4, "closing point should be dropped");
        assert!((terrain.boundary[0].x - 0.0).abs() < 1e-12 && (terrain.boundary[0].y - 0.0).abs() < 1e-12, "first point unchanged");
    }

    #[test]
    fn terrain_command_no_points_uses_default_elevation() {
        let mut s = Session::new();
        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                    "elevation": 5.0
                }),
            )
            .unwrap();
        assert_eq!(r["terrain"], "t0");
        assert_eq!(r["points"], 0);
        let range = r["range"].as_array().unwrap();
        assert!((range[0].as_f64().unwrap() - 5.0).abs() < 1e-9, "min elevation should be 5.0");
        assert!((range[1].as_f64().unwrap() - 5.0).abs() < 1e-9, "max elevation should be 5.0");
    }

    #[test]
    fn terrain_command_contours_parameter() {
        let mut s = Session::new();
        s.execute(
            "terrain",
            &json!({
                "boundary": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                "points": [[0.0, 0.0, 0.0], [10.0, 10.0, 2.0]],
                "contours": [0.5, 2.0]
            }),
        )
        .unwrap();
        let terrain = s.doc().unwrap().arch.terrain.as_ref().unwrap();
        assert!((terrain.contour_minor - 0.5).abs() < 1e-12, "minor contour interval should be 0.5");
        assert!((terrain.contour_major - 2.0).abs() < 1e-12, "major contour interval should be 2.0");
    }

    #[test]
    fn terrain_command_huge_coordinate_values() {
        let mut s = Session::new();
        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [100000.0, 0.0], [100000.0, 100000.0], [0.0, 100000.0]],
                    "points": [[0.0, 0.0, 0.0], [100000.0, 100000.0, 1000.0]]
                }),
            )
            .unwrap();
        assert_eq!(r["terrain"], "t0");
        let range = r["range"].as_array().unwrap();
        assert!((range[0].as_f64().unwrap() - 0.0).abs() < 1e-6);
        assert!((range[1].as_f64().unwrap() - 1000.0).abs() < 1e-3);
    }

    #[test]
    fn terrain_command_coincident_adjacent_boundary_points() {
        let mut s = Session::new();
        let r = s
            .execute(
                "terrain",
                &json!({
                    "boundary": [[0.0, 0.0], [5.0, 5.0], [5.0, 5.0], [10.0, 0.0]],
                    "points": [[0.0, 0.0, 0.0], [5.0, 5.0, 1.0], [10.0, 0.0, 0.0]]
                }),
            )
            .unwrap();
        assert_eq!(r["terrain"], "t0");
        let terrain = s.doc().unwrap().arch.terrain.as_ref().unwrap();
        assert_eq!(terrain.boundary.len(), 3, "coincident interior points are deduplicated");
    }
}
