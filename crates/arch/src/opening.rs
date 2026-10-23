//! Placement limits and auto-resize for architectural openings.
//!
//! Pure 2D logic (no rendering). Encodes Chief's placement rules as data:
//! minimum widths per opening kind/style, minimum wall segments, separation
//! between openings, and the rough allowance that the 3D cut uses. All checks
//! are finite and panic-free, so hostile DXF/command input cannot crash the
//! crate.

use cadcraft_doc::{Opening, OpeningKind};
use cadcraft_geom::Vec2;
use super::{ArchId, ArchModel};

/// Minimum width of a hinged/pocket/sliding opening, in metres.
pub const MIN_HINGED_WIDTH: f64 = 0.3;

/// Minimum width of a garage opening, in metres.
pub const MIN_GARAGE_WIDTH: f64 = 1.8;

/// Minimum wall segment on either side of a standard opening, in metres.
pub const MIN_WINDOW_SEGMENT: f64 = 0.15;

/// Minimum wall segment on either side of a bay/box/bow opening, in metres.
pub const MIN_BAY_SEGMENT: f64 = 0.725;

/// Minimum clear separation between two unrelated openings on a host wall, in metres.
pub const MIN_SEPARATION: f64 = 0.05;

/// Allowance added to finished dimensions to obtain the rough (cut) dimensions.
pub const ROUGH_ALLOWANCE: f64 = 0.12;

/// The tolerance used when comparing spans for touching/overlap.
const EPS: f64 = 1e-9;

/// A placement validation failure.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementError(pub String);

impl std::fmt::Display for PlacementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The left edge of the rough (cut) span, i.e. the finished offset shifted
/// so the finished opening is centred within the rough opening.
pub fn rough_offset(o: &Opening) -> f64 {
    o.offset - (o.rough_width - o.width) / 2.0
}

/// Project a world point `p` onto the centre line of the host wall and return
/// the offset (metres) from the wall start, clamped to `[0, len]`.
///
/// Returns `None` when the host is missing or has a zero/non-finite length.
pub fn project_to_wall(model: &ArchModel, host: ArchId, p: Vec2) -> Option<f64> {
    let wall = model.wall(host)?;
    if !wall.start.is_finite() || !wall.end.is_finite() {
        return None;
    }
    let len = wall.start.dist(wall.end);
    if !len.is_finite() || len < 1e-12 {
        return None;
    }
    let dir = (wall.end - wall.start).normalized();
    let t = (p - wall.start).dot(dir);
    Some(t.clamp(0.0, len))
}

/// Minimum opening width in metres for a given kind and style.
pub fn min_width(kind: OpeningKind, style: &str) -> f64 {
    if kind == OpeningKind::GarageDoor {
        return MIN_GARAGE_WIDTH;
    }
    let s = style.to_ascii_lowercase();
    if s.contains("bifold") {
        0.45
    } else if s.contains("hinged") || s.contains("pocket") || s.contains("sliding") {
        MIN_HINGED_WIDTH
    } else {
        0.1
    }
}

/// Minimum wall segment required on each side of an opening for a given style.
pub fn min_segment(style: &str) -> f64 {
    let s = style.to_ascii_lowercase();
    if s.contains("bay") || s.contains("box") || s.contains("bow") {
        MIN_BAY_SEGMENT
    } else {
        MIN_WINDOW_SEGMENT
    }
}

/// Whether the two closed-ish spans `[a0..a1]` and `[b0..b1]` overlap.
/// Touching endpoints are *not* an overlap.
pub fn spans_overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0 < b1 && b0 < a1
}

