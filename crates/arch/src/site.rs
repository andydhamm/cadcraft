//! Site location (phase 9): true north, the north arrow, and turning true-north directions into
//! drawing directions. Filled in by `p9-arch-site`.

use cadcraft_doc::arch::{ArchModel, SiteLocation};
use cadcraft_geom::Vec2;

/// The north arrow in the plan.
#[derive(Clone, Debug, PartialEq)]
pub struct NorthArrow {
    pub at: Vec2,
    /// Unit vector toward true north in drawing coordinates.
    pub direction: Vec2,
    /// Closed outline of the arrow head and shaft.
    pub outline: Vec<Vec2>,
}

/// Check a site location before it is stored.
pub fn validate_site(site: &SiteLocation) -> Result<(), String> {
    let _ = site;
    Ok(())
}

/// Unit vector toward true north in drawing coordinates.
pub fn north_direction(site: &SiteLocation) -> Vec2 {
    let _ = site;
    Vec2::new(0.0, 1.0)
}

/// Turn a vector given with +Y = true north (+X = east) into drawing coordinates.
pub fn true_to_drawing(site: Option<&SiteLocation>, v: [f64; 3]) -> [f64; 3] {
    let _ = site;
    v
}

/// The north arrow, when the drawing has a site location or a terrain.
pub fn north_arrow(model: &ArchModel) -> Option<NorthArrow> {
    let _ = model;
    None
}
