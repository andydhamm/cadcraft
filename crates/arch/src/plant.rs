//! Plants (phase 9): the built-in species catalogue, user species, growth and the plan symbol.

use cadcraft_doc::arch::{ArchModel, Plant, PlantForm, PlantSpecies};
use cadcraft_geom::Vec2;
use std::f64::consts::PI;

/// The built-in catalogue.
pub fn catalogue() -> Vec<PlantSpecies> {
    let mut v = Vec::new();
    v.push(species("Sugar Maple", PlantForm::DeciduousTree, cadcraft_doc::arch::CrownShape::Sphere, 20.0, 12.0, 2.5, 40.0));
    v.push(species("Red Oak", PlantForm::DeciduousTree, cadcraft_doc::arch::CrownShape::Sphere, 22.0, 15.0, 2.5, 50.0));
    v.push(species("Paper Birch", PlantForm::DeciduousTree, cadcraft_doc::arch::CrownShape::Ellipsoid, 15.0, 8.0, 2.0, 25.0));
    v.push(species("Trembling Aspen", PlantForm::DeciduousTree, cadcraft_doc::arch::CrownShape::Ellipsoid, 15.0, 6.0, 2.0, 20.0));
    v.push(species("Colorado Blue Spruce", PlantForm::EvergreenTree, cadcraft_doc::arch::CrownShape::Cone, 15.0, 5.0, 1.5, 30.0));
    v.push(species("Eastern White Pine", PlantForm::EvergreenTree, cadcraft_doc::arch::CrownShape::Cone, 25.0, 10.0, 1.5, 40.0));
    v.push(species("Douglas Fir", PlantForm::EvergreenTree, cadcraft_doc::arch::CrownShape::Cone, 30.0, 8.0, 1.5, 50.0));
    v.push(species("Common Lilac", PlantForm::Shrub, cadcraft_doc::arch::CrownShape::Sphere, 3.0, 2.5, 0.6, 8.0));
    v.push(species("Red Osier Dogwood", PlantForm::Shrub, cadcraft_doc::arch::CrownShape::Sphere, 2.5, 3.0, 0.5, 5.0));
    v.push(species("Boxwood", PlantForm::Shrub, cadcraft_doc::arch::CrownShape::Sphere, 1.2, 1.2, 0.3, 10.0));
    v.push(species("Mugo Pine", PlantForm::Shrub, cadcraft_doc::arch::CrownShape::Ellipsoid, 1.5, 3.0, 0.4, 15.0));
    v.push(species("Creeping Juniper", PlantForm::GroundCover, cadcraft_doc::arch::CrownShape::Ellipsoid, 0.3, 2.0, 0.1, 6.0));
    v.push(species("Wild Strawberry", PlantForm::GroundCover, cadcraft_doc::arch::CrownShape::Ellipsoid, 0.2, 0.6, 0.05, 3.0));
    v.push(species("Bearberry", PlantForm::GroundCover, cadcraft_doc::arch::CrownShape::Ellipsoid, 0.2, 1.5, 0.05, 5.0));
    v
}

fn species(
    name: &str,
    form: PlantForm,
    crown: cadcraft_doc::arch::CrownShape,
    mature_height: f64,
    mature_spread: f64,
    planted_height: f64,
    years_to_mature: f64,
) -> PlantSpecies {
    PlantSpecies { name: name.to_string(), form, crown, mature_height, mature_spread, planted_height, years_to_mature }
}

/// A species by name (case-insensitive): the drawing's own species first, then the catalogue.
pub fn find_species(model: &ArchModel, name: &str) -> Option<PlantSpecies> {
    let lower = name.to_lowercase();
    if let Some(sp) = model.plant_species.iter().find(|s| s.name.to_lowercase() == lower) {
        return Some(sp.clone());
    }
    catalogue().into_iter().find(|s| s.name.to_lowercase() == lower)
}

/// Check a user species before it is stored.
pub fn validate_species(sp: &PlantSpecies) -> Result<(), String> {
    if sp.name.is_empty() || sp.name.len() > 64 {
        return Err("name must be 1..=64 chars".into());
    }
    if !sp.mature_height.is_finite() || sp.mature_height <= 0.0 {
        return Err("mature_height must be finite and > 0".into());
    }
    if !sp.mature_spread.is_finite() || sp.mature_spread <= 0.0 {
        return Err("mature_spread must be finite and > 0".into());
    }
    if !sp.planted_height.is_finite() || sp.planted_height <= 0.0 {
        return Err("planted_height must be finite and > 0".into());
    }
    if sp.planted_height > sp.mature_height {
        return Err("planted_height must be <= mature_height".into());
    }
    if !sp.years_to_mature.is_finite() || sp.years_to_mature <= 0.0 || sp.years_to_mature > 500.0 {
        return Err("years_to_mature must be finite in (0, 500]".into());
    }
    if sp.mature_height > 150.0 {
        return Err("mature_height must be <= 150".into());
    }
    if sp.mature_spread > 100.0 {
        return Err("mature_spread must be <= 100".into());
    }
    Ok(())
}

