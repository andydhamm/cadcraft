//! Plants (phase 9): the built-in species catalogue, user species, growth and the plan symbol.
//! Filled in by `p9-arch-plant`.

use cadcraft_doc::arch::{ArchModel, Plant, PlantSpecies};
use cadcraft_geom::Vec2;

/// The built-in catalogue.
pub fn catalogue() -> Vec<PlantSpecies> {
    Vec::new()
}

/// A species by name (case-insensitive): the drawing's own species first, then the catalogue.
pub fn find_species(model: &ArchModel, name: &str) -> Option<PlantSpecies> {
    let _ = (model, name);
    None
}

/// Check a user species before it is stored.
pub fn validate_species(sp: &PlantSpecies) -> Result<(), String> {
    let _ = sp;
    Ok(())
}

/// `(height, spread)` of `plant` at its age.
pub fn plant_size(sp: &PlantSpecies, plant: &Plant) -> (f64, f64) {
    let _ = plant;
    (sp.planted_height, sp.mature_spread)
}

/// The plan symbol: polylines (the first is the circle at the spread).
pub fn plan_symbol(sp: &PlantSpecies, center: Vec2, spread: f64) -> Vec<Vec<Vec2>> {
    let _ = (sp, center, spread);
    Vec::new()
}
