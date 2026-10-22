//! Architectural data model: walls and their identifiers, stored on [`Drawing::arch`]
//! so that undo, autosave, AUDIT and DXF save all see them.
//!
//! This is the data layer (L1). 2D algorithms (joins, room detection, roof generation)
//! live in `crates/arch` (L2) and operate on this model.

use cadcraft_geom::{Bounds2, Vec2};
use serde::{Deserialize, Serialize};

/// A stable identifier for an architectural object.
///
/// Assigned by [`ArchModel::add_wall`]; monotonically increasing within a drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArchId(pub u32);

impl ArchId {
    /// Build an [`ArchId`] from a raw number.
    pub fn wall(n: u32) -> Self {
        ArchId(n)
    }
}

impl std::fmt::Display for ArchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "w{}", self.0)
    }
}

impl std::str::FromStr for ArchId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let digits = if s.starts_with('w') || s.starts_with('W') { &s[1..] } else { s };
        let n = u32::from_str_radix(digits, 10).map_err(|_| format!("invalid arch id `{s}`"))?;
        Ok(ArchId(n))
    }
}

/// Maximum number of walls in a drawing (caps join-stretch BFS and allocations).
pub const MAX_WALLS: usize = 4096;

/// Minimum wall thickness, in metres.
pub const MIN_WALL_THICKNESS: f64 = 1e-4;

/// Minimum wall length, in metres.
pub const MIN_WALL_LENGTH: f64 = 1e-6;

/// A wall: centre-line segment, thickness, height, and layer bands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Wall {
    /// Centre-line start point.
    pub start: Vec2,
    /// Centre-line end point.
    pub end: Vec2,
    /// Stable identifier assigned by [`ArchModel::push_wall`].
    pub id: ArchId,
    /// The building level this wall sits on.
    pub base_level: u32,
    /// Whether the wall extends to the ceiling of its level.
    pub to_ceiling: bool,
    /// Whether this is an exterior wall.
    pub exterior: bool,
    /// Name of the wall type (e.g. "Exterior", "Interior").
    #[serde(default = "default_wall_type")]
    pub wall_type: String,
    /// Join kind at the start end of the wall.
    pub join_start: JoinKind,
    /// Join kind at the end end of the wall.
    pub join_end: JoinKind,
    /// Wall thickness (perpendicular to the centre line).
    pub thickness: f64,
    /// Wall height (not used in 2D poché, but kept for 3D).
    pub height: f64,
    /// Layer bands: each `(name, thickness)` splits the wall thickness into nested
    /// sub-bands. The sum of all band thicknesses must equal `thickness`.
    pub layers: Vec<(String, f64)>,
}

fn default_wall_type() -> String {
    "Exterior".into()
}

impl Wall {
    /// A wall with the given centre line and thickness, 2.7 m high and no layer bands.
    pub fn new(start: Vec2, end: Vec2, thickness: f64) -> Self {
        Wall {
            start,
            end,
            thickness,
            height: 2.7,
            layers: Vec::new(),
            id: ArchId(0),
            base_level: 0,
            to_ceiling: false,
            exterior: true,
            wall_type: "Exterior".into(),
            join_start: JoinKind::Mitre,
            join_end: JoinKind::Mitre,
        }
    }
}

impl Default for Wall {
    fn default() -> Self {
        Wall {
            start: Vec2::ZERO,
            end: Vec2::ZERO,
            thickness: 0.2,
            height: 2.7,
            layers: Vec::new(),
            id: ArchId(0),
            base_level: 0,
            to_ceiling: false,
            exterior: true,
            wall_type: "Exterior".into(),
            join_start: JoinKind::Mitre,
            join_end: JoinKind::Mitre,
        }
    }
}

/// Which side of the wall centre line the reference is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RefSide {
    Left,
    Right,
    #[default]
    Centre,
}

/// A single layer within a wall type's build-up.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WallLayer {
    pub name: String,
    pub thickness: f64,
    pub material: String,
}

/// A named wall type with its layer build-up and reference side.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WallType {
    pub name: String,
    pub layers: Vec<WallLayer>,
    pub reference: RefSide,
}

/// Per-wall join override. `Mitre` means the solver decides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JoinKind {
    #[default]
    Mitre,
    Butt,
    Square,
}

/// A building level (floor plate).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Level {
    pub index: u32,
    pub name: String,
    pub elevation: f64,
    pub floor_to_floor: f64,
    pub ceiling_height: f64,
    pub structure: f64,
}

