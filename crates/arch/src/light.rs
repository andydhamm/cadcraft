//! Light fixtures in plan (phase 10): the built-in catalogue, photometry, where a fixture's light
//! point is, an even grid in a room and the plan symbol. The data is
//! `cadcraft_doc::arch::LightFixture`; the 3D housing and the scene lights are in
//! `arch3d::light`.
//!
//! The catalogue and the photometry are complete; `validate_fixture`, `kelvin_to_rgb`,
//! `light_point`, `aim_direction` and `grid_positions` are stubs filled by `p10-arch-light-placement`
//! (their signatures are final); `plan_symbol` lives in `crate::light_symbol`.

use cadcraft_doc::arch::{ArchModel, FixtureShape, LightFixture, LightKind, LightMount};
use cadcraft_geom::{Vec2, Vec3};

/// A catalogue entry: what `LIGHTFIXTURE {"type":…}` copies onto a new fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct FixtureType {
    /// Display name, matched case-insensitively (`Recessed pot light`).
    pub name: &'static str,
    pub shape: FixtureShape,
    pub kind: LightKind,
    pub mount: LightMount,
    pub lumens: f64,
    pub kelvin: f64,
    /// Full apex angles in degrees (spots; a point light keeps 60/60, unused).
    pub cone: f64,
    pub falloff: f64,
    /// Housing diameter (m): the trim ring, the shade, the dome or the sconce's width.
    pub diameter: f64,
}

/// The built-in catalogue, in this order: the recessed pot light (the default), the pendant, the
/// wall sconce, the surface ceiling fixture and the floor lamp.
pub fn catalogue() -> Vec<FixtureType> {
    vec![
        FixtureType {
            name: "Recessed pot light",
            shape: FixtureShape::RecessedCan,
            kind: LightKind::Spot,
            mount: LightMount::Ceiling,
            lumens: 800.0,
            kelvin: 3000.0,
            cone: 60.0,
            falloff: 60.0,
            diameter: 0.15,
        },
        FixtureType {
            name: "Pendant",
            shape: FixtureShape::Pendant,
            kind: LightKind::Point,
            mount: LightMount::Ceiling,
            lumens: 1100.0,
            kelvin: 2700.0,
            cone: 60.0,
            falloff: 60.0,
            diameter: 0.3,
        },
        FixtureType {
            name: "Wall sconce",
            shape: FixtureShape::Sconce,
            kind: LightKind::Point,
            mount: LightMount::Wall,
            lumens: 450.0,
            kelvin: 2700.0,
            cone: 60.0,
            falloff: 60.0,
            diameter: 0.2,
        },
        FixtureType {
            name: "Surface ceiling fixture",
            shape: FixtureShape::SurfaceDome,
            kind: LightKind::Point,
            mount: LightMount::Ceiling,
            lumens: 1500.0,
            kelvin: 3000.0,
            cone: 60.0,
            falloff: 60.0,
            diameter: 0.35,
        },
        FixtureType {
            name: "Floor lamp",
            shape: FixtureShape::FloorLamp,
            kind: LightKind::Point,
            mount: LightMount::Floor,
            lumens: 1200.0,
            kelvin: 2700.0,
            cone: 60.0,
            falloff: 60.0,
            diameter: 0.4,
        },
    ]
}

/// The catalogue entry called `name` (case-insensitive, surrounding spaces ignored).
pub fn find_type(name: &str) -> Option<FixtureType> {
    let n = name.trim();
    catalogue().into_iter().find(|t| t.name.eq_ignore_ascii_case(n))
}

/// A new fixture from a catalogue entry at `position` (every light value copied from `t`).
pub fn fixture_from_type(id: cadcraft_doc::arch::ArchId, t: &FixtureType, position: Vec2) -> LightFixture {
    LightFixture {
        shape: t.shape,
        kind: t.kind,
        mount: t.mount,
        lumens: t.lumens,
        kelvin: t.kelvin,
        cone: t.cone,
        falloff: t.falloff,
        ..LightFixture::new(id, t.name, position)
    }
}

/// Housing diameter of a fixture (m): its catalogue entry's, or 0.15 when the name is unknown.
pub fn housing_diameter(f: &LightFixture) -> f64 {
    find_type(&f.fixture).map(|t| t.diameter).unwrap_or(0.15)
}

/// Solid angle (sr) of a cone with full apex angle `apex_deg`: 2π(1 − cos(apex/2)).
/// 60° gives 0.841787…; 360° gives 4π.
pub fn cone_solid_angle(apex_deg: f64) -> f64 {
    let half = (apex_deg.clamp(0.0, 360.0) * 0.5).to_radians();
    2.0 * std::f64::consts::PI * (1.0 - half.cos())
}

