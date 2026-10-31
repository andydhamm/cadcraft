//! `terrain.query`, `terrain.contours` and `terrain.volumes`: read-only answers about the terrain (phase 9).

use serde_json::{Value, json};

use cadcraft_arch::terrain::contour::{label_point, label_text, site_contours};
use cadcraft_arch::terrain::mesh::site_mesh;
use cadcraft_arch::terrain::surface::SiteSurface;
use cadcraft_arch::terrain::volume::cut_fill;
use cadcraft_geom::Vec2;
use cadcraft_geom::tin_ops::ContourLine;

use crate::cmd::{CommandSpec, bad, point_value};
use crate::{Result, Session};

/// The most points one `terrain.query` may ask about.
const MAX_QUERY_POINTS: usize = 1024;

/// Command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("terrain.query", "Terrain Elevation", run_query)
            .menu(&["Architecture", "Terrain Elevation"])
            .params("{points: [[x,y],...] (1..=1024), surface?: \"modified\"|\"original\"}")
            .noundo(),
        CommandSpec::new("terrain.contours", "Contours", run_contours).menu(&["Architecture", "Contours"]).params("{}").noundo(),
        CommandSpec::new("terrain.volumes", "Cut and Fill", run_volumes).menu(&["Architecture", "Cut and Fill"]).params("{}").noundo(),
    ]
}

