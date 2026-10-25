//! Section cuts (phase 7): where a plane cuts the scene, as closed rings for poché, and the section view (the cut plus
//! the hidden-line drawing of everything beyond it).
//!
//! A triangle crosses the plane when its corners are on both sides. A corner exactly on the plane counts as in front
//! (the kept side), so a face lying in the plane cuts nothing and no corner is counted twice. The point where an edge
//! crosses is computed once per welded edge, so the two triangles sharing that edge produce the same point and the
//! pieces chain into rings by edge, never by comparing floats. A closed solid gives closed rings; an open surface (a
//! ceiling) gives open chains, returned as lines.

use std::collections::{BTreeMap, BTreeSet};

use crate::edges::{dot, unit, weld};
use crate::hlr::{Cap, Line2, OrthoView, visible_lines_beyond};
use crate::{ArchId, Scene3d};

/// A plane in world space: the points `p` with `normal · p = offset`, `normal` a unit vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    normal: [f64; 3],
    offset: f64,
}

impl Plane {
    /// The plane through `point` across `normal`. `None` when the normal is zero or anything is not finite.
    pub fn new(point: [f64; 3], normal: [f64; 3]) -> Option<Self> {
        let normal = unit(normal)?;
        let offset = dot(normal, point);
        offset.is_finite().then_some(Plane { normal, offset })
    }

    /// The unit normal; the side it points to is the kept side (in front of the cut, beyond it for a viewer).
    pub fn normal(&self) -> [f64; 3] {
        self.normal
    }

    /// The signed distance of `p` from the plane, positive on the side the normal points to.
    pub fn distance(&self, p: [f64; 3]) -> f64 {
        dot(self.normal, p) - self.offset
    }
}

/// One closed ring where a solid is cut, on the sheet (counter-clockwise), with the object and material it cuts.
#[derive(Debug, Clone, PartialEq)]
pub struct CutRing {
    pub object: ArchId,
    pub material: String,
    pub points: Vec<[f64; 2]>,
}

impl CutRing {
    pub fn new(object: ArchId, material: &str, points: Vec<[f64; 2]>) -> Self {
        CutRing { object, material: material.to_string(), points }
    }

    /// The shoelace area on the sheet, positive for a counter-clockwise ring (every ring this module returns).
    /// A solid with a hole cut through gives an outer ring and a hole ring; fill them even-odd.
    pub fn signed_area(&self) -> f64 {
        ring_area(&self.points)
    }
}

/// What a plane cuts: closed rings for poché, and open chains (cut open surfaces) as lines.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionCut {
    pub rings: Vec<CutRing>,
    pub lines: Vec<Line2>,
}

/// A section view: the cut, and the visible lines of everything beyond it, both on the same sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionView {
    pub view: OrthoView,
    pub cut: SectionCut,
    pub beyond: Vec<Line2>,
}

fn ring_area(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Where the plane cuts each mesh of the scene, projected onto `view`'s sheet. Rings and lines come mesh by mesh.
pub fn section_cut(scene: &Scene3d, plane: &Plane, view: &OrthoView) -> SectionCut {
    let mut cut = SectionCut::default();
    for mesh in &scene.meshes {
        let w = weld(mesh);
        let d: Vec<f64> = w.points.iter().map(|&p| plane.distance(p)).collect();
        let front = |i: u32| d.get(i as usize).is_some_and(|&x| x >= 0.0);
        let mut next: BTreeMap<(u32, u32), Vec<(u32, u32)>> = BTreeMap::new();
        for t in &w.tris {
            let crossing: Vec<(u32, u32)> = [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
                .into_iter()
                .filter(|&(i, j)| front(i) != front(j))
                .map(|(i, j)| (i.min(j), i.max(j)))
                .collect();
            let [e, f] = crossing.as_slice() else { continue };
            next.entry(*e).or_default().push(*f);
            next.entry(*f).or_default().push(*e);
        }
        // The crossing point of the welded edge (i, j), i < j: one formula per edge, so neighbours agree exactly.
        let point = |(i, j): (u32, u32)| {
            let (pi, pj) = (w.point(i), w.point(j));
            let (di, dj) = (d.get(i as usize).copied().unwrap_or(0.0), d.get(j as usize).copied().unwrap_or(0.0));
            let t = di / (di - dj); // di and dj are on opposite sides, so di - dj is never zero
            let p = [pi[0] + t * (pj[0] - pi[0]), pi[1] + t * (pj[1] - pi[1]), pi[2] + t * (pj[2] - pi[2])];
            let s = view.project(p);
            [s[0], s[1]]
        };
        for (chain, closed) in chains(&next) {
            let mut pts: Vec<[f64; 2]> = Vec::with_capacity(chain.len());
            for &k in &chain {
                let p = point(k);
                if pts.last().is_none_or(|q| (q[0] - p[0]).abs() > 1e-12 || (q[1] - p[1]).abs() > 1e-12) {
                    pts.push(p);
                }
            }
            if closed {
                if pts.len() > 1 && pts.first() == pts.last() {
                    pts.pop();
                }
                let a = ring_area(&pts);
                if pts.len() < 3 || !a.is_finite() || a.abs() <= 1e-12 {
                    continue;
                }
                if a < 0.0 {
                    pts.reverse();
                }
                cut.rings.push(CutRing::new(mesh.object, &mesh.material, pts));
            } else {
                cut.lines.extend(pts.windows(2).map(|s| Line2::new(s[0], s[1], mesh.object)));
            }
        }
    }
    cut
}

/// Walk the crossing graph into chains, in a fixed order: open chains from their ends first, then closed rings.
/// Returns each chain with whether it closes on its start.
fn chains(next: &BTreeMap<(u32, u32), Vec<(u32, u32)>>) -> Vec<(Vec<(u32, u32)>, bool)> {
    let mut seen: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut out = Vec::new();
    let ends = next.iter().filter(|(_, n)| n.len() == 1).map(|(k, _)| *k);
    let starts: Vec<(u32, u32)> = ends.chain(next.keys().copied()).collect();
    for start in starts {
        if !seen.insert(start) {
            continue;
        }
        let mut chain = vec![start];
        let mut cur = start;
        loop {
            let mut options: Vec<(u32, u32)> = next.get(&cur).map(|n| n.iter().copied().filter(|k| !seen.contains(k)).collect()).unwrap_or_default();
            options.sort();
            let Some(&n) = options.first() else { break };
            seen.insert(n);
            chain.push(n);
            cur = n;
        }
        let closed = chain.len() >= 3 && next.get(&cur).is_some_and(|n| n.contains(&start));
        out.push((chain, closed));
    }
    out
}

/// The section view looking along the plane's normal with `up` up the sheet: the cut, and the visible lines of what
/// lies beyond it. The cut faces hide what is behind them, as the solid's own material would. `None` when `up` is
/// parallel to the normal or not finite.
pub fn section_view(scene: &Scene3d, plane: &Plane, up: [f64; 3]) -> Option<SectionView> {
    let view = OrthoView::new(plane.normal(), up)?;
    let cut = section_cut(scene, plane, &view);
    let mut by_object: BTreeMap<ArchId, Vec<Vec<[f64; 2]>>> = BTreeMap::new();
    for r in &cut.rings {
        by_object.entry(r.object).or_default().push(r.points.clone());
    }
    let depth = plane.offset; // the view looks along the unit normal, so a point's depth is normal · p
    let caps: Vec<Cap> = by_object.into_values().map(|rings| Cap { rings, depth }).collect();
    let beyond = visible_lines_beyond(scene, &view, Some(plane), &caps);
    Some(SectionView { view, cut, beyond })
}
