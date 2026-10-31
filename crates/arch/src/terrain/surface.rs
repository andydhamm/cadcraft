//! The modified surface: the original TIN with the modifiers, the building pad and basement cuts
//! applied, as an exact elevation function.

use cadcraft_doc::arch::{ArchModel, ModifierKind, TerrainModifier};
use cadcraft_geom::tin::Tin;
use cadcraft_geom::tin_ops::TinIndex;
use cadcraft_geom::{Vec2, point_in_polygon};

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
    index: TinIndex,
}

impl SiteSurface {
    /// Build the surface of `model`'s terrain; an error when there is none or it is invalid.
    pub fn new(model: &ArchModel) -> Result<SiteSurface, TerrainError> {
        let terrain = model.terrain.as_ref().ok_or_else(|| TerrainError("the drawing has no terrain".into()))?;
        let original = super::original_tin(terrain)?;
        let index = TinIndex::new(&original);
        let breaklines = model.walls.iter().filter(|w| w.retaining).map(|w| [w.start, w.end]).collect();
        Ok(SiteSurface {
            original,
            boundary: terrain.boundary.clone(),
            modifiers: terrain.modifiers.clone(),
            pads: super::pad::pad_regions(model),
            breaklines,
            index,
        })
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
        if !p.is_finite() || !inside_or_on(&self.boundary, p) {
            return None;
        }
        let (i, w) = self.index.locate(&self.original, p)?;
        let tri = self.original.triangles.get(i)?;
        let mut z = 0.0;
        for (k, wk) in tri.iter().zip(w) {
            z += self.original.points.get(*k)?.z * wk;
        }
        z.is_finite().then_some(z)
    }

    /// Elevation of the modified surface at `p`, `None` outside the boundary.
    pub fn modified_at(&self, p: Vec2) -> Option<f64> {
        self.modified_toward(p, p)
    }

