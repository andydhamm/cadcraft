//! Hidden-line removal for orthographic views of a [`Scene3d`] (phase 7: generated elevations and sections).
//!
//! Exact and deterministic, on the CPU. Every feature edge ([`crate::edges`], plus the view's silhouettes) is tested
//! against every triangle that can cover it on the sheet; a uniform grid over the sheet finds those. Where a triangle
//! covers part of an edge on the sheet, the depth difference between the edge and the triangle's plane is linear along
//! the edge, so the hidden part is one interval found by solving a linear inequality: no sampling, no depth buffer.
//! What is left of each edge after removing the union of its hidden intervals is drawn, and [`merge_collinear`] joins
//! one object's overlapping runs.
//!
//! Covering is closed: a point on a triangle's outline on the sheet (within [`EPS_SHEET`]) counts as covered, so an edge
//! behind a face never leaks through the face's own triangulation diagonals (a back window's head at 2.1 m lined up with
//! a diagonal of the front wall). A triangle hides only what is more than [`EPS_DEPTH`] behind its plane, plus what its
//! slope could add across that outline band, so an edge is never hidden by the faces it lies in, and a face seen nearly
//! edge-on hides nothing.
//!
//! Sheet coordinates: `u` to the right, `v` up, in metres; depth grows away from the viewer.

use crate::edges::{CREASE_DEG, cross, dot, sub, unit, weld, welded_edges};
use crate::section::Plane;
use crate::{ArchId, Scene3d};

/// Points this close to a triangle's outline on the sheet (metres) count as covered by it.
pub const EPS_SHEET: f64 = 1e-5;
/// An edge must be this far (metres) behind a triangle's plane to be hidden by it.
pub const EPS_DEPTH: f64 = 1e-4;
/// Visible runs shorter than this on the sheet (metres) are dropped, as are edges seen end-on.
pub const MIN_RUN: f64 = 1e-5;
/// Lines whose directions differ by less than this (radians) may merge.
const ANGLE_Q: f64 = 1e-6;
/// The grid has at most this many cells along each side.
const GRID_MAX: usize = 128;

/// An orthographic view: looking along `dir`, with `up` pointing up the sheet and `right = dir × up` to the right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrthoView {
    right: [f64; 3],
    up: [f64; 3],
    dir: [f64; 3],
}

impl OrthoView {
    /// Looking along `dir`, with the part of `up` across `dir` pointing up the sheet. `None` when either is zero or not
    /// finite, or they are parallel.
    pub fn new(dir: [f64; 3], up: [f64; 3]) -> Option<Self> {
        let dir = unit(dir)?;
        let k = dot(up, dir);
        let up = unit([up[0] - k * dir[0], up[1] - k * dir[1], up[2] - k * dir[2]])?;
        Some(OrthoView { right: cross(dir, up), up, dir })
    }

    /// A horizontal elevation looking along `facing` (x, y) with +z up: `[0, 1]` looks north at the south face.
    pub fn elevation(facing: [f64; 2]) -> Option<Self> {
        Self::new([facing[0], facing[1], 0.0], [0.0, 0.0, 1.0])
    }

    pub fn dir(&self) -> [f64; 3] {
        self.dir
    }

    pub fn up(&self) -> [f64; 3] {
        self.up
    }

    pub fn right(&self) -> [f64; 3] {
        self.right
    }

    /// `p` on the sheet: `[u, v, depth]`.
    pub fn project(&self, p: [f64; 3]) -> [f64; 3] {
        [dot(p, self.right), dot(p, self.up), dot(p, self.dir)]
    }
}

/// A visible line on the sheet, tagged with the arch object it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line2 {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub object: ArchId,
}

impl Line2 {
    pub const fn new(a: [f64; 2], b: [f64; 2], object: ArchId) -> Self {
        Line2 { a, b, object }
    }

    pub fn length(&self) -> f64 {
        (self.b[0] - self.a[0]).hypot(self.b[1] - self.a[1])
    }
}

/// The cut face of one object in a section view: rings on the sheet (even-odd) at one depth. It hides what is behind
/// it inside its rings, as the solid's material would.
pub(crate) struct Cap {
    pub rings: Vec<Vec<[f64; 2]>>,
    pub depth: f64,
}

