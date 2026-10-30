//! `plant` and `plant.species`: plants from the catalogue and user species (phase 9).

use std::fs;

use cadcraft_arch::plant::{catalogue, find_species, plant_size, validate_species};
use cadcraft_doc::arch::{ArchId, CrownShape, MAX_PLANT_SPECIES, MAX_PLANTS, Plant, PlantForm, PlantSpecies};
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, point_req, str_param};
use crate::{Result, Session};

/// Species list `add` / `file` command.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plant", "Plant", run_plant)
            .menu(&["Architecture", "Plant"])
            .params("{species, at: [x,y], age?: years, plantedHeight?: m, delete?: \"pl0\"}"),
        CommandSpec::new("plant.species", "Plant Species", run_species).params("{add?: [species...], file?: path}").noundo(),
    ]
}

fn parse_plant_label(label: &str) -> Option<ArchId> {
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    let digits = label.strip_prefix('p').or_else(|| label.strip_prefix('P')).unwrap_or(label);
    let digits = digits.strip_prefix('l').or_else(|| digits.strip_prefix('L')).unwrap_or(digits);
    let n = digits.parse::<u32>().ok()?;
    Some(ArchId(n))
}

fn parse_form_value(value: &str) -> Option<PlantForm> {
    match value.trim().to_ascii_lowercase().as_str() {
        "deciduous-tree" | "deciduous tree" => Some(PlantForm::DeciduousTree),
        "evergreen-tree" | "evergreen tree" => Some(PlantForm::EvergreenTree),
        "shrub" => Some(PlantForm::Shrub),
        "ground-cover" | "ground cover" => Some(PlantForm::GroundCover),
        _ => None,
    }
}

fn parse_crown_value(value: &str) -> Option<CrownShape> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sphere" => Some(CrownShape::Sphere),
        "cone" => Some(CrownShape::Cone),
        "ellipsoid" => Some(CrownShape::Ellipsoid),
        _ => None,
    }
}

fn species_to_json(species: &PlantSpecies, builtin: bool) -> Value {
    json!({
        "name": species.name,
        "form": match species.form {
            PlantForm::DeciduousTree => "deciduous-tree",
            PlantForm::EvergreenTree => "evergreen-tree",
            PlantForm::Shrub => "shrub",
            PlantForm::GroundCover => "ground-cover",
        },
        "crown": match species.crown {
            CrownShape::Sphere => "sphere",
            CrownShape::Cone => "cone",
            CrownShape::Ellipsoid => "ellipsoid",
        },
        "matureHeight": species.mature_height,
        "matureSpread": species.mature_spread,
        "plantedHeight": species.planted_height,
        "yearsToMature": species.years_to_mature,
        "builtin": builtin,
    })
}

