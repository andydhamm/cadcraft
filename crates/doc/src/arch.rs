//! Architectural data model: walls and their identifiers, stored on [`Drawing::arch`]
//! so that undo, autosave, AUDIT and DXF save all see them.
//!
//! This is the data layer (L1). 2D algorithms (joins, room detection, roof generation)
//! live in `crates/arch` (L2) and operate on this model.

use cadcraft_geom::Bounds2;
use serde::{Deserialize, Serialize};

/// A stable identifier for an architectural object.
///
/// Assigned by [`ArchModel::add_wall`]; monotonically increasing within a drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArchId(pub u32);

/// A wall: centre-line segment, thickness, height, and layer bands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    /// Centre-line start point.
    pub start: Vec2,
    /// Centre-line end point.
    pub end: Vec2,
    /// Wall thickness (perpendicular to the centre line).
    pub thickness: f64,
    /// Wall height (not used in 2D poché, but kept for 3D).
    pub height: f64,
    /// Layer bands: each `(name, thickness)` splits the wall thickness into nested
    /// sub-bands. The sum of all band thicknesses must equal `thickness`.
    pub layers: Vec<(String, f64)>,
}

/// A wall entry: an [`ArchId`] paired with its [`Wall`] geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallEntry {
    pub id: ArchId,
    pub wall: Wall,
}

/// The architectural object model for a drawing.
///
/// Stored on [`Drawing::arch`]; the 3D pane and incremental rebuilds key off
/// [`Drawing::arch_revision`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ArchModel {
    pub walls: Vec<WallEntry>,
    pub next_id: u32,
}

impl ArchModel {
    /// Add a wall and return its new id.
    pub fn add_wall(&mut self, w: Wall) -> ArchId {
        let id = ArchId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.walls.push(WallEntry { id, wall: w });
        id
    }

    /// Look up a wall by id.
    pub fn wall(&self, id: ArchId) -> Option<&Wall> {
        self.walls.iter().find(|e| e.id == id).map(|e| &e.wall)
    }

    /// Iterate over all wall ids.
    pub fn wall_ids(&self) -> impl Iterator<Item = ArchId> + '_ {
        self.walls.iter().map(|e| e.id)
    }

    /// Whether the model has no walls.
    pub fn is_empty(&self) -> bool {
        self.walls.is_empty()
    }

    /// Union of every wall's poché ring bounds.
    ///
    /// Computed as `Bounds2::from_points` over each wall's four offset corners
    /// (start/end expanded by `thickness / 2.0` perpendicular to the centre line).
    /// Returns [`Bounds2::EMPTY`] when there are no walls.
    pub fn bounds(&self) -> Bounds2 {
        if self.walls.is_empty() {
            return Bounds2::EMPTY;
        }
        let mut pts = Vec::with_capacity(self.walls.len() * 4);
        for w in &self.walls {
            let wall = &w.wall;
            if !wall.start.is_finite() || !wall.end.is_finite() || wall.thickness <= 0.0 || !wall.thickness.is_finite() {
                continue;
            }
            let len = wall.start.dist(wall.end);
            if len < 1e-12 || !len.is_finite() {
                continue;
            }
            let half = wall.thickness / 2.0;
            let dir = (wall.end - wall.start).normalized();
            let perp = dir.perp();
            pts.push(wall.start - perp * half);
            pts.push(wall.end - perp * half);
            pts.push(wall.end + perp * half);
            pts.push(wall.start + perp * half);
        }
        if pts.is_empty() { Bounds2::EMPTY } else { Bounds2::from_points(pts.iter().copied()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Drawing;
    use cadcraft_geom::Vec2;

    fn test_wall() -> Wall {
        Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 2.0, height: 3.0, layers: Vec::new() }
    }

    #[test]
    fn arch_model_add_returns_increasing_ids() {
        let mut m = ArchModel::default();
        let id1 = m.add_wall(test_wall());
        let id2 = m.add_wall(test_wall());
        let id3 = m.add_wall(test_wall());
        assert_eq!(id1, ArchId(0));
        assert_eq!(id2, ArchId(1));
        assert_eq!(id3, ArchId(2));
    }

    #[test]
    fn arch_model_lookup_by_id() {
        let mut m = ArchModel::default();
        let id = m.add_wall(test_wall());
        assert!(m.wall(id).is_some());
        assert_eq!(m.wall(id).unwrap().thickness, 2.0);
        assert!(m.wall(ArchId(999)).is_none());
    }

    #[test]
    fn empty_arch_model_is_empty() {
        let m = ArchModel::default();
        assert!(m.is_empty());
        assert_eq!(m.bounds(), Bounds2::EMPTY);
    }

    #[test]
    fn touch_arch_bumps_revision() {
        let mut d = Drawing::new_metric();
        assert_eq!(d.arch_revision, 0);
        d.touch_arch();
        d.touch_arch();
        assert_eq!(d.arch_revision, 2);
    }
}
