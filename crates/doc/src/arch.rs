//! Architectural data model: walls and their identifiers, stored on [`Drawing::arch`]
//! so that undo, autosave, AUDIT and DXF save all see them.
//!
//! This is the data layer (L1). 2D algorithms (joins, room detection, roof generation)
//! live in `crates/arch` (L2) and operate on this model.

use crate::Handle;
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

/// Base of the synthetic handle range reserved for architectural objects.
///
/// Arch objects are not entities in the store; render, grip and pick layers
/// route on [`is_arch_handle`] to distinguish them from real entity handles.
pub const ARCH_HANDLE_BASE: u64 = 0xA000_0000;

/// The synthetic proxy handle for an architectural object.
///
/// Arch objects have no entity handle, so selection and picking map back to the
/// [`ArchId`] through this reserved range.
pub fn arch_handle(id: ArchId) -> Handle {
    Handle(ARCH_HANDLE_BASE.saturating_add(u64::from(id.0)))
}

/// Whether `h` falls in the synthetic arch-handle range.
pub fn is_arch_handle(h: Handle) -> bool {
    h.0 >= ARCH_HANDLE_BASE && h.0 < ARCH_HANDLE_BASE.saturating_add(1u64 << 32)
}

/// Offset within the arch-handle range reserved for opening proxy handles.
///
/// Wall handles occupy `[ARCH_HANDLE_BASE, ARCH_HANDLE_BASE + OPENING_HANDLE_OFFSET)`.
/// Opening handles occupy `[ARCH_HANDLE_BASE + OPENING_HANDLE_OFFSET, ...)` and remain
/// recognisable by [`is_arch_handle`] (the full `1 << 32` range).
pub const OPENING_HANDLE_OFFSET: u64 = 0x8000;

/// The synthetic proxy handle for an architectural opening.
///
/// Openings are not entities; their proxy handle lives in the upper half of the
/// arch range so picking/selection can disambiguate from wall handles.
pub fn opening_handle(id: ArchId) -> Handle {
    Handle(ARCH_HANDLE_BASE.saturating_add(OPENING_HANDLE_OFFSET).saturating_add(u64::from(id.0)))
}

/// Whether `h` falls in the synthetic opening-handle range.
pub fn is_opening_handle(h: Handle) -> bool {
    h.0 >= ARCH_HANDLE_BASE.saturating_add(OPENING_HANDLE_OFFSET)
        && h.0 < ARCH_HANDLE_BASE.saturating_add(OPENING_HANDLE_OFFSET).saturating_add(1u64 << 15)
}

/// Human-readable label for an opening id, e.g. `o0`, `o1`.
pub fn opening_label(id: ArchId) -> String {
    format!("o{}", id.0)
}

/// Human-readable label for a wall id, e.g. `w0`, `w1`.
pub fn wall_label(id: ArchId) -> String {
    format!("w{}", id.0)
}

/// Maximum number of walls in a drawing (caps join-stretch BFS and allocations).
pub const MAX_WALLS: usize = 4096;

/// Minimum wall thickness, in metres.
pub const MIN_WALL_THICKNESS: f64 = 1e-4;

/// Minimum wall length, in metres.
pub const MIN_WALL_LENGTH: f64 = 1e-6;

/// Maximum number of openings in a drawing.
pub const MAX_OPENINGS: usize = 8192;

/// Minimum opening width, in metres.
pub const MIN_OPENING_WIDTH: f64 = 0.1;

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

/// Per-wall join override. `Auto`/`Mitre` mean the solver decides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JoinKind {
    /// No explicit preference; the join solver decides.
    Auto,
    /// Default preference for new walls; the join solver decides.
    #[default]
    Mitre,
    /// Force a butt joint.
    Butt,
    /// Force a square joint.
    Square,
}

/// The kind of architectural opening.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpeningKind {
    /// A hinged or sliding door.
    #[default]
    Door,
    /// A window.
    Window,
    /// A garage door.
    GarageDoor,
    /// A cased opening (no swing).
    CasedOpening,
}

/// Which side the hinge is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Hinge {
    /// Hinge on the left.
    #[default]
    Left,
    /// Hinge on the right.
    Right,
}

/// Which direction the opening swings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Swing {
    /// Swings inward.
    #[default]
    In,
    /// Swings outward.
    Out,
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

