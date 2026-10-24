//! LEVEL command: add a level above/below another, or edit a level's heights.
use crate::cmd::{CommandSpec, Session, f64_or};
use serde_json::Value;

/// Default ceiling height (m) for a newly added level.
const DEFAULT_CEILING_HEIGHT: f64 = 2.4;

/// Default floor structure thickness (m) for a newly added level.
const DEFAULT_FLOOR_STRUCTURE: f64 = 0.3;

/// Find a level by name (string) or index (number). Returns the position in the levels vec.
fn find_level(arch: &cadcraft_doc::arch::ArchModel, ref_val: &Value) -> Result<usize, crate::EngineError> {
    if let Some(n) = ref_val.as_str() {
        // Try as index first
        if let Some(i) = n.parse::<u64>().ok() {
            if i < arch.levels.len() as u64 {
                return Ok(i as usize);
            }
        }
        // Then as name
        if let Some((i, l)) = arch.levels.iter().enumerate().find(|(_, l)| l.name == n) {
            return Ok(i);
        }
        return Err(crate::EngineError::Other(format!("level: unknown level '{n}'")));
    }
    if let Some(i) = ref_val.as_u64() {
        if i < arch.levels.len() as u64 {
            return Ok(i as usize);
        }
        return Err(crate::EngineError::Other(format!("level: unknown level index {i}")));
    }
    Err(crate::EngineError::Other("level: invalid level reference".into()))
}

/// Run the `level` command.
///
/// JSON: `{above: "Level 1" | index}` — add a level above.
/// `{below: "Level 1" | index}` — add a level below (renumbers walls).
/// `{index: n, ceiling?, structure?}` or `{name: "Level 1", ceiling?, structure?}` — edit.
pub fn run_level(s: &mut Session, p: &Value) -> Result<Value, crate::EngineError> {
    let doc = s.doc_mut()?;
    let arch = &mut doc.arch;

    let above = p.get("above");
    let below = p.get("below");
    let index = p.get("index").and_then(|v| v.as_u64());
    let name = p.get("name").and_then(|v| v.as_str());
    let ceiling = f64_or(p, "ceiling");
    let structure = f64_or(p, "structure");

    if above.is_none() && below.is_none() && index.is_none() && name.is_none() {
        return Err(crate::EngineError::Other("level: specify above, below, index or name".into()));
    }

    if let Some(above_ref) = above {
        let i = find_level(arch, above_ref)?;
        let base = &arch.levels[i];
        let new_elev = base.elevation + base.floor_to_floor();
        let mut new_level = cadcraft_doc::arch::Level {
            index: arch.levels.len() as u32,
            name: format!("Level {}", arch.levels.len() + 1),
            elevation: new_elev,
            ceiling_height: DEFAULT_CEILING_HEIGHT,
            structure: DEFAULT_FLOOR_STRUCTURE,
        };
        if let Some(c) = ceiling {
            if !c.is_finite() || c <= 0.0 {
                return Err(crate::EngineError::Other("level: ceiling must be finite and positive".into()));
            }
            new_level.ceiling_height = c;
        }
        if let Some(s) = structure {
            if !s.is_finite() || s <= 0.0 {
                return Err(crate::EngineError::Other("level: structure must be finite and positive".into()));
            }
            new_level.structure = s;
        }
        arch.levels.push(new_level);
        doc.touch_arch();
        return Ok(Value::from("ok"));
    }

    if let Some(below_ref) = below {
        let i = find_level(arch, below_ref)?;
        let base = &arch.levels[i];
        let new_elev = base.elevation - base.floor_to_floor();
        let mut new_level = cadcraft_doc::arch::Level {
            index: i as u32,
            name: format!("Level {}", i + 1),
            elevation: new_elev,
            ceiling_height: DEFAULT_CEILING_HEIGHT,
            structure: DEFAULT_FLOOR_STRUCTURE,
        };
        if let Some(c) = ceiling {
            if !c.is_finite() || c <= 0.0 {
                return Err(crate::EngineError::Other("level: ceiling must be finite and positive".into()));
            }
            new_level.ceiling_height = c;
        }
        if let Some(s) = structure {
            if !s.is_finite() || s <= 0.0 {
                return Err(crate::EngineError::Other("level: structure must be finite and positive".into()));
            }
            new_level.structure = s;
        }
        arch.levels.insert(i, new_level);
        // Renumber levels above the insertion point
        for l in arch.levels.iter_mut().skip(i + 1) {
            l.index += 1;
        }
        // Renumber walls: increment base_level for walls at or above the insertion point
        for w in arch.walls.iter_mut() {
            if w.base_level >= (i as u32) {
                w.base_level += 1;
            }
        }
        doc.touch_arch();
        return Ok(Value::from("ok"));
    }

    // Edit mode
    let i = if let Some(n) = name {
        find_level(arch, &Value::from(n))?
    } else if let Some(i) = index {
        if i < arch.levels.len() as u64 {
            i as usize
        } else {
            return Err(crate::EngineError::Other(format!("level: unknown level index {i}")));
        }
    } else {
        return Err(crate::EngineError::Other("level: no level specified".into()));
    };
    let l = arch.levels.get_mut(i).ok_or_else(|| crate::EngineError::Other("level: not found".into()))?;
    if let Some(c) = ceiling {
        if !c.is_finite() || c <= 0.0 {
            return Err(crate::EngineError::Other("level: ceiling must be finite and positive".into()));
        }
        l.ceiling_height = c;
    }
    if let Some(s) = structure {
        if !s.is_finite() || s <= 0.0 {
            return Err(crate::EngineError::Other("level: structure must be finite and positive".into()));
        }
        l.structure = s;
    }
    doc.touch_arch();
    Ok(Value::from("ok"))
}

