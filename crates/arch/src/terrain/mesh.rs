//! The site mesh: the modified surface triangulated with every breakline as an edge and an
//! elevation per triangle corner, so a vertical step is two triangles' corners at one point with
//! different elevations. Filled in by `p9-arch-terrain-mesh`.

use cadcraft_geom::Vec2;

use super::TerrainError;
use super::surface::SiteSurface;

/// Triangles in plan with an elevation per corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteMesh {
    pub xy: Vec<Vec2>,
    /// Counter-clockwise corner indices into `xy`.
    pub triangles: Vec<[usize; 3]>,
    /// The modified elevation at each triangle's three corners, seen from inside that triangle.
    pub z: Vec<[f64; 3]>,
}

/// Build the site mesh of `surf`.
pub fn site_mesh(surf: &SiteSurface) -> Result<SiteMesh, TerrainError> {
    let _ = surf;
    Err(TerrainError("terrain meshes are not available yet".into()))
}
