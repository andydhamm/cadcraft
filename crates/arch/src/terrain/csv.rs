//! Survey points from CSV text. Filled in by `p9-arch-terrain-csv`.

use cadcraft_geom::Vec3;

use super::TerrainError;

/// Parse `x,y,z` rows; `offset` (easting, northing) is subtracted from x and y when given.
pub fn parse_points_csv(text: &str, offset: Option<(f64, f64)>) -> Result<Vec<Vec3>, TerrainError> {
    let _ = (text, offset);
    Err(TerrainError("CSV import is not available yet".into()))
}
