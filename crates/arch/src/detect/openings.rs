//! Opening detection on wall runs: doors from swing arcs and from sliding leaf lines, windows from
//! glass lines between jambs, and cased candidates from the wall gaps nothing else explains.
//!
//! A wall's band is its centre line +- thickness / 2. Every candidate is checked with
//! [`crate::opening::validate_placement`] on a model holding its wall alone; one the wall cannot
//! hold keeps at most 0.3 confidence. Of candidates whose rough spans overlap on one wall the more
//! confident wins, a door before a window before a cased opening on a tie.

use std::collections::HashMap;

use cadcraft_doc::arch::{Hinge, OpeningKind, Swing};
use cadcraft_doc::{ArchModel, Handle, Wall};
use cadcraft_geom::Vec2;

use super::layers::{LayerRole, layer_role};
use super::{CandKind, Evidence, Item, Linework, OpeningCand, Prim, WallRun};
use crate::opening::{ROUGH_ALLOWANCE, spans_overlap, validate_placement};

/// Swing radius range (m): the leaf width.
const SWING_RADIUS: (f64, f64) = (0.5, 1.3);
/// Swing sweep range (degrees, unsigned).
const SWING_SWEEP_DEG: (f64, f64) = (60.0, 100.0);
/// How far outside the band a hinge may sit (m).
const HINGE_SLACK: f64 = 0.05;
/// |cos| between a radius and the wall for the radius to be the closed leaf.
const ALONG_COS: f64 = 0.95;
/// Lines within this angle of the wall (or of its normal, for jambs) count as parallel (or across).
const PARALLEL_DEG: f64 = 1.0;
/// A glass line keeps this clear of the faces (m), so face and frame lines are not glass.
const FACE_CLEARANCE: f64 = 0.005;
/// Shortest glass span (m).
const MIN_GLASS: f64 = 0.3;
/// Shortest piece of a glass span (m).
const MIN_GLASS_PIECE: f64 = 0.1;
/// Glass pieces this close across the wall are collinear (m).
const COLLINEAR: f64 = 0.01;
/// Glass pieces this close along the wall touch (m).
const TOUCH: f64 = 1.0e-3;
/// A jamb within this of a span's end belongs to it (m).
const JAMB_REACH: f64 = 0.08;
/// A jamb reaches both faces within this (m).
const JAMB_FACE_TOL: f64 = 0.01;
/// Sliding leaves overlap each other by at least this (m).
const MIN_LEAF_OVERLAP: f64 = 0.2;
/// Sliding door width range (m).
const SLIDING_SPAN: (f64, f64) = (0.6, 3.0);
/// Gap widths that make a cased candidate (m).
const CASED_GAP: (f64, f64) = (0.6, 2.6);
/// Slack along the wall for lines at or just past its ends (m).
const END_SLACK: f64 = 0.05;

const SWING_CONFIDENCE: f64 = 0.8;
const SLIDING_CONFIDENCE: f64 = 0.7;
const GLASS_CONFIDENCE: f64 = 0.7;
const CASED_CONFIDENCE: f64 = 0.4;
const LAYER_BONUS: f64 = 0.1;
const BOTH_JAMBS_BONUS: f64 = 0.1;
const REFUSED_CAP: f64 = 0.3;

/// Wall height of the temporary model used to validate a candidate (m).
const CHECK_WALL_HEIGHT: f64 = 2.7;
/// (sill, height) a candidate is validated with.
const DOOR_SIZE: (f64, f64) = (0.0, 2.1);
const WINDOW_SIZE: (f64, f64) = (0.9, 1.2);
const CASED_SIZE: (f64, f64) = (0.0, 2.1);

/// Grid cell (m) of the item index.
const CELL: f64 = 1.0;
/// An item or a query spanning more cells than this per axis skips the grid.
const MAX_CELLS: i64 = 64;

/// Find openings in linework given walls.
pub fn find_openings(lw: &Linework, walls: &[WallRun]) -> Vec<OpeningCand> {
    let frames: Vec<Option<Frame>> = walls.iter().map(Frame::of).collect();
    if frames.iter().all(Option::is_none) {
        return Vec::new();
    }
    let index = Index::new(&lw.items);
    let mut used = vec![false; lw.items.len()];

    let mut cands = swing_doors(lw, &frames, &index, &mut used);
    cands.extend(sliding_doors(lw, walls, &frames, &index, &mut used));
    cands.extend(windows(lw, &frames, &index, &used));
    for c in &mut cands {
        validate(walls, c);
    }
    let mut kept = keep_best(cands);

    let mut cased = cased_gaps(walls, &frames, &kept);
    for c in &mut cased {
        validate(walls, c);
    }
    kept.extend(cased);
    let mut out = keep_best(kept);
    out.sort_by(|a, b| a.host.cmp(&b.host).then(a.offset.total_cmp(&b.offset)));
    out
}

/// A wall run's centre line as an origin, a direction, its normal, length and half thickness.
#[derive(Clone, Copy, Debug)]
struct Frame {
    start: Vec2,
    dir: Vec2,
    perp: Vec2,
    len: f64,
    half: f64,
}