/// Validate that an opening of the given size can be placed on `host` at
/// `offset`. On success returns the clamped (on-wall) offset.
pub fn validate_placement(
    model: &ArchModel,
    host: ArchId,
    offset: f64,
    width: f64,
    sill: f64,
    height: f64,
    kind: OpeningKind,
    style: &str,
    ignore: Option<ArchId>,
) -> Result<f64, PlacementError> {
    let wall = model
        .wall(host)
        .ok_or_else(|| PlacementError(format!("host wall {host} not found")))
        .and_then(|w| if w.start.is_finite() && w.end.is_finite() { Some(w) } else { None })
        .ok_or_else(|| PlacementError("host wall has non-finite endpoints".into()))?;
    let len = wall.start.dist(wall.end);
    if !len.is_finite() || len < 1e-12 {
        return Err(PlacementError("host wall has zero or non-finite length".into()));
    }

    if !offset.is_finite() || !width.is_finite() || !sill.is_finite() || !height.is_finite() {
        return Err(PlacementError("non-finite numeric input".into()));
    }
    if width <= 0.0 || height <= 0.0 || sill < 0.0 {
        return Err(PlacementError("width, height and sill must be positive".into()));
    }

    let min_w = min_width(kind, style);
    if width < min_w {
        return Err(PlacementError(format!(
            "width {width} below minimum {min_w} for {kind:?}"
        )));
    }

    if !wall.height.is_finite() || sill + height > wall.height {
        return Err(PlacementError(format!(
            "sill+height {} exceeds wall height {}",
            sill + height,
            wall.height
        )));
    }

    let offset = offset.clamp(0.0, len);
    let rough_width = width + ROUGH_ALLOWANCE;
    let rough_off = offset - (rough_width - width) / 2.0;
    let min_seg = min_segment(style);
    if rough_off < min_seg {
        return Err(PlacementError("rough span extends past the start of the wall".into()));
    }
    if rough_off + rough_width > len - min_seg {
        return Err(PlacementError("rough span extends past the end of the wall".into()));
    }

    let c0 = rough_off;
    let c1 = rough_off + rough_width;
    for other in model.openings_on(host) {
        if ignore == Some(other.id) {
            continue;
        }
        if !other.offset.is_finite() || !other.width.is_finite() || !other.rough_width.is_finite() {
            continue;
        }
        let b0 = other.offset - (other.rough_width - other.width) / 2.0;
        let b1 = b0 + other.rough_width;
        let gap = (b0 - c1).max(c0 - b1);
        let need = if other.mull_group.is_some() { 0.0 } else { MIN_SEPARATION };
        if gap < need - EPS {
            return Err(PlacementError(format!(
                "opening overlaps or is too close to opening {other:?}"
            )));
        }
    }

    Ok(offset)
}

