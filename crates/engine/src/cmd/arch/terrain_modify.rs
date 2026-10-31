//! `terrain.modify`: flat regions, hills, valleys and draped paths (phase 9).

use cadcraft_arch::poly::ensure_ccw;
use cadcraft_arch::terrain::validate_terrain;
use cadcraft_doc::arch::{DEFAULT_PATH_THICKNESS, MAX_RING_POINTS, MAX_TERRAIN_MODIFIERS, ModifierKind, PathKind, TerrainModifier};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, distinct_points, f64_req, point_req, points_param, size_param};
use crate::{Result, Session};

const CMD: &str = "terrain.modify";

/// Command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(CMD, "Terrain Modifier", run_modify)
            .menu(&["Architecture", "Terrain Modifier"])
            .params("{flat?: {polygon, elevation, margin?}, hill?: {center, height, radius}, valley?: {center, depth, radius}, path?: {kind: road|driveway|sidewalk, polygon? | polyline+width, thickness?, material?}, delete?: \"tm0\"}"),
    ]
}

/// A polygon of 3..=MAX_RING_POINTS finite points, made counter-clockwise.
fn polygon_of(p: &Value, what: &str) -> Result<Vec<Vec2>> {
    let pts = points_param(p, "polygon").ok_or_else(|| bad(CMD, format!("`{what}.polygon` ([[x,y],...]) is required")))?;
    if pts.len() < 3 || pts.len() > MAX_RING_POINTS {
        return Err(bad(CMD, format!("`{what}.polygon` must have 3 to {MAX_RING_POINTS} points, got {}", pts.len())));
    }
    if pts.iter().any(|q| !q.is_finite()) {
        return Err(bad(CMD, format!("non-finite coordinate in `{what}.polygon`")));
    }
    Ok(ensure_ccw(&pts))
}

fn positive(p: &Value, key: &str) -> Result<f64> {
    let v = f64_req(CMD, p, key)?;
    if v > 0.0 { Ok(v) } else { Err(bad(CMD, format!("`{key}` must be positive"))) }
}

/// The polygon of a polyline widened to `width`: right side forward, left side back, mitred
/// joins (a mitre longer than 4·width is bevelled).
fn widen(line: &[Vec2], width: f64) -> Option<Vec<Vec2>> {
    let h = width / 2.0;
    let n = line.len();
    // Unit left normals of each segment.
    let mut normals = Vec::with_capacity(n.saturating_sub(1));
    for w in line.windows(2) {
        let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
        let len = dx.hypot(dy);
        if !(len.is_finite() && len > 1e-12) {
            return None;
        }
        normals.push((-dy / len, dx / len));
    }
    let side = |sign: f64| -> Vec<Vec2> {
        let mut out = Vec::new();
        for (i, p) in line.iter().enumerate() {
            let before = i.checked_sub(1).and_then(|k| normals.get(k));
            let after = normals.get(i);
            match (before, after) {
                (Some(a), Some(b)) => {
                    let (ax, ay) = (a.0 * sign, a.1 * sign);
                    let (bx, by) = (b.0 * sign, b.1 * sign);
                    let denom = 1.0 + ax * bx + ay * by;
                    let (mx, my) = (ax + bx, ay + by);
                    let mitre = if denom > 1e-9 { h * mx.hypot(my) / denom } else { f64::INFINITY };
                    if mitre <= 4.0 * width {
                        out.push(Vec2::new(p.x + mx / denom * h, p.y + my / denom * h));
                    } else {
                        out.push(Vec2::new(p.x + ax * h, p.y + ay * h));
                        out.push(Vec2::new(p.x + bx * h, p.y + by * h));
                    }
                }
                (Some(a), None) | (None, Some(a)) => out.push(Vec2::new(p.x + a.0 * sign * h, p.y + a.1 * sign * h)),
                (None, None) => {}
            }
        }
        out
    };
    let mut ring = side(-1.0);
    let mut left = side(1.0);
    left.reverse();
    ring.extend(left);
    if ring.iter().any(|q| !q.is_finite()) {
        return None;
    }
    let mut ring = ensure_ccw(&ring);
    ring.rotate_right(1);
    Some(ring)
}

fn parse_label(s: &str) -> Option<u32> {
    s.strip_prefix("tm")?.parse().ok()
}

