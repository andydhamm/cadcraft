//! Read-only architecture inspection command.
use crate::cmd::CommandSpec;
use crate::{EngineError, Session};
use cadcraft_arch::join::solve_joins;
use cadcraft_arch::wall::{wall_layer_polygons, wall_polygon};
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
            if let Some(poly) = wall_polygon(w) {
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

    let joins = solve_joins(&d.arch).len();

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
        let bounds = v["bounds"].as_array().expect("bounds should be an array");
        assert_eq!(bounds.len(), 2, "bounds should be [[min],[max]]");
        assert!(v["levels"].is_array(), "levels should be an array");
    }
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
        let bounds = v["bounds"].as_array().expect("bounds should be an array");
        assert_eq!(bounds.len(), 2, "bounds should be [[min],[max]]");
        assert!(v["levels"].is_array(), "levels should be an array");
    }
}
