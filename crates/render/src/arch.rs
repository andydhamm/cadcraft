use std::collections::BTreeMap;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{ArchId, Drawing, Handle, Wall};
use cadcraft_doc::arch::RoofEdgeKind;
use cadcraft_fonts::{TextFont, TextParams, place};
use cadcraft_geom::Vec2;

use crate::{DPrim, DisplayList, Kind, Options};

pub use cadcraft_doc::{is_arch_handle, opening_handle, room_handle, roof_handle};

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

        // Poché gaps: rectangular holes from openings, in wall coordinate space.
        let gaps = cadcraft_arch::symbol::poché_gaps(&d.arch, wall.id);
        let mut polys: Vec<Vec<Vec2>> = vec![ring.clone()];
        for &(gs, ge) in &gaps {
            let p1 = wall.start + dir * gs - perp * half;
            let p2 = wall.start + dir * gs + perp * half;
            let p3 = wall.start + dir * ge + perp * half;
            let p4 = wall.start + dir * ge - perp * half;
            if p1.is_finite() && p2.is_finite() && p3.is_finite() && p4.is_finite() {
                polys.push(vec![p1, p2, p3, p4]);
            }
        }

        // Filled poché with holes
        let tris = crate::triangulate_evenodd(&polys);
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

        // Gap edge outlines: one polyline per side of each cut.
        for &(gs, ge) in &gaps {
            let p1 = wall.start + dir * gs - perp * half;
            let p2 = wall.start + dir * gs + perp * half;
            if p1.is_finite() && p2.is_finite() {
                let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
                list.bounds.add(p1);
                list.bounds.add(p2);
                list.verts.push(p1);
                list.verts.push(p2);
                list.prims.push(DPrim {
                    handle: arch_handle(wall.id),
                    color: ink.rgb,
                    aci7: ink.aci7,
                    lw: 0.0,
                    transparency: 0,
                    kind: Kind::Polyline,
                    start,
                    len: 2,
                });
            }
            let p3 = wall.start + dir * ge - perp * half;
            let p4 = wall.start + dir * ge + perp * half;
            if p3.is_finite() && p4.is_finite() {
                let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
                list.bounds.add(p3);
                list.bounds.add(p4);
                list.verts.push(p3);
                list.verts.push(p4);
                list.prims.push(DPrim {
                    handle: arch_handle(wall.id),
                    color: ink.rgb,
                    aci7: ink.aci7,
                    lw: 0.0,
                    transparency: 0,
                    kind: Kind::Polyline,
                    start,
                    len: 2,
                });
            }
        }
    }

    // Opening symbols: each opening emits its generated 2D plan symbol,
    // tagged with the opening's proxy handle so picking and selection work.
    for o in &d.arch.openings {
        if let Some(sym) = cadcraft_arch::symbol::opening_symbol(&d.arch, o) {
            let all_finite = sym.polylines.iter().all(|p| p.iter().all(|pt| pt.is_finite()));
            if !all_finite {
                continue;
            }
            let sym_layer = d.layer(&sym.layer).map(|l| l.color).unwrap_or(Color::Index(7));
            let sym_ink = crate::ink(Color::ByLayer, sym_layer, Color::Index(7));
            for poly in &sym.polylines {
                if poly.len() < 2 {
                    continue;
                }
                let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
                for q in poly {
                    list.bounds.add(*q);
                    list.verts.push(*q);
                }
                list.prims.push(DPrim {
                    handle: opening_handle(o.id),
                    color: sym_ink.rgb,
                    aci7: sym_ink.aci7,
                    lw: 0.0,
                    transparency: 0,
                    kind: Kind::Polyline,
                    start,
                    len: u32::try_from(poly.len()).unwrap_or(u32::MAX),
                });
            }
        }
    }

    // Room labels: for each room, shape its name at the polygon centroid as 2D polyline
    // primitives tagged with the room's proxy handle on layer A-ROOM.
    for r in &d.arch.rooms {
        if r.polygon.len() < 3 {
            continue;
        }
        if !r.polygon.iter().all(|p| p.is_finite()) {
            continue;
        }
        let label_text = if r.name.is_empty() { format!("r{}", r.id.0) } else { r.name.clone() };
        let n = r.polygon.len() as f64;
        let cx = r.polygon.iter().map(|p| p.x).sum::<f64>() / n;
        let cy = r.polygon.iter().map(|p| p.y).sum::<f64>() / n;
        let label_pt = Vec2::new(cx, cy);
        let params = TextParams::centered(label_pt, 0.24, 0.0);
        let (sh, _) = place(&TextFont::Stroke, &label_text, &params);
        let room_layer = d.layer("A-ROOM").map(|l| l.color).unwrap_or(Color::Index(7));
        let room_ink = crate::ink(Color::ByLayer, room_layer, Color::Index(7));
        for stroke in &sh.strokes {
            if stroke.len() < 2 || !stroke.iter().all(|p| p.is_finite()) {
                continue;
            }
            let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
            for q in stroke {
                list.bounds.add(*q);
                list.verts.push(*q);
            }
            list.prims.push(DPrim {
                handle: room_handle(r.id),
                color: room_ink.rgb,
                aci7: room_ink.aci7,
                lw: 0.0,
                transparency: 0,
                kind: Kind::Polyline,
                start,
                len: u32::try_from(stroke.len()).unwrap_or(u32::MAX),
            });
        }
    }

    // Roof plan linework: each roof's non-eave edges (ridges, hips, valleys and gable ends)
    // as polylines on A-ROOF, tagged with the roof's proxy handle so picking resolves to the
    // roof. Eave edges are skipped — the poché already shows them along the walls.
    for roof in &d.arch.roofs {
        if roof.edges.is_empty() {
            continue;
        }
        let roof_layer = d.layer("A-ROOF").map(|l| l.color).unwrap_or(Color::Index(7));
        let roof_ink = crate::ink(Color::ByLayer, roof_layer, Color::Index(7));
        for edge in &roof.edges {
            if matches!(edge.kind, RoofEdgeKind::Eave) {
                continue;
            }
            if !edge.a.is_finite() || !edge.b.is_finite() {
                continue;
            }
            let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
            list.bounds.add(edge.a);
            list.bounds.add(edge.b);
            list.verts.push(edge.a);
            list.verts.push(edge.b);
            list.prims.push(DPrim {
                handle: roof_handle(roof.id),
                color: roof_ink.rgb,
                aci7: roof_ink.aci7,
                lw: 0.0,
                transparency: 0,
                kind: Kind::Polyline,
                start,
                len: 2,
            });
        }
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_color::Rgb;
    use cadcraft_doc::arch::{Opening, OpeningKind, Room, Roof, RoofEdge, RoofEdgeKind};
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
        let Some(poly) = poly else { return };
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
        let Some(poly) = poly else { return };
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

    #[test]
    fn openings_cut_gaps_in_the_wall_poché() {
        let mut d = Drawing::new_metric();
        let wid = d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.arch.push_opening(Opening::new(wid, 5.0, 1.0, 2.1, OpeningKind::Door));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = arch_handle(wid);
        let wall_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(wall_prims.len(), 4, "expected 4 wall prims, got {}", wall_prims.len());
        let gap_edges: Vec<&DPrim> = wall_prims.iter().copied().filter(|p| p.kind == Kind::Polyline && p.len == 2).collect();
        assert_eq!(gap_edges.len(), 2, "expected 2 gap-edge polylines, got {}", gap_edges.len());
    }

    #[test]
    fn opening_symbols_are_tagged_with_the_opening_handle() {
        let mut d = Drawing::new_metric();
        let wid = d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        let oid = d.arch.push_opening(Opening::new(wid, 5.0, 1.0, 2.1, OpeningKind::Door));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = opening_handle(oid);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!symbol_prims.is_empty(), "no prims tagged with the opening handle");
    }

    #[test]
    fn door_symbols_use_the_a_door_layer_colour() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.arch.push_opening(Opening::new(ArchId(0), 5.0, 1.0, 2.1, OpeningKind::Door));

        if let Some(l) = d.layer_mut("A-DOOR") {
            l.color = Color::True(Rgb::from_u32(0xff0000));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = opening_handle(ArchId(0));
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!symbol_prims.is_empty(), "no prims for the door opening");
        for p in &symbol_prims {
            assert_eq!(p.color, Rgb::from_u32(0xff0000));
        }
    }

    #[test]
    fn window_symbols_use_the_a_glaz_layer_colour() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.arch.push_opening(Opening::new(ArchId(0), 5.0, 1.0, 2.1, OpeningKind::Window));

        if let Some(l) = d.layer_mut("A-GLAZ") {
            l.color = Color::True(Rgb::from_u32(0x0000ff));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = opening_handle(ArchId(0));
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!symbol_prims.is_empty(), "no prims for the window opening");
        for p in &symbol_prims {
            assert_eq!(p.color, Rgb::from_u32(0x0000ff));
        }
    }

    #[test]
    fn a_missing_host_draws_nothing() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        // Opening whose host wall does not exist.
        d.arch.push_opening(Opening::new(ArchId(999), 5.0, 1.0, 2.1, OpeningKind::Door));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = opening_handle(ArchId(0));
        assert!(list.prims.iter().all(|p| p.handle != handle), "prims drawn for a missing-host opening");
    }

    /// The wall poché and the opening symbol must not share a handle when the wall and opening
    /// carry the same id.
    #[test]
    fn plan_symbol_prims_use_the_opening_handle() {
        let mut d = Drawing::new_metric();
        let wid = d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        let oid = d.arch.push_opening(Opening::new(wid, 5.0, 1.0, 2.1, OpeningKind::Door));
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        // In this fixture the wall and opening share id 0, so their proxy handles differ only
        // by range. Picking the opening must never land on the wall handle.
        assert_ne!(opening_handle(oid), arch_handle(wid), "opening and wall handles must differ");

        let open_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == opening_handle(oid)).collect();
        assert!(!open_prims.is_empty(), "no prims tagged with the opening handle");

        // One wall with one opening draws exactly four poché prims on the wall handle. If the
        // opening symbols mistakenly used the wall handle, this count would be larger.
        let wall_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == arch_handle(wid)).collect();
        assert_eq!(wall_prims.len(), 4, "wall poché prims should be untouched: {}", wall_prims.len());
    }

    #[test]
    fn walls_without_openings_are_unchanged() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 3.0, ..Wall::default() });
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
    }

    #[test]
    fn arch_prims_emits_room_label_prims() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living Room".into(),
            ..Room::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = room_handle(rid);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!label_prims.is_empty(), "no room label prims emitted for room {rid}");
        for p in &label_prims {
            assert_eq!(p.kind, Kind::Polyline, "room label prim kind wrong");
            assert!(p.len >= 2, "room label prim has fewer than 2 vertices: {}", p.len);
        }
    }

    /// A roof's plan edges (ridges/hips/valleys/gable ends) draw as polylines on the
    /// `A-ROOF` layer, using that layer's colour.
    #[test]
    fn roof_edges_draw_on_a_roof_layer() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(10.0, 0.0), b: Vec2::new(10.0, 8.0), ..RoofEdge::default() },
            ],
            ..Roof::default(),
        });
        if let Some(l) = d.layer_mut("A-ROOF") {
            l.color = Color::True(Rgb::from_u32(0xff00ff));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 2, "expected 2 roof prims (ridge+hip), got {}", roof_prims.len());
        for p in &roof_prims {
            assert_eq!(p.color, Rgb::from_u32(0xff00ff), "roof prim not on the A-ROOF colour");
            assert!(!p.aci7, "A-ROOF is a true colour, not aci7");
            assert_eq!(p.kind, Kind::Polyline, "roof edge should be a polyline");
            assert_eq!(p.len, 2, "roof edge should have 2 vertices");
        }
    }

    /// Every roof edge polyline carries the roof's proxy handle so picking/selection resolves to the roof.
    #[test]
    fn roof_plan_linework_is_tagged_with_the_roof_handle() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 4.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Valley, a: Vec2::new(4.0, 4.0), b: Vec2::new(4.0, 8.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::GableEnd, a: Vec2::new(0.0, 8.0), b: Vec2::new(4.0, 8.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Eave, a: Vec2::new(0.0, 0.0), b: Vec2::new(0.0, 8.0), ..RoofEdge::default() },
            ],
            ..Roof::default(),
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = roof_handle(rid);
        // All three non-eave edges produce prims on the roof handle.
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 3, "expected 3 non-eave roof prims, got {}", roof_prims.len());
        for p in &roof_prims {
            assert_eq!(p.handle, handle);
            assert_eq!(p.kind, Kind::Polyline);
        }
    }

    /// `Eave` edges contribute no prims — the poché already shows eaves along the walls, so
    /// they must not be drawn a second time by the roof linework.
    #[test]
    fn eave_edges_are_not_drawn_twice() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Eave, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Eave, a: Vec2::new(10.0, 0.0), b: Vec2::new(10.0, 8.0), ..RoofEdge::default() },
            ],
            ..Roof::default(),
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default());

        let handle = roof_handle(rid);
        assert!(list.prims.iter().all(|p| p.handle != handle), "eave edges were drawn as roof linework");
        assert_eq!(
            list.prims.iter().filter(|p| p.handle == handle).count(),
            0,
            "no prim may be emitted for eave edges"
        );
    }
}