impl Frame {
    fn of(w: &WallRun) -> Option<Frame> {
        if !w.start.is_finite() || !w.end.is_finite() || !w.thickness.is_finite() || w.thickness <= 0.0 {
            return None;
        }
        let len = w.start.dist(w.end);
        if !len.is_finite() || len < 1e-9 {
            return None;
        }
        let dir = (w.end - w.start) / len;
        Some(Frame { start: w.start, dir, perp: dir.perp(), len, half: w.thickness / 2.0 })
    }

    fn along(&self, p: Vec2) -> f64 {
        (p - self.start).dot(self.dir)
    }

    fn across(&self, p: Vec2) -> f64 {
        (p - self.start).dot(self.perp)
    }

    fn point(&self, t: f64) -> Vec2 {
        self.start + self.dir * t
    }

    fn holds(&self, t: f64) -> bool {
        t >= -END_SLACK && t <= self.len + END_SLACK
    }

    /// The bounding box of the band, grown by `grow`.
    fn bounds(&self, grow: f64) -> (Vec2, Vec2) {
        let r = self.half + grow;
        let end = self.point(self.len);
        let lo = self.start.min(end);
        let hi = self.start.max(end);
        (Vec2::new(lo.x - r, lo.y - r), Vec2::new(hi.x + r, hi.y + r))
    }
}

/// A line parallel to a wall, in the wall's frame: `lo..hi` along it, `off` across it.
#[derive(Clone, Copy, Debug)]
struct Run {
    item: usize,
    lo: f64,
    hi: f64,
    off: f64,
}

fn parallel_sin() -> f64 {
    PARALLEL_DEG.to_radians().sin()
}

/// `item` as a line parallel to the wall, or `None`.
fn parallel_run(f: &Frame, item: &Item, idx: usize) -> Option<Run> {
    let Prim::Seg { a, b } = item.prim else { return None };
    let d = b - a;
    let l = d.len();
    if !l.is_finite() || l < 1e-9 {
        return None;
    }
    if (d / l).cross(f.dir).abs() > parallel_sin() {
        return None;
    }
    let (ta, tb) = (f.along(a), f.along(b));
    let off = (f.across(a) + f.across(b)) / 2.0;
    if !ta.is_finite() || !tb.is_finite() || !off.is_finite() {
        return None;
    }
    Some(Run { item: idx, lo: ta.min(tb), hi: ta.max(tb), off })
}

/// Where `item` crosses the whole band (a jamb), as a distance along the wall.
fn jamb_at(f: &Frame, item: &Item) -> Option<f64> {
    let Prim::Seg { a, b } = item.prim else { return None };
    let d = b - a;
    let l = d.len();
    if !l.is_finite() || l < 1e-9 {
        return None;
    }
    if (d / l).dot(f.dir).abs() > parallel_sin() {
        return None;
    }
    let (oa, ob) = (f.across(a), f.across(b));
    if oa.min(ob) > -f.half + JAMB_FACE_TOL || oa.max(ob) < f.half - JAMB_FACE_TOL {
        return None;
    }
    let t = (f.along(a) + f.along(b)) / 2.0;
    (t.is_finite() && f.holds(t)).then_some(t)
}

fn role(item: &Item) -> LayerRole {
    layer_role(&item.layer)
}

/// Layers that never carry opening evidence.
fn ignored(r: LayerRole) -> bool {
    matches!(r, LayerRole::Annotation | LayerRole::Furniture)
}

fn evidence(items: &[&Item], rules: &[&str]) -> Evidence {
    let mut e = Evidence::default();
    for it in items {
        e.layers.push(it.layer.clone());
        e.handles.push(it.source);
    }
    e.rules.extend(rules.iter().map(|r| r.to_string()));
    tidy(&mut e);
    e
}

fn tidy(e: &mut Evidence) {
    e.layers.sort();
    e.layers.dedup();
    e.handles.sort();
    e.handles.dedup();
    e.rules.sort();
    e.rules.dedup();
}

fn add_rule(e: &mut Evidence, rule: &str) {
    e.rules.push(rule.to_string());
    tidy(e);
}

fn cand(kind: CandKind, host: usize, f: &Frame, (offset, width): (f64, f64), style: &str, confidence: f64, evidence: Evidence) -> OpeningCand {
    OpeningCand { evidence, ..OpeningCand::new(kind, host, offset, f.point(offset), width, style, confidence) }
}

