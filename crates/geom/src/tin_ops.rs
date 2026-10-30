//! Operations on a [`Tin`]: a grid index for point location, clipping to a ring, and chaining
//! contour segments into polylines.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::Vec2;
use crate::Vec3;
use crate::tin::{ContourSegment, Tin, barycentric};

/// A uniform grid over a TIN's triangles for fast point location.
#[derive(Clone, Debug, Default)]
pub struct TinIndex {
    /// Cells, row-major, `height` rows of `width` columns; each cell
    /// holds the triangle indices whose bounding box overlaps it.
    cells: Vec<Vec<Vec<usize>>>,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: usize,
    height: usize,
}

impl TinIndex {
    /// Build a uniform grid over the bounding box of `tin`'s triangles, about
    /// `sqrt(triangles)` cells per side (clamped to 1..=1024). Each cell lists
    /// every triangle whose bounding box overlaps the cell.
    pub fn new(tin: &Tin) -> TinIndex {
        const MAX_SIDE: usize = 1024;
        let n = tin.triangles.len();

        // Bounding box per triangle plus the global bounding box of all usable
        // vertices. A triangle with a bad or non-finite vertex can never
        // contain a point, so it carries no box and is never indexed.
        let mut tri_bbs: Vec<Option<([f64; 2], [f64; 2])>> = Vec::with_capacity(n);
        let mut x_min = f64::MAX;
        let mut x_max = f64::MIN;
        let mut y_min = f64::MAX;
        let mut y_max = f64::MIN;
        for t in tin.triangles.iter() {
            let (mut tx_min, mut tx_max) = (f64::MAX, f64::MIN);
            let (mut ty_min, mut ty_max) = (f64::MAX, f64::MIN);
            let ok = [t[0], t[1], t[2]].iter().all(|k| match tin.points.get(*k) {
                Some(p) => {
                    if p.x.is_finite() && p.y.is_finite() {
                        tx_min = tx_min.min(p.x);
                        tx_max = tx_max.max(p.x);
                        ty_min = ty_min.min(p.y);
                        ty_max = ty_max.max(p.y);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            });
            if ok {
                x_min = x_min.min(tx_min);
                x_max = x_max.max(tx_max);
                y_min = y_min.min(ty_min);
                y_max = y_max.max(ty_max);
                tri_bbs.push(Some(([tx_min, ty_min], [tx_max, ty_max])));
            } else {
                tri_bbs.push(None);
            }
        }

        let span = |min: f64, max: f64| -> bool { max.is_finite() && max > min };
        let width = (n as f64).sqrt().ceil().clamp(1.0, 1024.0) as usize;
        let height = (n as f64).sqrt().ceil().clamp(1.0, 1024.0) as usize;

        // Fill cells.
        let mut cells: Vec<Vec<Vec<usize>>> = (0..height).map(|_| vec![Vec::new(); width]).collect();
        if span(x_min, x_max) && span(y_min, y_max) {
            let dx = (x_max - x_min) / width as f64;
            let dy = (y_max - y_min) / height as f64;
            let cell = |v: f64, min: f64, span: f64, side: usize| -> usize {
                if !(v >= min && v <= min + span) {
                    return 0;
                }
                let t = (v - min) / span;
                if t >= (side - 1) as f64 { side - 1 } else { t.floor() as i64 as usize }
            };
            for (i, tbb) in tri_bbs.iter().enumerate() {
                let Some(tbb) = tbb else {
                    continue;
                };
                let tx_min = tbb.0[0];
                let tx_max = tbb.0[1];
                let ty_min = tbb.1[0];
                let ty_max = tbb.1[1];
                let col0 = cell(tx_min, x_min, x_max - x_min, width);
                let col1 = cell(tx_max, x_min, x_max - x_min, width);
                let row0 = cell(ty_min, y_min, y_max - y_min, height);
                let row1 = cell(ty_max, y_min, y_max - y_min, height);
                for row in row0..=row1 {
                    for col in col0..=col1 {
                        cells[row][col].push(i);
                    }
                }
            }
        }

        TinIndex { cells, x_min, x_max, y_min, y_max, width, height }
    }

    /// The triangle containing `p` and `p`'s barycentric weights for its three corners, or
    /// `None` outside the TIN. The lowest triangle index wins on a shared edge.
    pub fn locate(&self, tin: &Tin, p: Vec2) -> Option<(usize, [f64; 3])> {
        if !p.is_finite() {
            return None;
        }
        let col = self.cell_index(p.x, self.x_min, self.x_max, self.width)?;
        let row = self.cell_index(p.y, self.y_min, self.y_max, self.height)?;
        for &i in &self.cells[row][col] {
            let Some(t) = tin.triangles.get(i) else { continue };
            let Some(a) = tin.points.get(t[0]) else { return None };
            let Some(b) = tin.points.get(t[1]) else { return None };
            let Some(c) = tin.points.get(t[2]) else { return None };
            let a = Vec2::new(a.x, a.y);
            let b = Vec2::new(b.x, b.y);
            let c = Vec2::new(c.x, c.y);
            if let Some((u, v, w)) = barycentric(a, b, c, p) {
                return Some((i, [u, v, w]));
            }
        }
        None
    }

    fn cell_index(&self, v: f64, min: f64, max: f64, side: usize) -> Option<usize> {
        if !(v >= min && v <= max) {
            return None;
        }
        let span = max - min;
        if span <= 0.0 {
            return Some(0);
        }
        let t = (v - min) / span * side as f64;
        if t >= (side - 1) as f64 {
            Some(side - 1)
        } else {
            let i = t.floor() as i64;
            if i < 0 { None } else { Some(i as usize) }
        }
    }
}

/// Keep only the triangles of `tin` whose centroid lies inside `ring` (even-odd).
/// Points are kept, so indices stay valid. A ring with fewer than 3 points, or
/// with any non-finite point, removes every triangle.
pub fn retain_inside(tin: &mut Tin, ring: &[Vec2]) {
    let ring_ok = ring.len() >= 3 && ring.iter().all(|p| p.is_finite());
    if !ring_ok {
        tin.triangles.clear();
        return;
    }
    tin.triangles.retain(|t| {
        let Some(a) = tin.points.get(t[0]) else { return false };
        let Some(b) = tin.points.get(t[1]) else { return false };
        let Some(c) = tin.points.get(t[2]) else { return false };
        let cx = (a.x + b.x + c.x) / 3.0;
        let cy = (a.y + b.y + c.y) / 3.0;
        if !cx.is_finite() || !cy.is_finite() {
            return false;
        }
        point_in_ring(ring, Vec2::new(cx, cy))
    });
}

/// Even-odd rule: a ray from `p` to +infinity crosses the ring's edges an
/// odd number of times iff `p` is inside.
fn point_in_ring(ring: &[Vec2], p: Vec2) -> bool {
    let n = ring.len();
    let mut inside = false;
    for i in 0..n {
        let a = ring.get(i).copied();
        let b = ring.get((i + 1) % n).copied();
        if let (Some(a), Some(b)) = (a, b) {
            if (a.y > p.y) != (b.y > p.y) {
                let x_intersect = a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y);
                if x_intersect > p.x {
                    inside = !inside;
                }
            }
        } else {
            return false;
        }
    }
    inside
}

/// A contour polyline at one elevation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContourLine {
    pub elevation: f64,
    pub points: Vec<Vec2>,
    /// Whether the last point joins the first.
    pub closed: bool,
}

