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

/// Whether `h` falls in the synthetic arch-handle range (walls, openings, rooms).
pub fn is_arch_handle(h: Handle) -> bool {
    h.0 >= ARCH_HANDLE_BASE && h.0 < ARCH_HANDLE_BASE.saturating_add(1u64 << 32)
}

/// The synthetic proxy handle for an architectural room.
pub fn room_handle(id: ArchId) -> Handle {
    Handle(ARCH_HANDLE_BASE.saturating_add(ROOM_HANDLE_OFFSET).saturating_add(u64::from(id.0)))
}

/// Whether `h` falls in the synthetic room-handle range.
pub fn is_room_handle(h: Handle) -> bool {
    h.0 >= ARCH_HANDLE_BASE.saturating_add(ROOM_HANDLE_OFFSET) && h.0 < ARCH_HANDLE_BASE.saturating_add(ROOM_HANDLE_OFFSET).saturating_add(1u64 << 15)
}

/// Human-readable label for a room id, e.g. `r0`, `r1`.
pub fn room_label(id: ArchId) -> String {
    format!("r{}", id.0)
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

/// Offset within the arch-handle range reserved for roof proxy handles.
/// Roof handles occupy `[ARCH_HANDLE_BASE + ROOF_HANDLE_OFFSET, ...)`.
pub const ROOF_HANDLE_OFFSET: u64 = 0x18000;

/// Maximum number of roofs in a drawing.
pub const MAX_ROOFS: usize = 64;

/// Maximum number of roof edges in a single roof.
pub const MAX_ROOF_EDGES: usize = 256;

/// Maximum number of roof planes in a single roof.
pub const MAX_ROOF_PLANES: usize = 256;

/// The synthetic proxy handle for an architectural roof.
pub fn roof_handle(id: ArchId) -> Handle {
    Handle(ARCH_HANDLE_BASE.saturating_add(ROOF_HANDLE_OFFSET).saturating_add(u64::from(id.0)))
}

/// Whether `h` falls in the synthetic roof-handle range.
pub fn is_roof_handle(h: Handle) -> bool {
    h.0 >= ARCH_HANDLE_BASE.saturating_add(ROOF_HANDLE_OFFSET) && h.0 < ARCH_HANDLE_BASE.saturating_add(ROOF_HANDLE_OFFSET).saturating_add(1u64 << 15)
}

/// Human-readable label for a roof id, e.g. `r0`, `r1`.
pub fn roof_label(id: ArchId) -> String {
    format!("r{}", id.0)
}

/// Maximum number of walls in a drawing (caps join-stretch BFS and allocations).
pub const MAX_WALLS: usize = 4096;

/// Minimum wall thickness, in metres.
pub const MIN_WALL_THICKNESS: f64 = 1e-4;

/// Minimum wall length, in metres.
pub const MIN_WALL_LENGTH: f64 = 1e-6;

/// Maximum number of openings in a drawing.
pub const MAX_OPENINGS: usize = 8192;

/// Maximum number of rooms in a drawing.
pub const MAX_ROOMS: usize = 1024;

/// Minimum room area (m²) to be considered valid.
pub const MIN_ROOM_AREA: f64 = 0.05;

/// Offset within the arch-handle range reserved for room proxy handles.
/// Room handles occupy `[ARCH_HANDLE_BASE + ROOM_HANDLE_OFFSET, ...)`.
pub const ROOM_HANDLE_OFFSET: u64 = 0x10000;

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
    /// Whether this wall is a room-divider (encloses rooms but produces no poché/3D).
    #[serde(default)]
    pub divider: bool,
    /// How the roof meets this wall (auto-detected, hip, gable, or valley).
    #[serde(default)]
    pub roof_directive: RoofDirective,
    /// Per-wall roof pitch override as rise (e.g. 6 for 6/12); `None` means use the roof's default.
    #[serde(default)]
    pub roof_pitch: Option<f64>,
    /// Per-wall overhang override; `None` means use the roof's overhang.
    #[serde(default)]
    pub roof_overhang: Option<f64>,
    /// Eave height above the level floor; `None` means derive from level.
    #[serde(default)]
    pub eave_height: Option<f64>,
}

fn default_wall_type() -> String {
    "Exterior".into()
}