/// (1) Swing doors: an arc hinged in a wall's band with one end radius along the wall (the closed
/// leaf). The door spans from the hinge along that radius by the arc's radius.
fn swing_doors(lw: &Linework, frames: &[Option<Frame>], index: &Index, used: &mut [bool]) -> Vec<OpeningCand> {
    let mut out = Vec::new();
    for item in &lw.items {
        let Prim::Arc { c, r, start, sweep } = item.prim else { continue };
        if !c.is_finite() || !r.is_finite() || !start.is_finite() || !sweep.is_finite() {
            continue;
        }
        let deg = sweep.abs().to_degrees();
        if r < SWING_RADIUS.0 || r > SWING_RADIUS.1 || deg < SWING_SWEEP_DEG.0 || deg > SWING_SWEEP_DEG.1 {
            continue;
        }
        let ends = [Vec2::from_angle(start), Vec2::from_angle(start + sweep)];

        // The wall whose band holds the hinge and runs along one end radius; nearest centre line wins.
        let mut best: Option<(usize, f64, Vec2, Vec2)> = None;
        for (host, f) in frames.iter().enumerate() {
            let Some(f) = f else { continue };
            let across = f.across(c).abs();
            if across > f.half + HINGE_SLACK || !f.holds(f.along(c)) {
                continue;
            }
            let cos = [ends[0].dot(f.dir), ends[1].dot(f.dir)];
            let k = if cos[0].abs() >= cos[1].abs() { 0 } else { 1 };
            if cos[k].abs() <= ALONG_COS {
                continue;
            }
            if best.is_none_or(|(_, a, _, _)| across < a) {
                best = Some((host, across, ends[k], ends[1 - k]));
            }
        }
        let Some((host, _, closed, open)) = best else { continue };
        let Some(f) = frames[host] else { continue };

        let offset = f.along(c + closed * (r / 2.0));
        let door_layer = role(item) == LayerRole::Door;
        let mut rules = vec!["swing-arc"];
        if door_layer {
            rules.push("door-layer");
        }
        let confidence = SWING_CONFIDENCE + if door_layer { LAYER_BONUS } else { 0.0 };
        let mut o = cand(CandKind::Door, host, &f, (offset, r), "hinged", confidence, evidence(&[item], &rules));
        o.hinge = Some(if closed.dot(f.dir) > 0.0 { Hinge::Left } else { Hinge::Right });
        o.swing = Some(if open.dot(f.perp) > 0.0 { Swing::In } else { Swing::Out });
        out.push(o);

        // The closed leaf, when drawn as a line from the hinge, is the door's and not glass.
        let tip = c + closed * r;
        let (lo, hi) = (c.min(tip), c.max(tip));
        index.query(Vec2::new(lo.x - 0.05, lo.y - 0.05), Vec2::new(hi.x + 0.05, hi.y + 0.05), |i| {
            if let Prim::Seg { a, b } = lw.items[i].prim {
                let near = |p: Vec2, q: Vec2| p.dist(q) <= 0.02 * r.max(1.0);
                if (near(a, c) && (b - c).dot(closed) > 0.9 * r) || (near(b, c) && (a - c).dot(closed) > 0.9 * r) {
                    used[i] = true;
                }
            }
        });
    }
    out
}

/// (2) Sliding doors: two lines parallel to a wall, on opposite sides of its centre line within the
/// band, overlapping each other by >= 0.2 m and together 0.6..3.0 m long, inside one of the wall's
/// gaps (or anywhere in the band on a Door layer). Without either, two leaves between jambs look
/// like double glazing, so a layerless sliding door drawn over a continuous wall is left to (3).
fn sliding_doors(lw: &Linework, walls: &[WallRun], frames: &[Option<Frame>], index: &Index, used: &mut [bool]) -> Vec<OpeningCand> {
    let mut out = Vec::new();
    for (host, f) in frames.iter().enumerate() {
        let Some(f) = f else { continue };
        let gaps = &walls[host].gaps;
        let (lo, hi) = f.bounds(0.0);
        let mut leaves: Vec<(Run, bool)> = Vec::new();
        index.query(lo, hi, |i| {
            let item = &lw.items[i];
            let r = role(item);
            if used[i] || ignored(r) {
                return;
            }
            let Some(run) = parallel_run(f, item, i) else { return };
            if run.off.abs() < 1e-6 || run.off.abs() > f.half + 1e-6 {
                return;
            }
            let in_gap = gaps.iter().any(|&(g0, g1)| run.lo >= g0 - END_SLACK && run.hi <= g1 + END_SLACK);
            if in_gap || r == LayerRole::Door {
                leaves.push((run, r == LayerRole::Door));
            }
        });
        leaves.sort_by(|a, b| a.0.lo.total_cmp(&b.0.lo).then(a.0.item.cmp(&b.0.item)));

        let mut taken = vec![false; leaves.len()];
        for i in 0..leaves.len() {
            if taken[i] || leaves[i].0.off > 0.0 {
                continue;
            }
            let a = leaves[i].0;
            let mut best: Option<(usize, f64)> = None;
            for j in 0..leaves.len() {
                let b = leaves[j].0;
                if taken[j] || b.off < 0.0 {
                    continue;
                }
                let overlap = a.hi.min(b.hi) - a.lo.max(b.lo);
                let span = a.hi.max(b.hi) - a.lo.min(b.lo);
                if overlap >= MIN_LEAF_OVERLAP && (SLIDING_SPAN.0..=SLIDING_SPAN.1).contains(&span) && best.is_none_or(|(_, o)| overlap > o) {
                    best = Some((j, overlap));
                }
            }
            let Some((j, _)) = best else { continue };
            taken[i] = true;
            taken[j] = true;
            let b = leaves[j].0;
            let (u0, u1) = (a.lo.min(b.lo), a.hi.max(b.hi));
            used[a.item] = true;
            used[b.item] = true;
            let ev = evidence(&[&lw.items[a.item], &lw.items[b.item]], &["leaf-lines"]);
            out.push(cand(CandKind::Door, host, f, ((u0 + u1) / 2.0, u1 - u0), "sliding", SLIDING_CONFIDENCE, ev));
        }
    }
    out
}

