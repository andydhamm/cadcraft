//! The building pad and basement cuts. Filled in by `p9-arch-terrain-pad`.

use cadcraft_doc::arch::ArchModel;
use cadcraft_geom::Vec2;

/// Whether a region is the pad under the first floor or a basement's cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKind {
    Pad,
    Basement,
}

/// A footprint the ground is flattened to.
#[derive(Clone, Debug, PartialEq)]
pub struct PadRegion {
    pub kind: PadKind,
    /// The footprint's outer ring (counter-clockwise).
    pub ring: Vec<Vec2>,
    /// The ground's elevation inside: the level's elevation minus its floor structure.
    pub elevation: f64,
    /// Flat apron beyond the ring (pad only; 0 for a basement).
    pub margin: f64,
}

/// The pad of the first floor and the cut of every basement level that has a footprint.
pub fn pad_regions(model: &ArchModel) -> Vec<PadRegion> {
    let _ = model;
    Vec::new()
}

/// Apply `regions` to elevation `z` at `p` (containment decided a hair toward `toward`).
pub fn apply_pads(regions: &[PadRegion], p: Vec2, toward: Vec2, z: f64) -> f64 {
    let _ = (regions, p, toward);
    z
}