/// Command specs for the level module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("level", "Level", run_level)
            .menu(&["Architecture", "Level"])
            .params("{above?: name|index, below?: name|index, index?: n, name?: s, ceiling?: f, structure?: f}")
            .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use cadcraft_doc::arch::{ArchId, JoinKind, Level, RefSide, Wall};
    use cadcraft_geom::Vec2;
    use serde_json::json;

    fn setup_session() -> Session {
        let mut s = Session::new();
        s.new_drawing(false);
        s
    }

    #[test]
    fn level_above_uses_the_derived_floor_to_floor() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.push(Level { index: 0, name: "Level 1".into(), elevation: 0.0, ceiling_height: 2.4, structure: 0.3 });
        }
        let result = s.execute("level", &json!({"above": "Level 1"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        assert_eq!(arch.levels[1].elevation, 2.7);
    }

    #[test]
    fn level_below_renumbers_walls() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.push(Level { index: 0, name: "Level 1".into(), elevation: 0.0, ceiling_height: 2.4, structure: 0.3 });
            doc.arch.levels.push(Level { index: 1, name: "Level 2".into(), elevation: 2.7, ceiling_height: 2.4, structure: 0.3 });
            doc.arch.walls.push(Wall {
                id: ArchId(0),
                points: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)],
                thickness: 0.2,
                height: 2.4,
                base_level: 1,
                top_level: 1,
                exterior: true,
                reversed: false,
                join_start: JoinKind::None,
                join_end: JoinKind::None,
                ref_side: RefSide::Center,
                type_id: 0,
                split_height: 0.0,
                upper_type: 0,
            });
        }
        let result = s.execute("level", &json!({"below": "Level 2"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.walls[0].base_level, 2);
    }

    #[test]
    fn level_rejects_an_unknown_neighbour() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.push(Level { index: 0, name: "Level 1".into(), elevation: 0.0, ceiling_height: 2.4, structure: 0.3 });
        }
        let result = s.execute("level", &json!({"above": "Level 99"}));
        assert!(result.is_err());
    }

    #[test]
    fn level_rejects_a_non_positive_ceiling() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.push(Level { index: 0, name: "Level 1".into(), elevation: 0.0, ceiling_height: 2.4, structure: 0.3 });
        }
        let result = s.execute("level", &json!({"name": "Level 1", "ceiling": -1.0}));
        assert!(result.is_err());
    }
}