fn parse_species_object(value: &Value) -> Result<PlantSpecies> {
    let name =
        str_param(value, "name").map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| bad("plant.species", "`name` is required"))?.to_string();

    let form = value
        .get("form")
        .and_then(Value::as_str)
        .and_then(parse_form_value)
        .ok_or_else(|| bad("plant.species", format!("invalid `form` for `{name}`")))?;

    let crown = value
        .get("crown")
        .and_then(Value::as_str)
        .and_then(parse_crown_value)
        .ok_or_else(|| bad("plant.species", format!("invalid `crown` for `{name}`")))?;

    let mature_height = value
        .get("matureHeight")
        .or_else(|| value.get("mature_height"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad("plant.species", format!("`matureHeight` for `{name}` is required")))?;

    let mature_spread = value
        .get("matureSpread")
        .or_else(|| value.get("mature_spread"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad("plant.species", format!("`matureSpread` for `{name}` is required")))?;

    let planted_height = value
        .get("plantedHeight")
        .or_else(|| value.get("planted_height"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad("plant.species", format!("`plantedHeight` for `{name}` is required")))?;

    let years_to_mature = value
        .get("yearsToMature")
        .or_else(|| value.get("years_to_mature"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad("plant.species", format!("`yearsToMature` for `{name}` is required")))?;

    let mut species = PlantSpecies { name, form, crown, mature_height, mature_spread, planted_height, years_to_mature };
    validate_species(&species).map_err(|msg| bad("plant.species", format!("{msg} (`{}`)", species.name)))?;
    species.name = species.name.trim().to_string();
    Ok(species)
}

fn read_species_file(path: &str) -> Result<Vec<PlantSpecies>> {
    if cfg!(target_arch = "wasm32") {
        return Err(bad("plant.species", "`file` is not supported in wasm"));
    }
    let text = fs::read_to_string(path).map_err(|err| bad("plant.species", format!("could not read `{path}`: {err}")))?;
    let value: Value = serde_json::from_str(&text).map_err(|err| bad("plant.species", format!("invalid species file `{path}`: {err}")))?;
    let values = value.as_array().ok_or_else(|| bad("plant.species", format!("`file` must hold an array of species in `{path}`")))?;
    values.iter().map(parse_species_object).collect()
}

fn run_plant(session: &mut Session, params: &Value) -> Result<Value> {
    if let Some(value) = str_param(params, "delete") {
        let label = value.trim();
        let id = parse_plant_label(label).ok_or_else(|| bad("plant", format!("no such plant `{label}`")))?;
        let doc = session.doc_mut()?;
        let idx = doc.arch.plants.iter().position(|plant| plant.id == id).ok_or_else(|| bad("plant", format!("no such plant `{label}`")))?;
        doc.arch.plants.remove(idx);
        doc.touch_arch();
        return Ok(json!({ "plant": format!("pl{}", id.0) }));
    }

    let species_name = str_param(params, "species").map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| bad("plant", "`species` is required"))?;

    let model = session.doc()?.arch.clone();
    let species =
        find_species(&model, species_name).ok_or_else(|| bad("plant", format!("unknown species `{species_name}`; plant.species lists them")))?;

    let age = match params.get("age") {
        None | Some(Value::Null) => 0.0,
        Some(value) => value.as_f64().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| bad("plant", "`age` must be a finite number >= 0"))?,
    };

    let planted_height = match params.get("plantedHeight") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let h = value
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= species.mature_height)
                .ok_or_else(|| bad("plant", format!("`plantedHeight` must be finite, > 0 and <= {}", species.mature_height)))?;
            Some(h)
        }
    };

    let at = point_req("plant", params, "at")?;
    if !at.is_finite() {
        return Err(bad("plant", "`at` must hold finite coordinates"));
    }

    let doc = session.doc_mut()?;
    if doc.arch.plants.len() >= MAX_PLANTS {
        return Err(bad("plant", format!("at most {} plants are allowed", MAX_PLANTS)));
    }

    let id = ArchId(doc.arch.next_plant_id);
    doc.arch.next_plant_id = doc.arch.next_plant_id.saturating_add(1);
    let plant = Plant { id, species: species.name.clone(), position: at, age, planted_height };
    doc.arch.plants.push(plant.clone());
    doc.touch_arch();

    let (height, spread) = plant_size(&species, &plant);
    Ok(json!({ "plant": format!("pl{}", id.0), "height": height, "spread": spread }))
}

