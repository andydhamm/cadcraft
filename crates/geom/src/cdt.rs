//! Constrained Delaunay triangulation: a [`Tin`] in which given segments (breaklines, boundaries)
//! are triangle edges.
//!
//! Sloan (1993) / Anglada (1997): triangulate the points, then insert each constraint in turn. A
//! constraint that is already an edge is recorded. One that passes through a vertex is split there,
//! and one that crosses an earlier constraint is split at a new point where the two cross. Any other
//! removes the triangles it crosses and retriangulates the polygon on each side of it. Every step
//! scans the triangles (brute force), which is fine at terrain sizes.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::tin::{MAX_TIN_POINTS, Tin, TinError, in_circumcircle, triangulate};
use crate::{Vec2, Vec3};

/// A triangulation plus the edges that were forced into it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cdt {
    /// The triangulation. Its `points` start with the input points in order; points added where
    /// two constraints cross follow them.
    pub tin: Tin,
    /// Every constrained edge as `[lo, hi]` point indices (`lo < hi`), sorted, after splitting at
    /// crossings and at points lying on a constraint.
    pub constrained: Vec<[usize; 2]>,
}

/// A point within this fraction of `|ab|` of segment `ab` lies on it.
const ON_SEGMENT: f64 = 1e-9;
/// Two segments cross properly when each meets the other this far (as a fraction) from its ends.
const PROPER: f64 = 1e-9;
/// An input point no triangle uses stands for the used point nearest it within this fraction of
/// the points' extent (`triangulate` skips a point that coincides with an earlier one).
const SAME_POINT: f64 = 1e-6;

/// The Delaunay triangulation of `points` in which every `constraints` segment (a pair of point
/// indices) is a union of triangle edges.
///
/// A constraint `[i, i]`, or one joining two coincident points, is skipped; a constraint naming a
/// point that coincides with an earlier one uses the earlier point. Refuses an index past the
/// points ([`TinError::BadConstraint`]), more than `4 * MAX_TIN_POINTS` constraints
/// ([`TinError::TooManyConstraints`]) and whatever [`triangulate`] refuses; a constraint that would
/// need more than `4 * points` splits, or that the arithmetic cannot place, is
/// [`TinError::Degenerate`].
pub fn triangulate_constrained(points: &[Vec3], constraints: &[[usize; 2]]) -> Result<Cdt, TinError> {
    if constraints.len() > MAX_TIN_POINTS * 4 {
        return Err(TinError::TooManyConstraints(constraints.len()));
    }
    if let Some(&index) = constraints.iter().flatten().find(|&&i| i >= points.len()) {
        return Err(TinError::BadConstraint { index });
    }
    let mut work = Work::new(triangulate(points)?);
    for &[a, b] in constraints {
        let (Some(a), Some(b)) = (work.vertex(a), work.vertex(b)) else { continue };
        if a != b {
            work.insert_constraint(a, b)?;
        }
    }
    Ok(work.finish())
}

/// A triangulation being constrained.
struct Work {
    points: Vec<Vec3>,
    triangles: Vec<[usize; 3]>,
    /// Constrained edges as `(lo, hi)`.
    constrained: BTreeSet<(usize, usize)>,
    /// For each point, the point the triangulation uses for it (itself, or the earlier point it
    /// coincides with), `None` for one no triangle can use.
    vertex: Vec<Option<usize>>,
}

