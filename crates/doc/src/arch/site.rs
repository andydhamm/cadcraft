//! The site around the building (phase 9): where it is on Earth, the terrain it stands on, the
//! terrain's modifiers and the plants on it. Plain data on [`super::ArchModel`]; the algorithms
//! (TIN, modified surface, contours, volumes, plant sizes) live in `arch::terrain` and
//! `arch::plant`, the 3D in `arch3d`.

use super::ArchId;
use crate::Handle;
use cadcraft_geom::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Offset within the arch-handle range reserved for the terrain and its modifiers: the terrain
/// is `+0` (label `t0`), modifier `n` is `+1+n` (label `tm{n}`).
pub const TERRAIN_HANDLE_OFFSET: u64 = 0x38000;
/// Offset within the arch-handle range reserved for plants (label `pl{n}`).
pub const PLANT_HANDLE_OFFSET: u64 = 0x40000;

/// Most data points a terrain holds (points plus every elevation line's vertices).
pub const MAX_TERRAIN_POINTS: usize = 20_000;
/// Most elevation lines a terrain holds.
pub const MAX_ELEVATION_LINES: usize = 2_048;
/// Most vertices in a boundary, an elevation line or a modifier polygon.
pub const MAX_RING_POINTS: usize = 4_096;
/// Most modifiers on a terrain.
pub const MAX_TERRAIN_MODIFIERS: usize = 256;
/// Most plants in a drawing.
pub const MAX_PLANTS: usize = 4_096;
/// Most user-added plant species.
pub const MAX_PLANT_SPECIES: usize = 256;

/// Default minor and major contour intervals (m).
pub const DEFAULT_CONTOUR_MINOR: f64 = 0.25;
pub const DEFAULT_CONTOUR_MAJOR: f64 = 1.0;
/// Smallest contour interval a terrain accepts (m) and most contour levels drawn.
pub const MIN_CONTOUR_INTERVAL: f64 = 0.01;
pub const MAX_CONTOUR_LEVELS: usize = 1_000;
/// Default flat apron around the building pad (m), and the grade the ground takes back to the
/// surrounding surface beyond it (rise per run: 0.5 = 1:2).
pub const DEFAULT_PAD_MARGIN: f64 = 0.3;
pub const PAD_SLOPE: f64 = 0.5;
/// How far below the terrain's lowest point its solid's flat bottom sits (m).
pub const TERRAIN_SKIRT: f64 = 1.0;
/// A retaining wall's top above the high side's grade and its footing below the low side's (m).
pub const DEFAULT_RETAINING_FREEBOARD: f64 = 0.15;
pub const DEFAULT_RETAINING_FOOTING: f64 = 0.3;
/// Default draped thickness of roads, driveways and sidewalks (m).
pub const DEFAULT_PATH_THICKNESS: f64 = 0.1;

/// Where the drawing is on Earth (minimal georeferencing; nothing is ever transformed).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteLocation {
    /// Degrees, north positive (−90..=90).
    pub latitude: f64,
    /// Degrees, east positive (−180..=180).
    pub longitude: f64,
    /// Elevation of the model's `z = 0` above sea level (m).
    pub elevation: f64,
    /// Degrees from the drawing's +Y axis to true north, counter-clockwise positive: at 30 the
    /// north arrow points along (−sin 30°, cos 30°).
    pub true_north: f64,
    /// An optional model point with its real-world easting and northing.
    pub reference: Option<SiteReference>,
    /// Where the plan's north arrow is drawn; `None` = beside the terrain boundary (or the origin).
    pub north_arrow_at: Option<Vec2>,
}

/// A model point and its real-world coordinates, stored as given (never transformed).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteReference {
    pub point: Vec2,
    pub easting: f64,
    pub northing: f64,
    /// Name of the coordinate system the easting and northing are in, e.g. "EPSG:32611".
    pub crs: String,
}