/// A group of collinear glass lines.
struct Glass {
    lo: f64,
    hi: f64,
    off: f64,
    pieces: Vec<Run>,
}

/// (3) Windows: lines parallel to a wall strictly inside its band (clear of the faces and of frame
/// lines drawn on them), grouped when collinear and touching; a group is a window when a jamb closes
/// either end or it lies on a Window layer.
fn windows(lw: &Linework, frames: &[Option<Frame>], index: &Index, used: &[bool]) -> Vec<OpeningCand> {
    let mut out = Vec::new();
    for (host, f) in frames.iter().enumerate() {
        let Some(f) = f else { continue };
        let (lo, hi) = f.bounds(0.0);
        let mut glass: Vec<Run> = Vec::new();
        let mut jambs: Vec<(f64, usize)> = Vec::new();
        index.query(lo, hi, |i| {
            let item = &lw.items[i];
            let r = role(item);
            if ignored(r) {
                return;
            }
            if let Some(t) = jamb_at(f, item) {
                jambs.push((t, i));
                return;
            }
            if used[i] || r == LayerRole::Door {
                return;
            }
            let Some(run) = parallel_run(f, item, i) else { return };
            if run.off.abs() < f.half - FACE_CLEARANCE && run.hi - run.lo >= MIN_GLASS_PIECE && f.holds(run.lo) && f.holds(run.hi) {
                glass.push(run);
            }
        });
        glass.sort_by(|a, b| a.off.total_cmp(&b.off).then(a.lo.total_cmp(&b.lo)).then(a.item.cmp(&b.item)));

        let mut groups: Vec<Glass> = Vec::new();
        for run in glass {
            let joined = groups.iter_mut().find(|g| (g.off - run.off).abs() <= COLLINEAR && run.lo <= g.hi + TOUCH && run.hi >= g.lo - TOUCH);
            match joined {
                Some(g) => {
                    g.lo = g.lo.min(run.lo);
                    g.hi = g.hi.max(run.hi);
                    g.pieces.push(run);
                }
                None => groups.push(Glass { lo: run.lo, hi: run.hi, off: run.off, pieces: vec![run] }),
            }
        }

        for g in groups {
            let width = g.hi - g.lo;
            if width < MIN_GLASS {
                continue;
            }
            let start_jamb = jambs.iter().find(|(t, _)| (t - g.lo).abs() <= JAMB_REACH);
            let end_jamb = jambs.iter().find(|(t, _)| (t - g.hi).abs() <= JAMB_REACH);
            let window_layer = g.pieces.iter().any(|p| role(&lw.items[p.item]) == LayerRole::Window);
            if start_jamb.is_none() && end_jamb.is_none() && !window_layer {
                continue;
            }
            let mut items: Vec<&Item> = g.pieces.iter().map(|p| &lw.items[p.item]).collect();
            items.extend(start_jamb.iter().chain(end_jamb.iter()).map(|(_, i)| &lw.items[*i]));
            let mut rules = vec!["glass-line"];
            let mut confidence = GLASS_CONFIDENCE;
            if start_jamb.is_some() || end_jamb.is_some() {
                rules.push("jamb");
            }
            if start_jamb.is_some() && end_jamb.is_some() {
                confidence += BOTH_JAMBS_BONUS;
            }
            if window_layer {
                rules.push("window-layer");
                confidence += LAYER_BONUS;
            }
            let style = if is_slider(&g) { "slider" } else { "fixed" };
            out.push(cand(CandKind::Window, host, f, ((g.lo + g.hi) / 2.0, width), style, confidence, evidence(&items, &rules)));
        }
    }
    out
}

/// Two halves meeting near the middle of their span.
fn is_slider(g: &Glass) -> bool {
    if g.pieces.len() != 2 {
        return false;
    }
    let (a, b) = (g.pieces[0], g.pieces[1]);
    let (first, second) = if a.lo <= b.lo { (a, b) } else { (b, a) };
    let width = g.hi - g.lo;
    let meet = (first.hi + second.lo) / 2.0;
    (second.lo - first.hi).abs() <= TOUCH.max(0.01 * width) && (meet - (g.lo + g.hi) / 2.0).abs() <= 0.1 * width
}

/// (4) Gaps of 0.6..2.6 m that no door or window overlaps: low-confidence cased candidates.
fn cased_gaps(walls: &[WallRun], frames: &[Option<Frame>], found: &[OpeningCand]) -> Vec<OpeningCand> {
    let mut out = Vec::new();
    for (host, f) in frames.iter().enumerate() {
        let Some(f) = f else { continue };
        for &(g0, g1) in &walls[host].gaps {
            let width = g1 - g0;
            if !width.is_finite() || width < CASED_GAP.0 || width > CASED_GAP.1 {
                continue;
            }
            let explained = found.iter().any(|o| {
                let (r0, r1) = rough(o);
                o.host == host && spans_overlap(g0, g1, r0, r1)
            });
            if !explained {
                let ev = evidence(&[], &["wall-gap"]);
                out.push(cand(CandKind::Cased, host, f, ((g0 + g1) / 2.0, width), "cased", CASED_CONFIDENCE, ev));
            }
        }
    }
    out
}