impl Work {
    fn new(tin: Tin) -> Work {
        let Tin { points, triangles } = tin;
        let mut used = vec![false; points.len()];
        for &i in triangles.iter().flatten() {
            if let Some(u) = used.get_mut(i) {
                *u = true;
            }
        }
        let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
        for p in &points {
            lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        let near = SAME_POINT * (hi.x - lo.x).max(hi.y - lo.y).max(1.0);
        let xy = |p: &Vec3| Vec2::new(p.x, p.y);
        let vertex = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if used.get(i).copied().unwrap_or(false) {
                    return Some(i);
                }
                let d2 = |j: usize| points.get(j).map_or(f64::MAX, |q| (xy(q) - xy(p)).dot(xy(q) - xy(p)));
                (0..points.len())
                    .filter(|&j| used.get(j).copied().unwrap_or(false))
                    .min_by(|&j, &k| d2(j).total_cmp(&d2(k)))
                    .filter(|&j| d2(j) <= near * near)
            })
            .collect();
        Work { points, triangles, constrained: BTreeSet::new(), vertex }
    }

    /// The triangulation's vertex for input point `i`.
    fn vertex(&self, i: usize) -> Option<usize> {
        self.vertex.get(i).copied().flatten()
    }

    fn xy(&self, i: usize) -> Vec2 {
        self.points.get(i).map_or(Vec2::new(f64::NAN, f64::NAN), |p| Vec2::new(p.x, p.y))
    }

    fn finish(self) -> Cdt {
        Cdt { tin: Tin { points: self.points, triangles: self.triangles }, constrained: self.constrained.into_iter().map(|(a, b)| [a, b]).collect() }
    }

    /// Force segment `a`-`b` into the triangulation, splitting it where it meets a vertex or crosses
    /// an earlier constraint.
    fn insert_constraint(&mut self, a: usize, b: usize) -> Result<(), TinError> {
        let mut budget = 4 * self.points.len() + 4;
        let mut stack = vec![(a, b)];
        while let Some((a, b)) = stack.pop() {
            if a == b {
                continue;
            }
            if self.has_edge(a, b) {
                self.constrained.insert(key(a, b));
                continue;
            }
            budget = budget.checked_sub(1).ok_or(TinError::Degenerate)?;
            if let Some(p) = self.vertex_on(a, b) {
                stack.push((p, b));
                stack.push((a, p));
            } else if let Some((c, d, along)) = self.first_crossing(a, b) {
                let x = self.split(c, d, along)?;
                stack.push((x, b));
                stack.push((a, x));
            } else {
                self.carve(a, b)?;
                self.constrained.insert(key(a, b));
            }
        }
        Ok(())
    }

    fn has_edge(&self, a: usize, b: usize) -> bool {
        self.triangles.iter().any(|t| edges_of(*t).any(|(u, v)| (u, v) == (a, b) || (u, v) == (b, a)))
    }

    /// The vertex (other than `a` and `b`) lying on the open segment `a`-`b` nearest `a`.
    fn vertex_on(&self, a: usize, b: usize) -> Option<usize> {
        let (pa, ab) = (self.xy(a), self.xy(b) - self.xy(a));
        let len2 = ab.dot(ab);
        let mut used: Vec<usize> = self.triangles.iter().flatten().copied().collect();
        used.sort_unstable();
        used.dedup();
        used.into_iter()
            .filter(|&p| p != a && p != b)
            .filter_map(|p| {
                let ap = self.xy(p) - pa;
                let t = ab.dot(ap) / len2;
                (ab.cross(ap).abs() <= ON_SEGMENT * len2 && t > 0.0 && t < 1.0).then_some((t, p))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, p)| p)
    }

    /// The constrained edge `(c, d)` that the open segment `a`-`b` crosses nearest `a`, with how far
    /// along `c`-`d` the crossing lies.
    fn first_crossing(&self, a: usize, b: usize) -> Option<(usize, usize, f64)> {
        let (pa, pb) = (self.xy(a), self.xy(b));
        self.constrained
            .iter()
            .filter(|&&(c, d)| c != a && c != b && d != a && d != b)
            .filter_map(|&(c, d)| crossing(pa, pb, self.xy(c), self.xy(d)).map(|(s, t)| (s, c, d, t)))
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, c, d, t)| (c, d, t))
    }

    /// Add the point `along` of the way from `c` to `d` (z interpolated along the edge), insert it,
    /// and replace the constraint `(c, d)` by its two halves. Returns the new point's index.
    fn split(&mut self, c: usize, d: usize, along: f64) -> Result<usize, TinError> {
        let (Some(&pc), Some(&pd)) = (self.points.get(c), self.points.get(d)) else { return Err(TinError::Degenerate) };
        let x = self.points.len();
        self.points.push(Vec3::new(pc.x + (pd.x - pc.x) * along, pc.y + (pd.y - pc.y) * along, pc.z + (pd.z - pc.z) * along));
        self.insert_on_edge(x, c, d)?;
        self.constrained.remove(&key(c, d));
        self.constrained.insert(key(c, x));
        self.constrained.insert(key(x, d));
        Ok(x)
    }

    /// Bowyer-Watson insertion of point `x`, which lies on edge `c`-`d`: the cavity starts at the
    /// triangles beside that edge and grows through neighbours whose circumcircle holds `x`, never
    /// across a constrained edge; it is then refilled with a fan from `x`.
    fn insert_on_edge(&mut self, x: usize, c: usize, d: usize) -> Result<(), TinError> {
        let px = self.xy(x);
        let by_edge = self.edge_map();
        let mut cavity: HashSet<usize> = [by_edge.get(&(c, d)), by_edge.get(&(d, c))].into_iter().flatten().copied().collect();
        if cavity.is_empty() {
            return Err(TinError::Degenerate);
        }
        let mut todo: Vec<usize> = cavity.iter().copied().collect();
        while let Some(t) = todo.pop() {
            let Some(&tri) = self.triangles.get(t) else { continue };
            for (u, v) in edges_of(tri) {
                if key(u, v) == key(c, d) || self.constrained.contains(&key(u, v)) {
                    continue;
                }
                if let Some(&n) = by_edge.get(&(v, u))
                    && !cavity.contains(&n)
                    && self.triangles.get(n).is_some_and(|&[p, q, r]| in_circumcircle(self.xy(p), self.xy(q), self.xy(r), px))
                {
                    cavity.insert(n);
                    todo.push(n);
                }
            }
        }
        let mut fan = Vec::new();
        for &t in &cavity {
            let Some(&tri) = self.triangles.get(t) else { continue };
            for (u, v) in edges_of(tri) {
                let inside = by_edge.get(&(v, u)).is_some_and(|n| cavity.contains(n));
                if inside && key(u, v) != key(c, d) && self.constrained.contains(&key(u, v)) {
                    return Err(TinError::Degenerate); // a constraint would be lost inside the cavity
                }
                if !inside && key(u, v) != key(c, d) {
                    fan.push(self.ccw(u, v, x).filter(|t| *t == [u, v, x]).ok_or(TinError::Degenerate)?);
                }
            }
        }
        self.replace(&cavity, fan);
        Ok(())
    }

    /// Remove the triangles the open segment `a`-`b` crosses (walking from `a` to `b`) and fill the
    /// polygon on each side of it.
    fn carve(&mut self, a: usize, b: usize) -> Result<(), TinError> {
        let (pa, ab) = (self.xy(a), self.xy(b) - self.xy(a));
        let len2 = ab.dot(ab);
        let side = |p: Vec2| {
            let s = ab.cross(p - pa);
            if s.abs() <= ON_SEGMENT * len2 { 0 } else { s.signum() as i8 }
        };
        // The triangle at `a` that `a`-`b` leaves through: `a`, then a vertex right of the segment,
        // then one left of it.
        let start = self.triangles.iter().enumerate().find_map(|(t, &tri)| {
            let [p, q, r] = rotate_to(tri, a)?;
            (p == a && side(self.xy(q)) < 0 && side(self.xy(r)) > 0).then_some((t, q, r))
        });
        let (t0, mut right, mut left) = start.ok_or(TinError::Degenerate)?;
        let by_edge = self.edge_map();
        let mut cavity: HashSet<usize> = HashSet::from([t0]);
        let (mut below, mut above) = (vec![right], vec![left]);
        loop {
            if self.constrained.contains(&key(right, left)) {
                return Err(TinError::Degenerate); // a crossing the arithmetic did not see
            }
            let &t = by_edge.get(&(left, right)).ok_or(TinError::Degenerate)?;
            if !cavity.insert(t) {
                return Err(TinError::Degenerate);
            }
            let w = self.triangles.get(t).and_then(|&tri| third(tri, left, right)).ok_or(TinError::Degenerate)?;
            if w == b {
                break;
            }
            match side(self.xy(w)) {
                1 => {
                    left = w;
                    above.push(w);
                }
                -1 => {
                    right = w;
                    below.push(w);
                }
                _ => return Err(TinError::Degenerate),
            }
        }
        let mut filled = Vec::with_capacity(below.len() + above.len());
        self.fill(a, &below, b, &mut filled)?;
        self.fill(a, &above, b, &mut filled)?;
        if filled.len() != cavity.len() {
            return Err(TinError::Degenerate);
        }
        self.replace(&cavity, filled);
        Ok(())
    }

    /// Triangulate the polygon `a`, `chain`..., `b` (every chain vertex on one side of `a`-`b`) by
    /// Delaunay vertices: the chain vertex whose circle with `a` and `b` holds no other forms a
    /// triangle with them, and the two sub-chains either side of it are filled the same way.
    fn fill(&self, a: usize, chain: &[usize], b: usize, out: &mut Vec<[usize; 3]>) -> Result<(), TinError> {
        let mut todo = vec![(a, chain, b)];
        while let Some((a, chain, b)) = todo.pop() {
            let Some(&first) = chain.first() else { continue };
            let (pa, pb) = (self.xy(a), self.xy(b));
            let mut pick = (0, first);
            for (i, &v) in chain.iter().enumerate().skip(1) {
                if in_circumcircle(pa, pb, self.xy(pick.1), self.xy(v)) {
                    pick = (i, v);
                }
            }
            out.push(self.ccw(a, b, pick.1).ok_or(TinError::Degenerate)?);
            let (before, after) = chain.split_at(pick.0);
            todo.push((a, before, pick.1));
            todo.push((pick.1, after.get(1..).unwrap_or(&[]), b));
        }
        Ok(())
    }

    /// The triangle `p`, `q`, `r` in counter-clockwise order, `None` when they are collinear.
    fn ccw(&self, p: usize, q: usize, r: usize) -> Option<[usize; 3]> {
        let (pp, pq, pr) = (self.xy(p), self.xy(q), self.xy(r));
        let area = (pq - pp).cross(pr - pp);
        let scale = (pq - pp).dot(pq - pp).max((pr - pp).dot(pr - pp));
        if !(area.abs() > 1e-12 * scale) {
            return None;
        }
        Some(if area > 0.0 { [p, q, r] } else { [p, r, q] })
    }

    /// Directed edge -> the triangle holding it.
    fn edge_map(&self) -> HashMap<(usize, usize), usize> {
        let mut m = HashMap::with_capacity(self.triangles.len() * 3);
        for (t, &tri) in self.triangles.iter().enumerate() {
            for e in edges_of(tri) {
                m.insert(e, t);
            }
        }
        m
    }

    fn replace(&mut self, gone: &HashSet<usize>, new: Vec<[usize; 3]>) {
        let mut t = 0;
        self.triangles.retain(|_| {
            t += 1;
            !gone.contains(&(t - 1))
        });
        self.triangles.extend(new);
    }
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

