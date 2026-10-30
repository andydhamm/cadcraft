//! Wall run detection: a wall in plain linework is two parallel face lines
//! `MIN_THICKNESS..MAX_THICKNESS` apart.

use std::collections::HashMap;

use cadcraft_geom::Vec2;

use super::layers::{LayerRole, layer_role, role_bonus, wall_face_allowed};
use super::{Evidence, Linework, MAX_PRIMITIVES, MAX_THICKNESS, MIN_THICKNESS, Prim, WallRun};

/// Shortest face segment, metres.
const MIN_FACE_LEN: f64 = 0.05;
/// Direction bucket width (radians): 0.25 degrees.
const ANGLE_TOL: f64 = 0.25 * std::f64::consts::PI / 180.0;
/// Offset tolerance when grouping collinear segments, metres.
const OFFSET_TOL: f64 = 0.002;
/// Longest stretch with no face line still bridged inside one run (a door or window).
const MAX_GAP: f64 = 2.6;
/// Cap on band pieces, so hostile input stays bounded.
const MAX_PIECES: usize = 200_000;

struct FaceSeg {
    item: usize,
    ang: f64,
    a: Vec2,
    b: Vec2,
    len: f64,
}

/// A line carrying face segments: signed offset along the direction's normal.
struct FaceLine {
    dir: usize,
    offset: f64,
    intervals: Vec<(f64, f64)>,
    segs: Vec<(f64, f64, usize)>,
}

/// A candidate wall: the stretch where two face lines run side by side.
struct Piece {
    dir: usize,
    fa: usize,
    fb: usize,
    lo: f64,
    hi: f64,
    t: f64,
    c: f64,
    a_items: Vec<usize>,
    b_items: Vec<usize>,
    alive: bool,
}

fn unit(dir: usize, dirs: &[Vec2]) -> Vec2 {
    dirs.get(dir).copied().unwrap_or(Vec2::new(1.0, 0.0))
}

fn normal(d: Vec2) -> Vec2 {
    Vec2::new(-d.y, d.x)
}