/// Luminous intensity (candela) of a fixture: a point light spreads its lumens over 4π sr, a spot
/// over its `cone`'s solid angle (the falloff band adds light outside it and is not counted).
/// 800 lm in a 60° cone is 950.36 cd. Zero for an off fixture or a non-finite input.
pub fn intensity_cd(f: &LightFixture) -> f64 {
    if !f.on || !f.lumens.is_finite() || f.lumens <= 0.0 {
        return 0.0;
    }
    let omega = match f.kind {
        LightKind::Point => 4.0 * std::f64::consts::PI,
        LightKind::Spot => cone_solid_angle(f.cone),
    };
    if omega.is_finite() && omega > 0.0 { f.lumens / omega } else { 0.0 }
}

/// Check a fixture's values: finite position; lumens in `0..=MAX_LUMENS`; kelvin in
/// `MIN_KELVIN..=MAX_KELVIN`; a spot's cone in `(0, MAX_SPOT_ANGLE]` and falloff in
/// `cone..=MAX_SPOT_ANGLE`; tilt in `-90..=90`; rotation finite; a height, when set, finite and in
/// `0..=100`. The message names the field (`lumens must be 0..=200000, got -5`).
pub fn validate_fixture(f: &LightFixture) -> Result<(), String> {
    let _ = f;
    Ok(())
}

/// Linear RGB (each 0..=1, the largest component 1) of a black body at `kelvin`, clamped to
/// `MIN_KELVIN..=MAX_KELVIN`; non-finite = 6500 K (white).
pub fn kelvin_to_rgb(kelvin: f64) -> [f64; 3] {
    let _ = kelvin;
    [1.0, 1.0, 1.0]
}

/// The world position of the fixture's light point: its plan position at the level's elevation
/// plus its height (see `LightFixture::height` for the defaults by mount). `None` when the level
/// or a coordinate is not finite.
pub fn light_point(model: &ArchModel, f: &LightFixture) -> Option<Vec3> {
    let _ = (model, f);
    None
}

/// Unit direction a spot points: `tilt` degrees below the horizontal along `rotation` (90 =
/// straight down `[0,0,-1]`). A point light also returns this (it is unused).
pub fn aim_direction(f: &LightFixture) -> Vec3 {
    let _ = f;
    Vec3::new(0.0, 0.0, -1.0)
}

/// Light positions of an even `rows` × `cols` grid over `ring`'s bounding box: cell centres
/// (x = min.x + (i + 0.5)·w/cols, y = min.y + (j + 0.5)·h/rows), kept only when inside `ring`, in
/// row-major order from the minimum corner. Empty for rows or cols outside `1..=MAX_FIXTURE_GRID`
/// or a ring of fewer than three finite points.
pub fn grid_positions(ring: &[Vec2], rows: u32, cols: u32) -> Vec<Vec2> {
    let _ = (ring, rows, cols);
    Vec::new()
}

/// Plan symbol polylines of a fixture (in `crate::light_symbol`, filled by `p10-arch-light-symbol`).
pub use crate::light_symbol::plan_symbol;

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::ArchId;

    #[test]
    fn the_pot_light_is_800_lumens_in_a_60_degree_cone() {
        let t = find_type(" recessed POT light ").unwrap_or_else(|| catalogue().remove(1));
        assert_eq!((t.kind, t.lumens, t.kelvin, t.cone, t.falloff), (LightKind::Spot, 800.0, 3000.0, 60.0, 60.0));
        assert_eq!(catalogue().len(), 5);
        assert!(find_type("Pendant").is_some() && find_type("Wall sconce").is_some() && find_type("Surface ceiling fixture").is_some());
    }

    #[test]
    fn a_60_degree_cone_of_800_lumens_is_950_candela() {
        assert!((cone_solid_angle(60.0) - 0.841_787).abs() < 1e-6);
        let t = find_type("Recessed pot light").unwrap_or_else(|| catalogue().remove(0));
        let f = fixture_from_type(ArchId(0), &t, Vec2::new(1.0, 1.0));
        assert!((intensity_cd(&f) - 950.36).abs() < 0.01, "{}", intensity_cd(&f));
        let off = LightFixture { on: false, ..f.clone() };
        assert_eq!(intensity_cd(&off), 0.0);
        let point = LightFixture { kind: LightKind::Point, lumens: 4.0 * std::f64::consts::PI, ..f };
        assert!((intensity_cd(&point) - 1.0).abs() < 1e-12);
    }
}
