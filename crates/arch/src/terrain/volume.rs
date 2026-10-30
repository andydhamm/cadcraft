//! Cut and fill between the original and the modified surface. Filled in by
//! `p9-arch-terrain-volumes`.

use super::mesh::SiteMesh;
use super::surface::SiteSurface;

/// `(cut, fill)` in m³: where the modified surface is below the original, and above it.
pub fn cut_fill(mesh: &SiteMesh, surf: &SiteSurface) -> (f64, f64) {
    let _ = (mesh, surf);
    (0.0, 0.0)
}