/// An architectural opening (door, window, garage door, or cased opening).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Opening {
    pub id: ArchId,
    pub host: ArchId,
    pub offset: f64,
    pub width: f64,
    pub height: f64,
    pub sill: f64,
    pub kind: OpeningKind,
    pub style: String,
    pub hinge: Hinge,
    pub swing: Swing,
    pub recess: f64,
    pub frame_width: f64,
    pub frame_depth: f64,
    pub casing_width: f64,
    pub casing_depth: f64,
    pub sill_depth: f64,
    pub rough_width: f64,
    pub rough_height: f64,
    pub swing_angle: f64,
    pub percent_open: f64,
    pub glass_mode: String,
    pub mull_group: Option<ArchId>,
    pub layer: String,
}

impl Default for Opening {
    fn default() -> Self {
        Self {
            id: ArchId(0),
            host: ArchId(0),
            offset: 0.0,
            width: 0.9,
            height: 2.1,
            sill: 0.0,
            kind: OpeningKind::Door,
            style: "hinged".into(),
            hinge: Hinge::Left,
            swing: Swing::In,
            recess: 0.0,
            frame_width: 0.06,
            frame_depth: 0.10,
            casing_width: 0.05,
            casing_depth: 0.015,
            sill_depth: 0.03,
            rough_width: 1.02,
            rough_height: 2.22,
            swing_angle: 0.0,
            percent_open: 0.0,
            glass_mode: "automatic".into(),
            mull_group: None,
            layer: "A-DOOR".into(),
        }
    }
}

impl Opening {
    /// Create a new opening on the given host wall.
    ///
    /// Rough dimensions are set to finished + 0.12 m. Style and layer are
    /// derived from `kind`. Non-finite numeric inputs are clamped to defaults.
    pub fn new(host: ArchId, offset: f64, width: f64, height: f64, kind: OpeningKind) -> Self {
        let offset = if offset.is_finite() { offset } else { 0.0 };
        let width = if width.is_finite() { width } else { 0.9 };
        let height = if height.is_finite() { height } else { 2.1 };
        let style = match kind {
            OpeningKind::Window => "slider".to_string(),
            OpeningKind::GarageDoor => "garage".to_string(),
            OpeningKind::CasedOpening => "cased".to_string(),
            OpeningKind::Door => "hinged".to_string(),
        };
        let layer = if kind == OpeningKind::Window { "A-GLAZ" } else { "A-DOOR" };
        Self {
            id: ArchId(0),
            host,
            offset,
            width,
            height,
            sill: 0.0,
            kind,
            style,
            hinge: Hinge::Left,
            swing: Swing::In,
            recess: 0.0,
            frame_width: 0.06,
            frame_depth: 0.10,
            casing_width: 0.05,
            casing_depth: 0.015,
            sill_depth: 0.03,
            rough_width: width + 0.12,
            rough_height: height + 0.12,
            swing_angle: 0.0,
            percent_open: 0.0,
            glass_mode: "automatic".into(),
            mull_group: None,
            layer: layer.to_string(),
        }
    }
}

/// The architectural object model for a drawing.
///
/// Stored on [`Drawing::arch`]; the 3D pane and incremental rebuilds key off
/// [`Drawing::arch_revision`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArchModel {
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub openings: Vec<Opening>,
    #[serde(default)]
    pub next_id: u32,
    #[serde(default)]
    pub next_opening_id: u32,
    #[serde(default)]
    pub levels: Vec<Level>,
    #[serde(default)]
    pub types: Vec<WallType>,
}