fn intersect(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        let lo = x.0.max(y.0);
        let hi = x.1.min(y.1);
        if hi > lo {
            out.push((lo, hi));
        }
        if x.1 < y.1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn total(iv: &[(f64, f64)]) -> f64 {
    iv.iter().map(|(a, b)| b - a).sum()
}

/// Steps 1-2: face lines grouped by direction. Returns directions, face
/// lines (sorted by offset within each direction) and each direction's range.
fn face_lines(lw: &Linework) -> (Vec<Vec2>, Vec<FaceLine>, Vec<(usize, usize)>) {
    let mut segs: Vec<FaceSeg> = Vec::new();
    for (idx, it) in lw.items.iter().take(MAX_PRIMITIVES).enumerate() {
        let Prim::Seg { a, b } = it.prim else { continue };
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
            continue;
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        if !len.is_finite() || len < MIN_FACE_LEN || !wall_face_allowed(layer_role(&it.layer)) {
            continue;
        }
        let mut ang = dy.atan2(dx);
        if ang < 0.0 {
            ang += std::f64::consts::PI;
        }
        if ang >= std::f64::consts::PI - ANGLE_TOL {
            ang -= std::f64::consts::PI;
        }
        segs.push(FaceSeg { item: idx, ang, a, b, len });
    }
    segs.sort_by(|p, q| p.ang.total_cmp(&q.ang));

    let mut dirs = Vec::new();
    let mut faces = Vec::new();
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < segs.len() {
        let mut end = start + 1;
        while let (Some(prev), Some(next)) = (segs.get(end - 1), segs.get(end)) {
            if next.ang - prev.ang > ANGLE_TOL {
                break;
            }
            end += 1;
        }
        let cluster = segs.get(start..end).unwrap_or(&[]);
        start = end;
        let wsum: f64 = cluster.iter().map(|s| s.len).sum();
        if wsum <= 0.0 {
            continue;
        }
        let mean = cluster.iter().map(|s| s.ang * s.len).sum::<f64>() / wsum;
        let mut d = Vec2::new(mean.cos(), mean.sin());
        if d.x < -1e-9 || (d.x.abs() <= 1e-9 && d.y < 0.0) {
            d = Vec2::new(-d.x, -d.y);
        }
        let n = normal(d);
        let mut proj: Vec<(f64, f64, f64, usize)> = cluster
            .iter()
            .map(|s| {
                let p0 = s.a.x * d.x + s.a.y * d.y;
                let p1 = s.b.x * d.x + s.b.y * d.y;
                let off = ((s.a.x + s.b.x) * n.x + (s.a.y + s.b.y) * n.y) / 2.0;
                (off, p0.min(p1), p0.max(p1), s.item)
            })
            .collect();
        proj.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
        let dir_idx = dirs.len();
        dirs.push(d);
        let first_face = faces.len();
        let mut g = 0;
        while g < proj.len() {
            let base = proj.get(g).map_or(0.0, |p| p.0);
            let mut h = g;
            while proj.get(h).is_some_and(|p| p.0 - base <= OFFSET_TOL) {
                h += 1;
            }
            let group = proj.get(g..h).unwrap_or(&[]);
            g = h.max(g + 1);
            let offset = group.iter().map(|p| p.0).sum::<f64>() / group.len().max(1) as f64;
            let mut segs: Vec<(f64, f64, usize)> = group.iter().map(|p| (p.1, p.2, p.3)).collect();
            segs.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut intervals: Vec<(f64, f64)> = Vec::new();
            for &(lo, hi, _) in &segs {
                match intervals.last_mut() {
                    Some(last) if lo <= last.1 + OFFSET_TOL => last.1 = last.1.max(hi),
                    _ => intervals.push((lo, hi)),
                }
            }
            faces.push(FaceLine { dir: dir_idx, offset, intervals, segs });
        }
        ranges.push((first_face, faces.len()));
    }
    (dirs, faces, ranges)
}

/// Item indices of a face line's segments that overlap `lo..hi`.
fn items_in(f: &FaceLine, lo: f64, hi: f64) -> Vec<usize> {
    f.segs.iter().filter(|s| s.0 <= hi && s.1 >= lo).map(|s| s.2).collect()
}

/// Step 3: band pieces.
fn band_pieces(faces: &[FaceLine], ranges: &[(usize, usize)]) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for &(s, e) in ranges {
        for i in s..e {
            let Some(fi) = faces.get(i) else { continue };
            for j in (i + 1)..e {
                let Some(fj) = faces.get(j) else { break };
                let t = fj.offset - fi.offset;
                if t > MAX_THICKNESS + 1e-9 {
                    break;
                }
                if t < MIN_THICKNESS - 1e-9 {
                    continue;
                }
                let common = intersect(&fi.intervals, &fj.intervals);
                let sum = total(&common);
                if common.is_empty() {
                    continue;
                }
                let covered = (i + 1..j).any(|k| faces.get(k).is_some_and(|fk| total(&intersect(&common, &fk.intervals)) > 0.5 * sum));
                if covered {
                    continue;
                }
                let min_len = 0.2f64.max(1.5 * t);
                for &(lo, hi) in &common {
                    if hi - lo < min_len || pieces.len() >= MAX_PIECES {
                        continue;
                    }
                    pieces.push(Piece {
                        dir: fi.dir,
                        fa: i,
                        fb: j,
                        lo,
                        hi,
                        t,
                        c: (fi.offset + fj.offset) / 2.0,
                        a_items: items_in(fi, lo, hi),
                        b_items: items_in(fj, lo, hi),
                        alive: true,
                    });
                }
            }
        }
    }
    pieces
}