fn path_kind(p: &Value) -> Result<PathKind> {
    match p.get("kind").and_then(Value::as_str) {
        Some("road") => Ok(PathKind::Road),
        Some("driveway") => Ok(PathKind::Driveway),
        Some("sidewalk") => Ok(PathKind::Sidewalk),
        _ => Err(bad(CMD, "`path.kind` (road, driveway or sidewalk) is required")),
    }
}

fn run_modify(s: &mut Session, p: &Value) -> Result<Value> {
    let keys = ["flat", "hill", "valley", "path", "delete"];
    let present: Vec<&str> = keys.iter().copied().filter(|k| p.get(*k).is_some_and(|v| !v.is_null())).collect();
    let [key] = present.as_slice() else {
        return Err(bad(CMD, "give exactly one of `flat`, `hill`, `valley`, `path` or `delete`"));
    };
    let key = *key;
    let arg = p.get(key).unwrap_or(&Value::Null);

    let d = s.doc_mut()?;
    let Some(mut t) = d.arch.terrain.clone() else {
        return Err(bad(CMD, "there is no terrain; create one with `terrain` first"));
    };

    let reply;
    if key == "delete" {
        let label = arg.as_str().ok_or_else(|| bad(CMD, "`delete` must be a modifier label such as \"tm0\""))?;
        let id = parse_label(label).ok_or_else(|| bad(CMD, format!("`{label}` is not a modifier label")))?;
        let at = t.modifiers.iter().position(|m| m.id == id).ok_or_else(|| bad(CMD, format!("no terrain modifier `{label}`")))?;
        t.modifiers.remove(at);
        reply = json!({ "deleted": format!("tm{id}") });
    } else {
        if !arg.is_object() {
            return Err(bad(CMD, format!("`{key}` must be an object")));
        }
        let kind = match key {
            "flat" => {
                let polygon = polygon_of(arg, key)?;
                let elevation = f64_req(CMD, arg, "elevation")?;
                let margin = size_param(CMD, arg, "margin", true)?.unwrap_or(0.0);
                ModifierKind::Flat { polygon, elevation, margin }
            }
            "hill" => {
                ModifierKind::Hill { center: point_req(CMD, arg, "center")?, height: positive(arg, "height")?, radius: positive(arg, "radius")? }
            }
            "valley" => {
                ModifierKind::Hill { center: point_req(CMD, arg, "center")?, height: -positive(arg, "depth")?, radius: positive(arg, "radius")? }
            }
            _ => {
                let kind = path_kind(arg)?;
                let polygon = if arg.get("polygon").is_some_and(|v| !v.is_null()) {
                    polygon_of(arg, key)?
                } else {
                    let line = points_param(arg, "polyline").ok_or_else(|| bad(CMD, "`path` needs `polygon` or `polyline` and `width`"))?;
                    if line.len() > MAX_RING_POINTS / 2 || line.iter().any(|q| !q.is_finite()) {
                        return Err(bad(CMD, format!("`path.polyline` must have finite points, at most {}", MAX_RING_POINTS / 2)));
                    }
                    let line = distinct_points(&line);
                    if line.len() < 2 {
                        return Err(bad(CMD, "`path.polyline` needs at least 2 distinct points"));
                    }
                    let width = positive(arg, "width")?;
                    widen(&line, width).ok_or_else(|| bad(CMD, "`path.polyline` cannot be widened"))?
                };
                let thickness = size_param(CMD, arg, "thickness", false)?.unwrap_or(DEFAULT_PATH_THICKNESS);
                let material = match arg.get("material") {
                    None | Some(Value::Null) => kind.default_material().to_string(),
                    Some(Value::String(m)) if !m.is_empty() => m.clone(),
                    Some(_) => return Err(bad(CMD, "`path.material` must be a non-empty string")),
                };
                ModifierKind::Path { kind, polygon, thickness, material }
            }
        };
        if t.modifiers.len() >= MAX_TERRAIN_MODIFIERS {
            return Err(bad(CMD, format!("at most {MAX_TERRAIN_MODIFIERS} terrain modifiers")));
        }
        let id = t.next_modifier_id;
        t.next_modifier_id = id.checked_add(1).ok_or_else(|| bad(CMD, "modifier ids exhausted"))?;
        t.modifiers.push(TerrainModifier { id, kind });
        reply = json!({ "modifier": format!("tm{id}") });
    }

    validate_terrain(&t).map_err(|e| bad(CMD, e.to_string()))?;
    d.arch.terrain = Some(t);
    d.touch_arch();
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::Terrain;

    fn session() -> Session {
        let mut s = Session::new();
        if let Ok(d) = s.doc_mut() {
            d.arch.terrain = Some(Terrain {
                boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 40.0), Vec2::new(0.0, 40.0)],
                ..Default::default()
            });
        }
        s
    }

    fn mods(s: &Session) -> Vec<TerrainModifier> {
        s.doc().map(|d| d.arch.terrain.clone().map(|t| t.modifiers).unwrap_or_default()).unwrap_or_default()
    }

    #[test]
    fn modify_adds_flat_hill_valley_and_path() {
        let mut s = session();
        let r = s.execute(CMD, &json!({"flat": {"polygon": [[5, 5], [5, 10], [10, 10], [10, 5]], "elevation": 1.5}})).unwrap();
        assert_eq!(r["modifier"], "tm0");
        s.execute(CMD, &json!({"hill": {"center": [20, 20], "height": 2, "radius": 6}})).unwrap();
        s.execute(CMD, &json!({"valley": {"center": [30, 30], "depth": 1, "radius": 4}})).unwrap();
        s.execute(CMD, &json!({"path": {"kind": "sidewalk", "polygon": [[0, 1], [10, 1], [10, 2], [0, 2]]}})).unwrap();
        let m = mods(&s);
        assert_eq!(m.len(), 4);
        match &m[0].kind {
            ModifierKind::Flat { polygon, elevation, margin } => {
                assert_eq!((*elevation, *margin), (1.5, 0.0));
                // The clockwise input was reversed to counter-clockwise.
                assert_eq!(polygon.first().copied(), Some(Vec2::new(10.0, 5.0)));
                assert_eq!(polygon.get(1).copied(), Some(Vec2::new(10.0, 10.0)));
            }
            k => panic!("{k:?}"),
        }
        assert!(matches!(m[1].kind, ModifierKind::Hill { height, .. } if height == 2.0));
        assert!(matches!(m[2].kind, ModifierKind::Hill { height, .. } if height == -1.0));
        match &m[3].kind {
            ModifierKind::Path { kind, thickness, material, .. } => {
                assert_eq!(*kind, PathKind::Sidewalk);
                assert_eq!(*thickness, DEFAULT_PATH_THICKNESS);
                assert_eq!(material, "concrete");
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn modify_turns_a_polyline_and_width_into_a_polygon() {
        let mut s = session();
        s.execute(CMD, &json!({"path": {"kind": "driveway", "polyline": [[20, 0], [20, 10]], "width": 3}})).unwrap();
        let m = mods(&s);
        let ModifierKind::Path { polygon, .. } = &m[0].kind else { panic!("not a path") };
        let want = [(18.5, 0.0), (21.5, 0.0), (21.5, 10.0), (18.5, 10.0)];
        assert_eq!(polygon.len(), 4);
        for (q, w) in polygon.iter().zip(want) {
            assert!((q.x - w.0).abs() < 1e-9 && (q.y - w.1).abs() < 1e-9, "{polygon:?}");
        }
        // An L-shaped road gets a mitred corner.
        let ring = widen(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)], 2.0).unwrap();
        assert_eq!(ring.len(), 6);
    }

    #[test]
    fn modify_deletes_by_label() {
        let mut s = session();
        s.execute(CMD, &json!({"hill": {"center": [20, 20], "height": 2, "radius": 6}})).unwrap();
        let r = s.execute(CMD, &json!({"delete": "tm0"})).unwrap();
        assert_eq!(r["deleted"], "tm0");
        assert!(mods(&s).is_empty());
        assert!(s.execute(CMD, &json!({"delete": "tm0"})).is_err());
        assert!(s.execute(CMD, &json!({"delete": "x"})).is_err());
    }

    #[test]
    fn modify_refuses_without_terrain_and_bad_values() {
        let mut bare = Session::new();
        assert!(bare.execute(CMD, &json!({"hill": {"center": [0, 0], "height": 1, "radius": 1}})).is_err());
        let mut s = session();
        let bads = [
            json!({}),
            json!({"hill": {"center": [0, 0], "height": 1, "radius": 1}, "valley": {"center": [0, 0], "depth": 1, "radius": 1}}),
            json!({"hill": {"center": [20, 20], "height": 0, "radius": 1}}),
            json!({"hill": {"center": [20, 20], "height": 1, "radius": -1}}),
            json!({"valley": {"center": [20, 20], "depth": -1, "radius": 1}}),
            json!({"flat": {"polygon": [[1, 1], [2, 2]], "elevation": 0}}),
            json!({"flat": {"polygon": [[1, 1], [5, 1], [5, 5]], "elevation": 0, "margin": -1}}),
            json!({"path": {"polygon": [[1, 1], [5, 1], [5, 5]]}}),
            json!({"path": {"kind": "road", "polyline": [[1, 1], [5, 1]]}}),
            json!({"path": {"kind": "road", "polyline": [[1, 1], [5, 1]], "width": 0}}),
            json!({"path": {"kind": "road", "polyline": [[1, 1]], "width": 2}}),
            json!({"path": {"kind": "road", "polyline": [[1, 1], [5, 1]], "width": 2, "thickness": 0}}),
            json!({"path": {"kind": "lane", "polyline": [[1, 1], [5, 1]], "width": 2}}),
        ];
        for b in &bads {
            assert!(s.execute(CMD, b).is_err(), "{b}");
        }
        assert!(mods(&s).is_empty());
    }

    #[test]
    fn modify_ids_keep_counting_after_delete() {
        let mut s = session();
        let hill = json!({"hill": {"center": [20, 20], "height": 2, "radius": 6}});
        s.execute(CMD, &hill).unwrap();
        s.execute(CMD, &hill).unwrap();
        s.execute(CMD, &json!({"delete": "tm1"})).unwrap();
        let r = s.execute(CMD, &hill).unwrap();
        assert_eq!(r["modifier"], "tm2");
        let ids: Vec<u32> = mods(&s).iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![0, 2]);
    }

    #[test]
    fn modify_accepts_huge_finite_values() {
        let mut s = session();
        s.execute(CMD, &json!({"hill": {"center": [20, 20], "height": 1e300, "radius": 1e300}})).unwrap();
        s.execute(CMD, &json!({"flat": {"polygon": [[0, 0], [1e300, 0], [1e300, 1e300], [0, 1e300]], "elevation": 1e300}})).unwrap();
        let m = mods(&s);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn modify_accepts_polygon_with_coincident_points() {
        let mut s = session();
        s.execute(CMD, &json!({"flat": {"polygon": [[0, 0], [0, 0], [10, 0], [10, 10], [0, 10]], "elevation": 0}})).unwrap();
        let m = mods(&s);
        assert_eq!(m.len(), 1);
        match &m[0].kind {
            ModifierKind::Flat { polygon, .. } => {
                assert!(polygon.len() >= 3);
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn modify_accepts_polygon_with_collinear_points() {
        let mut s = session();
        s.execute(CMD, &json!({"flat": {"polygon": [[0, 0], [5, 0], [10, 0], [10, 10], [0, 10]], "elevation": 0}})).unwrap();
        let m = mods(&s);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn modify_widen_with_three_collinear_points() {
        let ring = widen(&[Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0)], 2.0).unwrap();
        assert_eq!(ring.len(), 6);
        for p in &ring {
            assert!((p.y - (-1.0)).abs() < 1e-9 || (p.y - 1.0).abs() < 1e-9, "y = {}", p.y);
        }
    }

    #[test]
    fn modify_path_polygon_takes_precedence_over_polyline() {
        let mut s = session();
        s.execute(CMD, &json!({"path": {"kind": "road", "polygon": [[0, 0], [10, 0], [10, 1], [0, 1]], "polyline": [[0, 0], [10, 0]], "width": 2}}))
            .unwrap();
        let m = mods(&s);
        let ModifierKind::Path { polygon, .. } = &m[0].kind else { panic!("not a path") };
        assert_eq!(polygon.len(), 4);
    }

    #[test]
    fn modify_path_ignores_width_when_polygon_given() {
        let mut s = session();
        s.execute(CMD, &json!({"path": {"kind": "road", "polygon": [[0, 0], [10, 0], [10, 1], [0, 1]], "width": 5}})).unwrap();
        let m = mods(&s);
        let ModifierKind::Path { polygon, .. } = &m[0].kind else { panic!("not a path") };
        assert_eq!(polygon.len(), 4);
    }

    #[test]
    fn modify_adds_to_terrain_with_existing_modifiers() {
        let mut s = session();
        s.execute(CMD, &json!({"hill": {"center": [10, 10], "height": 1, "radius": 3}})).unwrap();
        s.execute(CMD, &json!({"hill": {"center": [30, 30], "height": 2, "radius": 4}})).unwrap();
        let m = mods(&s);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].id, 0);
        assert_eq!(m[1].id, 1);
    }
}