/// Endpoints within 1e-9 m are the same knot.
const EPS: f64 = 1e-9;

/// Quantise `c` to the nearest multiple of `EPS` and return a hashable key.
fn quant(c: f64) -> i64 {
    let q = (c / EPS).round();
    if q.is_finite() && q > -(i64::MIN as f64) && q < (i64::MAX as f64) { q as i64 } else { 0 }
}

/// Chain segments that share endpoints (same elevation) into polylines.
/// Segments are grouped by exact elevation; within a group, segments whose
/// endpoints are within 1e-9 m are joined into maximal polylines. Open lines
/// run end-to-end (walked from their endpoints), remaining loops are closed
/// without repeating the first point. Output is by elevation ascending, then
/// first point (x, then y).
pub fn chain_contours(segs: &[ContourSegment]) -> Vec<ContourLine> {
    // Group by exact elevation (bit pattern), keeping input order within each group.
    let mut by_elev: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        if s.elevation.is_finite() && s.start.is_finite() && s.end.is_finite() {
            by_elev.entry(s.elevation.to_bits()).or_default().push(i);
        }
    }
    let mut lines: Vec<ContourLine> = Vec::new();
    let mut used: Vec<bool> = vec![false; segs.len()];
    for (_elev, ids) in by_elev {
        // Quantised endpoint -> list of segment indices (global) meeting there.
        let mut endpoints: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for &i in &ids {
            let s = &segs[i];
            endpoints.entry((quant(s.start.x), quant(s.start.y))).or_default().push(i);
            endpoints.entry((quant(s.end.x), quant(s.end.y))).or_default().push(i);
        }
        // Open lines: walk from each endpoint touched by exactly one segment.
        for (key, segs_at) in &endpoints {
            if segs_at.len() != 1 {
                continue;
            }
            let first = *segs_at.first().unwrap();
            if used[first] {
                continue;
            }
            lines.push(walk_line(segs, &endpoints, &mut used, first, *key));
        }
        // Closed loops from the remaining degree-two segments.
        for (key, segs_at) in &endpoints {
            if segs_at.len() != 2 {
                continue;
            }
            let a = segs_at[0];
            let b = segs_at[1];
            if used[a] || used[b] {
                continue;
            }
            lines.push(walk_loop(segs, &endpoints, &mut used, a, *key));
        }
    }
    lines.sort_by(lines_cmp);
    lines
}

