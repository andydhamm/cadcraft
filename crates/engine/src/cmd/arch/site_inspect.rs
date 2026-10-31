//! The site's part of `arch.inspect` (phase 9): `{"site":true}`, `{"terrain":true}` and
//! `{"plants":true}`.

use cadcraft_arch::plant::{find_species, plant_size};
use cadcraft_arch::site::north_arrow;
use cadcraft_arch::terrain::original_tin;
use cadcraft_doc::arch::{ArchModel, ModifierKind, handle_json, plant_handle, plant_label, terrain_handle, terrain_label};
use cadcraft_geom::Vec2;
use serde_json::{Map, Value, json};

fn flag(p: &Value, key: &str) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn pt(v: Vec2) -> Value {
    json!([v.x, v.y])
}

fn ring(v: &[Vec2]) -> Value {
    Value::Array(v.iter().map(|p| pt(*p)).collect())
}

/// Add the requested site keys to `out`. Read-only.
pub(crate) fn inspect_site(model: &ArchModel, p: &Value, out: &mut Map<String, Value>) {
    if flag(p, "site") {
        out.insert("site".into(), site_json(model));
    }
    if flag(p, "terrain") {
        out.insert("terrain".into(), terrain_json(model));
    }
    if flag(p, "plants") {
        out.insert("plants".into(), plants_json(model));
    }
}

fn site_json(model: &ArchModel) -> Value {
    if model.site.is_none() && model.terrain.is_none() {
        return Value::Null;
    }
    let arrow = north_arrow(model).map(|a| json!({"at": pt(a.at), "direction": pt(a.direction)}));
    match &model.site {
        Some(s) => {
            let reference = s.reference.as_ref().map(|r| json!({"point": pt(r.point), "easting": r.easting, "northing": r.northing, "crs": r.crs}));
            json!({
                "latitude": s.latitude,
                "longitude": s.longitude,
                "elevation": s.elevation,
                "trueNorth": s.true_north,
                "reference": reference,
                "northArrow": arrow,
            })
        }
        // A terrain without a site location: only the north arrow is known.
        None => Value::Null,
    }
}

fn terrain_json(model: &ArchModel) -> Value {
    let Some(t) = &model.terrain else { return Value::Null };
    let (triangles, range) = match original_tin(t) {
        Ok(tin) => (json!(tin.triangles.len()), tin.elevation_range().map_or(Value::Null, |(lo, hi)| json!([lo, hi]))),
        Err(_) => (Value::Null, Value::Null),
    };
    let modifiers: Vec<Value> = t
        .modifiers
        .iter()
        .map(|m| {
            let mut o = Map::new();
            o.insert("id".into(), json!(terrain_label(Some(m.id))));
            o.insert("handle".into(), json!(handle_json(terrain_handle(Some(m.id)))));
            match &m.kind {
                ModifierKind::Flat { polygon, elevation, margin } => {
                    o.insert("type".into(), json!("flat"));
                    o.insert("polygon".into(), ring(polygon));
                    o.insert("elevation".into(), json!(elevation));
                    o.insert("margin".into(), json!(margin));
                }
                ModifierKind::Hill { center, height, radius } => {
                    o.insert("type".into(), json!("hill"));
                    o.insert("center".into(), pt(*center));
                    o.insert("height".into(), json!(height));
                    o.insert("radius".into(), json!(radius));
                }
                ModifierKind::Path { kind, polygon, thickness, material } => {
                    o.insert("type".into(), json!("path"));
                    o.insert("kind".into(), json!(kind.name()));
                    o.insert("polygon".into(), ring(polygon));
                    o.insert("thickness".into(), json!(thickness));
                    o.insert("material".into(), json!(material));
                }
            }
            Value::Object(o)
        })
        .collect();
    json!({
        "id": terrain_label(None),
        "handle": handle_json(terrain_handle(None)),
        "boundary": ring(&t.boundary),
        "elevation": t.default_elevation,
        "points": t.points.len(),
        "lines": t.lines.len(),
        "contours": [t.contour_minor, t.contour_major],
        "pad": t.pad,
        "padMargin": t.pad_margin,
        "triangles": triangles,
        "range": range,
        "modifiers": modifiers,
    })
}

