//! The modified surface: the original TIN with the modifiers, the building pad and basement cuts
//! applied, as an exact elevation function. Filled in by `p9-arch-terrain-surface`.

use cadcraft_doc::arch::{ArchModel, TerrainModifier};
use cadcraft_geom::Vec2;
use cadcraft_geom::tin::Tin;

use super::TerrainError;
use super::pad::PadRegion;

/// Everything needed to answer elevations on the site, built once per model state.
#[derive(Clone, Debug, Default)]
pub struct SiteSurface {
    original: Tin,
    boundary: Vec<Vec2>,
    modifiers: Vec<TerrainModifier>,
    pads: Vec<PadRegion>,
    breaklines: Vec<[Vec2; 2]>,
}

impl SiteSurface {
    /// Build the surface of `model`'s terrain; an error when there is none or it is invalid.
    pub fn new(model: &ArchModel) -> Result<SiteSurface, TerrainError> {
        let _ = model;
        Err(TerrainError("terrain surfaces are not available yet".into()))
    }

    /// The original TIN (clipped to the boundary).
    pub fn original(&self) -> &Tin {
        &self.original
    }

    /// The boundary ring (counter-clockwise).
    pub fn boundary(&self) -> &[Vec2] {
        &self.boundary
    }

    /// The terrain's modifiers, in order.
    pub fn modifiers(&self) -> &[TerrainModifier] {
        &self.modifiers
    }

    /// The building pad and basement regions.
    pub fn pads(&self) -> &[PadRegion] {
        &self.pads
    }

    /// Extra breaklines the mesh must keep as edges: retaining walls' centre lines.
    pub fn breaklines(&self) -> &[[Vec2; 2]] {
        &self.breaklines
    }

    /// Elevation of the original surface at `p`, `None` outside the boundary.
    pub fn original_at(&self, p: Vec2) -> Option<f64> {
        let _ = p;
        None
    }

    /// Elevation of the modified surface at `p`, `None` outside the boundary.
    pub fn modified_at(&self, p: Vec2) -> Option<f64> {
        self.modified_toward(p, p)
    }

    /// The modified elevation at `p`, deciding which side of a region edge `p` is on by looking
    /// a hair from `p` toward `toward` (so a mesh corner on a step takes its own triangle's side).
    pub fn modified_toward(&self, p: Vec2, toward: Vec2) -> Option<f64> {
        let _ = (p, toward);
        None
    }
}