fn edges_of([p, q, r]: [usize; 3]) -> impl Iterator<Item = (usize, usize)> {
    [(p, q), (q, r), (r, p)].into_iter()
}

/// `tri` rotated to start at `v`, if `v` is one of its corners.
fn rotate_to([p, q, r]: [usize; 3], v: usize) -> Option<[usize; 3]> {
    if p == v {
        Some([p, q, r])
    } else if q == v {
        Some([q, r, p])
    } else if r == v {
        Some([r, p, q])
    } else {
        None
    }
}

/// The corner of `tri` that is neither `a` nor `b`.
fn third(tri: [usize; 3], a: usize, b: usize) -> Option<usize> {
    tri.into_iter().find(|&v| v != a && v != b)
}

/// Where segments `a`-`b` and `c`-`d` cross at a point inside both: the fractions along each.
fn crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(f64, f64)> {
    let (r, s) = (b - a, d - c);
    let denom = r.cross(s);
    if !(denom.abs() > 1e-12 * r.dot(r).sqrt() * s.dot(s).sqrt()) {
        return None; // parallel
    }
    let ac = c - a;
    let (t, u) = (ac.cross(s) / denom, ac.cross(r) / denom);
    let inside = |f: f64| f > PROPER && f < 1.0 - PROPER;
    (inside(t) && inside(u)).then_some((t, u))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Vec3> {
        vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0), Vec3::new(0.0, 10.0, 0.0)]
    }

    fn has_edge(tin: &Tin, a: usize, b: usize) -> bool {
        tin.triangles.iter().any(|&t| edges_of(t).any(|e| e == (a, b) || e == (b, a)))
    }

    fn twice_area(tin: &Tin, [a, b, c]: [usize; 3]) -> f64 {
        let p = |i: usize| tin.points.get(i).map_or(Vec2::new(f64::NAN, f64::NAN), |p| Vec2::new(p.x, p.y));
        (p(b) - p(a)).cross(p(c) - p(a))
    }

    fn area(tin: &Tin) -> f64 {
        tin.triangles.iter().map(|&t| twice_area(tin, t) * 0.5).sum()
    }

    /// Every triangle is counter-clockwise, every constrained pair is an edge, the pairs are sorted
    /// and distinct, every edge is shared by at most two triangles, and every edge that is not
    /// constrained is locally Delaunay.
    fn assert_sound(cdt: &Cdt) {
        let tin = &cdt.tin;
        for &t in &tin.triangles {
            assert!(twice_area(tin, t) > 0.0, "non-CCW triangle {t:?}");
        }
        for &[a, b] in &cdt.constrained {
            assert!(a < b && has_edge(tin, a, b), "constrained {a}-{b} is not an edge");
        }
        assert!(cdt.constrained.windows(2).all(|w| w.first() < w.get(1)), "constrained pairs not sorted and distinct");
        let mut by_edge: HashMap<(usize, usize), usize> = HashMap::new();
        for (i, &t) in tin.triangles.iter().enumerate() {
            for e in edges_of(t) {
                assert!(by_edge.insert(e, i).is_none(), "directed edge {e:?} in two triangles");
            }
        }
        let xy = |i: usize| Vec2::new(tin.points[i].x, tin.points[i].y);
        for (&(u, v), &t) in &by_edge {
            if cdt.constrained.contains(&[u.min(v), u.max(v)]) {
                continue;
            }
            let Some(&n) = by_edge.get(&(v, u)) else { continue };
            let [p, q, r] = tin.triangles[t];
            let w = third(tin.triangles[n], u, v).unwrap();
            assert!(!in_circumcircle(xy(p), xy(q), xy(r), xy(w)), "edge {u}-{v} is not locally Delaunay");
        }
    }

    #[test]
    fn cdt_keeps_a_breakline_the_plain_tin_would_cross() {
        let points = square();
        let plain = triangulate(&points).unwrap();
        let (a, b) = if has_edge(&plain, 0, 2) { (1, 3) } else { (0, 2) };
        let cdt = triangulate_constrained(&points, &[[a, b]]).unwrap();
        assert!(has_edge(&cdt.tin, a, b));
        assert_eq!(cdt.constrained, vec![[a.min(b), a.max(b)]]);
        assert_sound(&cdt);

        // A long breakline across a 9 x 9 grid crosses many triangles and touches no vertex.
        let mut grid: Vec<Vec3> = (0..81).map(|i| Vec3::new((i % 9) as f64, (i / 9) as f64, 0.0)).collect();
        grid.push(Vec3::new(0.3, 0.6, 1.0));
        grid.push(Vec3::new(7.7, 7.1, 1.0));
        let before = area(&triangulate(&grid).unwrap());
        let cdt = triangulate_constrained(&grid, &[[81, 82]]).unwrap();
        assert_eq!(cdt.constrained, vec![[81, 82]]);
        assert!((area(&cdt.tin) - before).abs() < 1e-9);
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_splits_crossing_constraints_at_their_intersection() {
        let mut points = square();
        for (i, z) in [(1, 2.0), (2, 4.0), (3, 6.0)] {
            points[i].z = z;
        }
        let cdt = triangulate_constrained(&points, &[[0, 2], [1, 3]]).unwrap();
        assert_eq!(cdt.tin.points.len(), 5);
        let x = cdt.tin.points[4];
        // On (5, 5), with z interpolated along the first constraint 0-2: (0 + 4) / 2.
        assert!((x.x - 5.0).abs() < 1e-9 && (x.y - 5.0).abs() < 1e-9 && (x.z - 2.0).abs() < 1e-9, "{x:?}");
        assert_eq!(cdt.constrained, vec![[0, 4], [1, 4], [2, 4], [3, 4]]);
        assert!(!has_edge(&cdt.tin, 0, 2) && !has_edge(&cdt.tin, 1, 3));
        assert_eq!(cdt.tin.triangles.len(), 4);
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_splits_a_constraint_at_a_point_lying_on_it() {
        let mut points = square();
        points.push(Vec3::new(5.0, 5.0, 2.0));
        let cdt = triangulate_constrained(&points, &[[0, 2]]).unwrap();
        assert_eq!(cdt.constrained, vec![[0, 4], [2, 4]]);
        assert_eq!(cdt.tin.points.len(), 5);
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_every_constrained_pair_is_an_edge() {
        // A 4 x 4 grid: the two long diagonals run through grid points and cross between them at
        // (1.5, 1.5); a third constraint repeats a piece of the first.
        let points: Vec<Vec3> = (0..16).map(|i| Vec3::new((i % 4) as f64, (i / 4) as f64, ((i % 4) + (i / 4)) as f64 * 0.1)).collect();
        let cdt = triangulate_constrained(&points, &[[0, 15], [3, 12], [5, 10]]).unwrap();
        assert_eq!(cdt.tin.points.len(), 17);
        assert_eq!(cdt.constrained, vec![[0, 5], [3, 6], [5, 16], [6, 16], [9, 12], [9, 16], [10, 15], [10, 16]]);
        assert_sound(&cdt);

        // A boundary vertex that coincides with a survey point uses the survey point.
        let mut dup = square();
        dup.extend(square());
        let cdt = triangulate_constrained(&dup, &[[4, 5], [5, 6], [6, 7], [7, 4], [4, 6]]).unwrap();
        assert_eq!(cdt.constrained, vec![[0, 1], [0, 2], [0, 3], [1, 2], [2, 3]]);
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_triangles_stay_counter_clockwise_and_cover_the_hull() {
        let mut points = square();
        points.push(Vec3::new(4.0, 6.0, 1.0));
        points.push(Vec3::new(9.0, 1.5, 1.0));
        let cdt = triangulate_constrained(&points, &[[0, 2], [1, 3], [4, 5], [0, 5]]).unwrap();
        assert!((area(&cdt.tin) - 100.0).abs() < 1e-7, "covered area {}", area(&cdt.tin));
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_refuses_out_of_range_constraint_indices() {
        let points = square();
        assert_eq!(triangulate_constrained(&points, &[[0, 9]]), Err(TinError::BadConstraint { index: 9 }));
        assert_eq!(triangulate_constrained(&points, &[[0, 1], [usize::MAX, 2]]), Err(TinError::BadConstraint { index: usize::MAX }));
        let too_many = vec![[0, 0]; MAX_TIN_POINTS * 4 + 1];
        assert_eq!(triangulate_constrained(&points, &too_many), Err(TinError::TooManyConstraints(MAX_TIN_POINTS * 4 + 1)));
        // Skipped, not refused: a constraint from a point to itself.
        assert_eq!(triangulate_constrained(&points, &[[2, 2]]).map(|c| c.constrained), Ok(vec![]));
    }

    #[test]
    fn cdt_empty_constraints_gives_plain_tin() {
        let points = square();
        let plain = triangulate(&points).unwrap();
        let cdt = triangulate_constrained(&points, &[]).unwrap();
        assert_eq!(cdt.tin.triangles.len(), plain.triangles.len());
        assert!(cdt.constrained.is_empty());
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_constraint_already_an_edge_is_recorded() {
        let points = square();
        let plain = triangulate(&points).unwrap();
        let (a, b) = (plain.triangles[0][0], plain.triangles[0][1]);
        let cdt = triangulate_constrained(&points, &[[a, b]]).unwrap();
        assert_eq!(cdt.constrained, vec![[a.min(b), a.max(b)]]);
        assert_eq!(cdt.tin.triangles.len(), plain.triangles.len());
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_refuses_nan_point() {
        let mut points = square();
        points[0] = Vec3::new(f64::NAN, 0.0, 0.0);
        assert_eq!(triangulate_constrained(&points, &[]), Err(TinError::NonFinitePoint { index: 0 }));
    }

    #[test]
    fn cdt_multiple_constraints_from_one_vertex() {
        let mut points = square();
        points.push(Vec3::new(5.0, 5.0, 0.0));
        let cdt = triangulate_constrained(&points, &[[4, 0], [4, 1], [4, 2], [4, 3]]).unwrap();
        assert_eq!(cdt.constrained.len(), 4);
        assert_sound(&cdt);
    }

    #[test]
    fn cdt_random_constraints_never_panic() {
        struct Lcg(u64);
        impl Lcg {
            fn next(&mut self) -> u64 {
                self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                self.0 >> 11
            }
            fn unit(&mut self) -> f64 {
                self.next() as f64 / (1u64 << 53) as f64
            }
        }
        for seed in 0..60u64 {
            let mut rng = Lcg(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x4d59_5df4_d0f3_3173);
            let n = 6 + (rng.next() % 40) as usize;
            // Odd seeds snap to a 1 m grid: collinear and cocircular points, constraints through them.
            let snap = |v: f64| if seed % 2 == 1 { v.round() } else { v };
            let points: Vec<Vec3> = (0..n).map(|_| Vec3::new(snap(rng.unit() * 12.0), snap(rng.unit() * 9.0), rng.unit() * 5.0)).collect();
            let Ok(plain) = triangulate(&points) else { continue };
            let constraints: Vec<[usize; 2]> = (0..1 + rng.next() % 14).map(|_| [(rng.next() as usize) % n, (rng.next() as usize) % n]).collect();
            let cdt = triangulate_constrained(&points, &constraints).unwrap_or_else(|e| panic!("seed {seed}: {e} for {constraints:?}"));
            assert!((area(&cdt.tin) - area(&plain)).abs() < 1e-6, "seed {seed}: area changed");
            assert_eq!(&cdt.tin.points[..n], &points[..], "seed {seed}: input points moved");
            assert_sound(&cdt);
        }
    }
}