/// A triangle on the sheet, `[u, v, depth]` per corner.
struct Tri {
    p: [[f64; 3]; 3],
    lo: [f64; 2],
    hi: [f64; 2],
    wmin: f64,
    /// Twice the signed area on the sheet.
    area2: f64,
    /// How far behind its plane an edge must be to be hidden: [`EPS_DEPTH`] plus the plane's depth change across the
    /// [`EPS_SHEET`] outline band.
    margin: f64,
}

impl Tri {
    fn new(p: [[f64; 3]; 3]) -> Option<Self> {
        let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let area2 = n[2];
        if !n.iter().all(|x| x.is_finite()) || area2.abs() <= 1e-12 {
            return None; // seen edge-on, degenerate or not finite: it covers no area
        }
        let slope = n[0].hypot(n[1]) / area2.abs();
        let lo = [p[0][0].min(p[1][0]).min(p[2][0]), p[0][1].min(p[1][1]).min(p[2][1])];
        let hi = [p[0][0].max(p[1][0]).max(p[2][0]), p[0][1].max(p[1][1]).max(p[2][1])];
        let wmin = p[0][2].min(p[1][2]).min(p[2][2]);
        Some(Tri { p, lo, hi, wmin, area2, margin: EPS_DEPTH + EPS_SHEET * slope })
    }

    /// The part `[lo, hi]` of the sheet segment `e0 → e1` the triangle covers (closed, within [`EPS_SHEET`]).
    fn covered(&self, e0: [f64; 3], e1: [f64; 3]) -> Option<(f64, f64)> {
        let s = self.area2.signum();
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for k in 0..3 {
            let (a, b) = (self.p[k], self.p[(k + 1) % 3]);
            let e = [b[0] - a[0], b[1] - a[1]];
            let len = e[0].hypot(e[1]);
            if !len.is_finite() || len <= 1e-15 {
                return None;
            }
            let f = |x: [f64; 3]| s * (e[0] * (x[1] - a[1]) - e[1] * (x[0] - a[0])) / len + EPS_SHEET;
            let (f0, f1) = (f(e0), f(e1));
            if f0 < 0.0 && f1 < 0.0 {
                return None;
            }
            if f0 >= 0.0 && f1 >= 0.0 {
                continue;
            }
            let t = f0 / (f0 - f1);
            if f0 < 0.0 { lo = lo.max(t) } else { hi = hi.min(t) }
        }
        (hi > lo).then_some((lo, hi))
    }

    /// The triangle's plane depth at sheet point `x` (extrapolated outside the triangle).
    fn depth_at(&self, x: [f64; 2]) -> f64 {
        let [a, b, c] = self.p;
        let cr = |p: [f64; 2], q: [f64; 2]| p[0] * q[1] - p[1] * q[0];
        let (ab, ac, ax) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]], [x[0] - a[0], x[1] - a[1]]);
        let lb = cr(ax, ac) / self.area2;
        let lc = cr(ab, ax) / self.area2;
        a[2] + lb * (b[2] - a[2]) + lc * (c[2] - a[2])
    }

    /// The part of the sheet segment `e0 → e1` this triangle hides, if any.
    fn hides(&self, e0: [f64; 3], e1: [f64; 3]) -> Option<(f64, f64)> {
        let (lo, hi) = self.covered(e0, e1)?;
        let at = |t: f64| {
            let x = [e0[0] + t * (e1[0] - e0[0]), e0[1] + t * (e1[1] - e0[1])];
            e0[2] + t * (e1[2] - e0[2]) - self.depth_at(x) - self.margin
        };
        let (g0, g1) = (at(lo), at(hi));
        match (g0 > 0.0, g1 > 0.0) {
            (true, true) => Some((lo, hi)),
            (false, false) => None,
            (true, false) => Some((lo, lo + (hi - lo) * g0 / (g0 - g1))),
            (false, true) => Some((lo + (hi - lo) * g0 / (g0 - g1), hi)),
        }
    }
}