/// (5) A candidate its wall cannot hold keeps at most 0.3 confidence.
fn validate(walls: &[WallRun], c: &mut OpeningCand) {
    let Some(w) = walls.get(c.host) else { return };
    let mut model = ArchModel::default();
    let mut wall = Wall::new(w.start, w.end, w.thickness);
    wall.height = CHECK_WALL_HEIGHT;
    let id = model.push_wall(wall);
    let (kind, (sill, height)) = match c.kind {
        CandKind::Door => (OpeningKind::Door, DOOR_SIZE),
        CandKind::Window => (OpeningKind::Window, WINDOW_SIZE),
        CandKind::Cased => (OpeningKind::CasedOpening, CASED_SIZE),
    };
    if validate_placement(&model, id, c.offset, c.width, sill, height, kind, &c.style, None).is_err() {
        c.confidence = c.confidence.min(REFUSED_CAP);
        add_rule(&mut c.evidence, "placement-refused");
    }
}

fn rough(o: &OpeningCand) -> (f64, f64) {
    let half = (o.width + ROUGH_ALLOWANCE) / 2.0;
    (o.offset - half, o.offset + half)
}

fn rank(k: CandKind) -> u8 {
    match k {
        CandKind::Door => 0,
        CandKind::Window => 1,
        CandKind::Cased => 2,
    }
}

/// Of candidates whose rough spans overlap on one wall, keep the most confident (a door before a
/// window before a cased opening on a tie).
fn keep_best(mut cands: Vec<OpeningCand>) -> Vec<OpeningCand> {
    cands.retain(|c| c.offset.is_finite() && c.width.is_finite() && c.confidence.is_finite());
    cands.sort_by(|a, b| {
        b.confidence.total_cmp(&a.confidence).then(rank(a.kind).cmp(&rank(b.kind))).then(a.host.cmp(&b.host)).then(a.offset.total_cmp(&b.offset))
    });
    let mut kept: Vec<OpeningCand> = Vec::new();
    for c in cands {
        let (c0, c1) = rough(&c);
        if !kept.iter().any(|k| {
            let (k0, k1) = rough(k);
            k.host == c.host && spans_overlap(c0, c1, k0, k1)
        }) {
            kept.push(c);
        }
    }
    kept
}

/// A uniform grid over the items' bounding boxes. Items too large for it, or with non-finite
/// coordinates in their box, are checked by every query.
struct Index {
    cells: HashMap<(i64, i64), Vec<usize>>,
    wide: Vec<usize>,
    count: usize,
}

impl Index {
    fn new(items: &[Item]) -> Index {
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let mut wide = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let (lo, hi) = bbox(&item.prim);
            match cell_range(lo, hi) {
                Some((x0, y0, x1, y1)) => {
                    for x in x0..=x1 {
                        for y in y0..=y1 {
                            cells.entry((x, y)).or_default().push(i);
                        }
                    }
                }
                None => wide.push(i),
            }
        }
        Index { cells, wide, count: items.len() }
    }

    /// Calls `f` once for every item whose box may meet `lo..hi`.
    fn query(&self, lo: Vec2, hi: Vec2, f: impl FnMut(usize)) {
        let Some((x0, y0, x1, y1)) = cell_range(lo, hi) else {
            (0..self.count).for_each(f);
            return;
        };
        let mut hits: Vec<usize> = self.wide.clone();
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(v) = self.cells.get(&(x, y)) {
                    hits.extend_from_slice(v);
                }
            }
        }
        hits.sort_unstable();
        hits.dedup();
        hits.into_iter().for_each(f);
    }
}

fn bbox(p: &Prim) -> (Vec2, Vec2) {
    match *p {
        Prim::Seg { a, b } => (a.min(b), a.max(b)),
        Prim::Arc { c, r, .. } => {
            let r = r.abs();
            (Vec2::new(c.x - r, c.y - r), Vec2::new(c.x + r, c.y + r))
        }
    }
}

/// The grid cells covering `lo..hi`, or `None` when the box is non-finite or too large.
fn cell_range(lo: Vec2, hi: Vec2) -> Option<(i64, i64, i64, i64)> {
    if !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    let lim = (i64::MAX / 4) as f64;
    let c = |v: f64| (v / CELL).floor();
    let (x0, y0, x1, y1) = (c(lo.x), c(lo.y), c(hi.x), c(hi.y));
    if [x0, y0, x1, y1].iter().any(|v| v.abs() > lim) {
        return None;
    }
    let (x0, y0, x1, y1) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64);
    if x1 - x0 > MAX_CELLS || y1 - y0 > MAX_CELLS {
        return None;
    }
    Some((x0, y0, x1, y1))
}

#[cfg(test)]
mod tests {
    use super::super::DEFAULT_MIN_CONFIDENCE;
    use super::super::arcs::polyline_prims;
    use super::*;
    use cadcraft_doc::arch::Opening;

