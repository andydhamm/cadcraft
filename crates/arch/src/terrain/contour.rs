//! Contours of the site mesh at minor and major intervals. Filled in by `p9-arch-terrain-contours`.

use cadcraft_geom::Vec2;
use cadcraft_geom::tin_ops::ContourLine;

use super::mesh::SiteMesh;

/// Chained contour lines: `minor` at every multiple of the minor interval (majors included),
/// `major` at every multiple of the major interval.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteContours {
    pub minor: Vec<ContourLine>,
    pub major: Vec<ContourLine>,
}

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