impl Wall {
    /// A wall with the given centre line and thickness, 2.4 m high and no layer bands.
    pub fn new(start: Vec2, end: Vec2, thickness: f64) -> Self {
        Wall {
            start,
            end,
            thickness,
            height: 2.4,
            layers: Vec::new(),
            id: ArchId(0),
            base_level: 0,
            to_ceiling: false,
            exterior: true,
            wall_type: "Exterior".into(),
            join_start: JoinKind::Mitre,
            join_end: JoinKind::Mitre,
            divider: false,
            roof_directive: RoofDirective::default(),
            roof_pitch: None,
            roof_overhang: None,
            eave_height: None,
        }
    }
}

impl Default for Wall {
    fn default() -> Self {
        Wall {
            start: Vec2::ZERO,
            end: Vec2::ZERO,
            thickness: 0.2,
            height: 2.4,
            layers: Vec::new(),
            id: ArchId(0),
            base_level: 0,
            to_ceiling: false,
            exterior: true,
            wall_type: "Exterior".into(),
            join_start: JoinKind::Mitre,
            join_end: JoinKind::Mitre,
            divider: false,
            roof_directive: RoofDirective::default(),
            roof_pitch: None,
            roof_overhang: None,
            eave_height: None,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Level {
    pub index: u32,
    pub name: String,
    pub elevation: f64,
    pub ceiling_height: f64,
    pub structure: f64,
}

impl Default for Level {
    fn default() -> Self {
        Level { index: 0, name: String::new(), elevation: 0.0, ceiling_height: 2.4, structure: 0.3 }
    }
}

impl Level {
    /// Derived floor-to-floor height: ceiling + structure.
    /// Returns 0.0 when either component is non-finite or <= 0.
    pub fn floor_to_floor(&self) -> f64 {
        if self.ceiling_height.is_finite() && self.structure.is_finite() && self.ceiling_height > 0.0 && self.structure > 0.0 {
            self.ceiling_height + self.structure
        } else {
            0.0
        }
    }
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

/// The type of an architectural room.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomType {
    Living,
    Bedroom,
    Kitchen,
    Bath,
    Closet,
    Utility,
    Garage,
    Porch,
    Deck,
    OpenBelow,
    #[default]
    Other,
}

impl RoomType {
    /// Whether this room type counts as interior (excludes Garage/Porch/Deck/OpenBelow).
    pub fn is_interior(self) -> bool {
        !matches!(self, RoomType::Garage | RoomType::Porch | RoomType::Deck | RoomType::OpenBelow)
    }

    /// Whether this room type has a ceiling (Deck/Porch do not).
    pub fn has_ceiling(self) -> bool {
        !matches!(self, RoomType::Deck | RoomType::Porch)
    }

    /// Whether this room type has a floor (OpenBelow does not).
    pub fn has_floor(self) -> bool {
        !matches!(self, RoomType::OpenBelow)
    }
}

/// An architectural room: enclosed area defined by wall faces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Room {
    pub id: ArchId,
    pub name: String,
    pub room_type: RoomType,
    pub level: u32,
    pub polygon: Vec<Vec2>,
    pub holes: Vec<Vec<Vec2>>,
    pub ceiling_height: Option<f64>,
    pub floor_height: Option<f64>,
    pub interior_area: f64,
    pub standard_area: f64,
}

impl Default for Room {
    fn default() -> Self {
        Room {
            id: ArchId(0),
            name: String::new(),
            room_type: RoomType::default(),
            level: 0,
            polygon: Vec::new(),
            holes: Vec::new(),
            ceiling_height: None,
            floor_height: None,
            interior_area: 0.0,
            standard_area: 0.0,
        }
    }
}

/// How a roof meets a wall: auto-detected, hip, gable, or valley.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoofDirective {
    /// No explicit preference; the roof generator decides.
    #[default]
    Auto,
    /// Hip roof meets this wall.
    Hip,
    /// Gable roof meets this wall.
    Gable,
    /// Valley where two gable roofs meet.
    Valley,
}

/// Classification of a roof edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoofEdgeKind {
    /// The lower edge where the roof meets the walls.
    #[default]
    Eave,
    /// The upper ridge line where two planes meet.
    Ridge,
    /// A sloped corner edge (hip).
    Hip,
    /// A valley corner edge.
    Valley,
    /// A vertical gable end face edge.
    GableEnd,
}

