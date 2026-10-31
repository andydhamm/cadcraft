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
        None => json!({
            "latitude": Value::Null,
            "longitude": Value::Null,
            "elevation": Value::Null,
            "trueNorth": Value::Null,
            "reference": Value::Null,
            "northArrow": arrow,
        }),
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

    #[test]
    fn inspect_site_terrain_only_has_north_arrow() {
        let mut m = ArchModel::default();
        m.terrain = Some(Terrain::default());
        let out = run(&m, json!({"site": true}));
        let s = &out["site"];
        // Terrain without a site location: site object must exist with a north arrow.
        assert!(s.is_object(), "site should be an object when terrain exists: {s}");
        assert!(s.get("northArrow").is_some(), "northArrow should be present: {s}");
        // Site-specific fields are null when there is no site location.
        assert!(s["latitude"].is_null());
        assert!(s["longitude"].is_null());
        assert!(s["elevation"].is_null());
        assert!(s["trueNorth"].is_null());
        assert!(s["reference"].is_null());
    }

    #[test]
    fn inspect_terrain_degenerate_boundary_gives_null_tin() {
        let mut m = ArchModel::default();
        // Empty boundary: original_tin should fail, triangles and range become null.
        m.terrain = Some(Terrain::default());
        let out = run(&m, json!({"terrain": true}));
        let t = &out["terrain"];
        assert!(t["triangles"].is_null(), "triangles should be null for empty boundary: {t}");
        assert!(t["range"].is_null(), "range should be null for empty boundary: {t}");
    }

    #[test]
    fn inspect_handle_formats() {
        let mut m = ArchModel::default();
        m.terrain = Some(Terrain {
            modifiers: vec![
                TerrainModifier { id: 0, kind: ModifierKind::Hill { center: Vec2::new(1.0, 1.0), height: 1.0, radius: 1.0 } },
                TerrainModifier { id: 5, kind: ModifierKind::Flat { polygon: square(1.0), elevation: 0.0, margin: 0.1 } },
            ],
            next_modifier_id: 6,
            ..Terrain::default()
        });
        m.plants.push(Plant { id: ArchId(3), species: "boxwood".into(), position: Vec2::new(1.0, 1.0), age: 0.0, planted_height: None });
        let out = run(&m, json!({"terrain": true, "plants": true}));
        // Handles are lowercase hex strings starting with the arch handle base prefix.
        let terrain_handle = out["terrain"]["handle"].as_str().unwrap_or("");
        assert!(terrain_handle.starts_with("a0"), "terrain handle format: {terrain_handle}");
        let mod0_handle = out["terrain"]["modifiers"][0]["handle"].as_str().unwrap_or("");
        assert!(mod0_handle.starts_with("a0"), "modifier handle format: {mod0_handle}");
        let mod1_handle = out["terrain"]["modifiers"][1]["handle"].as_str().unwrap_or("");
        assert!(mod1_handle.starts_with("a0"), "modifier handle format: {mod1_handle}");
        let plant_handle = out["plants"][0]["handle"].as_str().unwrap_or("");
        assert!(plant_handle.starts_with("a0"), "plant handle format: {plant_handle}");
        // Modifier id labels use the tm prefix with the numeric id.
        assert_eq!(out["terrain"]["modifiers"][0]["id"], json!("tm0"));
        assert_eq!(out["terrain"]["modifiers"][1]["id"], json!("tm5"));
        // Plant id label.
        assert_eq!(out["plants"][0]["id"], json!("pl3"));
    }

    #[test]
    fn inspect_nan_fields_do_not_panic() {
        let mut m = ArchModel::default();
        m.site = Some(SiteLocation {
            latitude: f64::NAN,
            longitude: f64::NAN,
            elevation: f64::NAN,
            true_north: f64::NAN,
            reference: None,
            north_arrow_at: None,
        });
        // NaN values must not panic; they serialize to null in JSON.
        let out = run(&m, json!({"site": true}));
        assert!(out.contains_key("site"));
        assert!(out["site"]["latitude"].is_null());
    }

    #[test]
    fn inspect_all_terrain_modifier_types() {
        let mut m = ArchModel::default();
        m.terrain = Some(Terrain {
            modifiers: vec![
                TerrainModifier { id: 0, kind: ModifierKind::Flat { polygon: square(2.0), elevation: 1.5, margin: 0.3 } },
                TerrainModifier { id: 1, kind: ModifierKind::Hill { center: Vec2::new(5.0, 5.0), height: 2.0, radius: 4.0 } },
                TerrainModifier {
                    id: 2,
                    kind: ModifierKind::Path { kind: PathKind::Driveway, polygon: square(1.0), thickness: 0.15, material: "concrete".into() },
                },
            ],
            next_modifier_id: 3,
            ..Terrain::default()
        });
        let out = run(&m, json!({"terrain": true}));
        let mods = &out["terrain"]["modifiers"];
        assert_eq!(mods.len(), 3);
        // Flat modifier fields.
        assert_eq!(mods[0]["type"], json!("flat"));
        assert_eq!(mods[0]["elevation"], json!(1.5));
        assert_eq!(mods[0]["margin"], json!(0.3));
        assert!(mods[0]["polygon"].is_array());
        // Hill modifier fields.
        assert_eq!(mods[1]["type"], json!("hill"));
        assert_eq!(mods[1]["height"], json!(2.0));
        assert_eq!(mods[1]["radius"], json!(4.0));
        assert_eq!(mods[1]["center"], json!([5.0, 5.0]));
        // Path modifier fields.
        assert_eq!(mods[2]["type"], json!("path"));
        assert_eq!(mods[2]["kind"], json!("driveway"));
        assert_eq!(mods[2]["thickness"], json!(0.15));
        assert_eq!(mods[2]["material"], json!("concrete"));
        assert!(mods[2]["polygon"].is_array());
    }
}
