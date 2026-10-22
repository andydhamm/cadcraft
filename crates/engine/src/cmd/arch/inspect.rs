//! Read-only architecture inspection command.
use crate::cmd::CommandSpec;
use crate::{EngineError, Session};
use cadcraft_arch::join::solve_joins;
use cadcraft_arch::wall::{wall_layer_polygons, wall_polygon};
use cadcraft_doc::Handle;
use serde_json::Value;
use serde_json::json;

/// Command specs for the arch inspection module.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = Vec::new();
    v.push(CommandSpec::new("arch.inspect", "Arch Inspect", run_inspect).noundo().params("{poché?: bool}"));
    v
}

/// Reports walls (optionally with poché polygons), join count, revision, bounds and
/// levels from the current drawing's arch model. This is a read-only command: it never
/// calls `doc_mut()` or `touch_arch()`, so no revision is bumped.
fn run_inspect(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;

    // Accept both the Unicode `poché` key and the ASCII `poche` key (JSON clients that
    // cannot emit the acute accent).
    let poché = p.get("poché").and_then(Value::as_bool).or_else(|| p.get("poche").and_then(Value::as_bool)).unwrap_or(false);

    let mut walls = Vec::new();
    for w in &d.arch.walls {
        let mut wall = json!({
            "id": w.id,
            "handle": Handle(w.handle).hex(),
            "start": [w.start.x, w.start.y],
            "end": [w.end.x, w.end.y],
            "thickness": w.thickness,
            "height": w.height,
            "exterior": w.exterior,
            "kind": w.kind.to_string(),
            "level": w.level,
        });
        if poché {
            if let Some(poly) = wall_polygon(w) {
                wall.insert("polygon".to_string(), json!(poly));
            }
            if let Some(layers) = wall_layer_polygons(w) {
                let layers_json = layers.iter().map(|(name, poly)| json!({ "name": name, "polygon": poly })).collect::<Vec<Value>>();
                wall.insert("layers".to_string(), json!(layers_json));
            }
        }
        walls.push(wall);
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
                "ceiling": l.ceiling,
                "structure": l.structure,
            })
        })
        .collect::<Vec<Value>>();

    json!({
        "walls": walls,
        "joins": joins,
        "revision": d.arch.revision,
        "bounds": bounds_json,
        "levels": levels_json,
    })
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
        let walls = v["walls"].as_array().unwrap_or(&vec![]);
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
        let walls = v["walls"].as_array().unwrap_or(&vec![]);
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
        let before = exec(&mut s, "arch.inspect", &json!({}))["revision"];
        let after = exec(&mut s, "arch.inspect", &json!({}))["revision"];
        assert_eq!(before, after, "arch.inspect must not bump the arch revision");
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