/// The three most common thicknesses (0.01 m bins weighted by piece length).
fn dominant_thicknesses(pieces: &[Piece]) -> Vec<f64> {
    let mut bins: HashMap<i64, f64> = HashMap::new();
    for p in pieces {
        *bins.entry((p.t / 0.01).round() as i64).or_insert(0.0) += p.hi - p.lo;
    }
    let mut v: Vec<(i64, f64)> = bins.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(3).map(|(b, _)| b as f64 * 0.01).collect()
}

fn dominant_dist(t: f64, dom: &[f64]) -> f64 {
    dom.iter().map(|d| (d - t).abs()).fold(f64::INFINITY, f64::min)
}

/// Step 4: does piece `a` beat piece `b`?
fn wins(a: &Piece, b: &Piece, dom: &[f64]) -> bool {
    let (la, lb) = (a.hi - a.lo, b.hi - b.lo);
    if la > lb * 1.25 {
        return true;
    }
    if lb > la * 1.25 {
        return false;
    }
    let (da, db) = (dominant_dist(a.t, dom), dominant_dist(b.t, dom));
    if (da - db).abs() > 1e-9 {
        return da < db;
    }
    a.t <= b.t
}

fn resolve_conflicts(pieces: &mut [Piece], face_count: usize, dom: &[f64]) {
    let mut by_face: Vec<Vec<usize>> = vec![Vec::new(); face_count];
    for (i, p) in pieces.iter().enumerate() {
        if let Some(l) = by_face.get_mut(p.fa) {
            l.push(i);
        }
        if let Some(l) = by_face.get_mut(p.fb) {
            l.push(i);
        }
    }
    for list in &mut by_face {
        list.sort_by(|&a, &b| {
            let lo = |i: usize| pieces.get(i).map_or(0.0, |p| p.lo);
            lo(a).total_cmp(&lo(b))
        });
        for x in 0..list.len() {
            for y in (x + 1)..list.len() {
                let (Some(&i), Some(&j)) = (list.get(x), list.get(y)) else { break };
                let (Some(pi), Some(pj)) = (pieces.get(i), pieces.get(j)) else { break };
                if pj.lo >= pi.hi - 1e-9 {
                    break;
                }
                if !pi.alive || !pj.alive || pi.hi.min(pj.hi) - pj.lo.max(pi.lo) <= 1e-9 {
                    continue;
                }
                let loser = if wins(pi, pj, dom) { j } else { i };
                if let Some(p) = pieces.get_mut(loser) {
                    p.alive = false;
                }
            }
        }
    }
}

/// A set of merged pieces on one line.
struct Merged {
    dir: usize,
    lo: f64,
    hi: f64,
    gaps: Vec<(f64, f64)>,
    members: Vec<usize>,
}

/// Step 5: merge collinear pieces of the same thickness.
fn merge_pieces(pieces: &[Piece]) -> Vec<Merged> {
    let mut order: Vec<usize> = (0..pieces.len()).filter(|&i| pieces.get(i).is_some_and(|p| p.alive)).collect();
    order.sort_by(|&a, &b| match (pieces.get(a), pieces.get(b)) {
        (Some(p), Some(q)) => p.dir.cmp(&q.dir).then(p.c.total_cmp(&q.c)).then(p.lo.total_cmp(&q.lo)),
        _ => std::cmp::Ordering::Equal,
    });
    // Clusters of (dir, c0, t0, members).
    let mut clusters: Vec<(usize, f64, f64, Vec<usize>)> = Vec::new();
    for &i in &order {
        let Some(p) = pieces.get(i) else { continue };
        let mut placed = false;
        for cl in clusters.iter_mut().rev() {
            if cl.0 != p.dir || p.c - cl.1 > 0.01 {
                break;
            }
            if (p.t - cl.2).abs() <= 0.1 * p.t.max(cl.2) {
                cl.3.push(i);
                placed = true;
                break;
            }
        }
        if !placed {
            clusters.push((p.dir, p.c, p.t, vec![i]));
        }
    }
    let mut out = Vec::new();
    for (dir, _, _, mut members) in clusters {
        members.sort_by(|&a, &b| {
            let lo = |i: usize| pieces.get(i).map_or(0.0, |p| p.lo);
            lo(a).total_cmp(&lo(b))
        });
        let mut cur: Option<Merged> = None;
        for i in members {
            let Some(p) = pieces.get(i) else { continue };
            match cur.as_mut() {
                Some(m) if p.lo <= m.hi + MAX_GAP => {
                    if p.lo > m.hi + 1e-6 {
                        m.gaps.push((m.hi, p.lo));
                    }
                    m.hi = m.hi.max(p.hi);
                    m.members.push(i);
                }
                _ => {
                    if let Some(m) = cur.take() {
                        out.push(m);
                    }
                    cur = Some(Merged { dir, lo: p.lo, hi: p.hi, gaps: Vec::new(), members: vec![i] });
                }
            }
        }
        out.extend(cur);
    }
    out
}

