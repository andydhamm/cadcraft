//! The building pad and basement cuts. Filled in by `p9-arch-terrain-pad`.

use std::cmp::Ordering;

use cadcraft_doc::arch::{ArchModel, Level, PAD_SLOPE};
use cadcraft_geom::{Vec2, point_in_polygon};

/// Whether a region is the pad under the first floor or a basement's cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKind {
    Pad,
    Basement,
}

/// A footprint the ground is flattened to.
#[derive(Clone, Debug, PartialEq)]
pub struct PadRegion {
    pub kind: PadKind,
    /// The footprint's outer ring (counter-clockwise).
    pub ring: Vec<Vec2>,
    /// The ground's elevation inside: the level's elevation minus its floor structure.
    pub elevation: f64,
    /// Flat apron beyond the ring (pad only; 0 for a basement).
    pub margin: f64,
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn level_first_floor(model: &ArchModel) -> Option<&Level> {
    model.levels.iter().filter(|level| level.elevation.is_finite() && level.elevation >= 0.0).min_by(|a, b| {
        match a.elevation.partial_cmp(&b.elevation).unwrap_or(Ordering::Equal) {
            Ordering::Equal => a.index.cmp(&b.index),
            other => other,
        }
    })
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let ap = p - a;
    let ab2 = ab.dot(ab);
    if ab2 <= 1e-18 {
        return p.dist(a);
    }
    let t = (ap.dot(ab) / ab2).clamp(0.0, 1.0);
    let q = a + ab * t;
    p.dist(q)
}

fn ring_distance(p: Vec2, ring: &[Vec2]) -> f64 {
    if ring.len() < 2 {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let d = segment_distance(p, a, b);
        if d < best {
            best = d;
        }
    }
    best
}

fn inside_or_on_ring(p: Vec2, ring: &[Vec2]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    point_in_polygon(ring, p) || ring_distance(p, ring) <= 1e-9
}

/// The pad of the first floor and the cut of every basement level that has a footprint.
pub fn pad_regions(model: &ArchModel) -> Vec<PadRegion> {
    let Some(terrain) = model.terrain.as_ref() else {
        return Vec::new();
    };

    let mut regions = Vec::new();
    let Some(first) = level_first_floor(model) else {
        return regions;
    };

    if terrain.pad {
        if let Some(fp) = crate::roof::outer_footprint(model, first.index) {
            regions.push(PadRegion {
                kind: PadKind::Pad,
                ring: fp.outer,
                elevation: finite_or(first.elevation - first.structure, first.elevation),
                margin: finite_or(terrain.pad_margin, 0.0),
            });
        }
    }

    let mut basements: Vec<&Level> = model.levels.iter().filter(|level| level.elevation.is_finite() && level.elevation < first.elevation).collect();
    basements.sort_by_key(|level| level.index);
    for level in basements {
        let Some(fp) = crate::roof::outer_footprint(model, level.index) else {
            continue;
        };
        regions.push(PadRegion {
            kind: PadKind::Basement,
            ring: fp.outer,
            elevation: finite_or(level.elevation - level.structure, level.elevation),
            margin: 0.0,
        });
    }

    regions
}

/// Apply `regions` to elevation `z` at `p` (containment decided a hair toward `toward`).
pub fn apply_pads(regions: &[PadRegion], p: Vec2, toward: Vec2, z: f64) -> f64 {
    if !p.is_finite() || !toward.is_finite() || !z.is_finite() {
        return z;
    }

    let mut z = z;
    for region in regions {
        if region.ring.is_empty() {
            continue;
        }

        let q = if toward == p { p } else { p + (toward - p) * 1e-6 };
        let inside = inside_or_on_ring(q, &region.ring);
        let d = if inside { 0.0 } else { ring_distance(p, &region.ring) };
        let elevation = finite_or(region.elevation, z);
        let margin = finite_or(region.margin, 0.0);

        match region.kind {
            PadKind::Pad => {
                if inside || d <= margin {
                    z = elevation;
                } else {
                    let s = d - margin;
                    if z > elevation {
                        z = z.min(elevation + PAD_SLOPE * s);
                    } else {
                        z = z.max(elevation - PAD_SLOPE * s);
                    }
                }
            }
            PadKind::Basement => {
                if inside {
                    z = z.min(elevation);
                }
            }
        }
    }

    z
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{ArchModel, Level, Terrain, Wall};

    fn make_house_model() -> ArchModel {
        let mut model = ArchModel::default();
        model.terrain = Some(Terrain::default());
        model.levels = vec![
            Level { index: 0, name: "Basement".into(), elevation: -2.7, ..Default::default() },
            Level { index: 1, name: "Level 1".into(), elevation: 0.0, ..Default::default() },
        ];
        model.walls = vec![
            Wall {
                id: cadcraft_doc::ArchId(0),
                start: Vec2::new(15.0, 11.0),
                end: Vec2::new(25.0, 11.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(1),
                start: Vec2::new(25.0, 11.0),
                end: Vec2::new(25.0, 19.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(2),
                start: Vec2::new(25.0, 19.0),
                end: Vec2::new(15.0, 19.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(3),
                start: Vec2::new(15.0, 19.0),
                end: Vec2::new(15.0, 11.0),
                base_level: 0,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(4),
                start: Vec2::new(15.0, 11.0),
                end: Vec2::new(25.0, 11.0),
                base_level: 1,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(5),
                start: Vec2::new(25.0, 11.0),
                end: Vec2::new(25.0, 19.0),
                base_level: 1,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(6),
                start: Vec2::new(25.0, 19.0),
                end: Vec2::new(15.0, 19.0),
                base_level: 1,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
            Wall {
                id: cadcraft_doc::ArchId(7),
                start: Vec2::new(15.0, 19.0),
                end: Vec2::new(15.0, 11.0),
                base_level: 1,
                thickness: 0.2,
                exterior: true,
                ..Wall::default()
            },
        ];
        model
    }

    #[test]
    fn pad_regions_finds_the_first_floor_and_basements() {
        let model = make_house_model();
        let regions = pad_regions(&model);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].kind, PadKind::Pad);
        assert_eq!(regions[0].elevation, -0.3);
        assert_eq!(regions[1].kind, PadKind::Basement);
        assert_eq!(regions[1].elevation, -3.0);
    }

    #[test]
    fn pad_flattens_inside_and_on_the_apron() {
        let rings = vec![Vec2::new(14.9, 11.0), Vec2::new(25.1, 11.0), Vec2::new(25.1, 19.0), Vec2::new(14.9, 19.0)];
        let region = PadRegion { kind: PadKind::Pad, ring: rings, elevation: -0.3, margin: 0.3 };

        assert_eq!(apply_pads(&[region.clone()], Vec2::new(20.0, 15.0), Vec2::new(20.0, 15.0), 0.0), -0.3);
        assert_eq!(apply_pads(&[region.clone()], Vec2::new(14.8, 15.0), Vec2::new(14.8, 15.0), 0.0), -0.3);
    }

    #[test]
    fn pad_grades_back_at_one_in_two() {
        let region = PadRegion {
            kind: PadKind::Pad,
            ring: vec![Vec2::new(14.9, 11.0), Vec2::new(25.1, 11.0), Vec2::new(25.1, 19.0), Vec2::new(14.9, 19.0)],
            elevation: -0.3,
            margin: 0.3,
        };
        let z = apply_pads(&[region], Vec2::new(14.4, 15.0), Vec2::new(14.4, 15.0), 0.72);
        assert!((z - (-0.2)).abs() < 1e-9);
    }

    #[test]
    fn basement_cuts_down_inside_its_footprint() {
        let region = PadRegion {
            kind: PadKind::Basement,
            ring: vec![Vec2::new(14.9, 11.0), Vec2::new(25.1, 11.0), Vec2::new(25.1, 19.0), Vec2::new(14.9, 19.0)],
            elevation: -3.0,
            margin: 0.0,
        };
        assert_eq!(apply_pads(&[region.clone()], Vec2::new(20.0, 15.0), Vec2::new(20.0, 15.0), 0.0), -3.0);
        assert_eq!(apply_pads(&[region], Vec2::new(30.0, 15.0), Vec2::new(30.0, 15.0), 0.0), 0.0);
    }

    #[test]
    fn no_terrain_means_no_pad() {
        let model = ArchModel::default();
        assert!(pad_regions(&model).is_empty());
    }
}
