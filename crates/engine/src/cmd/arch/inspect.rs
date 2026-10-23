//! Read-only architecture inspection command.
use crate::cmd::CommandSpec;
use crate::{EngineError, Session};
use cadcraft_arch::join::solve_joins;
use cadcraft_arch::wall::{wall_layer_polygons, wall_polygon};
use cadcraft_doc::ArchId;
use cadcraft_doc::Handle;
use serde_json::json;
use serde_json::{Map as JsonMap, Value};

/// Command specs for the arch inspection module.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = Vec::new();
    v.push(CommandSpec::new("arch.inspect", "Arch Inspect", run_inspect).noundo().params("{poché?: bool}"));
    v
}

/// Reports walls (optionally with poché polygons), join count, revision, bounds and
/// levels from the current drawing's arch model. This is a read-only command: it never
/// calls `doc_mut()` or `touch_arch()`, so no revision is bumped.
fn run_inspect(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let d = s.doc()?;

    // Accept both the Unicode `poché` key and the ASCII `poche` key (JSON clients that
    // cannot emit the acute accent).
    let poché = p.get("poché").and_then(Value::as_bool).or_else(|| p.get("poche").and_then(Value::as_bool)).unwrap_or(false);

    let shapes = solve_joins(&d.arch);
    let mut solved: std::collections::BTreeMap<ArchId, &Vec<cadcraft_geom::Vec2>> = std::collections::BTreeMap::new();
    for s in &shapes {
        solved.insert(s.id, &s.polygon);
    }

    let mut walls = Vec::new();
    for w in &d.arch.walls {
        let mut obj = JsonMap::new();
        obj.insert("id".to_string(), json!(w.id));
        obj.insert("handle".to_string(), json!(Handle(0xA000_0000u64 + u64::from(w.id.0)).hex()));
        obj.insert("start".to_string(), json!([w.start.x, w.start.y]));
        obj.insert("end".to_string(), json!([w.end.x, w.end.y]));
        obj.insert("thickness".to_string(), json!(w.thickness));
        obj.insert("height".to_string(), json!(w.height));
        obj.insert("exterior".to_string(), json!(w.exterior));
        obj.insert("kind".to_string(), json!(w.wall_type.to_string()));
        obj.insert("level".to_string(), json!(w.base_level));

        if poché {
            if let Some(poly) = solved.get(&w.id) {
                let poly_json: Vec<Value> = poly.iter().map(|p| json!([p.x, p.y])).collect();
                obj.insert("polygon".to_string(), Value::Array(poly_json));
            } else if let Some(poly) = wall_polygon(w) {
                let poly_json: Vec<Value> = poly.iter().map(|p| json!([p.x, p.y])).collect();
                obj.insert("polygon".to_string(), Value::Array(poly_json));
            }
            let layers = wall_layer_polygons(w);
            if !layers.is_empty() {
                let layers_json: Vec<Value> = layers
                    .iter()
                    .map(|(name, poly)| {
                        let poly_arr: Vec<Value> = poly.iter().map(|p| json!([p.x, p.y])).collect();
                        json!({ "name": name, "polygon": poly_arr })
                    })
                    .collect();
                obj.insert("layers".to_string(), Value::Array(layers_json));
            }
        }

        walls.push(Value::Object(obj));
    }

    let joins = shapes.len();

    let bounds = d.arch.bounds();
    let bounds_json = if bounds.min.is_finite() && bounds.max.is_finite() {
        json!([[bounds.min.x, bounds.min.y], [bounds.max.x, bounds.max.y]])
    } else {
        json!([[0.0, 0.0], [0.0, 0.0]])
    };

    let levels_json = d
        .arch
        .levels
        .iter()
        .map(|l| {
            json!({
                "name": l.name,
                "elevation": l.elevation,
                "floorToFloor": l.floor_to_floor,
                "ceiling": l.ceiling_height,
                "structure": l.structure,
            })
        })
        .collect::<Vec<Value>>();

    Ok(json!({
        "walls": walls,
        "joins": joins,
        "revision": d.arch_revision,
        "bounds": bounds_json,
        "levels": levels_json,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    /// Helper that runs a command and returns its Value, asserting it succeeded.
    /// (Avoids `unwrap`/`expect`/`panic!` which are denied by the crate lints.)
    fn exec(s: &mut Session, id: &str, params: &Value) -> Value {
        let r = s.execute(id, params);
        assert!(r.is_ok(), "command `{id}` failed: {r:?}");
        match r {
            Ok(v) => v,
            Err(_) => json!(null),
        }
    }

    #[test]
    fn arch_inspect_reports_poché_polygons() {
        let mut s = Session::new();
        exec(
            &mut s,
            "wall",
            &json!({
                "points": [[0.0, 0.0], [5.0, 0.0]],
                "thickness": 0.1,
            }),
        );
        let v = exec(&mut s, "arch.inspect", &json!({ "poche": true }));
        let empty: Vec<Value> = Vec::new();
        let walls = v["walls"].as_array().unwrap_or(&empty);
        assert!(!walls.is_empty(), "expected at least one wall");
        let w = &walls[0];
        assert!(w["handle"].is_string());
        assert!(w["start"].as_array().is_some());
        assert!(w["polygon"].is_array(), "poché polygon missing");
        assert!(w["layers"].is_array(), "layers missing");
    }

    #[test]
    fn arch_inspect_empty_model_is_not_an_error() {
        let mut s = Session::new();
        let v = exec(&mut s, "arch.inspect", &json!({}));
        let empty: Vec<Value> = Vec::new();
        let walls = v["walls"].as_array().unwrap_or(&empty);
        assert!(walls.is_empty(), "no walls expected on empty model");
        assert_eq!(v["revision"], json!(0));
    }

    #[test]
    fn arch_inspect_does_not_bump_revision() {
        let mut s = Session::new();
        exec(
            &mut s,
            "wall",
            &json!({
                "points": [[0.0, 0.0], [5.0, 0.0]],
                "thickness": 0.1,
            }),
        );
        let before = exec(&mut s, "arch.inspect", &json!({}));
        let after = exec(&mut s, "arch.inspect", &json!({}));
        assert_eq!(before["revision"], after["revision"], "arch.inspect must not bump the arch revision");
    }

    /// Find a wall in the inspect output by its start and end points.
    fn find_wall_by_points<'a>(walls: &'a [Value], start: [f64; 2], end: [f64; 2]) -> Option<&'a Value> {
        walls.iter().find(|w| {
            let ws = w["start"].as_array().and_then(|a| a.get(0).and_then(Value::as_f64).zip(a.get(1).and_then(Value::as_f64)));
            let we = w["end"].as_array().and_then(|a| a.get(0).and_then(Value::as_f64).zip(a.get(1).and_then(Value::as_f64)));
            match (ws, we) {
                (Some((sx, sy)), Some((ex, ey))) => {
                    (sx - start[0]).abs() < 1e-6 && (sy - start[1]).abs() < 1e-6 && (ex - end[0]).abs() < 1e-6 && (ey - end[1]).abs() < 1e-6
                }
                _ => false,
            }
        })
    }

    /// Build the 4-wall exterior rectangle (0,0)-(10,0)-(10,8)-(0,8) at thickness 0.2.
    fn build_exterior_rect(s: &mut Session) {
        exec(s, "wall", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]], "thickness": 0.2 }));
        exec(s, "wall", &json!({ "points": [[10.0, 0.0], [10.0, 8.0]], "thickness": 0.2 }));
        exec(s, "wall", &json!({ "points": [[10.0, 8.0], [0.0, 8.0]], "thickness": 0.2 }));
        exec(s, "wall", &json!({ "points": [[0.0, 8.0], [0.0, 0.0]], "thickness": 0.2 }));
    }

    /// Build the full 6-wall demo plan: exterior rectangle + two interior walls.
    fn build_full_demo_plan(s: &mut Session) {
        build_exterior_rect(s);
        exec(s, "wall", &json!({ "points": [[0.0, 3.0], [10.0, 3.0]], "thickness": 0.1 }));
        exec(s, "wall", &json!({ "points": [[6.0, 3.0], [6.0, 8.0]], "thickness": 0.1 }));
    }

    #[test]
    fn arch_inspect_mitres_exterior_corners() {
        let mut s = Session::new();
        build_exterior_rect(&mut s);
        let v = exec(&mut s, "arch.inspect", &json!({ "poche": true }));
        let empty: Vec<Value> = Vec::new();
        let walls = v["walls"].as_array().unwrap_or(&empty);
        let w = find_wall_by_points(walls, [0.0, 0.0], [10.0, 0.0]).expect("bottom wall not found");
        let poly = w["polygon"].as_array().expect("polygon array missing");
        let corners = [(-0.1, -0.1), (10.1, -0.1), (9.9, 0.1), (0.1, 0.1)];
        for (cx, cy) in corners {
            let found = poly.iter().any(|p| {
                let px = p.as_array().and_then(|a| a.get(0).and_then(Value::as_f64)).unwrap_or(f64::NAN);
                let py = p.as_array().and_then(|a| a.get(1).and_then(Value::as_f64)).unwrap_or(f64::NAN);
                (px - cx).abs() < 1e-6 && (py - cy).abs() < 1e-6
            });
            assert!(found, "corner ({cx}, {cy}) not in bottom wall polygon");
        }
        let has_bad_corner = poly.iter().any(|p| {
            let px = p.as_array().and_then(|a| a.get(0).and_then(Value::as_f64)).unwrap_or(f64::NAN);
            let py = p.as_array().and_then(|a| a.get(1).and_then(Value::as_f64)).unwrap_or(f64::NAN);
            ((px - 10.0).abs() < 1e-6 && ((py - 0.1).abs() < 1e-6 || (py + 0.1).abs() < 1e-6))
        });
        assert!(!has_bad_corner, "polygon should not contain (10.0, ±0.1)");
    }

    #[test]
    fn arch_inspect_interior_walls_butt_at_faces() {
        let mut s = Session::new();
        build_full_demo_plan(&mut s);
        let v = exec(&mut s, "arch.inspect", &json!({ "poche": true }));
        let empty: Vec<Value> = Vec::new();
        let walls = v["walls"].as_array().unwrap_or(&empty);
        for w in walls {
            let is_exterior = w["exterior"].as_bool().unwrap_or(false);
            if is_exterior {
                continue;
            }
            let poly = w["polygon"].as_array().expect("polygon array missing");
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for p in poly {
                let px = p.as_array().and_then(|a| a.get(0).and_then(Value::as_f64)).unwrap_or(f64::NAN);
                let py = p.as_array().and_then(|a| a.get(1).and_then(Value::as_f64)).unwrap_or(f64::NAN);
                min_x = min_x.min(px);
                max_x = max_x.max(px);
                min_y = min_y.min(py);
                max_y = max_y.max(py);
            }
            assert!(min_x >= 0.099999, "interior wall min_x {min_x} < 0.099999");
            assert!(max_x <= 9.900001, "interior wall max_x {max_x} > 9.900001");
            assert!(min_y >= 0.099999, "interior wall min_y {min_y} < 0.099999");
            assert!(max_y <= 7.900001, "interior wall max_y {max_y} > 7.900001");
        }
    }

    #[test]
    fn arch_inspect_poché_count_matches_walls() {
        let mut s = Session::new();
        build_full_demo_plan(&mut s);
        let v = exec(&mut s, "arch.inspect", &json!({ "poche": true }));
        let empty: Vec<Value> = Vec::new();
        let walls = v["walls"].as_array().unwrap_or(&empty);
        assert_eq!(walls.len(), 6, "expected 6 walls");
        for w in walls {
            let poly = w["polygon"].as_array().expect("polygon array missing");
            assert!(poly.len() >= 4, "polygon must have at least 4 vertices");
            let first = poly.first().unwrap_or(&Value::Null);
            let last = poly.last().unwrap_or(&Value::Null);
            let first_x = first.as_array().and_then(|a| a.get(0).and_then(Value::as_f64)).unwrap_or(f64::NAN);
            let first_y = first.as_array().and_then(|a| a.get(1).and_then(Value::as_f64)).unwrap_or(f64::NAN);
            let last_x = last.as_array().and_then(|a| a.get(0).and_then(Value::as_f64)).unwrap_or(f64::NAN);
            let last_y = last.as_array().and_then(|a| a.get(1).and_then(Value::as_f64)).unwrap_or(f64::NAN);
            assert!((first_x - last_x).abs() >= 1e-9 || (first_y - last_y).abs() >= 1e-9, "polygon[0] must differ from polygon[last]");
            for p in poly {
                let px = p.as_array().and_then(|a| a.get(0).and_then(Value::as_f64)).unwrap_or(f64::NAN);
                let py = p.as_array().and_then(|a| a.get(1).and_then(Value::as_f64)).unwrap_or(f64::NAN);
                assert!(px.is_finite() && py.is_finite(), "polygon vertex not finite: ({px}, {py})");
            }
        }
    }

    #[test]
    fn arch_inspect_lists_levels_and_bounds() {
        let mut s = Session::new();
        exec(
            &mut s,
            "wall",
            &json!({
                "points": [[0.0, 0.0], [5.0, 0.0]],
                "thickness": 0.1,
            }),
        );
        let v = exec(&mut s, "arch.inspect", &json!({}));
        let empty: Vec<Value> = Vec::new();
        let bounds = v["bounds"].as_array().unwrap_or(&empty);
        assert_eq!(bounds.len(), 2, "bounds should be [[min],[max]]");
        assert!(v["levels"].is_array(), "levels should be an array");
    }
}
