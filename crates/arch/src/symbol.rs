//! Generated 2D plan symbols for architectural openings.
//!
//! A [`Symbol`] is a set of world-space polylines (each a [`Vec<Vec2>`]).
//! The symbol is generated from [`Opening`] data together with its host wall
//! and is consumed at draw time by the plan renderer (`render::arch`) so that
//! generated symbols follow the one-source-of-truth rule.

use super::{ArchId, ArchModel};
use cadcraft_doc::arch::{Hinge, Opening, OpeningKind, Swing};
use cadcraft_geom::Vec2;

/// A world-space 2D plan symbol.
///
/// `polylines` is a list of polylines; each polyline is a list of world-space
/// `Vec2` vertices.  The symbol renders on the named [`Symbol::layer`].
pub struct Symbol {
    pub polylines: Vec<Vec<Vec2>>,
    pub layer: String,
}

const PI: f64 = std::f64::consts::PI;
const MAX_POLYLINES: usize = 64;
const MAX_POINTS: usize = 128;

/// Build the 2D plan symbol for `o` in world space.
///
/// Returns `None` when:
/// * the host wall is missing or degenerate (non-finite endpoints, zero/negative thickness);
/// * any generated vertex is non-finite;
/// * the output exceeds 64 polylines or 128 total vertices.
pub fn opening_symbol(model: &ArchModel, o: &Opening) -> Option<Symbol> {
    let w = model.wall(o.host)?;
    if !w.start.is_finite() || !w.end.is_finite() || w.thickness <= 0.0 || !w.thickness.is_finite() {
        return None;
    }
    let dir = crate::wall::wall_dir(w)?;
    if !dir.is_finite() {
        return None;
    }
    let perp = dir.perp();
    let half = (w.thickness / 2.0).abs();
    if !half.is_finite() {
        return None;
    }

    let offset = if o.offset.is_finite() { o.offset } else { 0.0 };
    let width = if o.width.is_finite() && o.width > 0.0 { o.width } else { 0.9 };
    let rough_width = if o.rough_width.is_finite() && o.rough_width > 0.0 { o.rough_width } else { width + 0.12 };

    let rough_start = offset - (rough_width - width) / 2.0;
    let rough_end = rough_start + rough_width;
    let finished_start = offset - width / 2.0;
    let finished_end = finished_start + width;

    if !rough_start.is_finite() || !rough_end.is_finite() || !finished_start.is_finite() || !finished_end.is_finite() {
        return None;
    }

    let base = w.start;

    let wp = |t: f64, off: f64| -> Option<Vec2> {
        let p = base + dir * t + perp * off;
        if p.is_finite() { Some(p) } else { None }
    };

    let mut polys: Vec<Vec<Vec2>> = Vec::new();

    match o.kind {
        OpeningKind::Door => {
            let style_lower = o.style.to_ascii_lowercase();
            let is_sliding = style_lower.contains("slider") || style_lower.contains("bifold") || style_lower.contains("pocket");
            if is_sliding {
                if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                let leaf_offset = (half * 0.15).max(1e-4);
                if let (Some(a), Some(b)) = (wp(finished_start, -leaf_offset), wp(finished_end, -leaf_offset)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(finished_start, leaf_offset), wp(finished_end, leaf_offset)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            } else {
                if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }

                let hinge_t = match o.hinge {
                    Hinge::Left => finished_start,
                    Hinge::Right => finished_end,
                };
                let closed_t = match o.hinge {
                    Hinge::Left => finished_end,
                    Hinge::Right => finished_start,
                };
                let hinge_pt = wp(hinge_t, 0.0)?;
                let leaf_tip = wp(closed_t, 0.0)?;
                polys.push(vec![hinge_pt, leaf_tip]);

                let (start_ang, sweep) = match (o.hinge, o.swing) {
                    (Hinge::Left, Swing::In) => (0.0, PI / 2.0),
                    (Hinge::Left, Swing::Out) => (0.0, -PI / 2.0),
                    (Hinge::Right, Swing::In) => (PI, -PI / 2.0),
                    (Hinge::Right, Swing::Out) => (PI, PI / 2.0),
                };
                let mut arc: Vec<Vec2> = Vec::with_capacity(17);
                for i in 0..=16usize {
                    let t = i as f64 / 16.0;
                    let ang = start_ang + t * sweep;
                    let px = hinge_pt.x + dir.x * (width * ang.cos()) + perp.x * (width * ang.sin());
                    let py = hinge_pt.y + dir.y * (width * ang.cos()) + perp.y * (width * ang.sin());
                    let p = Vec2::new(px, py);
                    if !p.is_finite() {
                        return None;
                    }
                    arc.push(p);
                }
                polys.push(arc);
            }
        }
        OpeningKind::Window => {
            if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_start, half), wp(rough_end, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_end, -half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            let style_lower = o.style.to_ascii_lowercase();
            let is_slider = style_lower.contains("slider") || style_lower.contains("casement");
            if is_slider {
                let mid = finished_start + width / 2.0;
                if let (Some(a), Some(b)) = (wp(finished_start, 0.0), wp(mid, 0.0)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(mid, 0.0), wp(finished_end, 0.0)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            } else {
                if let (Some(a), Some(b)) = (wp(finished_start, 0.0), wp(finished_end, 0.0)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            }
        }
        OpeningKind::GarageDoor => {
            let n = 4usize;
            for i in 0..n {
                let frac = if n == 1 { 0.0 } else { 2.0 * i as f64 / (n as f64 - 1.0) - 1.0 };
                let off = frac * half;
                if let (Some(a), Some(b)) = (wp(rough_start, off), wp(rough_end, off)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            }
        }
        OpeningKind::CasedOpening => {
            if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
        }
    }

    if polys.len() > MAX_POLYLINES {
        return None;
    }
    let total_pts: usize = polys.iter().map(|p| p.len()).sum();
    if total_pts > MAX_POINTS {
        return None;
    }
    for poly in &polys {
        for p in poly {
            if !p.is_finite() {
                return None;
            }
        }
    }

    Some(Symbol { polylines: polys, layer: o.layer.clone() })
}

/// Merged, sorted rough (cut) spans of every opening on `host`.
///
/// Returns `(start, end)` pairs in metres from the wall start.  Overlapping
/// or touching rough spans are merged into a single interval.  Openings with
/// non-finite values are silently skipped.
pub fn poché_gaps(model: &ArchModel, host: ArchId) -> Vec<(f64, f64)> {
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for o in model.openings_on(host) {
        if !o.offset.is_finite() || !o.width.is_finite() || !o.rough_width.is_finite() {
            continue;
        }
        if o.width <= 0.0 || o.rough_width <= 0.0 {
            continue;
        }
        let rough_start = o.offset - (o.rough_width - o.width) / 2.0;
        let rough_end = rough_start + o.rough_width;
        if rough_start.is_finite() && rough_end.is_finite() {
            intervals.push((rough_start, rough_end));
        }
    }
    if intervals.is_empty() {
        return Vec::new();
    }
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for &(s, e) in &intervals {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                if e > last.1 {
                    last.1 = e;
                }
            } else {
                merged.push((s, e));
            }
        } else {
            merged.push((s, e));
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArchModel, Wall};
    use cadcraft_geom::Vec2;

    fn test_wall(model: &mut ArchModel, start: Vec2, end: Vec2, thickness: f64) -> ArchId {
        model.push_wall(Wall { start, end, thickness, ..Wall::default() })
    }

    fn opening_on(model: &mut ArchModel, host: ArchId, offset: f64, width: f64, kind: OpeningKind, style: &str) -> usize {
        let mut o = Opening::new(host, offset, width, 2.0, kind);
        o.style = style.to_string();
        let idx = model.openings.len();
        model.push_opening(o);
        idx
    }

    fn assert_finite(s: &Symbol) {
        for poly in &s.polylines {
            for p in poly {
                assert!(p.is_finite(), "symbol has a non-finite vertex");
            }
        }
    }

    #[test]
    fn hinged_door_symbol_has_a_sampled_swing_arc() {
        let mut m = ArchModel::default();
        let h = test_wall(&mut m, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        let idx = opening_on(&mut m, h, 3.0, 0.9, OpeningKind::Door, "hinged");
        let o = &m.openings[idx];
        let s = opening_symbol(&m, o).unwrap();
        assert_finite(&s);
        assert_eq!(s.layer, "A-DOOR");
        assert_eq!(s.polylines.len(), 4);
        assert!(s.polylines.iter().any(|p| p.len() == 17), "no 17-point arc polyline found");

        let hinge = &s.polylines[2][0];
        let arc = &s.polylines[3];
        for pt in arc {
            let d = hinge.dist(*pt);
            assert!((d - o.width).abs() < 0.02, "arc radius {} != expected ~{} (width)", d, o.width);
        }
    }

    #[test]
    fn door_swing_flips_with_the_hinge_and_swing() {
        let mut m1 = ArchModel::default();
        let h1 = test_wall(&mut m1, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        let i1 = opening_on(&mut m1, h1, 3.0, 0.9, OpeningKind::Door, "hinged");
        m1.openings[i1].hinge = Hinge::Left;
        m1.openings[i1].swing = Swing::In;
        let s1 = opening_symbol(&m1, &m1.openings[i1]).unwrap();

        let mut m2 = ArchModel::default();
        let h2 = test_wall(&mut m2, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        let i2 = opening_on(&mut m2, h2, 3.0, 0.9, OpeningKind::Door, "hinged");
        m2.openings[i2].hinge = Hinge::Right;
        m2.openings[i2].swing = Swing::Out;
        let s2 = opening_symbol(&m2, &m2.openings[i2]).unwrap();

        assert_finite(&s1);
        assert_finite(&s2);

        let leaf1 = &s1.polylines[2];
        let leaf2 = &s2.polylines[2];
        assert!((leaf1[1].x - leaf1[0].x) > 0.3, "left-hinged leaf should run +x");
        assert!((leaf2[1].x - leaf2[0].x) < -0.3, "right-hinged leaf should run -x");

        let arc1_end = s1.polylines[3].last().unwrap();
        let arc2_end = s2.polylines[3].last().unwrap();
        assert!((arc1_end.y - arc2_end.y).abs() > 0.4, "swing flip not reflected: arc-end y diff = {:.6}", (arc1_end.y - arc2_end.y).abs());
    }

    #[test]
    fn sliding_door_symbol_uses_overlapping_leaf_lines() {
        let mut m = ArchModel::default();
        let h = test_wall(&mut m, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        let idx = opening_on(&mut m, h, 3.0, 1.0, OpeningKind::Door, "slider");
        let o = &m.openings[idx];
        let s = opening_symbol(&m, o).unwrap();
        assert_finite(&s);
        assert_eq!(s.polylines.len(), 4);
        assert!(s.polylines.iter().all(|p| p.len() == 2));

        for i in [2usize, 3] {
            let l = &s.polylines[i];
            let dx = (l[1].x - l[0].x).abs();
            let dy = (l[1].y - l[0].y).abs();
            assert!(dx > 0.5, "leaf line {} too short in x: {:.6}", i, dx);
            assert!(dx > dy * 10.0, "leaf line {} not parallel to wall: dx={} dy={}", i, dx, dy);
        }
    }

    #[test]
    fn window_symbol_has_frame_and_glass_lines() {
        let mut m = ArchModel::default();
        let h = test_wall(&mut m, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        let idx = opening_on(&mut m, h, 3.0, 1.0, OpeningKind::Window, "picture");
        let o = &m.openings[idx];
        let s = opening_symbol(&m, o).unwrap();
        assert_finite(&s);
        assert_eq!(s.polylines.len(), 5, "window (picture) should have 5 polylines");
        assert_eq!(s.layer, "A-GLAZ");
    }

    #[test]
    fn garage_and_cased_openings_produce_symbols() {
        let mut m = ArchModel::default();
        let h = test_wall(&mut m, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        let gi = opening_on(&mut m, h, 2.0, 2.0, OpeningKind::GarageDoor, "garage");
        let ci = opening_on(&mut m, h, 6.0, 0.5, OpeningKind::CasedOpening, "cased");

        let s_garage = opening_symbol(&m, &m.openings[gi]).unwrap();
        assert_finite(&s_garage);
        assert_eq!(s_garage.polylines.len(), 4, "garage should produce 4 parallel panel lines");
        assert!(s_garage.polylines.iter().all(|p| p.len() == 2));

        let s_cased = opening_symbol(&m, &m.openings[ci]).unwrap();
        assert_finite(&s_cased);
        assert_eq!(s_cased.polylines.len(), 2, "cased opening should produce 2 jamb lines only");
    }

    #[test]
    fn poché_gaps_merge_overlapping_spans() {
        let mut m = ArchModel::default();
        let h = test_wall(&mut m, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);

        opening_on(&mut m, h, 1.0, 1.0, OpeningKind::Window, "picture");
        opening_on(&mut m, h, 1.8, 1.0, OpeningKind::Window, "picture");
        opening_on(&mut m, h, 6.0, 1.0, OpeningKind::Window, "picture");

        let gaps = poché_gaps(&m, h);
        assert_eq!(gaps.len(), 2, "expected 2 merged gaps; got {} gaps: {:?}", gaps.len(), gaps);
        assert!((gaps[0].0 - 0.94).abs() < 1e-6, "gap[0].start = {}", gaps[0].0);
        assert!((gaps[0].1 - 2.86).abs() < 1e-6, "gap[0].end = {}", gaps[0].1);
        assert!((gaps[1].0 - 5.94).abs() < 1e-6, "gap[1].start = {}", gaps[1].0);
        assert!((gaps[1].1 - 7.06).abs() < 1e-6, "gap[1].end = {}", gaps[1].1);
        assert!(gaps[0].1 <= gaps[1].0, "merged gaps still overlap");
    }

    #[test]
    fn symbol_is_none_for_a_missing_host() {
        let mut m = ArchModel::default();
        let _ = test_wall(&mut m, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.2);
        opening_on(&mut m, ArchId(999), 0.0, 0.9, OpeningKind::Door, "hinged");
        let o = &m.openings[m.openings.len() - 1];
        let s = opening_symbol(&m, o);
        assert!(s.is_none(), "expected None for missing host wall");
    }
}

/// A world-space 2D plan symbol.
///
/// `polylines` is a list of polylines; each polyline is a list of world-space
/// `Vec2` vertices.  The symbol renders on the named [`Symbol::layer`].
pub struct Symbol {
    pub polylines: Vec<Vec<Vec2>>,
    pub layer: String,
}

const PI: f64 = std::f64::consts::PI;
const MAX_POLYLINES: usize = 64;
const MAX_POINTS: usize = 128;

/// Build the 2D plan symbol for `o` in world space.
///
/// Returns `None` when:
/// * the host wall is missing or degenerate (non-finite endpoints, zero/negative thickness);
/// * any generated vertex is non-finite;
/// * the output exceeds 64 polylines or 128 total vertices.
pub fn opening_symbol(model: &ArchModel, o: &Opening) -> Option<Symbol> {
    let w = model.wall(o.host)?;
    if !w.start.is_finite() || !w.end.is_finite() || w.thickness <= 0.0 || !w.thickness.is_finite() {
        return None;
    }
    let dir = crate::wall::wall_dir(w)?;
    if !dir.is_finite() {
        return None;
    }
    let perp = dir.perp();
    let half = (w.thickness / 2.0).abs();
    if !half.is_finite() {
        return None;
    }

    let offset = if o.offset.is_finite() { o.offset } else { 0.0 };
    let width = if o.width.is_finite() && o.width > 0.0 { o.width } else { 0.9 };
    let rough_width = if o.rough_width.is_finite() && o.rough_width > 0.0 { o.rough_width } else { width + 0.12 };

    let rough_start = offset - (rough_width - width) / 2.0;
    let rough_end = rough_start + rough_width;
    let finished_start = offset - width / 2.0;
    let finished_end = finished_start + width;

    if !rough_start.is_finite() || !rough_end.is_finite() || !finished_start.is_finite() || !finished_end.is_finite() {
        return None;
    }

    let base = w.start;

    let wp = |t: f64, off: f64| -> Option<Vec2> {
        let p = base + dir * t + perp * off;
        if p.is_finite() { Some(p) } else { None }
    };

    let mut polys: Vec<Vec<Vec2>> = Vec::new();

    match o.kind {
        OpeningKind::Door => {
            let style_lower = o.style.to_ascii_lowercase();
            let is_sliding = style_lower.contains("slider") || style_lower.contains("bifold") || style_lower.contains("pocket");
            if is_sliding {
                if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                let leaf_offset = (half * 0.15).max(1e-4);
                if let (Some(a), Some(b)) = (wp(finished_start, -leaf_offset), wp(finished_end, -leaf_offset)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(finished_start, leaf_offset), wp(finished_end, leaf_offset)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            } else {
                if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }

                let hinge_t = match o.hinge {
                    Hinge::Left => finished_start,
                    Hinge::Right => finished_end,
                };
                let closed_t = match o.hinge {
                    Hinge::Left => finished_end,
                    Hinge::Right => finished_start,
                };
                let hinge_pt = wp(hinge_t, 0.0)?;
                let leaf_tip = wp(closed_t, 0.0)?;
                polys.push(vec![hinge_pt, leaf_tip]);

                let (start_ang, sweep) = match (o.hinge, o.swing) {
                    (Hinge::Left, Swing::In) => (0.0, PI / 2.0),
                    (Hinge::Left, Swing::Out) => (0.0, -PI / 2.0),
                    (Hinge::Right, Swing::In) => (PI, -PI / 2.0),
                    (Hinge::Right, Swing::Out) => (PI, PI / 2.0),
                };
                let mut arc: Vec<Vec2> = Vec::with_capacity(17);
                for i in 0..=16usize {
                    let t = i as f64 / 16.0;
                    let ang = start_ang + t * sweep;
                    let px = hinge_pt.x + dir.x * (width * ang.cos()) + perp.x * (width * ang.sin());
                    let py = hinge_pt.y + dir.y * (width * ang.cos()) + perp.y * (width * ang.sin());
                    let p = Vec2::new(px, py);
                    if !p.is_finite() {
                        return None;
                    }
                    arc.push(p);
                }
                polys.push(arc);
            }
        }
        OpeningKind::Window => {
            if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_start, half), wp(rough_end, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_end, -half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            let style_lower = o.style.to_ascii_lowercase();
            let is_slider = style_lower.contains("slider") || style_lower.contains("casement");
            if is_slider {
                let mid = finished_start + width / 2.0;
                if let (Some(a), Some(b)) = (wp(finished_start, 0.0), wp(mid, 0.0)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
                if let (Some(a), Some(b)) = (wp(mid, 0.0), wp(finished_end, 0.0)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            } else {
                if let (Some(a), Some(b)) = (wp(finished_start, 0.0), wp(finished_end, 0.0)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            }
        }
        OpeningKind::GarageDoor => {
            let n = 4usize;
            for i in 0..n {
                let frac = if n == 1 { 0.0 } else { 2.0 * i as f64 / (n as f64 - 1.0) - 1.0 };
                let off = frac * half;
                if let (Some(a), Some(b)) = (wp(rough_start, off), wp(rough_end, off)) {
                    polys.push(vec![a, b]);
                } else {
                    return None;
                }
            }
        }
        OpeningKind::CasedOpening => {
            if let (Some(a), Some(b)) = (wp(rough_start, -half), wp(rough_start, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
            if let (Some(a), Some(b)) = (wp(rough_end, -half), wp(rough_end, half)) {
                polys.push(vec![a, b]);
            } else {
                return None;
            }
        }
    }

    if polys.len() > MAX_POLYLINES {
        return None;
    }
    let total_pts: usize = polys.iter().map(|p| p.len()).sum();
    if total_pts > MAX_POINTS {
        return None;
    }
    for poly in &polys {
        for p in poly {
            if !p.is_finite() {
                return None;
            }
        }
    }

    Some(Symbol { polylines: polys, layer: o.layer.clone() })
}

/// Merged, sorted rough (cut) spans of every opening on `host`.
///
/// Returns `(start, end)` pairs in metres from the wall start.  Overlapping
/// or touching rough spans are merged into a single interval.  Openings with
/// non-finite values are silently skipped.
pub fn poché_gaps(model: &ArchModel, host: ArchId) -> Vec<(f64, f64)> {
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for o in model.openings_on(host) {
        if !o.offset.is_finite() || !o.width.is_finite() || !o.rough_width.is_finite() {
            continue;
        }
        if o.width <= 0.0 || o.rough_width <= 0.0 {
            continue;
        }
        let rough_start = o.offset - (o.rough_width - o.width) / 2.0;
        let rough_end = rough_start + o.rough_width;
        if rough_start.is_finite() && rough_end.is_finite() {
            intervals.push((rough_start, rough_end));
        }
    }
    if intervals.is_empty() {
        return Vec::new();
    }
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for &(s, e) in &intervals {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                if e > last.1 {
                    last.1 = e;
                }
            } else {
                merged.push((s, e));
            }
        } else {
            merged.push((s, e));
        }
    }
    merged
}