/// The architectural object model for a drawing.
///
/// Stored on [`Drawing::arch`]; the 3D pane and incremental rebuilds key off
/// [`Drawing::arch_revision`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchModel {
    pub walls: Vec<Wall>,
    pub next_id: u32,
    pub levels: Vec<Level>,
    pub types: Vec<WallType>,
}

impl Default for ArchModel {
    fn default() -> Self {
        ArchModel {
            walls: Vec::new(),
            next_id: 0,
            levels: vec![Level { index: 0, name: "Level 1".into(), elevation: 0.0, floor_to_floor: 2.4, ceiling_height: 2.4, structure: 0.2 }],
            types: vec![
                WallType {
                    name: "Exterior".into(),
                    layers: vec![
                        WallLayer { name: "siding".into(), thickness: 0.02, material: "siding".into() },
                        WallLayer { name: "sheathing".into(), thickness: 0.015, material: "sheathing".into() },
                        WallLayer { name: "stud".into(), thickness: 0.09, material: "stud".into() },
                        WallLayer { name: "drywall".into(), thickness: 0.013, material: "drywall".into() },
                    ],
                    reference: RefSide::Left,
                },
                WallType {
                    name: "Interior".into(),
                    layers: vec![
                        WallLayer { name: "drywall".into(), thickness: 0.013, material: "drywall".into() },
                        WallLayer { name: "stud".into(), thickness: 0.074, material: "stud".into() },
                        WallLayer { name: "drywall".into(), thickness: 0.013, material: "drywall".into() },
                    ],
                    reference: RefSide::Centre,
                },
            ],
        }
    }
}

impl ArchModel {
    /// Add a wall and return its new id.
    pub fn push_wall(&mut self, mut w: Wall) -> ArchId {
        let id = ArchId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        w.id = id;
        self.walls.push(w);
        id
    }

    /// Add a wall and return its new id.
    pub fn add_wall(&mut self, w: Wall) -> ArchId {
        self.push_wall(w)
    }

    /// Look up a wall by id.
    pub fn wall(&self, id: ArchId) -> Option<&Wall> {
        self.walls.iter().find(|w| w.id == id)
    }

    /// The index of a wall by id, or `None` if not found.
    pub fn wall_index(&self, id: ArchId) -> Option<usize> {
        self.walls.iter().position(|w| w.id == id)
    }

    /// The id that will be assigned to the next wall.
    pub fn next_wall_id(&self) -> ArchId {
        ArchId(self.next_id)
    }

    /// The number of walls in the model.
    pub fn len(&self) -> usize {
        self.walls.len()
    }

