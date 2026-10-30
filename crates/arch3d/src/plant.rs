//! Plants in 3D (phase 9): a trunk and a crown per plant. Filled in by `p9-arch3d-plants`.

use crate::Mesh;
use cadcraft_doc::arch::ArchModel;

/// One crown mesh (`"plant-crown"`) and, for trees, one trunk mesh (`"plant-trunk"`) per plant,
/// each with the plant's id as `object`.
pub fn plant_meshes(model: &ArchModel) -> Vec<Mesh> {
    let _ = model;
    Vec::new()
}