impl Default for ArchModel {
    fn default() -> Self {
        ArchModel {
            walls: Vec::new(),
            openings: Vec::new(),
            next_id: 0,
            next_opening_id: 0,
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

    /// Add an opening and return its new id.
    ///
    /// Openings use their own counter (`next_opening_id`) so the first opening
    /// placed is always `ArchId(0)` regardless of how many walls exist.
    pub fn push_opening(&mut self, mut o: Opening) -> ArchId {
        let id = ArchId(self.next_opening_id);
        self.next_opening_id = self.next_opening_id.saturating_add(1);
        o.id = id;
        self.openings.push(o);
        id
    }

    /// Look up an opening by id.
    pub fn opening(&self, id: ArchId) -> Option<&Opening> {
        self.openings.iter().find(|o| o.id == id)
    }

    /// The index of an opening by id, or `None` if not found.
    pub fn opening_index(&self, id: ArchId) -> Option<usize> {
        self.openings.iter().position(|o| o.id == id)
    }

    /// All openings on a given host wall, sorted by offset.
    pub fn openings_on(&self, host: ArchId) -> Vec<&Opening> {
        let mut v: Vec<&Opening> = self.openings.iter().filter(|o| o.host == host).collect();
        v.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Remove an opening by id. Returns true if it was found.
    pub fn remove_opening(&mut self, id: ArchId) -> bool {
        if let Some(pos) = self.openings.iter().position(|o| o.id == id) {
            self.openings.remove(pos);
            true
        } else {
            false
        }
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

    /// Validate the model, returning a list of human-readable error messages.
    ///
    /// Checks (in order):
    /// 1. Wall count does not exceed [`MAX_WALLS`].
    /// 2. Per-wall: finite endpoints, thickness above minimum, non-zero length,
    ///    known wall type, known level.
    /// 3. No duplicate wall ids.
    pub fn validate(&self) -> Vec<String> {
        let mut messages = Vec::new();

        if self.walls.len() > MAX_WALLS {
            messages.push(format!("model has {} walls, max {MAX_WALLS}", self.walls.len()));
        }

        for w in &self.walls {
            let id = format!("{}", w.id);

            if !w.start.is_finite() || !w.end.is_finite() {
                messages.push(format!("{}: non-finite endpoints", id));
                continue;
            }

            if !(w.thickness > MIN_WALL_THICKNESS) {
                messages.push(format!("{}: thickness {} below minimum {}", id, w.thickness, MIN_WALL_THICKNESS));
            }

            let length = w.start.dist(w.end);
            if !(length > MIN_WALL_LENGTH) {
                messages.push(format!("{}: zero or near-zero length", id));
            }

            if !w.wall_type.is_empty() && self.wall_type(&w.wall_type).is_none() {
                messages.push(format!("{}: unknown wall type `{}`", id, w.wall_type));
            }

            if self.level(w.base_level).is_none() {
                messages.push(format!("{}: unknown level {}", id, w.base_level));
            }
        }

        let mut seen = std::collections::HashSet::new();
        let mut dupes = Vec::new();
        for w in &self.walls {
            if seen.contains(&w.id) {
                if !dupes.contains(&w.id) {
                    dupes.push(w.id);
                }
            } else {
                seen.insert(w.id);
            }
        }
        for id in &dupes {
            messages.push(format!("duplicate wall id {}", id));
        }

        if self.openings.len() > MAX_OPENINGS {
            messages.push(format!("model has {} openings, max {MAX_OPENINGS}", self.openings.len()));
        }

        for o in &self.openings {
            let id = format!("{}", o.id);

            if !o.offset.is_finite()
                || !o.width.is_finite()
                || !o.height.is_finite()
                || !o.sill.is_finite()
                || !o.recess.is_finite()
                || !o.frame_width.is_finite()
                || !o.frame_depth.is_finite()
                || !o.casing_width.is_finite()
                || !o.casing_depth.is_finite()
                || !o.sill_depth.is_finite()
                || !o.rough_width.is_finite()
                || !o.rough_height.is_finite()
                || !o.swing_angle.is_finite()
                || !o.percent_open.is_finite()
            {
                messages.push(format!("{}: non-finite numeric value", id));
                continue;
            }

            if !(o.width >= MIN_OPENING_WIDTH) {
                messages.push(format!("{}: width {} below minimum {}", id, o.width, MIN_OPENING_WIDTH));
            }

            if o.sill < 0.0 {
                messages.push(format!("{}: negative sill {}", id, o.sill));
            }

            if let Some(host) = self.wall(o.host) {
                if o.sill + o.height > host.height {
                    messages.push(format!("{}: sill+height {} exceeds wall height {}", id, o.sill + o.height, host.height));
                }
            } else {
                messages.push(format!("{}: unknown host wall {}", id, o.host));
                continue;
            }

            if o.rough_width < o.width {
                messages.push(format!("{}: rough width {} below finished width {}", id, o.rough_width, o.width));
            }

            if o.rough_height < o.height {
                messages.push(format!("{}: rough height {} below finished height {}", id, o.rough_height, o.height));
            }
        }

        let mut seen_op = std::collections::HashSet::new();
        let mut dupe_ops = Vec::new();
        for o in &self.openings {
            if seen_op.contains(&o.id) {
                if !dupe_ops.contains(&o.id) {
                    dupe_ops.push(o.id);
                }
            } else {
                seen_op.insert(o.id);
            }
        }
        for id in &dupe_ops {
            messages.push(format!("duplicate opening id {}", id));
        }

        messages
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
    fn arch_model_add_returns_increasing_ids() {
        let mut m = ArchModel::default();
        let id1 = m.add_wall(Wall::default());
        let id2 = m.add_wall(Wall::default());
        assert_eq!(id1, ArchId(0));
        assert_eq!(id2, ArchId(1));
        assert!(m.wall(id1).is_some());
        assert!(m.wall(id2).is_some());
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

    #[test]
    fn validate_accepts_a_clean_model() {
        let m = ArchModel::default();
        assert!(m.validate().is_empty());

        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        assert!(m.validate().is_empty());
    }

    #[test]
    fn validate_rejects_non_positive_thickness_and_zero_length() {
        let mut m = ArchModel::default();

        // thickness 0
        m.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.0, ..Wall::default() });

        // thickness -1
        m.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: -1.0, ..Wall::default() });

        // start == end (zero length)
        m.push_wall(Wall { start: Vec2::new(5.0, 5.0), end: Vec2::new(5.0, 5.0), thickness: 0.2, ..Wall::default() });

        // NaN thickness
        m.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: f64::NAN, ..Wall::default() });

        let msgs = m.validate();
        assert_eq!(msgs.len(), 4);
        assert!(msgs[0].contains("below minimum"));
        assert!(msgs[1].contains("below minimum"));
        assert!(msgs[2].contains("zero or near-zero length"));
        assert!(msgs[3].contains("below minimum"));
    }