fn run_query(s: &mut Session, params: &Value) -> Result<Value> {
    let cmd = "terrain.query";
    let modified = match params.get("surface") {
        None | Some(Value::Null) => true,
        Some(v) => match v.as_str() {
            Some("modified") => true,
            Some("original") => false,
            _ => return Err(bad(cmd, "`surface` must be \"modified\" or \"original\"")),
        },
    };
    let list = params.get("points").and_then(Value::as_array).ok_or_else(|| bad(cmd, "`points` ([[x,y],...]) is required"))?;
    if list.is_empty() || list.len() > MAX_QUERY_POINTS {
        return Err(bad(cmd, format!("`points` must hold 1 to {MAX_QUERY_POINTS} points")));
    }
    let points: Vec<Vec2> = list
        .iter()
        .map(|v| {
            let arr = v.as_array()?;
            if arr.len() != 2 {
                return None;
            }
            // Null coordinates (NaN/Infinity serialize to null in JSON) become non-finite;
            // any other non-number is a bad point.
            let num = |v: &Value| -> Option<f64> {
                match v {
                    Value::Null => Some(f64::NAN),
                    Value::Number(n) => n.as_f64().or(Some(f64::NAN)),
                    _ => None,
                }
            };
            let x = num(&arr[0])?;
            let y = num(&arr[1])?;
            Some(Vec2::new(x, y))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| bad(cmd, "every point must be [x, y] with numbers"))?;
    let surf = SiteSurface::new(&s.doc()?.arch).map_err(|e| bad(cmd, e.0))?;
    let elevations: Vec<Value> = points
        .iter()
        .map(|p| {
            if !p.is_finite() {
                return Value::Null;
            }
            let z = if modified { surf.modified_at(*p) } else { surf.original_at(*p) };
            z.filter(|z| z.is_finite()).map_or(Value::Null, |z| json!(z))
        })
        .collect();
    Ok(json!({ "elevations": elevations }))
}

fn pts(line: &ContourLine) -> Vec<Value> {
    line.points.iter().map(|p| json!([p.x, p.y])).collect()
}

fn run_contours(s: &mut Session, _params: &Value) -> Result<Value> {
    let cmd = "terrain.contours";
    let arch = &s.doc()?.arch;
    let (minor, major) = arch.terrain.as_ref().map(|t| (t.contour_minor, t.contour_major)).ok_or_else(|| bad(cmd, "the drawing has no terrain"))?;
    let surf = SiteSurface::new(arch).map_err(|e| bad(cmd, e.0))?;
    let mesh = site_mesh(&surf).map_err(|e| bad(cmd, e.0))?;
    let set = site_contours(&mesh, minor, major);
    let minor_lines: Vec<Value> = set.minor.iter().map(|l| json!({ "elevation": l.elevation, "points": pts(l), "closed": l.closed })).collect();
    let major_lines: Vec<Value> = set
        .major
        .iter()
        .map(|l| {
            let at = label_point(l).map_or(Value::Null, |p| json!([p.x, p.y]));
            json!({ "elevation": l.elevation, "points": pts(l), "closed": l.closed, "label": label_text(l.elevation), "labelAt": at })
        })
        .collect();
    Ok(json!({
        "minor": { "interval": minor, "lines": minor_lines },
        "major": { "interval": major, "lines": major_lines },
    }))
}

fn run_volumes(s: &mut Session, _params: &Value) -> Result<Value> {
    let cmd = "terrain.volumes";
    let surf = SiteSurface::new(&s.doc()?.arch).map_err(|e| bad(cmd, e.0))?;
    let mesh = site_mesh(&surf).map_err(|e| bad(cmd, e.0))?;
    let (cut, fill) = cut_fill(&mesh, &surf);
    Ok(json!({ "cut": cut, "fill": fill }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(z: [f64; 4]) -> Session {
        let mut s = Session::new();
        s.execute(
            "terrain",
            &json!({
                "boundary": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                "points": [[0.0, 0.0, z[0]], [10.0, 0.0, z[1]], [10.0, 10.0, z[2]], [0.0, 10.0, z[3]]],
            }),
        )
        .unwrap();
        s
    }

    #[test]
    fn terrain_query_answers_modified_and_original() {
        let mut s = session([1.0, 1.0, 1.0, 1.0]);
        for surface in ["modified", "original"] {
            let r = s.execute("terrain.query", &json!({"points": [[5.0, 5.0], [50.0, 50.0]], "surface": surface})).unwrap();
            let e = r["elevations"].as_array().unwrap();
            assert!((e[0].as_f64().unwrap() - 1.0).abs() < 1e-9);
            assert!(e[1].is_null());
        }
        let r = s.execute("terrain.query", &json!({"points": [[5.0, 5.0]]})).unwrap();
        assert_eq!(r["elevations"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn terrain_query_refuses_bad_points() {
        let mut s = session([0.0; 4]);
        assert!(s.execute("terrain.query", &json!({"points": []})).is_err());
        assert!(s.execute("terrain.query", &json!({})).is_err());
        assert!(s.execute("terrain.query", &json!({"points": [[1.0, "x"]]})).is_err());
        assert!(s.execute("terrain.query", &json!({"points": [[1.0, 1.0]], "surface": "bogus"})).is_err());
        let many: Vec<Value> = (0..=MAX_QUERY_POINTS).map(|_| json!([1.0, 1.0])).collect();
        assert!(s.execute("terrain.query", &json!({"points": many})).is_err());
        let mut none = Session::new();
        assert!(none.execute("terrain.query", &json!({"points": [[1.0, 1.0]]})).is_err());
    }

    #[test]
    fn terrain_contours_reply_shape() {
        let mut s = session([0.0, 2.0, 2.0, 0.0]);
        let r = s.execute("terrain.contours", &json!({})).unwrap();
        assert!(r["minor"]["interval"].as_f64().unwrap() > 0.0);
        assert!(r["major"]["interval"].as_f64().unwrap() > 0.0);
        let minor = r["minor"]["lines"].as_array().unwrap();
        assert!(!minor.is_empty());
        assert!(minor[0]["points"].is_array() && minor[0]["closed"].is_boolean());
        let major = r["major"]["lines"].as_array().unwrap();
        assert!(!major.is_empty());
        assert!(major[0]["label"].is_string());
        assert_eq!(major[0]["labelAt"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn terrain_volumes_of_a_flat_region() {
        let mut s = session([1.0; 4]);
        let r = s.execute("terrain.volumes", &json!({})).unwrap();
        assert!(r["cut"].as_f64().unwrap().abs() < 1e-6);
        assert!(r["fill"].as_f64().unwrap().abs() < 1e-6);
    }

    #[test]
    fn terrain_queries_change_nothing() {
        let mut s = session([0.0, 2.0, 2.0, 0.0]);
        let before = s.doc().unwrap().arch.clone();
        let rev = s.doc().unwrap().arch_revision;
        s.execute("terrain.query", &json!({"points": [[5.0, 5.0]]})).unwrap();
        s.execute("terrain.contours", &json!({})).unwrap();
        s.execute("terrain.volumes", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().arch, before);
        assert_eq!(s.doc().unwrap().arch_revision, rev);
    }

    #[test]
    fn terrain_contours_refuses_without_terrain() {
        let mut s = Session::new();
        assert!(s.execute("terrain.contours", &json!({})).is_err());
    }

    #[test]
    fn terrain_volumes_refuses_without_terrain() {
        let mut s = Session::new();
        assert!(s.execute("terrain.volumes", &json!({})).is_err());
    }

    #[test]
    fn terrain_query_with_nan_and_infinite_points() {
        let mut s = session([0.0; 4]);
        let r = s.execute("terrain.query", &json!({"points": [[f64::NAN, 1.0], [f64::INFINITY, 1.0], [1.0, f64::NAN]]})).unwrap();
        let e = r["elevations"].as_array().unwrap();
        assert_eq!(e.len(), 3);
        assert!(e[0].is_null());
        assert!(e[1].is_null());
        assert!(e[2].is_null());
    }

    #[test]
    fn terrain_query_coincident_and_boundary_points() {
        let mut s = session([0.0, 2.0, 2.0, 0.0]);
        let r = s.execute("terrain.query", &json!({"points": [[0.0, 0.0], [10.0, 0.0], [5.0, 5.0], [5.0, 5.0]]})).unwrap();
        let e = r["elevations"].as_array().unwrap();
        assert_eq!(e.len(), 4);
        assert!(e[0].as_f64().is_some());
        assert!(e[1].as_f64().is_some());
        assert_eq!(e[2], e[3]);
    }

    #[test]
    fn terrain_query_huge_coordinate_values() {
        let mut s = session([0.0; 4]);
        let r = s.execute("terrain.query", &json!({"points": [[1e15, 1e15], [-1e15, -1e15]]})).unwrap();
        let e = r["elevations"].as_array().unwrap();
        assert_eq!(e.len(), 2);
        assert!(e[0].is_null());
        assert!(e[1].is_null());
    }
}
