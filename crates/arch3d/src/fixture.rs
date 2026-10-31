//! Light-fixture housings in 3D (phase 10). Filled by `p10-arch3d-fixture-meshes`.

use crate::Mesh;
use cadcraft_doc::arch::ArchModel;

/// The housings of every fixture (on or off): closed prisms with the fixture's id as `object`
/// and `light-…` materials (`light-trim`, `light-shade`, `light-cord`, and `light-lens-lit` for a
/// fixture that is on or `light-lens` for one that is off).
pub fn fixture_meshes(model: &ArchModel) -> Vec<Mesh> {
    let _ = model;
    Vec::new()
}
