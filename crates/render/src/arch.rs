use std::collections::BTreeMap;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{ArchId, Drawing, Handle, Wall};
use cadcraft_geom::Vec2;

use crate::{DPrim, DisplayList, Kind, Options};

pub use cadcraft_doc::is_arch_handle;

/// Stable proxy handle for an architectural object: arch objects have no entity handle, so
/// selection and picking map back to the `ArchId` through this reserved range.
pub fn arch_handle(id: ArchId) -> Handle {
    cadcraft_doc::arch_handle(id)
}

/// The poché (2D cross-section) primitives for `d.arch`, in wall order. The caller merges the
/// returned list into the main one. `_opts` is unused for now but kept for the caller's hook.
pub fn build_arch(d: &Drawing, _opts: &Options) -> DisplayList {
    let mut list = DisplayList::default();
    let layer = d.layer("A-WALL").map(|l| l.color).unwrap_or(Color::Index(7));
    let ink = crate::ink(Color::ByLayer, layer, Color::Index(7));

    let shapes = cadcraft_arch::join::solve_joins(&d.arch);
    let solved: BTreeMap<ArchId, &Vec<Vec2>> = shapes.iter().map(|s| (s.id, &s.polygon)).collect();

    for wall in &d.arch.walls {
        if !wall.start.is_finite() || !wall.end.is_finite() {
            continue;
        }
        let len = wall.start.dist(wall.end);
        if len < 1e-12 || !len.is_finite() {
            continue;
        }
        if wall.thickness <= 0.0 || !wall.thickness.is_finite() {
            continue;
        }
        let half = wall.thickness / 2.0;
        let dir = (wall.end - wall.start).normalized();
        let perp = dir.perp();
        let a = wall.start - perp * half;
        let b = wall.end - perp * half;
        let c = wall.end + perp * half;
        let e = wall.start + perp * half;

        let ring: Vec<Vec2> = match solved.get(&wall.id) {
            Some(r) if !r.is_empty() => r.iter().map(|p| p.clone()).collect(),
            _ => vec![a, b, c, e],
        };

        // Filled poché
        let tris = crate::triangulate_evenodd(&[ring.clone()]);
        if tris.len() >= 3 {
            let start = u32::try_from(list.tris.len()).unwrap_or(u32::MAX);
            for q in &tris {
                list.bounds.add(*q);
                list.tris.push(*q);
            }
            list.prims.push(DPrim {
                handle: arch_handle(wall.id),
                color: ink.rgb,
                aci7: ink.aci7,
                lw: 0.0,
                transparency: 0,
                kind: Kind::Tris,
                start,
                len: u32::try_from(tris.len()).unwrap_or(u32::MAX),
            });
        }

        // Outline
        let mut closed = ring;
        if !closed.is_empty() {
            closed.push(closed[0]);
        }
        let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
        for q in &closed {
            list.bounds.add(*q);
            list.verts.push(*q);
        }
        list.prims.push(DPrim {
            handle: arch_handle(wall.id),
            color: ink.rgb,
            aci7: ink.aci7,
            lw: 0.0,
            transparency: 0,
            kind: Kind::Polyline,
            start,
            len: u32::try_from(closed.len()).unwrap_or(u32::MAX),
        });
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_color::Rgb;
    use cadcraft_doc::{Drawing, Layer};

    fn wall(start: Vec2, end: Vec2, thickness: f64) -> Wall {
        Wall { start, end, thickness, height: 3.0, ..Wall::default() }
    }

    #[test]
    fn walls_draw_as_filled_poché_in_the_plan() {
        let mut d = Drawing::new_metric();
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let tris_prim = list.prims.iter().find(|p| p.kind == Kind::Tris).expect("no tris prim");
        assert!(tris_prim.len >= 3);

        let poly_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.kind == Kind::Polyline).collect();
        assert_eq!(poly_prims.len(), 1);
        let poly = &poly_prims[0];
        assert_eq!(poly.len, 5);

        let pts = list.points(poly);
        let expected = [Vec2::new(0.0, -0.1), Vec2::new(10.0, -0.1), Vec2::new(10.0, 0.1), Vec2::new(0.0, 0.1), Vec2::new(0.0, -0.1)];
        for (i, (got, exp)) in pts.iter().zip(expected.iter()).enumerate() {
            assert!((got.x - exp.x).abs() < 1e-9, "x[{}]: {} != {}", i, got.x, exp.x);
            assert!((got.y - exp.y).abs() < 1e-9, "y[{}]: {} != {}", i, got.y, exp.y);
        }

        assert!((list.bounds.min.x - 0.0).abs() < 1e-9);
        assert!((list.bounds.max.x - 10.0).abs() < 1e-9);
        assert!((list.bounds.min.y - (-0.1)).abs() < 1e-9);
        assert!((list.bounds.max.y - 0.1).abs() < 1e-9);
    }

    #[test]
    fn arch_prims_carry_the_proxy_handle() {
        let mut d = Drawing::new_metric();
        let id0 = d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let id1 = d.arch.add_wall(wall(Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        assert_eq!(list.prims[0].handle, arch_handle(id0));
        assert_eq!(list.prims[1].handle, arch_handle(id0));
        assert_eq!(list.prims[2].handle, arch_handle(id1));
        assert_eq!(list.prims[3].handle, arch_handle(id1));
    }

    #[test]
    fn empty_arch_model_adds_no_prims() {
        let d = Drawing::new_metric();
        let list = build_arch(&d, &Options::default());
        assert!(list.prims.is_empty());
        assert!(list.bounds.is_empty());
    }

    #[test]
    fn arch_poché_respects_the_layer_colour() {
        let mut d = Drawing::new_metric();
        if let Some(l) = d.layer_mut("A-WALL") {
            l.color = Color::Index(3);
        }
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());
        for p in &list.prims {
            assert_eq!(p.color, Rgb::from_u32(0x00ff00));
            assert!(!p.aci7);
        }

        let mut d = Drawing::new_metric();
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());
        for p in &list.prims {
            assert!(p.aci7);
        }
    }

    #[test]
    fn arch_handle_is_stable_and_unique_per_wall() {
        assert_eq!(arch_handle(ArchId(0)), Handle(0xA000_0000));
        assert_eq!(arch_handle(ArchId(7)).0, 0xA000_0007);
        assert_ne!(arch_handle(ArchId(0)), arch_handle(ArchId(1)));

        let mut d = Drawing::new_metric();
        let id0 = d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let id1 = d.arch.add_wall(wall(Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());
        assert_eq!(list.prims[0].handle, arch_handle(id0));
        assert_eq!(list.prims[2].handle, arch_handle(id1));
        assert_ne!(list.prims[0].handle, list.prims[2].handle);
    }

    #[test]
    fn plan_poché_mitres_exterior_corners() {
        let mut d = Drawing::new_metric();
        let t = 0.2;
        let id0 = d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), t));
        d.arch.add_wall(wall(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), t));
        d.arch.add_wall(wall(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), t));
        d.arch.add_wall(wall(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), t));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let poly = list.prims.iter().find(|p| p.kind == Kind::Polyline && p.handle == arch_handle(id0));
        assert!(poly.is_some(), "no polyline prim for bottom wall");
        let poly = *poly;
        let pts = list.points(poly);

        let expected = [Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(9.9, 0.1), Vec2::new(0.1, 0.1)];
        for exp in &expected {
            assert!(
                pts.iter().any(|p| (p.x - exp.x).abs() < 1e-6 && (p.y - exp.y).abs() < 1e-6),
                "expected point ({}, {}) not in polyline",
                exp.x,
                exp.y
            );
        }
        assert!(!pts.iter().any(|p| (p.x - 10.0).abs() < 1e-6 && (p.y - 0.1).abs() < 1e-6), "unexpected point at (10.0, 0.1)");
        assert!(!pts.iter().any(|p| (p.x - 10.0).abs() < 1e-6 && (p.y + 0.1).abs() < 1e-6), "unexpected point at (10.0, -0.1)");
    }

    #[test]
    fn plan_poché_butts_interior_walls() {
        let mut d = Drawing::new_metric();
        let t = 0.2;
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), t));
        d.arch.add_wall(wall(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), t));
        d.arch.add_wall(wall(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), t));
        d.arch.add_wall(wall(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), t));
        let id_int = d.arch.add_wall(wall(Vec2::new(0.0, 3.0), Vec2::new(10.0, 3.0), 0.1));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let poly = list.prims.iter().find(|p| p.kind == Kind::Polyline && p.handle == arch_handle(id_int));
        assert!(poly.is_some(), "no polyline prim for interior wall");
        let poly = *poly;
        let pts = list.points(poly);

        let min_x = pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - 0.1).abs() < 1e-6, "min_x {} != 0.1", min_x);
        assert!((max_x - 9.9).abs() < 1e-6, "max_x {} != 9.9", max_x);
    }

    #[test]
    fn plan_poché_envelope_is_the_joined_envelope() {
        let mut d = Drawing::new_metric();
        let t = 0.2;
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), t));
        d.arch.add_wall(wall(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), t));
        d.arch.add_wall(wall(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), t));
        d.arch.add_wall(wall(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), t));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        assert!((list.bounds.min.x - (-0.1)).abs() < 1e-6);
        assert!((list.bounds.min.y - (-0.1)).abs() < 1e-6);
        assert!((list.bounds.max.x - 10.1).abs() < 1e-6);
        assert!((list.bounds.max.y - 8.1).abs() < 1e-6);

        let tris_count = list.prims.iter().filter(|p| p.kind == Kind::Tris).count();
        assert_eq!(tris_count, 4, "expected 4 tris prims, got {}", tris_count);
    }
}