/// Short segments sorted by midpoint x, for end-cap lookups.
struct Caps {
    list: Vec<(f64, Vec2, Vec2, f64)>,
}

impl Caps {
    fn new(lw: &Linework) -> Self {
        let mut list = Vec::new();
        for it in lw.items.iter().take(MAX_PRIMITIVES) {
            let Prim::Seg { a, b } = it.prim else { continue };
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len.is_finite() && (MIN_THICKNESS * 0.9..=MAX_THICKNESS * 1.1).contains(&len) && wall_face_allowed(layer_role(&it.layer)) {
                list.push(((a.x + b.x) / 2.0, a, b, len));
            }
        }
        list.sort_by(|p, q| p.0.total_cmp(&q.0));
        Self { list }
    }

    /// A segment across the band at `end`, length within 10 % of `t`.
    fn closes(&self, end: Vec2, t: f64, dir: Vec2) -> bool {
        let from = self.list.partition_point(|p| p.0 < end.x - 0.03);
        self.list.iter().skip(from).take_while(|p| p.0 <= end.x + 0.03).any(|&(mx, a, b, len)| {
            let my = (a.y + b.y) / 2.0;
            let u = Vec2::new((b.x - a.x) / len, (b.y - a.y) / len);
            (len - t).abs() <= 0.1 * t && (mx - end.x).hypot(my - end.y) <= 0.03 && (u.x * dir.x + u.y * dir.y).abs() <= 0.2
        })
    }
}