/// Total-order comparator: elevation, then first point (x, then y).
fn lines_cmp(a: &ContourLine, b: &ContourLine) -> Ordering {
    let ce = a.elevation.total_cmp(&b.elevation);
    if ce != Ordering::Equal {
        return ce;
    }
    match (a.points.first(), b.points.first()) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(pa), Some(pb)) => {
            let cx = pa.x.total_cmp(&pb.x);
            if cx != Ordering::Equal {
                return cx;
            }
            pa.y.total_cmp(&pb.y)
        }
    }
}

/// The quantised key of the segment's other endpoint.
fn other_key_of(s: &ContourSegment, key: (i64, i64)) -> (i64, i64) {
    let sk = (quant(s.start.x), quant(s.start.y));
    if sk == key { (quant(s.end.x), quant(s.end.y)) } else { sk }
}

/// The un-quantised point at `key` of segment `s`.
fn point_of(s: &ContourSegment, key: (i64, i64)) -> Vec2 {
    let sk = (quant(s.start.x), quant(s.start.y));
    if sk == key { s.start } else { s.end }
}

/// Walk a maximal open polyline from segment `i` entering through `start_key`.
fn walk_line(
    segs: &[ContourSegment],
    endpoints: &HashMap<(i64, i64), Vec<usize>>,
    used: &mut Vec<bool>,
    i: usize,
    start_key: (i64, i64),
) -> ContourLine {
    let elevation = segs[i].elevation;
    let mut points = Vec::new();
    points.push(point_of(&segs[i], start_key));
    let mut back_key = start_key;
    let mut cur = i;
    used[cur] = true;
    loop {
        let front_key = other_key_of(&segs[cur], back_key);
        let next_idx = match endpoints.get(&front_key) {
            Some(list) => list.iter().copied().find(|&j| !used[j]),
            None => None,
        };
        points.push(point_of(&segs[cur], front_key));
        match next_idx {
            Some(j) => {
                back_key = front_key;
                cur = j;
                used[j] = true;
            }
            None => break,
        }
    }
    ContourLine { elevation, points, closed: false }
}

