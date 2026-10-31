//! Plants in 3D (phase 9): a trunk and a crown per plant.

use crate::Mesh;
use cadcraft_arch::plant::{find_species, plant_size};
use cadcraft_doc::arch::{ArchId, ArchModel, CrownShape, PlantForm};
use cadcraft_geom::{Vec2, Vec3};

const MERIDIANS: usize = 16;
const LATITUDE_BANDS: usize = 8;
const CONE_SIDES: usize = 16;
const TRUNK_SIDES: usize = 8;

/// One crown mesh (`"plant-crown"`) and, for trees, one trunk mesh (`"plant-trunk"`) per plant,
/// each with the plant's id as `object`.
pub fn plant_meshes(model: &ArchModel) -> Vec<Mesh> {
    let mut meshes = Vec::new();

    for plant in &model.plants {
        let Some(species) = find_species(model, &plant.species) else {
            continue;
        };
        let (h, s) = plant_size(&species, plant);
        if !h.is_finite() || !s.is_finite() || h <= 0.0 || s <= 0.0 {
            continue;
        }

        let position = plant.position;
        if !position.is_finite() {
            continue;
        }
        let z0 = site_elevation(model, position).filter(|z| z.is_finite()).unwrap_or(0.0);
        let r = s / 2.0;
        let f = trunk_fraction(species.form);
        let x = position.x;
        let y = position.y;

        let (crown, trunk_top) = match species.crown {
            CrownShape::Sphere => {
                let vertical_r = r.min(h / 2.0);
                let centre = Vec3::new(x, y, z0 + h - vertical_r);
                (ellipsoid_crown(plant.id, centre, r, vertical_r, z0 + h), centre.z)
            }
            CrownShape::Ellipsoid => {
                let vertical_r = (h - h * f) / 2.0;
                let centre = Vec3::new(x, y, z0 + h - vertical_r);
                (ellipsoid_crown(plant.id, centre, r, vertical_r, z0 + h), centre.z)
            }
            CrownShape::Cone => {
                let base = z0 + h * f;
                let apex = z0 + h;
                (cone_crown(plant.id, x, y, base, apex, r), base + 0.1 * (h - h * f))
            }
        };
        meshes.push(crown);

        if f > 0.0 {
            let trunk_r = 0.05_f64.max(0.04 * s);
            meshes.push(trunk_mesh(plant.id, x, y, z0, trunk_top, trunk_r));
        }
    }
    meshes
}

fn site_elevation(model: &ArchModel, position: Vec2) -> Option<f64> {
    let terrain = model.terrain.as_ref()?;
    if !inside_ring(position, &terrain.boundary) {
        return None;
    }
    Some(terrain.default_elevation).filter(|z| z.is_finite())
}