/// A uniform grid over the sheet: each cell lists the triangles whose (outline-band-grown) box touches it.
struct Grid {
    lo: [f64; 2],
    cell: [f64; 2],
    n: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid {
    fn new(tris: &[Tri]) -> Self {
        let mut lo = [f64::MAX; 2];
        let mut hi = [f64::MIN; 2];
        for t in tris {
            for i in 0..2 {
                lo[i] = lo[i].min(t.lo[i]);
                hi[i] = hi[i].max(t.hi[i]);
            }
        }
        let n = ((tris.len() as f64).sqrt().ceil() as usize).clamp(1, GRID_MAX);
        let cell = [((hi[0] - lo[0]) / n as f64).max(1e-9), ((hi[1] - lo[1]) / n as f64).max(1e-9)];
        let mut g = Grid { lo, cell, n, cells: vec![Vec::new(); n * n] };
        for (k, t) in tris.iter().enumerate() {
            let (x0, y0, x1, y1) = g.span(t.lo, t.hi);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(c) = g.cells.get_mut(y * n + x) {
                        c.push(k as u32);
                    }
                }
            }
        }
        g
    }

    /// The cell index range covering the box `lo..hi` grown by the outline band, clamped to the grid.
    fn span(&self, lo: [f64; 2], hi: [f64; 2]) -> (usize, usize, usize, usize) {
        let ix = |v: f64, i: usize| {
            let c = ((v - self.lo[i]) / self.cell[i]).floor();
            if c.is_nan() { 0 } else { (c.max(0.0) as usize).min(self.n - 1) }
        };
        (ix(lo[0] - EPS_SHEET, 0), ix(lo[1] - EPS_SHEET, 1), ix(hi[0] + EPS_SHEET, 0), ix(hi[1] + EPS_SHEET, 1))
    }
}

/// Clip a triangle to the kept side of `plane` (distance ≥ 0): none, the triangle, or a fan of one or two pieces.
fn clip_triangle(c: [[f64; 3]; 3], plane: &Plane) -> Vec<[[f64; 3]; 3]> {
    let d = c.map(|p| plane.distance(p));
    if d.iter().all(|&x| x >= 0.0) {
        return vec![c];
    }
    let mut poly: Vec<[f64; 3]> = Vec::with_capacity(4);
    for i in 0..3 {
        let j = (i + 1) % 3;
        if d[i] >= 0.0 {
            poly.push(c[i]);
        }
        if (d[i] >= 0.0) != (d[j] >= 0.0) {
            let t = d[i] / (d[i] - d[j]);
            poly.push([c[i][0] + t * (c[j][0] - c[i][0]), c[i][1] + t * (c[j][1] - c[i][1]), c[i][2] + t * (c[j][2] - c[i][2])]);
        }
    }
    (1..poly.len().saturating_sub(1)).map(|k| [poly[0], poly[k], poly[k + 1]]).collect()
}

/// The part of the segment `a → b` on the kept side of `plane`, if any.
fn clip_segment(a: [f64; 3], b: [f64; 3], plane: &Plane) -> Option<([f64; 3], [f64; 3])> {
    let (da, db) = (plane.distance(a), plane.distance(b));
    match (da >= 0.0, db >= 0.0) {
        (true, true) => Some((a, b)),
        (false, false) => None,
        (ka, _) => {
            let t = da / (da - db);
            let m = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
            Some(if ka { (a, m) } else { (m, b) })
        }
    }
}

/// Where the sheet segment `p0 → p1` meets the segment `a → b`, as parameters along `p0 → p1`: one crossing, or both
/// ends of a collinear overlap.
fn split_params(p0: [f64; 2], p1: [f64; 2], a: [f64; 2], b: [f64; 2], out: &mut Vec<f64>) {
    let r = [p1[0] - p0[0], p1[1] - p0[1]];
    let s = [b[0] - a[0], b[1] - a[1]];
    let cr = |x: [f64; 2], y: [f64; 2]| x[0] * y[1] - x[1] * y[0];
    let den = cr(r, s);
    let q = [a[0] - p0[0], a[1] - p0[1]];
    let rr = r[0] * r[0] + r[1] * r[1];
    if den.abs() > 1e-15 {
        let t = cr(q, s) / den;
        let u = cr(q, r) / den;
        if (0.0..=1.0).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&u) {
            out.push(t);
        }
    } else if rr > 0.0 && cr(q, r).abs() / rr.sqrt() <= EPS_SHEET {
        for p in [a, b] {
            let t = ((p[0] - p0[0]) * r[0] + (p[1] - p0[1]) * r[1]) / rr;
            if (0.0..=1.0).contains(&t) {
                out.push(t);
            }
        }
    }
}