/// Walk a closed loop starting at segment `i` entering through `start_key`.
fn walk_loop(
    segs: &[ContourSegment],
    endpoints: &HashMap<(i64, i64), Vec<usize>>,
    used: &mut Vec<bool>,
    i: usize,
    start_key: (i64, i64),
) -> ContourLine {
    let elevation = segs[i].elevation;
    let mut points = Vec::new();
    points.push(point_of(&segs[i], start_key));
    let mut back_key = start_key;
    let mut cur = i;
    used[cur] = true;
    loop {
        let front_key = other_key_of(&segs[cur], back_key);
        if front_key == start_key {
            break;
        }
        let next_idx = match endpoints.get(&front_key) {
            Some(list) => list.iter().copied().find(|&j| !used[j]),
            None => None,
        };
        points.push(point_of(&segs[cur], front_key));
        match next_idx {
            Some(j) => {
                back_key = front_key;
                cur = j;
                used[j] = true;
            }
            None => break,
        }
    }
    ContourLine { elevation, points, closed: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tin_index_locates_like_a_full_scan() {
        let mut tin = Tin::default();
        tin.points.push(Vec3::new(0.0, 0.0, 0.0));
        tin.points.push(Vec3::new(10.0, 0.0, 0.0));
        tin.points.push(Vec3::new(5.0, 5.0, 0.0));
        tin.triangles.push([0, 1, 2]);
        let idx = TinIndex::new(&tin);
        assert_eq!(idx.locate(&tin, Vec2::new(3.0, 2.0)).unwrap().0, 0);
    }

    #[test]
    fn tin_index_handles_points_outside_and_nan() {
        let mut tin = Tin::default();
        tin.points.push(Vec3::new(0.0, 0.0, 0.0));
        tin.points.push(Vec3::new(1.0, 0.0, 0.0));
        tin.triangles.push([0, 1, 1]);
        let idx = TinIndex::new(&tin);
        assert_eq!(idx.locate(&tin, Vec2::new(5.0, 5.0)), None);
        assert_eq!(idx.locate(&tin, Vec2::new(f64::NAN, 0.0)), None);
    }

    #[test]
    fn retain_inside_drops_triangles_outside_a_concave_ring() {
        let mut tin = Tin::default();
        tin.points.push(Vec3::new(0.0, 0.0, 0.0));
        tin.points.push(Vec3::new(4.0, 0.0, 0.0));
        tin.points.push(Vec3::new(2.0, 2.0, 0.0));
        tin.triangles.push([0, 1, 2]);
        retain_inside(&mut tin, &[Vec2::new(0.0, 0.0), Vec2::new(2.0, 1.0), Vec2::new(0.0, 2.0)]);
        assert_eq!(tin.triangles.len(), 1);
    }

    #[test]
    fn chain_contours_joins_segments_end_to_end() {
        let segs = vec![
            ContourSegment { elevation: 1.0, start: Vec2::new(0.0, 0.0), end: Vec2::new(1.0, 0.0) },
            ContourSegment { elevation: 1.0, start: Vec2::new(1.0, 0.0), end: Vec2::new(2.0, 0.0) },
        ];
        let lines = chain_contours(&segs);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].points.len(), 3);
    }

    #[test]
    fn chain_contours_closes_loops() {
        let segs = vec![
            ContourSegment { elevation: 1.0, start: Vec2::new(0.0, 0.0), end: Vec2::new(1.0, 0.0) },
            ContourSegment { elevation: 1.0, start: Vec2::new(1.0, 0.0), end: Vec2::new(1.0, 1.0) },
            ContourSegment { elevation: 1.0, start: Vec2::new(1.0, 1.0), end: Vec2::new(0.0, 1.0) },
            ContourSegment { elevation: 1.0, start: Vec2::new(0.0, 1.0), end: Vec2::new(0.0, 0.0) },
        ];
        let lines = chain_contours(&segs);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].closed);
        assert_eq!(lines[0].points.len(), 4);
    }

    #[test]
    fn chain_contours_keeps_levels_apart() {
        let segs = vec![
            ContourSegment { elevation: 1.0, start: Vec2::new(0.0, 0.0), end: Vec2::new(1.0, 0.0) },
            ContourSegment { elevation: 2.0, start: Vec2::new(1.0, 0.0), end: Vec2::new(2.0, 0.0) },
        ];
        let lines = chain_contours(&segs);
        assert_eq!(lines.len(), 2);
    }
}