    /// Iterate over all wall ids.
    pub fn wall_ids(&self) -> impl Iterator<Item = ArchId> + '_ {
        self.walls.iter().map(|w| w.id)
    }

    /// Whether the model has no walls.
    pub fn is_empty(&self) -> bool {
        self.walls.is_empty()
    }

    /// Look up a wall type by name (case-insensitive).
    pub fn wall_type(&self, name: &str) -> Option<&WallType> {
        self.types.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// Look up a level by index.
    pub fn level(&self, index: u32) -> Option<&Level> {
        self.levels.iter().find(|l| l.index == index)
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
            if !w.start.is_finite() || !w.end.is_finite() || w.thickness <= 0.0 || !w.thickness.is_finite() {
                continue;
            }
            let len = w.start.dist(w.end);
            if len < 1e-12 || !len.is_finite() {
                continue;
            }
            let half = w.thickness / 2.0;
            let dir = (w.end - w.start).normalized();
            let perp = dir.perp();
            pts.push(w.start - perp * half);
            pts.push(w.end - perp * half);
            pts.push(w.end + perp * half);
            pts.push(w.start + perp * half);
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
        Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 2.0, height: 3.0, ..Wall::default() }
    }

    #[test]
    fn push_wall_assigns_increasing_ids_and_stores_them() {
        let mut m = ArchModel::default();
        let id1 = m.push_wall(test_wall());
        let id2 = m.push_wall(test_wall());
        assert_eq!(id1, ArchId(0));
        assert_eq!(id2, ArchId(1));
        assert!(m.wall(id1).is_some());
        assert!(m.wall(id2).is_some());
        assert_eq!(m.wall(id1).unwrap().start, Vec2::new(0.0, 0.0));
        assert_eq!(m.wall(id2).unwrap().end, Vec2::new(10.0, 0.0));
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

    #[test]
    fn wall_new_has_default_height_and_no_layers() {
        let w = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), 0.15);
        assert_eq!(w.thickness, 0.15);
        assert_eq!(w.height, 2.7);
        assert!(w.layers.is_empty());
        assert_eq!(w.start, Vec2::new(0.0, 0.0));
        assert_eq!(w.end, Vec2::new(1.0, 0.0));
    }

    #[test]
    fn wall_default_is_a_usable_thin_wall() {
        let w = Wall::default();
        assert!(w.thickness > 0.0);
        assert!(w.height > 0.0);
    }

    #[test]
    fn wall_default_works_in_struct_update() {
        let w = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), ..Wall::default() };
        assert_eq!(w.thickness, Wall::default().thickness);
    }

    #[test]
    fn wall_index_finds_position_and_none_for_unknown() {
        let mut m = ArchModel::default();
        m.push_wall(test_wall());
        m.push_wall(test_wall());
        m.push_wall(test_wall());
        assert_eq!(m.wall_index(ArchId(0)), Some(0));
        assert_eq!(m.wall_index(ArchId(1)), Some(1));
        assert_eq!(m.wall_index(ArchId(2)), Some(2));
        assert_eq!(m.wall_index(ArchId(99)), None);
    }

    #[test]
    fn next_wall_id_tracks_next_id() {
        let mut m = ArchModel::default();
        assert_eq!(m.next_wall_id(), ArchId(0));
        m.push_wall(test_wall());
        assert_eq!(m.next_wall_id(), ArchId(1));
        m.push_wall(test_wall());
        assert_eq!(m.next_wall_id(), ArchId(2));
    }

    #[test]
    fn wall_defaults_are_a_plain_exterior_wall() {
        let w = Wall::default();
        assert_eq!(w.join_start, JoinKind::Mitre);
        assert_eq!(w.join_end, JoinKind::Mitre);
        assert_eq!(w.base_level, 0);
        assert!(w.exterior);
        assert_eq!(w.wall_type, "Exterior");
    }

    #[test]
    fn arch_model_bounds_uses_poché_envelope() {
        let mut m = ArchModel::default();
        // Bottom wall: (0,0) to (10,0)
        m.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, ..Wall::default() });
        // Right wall: (10,0) to (10,8)
        m.push_wall(Wall { start: Vec2::new(10.0, 0.0), end: Vec2::new(10.0, 8.0), thickness: 0.2, ..Wall::default() });
        // Top wall: (10,8) to (0,8)
        m.push_wall(Wall { start: Vec2::new(10.0, 8.0), end: Vec2::new(0.0, 8.0), thickness: 0.2, ..Wall::default() });
        // Left wall: (0,8) to (0,0)
        m.push_wall(Wall { start: Vec2::new(0.0, 8.0), end: Vec2::new(0.0, 0.0), thickness: 0.2, ..Wall::default() });
        // Degenerate zero-length wall (should be skipped)
        m.push_wall(Wall { start: Vec2::new(5.0, 4.0), end: Vec2::new(5.0, 4.0), thickness: 0.2, ..Wall::default() });

        let b = m.bounds();
        assert!((b.min.x - (-0.1)).abs() < 1e-9, "min.x: {} != -0.1", b.min.x);
        assert!((b.min.y - (-0.1)).abs() < 1e-9, "min.y: {} != -0.1", b.min.y);
        assert!((b.max.x - 10.1).abs() < 1e-9, "max.x: {} != 10.1", b.max.x);
        assert!((b.max.y - 8.1).abs() < 1e-9, "max.y: {} != 8.1", b.max.y);
    }

    #[test]
    fn arch_model_defaults_have_two_wall_types() {
        let m = ArchModel::default();
        assert_eq!(m.types.len(), 2);
        assert_eq!(m.types[0].name, "Exterior");
        assert_eq!(m.types[1].name, "Interior");
    }

    #[test]
    fn exterior_wall_type_layers_sum_to_thickness() {
        let m = ArchModel::default();
        let exterior = &m.types[0];
        assert_eq!(exterior.layers.len(), 4);
        let sum: f64 = exterior.layers.iter().map(|l| l.thickness).sum();
        assert!((sum - 0.138).abs() < 1e-9);
    }

    #[test]
    fn level_default_is_ground_floor() {
        let m = ArchModel::default();
        let level = &m.levels[0];
        assert_eq!(level.index, 0);
        assert_eq!(level.elevation, 0.0);
        assert_eq!(level.floor_to_floor, 2.4);
        assert_eq!(level.ceiling_height, 2.4);
        assert_eq!(level.structure, 0.2);
    }

    #[test]
    fn join_kind_defaults_to_mitre() {
        assert_eq!(JoinKind::default(), JoinKind::Mitre);
    }

    #[test]
    fn wall_type_lookup_is_case_insensitive() {
        let m = ArchModel::default();
        assert!(m.wall_type("exterior").is_some());
    }
}