/// `(height, spread)` of `plant` at its age.
pub fn plant_size(sp: &PlantSpecies, plant: &Plant) -> (f64, f64) {
    let planted = plant.planted_height.unwrap_or(sp.planted_height);
    let age = if plant.age.is_finite() { plant.age } else { 0.0 };
    let f = if sp.years_to_mature > 0.0 { (age / sp.years_to_mature).clamp(0.0, 1.0) } else { 1.0 };
    let height = planted + (sp.mature_height - planted) * f;
    let spread = if sp.mature_height > 0.0 { sp.mature_spread * height / sp.mature_height } else { 0.0 };
    (height, spread)
}

/// The plan symbol: polylines (the first is the circle at the spread).
pub fn plan_symbol(sp: &PlantSpecies, center: Vec2, spread: f64) -> Vec<Vec<Vec2>> {
    if !spread.is_finite() || spread <= 0.0 {
        return Vec::new();
    }
    let r = spread / 2.0;
    let mut out = Vec::new();

    // 1. Closed 48-point circle of radius r
    let mut circle = Vec::with_capacity(49);
    for i in 0..48 {
        let a = (i as f64) * 2.0 * PI / 48.0;
        circle.push(center + Vec2::new(a.cos() * r, a.sin() * r));
    }
    circle.push(circle[0]);
    out.push(circle);

    match sp.form {
        PlantForm::DeciduousTree => {
            // 8 radial lines from 0.25r to 0.9r
            for i in 0..8 {
                let a = (i as f64) * 2.0 * PI / 8.0;
                let dir = Vec2::from_angle(a);
                let p0 = center + dir * (0.25 * r);
                let p1 = center + dir * (0.9 * r);
                out.push(vec![p0, p1]);
            }
        }
        PlantForm::EvergreenTree => {
            // 12-point star polyline alternating radii 0.95r and 0.6r (closed)
            let mut star = Vec::with_capacity(25);
            for i in 0..24 {
                let a = (i as f64) * 2.0 * PI / 24.0;
                let rad = if i % 2 == 0 { 0.95 * r } else { 0.6 * r };
                star.push(center + Vec2::from_angle(a) * rad);
            }
            star.push(star[0]);
            out.push(star);
        }
        PlantForm::Shrub => {
            // 10 small arcs (scallops) inside the rim, each a 9-point polyline
            for i in 0..10 {
                let a = (i as f64) * 2.0 * PI / 10.0;
                let mut arc = Vec::with_capacity(9);
                // Arc spans a small angle around the radial direction
                let start_a = a - 0.3;
                let end_a = a + 0.3;
                for j in 0..9 {
                    let t = (j as f64) / 8.0;
                    let ang = start_a + (end_a - start_a) * t;
                    // Radius varies slightly to make it look like a scallop
                    let rr = r * (0.85 + 0.1 * (t * PI).sin());
                    arc.push(center + Vec2::from_angle(ang) * rr);
                }
                out.push(arc);
            }
        }
        PlantForm::GroundCover => {
            // 7 small closed circles of radius 0.12r placed on a ring at 0.55r plus one at the centre
            let small_r = 0.12 * r;
            let ring_r = 0.55 * r;

            // One at centre
            let mut c0 = Vec::with_capacity(17);
            for i in 0..16 {
                let a = (i as f64) * 2.0 * PI / 16.0;
                c0.push(center + Vec2::new(a.cos() * small_r, a.sin() * small_r));
            }
            c0.push(c0[0]);
            out.push(c0);

            // 6 on a ring
            for i in 0..6 {
                let a = (i as f64) * 2.0 * PI / 6.0;
                let c = center + Vec2::from_angle(a) * ring_r;
                let mut cc = Vec::with_capacity(17);
                for j in 0..16 {
                    let b = (j as f64) * 2.0 * PI / 16.0;
                    cc.push(c + Vec2::new(b.cos() * small_r, b.sin() * small_r));
                }
                cc.push(cc[0]);
                out.push(cc);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{ArchModel, Plant, PlantForm, PlantSpecies};
    use cadcraft_geom::Vec2;

    #[test]
    fn catalogue_has_fourteen_species_of_every_form() {
        let cat = catalogue();
        assert_eq!(cat.len(), 14);
        let forms: Vec<_> = cat.iter().map(|s| s.form).collect();
        assert!(forms.contains(&PlantForm::DeciduousTree));
        assert!(forms.contains(&PlantForm::EvergreenTree));
        assert!(forms.contains(&PlantForm::Shrub));
        assert!(forms.contains(&PlantForm::GroundCover));
    }

    #[test]
    fn find_species_prefers_the_drawings_own() {
        let mut model = ArchModel::default();
        let custom = PlantSpecies {
            name: "Sugar Maple".into(),
            form: PlantForm::Shrub, // Different from catalogue
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 1.0,
            mature_spread: 1.0,
            planted_height: 0.5,
            years_to_mature: 1.0,
        };
        model.plant_species.push(custom.clone());

        let found = find_species(&model, "sugar maple").unwrap();
        assert_eq!(found.form, PlantForm::Shrub);

        let found2 = find_species(&model, "Red Oak").unwrap();
        assert_eq!(found2.form, PlantForm::DeciduousTree);
    }

    #[test]
    fn plant_size_grows_linearly_to_maturity() {
        let sp = PlantSpecies {
            name: "Test".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 5.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };

        // Age 0
        let p0 = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: 0.0, planted_height: None };
        let (h0, s0) = plant_size(&sp, &p0);
        assert!((h0 - 2.0).abs() < 1e-9);
        assert!((s0 - 1.0).abs() < 1e-9); // 5 * 2/10

        // Age 5 (halfway)
        let p5 = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: 5.0, planted_height: None };
        let (h5, s5) = plant_size(&sp, &p5);
        assert!((h5 - 6.0).abs() < 1e-9); // 2 + (10-2)*0.5
        assert!((s5 - 3.0).abs() < 1e-9); // 5 * 6/10

        // Age 10 (mature)
        let p10 = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: 10.0, planted_height: None };
        let (h10, s10) = plant_size(&sp, &p10);
        assert!((h10 - 10.0).abs() < 1e-9);
        assert!((s10 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn validate_species_refuses_bad_numbers() {
        let mut sp = PlantSpecies {
            name: "Test".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 5.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };
        assert!(validate_species(&sp).is_ok());

        sp.mature_height = -1.0;
        assert!(validate_species(&sp).is_err());

        sp.mature_height = 10.0;
        sp.planted_height = 11.0;
        assert!(validate_species(&sp).is_err());

        sp.planted_height = 2.0;
        sp.years_to_mature = 0.0;
        assert!(validate_species(&sp).is_err());

        sp.years_to_mature = 10.0;
        sp.mature_height = 200.0;
        assert!(validate_species(&sp).is_err());
    }

    #[test]
    fn plan_symbol_starts_with_the_spread_circle() {
        let sp = PlantSpecies {
            name: "Test".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 10.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };
        let center = Vec2::new(5.0, 5.0);
        let spread = 10.0;
        let symbol = plan_symbol(&sp, center, spread);

        assert!(!symbol.is_empty());
        let circle = &symbol[0];
        assert_eq!(circle.len(), 49); // 48 points + closure

        // Check first point is at distance 5.0 (radius)
        let d = (circle[0] - center).len();
        assert!((d - 5.0).abs() < 1e-9);
    }

    #[test]
    fn validate_species_edge_cases() {
        let valid = PlantSpecies {
            name: "Test".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 5.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };
        assert!(validate_species(&valid).is_ok());

        // Empty name
        let mut sp = valid.clone();
        sp.name = "".into();
        assert!(validate_species(&sp).is_err());

        // Name too long (> 64 chars)
        sp.name = "a".repeat(65);
        assert!(validate_species(&sp).is_err());

        // Name exactly 64 chars is ok
        sp.name = "a".repeat(64);
        assert!(validate_species(&sp).is_ok());

        // years_to_mature > 500
        sp.years_to_mature = 501.0;
        assert!(validate_species(&sp).is_err());

        // years_to_mature exactly 500 is ok
        sp.years_to_mature = 500.0;
        assert!(validate_species(&sp).is_ok());

        // mature_height non-finite (NaN)
        sp.mature_height = f64::NAN;
        assert!(validate_species(&sp).is_err());

        // mature_spread non-finite
        sp.mature_height = 10.0;
        sp.mature_spread = f64::NAN;
        assert!(validate_species(&sp).is_err());

        // planted_height non-finite
        sp.mature_spread = 5.0;
        sp.planted_height = f64::NAN;
        assert!(validate_species(&sp).is_err());

        // years_to_mature non-finite
        sp.planted_height = 2.0;
        sp.years_to_mature = f64::NAN;
        assert!(validate_species(&sp).is_err());

        // mature_spread > 100
        sp.years_to_mature = 10.0;
        sp.mature_spread = 101.0;
        assert!(validate_species(&sp).is_err());

        // mature_spread exactly 100 is ok
        sp.mature_spread = 100.0;
        assert!(validate_species(&sp).is_ok());

        // mature_height exactly 150 is ok
        sp.mature_height = 150.0;
        assert!(validate_species(&sp).is_ok());
    }

    #[test]
    fn plant_size_edge_cases() {
        let sp = PlantSpecies {
            name: "Test".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 5.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };

        // Non-finite age -> treated as 0
        let p = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: f64::NAN, planted_height: None };
        let (h, s) = plant_size(&sp, &p);
        assert!((h - 2.0).abs() < 1e-9);
        assert!((s - 1.0).abs() < 1e-9);

        // Age > years_to_mature -> clamped to mature size
        let p = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: 999.0, planted_height: None };
        let (h, s) = plant_size(&sp, &p);
        assert!((h - 10.0).abs() < 1e-9);
        assert!((s - 5.0).abs() < 1e-9);

        // Custom planted_height overrides species
        let p = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: 0.0, planted_height: Some(5.0) };
        let (h, s) = plant_size(&sp, &p);
        assert!((h - 5.0).abs() < 1e-9);
        assert!((s - 2.5).abs() < 1e-9); // 5 * 5/10

        // Negative age -> clamped to 0
        let p = Plant { id: cadcraft_doc::arch::ArchId(0), species: "Test".into(), position: Vec2::ZERO, age: -5.0, planted_height: None };
        let (h, s) = plant_size(&sp, &p);
        assert!((h - 2.0).abs() < 1e-9);
        assert!((s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn plan_symbol_all_forms() {
        let center = Vec2::new(5.0, 5.0);
        let spread = 10.0;

        // DeciduousTree: 1 circle + 8 radial lines = 9 polylines
        let sp_dec = PlantSpecies {
            name: "Dec".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 10.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };
        let sym = plan_symbol(&sp_dec, center, spread);
        assert_eq!(sym.len(), 9);
        for poly in &sym {
            for pt in poly {
                assert!(pt.is_finite());
            }
        }

        // EvergreenTree: 1 circle + 1 star = 2 polylines
        let sp_ever = PlantSpecies {
            name: "Ever".into(),
            form: PlantForm::EvergreenTree,
            crown: cadcraft_doc::arch::CrownShape::Cone,
            mature_height: 15.0,
            mature_spread: 5.0,
            planted_height: 1.5,
            years_to_mature: 30.0,
        };
        let sym = plan_symbol(&sp_ever, center, spread);
        assert_eq!(sym.len(), 2);
        // Star has 24 points + closure = 25
        assert_eq!(sym[1].len(), 25);
        for poly in &sym {
            for pt in poly {
                assert!(pt.is_finite());
            }
        }

        // Shrub: 1 circle + 10 scallops = 11 polylines
        let sp_shrub = PlantSpecies {
            name: "Shrub".into(),
            form: PlantForm::Shrub,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 3.0,
            mature_spread: 2.5,
            planted_height: 0.6,
            years_to_mature: 8.0,
        };
        let sym = plan_symbol(&sp_shrub, center, spread);
        assert_eq!(sym.len(), 11);
        for poly in &sym {
            for pt in poly {
                assert!(pt.is_finite());
            }
        }

        // GroundCover: 1 circle + 7 small circles = 8 polylines
        let sp_gc = PlantSpecies {
            name: "GC".into(),
            form: PlantForm::GroundCover,
            crown: cadcraft_doc::arch::CrownShape::Ellipsoid,
            mature_height: 0.3,
            mature_spread: 2.0,
            planted_height: 0.1,
            years_to_mature: 6.0,
        };
        let sym = plan_symbol(&sp_gc, center, spread);
        assert_eq!(sym.len(), 8);
        for poly in &sym {
            for pt in poly {
                assert!(pt.is_finite());
            }
        }
    }

    #[test]
    fn plan_symbol_degenerate() {
        let sp = PlantSpecies {
            name: "Test".into(),
            form: PlantForm::DeciduousTree,
            crown: cadcraft_doc::arch::CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 10.0,
            planted_height: 2.0,
            years_to_mature: 10.0,
        };
        let center = Vec2::new(5.0, 5.0);

        // spread = 0 -> empty
        let sym = plan_symbol(&sp, center, 0.0);
        assert!(sym.is_empty());

        // spread non-finite -> empty
        let sym = plan_symbol(&sp, center, f64::NAN);
        assert!(sym.is_empty());

        let sym = plan_symbol(&sp, center, f64::INFINITY);
        assert!(sym.is_empty());

        // Negative spread -> empty
        let sym = plan_symbol(&sp, center, -1.0);
        assert!(sym.is_empty());
    }

    #[test]
    fn find_species_unknown_returns_none() {
        let model = ArchModel::default();
        assert!(find_species(&model, "Nonexistent Species").is_none());
    }
}
