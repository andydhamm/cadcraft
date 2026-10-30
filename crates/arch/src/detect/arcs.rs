//! Arc and polyline primitives.

use super::Prim;
use cadcraft_geom::Vec2;

/// Generate primitives for a polyline.
pub fn polyline_prims(_points: &[Vec2], _closed: bool) -> Vec<Prim> {
    Vec::new()
}
