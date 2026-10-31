//! Contours of the site mesh at minor and major intervals. Filled in by `p9-arch-terrain-contours`.

use cadcraft_doc::arch::MAX_CONTOUR_LEVELS;
use cadraft_geom::tin::ContourSegment;
use cadraft_geom::tin_ops::ContourLine;
use cadraft_geom::tin_ops::chain_contours;

use super::mesh::SiteMesh;

/// The contours of `mesh`.
pub fn site_contours(mesh: &SiteMesh, minor: f64, major: f64) -> SiteContours {
    let _ = (mesh, minor, major);
    SiteContours::default()
}

/// A major contour's label: its elevation with two decimals ("1.00", "-0.50").
pub fn label_text(elevation: f64) -> String {
    format!("{elevation:.2}")
}

/// Where a contour's label goes: the point halfway along the line's length.
pub fn label_point(line: &ContourLine) -> Option<Vec2> {
    let _ = line;
    None
}