    fn v(p: (f64, f64)) -> Vec2 {
        Vec2::new(p.0, p.1)
    }

    fn seg(a: (f64, f64), b: (f64, f64), layer: &str) -> Item {
        Item::new(Prim::Seg { a: v(a), b: v(b) }, layer, Handle(1), None)
    }

    /// A DXF ARC: counter-clockwise from `a0` to `a1` degrees.
    fn arc(c: (f64, f64), r: f64, a0: f64, a1: f64, layer: &str) -> Item {
        let sweep = (a1 - a0).rem_euclid(360.0);
        Item::new(Prim::Arc { c: v(c), r, start: a0.to_radians(), sweep: sweep.to_radians() }, layer, Handle(2), None)
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64, layer: &str) -> Vec<Item> {
        let p = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        (0..4).map(|i| Item::new(Prim::Seg { a: v(p[i]), b: v(p[(i + 1) % 4]) }, layer, Handle(3), Some(7))).collect()
    }

    fn run(a: (f64, f64), b: (f64, f64), t: f64, gaps: &[(f64, f64)]) -> WallRun {
        WallRun { start: v(a), end: v(b), thickness: t, gaps: gaps.to_vec(), confidence: 1.0, ..Default::default() }
    }

    fn lw(items: Vec<Item>) -> Linework {
        Linework { items, ..Default::default() }
    }

    fn near(p: Vec2, q: (f64, f64)) -> bool {
        p.dist(v(q)) < 1e-6
    }

    /// examples/apartment.dxf in metres: walls W1..W6, their face lines, two doors, five windows.
    fn apartment() -> (Linework, Vec<WallRun>) {
        let walls = vec![
            run((0.1, 0.1), (11.9, 0.1), 0.2, &[]),
            run((11.9, 0.1), (11.9, 7.9), 0.2, &[]),
            run((11.9, 7.9), (0.1, 7.9), 0.2, &[]),
            run((0.1, 7.9), (0.1, 0.1), 0.2, &[]),
            run((7.05, 0.1), (7.05, 7.9), 0.1, &[(1.4, 2.3)]),
            run((7.05, 5.05), (11.9, 5.05), 0.1, &[(1.95, 2.75)]),
        ];
        let mut items = Vec::new();
        items.extend(rect(0.0, 0.0, 12.0, 8.0, "A-WALL"));
        items.extend(rect(0.2, 0.2, 11.8, 7.8, "A-WALL"));
        items.extend(rect(7.0, 0.2, 7.1, 1.5, "A-WALL"));
        items.extend(rect(7.0, 2.4, 7.1, 7.8, "A-WALL"));
        items.extend(rect(7.1, 5.0, 9.0, 5.1, "A-WALL"));
        items.extend(rect(9.8, 5.0, 11.8, 5.1, "A-WALL"));
        for (x0, y0, x1, y1, a, b) in [
            (1.5, 0.0, 3.5, 0.2, (1.5, 0.1), (3.5, 0.1)),
            (4.2, 0.0, 6.2, 0.2, (4.2, 0.1), (6.2, 0.1)),
            (8.6, 0.0, 10.6, 0.2, (8.6, 0.1), (10.6, 0.1)),
            (0.0, 4.3, 0.2, 6.3, (0.1, 4.3), (0.1, 6.3)),
            (11.8, 5.8, 12.0, 7.2, (11.9, 5.8), (11.9, 7.2)),
        ] {
            items.extend(rect(x0, y0, x1, y1, "A-GLAZ"));
            items.push(seg(a, b, "A-GLAZ"));
        }
        items.push(seg((7.1, 1.5), (8.0, 1.5), "A-DOOR"));
        items.push(arc((7.1, 1.5), 0.9, 0.0, 90.0, "A-DOOR"));
        items.push(seg((9.0, 5.0), (9.0, 4.2), "A-DOOR"));
        items.push(arc((9.0, 5.0), 0.8, 270.0, 0.0, "A-DOOR"));
        (lw(items), walls)
    }

    fn accepted(c: &[OpeningCand]) -> Vec<&OpeningCand> {
        c.iter().filter(|o| o.confidence >= DEFAULT_MIN_CONFIDENCE).collect()
    }

    #[test]
    fn apartment_doors_are_found_from_their_swing_arcs() {
        let (lw, walls) = apartment();
        let found = find_openings(&lw, &walls);
        let doors: Vec<&OpeningCand> = accepted(&found).into_iter().filter(|o| o.kind == CandKind::Door).collect();
        assert_eq!(doors.len(), 2, "{found:#?}");

        let d1 = doors.iter().find(|o| o.host == 4).expect("door on W5");
        assert!(near(d1.center, (7.05, 1.95)), "{d1:?}");
        assert!((d1.width - 0.9).abs() < 1e-9 && (d1.offset - 1.85).abs() < 1e-9, "{d1:?}");
        assert_eq!((d1.hinge, d1.swing), (Some(Hinge::Left), Some(Swing::Out)));

        let d2 = doors.iter().find(|o| o.host == 5).expect("door on W6");
        assert!(near(d2.center, (9.4, 5.05)), "{d2:?}");
        assert!((d2.width - 0.8).abs() < 1e-9, "{d2:?}");
        assert_eq!((d2.hinge, d2.swing), (Some(Hinge::Left), Some(Swing::Out)));

        for d in [d1, d2] {
            assert_eq!(d.style, "hinged");
            assert!((d.confidence - 0.9).abs() < 1e-9, "{d:?}");
            assert_eq!(d.evidence.rules, ["door-layer", "swing-arc"]);
            assert_eq!(d.evidence.layers, ["A-DOOR"]);
        }
        assert!(found.iter().all(|o| o.kind != CandKind::Cased), "the door gaps are explained: {found:#?}");
    }