/// Place an opening on its host wall. When `fit` is true and the opening does
/// not fit, the width is shrunk (bisection) to the largest value that still
/// fits and is at least `min_width`; the offset is kept clamped to the wall.
pub fn place_opening(model: &ArchModel, mut o: Opening, fit: bool) -> Result<Opening, PlacementError> {
    let min_w = min_width(o.kind, &o.style);

    match validate_placement(
        model,
        o.host,
        o.offset,
        o.width,
        o.sill,
        o.height,
        o.kind,
        &o.style,
        None,
    ) {
        Ok(offset) => {
            o.offset = offset;
            o.rough_width = o.width + ROUGH_ALLOWANCE;
            o.rough_height = o.height + ROUGH_ALLOWANCE;
            auto_panel(&mut o);
            clamp_recess(model, &mut o);
            Ok(o)
        }
        Err(e) if fit => {
            if o.width < min_w {
                return Err(e);
            }
            let mut lo = min_w;
            let mut hi = o.width;
            for _ in 0..64 {
                let mid = (lo + hi) / 2.0;
                if mid == lo || mid == hi {
                    break;
                }
                if validate_placement(
                    model,
                    o.host,
                    o.offset,
                    mid,
                    o.sill,
                    o.height,
                    o.kind,
                    &o.style,
                    None,
                )
                .is_ok()
                {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            o.width = lo;
            o.rough_width = lo + ROUGH_ALLOWANCE;
            o.rough_height = o.height + ROUGH_ALLOWANCE;
            match validate_placement(
                model,
                o.host,
                o.offset,
                o.width,
                o.sill,
                o.height,
                o.kind,
                &o.style,
                None,
            ) {
                Ok(offset) => {
                    o.offset = offset;
                    auto_panel(&mut o);
                    clamp_recess(model, &mut o);
                    Ok(o)
                }
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Whether an opening is wide enough to be a double (two-leaf) unit.
pub fn is_double(o: &Opening) -> bool {
    let s = o.style.to_ascii_lowercase();
    let hinged_or_pocket = s.contains("hinged") || s.contains("pocket");
    if hinged_or_pocket {
        o.width >= 1.2
    } else if s.contains("bifold") {
        o.width > 0.9
    } else {
        false
    }
}

/// Append " double" to the style when the opening qualifies as a double unit.
pub fn auto_panel(o: &mut Opening) {
    if is_double(o) && !o.style.contains("double") {
        o.style.push_str(" double");
    }
}

/// Clamp the opening recess into the host wall thickness.
pub fn clamp_recess(model: &ArchModel, o: &mut Opening) -> f64 {
    let max = match model.wall(o.host) {
        Some(w) if w.thickness.is_finite() => w.thickness,
        _ => 0.0,
    };
    o.recess = o.recess.clamp(0.0, max);
    o.recess
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::Vec2;

    fn wall(model: &mut ArchModel, start: Vec2, end: Vec2, thickness: f64) -> ArchId {
        model.push_wall(Wall {
            start,
            end,
            thickness,
            ..Wall::default()
        })
    }

    fn opening_on(model: &mut ArchModel, host: ArchId, offset: f64, width: f64) -> ArchId {
        let o = Opening::new(host, offset, width, 2.0, OpeningKind::Door);
        model.push_opening(o)
    }

    #[test]
    fn validate_rejects_an_opening_wider_than_the_wall() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(1.0, 0.0), 0.2);
        let res = validate_placement(
            &model,
            host,
            0.0,
            2.0,
            0.0,
            2.0,
            OpeningKind::Door,
            "hinged",
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn validate_enforces_the_minimum_garage_width() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(5.0, 0.0), 0.2);
        let res = validate_placement(
            &model,
            host,
            1.0,
            1.0,
            0.0,
            2.0,
            OpeningKind::GarageDoor,
            "garage",
            None,
        );
        assert!(res.is_err());
        let ok = validate_placement(
            &model,
            host,
            1.0,
            2.0,
            0.0,
            2.0,
            OpeningKind::GarageDoor,
            "garage",
            None,
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn validate_requires_a_minimum_wall_segment_for_windows() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(1.0, 0.0), 0.2);
        // Window at offset 0 leaves no segment at the start.
        let res = validate_placement(
            &model,
            host,
            0.0,
            0.6,
            0.0,
            1.5,
            OpeningKind::Window,
            "slider",
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn validate_rejects_overlapping_openings_on_the_same_host() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(5.0, 0.0), 0.2);
        opening_on(&mut model, host, 0.5, 1.0);
        let res = validate_placement(
            &model,
            host,
            1.0,
            1.0,
            0.0,
            2.0,
            OpeningKind::Door,
            "hinged",
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn validate_rejects_a_sill_above_the_header() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(5.0, 0.0), 0.2);
        let res = validate_placement(
            &model,
            host,
            1.0,
            0.6,
            2.0,
            2.0,
            OpeningKind::Window,
            "slider",
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn fit_true_shrinks_the_opening_to_the_available_span() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(1.5, 0.0), 0.2);
        let o = Opening::new(host, 0.5, 2.0, 2.0, OpeningKind::Door);
        let placed = place_opening(&mut model, o, true).unwrap();
        assert!(placed.width >= min_width(OpeningKind::Door, "hinged"));
        assert!(placed.width < 2.0);
        assert!(validate_placement(
            &model,
            host,
            placed.offset,
            placed.width,
            placed.sill,
            placed.height,
            OpeningKind::Door,
            &placed.style,
            None,
        )
        .is_ok());
    }

    #[test]
    fn project_to_wall_maps_a_point_to_an_offset() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        assert_eq!(project_to_wall(&model, host, Vec2::new(3.0, 5.0)), Some(3.0));
        assert_eq!(project_to_wall(&model, host, Vec2::new(-1.0, 0.0)), Some(0.0));
        assert_eq!(project_to_wall(&model, host, Vec2::new(12.0, 0.0)), Some(10.0));
    }

    #[test]
    fn auto_panel_makes_a_wide_hinged_door_double() {
        let mut o = Opening::new(ArchId(1), 0.0, 1.5, 2.0, OpeningKind::Door);
        auto_panel(&mut o);
        assert!(o.style.contains("double"));
        let narrow = Opening::new(ArchId(2), 0.0, 0.9, 2.0, OpeningKind::Door);
        auto_panel(&mut narrow);
        assert!(!narrow.style.contains("double"));
    }

    #[test]
    fn validate_survives_nan_and_infinite_input() {
        let mut model = ArchModel::default();
        let host = wall(&mut model, Vec2::ZERO, Vec2::new(5.0, 0.0), 0.2);
        let nan = validate_placement(
            &model,
            host,
            f64::NAN,
            0.9,
            0.0,
            2.0,
            OpeningKind::Door,
            "hinged",
            None,
        );
        assert!(nan.is_err());
        let inf = validate_placement(
            &model,
            host,
            f64::INFINITY,
            0.9,
            0.0,
            2.0,
            OpeningKind::Door,
            "hinged",
            None,
        );
        assert!(inf.is_err());
    }
}
```

crates/arch/src/lib.rs
```python
<<<<<<< SEARCH
pub mod graph;
pub mod join;
pub mod wall;
