//! LEVEL command: add a level above/below another, or edit a level's heights.
use crate::cmd::{CommandSpec, Session, f64_or};
use serde_json::Value;

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

/// True if `name` is an auto-generated level name (`Level <n>`, case-insensitive).
fn is_auto_level_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let lower = lower.trim();
    if !lower.starts_with("level") {
        return false;
    }
    let rest = &lower["level".len()..].trim();
    !rest.is_empty() && rest.parse::<i64>().is_ok()
}

/// Derives an auto-generated level name based on a neighbour's name or a fallback index.
///
/// If `neighbour` looks like `Level N`, the new name starts at `Level N+1`.
/// Otherwise it starts at `Level {fallback}`.
/// Collisions with existing names in `levels` are resolved by incrementing the number.
fn auto_level_name(levels: &[cadcraft_doc::arch::Level], neighbour: &str, fallback: usize) -> String {
    let mut n: i64 = if is_auto_level_name(neighbour) {
        let lower = neighbour.to_ascii_lowercase();
        let lower = lower.trim();
        let rest = &lower["level".len()..].trim();
        rest.parse::<i64>().unwrap_or(fallback as i64) + 1
    } else {
        fallback as i64
    };

    let mut name = format!("Level {n}");
    let mut count = 0;
    while levels.iter().any(|l| l.name == name) {
        n += 1;
        name = format!("Level {n}");
        count += 1;
        if count > levels.len() + 2 {
            break;
        }
    }
    name
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
    let ceiling = p.get("ceiling").and_then(|v| v.as_f64());
    let structure = p.get("structure").and_then(|v| v.as_f64());

    if above.is_none() && below.is_none() && index.is_none() && name.is_none() {
        return Err(crate::EngineError::Other("level: specify above, below, index or name".into()));
    }

    if let Some(above_ref) = above {
        let i = find_level(arch, above_ref)?;
        let base = &arch.levels[i];
        let new_elev = base.elevation + base.floor_to_floor();
        // A new level takes `Level::default()`'s ceiling height and floor structure.
        let name_str = name.map(|n| n.trim()).filter(|n| !n.is_empty()).map(String::from);
        let auto_name = auto_level_name(&arch.levels, &base.name, arch.levels.len() + 1);
        let mut new_level = cadcraft_doc::arch::Level {
            index: arch.levels.len() as u32,
            name: name_str.unwrap_or(auto_name),
            elevation: new_elev,
            ..Default::default()
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
        let name_str = name.map(|n| n.trim()).filter(|n| !n.is_empty()).map(String::from);

        let auto_name = if i == 0 && name_str.is_none() { "Basement".to_string() } else { auto_level_name(&arch.levels, &base.name, i + 1) };

        let mut new_level =
            cadcraft_doc::arch::Level { index: i as u32, name: name_str.unwrap_or(auto_name), elevation: new_elev, ..Default::default() };
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
                w.base_level = w.base_level.saturating_add(1);
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
    if p.get("ceiling").is_some() {
        let c = ceiling.ok_or_else(|| crate::EngineError::Other("level: ceiling must be a number".into()))?;
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
            .params("{above?: name|index, below?: name|index, index?: n, name?: s, ceiling?: f, structure?: f}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use cadcraft_doc::arch::{Level, Wall};
    use cadcraft_geom::Vec2;
    use serde_json::json;

    fn setup_session() -> Session {
        Session::new()
    }

    #[test]
    fn level_above_uses_the_derived_floor_to_floor() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        let result = s.execute("level", &json!({"above": "Level 1"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        assert!((arch.levels[1].elevation - 2.7).abs() < 1e-9);
    }

    #[test]
    fn level_above_takes_the_default_ceiling_and_structure() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        s.execute("level", &json!({"above": "Level 1"})).unwrap();
        let doc = s.doc().unwrap();
        let added = &doc.arch.levels[1];
        assert!((added.ceiling_height - 2.4).abs() < 1e-9);
        assert!((added.structure - 0.3).abs() < 1e-9);
    }

    #[test]
    fn level_below_renumbers_walls() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
            doc.arch.levels.push(Level { index: 1, name: "Level 2".into(), elevation: 2.7, ..Default::default() });
            doc.arch.walls.push(Wall {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(10.0, 0.0),
                base_level: 1,
                to_ceiling: true,
                ..Default::default()
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
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        let result = s.execute("level", &json!({"above": "Level 99"}));
        assert!(result.is_err());
    }

    #[test]
    fn level_rejects_a_non_positive_ceiling() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        let result = s.execute("level", &json!({"name": "Level 1", "ceiling": -1.0}));
        assert!(result.is_err());
    }

    #[test]
    fn level_below_uses_the_requested_name() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        let _ = s.execute("level", &json!({"below": "Level 1", "name": "Basement"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        assert_eq!(arch.levels[0].name, "Basement");
        assert_eq!(arch.levels[1].name, "Level 1");
    }

    #[test]
    fn level_above_names_the_new_level_by_position() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        let _ = s.execute("level", &json!({"above": "Level 1"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        assert_eq!(arch.levels[1].name, "Level 2");
    }

    #[test]
    fn level_above_names_the_new_level_after_a_basement() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        // Add a basement below Level 1
        s.execute("level", &json!({"name": "Basement", "below": "Level 1", "ceiling": 2.4, "structure": 0.3})).unwrap();
        // Add a level above Level 1
        s.execute("level", &json!({"above": "Level 1"})).unwrap();

        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 3);
        assert_eq!(arch.levels[0].name, "Basement");
        assert_eq!(arch.levels[1].name, "Level 1");
        assert_eq!(arch.levels[2].name, "Level 2");

        // Check elevations: Basement (-2.7), Level 1 (0.0), Level 2 (2.7)
        assert!((arch.levels[0].elevation - (-2.7)).abs() < 1e-9);
        assert!((arch.levels[1].elevation - 0.0).abs() < 1e-9);
        assert!((arch.levels[2].elevation - 2.7).abs() < 1e-9);
    }

    #[test]
    fn level_below_the_bottom_level_defaults_to_basement() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        // Add a level below Level 1 without specifying a name
        s.execute("level", &json!({"below": "Level 1"})).unwrap();

        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        assert_eq!(arch.levels[0].name, "Basement");
        assert_eq!(arch.levels[1].name, "Level 1");
    }

    #[test]
    fn level_auto_names_never_collide() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
            doc.arch.levels.push(Level { name: "Level 2".into(), elevation: 2.7, ..Default::default() });
        }
        // Add a level above Level 1. Auto name would try "Level 2", which collides.
        // It should increment to "Level 3".
        s.execute("level", &json!({"above": "Level 1"})).unwrap();

        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 3);
        assert_eq!(arch.levels[0].name, "Level 1");
        assert_eq!(arch.levels[1].name, "Level 2");
        assert_eq!(arch.levels[2].name, "Level 3");
    }

    #[test]
    fn level_auto_name_i32_max_neighbour() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 2147483647".into(), ..Default::default() });
        }
        s.execute("level", &json!({"above": "Level 2147483647"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        assert_eq!(arch.levels[1].name, "Level 2147483648");
    }

    #[test]
    fn level_auto_name_non_auto_neighbour() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Ground".into(), ..Default::default() });
        }
        s.execute("level", &json!({"above": "Ground"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 2);
        // Neighbour "Ground" is not auto-named, so fallback = levels.len() = 2
        assert_eq!(arch.levels[1].name, "Level 2");
    }

    #[test]
    fn level_auto_name_all_names_collide() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
            doc.arch.levels.push(Level { name: "Level 2".into(), elevation: 2.7, ..Default::default() });
            doc.arch.levels.push(Level { name: "Level 3".into(), elevation: 5.4, ..Default::default() });
        }
        s.execute("level", &json!({"above": "Level 1"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 4);
        assert_eq!(arch.levels[3].name, "Level 4");
    }

    #[test]
    fn level_find_by_name_that_looks_like_index() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
            doc.arch.levels.push(Level { name: "Level 2".into(), elevation: 2.7, ..Default::default() });
        }
        // "1" as a string: try index 1 first (Level 2), not name "1"
        s.execute("level", &json!({"above": "1"})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        assert_eq!(arch.levels.len(), 3);
        // Above Level 2 (index 1), so new level has elevation 2.7 + 2.7 = 5.4
        assert!((arch.levels[2].elevation - 5.4).abs() < 1e-9);
    }

    #[test]
    fn level_edit_with_both_index_and_name() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
            doc.arch.levels.push(Level { name: "Level 2".into(), elevation: 2.7, ..Default::default() });
        }
        // Both index 0 and name "Level 2" — name should win
        s.execute("level", &json!({"index": 0, "name": "Level 2", "ceiling": 3.0})).unwrap();
        let doc = s.doc().unwrap();
        let arch = &doc.arch;
        // Level 2 (index 1) should have ceiling 3.0, Level 1 should be unchanged
        assert!((arch.levels[0].ceiling_height - 2.4).abs() < 1e-9);
        assert!((arch.levels[1].ceiling_height - 3.0).abs() < 1e-9);
    }

    #[test]
    fn level_rejects_nan_ceiling() {
        let mut s = setup_session();
        {
            let doc = s.doc_mut().unwrap();
            doc.arch.levels.clear();
            doc.arch.levels.push(Level { name: "Level 1".into(), ..Default::default() });
        }
        let mut map = serde_json::Map::new();
        map.insert("name".to_string(), Value::from("Level 1"));
        map.insert("ceiling".to_string(), Value::from(f64::NAN));
        let result = s.execute("level", &Value::Object(map));
        assert!(result.is_err());
    }

    #[test]
    fn is_auto_level_name_edge_cases() {
        assert!(!is_auto_level_name(""));
        assert!(!is_auto_level_name("Level"));
        assert!(is_auto_level_name("Level 0"));
        assert!(is_auto_level_name("Level 1"));
        assert!(is_auto_level_name("LEVEL 1"));
        assert!(is_auto_level_name("Level  1"));
        assert!(is_auto_level_name("Level -1"));
        assert!(!is_auto_level_name("Level 1.5"));
        assert!(!is_auto_level_name("Level 1abc"));
        assert!(!is_auto_level_name("Floor 1"));
    }
}