    #[test]
    fn apartment_windows_are_found_from_their_glass_lines() {
        let (lw, walls) = apartment();
        let found = find_openings(&lw, &walls);
        let wins: Vec<&OpeningCand> = accepted(&found).into_iter().filter(|o| o.kind == CandKind::Window).collect();
        assert_eq!(wins.len(), 5, "{found:#?}");
        for (host, c, w) in [(0, (2.5, 0.1), 2.0), (0, (5.2, 0.1), 2.0), (0, (9.6, 0.1), 2.0), (3, (0.1, 5.3), 2.0), (1, (11.9, 6.5), 1.4)] {
            let hit = wins.iter().find(|o| o.host == host && near(o.center, c)).unwrap_or_else(|| panic!("no window at {c:?}: {wins:#?}"));
            assert!((hit.width - w).abs() < 1e-9, "{hit:?}");
            assert_eq!(hit.style, "fixed");
            assert!((hit.confidence - 0.9).abs() < 1e-9, "{hit:?}");
            assert_eq!(hit.evidence.rules, ["glass-line", "jamb", "window-layer"]);
        }
        assert_eq!(accepted(&found).len(), 7, "two doors and five windows only: {found:#?}");
    }

    #[test]
    fn our_own_door_symbol_is_found_at_its_centre() {
        let mut m = ArchModel::default();
        let host = m.push_wall(Wall::new(Vec2::ZERO, Vec2::new(6.0, 0.0), 0.2));
        let mut o = Opening::new(host, 3.0, 0.9, 2.1, OpeningKind::Door);
        o.style = "hinged".into();
        m.push_opening(o);
        let sym = crate::symbol::opening_symbol(&m, &m.openings[0]).expect("symbol");
        assert!(sym.polylines.iter().any(|p| p.len() == 17));

        let mut items = Vec::new();
        for poly in &sym.polylines {
            items.extend(polyline_prims(poly, false).into_iter().map(|p| Item::new(p, "0", Handle(9), None)));
        }
        assert!(items.iter().any(|i| matches!(i.prim, Prim::Arc { .. })), "the swing was not fitted to an arc");
        let (r0, r1) = (3.0 - 0.51, 3.0 + 0.51);
        items.push(seg((0.0, -0.1), (r0, -0.1), "0"));
        items.push(seg((0.0, 0.1), (r0, 0.1), "0"));
        items.push(seg((r1, -0.1), (6.0, -0.1), "0"));
        items.push(seg((r1, 0.1), (6.0, 0.1), "0"));
        let walls = [run((0.0, 0.0), (6.0, 0.0), 0.2, &[(r0, r1)])];

        let found = find_openings(&lw(items), &walls);
        assert_eq!(found.len(), 1, "{found:#?}");
        let d = &found[0];
        assert_eq!((d.kind, d.style.as_str()), (CandKind::Door, "hinged"));
        assert!((d.width - 0.9).abs() < 0.01 && (d.offset - 3.0).abs() < 0.01, "{d:?}");
        assert_eq!((d.hinge, d.swing), (Some(Hinge::Left), Some(Swing::In)));
        assert!(d.confidence >= DEFAULT_MIN_CONFIDENCE);
    }

    /// A 0.2 m wall along x with a window or door of finished span 2.0..3.2: its jambs and frame.
    fn wall_with_jambs(gap: bool) -> (Vec<Item>, Vec<WallRun>) {
        let (r0, r1) = (1.94, 3.26);
        let mut items = vec![seg((r0, -0.1), (r0, 0.1), "0"), seg((r1, -0.1), (r1, 0.1), "0")];
        if !gap {
            items.push(seg((r0, -0.1), (r1, -0.1), "0"));
            items.push(seg((r0, 0.1), (r1, 0.1), "0"));
        }
        let gaps: &[(f64, f64)] = if gap { &[(r0, r1)] } else { &[] };
        (items, vec![run((0.0, 0.0), (6.0, 0.0), 0.2, gaps)])
    }

    #[test]
    fn a_sliders_two_halves_are_one_window() {
        let (mut items, walls) = wall_with_jambs(false);
        items.push(seg((2.0, 0.0), (2.6, 0.0), "0"));
        items.push(seg((2.6, 0.0), (3.2, 0.0), "0"));
        let found = find_openings(&lw(items), &walls);
        assert_eq!(found.len(), 1, "{found:#?}");
        let w = &found[0];
        assert_eq!((w.kind, w.style.as_str()), (CandKind::Window, "slider"));
        assert!((w.width - 1.2).abs() < 1e-9 && (w.offset - 2.6).abs() < 1e-9, "{w:?}");
        assert!((w.confidence - 0.8).abs() < 1e-9, "jambs at both ends: {w:?}");
        assert_eq!(w.evidence.rules, ["glass-line", "jamb"]);
    }

