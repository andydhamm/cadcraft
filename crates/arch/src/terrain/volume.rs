//! Cut and fill between the original and the modified surface.
//!
//! The mesh refines the original TIN, so over each triangle the height difference is linear and
//! the volume is integrated exactly, splitting a triangle at its zero line when the sign changes.

use super::mesh::SiteMesh;
use super::surface::SiteSurface;

/// `(cut, fill)` in m³: where the modified surface is below the original, and above it.
pub fn cut_fill(mesh: &SiteMesh, surf: &SiteSurface) -> (f64, f64) {
    let mut cut = 0.0;
    let mut fill = 0.0;
    for (tri, zs) in mesh.triangles.iter().zip(&mesh.z) {
        let mut corners = [(0.0, 0.0); 3];
        let mut d = [0.0; 3];
        let mut ok = true;
        for (k, &ci) in tri.iter().enumerate() {
            let (Some(&p), Some(&z)) = (mesh.xy.get(ci), zs.get(k)) else {
                ok = false;
                break;
            };
            let Some(orig) = surf.original_at(p) else {
                ok = false;
                break;
            };
            if let (Some(c), Some(dk)) = (corners.get_mut(k), d.get_mut(k)) {
                *c = (p.x, p.y);
                *dk = z - orig;
            }
        }
        if !ok || d.iter().any(|v| !v.is_finite()) {
            continue;
        }
        let [(ax, ay), (bx, by), (cx, cy)] = corners;
        let area = 0.5 * ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)).abs();
        if !area.is_finite() {
            continue;
        }
        let total = area * (d[0] + d[1] + d[2]) / 3.0;
        if d.iter().all(|&v| v >= 0.0) {
            fill += total;
        } else if d.iter().all(|&v| v <= 0.0) {
            cut -= total;
        } else {
            // Mixed signs: find a vertex whose neighbours are on the other side (or on the line).
            let lone = (0..3).find(|&i| {
                let (p, q, r) = (d[i], d[(i + 1) % 3], d[(i + 2) % 3]);
                p != 0.0 && p * q <= 0.0 && p * r <= 0.0
            });
            let Some(i) = lone else { continue };
            let (p, q, r) = (d[i], d[(i + 1) % 3], d[(i + 2) % 3]);
            let denom = 3.0 * (p - q) * (p - r);
            if denom == 0.0 || !denom.is_finite() {
                continue;
            }
            let part = area * p * p * p / denom; // sign of p
            let rest = total - part;
            for v in [part, rest] {
                if v > 0.0 {
                    fill += v;
                } else {
                    cut -= v;
                }
            }
        }
    }
    (cut.max(0.0), fill.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::mesh::site_mesh;
    use cadcraft_doc::arch::{ArchModel, ModifierKind, Terrain, TerrainModifier};
    use cadcraft_geom::{Vec2, Vec3};

    fn model(modifiers: Vec<ModifierKind>) -> ArchModel {
        let terrain = Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)],
            points: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)],
            modifiers: modifiers.into_iter().enumerate().map(|(i, kind)| TerrainModifier { id: i as u32 + 1, kind }).collect(),
            pad: false,
            ..Terrain::default()
        };
        ArchModel { terrain: Some(terrain), ..ArchModel::default() }
    }

    fn volumes(modifiers: Vec<ModifierKind>) -> (f64, f64) {
        let m = model(modifiers);
        let surf = SiteSurface::new(&m).expect("surface");
        let mesh = site_mesh(&surf).expect("mesh");
        cut_fill(&mesh, &surf)
    }

    #[test]
    fn cut_fill_of_a_flat_region_on_a_slope() {
        let poly = vec![Vec2::new(15.0, 10.0), Vec2::new(25.0, 10.0), Vec2::new(25.0, 20.0), Vec2::new(15.0, 20.0)];
        let (cut, fill) = volumes(vec![ModifierKind::Flat { polygon: poly, elevation: 1.0, margin: 0.0 }]);
        assert!((cut - 6.25).abs() < 1e-9, "cut {cut}");
        assert!((fill - 6.25).abs() < 1e-9, "fill {fill}");
    }

    #[test]
    fn cut_fill_is_zero_without_modifiers() {
        let (cut, fill) = volumes(vec![]);
        assert!(cut.abs() < 1e-9 && fill.abs() < 1e-9, "{cut} {fill}");
    }

    #[test]
    fn cut_fill_splits_a_triangle_at_its_zero_line() {
        let m = model(vec![]);
        let surf = SiteSurface::new(&m).expect("surface");
        let (a, b, c) = (Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0), Vec2::new(10.0, 20.0));
        let o = |p: Vec2| surf.original_at(p).unwrap_or(0.0);
        let mesh = SiteMesh { xy: vec![a, b, c], triangles: vec![[0, 1, 2]], z: vec![[o(a) + 1.0, o(b), o(c) - 1.0]], ..Default::default() };
        // Area 50, d = (1, 0, -1): the total is zero and each side holds A/18... = 50/6 / 1.
        let (cut, fill) = cut_fill(&mesh, &surf);
        assert!((cut - 50.0 / 6.0).abs() < 1e-9, "cut {cut}");
        assert!((fill - 50.0 / 6.0).abs() < 1e-9, "fill {fill}");
        // Lone positive vertex: d = (2, -1, -1): fill = A*8/(3*3*3), cut = fill - total.
        let mesh = SiteMesh { z: vec![[o(a) + 2.0, o(b) - 1.0, o(c) - 1.0]], ..mesh };
        let (cut, fill) = cut_fill(&mesh, &surf);
        let f = 50.0 * 8.0 / 27.0;
        assert!((fill - f).abs() < 1e-9, "fill {fill}");
        assert!((cut - f).abs() < 1e-9, "cut {cut}"); // total is zero
    }

    #[test]
    fn cut_fill_of_a_hill_is_all_fill() {
        let (cut, fill) = volumes(vec![ModifierKind::Hill { center: Vec2::new(20.0, 15.0), height: 2.0, radius: 6.0 }]);
        assert!(cut.abs() < 1e-9, "cut {cut}");
        assert!(fill > 0.0, "fill {fill}");
    }

    #[test]
    fn cut_fill_of_an_empty_mesh_is_zero() {
        let m = model(vec![]);
        let surf = SiteSurface::new(&m).expect("surface");
        let mesh = SiteMesh::default();
        let (cut, fill) = cut_fill(&mesh, &surf);
        assert_eq!(cut, 0.0);
        assert_eq!(fill, 0.0);
    }

    #[test]
    fn cut_fill_of_a_low_flat_region_is_all_cut() {
        let poly = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let (cut, fill) = volumes(vec![ModifierKind::Flat { polygon: poly, elevation: 0.0, margin: 0.0 }]);
        assert!(fill.abs() < 1e-9, "fill {fill}");
        assert!(cut > 0.0, "cut {cut}");
    }

    #[test]
    fn cut_fill_skips_triangles_with_non_finite_z() {
        let m = model(vec![]);
        let surf = SiteSurface::new(&m).expect("surface");
        let (a, b, c) = (Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0), Vec2::new(10.0, 20.0));
        let o = |p: Vec2| surf.original_at(p).unwrap_or(0.0);
        let mesh = SiteMesh {
            xy: vec![a, b, c, a, b, c],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
            z: vec![[f64::NAN, o(b), o(c)], [o(a) + 1.0, o(b) + 1.0, o(c) + 1.0]],
            ..Default::default()
        };
        let (cut, fill) = cut_fill(&mesh, &surf);
        assert!(cut.abs() < 1e-9, "cut {cut}");
        assert!((fill - 50.0).abs() < 1e-9, "fill {fill}");
    }

    #[test]
    fn cut_fill_degenerate_triangle_contributes_nothing() {
        let m = model(vec![]);
        let surf = SiteSurface::new(&m).expect("surface");
        let (a, b, c) = (Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0), Vec2::new(30.0, 10.0));
        let o = |p: Vec2| surf.original_at(p).unwrap_or(0.0);
        let mesh = SiteMesh { xy: vec![a, b, c], triangles: vec![[0, 1, 2]], z: vec![[o(a) + 1.0, o(b) + 2.0, o(c) + 3.0]], ..Default::default() };
        let (cut, fill) = cut_fill(&mesh, &surf);
        assert!(cut.abs() < 1e-9, "cut {cut}");
        assert!(fill.abs() < 1e-9, "fill {fill}");
    }

    #[test]
    fn cut_fill_over_multiple_triangles() {
        let m = model(vec![]);
        let surf = SiteSurface::new(&m).expect("surface");
        let (a, b, c, d) = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0));
        let o = |p: Vec2| surf.original_at(p).unwrap_or(0.0);
        let mesh = SiteMesh {
            xy: vec![a, b, c, d],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            z: vec![[o(a) + 1.0, o(b) + 1.0, o(c) + 1.0], [o(a) + 1.0, o(c) + 1.0, o(d) + 1.0]],
            ..Default::default()
        };
        let (cut, fill) = cut_fill(&mesh, &surf);
        assert!(cut.abs() < 1e-9, "cut {cut}");
        assert!((fill - 100.0).abs() < 1e-9, "fill {fill}");
    }

    #[test]
    fn cut_fill_results_are_non_negative() {
        let (cut, fill) = volumes(vec![
            ModifierKind::Flat {
                polygon: vec![Vec2::new(15.0, 10.0), Vec2::new(25.0, 10.0), Vec2::new(25.0, 20.0), Vec2::new(15.0, 20.0)],
                elevation: 1.0,
                margin: 0.0,
            },
            ModifierKind::Hill { center: Vec2::new(10.0, 5.0), height: 1.0, radius: 3.0 },
        ]);
        assert!(cut >= 0.0, "cut {cut}");
        assert!(fill >= 0.0, "fill {fill}");
    }
}
