//! Retaining walls: their top and bottom from the ground on each side.

use cadcraft_doc::arch::{ArchModel, DEFAULT_RETAINING_FOOTING, DEFAULT_RETAINING_FREEBOARD, Wall};
use cadcraft_geom::Vec2;

use super::surface::SiteSurface;

/// Samples along the wall's centre line (0..=SAMPLES).
const SAMPLES: usize = 8;

/// `(base, top)` of retaining wall `w`, or `None` without a terrain (the wall keeps its level span).
///
/// The top stands `freeboard` above the highest ground on the high side; the base is `footing`
/// below the lowest ground on the low side.
pub fn retaining_span(model: &ArchModel, w: &Wall) -> Option<(f64, f64)> {
    if !w.retaining || model.terrain.is_none() {
        return None;
    }
    let delta = w.end - w.start;
    let len = delta.len();
    if !(w.start.is_finite() && w.end.is_finite() && w.thickness.is_finite()) || !(len >= 1e-9) || w.thickness <= 0.0 {
        return None;
    }
    let surf = SiteSurface::new(model).ok()?;
    let u = delta * (1.0 / len);
    let n = Vec2::new(-u.y, u.x);
    let off = w.thickness / 2.0 + 0.01;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for k in 0..=SAMPLES {
        let c = w.start + delta * (k as f64 / SAMPLES as f64);
        if let Some(z) = surf.modified_at(c + n * off) {
            left.push(z);
        }
        if let Some(z) = surf.modified_at(c - n * off) {
            right.push(z);
        }
    }
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let (high, low) = if mean(&left) >= mean(&right) { (&left, &right) } else { (&right, &left) };
    let freeboard = w.freeboard.unwrap_or(DEFAULT_RETAINING_FREEBOARD);
    let footing = w.footing.unwrap_or(DEFAULT_RETAINING_FOOTING);
    let top = high.iter().copied().fold(f64::NEG_INFINITY, f64::max) + freeboard;
    let bottom = low.iter().copied().fold(f64::INFINITY, f64::min) - footing;
    (top.is_finite() && bottom.is_finite() && top > bottom).then_some((bottom, top))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{ModifierKind, Terrain, TerrainModifier};
    use cadcraft_geom::Vec3;

    fn model() -> ArchModel {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let kind = ModifierKind::Flat {
            polygon: vec![Vec2::new(30.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(30.0, 30.0)],
            elevation: 2.5,
            margin: 0.0,
        };
        let terrain = Terrain { boundary, points, modifiers: vec![TerrainModifier { id: 1, kind }], pad: false, ..Terrain::default() };
        ArchModel { terrain: Some(terrain), ..ArchModel::default() }
    }

    fn wall() -> Wall {
        let mut w = Wall::new(Vec2::new(30.0, 0.0), Vec2::new(30.0, 30.0), 0.2);
        w.retaining = true;
        w
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn retaining_span_rises_above_the_high_side() {
        let (_, top) = retaining_span(&model(), &wall()).expect("span");
        assert!(near(top, 2.65), "{top}");
    }

    #[test]
    fn retaining_span_footing_below_the_low_side() {
        let (bottom, _) = retaining_span(&model(), &wall()).expect("span");
        assert!(near(bottom, 1.1945), "{bottom}");
    }

    #[test]
    fn retaining_span_honours_freeboard_and_footing() {
        let mut w = wall();
        w.freeboard = Some(0.5);
        w.footing = Some(1.0);
        let (bottom, top) = retaining_span(&model(), &w).expect("span");
        assert!(near(top, 3.0) && near(bottom, 0.4945), "{bottom} {top}");
    }

    #[test]
    fn retaining_span_is_none_without_a_terrain() {
        let m = ArchModel::default();
        assert!(retaining_span(&m, &wall()).is_none());
        let mut w = wall();
        w.retaining = false;
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_horizontal_wall() {
        // Terrain sloping in y: z = 0.1*y
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 1.0), Vec3::new(0.0, 10.0, 1.0)];
        let terrain = Terrain { boundary, points, ..Terrain::default() };
        let model = ArchModel { terrain: Some(terrain), ..ArchModel::default() };

        // Wall from (0,5) to (10,5), 0.2 thick
        let mut w = Wall::new(Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 0.2);
        w.retaining = true;

        // Left side (y=5.11) is higher: z = 0.511
        // Right side (y=4.89) is lower: z = 0.489
        // Top = 0.511 + 0.15 = 0.661
        // Bottom = 0.489 - 0.3 = 0.189
        let (bottom, top) = retaining_span(&model, &w).expect("span");
        assert!(near(top, 0.661), "top: {top}");
        assert!(near(bottom, 0.189), "bottom: {bottom}");
    }

    #[test]
    fn retaining_span_flat_terrain() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let points = vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 1.0), Vec3::new(10.0, 10.0, 1.0), Vec3::new(0.0, 10.0, 1.0)];
        let terrain = Terrain { boundary, points, ..Terrain::default() };
        let model = ArchModel { terrain: Some(terrain), ..ArchModel::default() };

        let mut w = Wall::new(Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 0.2);
        w.retaining = true;

        // Both sides at 1.0
        // Top = 1.0 + 0.15 = 1.15
        // Bottom = 1.0 - 0.3 = 0.7
        let (bottom, top) = retaining_span(&model, &w).expect("span");
        assert!(near(top, 1.15), "top: {top}");
        assert!(near(bottom, 0.7), "bottom: {bottom}");
    }

    #[test]
    fn retaining_span_top_equals_bottom() {
        // Flat terrain with zero freeboard and footing: top == bottom -> None
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let points = vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 1.0), Vec3::new(10.0, 10.0, 1.0), Vec3::new(0.0, 10.0, 1.0)];
        let terrain = Terrain { boundary, points, ..Terrain::default() };
        let model = ArchModel { terrain: Some(terrain), ..ArchModel::default() };

        let mut w = Wall::new(Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 0.2);
        w.retaining = true;
        w.freeboard = Some(0.0);
        w.footing = Some(0.0);

        // Both sides at 1.0, freeboard = 0, footing = 0
        // top = 1.0, bottom = 1.0, so top == bottom -> None
        assert!(retaining_span(&model, &w).is_none());
    }

    #[test]
    fn retaining_span_nan_freeboard() {
        let mut w = wall();
        w.freeboard = Some(f64::NAN);
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_inf_footing() {
        let mut w = wall();
        w.footing = Some(f64::INFINITY);
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_degenerate_wall() {
        let mut w = Wall::new(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0), 0.2);
        w.retaining = true;
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_zero_thickness() {
        let mut w = Wall::new(Vec2::new(30.0, 0.0), Vec2::new(30.0, 30.0), 0.0);
        w.retaining = true;
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_negative_thickness() {
        let mut w = Wall::new(Vec2::new(30.0, 0.0), Vec2::new(30.0, 30.0), -0.2);
        w.retaining = true;
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_nan_start() {
        let mut w = Wall::new(Vec2::new(f64::NAN, 0.0), Vec2::new(30.0, 30.0), 0.2);
        w.retaining = true;
        assert!(retaining_span(&model(), &w).is_none());
    }

    #[test]
    fn retaining_span_inf_end() {
        let mut w = Wall::new(Vec2::new(30.0, 0.0), Vec2::new(f64::INFINITY, 30.0), 0.2);
        w.retaining = true;
        assert!(retaining_span(&model(), &w).is_none());
    }
}