fn inside_ring(point: Vec2, ring: &[Vec2]) -> bool {
    if !point.is_finite() {
        return false;
    }
    let Some(mut previous) = ring.last().copied() else {
        return false;
    };
    let mut inside = false;
    for &current in ring {
        if previous.is_finite() && current.is_finite() && (current.y > point.y) != (previous.y > point.y) {
            let crossing = current.x + (point.y - current.y) * (previous.x - current.x) / (previous.y - current.y);
            if crossing.is_finite() && point.x < crossing {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn trunk_fraction(form: PlantForm) -> f64 {
    match form {
        PlantForm::DeciduousTree => 0.4,
        PlantForm::EvergreenTree => 0.15,
        PlantForm::Shrub | PlantForm::GroundCover => 0.0,
    }
}

fn ellipsoid_crown(id: ArchId, centre: Vec3, r: f64, vertical_r: f64, top_z: f64) -> Mesh {
    let mut mesh = Mesh::new("plant-crown", id);
    vertex(&mut mesh, Vec3::new(centre.x, centre.y, centre.z - vertical_r), -Vec3::Z);

    let ring_count = LATITUDE_BANDS - 1;
    for ring in 1..=ring_count {
        let latitude = -std::f64::consts::FRAC_PI_2 + ring as f64 * std::f64::consts::PI / LATITUDE_BANDS as f64;
        let ring_radius = latitude.cos();
        for meridian in 0..MERIDIANS {
            let theta = meridian as f64 * std::f64::consts::TAU / MERIDIANS as f64;
            let nx = ring_radius * theta.cos();
            let ny = ring_radius * theta.sin();
            let nz = latitude.sin();
            vertex(
                &mut mesh,
                Vec3::new(centre.x + r * nx, centre.y + r * ny, centre.z + vertical_r * nz),
                Vec3::new(nx / r, ny / r, nz / vertical_r),
            );
        }
    }

    let top = 1 + ring_count * MERIDIANS;
    vertex(&mut mesh, Vec3::new(centre.x, centre.y, top_z), Vec3::Z);

    for meridian in 0..MERIDIANS {
        let next = (meridian + 1) % MERIDIANS;
        triangle(&mut mesh, 0, 1 + next, 1 + meridian);
    }
    for ring in 1..ring_count {
        let lower = 1 + (ring - 1) * MERIDIANS;
        let upper = lower + MERIDIANS;
        for meridian in 0..MERIDIANS {
            let next = (meridian + 1) % MERIDIANS;
            triangle(&mut mesh, lower + meridian, lower + next, upper + meridian);
            triangle(&mut mesh, lower + next, upper + next, upper + meridian);
        }
    }
    let last = 1 + (ring_count - 1) * MERIDIANS;
    for meridian in 0..MERIDIANS {
        let next = (meridian + 1) % MERIDIANS;
        triangle(&mut mesh, last + meridian, last + next, top);
    }
    mesh
}

fn cone_crown(id: ArchId, x: f64, y: f64, base: f64, apex: f64, r: f64) -> Mesh {
    let mut mesh = Mesh::new("plant-crown", id);
    let slope = r / (apex - base);
    for side in 0..CONE_SIDES {
        let theta = side as f64 * std::f64::consts::TAU / CONE_SIDES as f64;
        let nx = theta.cos();
        let ny = theta.sin();
        vertex(&mut mesh, Vec3::new(x + r * nx, y + r * ny, base), Vec3::new(nx, ny, slope));
    }
    let base_centre = CONE_SIDES;
    let apex_index = base_centre + 1;
    vertex(&mut mesh, Vec3::new(x, y, base), -Vec3::Z);
    vertex(&mut mesh, Vec3::new(x, y, apex), Vec3::Z);

    for side in 0..CONE_SIDES {
        let next = (side + 1) % CONE_SIDES;
        triangle(&mut mesh, side, next, apex_index);
        triangle(&mut mesh, base_centre, next, side);
    }
    mesh
}

fn trunk_mesh(id: ArchId, x: f64, y: f64, z0: f64, top: f64, r: f64) -> Mesh {
    let mut mesh = Mesh::new("plant-trunk", id);
    for z in [z0, top] {
        for side in 0..TRUNK_SIDES {
            let theta = side as f64 * std::f64::consts::TAU / TRUNK_SIDES as f64;
            let nx = theta.cos();
            let ny = theta.sin();
            vertex(&mut mesh, Vec3::new(x + r * nx, y + r * ny, z), Vec3::new(nx, ny, 0.0));
        }
    }
    let bottom_centre = TRUNK_SIDES * 2;
    let top_centre = bottom_centre + 1;
    vertex(&mut mesh, Vec3::new(x, y, z0), -Vec3::Z);
    vertex(&mut mesh, Vec3::new(x, y, top), Vec3::Z);

    for side in 0..TRUNK_SIDES {
        let next = (side + 1) % TRUNK_SIDES;
        let upper = side + TRUNK_SIDES;
        let upper_next = next + TRUNK_SIDES;
        triangle(&mut mesh, side, next, upper);
        triangle(&mut mesh, next, upper_next, upper);
        triangle(&mut mesh, bottom_centre, next, side);
        triangle(&mut mesh, top_centre, upper, upper_next);
    }
    mesh
}

fn vertex(mesh: &mut Mesh, position: Vec3, normal: Vec3) {
    let normal = normalise(normal);
    mesh.positions.push([position.x as f32, position.y as f32, position.z as f32]);
    mesh.normals.push([normal.x as f32, normal.y as f32, normal.z as f32]);
    mesh.uvs.push([0.0, 0.0]);
}

fn normalise(v: Vec3) -> Vec3 {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if length.is_finite() && length > 1e-12 { Vec3::new(v.x / length, v.y / length, v.z / length) } else { Vec3::Z }
}

fn triangle(mesh: &mut Mesh, a: usize, b: usize, c: usize) {
    if let (Ok(a), Ok(b), Ok(c)) = (u32::try_from(a), u32::try_from(b), u32::try_from(c)) {
        mesh.indices.extend([a, b, c]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{Plant, PlantSpecies, Terrain};
    use std::collections::HashMap;

    fn spruce_model() -> ArchModel {
        let plant = Plant { species: "Colorado Blue Spruce".into(), position: Vec2::new(10.0, 10.0), age: 30.0, ..Default::default() };
        ArchModel { plants: vec![plant], ..Default::default() }
    }

    fn bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
        let mut points = mesh.positions.iter().map(|point| {
            let [x, y, z] = *point;
            Vec3::new(f64::from(x), f64::from(y), f64::from(z))
        });
        let first = points.next()?;
        let mut min = first;
        let mut max = first;
        for point in points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            min.z = min.z.min(point.z);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            max.z = max.z.max(point.z);
        }
        Some((min, max))
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "expected {expected}, got {actual}");
    }

    #[test]
    fn plant_crown_bounds_match_height_and_spread() {
        let meshes = plant_meshes(&spruce_model());
        let crown = meshes.iter().find(|mesh| mesh.material == "plant-crown");
        assert!(crown.is_some(), "the spruce must have a crown");
        let Some(crown) = crown else { return };
        let bounds = bounds(crown);
        assert!(bounds.is_some(), "the crown must have vertices");
        let Some((min, max)) = bounds else { return };

        close(min.x, 7.5);
        close(max.x, 12.5);
        close(min.y, 7.5);
        close(max.y, 12.5);
        close(min.z, 2.25);
        close(max.z, 15.0);
    }

    #[test]
    fn plant_trunk_only_for_trees() {
        assert!(trunk_fraction(PlantForm::DeciduousTree) > 0.0);
        assert!(trunk_fraction(PlantForm::EvergreenTree) > 0.0);
        assert_eq!(trunk_fraction(PlantForm::Shrub), 0.0);
        assert_eq!(trunk_fraction(PlantForm::GroundCover), 0.0);

        let meshes = plant_meshes(&spruce_model());
        assert_eq!(meshes.iter().filter(|mesh| mesh.material == "plant-trunk").count(), 1);
    }

    #[test]
    fn plant_sits_on_the_terrain() {
        let mut model = spruce_model();
        model.terrain = Some(Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(20.0, 20.0), Vec2::new(0.0, 20.0), Vec2::new(0.0, 0.0)],
            default_elevation: 1.25,
            points: vec![Vec3::new(0.0, 0.0, 1.25), Vec3::new(20.0, 0.0, 1.25), Vec3::new(20.0, 20.0, 1.25), Vec3::new(0.0, 20.0, 1.25)],
            ..Default::default()
        });

        let meshes = plant_meshes(&model);
        let trunk = meshes.iter().find(|mesh| mesh.material == "plant-trunk");
        assert!(trunk.is_some(), "the spruce must have a trunk");
        let Some(trunk) = trunk else { return };
        let bounds = bounds(trunk);
        assert!(bounds.is_some(), "the trunk must have vertices");
        let Some((min, _)) = bounds else { return };
        close(min.z, 1.25);
    }

    #[test]
    fn plant_meshes_are_closed() {
        for mesh in plant_meshes(&spruce_model()) {
            assert_eq!(mesh.indices.len() % 3, 0);
            let mut edges = HashMap::new();
            for triangle in mesh.indices.chunks_exact(3) {
                let (Some(&a), Some(&b), Some(&c)) = (triangle.first(), triangle.get(1), triangle.get(2)) else {
                    continue;
                };
                add_edge(&mut edges, a, b);
                add_edge(&mut edges, b, c);
                add_edge(&mut edges, c, a);
            }
            assert!(!edges.is_empty());
            assert!(edges.values().all(|count| *count == 2), "every edge of {} must belong to two triangles", mesh.material);
        }
    }

    fn add_edge(edges: &mut HashMap<(u32, u32), usize>, a: u32, b: u32) {
        let key = if a < b { (a, b) } else { (b, a) };
        *edges.entry(key).or_insert(0) += 1;
    }

    #[test]
    fn shrub_produces_no_trunk() {
        let species = PlantSpecies {
            name: "Test Shrub".into(),
            form: PlantForm::Shrub,
            crown: CrownShape::Sphere,
            mature_height: 1.0,
            mature_spread: 1.0,
            planted_height: 0.3,
            years_to_mature: 5.0,
        };
        let plant = Plant { species: "Test Shrub".into(), position: Vec2::new(5.0, 5.0), age: 10.0, ..Default::default() };
        let model = ArchModel { plants: vec![plant], plant_species: vec![species], ..Default::default() };
        let meshes = plant_meshes(&model);
        assert!(meshes.iter().any(|m| m.material == "plant-crown"), "shrub must have a crown");
        assert!(!meshes.iter().any(|m| m.material == "plant-trunk"), "shrub must not have a trunk");
    }

    #[test]
    fn plant_outside_terrain_boundary_uses_zero_elevation() {
        let mut model = spruce_model();
        model.terrain = Some(Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0), Vec2::new(0.0, 5.0), Vec2::new(0.0, 0.0)],
            default_elevation: 2.0,
            points: vec![],
            ..Default::default()
        });
        // Spruce is at (10, 10), outside the 5x5 boundary
        let meshes = plant_meshes(&model);
        let trunk = meshes.iter().find(|m| m.material == "plant-trunk");
        assert!(trunk.is_some(), "spruce must have a trunk");
        let Some(trunk) = trunk else { return };
        let b = bounds(trunk);
        assert!(b.is_some(), "trunk must have vertices");
        let Some((min, _)) = b else { return };
        close(min.z, 0.0);
    }

    #[test]
    fn unknown_species_is_skipped() {
        let plant = Plant { species: "Nonexistent Species".into(), position: Vec2::new(5.0, 5.0), age: 5.0, ..Default::default() };
        let model = ArchModel { plants: vec![plant], ..Default::default() };
        let meshes = plant_meshes(&model);
        assert!(meshes.is_empty(), "unknown species must produce no meshes");
    }

    #[test]
    fn plant_with_nan_position_is_skipped() {
        let plant = Plant { species: "Colorado Blue Spruce".into(), position: Vec2::new(f64::NAN, 10.0), age: 30.0, ..Default::default() };
        let model = ArchModel { plants: vec![plant], ..Default::default() };
        let meshes = plant_meshes(&model);
        assert!(meshes.is_empty(), "plant with NaN position must be skipped");
    }

    #[test]
    fn plant_meshes_carry_the_plant_id() {
        let model = spruce_model();
        let plant_id = model.plants[0].id;
        let meshes = plant_meshes(&model);
        assert!(!meshes.is_empty(), "must produce meshes");
        for mesh in &meshes {
            assert_eq!(mesh.object, plant_id, "mesh must carry the plant's id");
        }
    }

    #[test]
    fn multiple_plants_produce_separate_meshes() {
        let p1 = Plant { species: "Colorado Blue Spruce".into(), position: Vec2::new(5.0, 5.0), age: 30.0, ..Default::default() };
        let p2 = Plant { species: "Colorado Blue Spruce".into(), position: Vec2::new(15.0, 15.0), age: 30.0, ..Default::default() };
        let model = ArchModel { plants: vec![p1, p2], ..Default::default() };
        let meshes = plant_meshes(&model);
        // Each spruce (tree) produces a crown and a trunk
        assert_eq!(meshes.len(), 4, "two spruces must produce four meshes");
        let crown_count = meshes.iter().filter(|m| m.material == "plant-crown").count();
        let trunk_count = meshes.iter().filter(|m| m.material == "plant-trunk").count();
        assert_eq!(crown_count, 2);
        assert_eq!(trunk_count, 2);
    }
}