    #[test]
    fn validate_reports_unknown_wall_type_and_level() {
        let mut m = ArchModel::default();
        m.push_wall(Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            base_level: 7,
            exterior: true,
            wall_type: "Bogus".into(),
            ..Wall::default()
        });

        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("unknown wall type")));
        assert!(msgs.iter().any(|m| m.contains("unknown level")));
    }

    #[test]
    fn validate_reports_duplicate_ids() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(20.0, 0.0), Vec2::new(30.0, 0.0), 0.2));
        m.walls[1].id = m.walls[0].id;

        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("duplicate wall id")));
    }

    #[test]
    fn validate_flags_too_many_walls() {
        let mut m = ArchModel::default();
        for _ in 0..MAX_WALLS + 1 {
            m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), 0.2));
        }
        let msgs = m.validate();
        let count = msgs.iter().filter(|m| m.contains("max")).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn arch_handle_round_trips_and_is_recognised() {
        assert!(is_arch_handle(arch_handle(ArchId(0))));
        assert!(is_arch_handle(arch_handle(ArchId(7))));
        assert_ne!(arch_handle(ArchId(0)), arch_handle(ArchId(1)));
    }

    #[test]
    fn is_arch_handle_false_for_entity_handles() {
        assert!(!is_arch_handle(Handle(0)));
        assert!(!is_arch_handle(Handle(0x9FFF_FFFF)));
    }

    #[test]
    fn drawing_default_has_an_empty_arch_model() {
        assert!(Drawing::new_metric().arch.is_empty());
        assert!(Drawing::default().arch.walls.is_empty());
    }

    #[test]
    fn next_wall_id_never_reuses_an_id() {
        let mut m = ArchModel::default();
        let id1 = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), 0.2));
        let id2 = m.push_wall(Wall::new(Vec2::new(2.0, 0.0), Vec2::new(3.0, 0.0), 0.2));
        assert_ne!(id1, id2);
        assert_ne!(id1, m.next_wall_id());
        assert_ne!(id2, m.next_wall_id());
        assert_eq!(m.next_wall_id(), ArchId(2));
    }

    #[test]
    fn arch_id_from_str_is_case_insensitive() {
        assert_eq!("w12".parse::<ArchId>().unwrap(), ArchId(12));
        assert_eq!("W12".parse::<ArchId>().unwrap(), ArchId(12));
    }

    #[test]
    fn opening_ids_have_their_own_counter() {
        let mut m = ArchModel::default();
        let wid = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let oid = m.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        assert_eq!(wid, ArchId(0));
        assert_eq!(oid, ArchId(0));
        assert_eq!(m.next_wall_id(), ArchId(1));
        assert_eq!(m.next_opening_id, 1);
        assert!(m.opening(oid).is_some());
        assert!(m.opening(ArchId(999)).is_none());
    }

    #[test]
    fn opening_defaults_have_rough_larger_than_finished() {
        let o = Opening::new(ArchId(1), 0.5, 0.9, 2.1, OpeningKind::Door);
        assert!((o.rough_width - (o.width + 0.12)).abs() < 1e-12);
        assert!((o.rough_height - (o.height + 0.12)).abs() < 1e-12);
        assert!(o.rough_width > o.width);
        assert!(o.rough_height > o.height);

        let d = Opening::default();
        assert!(d.rough_width > d.width);
        assert!(d.rough_height > d.height);
    }

    #[test]
    fn opening_defaults_pick_the_a_door_or_a_glaz_layer() {
        let door = Opening::new(ArchId(1), 0.0, 0.9, 2.1, OpeningKind::Door);
        let window = Opening::new(ArchId(1), 0.0, 0.9, 2.1, OpeningKind::Window);
        let garage = Opening::new(ArchId(1), 0.0, 2.0, 2.5, OpeningKind::GarageDoor);
        let cased = Opening::new(ArchId(1), 0.0, 0.5, 2.0, OpeningKind::CasedOpening);
        assert_eq!(door.layer, "A-DOOR");
        assert_eq!(window.layer, "A-GLAZ");
        assert_eq!(garage.layer, "A-DOOR");
        assert_eq!(cased.layer, "A-DOOR");
    }

    #[test]
    fn validate_rejects_an_opening_on_a_missing_host() {
        let mut m = ArchModel::default();
        m.push_opening(Opening::new(ArchId(999), 0.0, 0.9, 2.1, OpeningKind::Door));
        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("unknown host wall")));
    }

    #[test]
    fn validate_rejects_a_sill_above_the_wall_height() {
        let mut m = ArchModel::default();
        let wid = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let mut o = Opening::new(wid, 1.0, 0.9, 1.5, OpeningKind::Window);
        o.sill = 2.0;
        m.push_opening(o);
        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("exceeds wall height")));
    }

    #[test]
    fn validate_rejects_too_many_openings() {
        let mut m = ArchModel::default();
        for i in 0..MAX_OPENINGS + 1 {
            m.push_opening(Opening::new(ArchId(i as u32), 0.0, 0.9, 2.1, OpeningKind::Door));
        }
        let msgs = m.validate();
        let count = msgs.iter().filter(|m| m.contains("max")).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn opening_json_roundtrips_with_rough_and_recess() {
        let mut o = Opening::new(ArchId(1), 0.5, 1.2, 1.8, OpeningKind::Window);
        o.recess = 0.08;
        o.rough_width = 1.32;
        o.rough_height = 1.92;
        o.id = ArchId(42);
        let json = serde_json::to_string(&o).unwrap();
        let back: Opening = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, ArchId(42));
        assert_eq!(back.host, ArchId(1));
        assert_eq!(back.recess, 0.08);
        assert_eq!(back.rough_width, 1.32);
        assert_eq!(back.rough_height, 1.92);
        assert_eq!(back.layer, "A-GLAZ");
    }

    #[test]
    fn opening_handles_are_distinct_from_wall_handles() {
        let wh = arch_handle(ArchId(0));
        let oh = opening_handle(ArchId(0));
        assert_ne!(wh, oh);
        assert!(is_arch_handle(wh));
        assert!(is_arch_handle(oh));
        assert!(is_opening_handle(oh));
        assert!(!is_opening_handle(wh));
        assert!(!is_arch_handle(Handle(0)));
    }

    #[test]
    fn opening_handle_hex_starts_with_a000() {
        let oh = opening_handle(ArchId(0));
        let hex = oh.hex().to_lowercase();
        assert!(hex.starts_with("a000"), "opening handle hex is `{hex}`, expected prefix `a000`");
        let oh7 = opening_handle(ArchId(7));
        let hex7 = oh7.hex().to_lowercase();
        assert!(hex7.starts_with("a000"), "opening handle hex is `{hex7}`, expected prefix `a000`");
    }

    #[test]
    fn validate_flags_duplicate_opening_ids() {
        let mut m = ArchModel::default();
        let wid = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        let oid = m.openings.last().unwrap().id.clone();
        m.push_opening(Opening::new(wid, 2.0, 0.9, 2.1, OpeningKind::Door));
        m.openings.last_mut().unwrap().id = oid;

        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("duplicate opening id")));
    }
}
