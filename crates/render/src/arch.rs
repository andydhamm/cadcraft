use std::collections::BTreeMap;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::arch::RoofEdgeKind;
use cadcraft_doc::{ArchId, Drawing, Handle, Wall};
use cadcraft_fonts::{TextFont, TextParams, place};
use cadcraft_geom::Vec2;

use crate::{DPrim, DisplayList, Kind, Options};

pub use cadcraft_doc::arch::roof_handle;
use cadcraft_doc::arch::{railing_handle, stair_handle};
pub use cadcraft_doc::{is_arch_handle, opening_handle, room_handle};

/// Stable proxy handle for an architectural object: arch objects have no entity handle, so
/// selection and picking map back to the `ArchId` through this reserved range.
pub fn arch_handle(id: ArchId) -> Handle {
    cadcraft_doc::arch_handle(id)
}

/// Whether `build_arch` should skip linework drawn on `layer`.
///
/// A layer that exists and is off, frozen, or — while plotting — marked no-plot is skipped.
/// A missing layer is never skipped, so it draws as before.
fn arch_layer_skipped(d: &Drawing, layer: &str, plotting: bool) -> bool {
    d.layer(layer).map_or(false, |l| !l.on || l.frozen || (plotting && !l.plot))
}

/// The poché (2D cross-section) primitives for `d.arch`, in wall order. The caller merges
/// the returned list into the main one. `_opts` is unused but kept as the caller's hook.
///
/// A wall's poché (fill, outline, gap edges) is skipped when `A-WALL` is off or frozen
/// (and no-plot while plotting); an opening's symbol is skipped by its own layer's state.
/// A missing layer draws as before.
pub fn build_arch(d: &Drawing, _opts: &Options, plotting: bool) -> DisplayList {
    let mut list = DisplayList::default();
    let layer = d.layer("A-WALL").map(|l| l.color).unwrap_or(Color::Index(7));
    let ink = crate::ink(Color::ByLayer, layer, Color::Index(7));

    let shapes = cadcraft_arch::join::solve_joins(&d.arch);
    let solved: BTreeMap<ArchId, &Vec<Vec2>> = shapes.iter().map(|s| (s.id, &s.polygon)).collect();

    // Poché gaps from openings are only meaningful while the wall poché is drawn, so the
    // whole loop is skipped when the wall's layer is off or frozen (or no-plot while plotting).
    if !arch_layer_skipped(d, "A-WALL", plotting) {
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
    }

    // Opening symbols: each opening emits its generated 2D plan symbol,
    // tagged with the opening's proxy handle so picking and selection work.
    for o in &d.arch.openings {
        if let Some(sym) = cadcraft_arch::symbol::opening_symbol(&d.arch, o) {
            if arch_layer_skipped(d, &sym.layer, plotting) {
                continue;
            }
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
    if !arch_layer_skipped(d, "A-ROOM", plotting) {
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
    }

    // Roof plan linework: each roof's non-eave edges (ridges, hips, valleys and gable ends)
    // as polylines on A-ROOF, tagged with the roof's proxy handle so picking resolves to the
    // roof. Eave edges are skipped — the poché already shows them along the walls.
    if !arch_layer_skipped(d, "A-ROOF", plotting) {
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
    }

    // Stair plan linework: the flight outline, one line per riser across the width, and the
    // well polygon when present, all tagged with the stair's proxy handle.
    if !arch_layer_skipped(d, "A-STAIR", plotting) {
        let layer_color = d.layer("A-STAIR").map(|l| l.color).unwrap_or(Color::Index(7));
        let stair_ink = crate::ink(Color::ByLayer, layer_color, Color::Index(7));
        for s in &d.arch.stairs {
            let handle = stair_handle(s.id);
            let Some(outline) = flight_outline(s) else { continue };
            push_polyline(&mut list, handle, &stair_ink, &outline);
            let (dir, side) = (Vec2::new(s.direction.cos(), s.direction.sin()), Vec2::new(-s.direction.sin(), s.direction.cos()));
            if s.tread_depth.is_finite() && s.tread_depth > 1e-6 {
                let mut n = 1usize;
                while n <= MAX_RISER_LINES && (n as f64) * s.tread_depth < s.run - 1e-9 {
                    let a = s.start + dir * (n as f64 * s.tread_depth);
                    let b = a + side * s.width;
                    push_polyline(&mut list, handle, &stair_ink, &[a, b]);
                    n += 1;
                }
            }
            if s.well.len() >= 3 && s.well.iter().all(|p| p.is_finite()) {
                let mut ring = s.well.clone();
                ring.push(s.well[0]);
                push_polyline(&mut list, handle, &stair_ink, &ring);
            }
        }
    }

    // Railing plan linework: each railing path as one polyline on A-RAIL.
    if !arch_layer_skipped(d, "A-RAIL", plotting) {
        let layer_color = d.layer("A-RAIL").map(|l| l.color).unwrap_or(Color::Index(7));
        let rail_ink = crate::ink(Color::ByLayer, layer_color, Color::Index(7));
        for r in &d.arch.railings {
            if r.path.len() < 2 || !r.path.iter().all(|p| p.is_finite()) {
                continue;
            }
            push_polyline(&mut list, railing_handle(r.id), &rail_ink, &r.path);
        }
    }

    list
}

/// Upper bound on riser lines drawn for one stair (hostile tread depths).
const MAX_RISER_LINES: usize = 512;

/// The plan rectangle of a stair's flight (start, direction, width, run), or `None` when any
/// dimension is non-finite or non-positive.
fn flight_outline(s: &cadcraft_doc::arch::Stair) -> Option<[Vec2; 5]> {
    let ok = s.start.is_finite() && s.direction.is_finite() && s.width.is_finite() && s.run.is_finite();
    if !ok || s.width <= 0.0 || s.run <= 0.0 {
        return None;
    }
    let dir = Vec2::new(s.direction.cos(), s.direction.sin());
    let side = Vec2::new(-s.direction.sin(), s.direction.cos());
    let p0 = s.start;
    let p1 = p0 + dir * s.run;
    let p2 = p1 + side * s.width;
    let p3 = p0 + side * s.width;
    Some([p0, p1, p2, p3, p0])
}

/// Append one polyline prim of `pts` tagged `handle`.
fn push_polyline(list: &mut DisplayList, handle: Handle, ink: &crate::Ink, pts: &[Vec2]) {
    let start = u32::try_from(list.verts.len()).unwrap_or(u32::MAX);
    for &p in pts {
        list.bounds.add(p);
        list.verts.push(p);
    }
    list.prims.push(DPrim {
        handle,
        color: ink.rgb,
        aci7: ink.aci7,
        lw: 0.0,
        transparency: 0,
        kind: Kind::Polyline,
        start,
        len: u32::try_from(pts.len()).unwrap_or(u32::MAX),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_color::Rgb;
    use cadcraft_doc::arch::{Opening, OpeningKind, Roof, RoofEdge, RoofEdgeKind, Room};
    use cadcraft_doc::{Drawing, Layer};

    fn wall(start: Vec2, end: Vec2, thickness: f64) -> Wall {
        Wall { start, end, thickness, height: 3.0, ..Wall::default() }
    }

    #[test]
    fn walls_draw_as_filled_poché_in_the_plan() {
        let mut d = Drawing::new_metric();
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

        assert_eq!(list.prims[0].handle, arch_handle(id0));
        assert_eq!(list.prims[1].handle, arch_handle(id0));
        assert_eq!(list.prims[2].handle, arch_handle(id1));
        assert_eq!(list.prims[3].handle, arch_handle(id1));
    }

    #[test]
    fn empty_arch_model_adds_no_prims() {
        let d = Drawing::new_metric();
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty());
        assert!(list.bounds.is_empty());
    }

    #[test]
    fn arch_poché_respects_the_layer_colour() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-WALL");
        if let Some(l) = d.layer_mut("A-WALL") {
            l.color = Color::Index(3);
        }
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        for p in &list.prims {
            assert_eq!(p.color, Rgb::from_u32(0x00ff00));
            assert!(!p.aci7);
        }

        let mut d = Drawing::new_metric();
        d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        for p in &list.prims {
            assert!(p.aci7);
        }
    }

    #[test]
    fn arch_handle_is_stable_and_unique_per_wall() {
        assert_eq!(arch_handle(ArchId(0)), Handle(0xFFFF_FFFF_0000_0000));
        assert_eq!(arch_handle(ArchId(7)).0, 0xFFFF_FFFF_0000_0007);
        assert_ne!(arch_handle(ArchId(0)), arch_handle(ArchId(1)));

        let mut d = Drawing::new_metric();
        let id0 = d.arch.add_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let id1 = d.arch.add_wall(wall(Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0), 0.2));
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

        let handle = opening_handle(oid);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!symbol_prims.is_empty(), "no prims tagged with the opening handle");
    }

    #[test]
    fn door_symbols_use_the_a_door_layer_colour() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.arch.push_opening(Opening::new(ArchId(0), 5.0, 1.0, 2.1, OpeningKind::Door));

        d.ensure_layer("A-DOOR");
        if let Some(l) = d.layer_mut("A-DOOR") {
            l.color = Color::True(Rgb::from_u32(0xff0000));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

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

        d.ensure_layer("A-GLAZ");
        if let Some(l) = d.layer_mut("A-GLAZ") {
            l.color = Color::True(Rgb::from_u32(0x0000ff));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
        let list = build_arch(&d, &Options::default(), false);

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
            ..Roof::default()
        });
        d.ensure_layer("A-ROOF");
        if let Some(l) = d.layer_mut("A-ROOF") {
            l.color = Color::True(Rgb::from_u32(0xff00ff));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

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
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

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
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        assert!(list.prims.iter().all(|p| p.handle != handle), "eave edges were drawn as roof linework");
        assert_eq!(list.prims.iter().filter(|p| p.handle == handle).count(), 0, "no prim may be emitted for eave edges");
    }

    /// Edges with NaN or infinite endpoints are skipped entirely, not drawn as degenerate prims.
    #[test]
    fn roof_edges_with_nan_endpoints_are_skipped() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(f64::NAN, 0.0), b: Vec2::new(10.0, 8.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Valley, a: Vec2::new(10.0, 0.0), b: Vec2::new(f64::INFINITY, 8.0), ..RoofEdge::default() },
            ],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 1, "only the finite ridge edge should draw, got {}", roof_prims.len());
    }

    /// Multiple roofs each produce prims tagged with their own distinct proxy handle.
    #[test]
    fn multiple_roofs_get_distinct_handles() {
        let mut d = Drawing::new_metric();
        let rid0 = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        let rid1 = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(0.0, 10.0), b: Vec2::new(10.0, 10.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let h0 = roof_handle(rid0);
        let h1 = roof_handle(rid1);
        let prims0: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h0).collect();
        let prims1: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h1).collect();
        assert_eq!(prims0.len(), 1, "roof 0 should have 1 prim");
        assert_eq!(prims1.len(), 1, "roof 1 should have 1 prim");
        assert_ne!(h0, h1, "roof handles must be distinct");
    }

    /// A zero-length roof edge (a == b) is drawn as a 2-point polyline at that location.
    #[test]
    fn zero_length_roof_edge_is_drawn() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(5.0, 5.0), b: Vec2::new(5.0, 5.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 1, "zero-length edge should still draw");
        let prim = &roof_prims[0];
        assert_eq!(prim.kind, Kind::Polyline);
        assert_eq!(prim.len, 2);
        let pts = list.points(prim);
        assert_eq!(pts.len(), 2);
        assert!((pts[0].x - 5.0).abs() < 1e-9 && (pts[0].y - 5.0).abs() < 1e-9);
        assert!((pts[1].x - 5.0).abs() < 1e-9 && (pts[1].y - 5.0).abs() < 1e-9);
    }

    /// Multiple collinear roof edges each draw as separate prims (no automatic merging).
    #[test]
    fn collinear_roof_edges_each_draw_separately() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(5.0, 0.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(5.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() },
            ],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 2, "two collinear edges should produce two prims");
    }

    /// Roof edges with very large coordinates render without overflow or precision loss.
    #[test]
    fn roof_edges_with_huge_coordinates() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(1e12, 1e12), b: Vec2::new(2e12, 2e12), ..RoofEdge::default() }],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 1, "huge-coordinate edge should draw");
        let prim = &roof_prims[0];
        let pts = list.points(prim);
        assert!((pts[0].x - 1e12).abs() < 1.0, "x0 precision: {}", pts[0].x);
        assert!((pts[0].y - 1e12).abs() < 1.0, "y0 precision: {}", pts[0].y);
        assert!((pts[1].x - 2e12).abs() < 1.0, "x1 precision: {}", pts[1].x);
        assert!((pts[1].y - 2e12).abs() < 1.0, "y1 precision: {}", pts[1].y);
    }

    /// Stair plan linework carries the stair handle: outline, risers and the well.
    #[test]
    fn stair_plan_linework_is_tagged() {
        use cadcraft_doc::arch::Stair;
        let mut d = Drawing::new_metric();
        let sid = d.arch.push_stair(Stair {
            run: 1.0,
            tread_depth: 0.25,
            width: 1.0,
            well: vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(2.0, 1.0)],
            ..Stair::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        let h = stair_handle(sid);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        // outline + 3 interior risers (0.25, 0.5, 0.75) + well
        assert_eq!(prims.len(), 5);
        assert!(prims.iter().all(|p| p.kind == Kind::Polyline));
        assert_eq!(prims[0].len, 5);
        assert_eq!(prims[4].len, 4);
    }

    /// Railing linework is one polyline and obeys the A-RAIL layer state.
    #[test]
    fn railing_linework_follows_layer_state() {
        use cadcraft_doc::arch::Railing;
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_railing(Railing { path: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(3.0, 2.0)], ..Railing::default() });
        d.touch_arch();
        let h = railing_handle(rid);
        let list = build_arch(&d, &Options::default(), false);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        assert_eq!(prims.len(), 1);
        assert_eq!(prims[0].len, 3);

        d.ensure_layer("A-RAIL");
        if let Some(l) = d.layer_mut("A-RAIL") {
            l.on = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.iter().all(|p| p.handle != h), "hidden layer still drew");
    }

    /// A roof with an empty edges list draws no prims.
    #[test]
    fn roof_without_edges_draws_nothing() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof { edges: vec![], ..Roof::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        assert!(list.prims.iter().all(|p| p.handle != handle), "roof with no edges drew prims");
    }

    /// When the A-ROOF layer does not exist, roof edges fall back to Color::Index(7) (aci7).
    #[test]
    fn roof_falls_back_to_index_7_when_layer_missing() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 1, "expected 1 roof prim");
        for p in &roof_prims {
            assert!(p.aci7, "roof prim should use aci7 when A-ROOF layer is missing");
        }
    }

    /// Roof handles are stable and unique per roof.
    #[test]
    fn roof_handle_is_stable_and_unique() {
        let h0 = roof_handle(ArchId(0));
        let h1 = roof_handle(ArchId(1));
        assert_ne!(h0, h1, "roof handles must be unique");
        assert_eq!(roof_handle(ArchId(0)), h0, "roof handle must be stable");

        let mut d = Drawing::new_metric();
        let rid0 = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        let rid1 = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(0.0, 10.0), b: Vec2::new(10.0, 10.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let prims0: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == roof_handle(rid0)).collect();
        let prims1: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == roof_handle(rid1)).collect();
        assert_eq!(prims0.len(), 1, "roof 0 should have 1 prim");
        assert_eq!(prims1.len(), 1, "roof 1 should have 1 prim");
    }

    /// Roof edges are drawn in the order they appear in the edges list.
    #[test]
    fn roof_edges_drawn_in_order() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_roof(Roof {
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(10.0, 0.0), b: Vec2::new(10.0, 8.0), ..RoofEdge::default() },
                RoofEdge { kind: RoofEdgeKind::Valley, a: Vec2::new(0.0, 8.0), b: Vec2::new(10.0, 8.0), ..RoofEdge::default() },
            ],
            ..Roof::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert_eq!(roof_prims.len(), 3, "expected 3 roof prims");

        let all_verts: Vec<Vec2> = roof_prims.iter().flat_map(|p| list.points(p).iter().copied()).collect();
        let expected =
            vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), Vec2::new(10.0, 8.0)];
        for (got, exp) in all_verts.iter().zip(expected.iter()) {
            assert!((got.x - exp.x).abs() < 1e-9 && (got.y - exp.y).abs() < 1e-9, "vertex mismatch");
        }
    }

    /// A room with an empty name uses the fallback label "r{id}".
    #[test]
    fn room_with_empty_name_uses_fallback_label() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "".into(),
            ..Room::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = room_handle(rid);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!label_prims.is_empty(), "no label prims for room with empty name");
    }

    /// Room label prims use the A-ROOM layer's colour when it exists.
    #[test]
    fn room_label_uses_a_room_layer_colour() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living".into(),
            ..Room::default()
        });
        if let Some(l) = d.layer_mut("A-ROOM") {
            l.color = Color::True(Rgb::from_u32(0x00ffff));
        }
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = room_handle(ArchId(0));
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!label_prims.is_empty(), "no label prims");
        for p in &label_prims {
            assert_eq!(p.color, Rgb::from_u32(0x00ffff), "label not on A-ROOM colour");
            assert!(!p.aci7, "A-ROOM is a true colour, not aci7");
        }
    }

    /// When the A-ROOM layer does not exist, room label prims fall back to Color::Index(7) (aci7).
    #[test]
    fn room_label_falls_back_to_index_7_when_layer_missing() {
        let mut d = Drawing::new_metric();
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living".into(),
            ..Room::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = room_handle(ArchId(0));
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!label_prims.is_empty(), "no label prims");
        for p in &label_prims {
            assert!(p.aci7, "label should use aci7 when A-ROOM layer is missing");
        }
    }

    /// A wall's poché (fill, outline, gap edges) is skipped when `A-WALL` is off or frozen;
    /// a screen build still draws it when the layer is on and not frozen.
    #[test]
    fn arch_walls_are_not_drawn_when_a_wall_is_off_or_frozen() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-WALL");
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 3.0, ..Wall::default() });
        d.touch_arch();

        // A-WALL on and not frozen: the wall draws.
        let list = build_arch(&d, &Options::default(), false);
        assert!(!list.prims.is_empty(), "wall should draw when A-WALL is on and not frozen");

        // A-WALL off: no arch prims.
        if let Some(l) = d.layer_mut("A-WALL") {
            l.on = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty(), "no prims when A-WALL is off");

        // A-WALL frozen: no arch prims.
        if let Some(l) = d.layer_mut("A-WALL") {
            l.on = true;
            l.frozen = true;
        }
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty(), "no prims when A-WALL is frozen");
    }

    /// A plot build skips the wall when `A-WALL` is set to no-plot; a screen build still draws it.
    #[test]
    fn arch_walls_are_not_plotted_when_a_wall_is_no_plot() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-WALL");
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 3.0, ..Wall::default() });
        d.touch_arch();

        // Screen build (plotting = false) still draws the wall.
        let list = build_arch(&d, &Options::default(), false);
        assert!(!list.prims.is_empty(), "wall should draw in a screen build");

        // Mark A-WALL no-plot; a plot build skips it.
        if let Some(l) = d.layer_mut("A-WALL") {
            l.plot = false;
        }
        let list = build_arch(&d, &Options::default(), true);
        assert!(list.prims.is_empty(), "no prims when A-WALL is no-plot during plotting");
    }

    /// Opening symbols (doors, windows) are skipped when their own layer is off or frozen.
    #[test]
    fn opening_symbols_respect_layer_state() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-DOOR");
        let wid = d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        let oid = d.arch.push_opening(Opening::new(wid, 5.0, 1.0, 2.1, OpeningKind::Door));
        d.touch_arch();

        // A-DOOR on: symbol draws.
        let list = build_arch(&d, &Options::default(), false);
        let handle = opening_handle(oid);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!symbol_prims.is_empty(), "door symbol should draw when A-DOOR is on");

        // A-DOOR off: symbol skipped.
        if let Some(l) = d.layer_mut("A-DOOR") {
            l.on = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(symbol_prims.is_empty(), "door symbol should be skipped when A-DOOR is off");

        // A-DOOR frozen: symbol skipped.
        if let Some(l) = d.layer_mut("A-DOOR") {
            l.on = true;
            l.frozen = true;
        }
        let list = build_arch(&d, &Options::default(), false);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(symbol_prims.is_empty(), "door symbol should be skipped when A-DOOR is frozen");

        // A-DOOR no-plot: skipped in plot build, drawn in screen build.
        if let Some(l) = d.layer_mut("A-DOOR") {
            l.on = true;
            l.frozen = false;
            l.plot = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!symbol_prims.is_empty(), "door symbol should draw in screen build when A-DOOR is no-plot");

        let list = build_arch(&d, &Options::default(), true);
        let symbol_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(symbol_prims.is_empty(), "door symbol should be skipped in plot build when A-DOOR is no-plot");
    }

    /// Roof edges are skipped when the A-ROOF layer is off, frozen, or no-plot during plotting.
    #[test]
    fn roof_edges_respect_layer_state() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOF");
        let rid = d.arch.push_roof(Roof {
            edges: vec![RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), ..RoofEdge::default() }],
            ..Roof::default()
        });
        d.touch_arch();

        // A-ROOF on: edges draw.
        let list = build_arch(&d, &Options::default(), false);
        let handle = roof_handle(rid);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!roof_prims.is_empty(), "roof edges should draw when A-ROOF is on");

        // A-ROOF off: edges skipped.
        if let Some(l) = d.layer_mut("A-ROOF") {
            l.on = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(roof_prims.is_empty(), "roof edges should be skipped when A-ROOF is off");

        // A-ROOF frozen: edges skipped.
        if let Some(l) = d.layer_mut("A-ROOF") {
            l.on = true;
            l.frozen = true;
        }
        let list = build_arch(&d, &Options::default(), false);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(roof_prims.is_empty(), "roof edges should be skipped when A-ROOF is frozen");

        // A-ROOF no-plot: skipped in plot build, drawn in screen build.
        if let Some(l) = d.layer_mut("A-ROOF") {
            l.on = true;
            l.frozen = false;
            l.plot = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!roof_prims.is_empty(), "roof edges should draw in screen build when A-ROOF is no-plot");

        let list = build_arch(&d, &Options::default(), true);
        let roof_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(roof_prims.is_empty(), "roof edges should be skipped in plot build when A-ROOF is no-plot");
    }

    /// Room labels are skipped when the A-ROOM layer is off, frozen, or no-plot during plotting.
    #[test]
    fn room_labels_respect_layer_state() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living".into(),
            ..Room::default()
        });
        d.touch_arch();

        // A-ROOM on: labels draw.
        let list = build_arch(&d, &Options::default(), false);
        let handle = room_handle(rid);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!label_prims.is_empty(), "room labels should draw when A-ROOM is on");

        // A-ROOM off: labels skipped.
        if let Some(l) = d.layer_mut("A-ROOM") {
            l.on = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(label_prims.is_empty(), "room labels should be skipped when A-ROOM is off");

        // A-ROOM frozen: labels skipped.
        if let Some(l) = d.layer_mut("A-ROOM") {
            l.on = true;
            l.frozen = true;
        }
        let list = build_arch(&d, &Options::default(), false);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(label_prims.is_empty(), "room labels should be skipped when A-ROOM is frozen");

        // A-ROOM no-plot: skipped in plot build, drawn in screen build.
        if let Some(l) = d.layer_mut("A-ROOM") {
            l.on = true;
            l.frozen = false;
            l.plot = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(!label_prims.is_empty(), "room labels should draw in screen build when A-ROOM is no-plot");

        let list = build_arch(&d, &Options::default(), true);
        let label_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        assert!(label_prims.is_empty(), "room labels should be skipped in plot build when A-ROOM is no-plot");
    }

    /// Two openings on the same wall produce four gap-edge polylines (two per gap).
    #[test]
    fn multiple_openings_create_multiple_poché_gaps() {
        let mut d = Drawing::new_metric();
        let wid = d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.arch.push_opening(Opening::new(wid, 3.0, 1.0, 2.1, OpeningKind::Door));
        d.arch.push_opening(Opening::new(wid, 7.0, 1.0, 2.1, OpeningKind::Door));
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let handle = arch_handle(wid);
        let wall_prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == handle).collect();
        // 1 tris + 1 outline + 4 gap edges (2 per opening) = 6
        assert_eq!(wall_prims.len(), 6, "expected 6 wall prims for 2 openings, got {}", wall_prims.len());
        let gap_edges: Vec<&DPrim> = wall_prims.iter().copied().filter(|p| p.kind == Kind::Polyline && p.len == 2).collect();
        assert_eq!(gap_edges.len(), 4, "expected 4 gap-edge polylines for 2 openings, got {}", gap_edges.len());
    }

    /// A wall with NaN in either endpoint is skipped entirely.
    #[test]
    fn wall_with_nan_endpoints_draws_nothing() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(f64::NAN, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty(), "wall with NaN start should draw nothing");

        let mut d2 = Drawing::new_metric();
        d2.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, f64::NAN), thickness: 0.2, height: 2.7, ..Wall::default() });
        d2.touch_arch();
        let list2 = build_arch(&d2, &Options::default(), false);
        assert!(list2.prims.is_empty(), "wall with NaN end should draw nothing");
    }

    /// A wall with zero thickness is skipped.
    #[test]
    fn wall_with_zero_thickness_draws_nothing() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.0, height: 2.7, ..Wall::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty(), "wall with zero thickness should draw nothing");
    }

    /// A wall with negative thickness is skipped.
    #[test]
    fn wall_with_negative_thickness_draws_nothing() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: -0.1, height: 2.7, ..Wall::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty(), "wall with negative thickness should draw nothing");
    }

    /// A wall whose start and end coincide is skipped.
    #[test]
    fn zero_length_wall_draws_nothing() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall { start: Vec2::new(5.0, 5.0), end: Vec2::new(5.0, 5.0), thickness: 0.2, height: 2.7, ..Wall::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.is_empty(), "zero-length wall should draw nothing");
    }

    /// Stair plan linework obeys the A-STAIR layer state (off, frozen, no-plot).
    #[test]
    fn stair_linework_follows_layer_state() {
        use cadcraft_doc::arch::Stair;
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-STAIR");
        let sid = d.arch.push_stair(Stair { run: 1.0, tread_depth: 0.25, width: 1.0, ..Stair::default() });
        d.touch_arch();
        let h = stair_handle(sid);

        // A-STAIR on: draws.
        let list = build_arch(&d, &Options::default(), false);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        assert!(!prims.is_empty(), "stair should draw when A-STAIR is on");

        // A-STAIR off: skipped.
        if let Some(l) = d.layer_mut("A-STAIR") {
            l.on = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.iter().all(|p| p.handle != h), "stair should be skipped when A-STAIR is off");

        // A-STAIR frozen: skipped.
        if let Some(l) = d.layer_mut("A-STAIR") {
            l.on = true;
            l.frozen = true;
        }
        let list = build_arch(&d, &Options::default(), false);
        assert!(list.prims.iter().all(|p| p.handle != h), "stair should be skipped when A-STAIR is frozen");

        // A-STAIR no-plot: drawn in screen build, skipped in plot build.
        if let Some(l) = d.layer_mut("A-STAIR") {
            l.on = true;
            l.frozen = false;
            l.plot = false;
        }
        let list = build_arch(&d, &Options::default(), false);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        assert!(!prims.is_empty(), "stair should draw in screen build when A-STAIR is no-plot");

        let list = build_arch(&d, &Options::default(), true);
        assert!(list.prims.iter().all(|p| p.handle != h), "stair should be skipped in plot build when A-STAIR is no-plot");
    }

    /// A stair with tread_depth larger than run draws only the outline, no riser lines.
    #[test]
    fn stair_with_large_tread_depth_draws_no_riser_lines() {
        use cadcraft_doc::arch::Stair;
        let mut d = Drawing::new_metric();
        let sid = d.arch.push_stair(Stair { run: 1.0, tread_depth: 2.0, width: 1.0, ..Stair::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        let h = stair_handle(sid);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        // Only the outline (5-point polyline), no riser lines.
        assert_eq!(prims.len(), 1, "expected 1 prim (outline only), got {}", prims.len());
        assert_eq!(prims[0].len, 5, "outline should have 5 vertices");
    }

    /// A stair with very small tread_depth caps riser lines at MAX_RISER_LINES (512).
    #[test]
    fn stair_with_tiny_tread_depth_caps_riser_lines() {
        use cadcraft_doc::arch::Stair;
        let mut d = Drawing::new_metric();
        let sid = d.arch.push_stair(Stair { run: 10.0, tread_depth: 0.001, width: 1.0, ..Stair::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        let h = stair_handle(sid);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        // 1 outline + 512 riser lines = 513 prims
        assert_eq!(prims.len(), 513, "expected 513 prims (1 outline + 512 risers), got {}", prims.len());
    }

    /// A stair with NaN in its well polygon draws the outline but not the well.
    #[test]
    fn stair_with_nan_in_well_draws_no_well() {
        use cadcraft_doc::arch::Stair;
        let mut d = Drawing::new_metric();
        let sid = d.arch.push_stair(Stair {
            run: 1.0,
            tread_depth: 0.25,
            width: 1.0,
            well: vec![Vec2::new(0.0, 0.0), Vec2::new(f64::NAN, 0.0), Vec2::new(2.0, 1.0)],
            ..Stair::default()
        });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        let h = stair_handle(sid);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        // outline + 3 riser lines, no well
        assert_eq!(prims.len(), 4, "expected 4 prims (outline + 3 risers, no well), got {}", prims.len());
    }

    /// Multiple stairs each produce prims tagged with their own distinct proxy handle.
    #[test]
    fn multiple_stairs_get_distinct_handles() {
        use cadcraft_doc::arch::Stair;
        let mut d = Drawing::new_metric();
        let sid0 = d.arch.push_stair(Stair { run: 1.0, tread_depth: 0.25, width: 1.0, ..Stair::default() });
        let sid1 = d.arch.push_stair(Stair { start: Vec2::new(5.0, 5.0), run: 1.0, tread_depth: 0.25, width: 1.0, ..Stair::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);

        let h0 = stair_handle(sid0);
        let h1 = stair_handle(sid1);
        let prims0: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h0).collect();
        let prims1: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h1).collect();
        assert!(!prims0.is_empty(), "stair 0 should have prims");
        assert!(!prims1.is_empty(), "stair 1 should have prims");
        assert_ne!(h0, h1, "stair handles must be distinct");
    }

    /// A railing with coincident points is still drawn (no automatic dedup).
    #[test]
    fn railing_with_coincident_points_is_drawn() {
        use cadcraft_doc::arch::Railing;
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_railing(Railing { path: vec![Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)], ..Railing::default() });
        d.touch_arch();
        let list = build_arch(&d, &Options::default(), false);
        let h = railing_handle(rid);
        let prims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h).collect();
        assert_eq!(prims.len(), 1, "expected 1 railing prim");
        assert_eq!(prims[0].len, 3, "railing should have 3 vertices (no dedup)");
    }
}