/// Detect wall runs from linework.
pub fn wall_runs(lw: &Linework) -> Vec<WallRun> {
    let (dirs, faces, ranges) = face_lines(lw);
    let mut pieces = band_pieces(&faces, &ranges);
    let dom = dominant_thicknesses(&pieces);
    resolve_conflicts(&mut pieces, faces.len(), &dom);
    let merged = merge_pieces(&pieces);

    let caps = Caps::new(lw);
    let mut polys: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, it) in lw.items.iter().enumerate().take(MAX_PRIMITIVES) {
        if let Some(p) = it.poly {
            polys.entry(p).or_default().push(i);
        }
    }

    let mut runs = Vec::new();
    for m in merged {
        let d = unit(m.dir, &dirs);
        let n = normal(d);
        let wsum: f64 = m.members.iter().filter_map(|&i| pieces.get(i)).map(|p| p.hi - p.lo).sum();
        if wsum <= 0.0 {
            continue;
        }
        let wmean = |f: &dyn Fn(&Piece) -> f64| m.members.iter().filter_map(|&i| pieces.get(i)).map(|p| f(p) * (p.hi - p.lo)).sum::<f64>() / wsum;
        let t = wmean(&|p| p.t);
        let c = wmean(&|p| p.c);
        if m.hi - m.lo < 0.3f64.max(1.5 * t) {
            continue;
        }
        let at = |s: f64| Vec2::new(s * d.x + c * n.x, s * d.y + c * n.y);
        let (start, end) = (at(m.lo), at(m.hi));
        let gaps = m.gaps.iter().map(|&(a, b)| (a - m.lo, b - m.lo)).collect();

        let mut a_items: Vec<usize> = Vec::new();
        let mut b_items: Vec<usize> = Vec::new();
        for p in m.members.iter().filter_map(|&i| pieces.get(i)) {
            a_items.extend(&p.a_items);
            b_items.extend(&p.b_items);
        }
        a_items.sort_unstable();
        a_items.dedup();
        b_items.sort_unstable();
        b_items.dedup();
        let all: Vec<usize> = a_items.iter().chain(&b_items).copied().collect();
        let item_role = |i: &usize| lw.items.get(*i).map(|it| layer_role(&it.layer));
        let face_role = |items: &[usize]| {
            if items.iter().any(|i| item_role(i) == Some(LayerRole::Wall)) {
                LayerRole::Wall
            } else {
                items.first().and_then(item_role).unwrap_or(LayerRole::Other)
            }
        };

        let mut rules = vec!["face-pair".to_string()];
        let mut conf: f64 = 0.55;
        let (ra, rb) = (face_role(&a_items), face_role(&b_items));
        if role_bonus(ra, LayerRole::Wall) > 0.0 && role_bonus(rb, LayerRole::Wall) > 0.0 {
            conf += 0.15;
            rules.push("wall-layer".into());
        }
        if caps.closes(start, t, d) || caps.closes(end, t, d) {
            conf += 0.1;
            rules.push("end-cap".into());
        }
        if dominant_dist(t, &dom) <= 0.01 {
            conf += 0.1;
            rules.push("dominant-thickness".into());
        }
        // Furniture-sized closed box: both faces edges of one small polyline.
        let polys_of: Vec<Option<u32>> = all.iter().map(|&i| lw.items.get(i).and_then(|it| it.poly)).collect();
        if let Some(Some(pid)) = polys_of.first().copied()
            && polys_of.iter().all(|p| *p == Some(pid))
            && ra != LayerRole::Wall
            && rb != LayerRole::Wall
            && let Some(sides) = polys.get(&pid)
            && sides.len() >= 3
        {
            let small = sides.iter().filter_map(|&i| lw.items.get(i)).all(|it| match it.prim {
                Prim::Seg { a, b } => {
                    let l = (b.x - a.x).hypot(b.y - a.y);
                    let par = ((b.x - a.x) * d.y - (b.y - a.y) * d.x).abs() <= 0.3 * l;
                    par || l < 0.6
                }
                Prim::Arc { .. } => true,
            });
            if small {
                conf -= 0.35;
            }
        }

        let mut layers: Vec<String> = all.iter().filter_map(|&i| lw.items.get(i)).map(|it| it.layer.clone()).collect();
        layers.sort();
        layers.dedup();
        let mut handles: Vec<_> = all.iter().filter_map(|&i| lw.items.get(i)).map(|it| it.source).collect();
        handles.sort();
        handles.dedup();
        rules.sort();
        rules.dedup();
        runs.push(WallRun { start, end, thickness: t, gaps, confidence: conf.clamp(0.0, 1.0), evidence: Evidence { layers, handles, rules } });
    }
    runs.sort_by(|a, b| {
        a.start.x.total_cmp(&b.start.x).then(a.start.y.total_cmp(&b.start.y)).then(a.end.x.total_cmp(&b.end.x)).then(a.end.y.total_cmp(&b.end.y))
    });
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Item;
    use cadcraft_doc::Handle;

    fn seg(items: &mut Vec<Item>, a: (f64, f64), b: (f64, f64), layer: &str, poly: Option<u32>) {
        items.push(Item {
            prim: Prim::Seg { a: Vec2::new(a.0, a.1), b: Vec2::new(b.0, b.1) },
            layer: layer.to_string(),
            source: Handle(items.len() as u64 + 1),
            poly,
        });
    }

    fn rect(items: &mut Vec<Item>, x0: f64, y0: f64, x1: f64, y1: f64, layer: &str, poly: Option<u32>) {
        seg(items, (x0, y0), (x1, y0), layer, poly);
        seg(items, (x1, y0), (x1, y1), layer, poly);
        seg(items, (x1, y1), (x0, y1), layer, poly);
        seg(items, (x0, y1), (x0, y0), layer, poly);
    }

    fn lw(items: Vec<Item>) -> Linework {
        Linework { items, labels: Vec::new(), entities: 0 }
    }

    #[test]
    fn apartment_faces_give_six_wall_runs_merged_across_door_gaps() {
        let mut it = Vec::new();
        rect(&mut it, 0.0, 0.0, 12.0, 8.0, "A-WALL", Some(1));
        rect(&mut it, 0.2, 0.2, 11.8, 7.8, "A-WALL", Some(2));
        rect(&mut it, 7.0, 0.2, 7.1, 1.5, "A-WALL", Some(3));
        rect(&mut it, 7.0, 2.4, 7.1, 7.8, "A-WALL", Some(4));
        rect(&mut it, 7.1, 5.0, 9.0, 5.1, "A-WALL", Some(5));
        rect(&mut it, 9.8, 5.0, 11.8, 5.1, "A-WALL", Some(6));
        let runs = wall_runs(&lw(it));
        assert_eq!(runs.len(), 6, "{runs:?}");
        let mut ts: Vec<f64> = runs.iter().map(|r| (r.thickness * 100.0).round() / 100.0).collect();
        ts.sort_by(f64::total_cmp);
        assert_eq!(ts, vec![0.1, 0.1, 0.2, 0.2, 0.2, 0.2]);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let with_gap = |g: (f64, f64)| runs.iter().any(|r| r.gaps.len() == 1 && near(r.gaps[0].0, g.0) && near(r.gaps[0].1, g.1));
        assert!(with_gap((1.3, 2.2)), "{runs:?}");
        assert!(with_gap((1.9, 2.7)), "{runs:?}");
    }

    #[test]
    fn two_partitions_with_a_closet_between_stay_two_walls() {
        let mut it = Vec::new();
        rect(&mut it, 0.0, 0.0, 5.0, 0.1, "A-WALL", None);
        rect(&mut it, 0.0, 0.3, 5.0, 0.4, "A-WALL", None);
        let runs = wall_runs(&lw(it));
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert!(runs.iter().all(|r| (r.thickness - 0.1).abs() < 1e-6));
    }

    #[test]
    fn a_glass_line_on_the_centre_does_not_split_the_wall() {
        let mut it = Vec::new();
        rect(&mut it, 0.0, 0.0, 4.0, 0.2, "0", None);
        seg(&mut it, (1.4, 0.1), (2.6, 0.1), "0", None);
        let runs = wall_runs(&lw(it));
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert!((runs[0].thickness - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_small_box_is_not_a_confident_wall() {
        let mut it = Vec::new();
        rect(&mut it, 0.0, 0.0, 0.45, 0.45, "0", Some(1));
        let runs = wall_runs(&lw(it));
        assert!(runs.iter().all(|r| r.confidence <= 0.5), "{runs:?}");
    }

    #[test]
    fn lines_too_far_apart_are_not_a_wall() {
        let mut it = Vec::new();
        seg(&mut it, (0.0, 0.0), (5.0, 0.0), "0", None);
        seg(&mut it, (0.0, 0.8), (5.0, 0.8), "0", None);
        assert!(wall_runs(&lw(it)).is_empty());
    }

    #[test]
    fn random_segments_never_panic() {
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let mut it = Vec::new();
        for _ in 0..20_000 {
            let a = (next() * 50.0, next() * 50.0);
            let b = if next() < 0.5 { (a.0 + next() * 5.0, a.1) } else { (a.0 + next() * 5.0, a.1 + next() * 5.0) };
            seg(&mut it, a, b, "0", None);
        }
        seg(&mut it, (f64::NAN, 0.0), (1.0, f64::INFINITY), "0", None);
        let _ = wall_runs(&lw(it));
    }
}