/// Whether `x` is inside the rings (even-odd) or within [`EPS_SHEET`] of one of them.
fn in_rings(x: [f64; 2], rings: &[Vec<[f64; 2]>]) -> bool {
    let mut inside = false;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let e = [b[0] - a[0], b[1] - a[1]];
            let ll = e[0] * e[0] + e[1] * e[1];
            let t = if ll > 0.0 { (((x[0] - a[0]) * e[0] + (x[1] - a[1]) * e[1]) / ll).clamp(0.0, 1.0) } else { 0.0 };
            if (x[0] - a[0] - t * e[0]).hypot(x[1] - a[1] - t * e[1]) <= EPS_SHEET {
                return true;
            }
            if (a[1] > x[1]) != (b[1] > x[1]) && x[0] < a[0] + (x[1] - a[1]) * e[0] / e[1] {
                inside = !inside;
            }
        }
    }
    inside
}

/// The parts of the sheet segment `e0 → e1` a cap hides: inside its rings and more than [`EPS_DEPTH`] behind it.
fn cap_hides(e0: [f64; 3], e1: [f64; 3], cap: &Cap, out: &mut Vec<(f64, f64)>) {
    let (p0, p1) = ([e0[0], e0[1]], [e1[0], e1[1]]);
    let mut ts = vec![0.0, 1.0];
    for ring in &cap.rings {
        for i in 0..ring.len() {
            split_params(p0, p1, ring[i], ring[(i + 1) % ring.len()], &mut ts);
        }
    }
    let dw = e1[2] - e0[2];
    if dw.abs() > 1e-15 {
        let t = (cap.depth + EPS_DEPTH - e0[2]) / dw;
        if t > 0.0 && t < 1.0 {
            ts.push(t);
        }
    }
    ts.sort_by(f64::total_cmp);
    for w in ts.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        if t1 - t0 <= 0.0 {
            continue;
        }
        let m = (t0 + t1) / 2.0;
        let x = [p0[0] + m * (p1[0] - p0[0]), p0[1] + m * (p1[1] - p0[1])];
        if e0[2] + m * dw > cap.depth + EPS_DEPTH && in_rings(x, &cap.rings) {
            out.push((t0, t1));
        }
    }
}

/// The visible lines of the scene in `view`, merged per object.
pub fn visible_lines(scene: &Scene3d, view: &OrthoView) -> Vec<Line2> {
    visible_lines_beyond(scene, view, None, &[])
}

/// The visible lines of the part of the scene on the kept side of `keep` (all of it when `None`), with `caps` (a
/// section's cut faces) hiding what is behind them. Merged per object, in a fixed order.
pub(crate) fn visible_lines_beyond(scene: &Scene3d, view: &OrthoView, keep: Option<&Plane>, caps: &[Cap]) -> Vec<Line2> {
    let mut tris: Vec<Tri> = Vec::new();
    let mut edges: Vec<([f64; 3], [f64; 3], ArchId)> = Vec::new();
    for mesh in &scene.meshes {
        let w = weld(mesh);
        for t in &w.tris {
            let c = w.corners(t);
            let pieces = match keep {
                None => vec![c],
                Some(pl) => clip_triangle(c, pl),
            };
            tris.extend(pieces.into_iter().filter_map(|p| Tri::new(p.map(|q| view.project(q)))));
        }
        for e in welded_edges(&w, CREASE_DEG, Some(view.dir()), mesh.object) {
            let kept = match keep {
                None => Some((e.a, e.b)),
                Some(pl) => clip_segment(e.a, e.b, pl),
            };
            if let Some((a, b)) = kept {
                edges.push((view.project(a), view.project(b), e.object));
            }
        }
    }
    let grid = Grid::new(&tris);
    let mut stamp: Vec<usize> = vec![usize::MAX; tris.len()];
    let mut out = Vec::new();
    let mut hidden: Vec<(f64, f64)> = Vec::new();
    for (k, &(e0, e1, object)) in edges.iter().enumerate() {
        let len = (e1[0] - e0[0]).hypot(e1[1] - e0[1]);
        if !len.is_finite() || len < MIN_RUN {
            continue; // seen end-on (or not finite): a point, not a line
        }
        hidden.clear();
        let elo = [e0[0].min(e1[0]), e0[1].min(e1[1])];
        let ehi = [e0[0].max(e1[0]), e0[1].max(e1[1])];
        let wmax = e0[2].max(e1[2]);
        if !tris.is_empty() {
            let (x0, y0, x1, y1) = grid.span(elo, ehi);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    for &ti in grid.cells.get(y * grid.n + x).map(Vec::as_slice).unwrap_or(&[]) {
                        let Some(s) = stamp.get_mut(ti as usize) else { continue };
                        if *s == k {
                            continue;
                        }
                        *s = k;
                        let Some(t) = tris.get(ti as usize) else { continue };
                        let apart = (0..2).any(|i| t.lo[i] > ehi[i] + EPS_SHEET || t.hi[i] < elo[i] - EPS_SHEET);
                        if apart || t.wmin + t.margin >= wmax {
                            continue;
                        }
                        if let Some(h) = t.hides(e0, e1) {
                            hidden.push(h);
                        }
                    }
                }
            }
        }
        for cap in caps {
            cap_hides(e0, e1, cap, &mut hidden);
        }
        hidden.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut t = 0.0_f64;
        let mut runs: Vec<(f64, f64)> = Vec::new();
        for &(h0, h1) in &hidden {
            if h0 > t {
                runs.push((t, h0));
            }
            t = t.max(h1);
        }
        if t < 1.0 {
            runs.push((t, 1.0));
        }
        for (t0, t1) in runs {
            if (t1 - t0) * len < MIN_RUN {
                continue;
            }
            let at = |t: f64| [e0[0] + t * (e1[0] - e0[0]), e0[1] + t * (e1[1] - e0[1])];
            out.push(Line2::new(at(t0), at(t1), object));
        }
    }
    merge_collinear(out)
}