fn run_species(session: &mut Session, params: &Value) -> Result<Value> {
    if params.as_object().is_some_and(|obj| obj.is_empty()) {
        let doc = session.doc()?;
        let mut species = catalogue().into_iter().map(|sp| species_to_json(&sp, true)).collect::<Vec<_>>();
        species.extend(doc.arch.plant_species.iter().map(|sp| species_to_json(sp, false)));
        return Ok(json!({ "species": species }));
    }

    let mut parsed = Vec::new();
    if let Some(add) = params.get("add") {
        let values = add.as_array().ok_or_else(|| bad("plant.species", "`add` must be an array of species objects"))?;
        for value in values {
            if !value.is_object() {
                return Err(bad("plant.species", "each `add` item must be a species object"));
            }
            parsed.push(parse_species_object(value)?);
        }
    }
    if let Some(path) = params.get("file") {
        let path = path.as_str().ok_or_else(|| bad("plant.species", "`file` must be a path string"))?;
        parsed.extend(read_species_file(path)?);
    }

    if parsed.is_empty() {
        let doc = session.doc()?;
        let mut species = catalogue().into_iter().map(|sp| species_to_json(&sp, true)).collect::<Vec<_>>();
        species.extend(doc.arch.plant_species.iter().map(|sp| species_to_json(sp, false)));
        return Ok(json!({ "species": species }));
    }

    let doc = session.doc()?;
    let builtins = catalogue();
    let mut existing = doc.arch.plant_species.clone();
    let mut added_names = Vec::new();

    for sp in parsed {
        let name = sp.name.trim().to_string();
        if builtins.iter().any(|builtin| builtin.name.eq_ignore_ascii_case(&name)) {
            return Err(bad("plant.species", format!("`{name}` is already a built-in species")));
        }

        if let Some(index) = existing.iter().position(|existing| existing.name.eq_ignore_ascii_case(&name)) {
            existing[index] = sp.clone();
        } else {
            existing.push(sp.clone());
        }
        added_names.push(name.clone());
        if existing.len() > MAX_PLANT_SPECIES {
            return Err(bad("plant.species", format!("at most {} plant species are allowed", MAX_PLANT_SPECIES)));
        }
    }

    let doc = session.doc_mut()?;
    doc.arch.plant_species = existing;
    doc.touch_arch();
    Ok(json!({ "added": added_names }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plant_places_a_catalogue_species() {
        let mut session = Session::new();
        let out = run_plant(&mut session, &json!({"species": "Colorado Blue Spruce", "at": [10.0, 10.0], "age": 30.0})).unwrap();
        assert_eq!(out["plant"], "pl0");
        assert!((out["height"].as_f64().unwrap() - 15.0).abs() < 1e-9);
        assert!((out["spread"].as_f64().unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn plant_grows_with_age() {
        let mut session = Session::new();
        let out = run_plant(&mut session, &json!({"species": "Colorado Blue Spruce", "at": [0.0, 0.0], "age": 15.0})).unwrap();
        let height = out["height"].as_f64().unwrap();
        let spread = out["spread"].as_f64().unwrap();
        assert!((height - 8.25).abs() < 1e-9);
        assert!((spread - 2.75).abs() < 1e-9);
    }

    #[test]
    fn plant_refuses_unknown_species_and_bad_age() {
        let mut session = Session::new();
        let err = run_plant(&mut session, &json!({"species": "Nonesuch", "at": [0.0, 0.0]})).unwrap_err();
        assert!(matches!(err, crate::EngineError::BadParams { .. }));
        let err = run_plant(&mut session, &json!({"species": "Colorado Blue Spruce", "at": [0.0, 0.0], "age": -1.0})).unwrap_err();
        assert!(matches!(err, crate::EngineError::BadParams { .. }));
    }

    #[test]
    fn plant_species_lists_the_catalogue() {
        let mut session = Session::new();
        let out = run_species(&mut session, &json!({})).unwrap();
        let species = out["species"].as_array().unwrap();
        assert!(species.iter().any(|item| item["name"].as_str() == Some("Colorado Blue Spruce")));
        assert!(species.iter().any(|item| item["builtin"].as_bool() == Some(true)));
    }

    #[test]
    fn plant_species_adds_from_json_and_refuses_duplicates_of_builtins() {
        let mut session = Session::new();
        let file = std::env::temp_dir().join("cadcraft-plant-species.json");
        std::fs::write(
            &file,
            r#"[{"name":"Saskatoon","form":"shrub","crown":"sphere","matureHeight":4.0,"matureSpread":3.0,"plantedHeight":1.0,"yearsToMature":10.0}]"#,
        )
        .unwrap();
        let added = run_species(&mut session, &json!({"file": file.to_string_lossy().as_ref()})).unwrap();
        assert_eq!(added["added"].as_array().unwrap()[0], "Saskatoon");

        let err = run_species(&mut session, &json!({"add": [{"name": "Red Oak", "form": "shrub", "crown": "sphere", "matureHeight": 1.0, "matureSpread": 1.0, "plantedHeight": 0.5, "yearsToMature": 2.0}]})).unwrap_err();
        assert!(matches!(err, crate::EngineError::BadParams { .. }));
    }
}
