//! The site in 3D (phase 9): the terrain solid, draped roads, driveways and sidewalks, and the
//! spans of retaining walls. Plants are in [`crate::plant`].
//!
//! Materials: the terrain solid is `"terrain"`; a path is `"path-{kind}-{material}"` (e.g.
//! `"path-driveway-concrete"`), its mesh `object` being the modifier id; plants are
//! `"plant-crown"` / `"plant-trunk"` with the plant id. `probe::part_of` maps them to the parts
//! `terrain`, `path` and `plant`, and `io::glb` names their nodes `terrain`, `{kind}-tm{id}` and
//! `plant-pl{id}` / `plant-pl{id}-trunk`.

use crate::Mesh;
use cadcraft_doc::arch::{ArchModel, Wall};

/// Everything the site adds to a scene.
#[derive(Clone, Debug, Default)]
pub struct SiteMeshes {
    pub meshes: Vec<Mesh>,
    /// 1 when the terrain solid was built.
    pub terrain: usize,
    pub paths: usize,
    pub plants: usize,
}

/// Whether a mesh material belongs to the site (never reused by `build_dirty`).
pub fn is_site_material(material: &str) -> bool {
    material == "terrain" || material.starts_with("path-") || material.starts_with("plant-")
}

/// The site's meshes: the terrain solid and its paths (`p9-arch3d-terrain-solid`) plus the
/// plants (`crate::plant::plant_meshes`).
pub fn site_meshes(model: &ArchModel) -> SiteMeshes {
    let plants = crate::plant::plant_meshes(model);
    let n = plants.len();
    SiteMeshes { meshes: plants, terrain: 0, paths: 0, plants: n }
}

/// `(base, top)` of a retaining wall from the ground on each side, or `None` (then the wall
/// keeps its level span).
pub fn retaining_span(model: &ArchModel, w: &Wall) -> Option<(f64, f64)> {
    cadcraft_arch::terrain::retaining::retaining_span(model, w)
}