fn plants_json(model: &ArchModel) -> Value {
    let list: Vec<Value> = model
        .plants
        .iter()
        .map(|pl| {
            let size = find_species(model, &pl.species).map(|sp| plant_size(&sp, pl));
            json!({
                "id": plant_label(pl.id),
                "handle": handle_json(plant_handle(pl.id)),
                "species": pl.species,
                "at": pt(pl.position),
                "age": pl.age,
                "height": size.map(|s| s.0),
                "spread": size.map(|s| s.1),
            })
        })
        .collect();
    Value::Array(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::ArchId;
    use cadcraft_doc::arch::{PathKind, Plant, SiteLocation, SiteReference, Terrain, TerrainModifier};
    use cadcraft_geom::Vec3;

    fn run(model: &ArchModel, p: Value) -> Map<String, Value> {
        let mut out = Map::new();
        inspect_site(model, &p, &mut out);
        out
    }

    fn square(s: f64) -> Vec<Vec2> {
        vec![Vec2::new(0.0, 0.0), Vec2::new(s, 0.0), Vec2::new(s, s), Vec2::new(0.0, s)]
    }

    #[test]
    fn inspect_reports_site_and_north_arrow() {
        let mut m = ArchModel::default();
        assert_eq!(run(&m, json!({"site": true})).get("site"), Some(&Value::Null));
        m.site = Some(SiteLocation {
            latitude: 45.0,
            longitude: -75.0,
            elevation: 70.0,
            true_north: 30.0,
            reference: Some(SiteReference { point: Vec2::new(1.0, 2.0), easting: 500000.0, northing: 4000000.0, crs: "EPSG:32618".into() }),
            north_arrow_at: Some(Vec2::new(5.0, 6.0)),
        });
        let out = run(&m, json!({"site": true}));
        let s = &out["site"];
        assert_eq!(s["latitude"], json!(45.0));
        assert_eq!(s["trueNorth"], json!(30.0));
        assert_eq!(s["reference"]["crs"], json!("EPSG:32618"));
        assert_eq!(s["northArrow"]["at"], json!([5.0, 6.0]));
        let dx = s["northArrow"]["direction"][0].as_f64().unwrap_or(0.0);
        assert!((dx + 0.5).abs() < 1e-9, "{dx}");
    }

    #[test]
    fn inspect_reports_terrain_and_modifiers() {
        let mut m = ArchModel::default();
        assert_eq!(run(&m, json!({"terrain": true})).get("terrain"), Some(&Value::Null));
        let t = Terrain {
            boundary: square(10.0),
            points: vec![Vec3::new(1.0, 1.0, 0.0), Vec3::new(9.0, 1.0, 1.0), Vec3::new(5.0, 9.0, 2.0)],
            modifiers: vec![
                TerrainModifier { id: 0, kind: ModifierKind::Hill { center: Vec2::new(5.0, 5.0), height: 1.0, radius: 3.0 } },
                TerrainModifier {
                    id: 1,
                    kind: ModifierKind::Path { kind: PathKind::Road, polygon: square(2.0), thickness: 0.1, material: "asphalt".into() },
                },
            ],
            next_modifier_id: 2,
            ..Terrain::default()
        };
        m.terrain = Some(t);
        let out = run(&m, json!({"terrain": true, "site": true}));
        let t = &out["terrain"];
        assert_eq!(t["id"], json!("t0"));
        assert_eq!(t["points"], json!(3));
        assert_eq!(t["contours"], json!([0.25, 1.0]));
        assert!(t["triangles"].as_u64().unwrap_or(0) > 0, "{t}");
        assert!(t["range"].is_array());
        assert_eq!(t["modifiers"][0]["type"], json!("hill"));
        assert_eq!(t["modifiers"][1]["id"], json!("tm1"));
        assert_eq!(t["modifiers"][1]["kind"], json!("road"));
        // A terrain alone still has a north arrow in the site key's absence of a location.
        assert!(out.contains_key("site"));
    }

    #[test]
    fn inspect_reports_plants_with_sizes() {
        let mut m = ArchModel::default();
        m.plants.push(Plant { id: ArchId(0), species: "boxwood".into(), position: Vec2::new(1.0, 2.0), age: 1000.0, planted_height: None });
        m.plants.push(Plant { id: ArchId(1), species: "Nonesuch".into(), position: Vec2::new(3.0, 4.0), ..Plant::default() });
        let out = run(&m, json!({"plants": true}));
        let list = out["plants"].as_array().cloned().unwrap_or_default();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["id"], json!("pl0"));
        assert_eq!(list[0]["at"], json!([1.0, 2.0]));
        assert!((list[0]["height"].as_f64().unwrap_or(0.0) - 1.2).abs() < 1e-9);
        assert!((list[0]["spread"].as_f64().unwrap_or(0.0) - 1.2).abs() < 1e-9);
        assert!(list[1]["height"].is_null() && list[1]["spread"].is_null());
    }

    #[test]
    fn inspect_site_keys_absent_unless_asked() {
        let mut m = ArchModel::default();
        m.terrain = Some(Terrain::default());
        m.plants.push(Plant::default());
        let out = run(&m, json!({}));
        assert!(!out.contains_key("site") && !out.contains_key("terrain") && !out.contains_key("plants"));
    }
}
