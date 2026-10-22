//! Wall extrusion: turns a solved 2D `WallShape` into a 3D extruded mesh.

use crate::mesh::Mesh;

use cadcraft_arch::join::WallShape;

/// Extrude a wall shape into a 3D mesh prism.
///
/// The shape's plan polygon is extruded from `level_elevation` to
/// `level_elevation + height`.
///
/// Returns `None` for degenerate rings (fewer than 3 vertices, zero area,
/// non-finite coordinates, or zero/negative height).
pub fn extrude_wall(shape: &WallShape, height: f64, level_elevation: f64, material: &str) -> Option<Mesh> {
    if !height.is_finite() || height < 1e-9 {
        return None;
    }
    if level_elevation.is_nan() || !level_elevation.is_finite() {
        return None;
    }
    let mut mesh = Mesh::new(material, shape.id);
    mesh.push_prism(&shape.polygon, level_elevation, level_elevation + height);
    (mesh.triangle_count() > 0).then(|| mesh)
}