/// The ground the building stands on: one per drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terrain {
    /// The site boundary: a closed counter-clockwise ring (first point not repeated).
    pub boundary: Vec<Vec2>,
    /// Elevation used where no data reaches (and for a terrain with no data at all).
    pub default_elevation: f64,
    /// Survey points `(x, y, z)`.
    pub points: Vec<Vec3>,
    /// Elevation lines (contours): polylines at one elevation, used as breaklines.
    pub lines: Vec<ElevationLine>,
    /// Edits to the surface, applied in order.
    pub modifiers: Vec<TerrainModifier>,
    pub next_modifier_id: u32,
    pub contour_minor: f64,
    pub contour_major: f64,
    /// Whether the house's footprint flattens a building pad.
    pub pad: bool,
    pub pad_margin: f64,
}

impl Default for Terrain {
    fn default() -> Self {
        Terrain {
            boundary: Vec::new(),
            default_elevation: 0.0,
            points: Vec::new(),
            lines: Vec::new(),
            modifiers: Vec::new(),
            next_modifier_id: 0,
            contour_minor: DEFAULT_CONTOUR_MINOR,
            contour_major: DEFAULT_CONTOUR_MAJOR,
            pad: true,
            pad_margin: DEFAULT_PAD_MARGIN,
        }
    }
}

/// A polyline at one elevation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ElevationLine {
    pub points: Vec<Vec2>,
    pub elevation: f64,
    /// Whether the last point joins back to the first.
    pub closed: bool,
}

/// One edit to the terrain surface (label `tm{id}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainModifier {
    pub id: u32,
    pub kind: ModifierKind,
}

/// What a modifier does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModifierKind {
    /// The surface inside `polygon` is flat at `elevation`; outside it the surface blends
    /// linearly back to the surrounding surface over `margin` metres (0 = a vertical step).
    Flat { polygon: Vec<Vec2>, elevation: f64, margin: f64 },
    /// A hill (positive `height`) or valley (negative) at `center`: the surface rises by
    /// `height·(1 − t²)²` with `t = distance / radius`, nothing beyond `radius`.
    Hill { center: Vec2, height: f64, radius: f64 },
    /// A road, driveway or sidewalk: a region draped on the surface, `thickness` thick, with its
    /// own material. It does not change the surface.
    Path { kind: PathKind, polygon: Vec<Vec2>, thickness: f64, material: String },
}

/// The kind of a draped region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    #[default]
    Road,
    Driveway,
    Sidewalk,
}

impl PathKind {
    /// The lower-case name used in JSON, GLB node names and default materials.
    pub fn name(self) -> &'static str {
        match self {
            PathKind::Road => "road",
            PathKind::Driveway => "driveway",
            PathKind::Sidewalk => "sidewalk",
        }
    }

    /// The material a new path of this kind gets when none is given.
    pub fn default_material(self) -> &'static str {
        match self {
            PathKind::Road => "asphalt",
            PathKind::Driveway | PathKind::Sidewalk => "concrete",
        }
    }
}

/// A plant placed in the plan (label `pl{n}`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plant {
    pub id: ArchId,
    /// Species name, matched case-insensitively against the built-in catalogue, then the
    /// drawing's own species.
    pub species: String,
    pub position: Vec2,
    /// Years since planting; the plant grows linearly from its planted height to maturity.
    pub age: f64,
    /// Height when planted; `None` = the species' planted height.
    pub planted_height: Option<f64>,
}

/// A species: the built-in catalogue lives in `arch::plant`; user species are stored on the
/// drawing in `ArchModel::plant_species`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlantSpecies {
    pub name: String,
    pub form: PlantForm,
    pub crown: CrownShape,
    /// Mature height and spread (crown diameter) in metres.
    pub mature_height: f64,
    pub mature_spread: f64,
    /// Height when planted (m) and years from planting to maturity.
    pub planted_height: f64,
    pub years_to_mature: f64,
}

impl Default for PlantSpecies {
    fn default() -> Self {
        PlantSpecies {
            name: String::new(),
            form: PlantForm::DeciduousTree,
            crown: CrownShape::Sphere,
            mature_height: 10.0,
            mature_spread: 8.0,
            planted_height: 2.0,
            years_to_mature: 20.0,
        }
    }
}