/// A single roof plane: its footprint ring, slope, and dimensions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoofPlane {
    /// Planar ring (outer boundary) of this roof plane, CCW.
    pub ring: Vec<Vec2>,
    /// Slope (rise/run) of this plane.
    pub slope: f64,
    /// Projected area onto the plan, in square metres.
    pub projected_area: f64,
    /// Wall id this plane's eave rests on (if any).
    pub eave_wall: Option<ArchId>,
    /// 4-float encoding of the plane's base edge (x0, y0, x1, y1) in plan.
    pub base_edge: [f64; 4],
    /// Vertical thickness of the roof material at this plane.
    pub thickness: f64,
    /// Plan overhang: the distance from the ring's base edge (the overhang outline) to
    /// the architectural eave line, where `eave_z` applies.
    pub overhang: f64,
    /// Height of the roof underside at the base edge (eave height).
    pub eave_z: f64,
}

/// A classified edge of a roof (eave, ridge, hip, valley, gable end).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoofEdge {
    pub kind: RoofEdgeKind,
    /// Plan point A.
    pub a: Vec2,
    /// Plan point B.
    pub b: Vec2,
    /// Height (elevation) at A.
    pub za: f64,
    /// Height (elevation) at B.
    pub zb: f64,
}

/// An automatic or manual roof model, stored per level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Roof {
    /// Stable identifier assigned by [`ArchModel::push_roof`].
    pub id: ArchId,
    pub level: u32,
    /// Roof pitch rise (e.g. 6 for 6/12).
    pub pitch_rise: f64,
    /// Roof pitch run (e.g. 12 for 6/12).
    pub pitch_run: f64,
    /// Uniform overhang, in metres.
    pub overhang: f64,
    /// Roof material thickness, in metres.
    pub thickness: f64,
    /// Emit fascia boards around the eave.
    pub fascia: bool,
    /// Emit soffit panels under the fascia.
    pub soffit: bool,
    /// Created by a manual `roofplane` command rather than `ROOFAUTO`.
    pub manual: bool,
    /// User-edited planes; rebuilds stop touching this roof.
    pub hand_edited: bool,
    pub planes: Vec<RoofPlane>,
    pub edges: Vec<RoofEdge>,
}