    /// The modified elevation at `p`, deciding which side of a region edge `p` is on by looking
    /// a hair from `p` toward `toward` (so a mesh corner on a step takes its own triangle's side).
    pub fn modified_toward(&self, p: Vec2, toward: Vec2) -> Option<f64> {
        let mut z = self.original_at(p)?;
        let q = if toward.is_finite() && toward != p { p + (toward - p) * 1e-6 } else { p };
        for modifier in &self.modifiers {
            match &modifier.kind {
                ModifierKind::Flat { polygon, elevation, margin } => {
                    if inside_or_on(polygon, q) {
                        z = *elevation;
                    } else if *margin > 0.0 {
                        let d = ring_distance(p, polygon);
                        if d < *margin {
                            z = elevation + (z - elevation) * (d / margin);
                        }
                    }
                }
                ModifierKind::Hill { center, height, radius } => {
                    if *radius > 0.0 {
                        let t = p.dist(*center) / radius;
                        if t < 1.0 {
                            let f = 1.0 - t * t;
                            z += height * f * f;
                        }
                    }
                }
                ModifierKind::Path { .. } => {}
            }
        }
        let z = super::pad::apply_pads(&self.pads, p, toward, z);
        z.is_finite().then_some(z)
    }
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 <= 1e-18 {
        return p.dist(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

fn ring_distance(p: Vec2, ring: &[Vec2]) -> f64 {
    let n = ring.len();
    let mut best = f64::INFINITY;
    for (i, a) in ring.iter().enumerate() {
        if let Some(b) = ring.get((i + 1) % n) {
            best = best.min(segment_distance(p, *a, *b));
        }
    }
    best
}

/// Even-odd containment where a point on the ring counts as inside.
fn inside_or_on(ring: &[Vec2], p: Vec2) -> bool {
    ring.len() >= 3 && (point_in_polygon(ring, p) || ring_distance(p, ring) <= 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{Terrain, Wall};
    use cadcraft_geom::Vec3;

    fn model(modifiers: Vec<ModifierKind>) -> ArchModel {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let modifiers = modifiers.into_iter().enumerate().map(|(i, kind)| TerrainModifier { id: i as u32 + 1, kind }).collect();
        let terrain = Terrain { boundary, points, modifiers, pad: false, ..Terrain::default() };
        ArchModel { terrain: Some(terrain), ..ArchModel::default() }
    }

    fn flat(margin: f64) -> ModifierKind {
        ModifierKind::Flat {
            polygon: vec![Vec2::new(30.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(30.0, 30.0)],
            elevation: 2.5,
            margin,
        }
    }

    fn at(m: &ArchModel, x: f64, y: f64) -> Option<f64> {
        SiteSurface::new(m).ok()?.modified_at(Vec2::new(x, y))
    }

    fn near(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn surface_follows_the_original_tin() {
        let m = model(Vec::new());
        let s = SiteSurface::new(&m).unwrap();
        assert!(near(s.original_at(Vec2::new(25.0, 12.0)), 1.25));
        assert!(near(s.modified_at(Vec2::new(25.0, 12.0)), 1.25));
        assert!(SiteSurface::new(&ArchModel::default()).is_err());
    }

    #[test]
    fn flat_region_with_margin_zero_is_a_step() {
        let m = model(vec![flat(0.0)]);
        assert!(near(at(&m, 30.01, 15.0), 2.5));
        assert!(near(at(&m, 29.99, 15.0), 1.4995));
    }

    #[test]
    fn flat_region_margin_blends_back() {
        let m = model(vec![flat(2.0)]);
        // 1 m outside the edge: halfway between the flat elevation and the ground (1.45).
        assert!(near(at(&m, 29.0, 15.0), 2.5 + (1.45 - 2.5) * 0.5));
        assert!(near(at(&m, 27.0, 15.0), 1.35));
    }

    #[test]
    fn hill_and_valley_use_the_smooth_falloff() {
        let hill = ModifierKind::Hill { center: Vec2::new(10.0, 20.0), height: 2.0, radius: 5.0 };
        let m = model(vec![hill]);
        assert!(near(at(&m, 10.0, 20.0), 2.5));
        assert!(near(at(&m, 13.0, 20.0), 0.65 + 2.0 * 0.4096));
        assert!(near(at(&m, 16.0, 20.0), 0.8));
        let valley = ModifierKind::Hill { center: Vec2::new(10.0, 20.0), height: -2.0, radius: 5.0 };
        assert!(near(at(&model(vec![valley]), 10.0, 20.0), -1.5));
    }

    #[test]
    fn modified_toward_takes_the_side_it_looks_at() {
        let m = model(vec![flat(0.0)]);
        let s = SiteSurface::new(&m).unwrap();
        let p = Vec2::new(30.0, 15.0);
        assert!(near(s.modified_toward(p, Vec2::new(31.0, 15.0)), 2.5));
        assert!(near(s.modified_toward(p, Vec2::new(29.0, 15.0)), 1.5));
    }

    #[test]
    fn surface_is_none_outside_the_boundary() {
        let m = model(Vec::new());
        let s = SiteSurface::new(&m).unwrap();
        assert!(s.original_at(Vec2::new(-1.0, 5.0)).is_none());
        assert!(s.modified_at(Vec2::new(20.0, 31.0)).is_none());
        assert!(s.modified_at(Vec2::new(f64::NAN, 1.0)).is_none());
        assert!(s.modified_at(Vec2::new(0.0, 15.0)).is_some());
    }

    #[test]
    fn surface_lists_retaining_walls_as_breaklines() {
        let mut m = model(Vec::new());
        let mut w = Wall::new(Vec2::new(5.0, 5.0), Vec2::new(15.0, 5.0), 0.3);
        w.retaining = true;
        m.walls.push(w);
        m.walls.push(Wall::new(Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0), 0.3));
        let s = SiteSurface::new(&m).unwrap();
        assert_eq!(s.breaklines(), &[[Vec2::new(5.0, 5.0), Vec2::new(15.0, 5.0)]]);
    }
}