/// What kind of plant a species is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlantForm {
    #[default]
    DeciduousTree,
    EvergreenTree,
    Shrub,
    GroundCover,
}

/// The shape of a plant's crown in 3D.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrownShape {
    #[default]
    Sphere,
    Cone,
    Ellipsoid,
}

/// The proxy handle of the terrain (`modifier = None`) or of one of its modifiers.
pub fn terrain_handle(modifier: Option<u32>) -> Handle {
    let off = match modifier {
        None => 0,
        Some(id) => u64::from(id).saturating_add(1),
    };
    Handle(super::ARCH_HANDLE_BASE.saturating_add(TERRAIN_HANDLE_OFFSET).saturating_add(off))
}

/// Whether `h` falls in the terrain-handle range.
pub fn is_terrain_handle(h: Handle) -> bool {
    let lo = super::ARCH_HANDLE_BASE.saturating_add(TERRAIN_HANDLE_OFFSET);
    h.0 >= lo && h.0 < lo.saturating_add(1u64 << 15)
}

/// The proxy handle of plant `id`.
pub fn plant_handle(id: ArchId) -> Handle {
    Handle(super::ARCH_HANDLE_BASE.saturating_add(PLANT_HANDLE_OFFSET).saturating_add(u64::from(id.0)))
}

/// Whether `h` falls in the plant-handle range.
pub fn is_plant_handle(h: Handle) -> bool {
    let lo = super::ARCH_HANDLE_BASE.saturating_add(PLANT_HANDLE_OFFSET);
    h.0 >= lo && h.0 < lo.saturating_add(1u64 << 15)
}

/// `t0` for the terrain, `tm{n}` for a modifier.
pub fn terrain_label(modifier: Option<u32>) -> String {
    match modifier {
        None => "t0".into(),
        Some(id) => format!("tm{id}"),
    }
}

/// `pl{n}`.
pub fn plant_label(id: ArchId) -> String {
    format!("pl{}", id.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_handles_do_not_overlap_views_or_each_other() {
        let views_end = super::super::ARCH_HANDLE_BASE + super::super::VIEW_HANDLE_OFFSET + (1u64 << 15);
        assert!(terrain_handle(None).0 >= views_end);
        assert!(is_terrain_handle(terrain_handle(Some(5))));
        assert!(!is_plant_handle(terrain_handle(Some(5))));
        assert!(is_plant_handle(plant_handle(ArchId(7))));
        assert!(!is_terrain_handle(plant_handle(ArchId(7))));
        assert_eq!(terrain_label(None), "t0");
        assert_eq!(terrain_label(Some(3)), "tm3");
        assert_eq!(plant_label(ArchId(2)), "pl2");
    }

    #[test]
    fn modifiers_round_trip_through_json_with_a_type_tag() {
        let m = TerrainModifier { id: 1, kind: ModifierKind::Flat { polygon: vec![Vec2::new(0.0, 0.0)], elevation: 1.0, margin: 0.0 } };
        let s = serde_json::to_string(&m).unwrap_or_default();
        assert!(s.contains("\"type\":\"flat\""), "{s}");
        let back: Option<TerrainModifier> = serde_json::from_str(&s).ok();
        assert_eq!(back, Some(m));
        let p = ModifierKind::Path { kind: PathKind::Driveway, polygon: vec![], thickness: 0.1, material: "concrete".into() };
        let s = serde_json::to_string(&p).unwrap_or_default();
        assert!(s.contains("\"kind\":\"driveway\""), "{s}");
    }

    #[test]
    fn terrain_defaults_follow_the_brief() {
        let t = Terrain::default();
        assert_eq!((t.contour_minor, t.contour_major, t.pad, t.pad_margin), (0.25, 1.0, true, 0.3));
        assert_eq!(serde_json::from_str::<PlantForm>("\"evergreen-tree\"").ok(), Some(PlantForm::EvergreenTree));
    }
}