impl Default for Roof {
    fn default() -> Self {
        Roof {
            id: ArchId(0),
            level: 0,
            pitch_rise: 6.0,
            pitch_run: 12.0,
            overhang: 0.4,
            thickness: 0.25,
            fascia: true,
            soffit: true,
            manual: false,
            hand_edited: false,
            planes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl Roof {
    /// Rise/run ratio when both are finite and run > 0, else 0.5.
    pub fn slope(&self) -> f64 {
        if self.pitch_rise.is_finite() && self.pitch_run.is_finite() && self.pitch_run > 0.0 { self.pitch_rise / self.pitch_run } else { 0.5 }
    }

    /// Sum of all finite plane projected areas.
    pub fn projected_area(&self) -> f64 {
        self.planes.iter().filter(|p| p.projected_area.is_finite()).map(|p| p.projected_area).sum()
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
    pub rooms: Vec<Room>,
    #[serde(default)]
    pub next_id: u32,
    #[serde(default)]
    pub next_opening_id: u32,
    #[serde(default)]
    pub next_room_id: u32,
    #[serde(default)]
    pub levels: Vec<Level>,
    #[serde(default)]
    pub types: Vec<WallType>,
    #[serde(default)]
    pub roofs: Vec<Roof>,
    #[serde(default)]
    pub next_roof_id: u32,
}

impl Default for ArchModel {
    fn default() -> Self {
        ArchModel {
            walls: Vec::new(),
            openings: Vec::new(),
            rooms: Vec::new(),
            next_id: 0,
            next_opening_id: 0,
            next_room_id: 0,
            roofs: Vec::new(),
            next_roof_id: 0,
            levels: vec![Level { index: 0, name: "Level 1".into(), elevation: 0.0, ceiling_height: 2.4, structure: 0.3 }],
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

    /// Add a room and return its new id.
    pub fn push_room(&mut self, mut r: Room) -> ArchId {
        let id = ArchId(self.next_room_id);
        self.next_room_id = self.next_room_id.saturating_add(1);
        r.id = id;
        self.rooms.push(r);
        id
    }

    /// Look up a room by id.
    pub fn room(&self, id: ArchId) -> Option<&Room> {
        self.rooms.iter().find(|r| r.id == id)
    }

    /// The index of a room by id, or `None` if not found.
    pub fn room_index(&self, id: ArchId) -> Option<usize> {
        self.rooms.iter().position(|r| r.id == id)
    }

    /// Remove a room by id. Returns true if it was found.
    pub fn remove_room(&mut self, id: ArchId) -> bool {
        if let Some(pos) = self.rooms.iter().position(|r| r.id == id) {
            self.rooms.remove(pos);
            true
        } else {
            false
        }
    }

    /// Iterate over all room ids.
    pub fn room_ids(&self) -> impl Iterator<Item = ArchId> + '_ {
        self.rooms.iter().map(|r| r.id)
    }

    /// Add a roof and return its new id.
    pub fn push_roof(&mut self, mut r: Roof) -> ArchId {
        let id = ArchId(self.next_roof_id);
        self.next_roof_id = self.next_roof_id.saturating_add(1);
        r.id = id;
        self.roofs.push(r);
        id
    }

    /// Look up a roof by id.
    pub fn roof(&self, id: ArchId) -> Option<&Roof> {
        self.roofs.iter().find(|r| r.id == id)
    }

    /// The index of a roof by id, or `None` if not found.
    pub fn roof_index(&self, id: ArchId) -> Option<usize> {
        self.roofs.iter().position(|r| r.id == id)
    }

    /// Iterate over all roof ids.
    pub fn roof_ids(&self) -> impl Iterator<Item = ArchId> + '_ {
        self.roofs.iter().map(|r| r.id)
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

        if self.rooms.len() > MAX_ROOMS {
            messages.push(format!("model has {} rooms, max {MAX_ROOMS}", self.rooms.len()));
        }

        for r in &self.rooms {
            let id = format!("{}", r.id);
            if r.polygon.len() < 3 {
                messages.push(format!("{}: outer polygon has {} vertices (<3)", id, r.polygon.len()));
            }
            for p in &r.polygon {
                if !p.is_finite() {
                    messages.push(format!("{}: non-finite polygon point", id));
                    break;
                }
            }
            for hole in &r.holes {
                if hole.len() < 3 {
                    messages.push(format!("{}: hole has {} vertices (<3)", id, hole.len()));
                }
                for p in hole {
                    if !p.is_finite() {
                        messages.push(format!("{}: non-finite hole point", id));
                        break;
                    }
                }
            }
            if !r.interior_area.is_finite() || r.interior_area < 0.0 {
                messages.push(format!("{}: invalid interior area {}", id, r.interior_area));
            }
            if !r.standard_area.is_finite() || r.standard_area < 0.0 {
                messages.push(format!("{}: invalid standard area {}", id, r.standard_area));
            }
            if self.level(r.level).is_none() {
                messages.push(format!("{}: unknown level {}", id, r.level));
            }
        }

        let mut seen_room = std::collections::HashSet::new();
        let mut dupe_rooms = Vec::new();
        for r in &self.rooms {
            if seen_room.contains(&r.id) {
                if !dupe_rooms.contains(&r.id) {
                    dupe_rooms.push(r.id);
                }
            } else {
                seen_room.insert(r.id);
            }
        }
        for id in &dupe_rooms {
            messages.push(format!("duplicate room id {}", id));
        }

        if self.roofs.len() > MAX_ROOFS {
            messages.push(format!("model has {} roofs, max {MAX_ROOFS}", self.roofs.len()));
        }

        for r in &self.roofs {
            let id = format!("{}", r.id);

            if !r.pitch_rise.is_finite() || !r.pitch_run.is_finite() || !r.overhang.is_finite() || !r.thickness.is_finite() {
                messages.push(format!("{}: non-finite numeric value", id));
                continue;
            }

            if r.pitch_run <= 0.0 {
                messages.push(format!("{}: pitch run {} must be positive", id, r.pitch_run));
            }

            if r.planes.len() > MAX_ROOF_PLANES {
                messages.push(format!("{}: {} planes, max {MAX_ROOF_PLANES}", id, r.planes.len()));
            }

            if r.edges.len() > MAX_ROOF_EDGES {
                messages.push(format!("{}: {} edges, max {MAX_ROOF_EDGES}", id, r.edges.len()));
            }

            for (i, p) in r.planes.iter().enumerate() {
                if !p.slope.is_finite() || !p.projected_area.is_finite() || !p.thickness.is_finite() || !p.overhang.is_finite() {
                    messages.push(format!("{}: plane {} has non-finite value", id, i));
                }
            }

            for (i, e) in r.edges.iter().enumerate() {
                if !e.a.is_finite() || !e.b.is_finite() || !e.za.is_finite() || !e.zb.is_finite() {
                    messages.push(format!("{}: edge {} has non-finite value", id, i));
                }
            }
        }

        let mut seen_roof = std::collections::HashSet::new();
        let mut dupe_roofs = Vec::new();
        for r in &self.roofs {
            if seen_roof.contains(&r.id) {
                if !dupe_roofs.contains(&r.id) {
                    dupe_roofs.push(r.id);
                }
            } else {
                seen_roof.insert(r.id);
            }
        }
        for id in &dupe_roofs {
            messages.push(format!("duplicate roof id {}", id));
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
        assert_eq!(w.height, 2.4);
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
        assert_eq!(level.ceiling_height, 2.4);
        assert_eq!(level.structure, 0.3);
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
    fn push_opening_shares_the_id_counter_with_walls() {
        let mut m = ArchModel::default();
        let w0 = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let w1 = m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        let o0 = m.push_opening(Opening::new(w0, 1.0, 0.9, 2.1, OpeningKind::Door));
        let o1 = m.push_opening(Opening::new(w1, 1.0, 0.9, 2.1, OpeningKind::Window));
        assert_eq!(w0, ArchId(0));
        assert_eq!(w1, ArchId(1));
        assert_eq!(o0, ArchId(0));
        assert_eq!(o1, ArchId(1));
        assert_eq!(m.next_wall_id(), ArchId(2));
        assert_eq!(m.next_opening_id, 2);
        assert!(m.opening(o0).is_some());
        assert!(m.opening(o1).is_some());
        assert!(m.opening(ArchId(2)).is_none());
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

    #[test]
    fn level_floor_to_floor_is_derived() {
        let l = Level { ceiling_height: 2.4, structure: 0.3, ..Level::default() };
        assert!((l.floor_to_floor() - 2.7).abs() < 1e-12);
        let l2 = Level { ceiling_height: 3.0, structure: 0.2, ..Level::default() };
        assert!((l2.floor_to_floor() - 3.2).abs() < 1e-12);
        let l3 = Level { ceiling_height: 0.0, structure: 0.3, ..Level::default() };
        assert_eq!(l3.floor_to_floor(), 0.0);
        let l4 = Level { ceiling_height: f64::NAN, structure: 0.3, ..Level::default() };
        assert_eq!(l4.floor_to_floor(), 0.0);
    }

    #[test]
    fn level_defaults_are_ceiling_2_4_structure_0_3() {
        let l = Level::default();
        assert!((l.ceiling_height - 2.4).abs() < 1e-12);
        assert!((l.structure - 0.3).abs() < 1e-12);
        let m = ArchModel::default();
        let level = &m.levels[0];
        assert!((level.ceiling_height - 2.4).abs() < 1e-12);
        assert!((level.structure - 0.3).abs() < 1e-12);
    }

    #[test]
    fn push_room_assigns_increasing_ids() {
        let mut m = ArchModel::default();
        let r1 = m.push_room(Room::default());
        let r2 = m.push_room(Room::default());
        assert_eq!(r1, ArchId(0));
        assert_eq!(r2, ArchId(1));
        assert!(m.room(r1).is_some());
        assert!(m.room(r2).is_some());
        assert_eq!(m.next_room_id, 2);
    }

    #[test]
    fn room_handle_is_in_the_room_range() {
        let rh = room_handle(ArchId(0));
        assert!(is_room_handle(rh));
        assert!(is_arch_handle(rh));
        assert!(!is_opening_handle(rh));
        let rh7 = room_handle(ArchId(7));
        assert!(is_room_handle(rh7));
        assert_ne!(rh, rh7);
        // Wall handle should not be a room handle.
        assert!(!is_room_handle(arch_handle(ArchId(0))));
    }

    #[test]
    fn room_type_interior_and_ceiling_rules() {
        assert!(RoomType::Living.is_interior());
        assert!(RoomType::Bedroom.is_interior());
        assert!(RoomType::Kitchen.is_interior());
        assert!(RoomType::Bath.is_interior());
        assert!(RoomType::Closet.is_interior());
        assert!(RoomType::Utility.is_interior());
        assert!(RoomType::Other.is_interior());
        assert!(!RoomType::Garage.is_interior());
        assert!(!RoomType::Porch.is_interior());
        assert!(!RoomType::Deck.is_interior());
        assert!(!RoomType::OpenBelow.is_interior());

        assert!(RoomType::Living.has_ceiling());
        assert!(RoomType::Bedroom.has_ceiling());
        assert!(!RoomType::Deck.has_ceiling());
        assert!(!RoomType::Porch.has_ceiling());

        assert!(RoomType::Living.has_floor());
        assert!(RoomType::Garage.has_floor());
        assert!(!RoomType::OpenBelow.has_floor());

        assert_eq!(RoomType::default(), RoomType::Other);
    }

    #[test]
    fn validate_rejects_degenerate_room() {
        let mut m = ArchModel::default();
        // Polygon with only 2 points
        m.push_room(Room { polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)], ..Room::default() });
        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("<3")));
    }

    #[test]
    fn divider_wall_defaults_to_false() {
        let w = Wall::default();
        assert!(!w.divider);
        let w2 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        assert!(!w2.divider);
    }

    #[test]
    fn roof_default_is_six_twelfths() {
        let r = Roof::default();
        assert_eq!(r.pitch_rise, 6.0);
        assert_eq!(r.pitch_run, 12.0);
        assert!((r.slope() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn push_roof_assigns_ids() {
        let mut m = ArchModel::default();
        let id1 = m.push_roof(Roof::default());
        let id2 = m.push_roof(Roof::default());
        assert_eq!(id1, ArchId(0));
        assert_eq!(id2, ArchId(1));
        assert!(m.roof(id1).is_some());
        assert!(m.roof(id2).is_some());
        assert_eq!(m.next_roof_id, 2);
    }

    #[test]
    fn roof_handle_is_disjoint_from_room_and_opening() {
        let rh = roof_handle(ArchId(0));
        let rhm = room_handle(ArchId(0));
        let oh = opening_handle(ArchId(0));
        assert!(is_roof_handle(rh));
        assert!(is_arch_handle(rh));
        assert_ne!(rh, rhm);
        assert_ne!(rh, oh);
        assert!(!is_room_handle(rh));
        assert!(!is_opening_handle(rh));
    }

    #[test]
    fn wall_roof_fields_default_to_auto() {
        let w = Wall::default();
        assert_eq!(w.roof_directive, RoofDirective::default());
        assert!(w.roof_pitch.is_none());
        assert!(w.roof_overhang.is_none());
        assert!(w.eave_height.is_none());

        let w2 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), 0.15);
        assert_eq!(w2.roof_directive, RoofDirective::default());
        assert!(w2.roof_pitch.is_none());
        assert!(w2.roof_overhang.is_none());
        assert!(w2.eave_height.is_none());
    }

    #[test]
    fn roof_projected_area_sums_planes() {
        let mut r = Roof::default();
        r.planes.push(RoofPlane {
            ring: Vec::new(),
            slope: 0.5,
            projected_area: 10.5,
            eave_wall: None,
            base_edge: [0.0, 0.0, 0.0, 0.0],
            thickness: 0.25,
            eave_z: 0.0,
            ..RoofPlane::default()
        });
        r.planes.push(RoofPlane {
            ring: Vec::new(),
            slope: 0.5,
            projected_area: 7.3,
            eave_wall: None,
            base_edge: [0.0, 0.0, 0.0, 0.0],
            thickness: 0.25,
            eave_z: 0.0,
            ..RoofPlane::default()
        });
        assert!((r.projected_area() - 17.8).abs() < 1e-9);
    }

    #[test]
    fn roof_slope_handles_degenerate_inputs() {
        let r = Roof { pitch_rise: f64::NAN, pitch_run: 12.0, ..Roof::default() };
        assert!((r.slope() - 0.5).abs() < 1e-12);

        let r2 = Roof { pitch_rise: 6.0, pitch_run: 0.0, ..Roof::default() };
        assert!((r2.slope() - 0.5).abs() < 1e-12);

        let r3 = Roof { pitch_rise: 6.0, pitch_run: -12.0, ..Roof::default() };
        assert!((r3.slope() - 0.5).abs() < 1e-12);

        let r4 = Roof { pitch_rise: f64::INFINITY, pitch_run: 12.0, ..Roof::default() };
        assert!((r4.slope() - 0.5).abs() < 1e-12);

        let r5 = Roof { pitch_rise: 8.0, pitch_run: 12.0, ..Roof::default() };
        assert!((r5.slope() - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn roof_projected_area_ignores_non_finite() {
        let mut r = Roof::default();
        r.planes.push(RoofPlane {
            ring: Vec::new(),
            slope: 0.5,
            projected_area: 10.0,
            eave_wall: None,
            base_edge: [0.0; 4],
            thickness: 0.25,
            eave_z: 0.0,
            ..RoofPlane::default()
        });
        r.planes.push(RoofPlane {
            ring: Vec::new(),
            slope: 0.5,
            projected_area: f64::NAN,
            eave_wall: None,
            base_edge: [0.0; 4],
            thickness: 0.25,
            eave_z: 0.0,
            ..RoofPlane::default()
        });
        r.planes.push(RoofPlane {
            ring: Vec::new(),
            slope: 0.5,
            projected_area: f64::INFINITY,
            eave_wall: None,
            base_edge: [0.0; 4],
            thickness: 0.25,
            eave_z: 0.0,
            ..RoofPlane::default()
        });
        assert!((r.projected_area() - 10.0).abs() < 1e-12);
    }

    #[test]
    fn roof_handle_range_boundaries() {
        let base = ARCH_HANDLE_BASE + ROOF_HANDLE_OFFSET;
        assert!(is_roof_handle(Handle(base)));
        assert!(is_roof_handle(Handle(base + 32767)));
        assert!(!is_roof_handle(Handle(base + 32768)));
        assert!(!is_roof_handle(Handle(base - 1)));
    }

    #[test]
    fn validate_rejects_too_many_roofs() {
        let mut m = ArchModel::default();
        for _ in 0..MAX_ROOFS + 1 {
            m.push_roof(Roof::default());
        }
        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("max") && m.contains("roofs")));
    }

    #[test]
    fn validate_flags_duplicate_roof_ids() {
        let mut m = ArchModel::default();
        m.push_roof(Roof::default());
        m.push_roof(Roof::default());
        m.roofs[1].id = m.roofs[0].id;
        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("duplicate roof id")));
    }

    #[test]
    fn validate_rejects_non_finite_roof_fields() {
        let mut m = ArchModel::default();
        m.push_roof(Roof { pitch_rise: f64::NAN, ..Roof::default() });
        let msgs = m.validate();
        assert!(msgs.iter().any(|m| m.contains("non-finite")));
    }

    #[test]
    fn roof_json_roundtrips() {
        let mut r = Roof::default();
        r.id = ArchId(5);
        r.planes.push(RoofPlane {
            ring: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)],
            slope: 0.5,
            projected_area: 80.0,
            eave_wall: Some(ArchId(2)),
            base_edge: [0.0, 0.0, 10.0, 0.0],
            thickness: 0.25,
            eave_z: 0.0,
            ..RoofPlane::default()
        });
        r.edges.push(RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(4.0, 4.0), b: Vec2::new(6.0, 4.0), za: 4.45, zb: 4.45 });
        let json = serde_json::to_string(&r).unwrap();
        let back: Roof = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, ArchId(5));
        assert_eq!(back.planes.len(), 1);
        assert_eq!(back.planes[0].projected_area, 80.0);
        assert_eq!(back.planes[0].eave_wall, Some(ArchId(2)));
        assert_eq!(back.edges.len(), 1);
        assert_eq!(back.edges[0].kind, RoofEdgeKind::Ridge);
        assert!((back.edges[0].za - 4.45).abs() < 1e-12);
    }
}