    #[test]
    fn a_sliding_door_without_an_arc_is_found() {
        let (mut items, walls) = wall_with_jambs(true);
        items.push(seg((2.0, -0.03), (2.7, -0.03), "0"));
        items.push(seg((2.5, 0.03), (3.2, 0.03), "0"));
        let found = find_openings(&lw(items), &walls);
        assert_eq!(found.len(), 1, "{found:#?}");
        let d = &found[0];
        assert_eq!((d.kind, d.style.as_str()), (CandKind::Door, "sliding"));
        assert!((d.width - 1.2).abs() < 1e-9 && (d.offset - 2.6).abs() < 1e-9, "{d:?}");
        assert!((d.confidence - 0.7).abs() < 1e-9 && d.hinge.is_none() && d.swing.is_none(), "{d:?}");
        assert_eq!(d.evidence.rules, ["leaf-lines"]);
    }

    #[test]
    fn our_own_sliding_door_symbol_is_a_door_not_a_window() {
        let mut m = ArchModel::default();
        let host = m.push_wall(Wall::new(Vec2::ZERO, Vec2::new(6.0, 0.0), 0.2));
        let mut o = Opening::new(host, 3.0, 1.6, 2.1, OpeningKind::Door);
        o.style = "slider".into();
        m.push_opening(o);
        let sym = crate::symbol::opening_symbol(&m, &m.openings[0]).expect("symbol");
        let items: Vec<Item> = sym.polylines.iter().flat_map(|p| polyline_prims(p, false)).map(|p| Item::new(p, "0", Handle(4), None)).collect();
        let walls = [run((0.0, 0.0), (6.0, 0.0), 0.2, &[(3.0 - 0.86, 3.0 + 0.86)])];
        let found = find_openings(&lw(items), &walls);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!((found[0].kind, found[0].style.as_str()), (CandKind::Door, "sliding"));
        assert!((found[0].width - 1.6).abs() < 1e-9 && (found[0].offset - 3.0).abs() < 1e-9, "{:?}", found[0]);
    }

    #[test]
    fn an_arc_away_from_walls_is_not_a_door() {
        let walls = [run((0.0, 0.0), (6.0, 0.0), 0.2, &[])];
        let items = vec![arc((3.0, 3.0), 0.9, 0.0, 90.0, "A-DOOR"), seg((3.0, 3.0), (3.9, 3.0), "A-DOOR")];
        assert!(find_openings(&lw(items), &walls).is_empty());
    }

    #[test]
    fn an_empty_gap_is_a_low_confidence_cased_opening() {
        let walls = [run((0.0, 0.0), (6.0, 0.0), 0.2, &[(2.0, 3.0)])];
        let found = find_openings(&Linework::default(), &walls);
        assert_eq!(found.len(), 1, "{found:#?}");
        let c = &found[0];
        assert_eq!((c.kind, c.style.as_str()), (CandKind::Cased, "cased"));
        assert!((c.width - 1.0).abs() < 1e-9 && (c.offset - 2.5).abs() < 1e-9, "{c:?}");
        assert!(c.confidence < DEFAULT_MIN_CONFIDENCE, "{c:?}");
        assert_eq!(c.evidence.rules, ["wall-gap"]);
    }

    #[test]
    fn a_door_hanging_off_the_wall_end_is_refused() {
        let walls = [run((0.0, 0.0), (2.0, 0.0), 0.2, &[])];
        let items = vec![arc((1.6, 0.0), 0.9, 0.0, 90.0, "A-DOOR")];
        let found = find_openings(&lw(items), &walls);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].confidence <= 0.3 + 1e-12, "{:?}", found[0]);
        assert!(found[0].evidence.rules.iter().any(|r| r == "placement-refused"));
    }

    #[test]
    fn hostile_linework_never_panics() {
        let big = 1.0e300;
        let walls = [
            run((0.0, 0.0), (6.0, 0.0), 0.2, &[(f64::NAN, 2.0), (1.0, f64::INFINITY)]),
            run((0.0, 0.0), (0.0, 0.0), 0.2, &[]),
            run((-big, 0.0), (big, 0.0), 0.2, &[(0.0, 1.0)]),
            run((0.0, 0.0), (f64::NAN, 1.0), 0.2, &[]),
        ];
        let items = vec![
            arc((1.0, 0.0), f64::NAN, 0.0, 90.0, "0"),
            arc((f64::INFINITY, 0.0), 0.9, 0.0, 90.0, "0"),
            seg((-big, 0.0), (big, 0.0), "0"),
            seg((1.0, 0.0), (1.0, 0.0), "0"),
            seg((f64::NAN, 0.0), (1.0, 0.0), "0"),
            arc((1.0, 0.05), 0.9, 0.0, 90.0, "0"),
        ];
        let found = find_openings(&lw(items), &walls);
        assert!(found.iter().all(|o| o.offset.is_finite() && o.width.is_finite() && o.center.is_finite()));
    }
}
