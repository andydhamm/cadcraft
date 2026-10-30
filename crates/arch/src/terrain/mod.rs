//! Terrain algorithms (phase 9): the original surface (a constrained TIN of the survey points,
//! elevation lines and boundary), the modified surface (flat regions, hills and valleys, the
//! building pad, basement cuts), the site mesh, contours, cut and fill, CSV import and retaining
//! walls. Pure and deterministic; the data is `cadcraft_doc::arch::{Terrain, SiteLocation}`.

pub mod contour;
pub mod csv;
pub mod mesh;
pub mod pad;
pub mod retaining;
pub mod surface;
pub mod volume;

use cadcraft_doc::arch::Terrain;
use cadcraft_geom::tin::Tin;

/// Why a terrain operation failed; the text is shown to the user.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainError(pub String);

impl std::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TerrainError {}

/// Check a terrain against every cap and invariant before it is stored. Filled in by
/// `p9-arch-terrain-tin`.
pub fn validate_terrain(t: &Terrain) -> Result<(), TerrainError> {
    let _ = t;
    Ok(())
}

/// The original surface: a constrained TIN of the data and the boundary, clipped to the
/// boundary. Filled in by `p9-arch-terrain-tin`.
pub fn original_tin(t: &Terrain) -> Result<Tin, TerrainError> {
    let _ = t;
    Err(TerrainError("terrain surfaces are not available yet".into()))
}