/// Join each object's collinear lines that overlap or touch (within [`EPS_SHEET`]) into one, so an edge split by
/// welding or by occlusion, or drawn twice by one object, is a single stroke. Lines of different objects never merge:
/// each keeps the object it belongs to. Lines shorter than [`MIN_RUN`] go. The result is in a fixed order.
pub fn merge_collinear(lines: Vec<Line2>) -> Vec<Line2> {
    struct Item {
        object: ArchId,
        q: i64,
        rho: f64,
        s0: f64,
        s1: f64,
        d: [f64; 2],
    }
    let mut items: Vec<Item> = lines
        .iter()
        .filter_map(|l| {
            let len = l.length();
            if !len.is_finite() || len < MIN_RUN {
                return None;
            }
            let mut d = [(l.b[0] - l.a[0]) / len, (l.b[1] - l.a[1]) / len];
            if d[0] < -1e-6 || (d[0].abs() <= 1e-6 && d[1] < 0.0) {
                d = [-d[0], -d[1]];
            }
            let q = (d[1].atan2(d[0]) / ANGLE_Q).round() as i64;
            let rho = -d[1] * l.a[0] + d[0] * l.a[1];
            let (sa, sb) = (d[0] * l.a[0] + d[1] * l.a[1], d[0] * l.b[0] + d[1] * l.b[1]);
            Some(Item { object: l.object, q, rho, s0: sa.min(sb), s1: sa.max(sb), d })
        })
        .collect();
    items.sort_by(|a, b| a.object.cmp(&b.object).then(a.q.cmp(&b.q)).then(a.rho.total_cmp(&b.rho)).then(a.s0.total_cmp(&b.s0)));
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let first = &items[i];
        let mut j = i + 1;
        while j < items.len() && items[j].object == first.object && items[j].q == first.q && (items[j].rho - first.rho).abs() <= EPS_SHEET {
            j += 1;
        }
        let (d, rho) = (first.d, first.rho);
        let mut spans: Vec<(f64, f64)> = items[i..j].iter().map(|x| (x.s0, x.s1)).collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = |s: f64| [s * d[0] - rho * d[1], s * d[1] + rho * d[0]];
        let mut cur: Option<(f64, f64)> = None;
        for (s0, s1) in spans {
            cur = match cur {
                Some((c0, c1)) if s0 <= c1 + EPS_SHEET => Some((c0, c1.max(s1))),
                Some((c0, c1)) => {
                    out.push(Line2::new(at(c0), at(c1), first.object));
                    Some((s0, s1))
                }
                None => Some((s0, s1)),
            };
        }
        if let Some((c0, c1)) = cur {
            out.push(Line2::new(at(c0), at(c1), first.object));
        }
        i = j;
    }
    out
}
