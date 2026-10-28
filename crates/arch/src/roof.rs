//! Outer-face footprint for the roof generator.
//!
//! Computes the CCW outer boundary of a building level from the arch model: each
//! exterior, non-divider wall contributes its outer-face edge (the long poché edge
//! facing away from the building interior). Collinear runs are merged into a single
//! edge attributed to the first wall. The result is a simple, finite, bounded ring
//! (≤ 256 edges) used by the roof skeleton (phase 4).

use cadcraft_doc::{ArchId, ArchModel, Roof, RoofDirective, RoofPlane, Wall};
use cadcraft_geom::Vec2;

/// One edge of the outer-face footprint, attributed to the wall it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct FootprintEdge {
    /// The wall this edge belongs to.
    pub wall: ArchId,
    /// Start point of the edge.
    pub a: Vec2,
    /// End point of the edge.
    pub b: Vec2,
}

/// The outer-face footprint of a building level: a CCW ring of points plus the
/// per-edge wall attribution.
#[derive(Clone, Debug, PartialEq)]
pub struct Footprint {
    /// CCW outer ring points, no repeated first point.
    pub outer: Vec<Vec2>,
    /// Edges in ring order; `edges[i]` runs `outer[i]` → `outer[(i+1) % n]`.
    pub edges: Vec<FootprintEdge>,
}

/// Build the outer-face footprint of `level`.
///
/// Returns `None` when there are fewer than 3 exterior non-divider walls on the level,
/// the merged ring is not simple, any point is non-finite, or the ring has more than
/// 256 edges.
pub fn outer_footprint(model: &ArchModel, level: u32) -> Option<Footprint> {
    // 1. Exterior, non-divider walls on the level.
    let candidates: Vec<&Wall> =
        model.walls.iter().filter(|w| w.exterior && !w.divider && w.base_level == level && w.start.is_finite() && w.end.is_finite()).collect();
    if candidates.len() < 3 {
        return None;
    }

    // 2. Centre-line graph: give each connected building its own centroid (centre of its bounds).
    let centre_lines: Vec<FootprintEdge> = candidates.iter().map(|w| FootprintEdge { wall: w.id, a: w.start, b: w.end }).collect();
    let centre_origin = ring_origin(&centre_lines);
    let kept = centre_lines.clone();
    let mut centroids: std::collections::BTreeMap<ArchId, Vec2> = std::collections::BTreeMap::new();
    for group in components(&kept, centre_origin) {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for e in group.iter().filter_map(|&i| kept.get(i)) {
            for p in [e.a, e.b] {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
        let centroid = Vec2::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        let group_edges: Vec<FootprintEdge> = group.iter().filter_map(|&i| kept.get(i).cloned()).collect();
        for e in &group_edges {
            centroids.insert(e.wall, centroid);
        }
        // A closed centre-line ring says exactly which side of each wall is inside, which
        // the bounds centre cannot for concave buildings: aim at a point on the inner side.
        if let Some(ring) = walk_ring(&group_edges, centre_origin) {
            let pts: Vec<Vec2> = ring.iter().map(|e| e.a).collect();
            let sign = if crate::poly::ring_area(&pts) >= 0.0 { 1.0 } else { -1.0 };
            for e in &ring {
                let d = e.b - e.a;
                let len = d.len();
                if len < 1e-12 {
                    continue;
                }
                let mid = Vec2::new((e.a.x + e.b.x) / 2.0, (e.a.y + e.b.y) / 2.0);
                let inner = d.perp() * (sign / len);
                centroids.insert(e.wall, mid + inner);
            }
        }
    }
    let walls: Vec<&Wall> = candidates.iter().copied().filter(|w| centroids.contains_key(&w.id)).collect();

    // 3. Solved poché rings. A dangling stub that does not run along another wall would
    // only distort the corner joins, so it is left out; an overlapping wall stays.
    let mut live = centre_lines.clone();
    prune_spurs(&mut live, centre_origin);
    let along_another = |w: &Wall| {
        let mid = Vec2::new((w.start.x + w.end.x) / 2.0, (w.start.y + w.end.y) / 2.0);
        candidates.iter().any(|o| {
            o.id != w.id && {
                let d = o.end - o.start;
                let l2 = d.dot(d);
                l2 > 1e-18 && {
                    let t = ((mid - o.start).dot(d) / l2).clamp(0.0, 1.0);
                    (o.start + d * t).dist(mid) < 1e-6
                }
            }
        })
    };
    let mut pruned = model.clone();
    pruned
        .walls
        .retain(|w| w.base_level == level && (!candidates.iter().any(|c| c.id == w.id) || live.iter().any(|e| e.wall == w.id) || along_another(w)));
    let shapes = crate::join::solve_joins(&pruned);

    // 4. One outer-face edge per wall.
    let mut edges: Vec<FootprintEdge> = Vec::with_capacity(walls.len());
    for wall in &walls {
        let Some(shape) = shapes.iter().find(|s| s.id == wall.id) else {
            continue;
        };
        let Some(centroid) = centroids.get(&wall.id) else {
            continue;
        };
        let Some((a, b)) = outer_face_edge(&shape.polygon, wall, centroid) else {
            continue;
        };
        if !a.is_finite() || !b.is_finite() || a.near(b, 1e-9) {
            continue;
        }
        edges.push(FootprintEdge { wall: wall.id, a, b });
    }
    if edges.len() < 3 {
        return None;
    }

    // 5. Candidate rings, one per connected component; keep the largest.
    let origin = ring_origin(&edges);
    let mut best: Option<(f64, Vec<FootprintEdge>)> = None;
    for group in components(&edges, origin) {
        let mut group_edges: Vec<FootprintEdge> = group.iter().filter_map(|&i| edges.get(i).cloned()).collect();
        prune_spurs(&mut group_edges, origin);
        let Some(mut ring) = walk_ring(&group_edges, origin) else {
            continue;
        };
        merge_straight(&mut ring, origin);
        if ring.len() < 3 {
            continue;
        }
        let pts: Vec<Vec2> = ring.iter().map(|e| e.a).collect();
        let area = crate::poly::ring_area(&pts).abs();
        if !area.is_finite() {
            continue;
        }
        let better = match &best {
            None => true,
            Some((ba, br)) => area > *ba || (area == *ba && ring.len() > br.len()),
        };
        if better {
            best = Some((area, ring));
        }
    }
    let (_, mut ring) = match best {
        Some(b) => b,
        // Overlapping or T-joined walls leave no clean cycle: join the edge starts in angular
        // order instead (the ring is still checked for simplicity below).
        None => (0.0, angular_ring(&edges)?),
    };

    // 6. Validate the ring.
    let raw: Vec<Vec2> = ring.iter().map(|e| e.a).collect();
    if raw.len() > 256 || raw.iter().any(|p| !p.is_finite()) || !crate::poly::is_simple(&raw) {
        return None;
    }

    // 7. Orient CCW, reversing the edge list to match.
    let outer = crate::poly::ensure_ccw(&raw);
    if crate::poly::ring_area(&raw) < 0.0 {
        ring.reverse();
        for e in &mut ring {
            std::mem::swap(&mut e.a, &mut e.b);
        }
    }
    Some(Footprint { outer, edges: ring })
}

/// Fallback ring for edge sets with no connected cycle: edges ordered by the angle of their
/// midpoint around the bounds centre, each re-run from its start to the next start.
fn angular_ring(edges: &[FootprintEdge]) -> Option<Vec<FootprintEdge>> {
    let origin = ring_origin(edges);
    let (mut max_x, mut max_y) = (origin.x, origin.y);
    for p in edges.iter().flat_map(|e| [e.a, e.b]) {
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let centre = Vec2::new((origin.x + max_x) / 2.0, (origin.y + max_y) / 2.0);
    let mut tagged: Vec<(f64, &FootprintEdge)> = edges
        .iter()
        .map(|e| {
            let v = Vec2::new((e.a.x + e.b.x) / 2.0, (e.a.y + e.b.y) / 2.0) - centre;
            (v.y.atan2(v.x), e)
        })
        .collect();
    tagged.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<FootprintEdge> = Vec::with_capacity(tagged.len());
    for (_, e) in tagged {
        if let Some(last) = merged.last_mut() {
            let (v1, v2) = (last.b - last.a, e.b - e.a);
            if last.b.near(e.a, 1e-6) && v1.cross(v2).abs() <= 1e-9 * (v1.len() * v2.len()) && v1.dot(v2) > 0.0 {
                last.b = e.b;
                continue;
            }
        }
        merged.push(e.clone());
    }
    let n = merged.len();
    if n < 3 {
        return None;
    }
    let starts: Vec<Vec2> = merged.iter().map(|e| e.a).collect();
    Some(merged.iter().enumerate().filter_map(|(i, e)| Some(FootprintEdge { wall: e.wall, a: e.a, b: *starts.get((i + 1) % n)? })).collect())
}

/// The long poché edge of `ring` that faces away from the building `centroid`.
///
/// A candidate edge must be roughly parallel to the wall's centre line and lie on the
/// outer side of that line (the side opposite the centroid). The most parallel, most
/// outer candidate wins.
fn outer_face_edge(ring: &[Vec2], wall: &Wall, centroid: &Vec2) -> Option<(Vec2, Vec2)> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let Some(dir) = crate::wall::wall_dir(wall) else {
        return None;
    };
    let centre_mid = Vec2::new((wall.start.x + wall.end.x) / 2.0, (wall.start.y + wall.end.y) / 2.0);
    let to_centroid = *centroid - centre_mid;
    let perp = dir.perp();
    // Outer side = the side of the centre line opposite the centroid.
    let outer_side = if to_centroid.dot(perp) > 0.0 { -perp } else { perp };

    let mut best: Option<(f64, (Vec2, Vec2))> = None;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        if !a.is_finite() || !b.is_finite() {
            continue;
        }
        let edge_vec = b - a;
        let len = edge_vec.len();
        if len < 1e-12 {
            continue;
        }
        let edge_dir = edge_vec / len;
        let parallel = edge_dir.dot(dir);
        let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let side = (mid - centre_mid).dot(outer_side);
        if parallel > 0.5 && side > 0.0 {
            let score = parallel * side;
            if best.map_or(true, |b| score > b.0) {
                best = Some((score, (a, b)));
            }
        }
    }
    best.map(|(_, e)| e)
}

/// Micron-quantised vertex key of `p`, relative to `origin` so georeferenced
/// coordinates do not saturate the integer range.
#[allow(dead_code)]
fn vkey(p: Vec2, origin: Vec2) -> (i64, i64) {
    (((p.x - origin.x) / 1e-6).round() as i64, ((p.y - origin.y) / 1e-6).round() as i64)
}

/// Bounds-minimum of all finite endpoints (the `origin` for [`vkey`]).
#[allow(dead_code)]
fn ring_origin(edges: &[FootprintEdge]) -> Vec2 {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for p in edges.iter().flat_map(|e| [e.a, e.b]) {
        if p.is_finite() {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
        }
    }
    if min_x.is_finite() && min_y.is_finite() { Vec2::new(min_x, min_y) } else { Vec2::ZERO }
}

/// Repeatedly drop degenerate edges and edges ending at a dangling (single-end) vertex.
/// Vertices with three or more ends are kept; `walk_ring` rejects them.
#[allow(dead_code)]
fn prune_spurs(edges: &mut Vec<FootprintEdge>, origin: Vec2) {
    loop {
        let mut counts: std::collections::BTreeMap<(i64, i64), usize> = std::collections::BTreeMap::new();
        for e in edges.iter() {
            for k in [vkey(e.a, origin), vkey(e.b, origin)] {
                *counts.entry(k).or_insert(0) += 1;
            }
        }
        let before = edges.len();
        edges.retain(|e| {
            let (ka, kb) = (vkey(e.a, origin), vkey(e.b, origin));
            ka != kb && counts.get(&ka).is_some_and(|&c| c >= 2) && counts.get(&kb).is_some_and(|&c| c >= 2)
        });
        if edges.len() == before {
            break;
        }
    }
}

/// Connected components of the edge graph, as edge-index groups sorted by first index.
#[allow(dead_code)]
fn components(edges: &[FootprintEdge], origin: Vec2) -> Vec<Vec<usize>> {
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while let Some(&p) = parent.get(i) {
            if p == i {
                break;
            }
            let gp = parent.get(p).copied().unwrap_or(p);
            if let Some(slot) = parent.get_mut(i) {
                *slot = gp;
            }
            i = gp;
        }
        i
    }
    let mut ids: std::collections::BTreeMap<(i64, i64), usize> = std::collections::BTreeMap::new();
    let mut ends = Vec::with_capacity(edges.len());
    for e in edges {
        let mut id_of = |k: (i64, i64)| {
            let next = ids.len();
            *ids.entry(k).or_insert(next)
        };
        let ia = id_of(vkey(e.a, origin));
        let ib = id_of(vkey(e.b, origin));
        ends.push((ia, ib));
    }
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for &(ia, ib) in &ends {
        let (ra, rb) = (find(&mut parent, ia), find(&mut parent, ib));
        if ra != rb {
            if let Some(slot) = parent.get_mut(rb) {
                *slot = ra;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, &(ia, _)) in ends.iter().enumerate() {
        let root = find(&mut parent, ia);
        groups.entry(root).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = groups.into_values().collect();
    out.sort_by_key(|g| g.first().copied().unwrap_or(usize::MAX));
    out
}

/// Walk `edges` (spurs already pruned) into one closed, consistently oriented ring.
#[allow(dead_code)]
fn walk_ring(edges: &[FootprintEdge], origin: Vec2) -> Option<Vec<FootprintEdge>> {
    if edges.len() < 3 {
        return None;
    }
    let mut at: std::collections::BTreeMap<(i64, i64), Vec<usize>> = std::collections::BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
        let (ka, kb) = (vkey(e.a, origin), vkey(e.b, origin));
        if ka == kb {
            return None;
        }
        at.entry(ka).or_default().push(i);
        at.entry(kb).or_default().push(i);
    }
    for list in at.values() {
        match list.as_slice() {
            [x, y] if x != y => {}
            _ => return None,
        }
    }
    let first = edges.first()?;
    let start = vkey(first.a, origin);
    let mut current = vkey(first.b, origin);
    let mut used = vec![false; edges.len()];
    *used.get_mut(0)? = true;
    let mut ring = vec![first.clone()];
    while current != start {
        let next = at.get(&current)?.iter().copied().find(|&i| !used.get(i).copied().unwrap_or(true))?;
        *used.get_mut(next)? = true;
        let e = edges.get(next)?;
        let oriented = if vkey(e.a, origin) == current { e.clone() } else { FootprintEdge { wall: e.wall, a: e.b, b: e.a } };
        current = vkey(oriented.b, origin);
        ring.push(oriented);
    }
    if used.iter().any(|u| !u) {
        return None;
    }
    Some(ring)
}

/// Drop vertices where two consecutive edges are collinear and continue in the same direction.
#[allow(dead_code)]
fn merge_straight(ring: &mut Vec<FootprintEdge>, _origin: Vec2) {
    const TOL: f64 = 1e-6;
    'outer: loop {
        let n = ring.len();
        if n <= 3 {
            return;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let (Some(prev), Some(next)) = (ring.get(i), ring.get(j)) else {
                return;
            };
            let (pa, pb) = (prev.a, prev.b);
            if (pb.x - next.a.x) * (pb.x - next.a.x) + (pb.y - next.a.y) * (pb.y - next.a.y) >= TOL * TOL {
                continue;
            }
            let v1 = pb - pa;
            let v2 = next.b - next.a;
            if v1.cross(v2).abs() <= 1e-9 * (v1.len() * v2.len()) && v1.dot(v2) > 0.0 {
                let nb = next.b;
                if let Some(p) = ring.get_mut(i) {
                    p.b = nb;
                }
                ring.remove(j);
                continue 'outer;
            }
        }
        return;
    }
}

/// Outset the footprint ring uniformly by `overhang` (outward) and re-attribute each
/// new edge to the nearest original footprint edge, keeping its wall attribution.
///
/// A non-finite or negative `overhang` returns [`RoofError::Degenerate`; a zero
/// `overhang` returns a clone of the input unchanged. A failed outset, a ring with
/// fewer than three points, a non-finite point, or a self-crossing ring maps to the
/// corresponding [`RoofError`] (never a partial footprint).
pub fn outset_footprint(fp: &Footprint, overhang: f64) -> Result<Footprint, RoofError> {
    if overhang < 0.0 || !overhang.is_finite() {
        return Err(RoofError::Degenerate);
    }
    if overhang == 0.0 {
        return Ok(fp.clone());
    }
    let region = match cadcraft_geom::outset(&fp.outer, &[], overhang) {
        Ok(region) => region,
        Err(_) => return Err(RoofError::Skeleton),
    };
    let outer = region.outer;
    if outer.len() < 3 {
        return Err(RoofError::Degenerate);
    }
    for p in &outer {
        if !p.is_finite() {
            return Err(RoofError::Degenerate);
        }
    }
    if !crate::poly::is_simple(&outer) {
        return Err(RoofError::BadFootprint);
    }
    let outer = crate::poly::ensure_ccw(&outer);
    let n = outer.len();
    let mut edges: Vec<FootprintEdge> = Vec::with_capacity(n);
    for i in 0..n {
        let a = outer[i];
        let b = outer[(i + 1) % n];
        let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let (mut best_j, mut best_d) = (0usize, f64::INFINITY);
        for (j, fe) in fp.edges.iter().enumerate() {
            let d = crate::poly::point_segment_dist(mid, fe.a, fe.b);
            if d < best_d {
                best_d = d;
                best_j = j;
            }
        }
        edges.push(FootprintEdge { wall: fp.edges[best_j].wall, a, b });
    }
    Ok(Footprint { outer, edges })
}

/// One slope per `fp.edges`.
///
/// A `RoofDirective::Gable` wall yields a vertical face (`f64::INFINITY`). Otherwise
/// the wall's `roof_pitch` (rise) over the roof's run, falling back to `roof.slope()`
/// when the pitch is non-finite or the run is not positive.
pub fn wall_slopes(model: &ArchModel, level: u32, fp: &Footprint, roof: &Roof) -> Vec<f64> {
    let default_slope = roof.slope();
    fp.edges
        .iter()
        .map(|edge| {
            let Some(wall) = model.wall(edge.wall) else {
                return default_slope;
            };
            if wall.roof_directive == RoofDirective::Gable {
                return f64::INFINITY;
            }
            let pitch = wall.roof_pitch.unwrap_or(roof.pitch_rise);
            if pitch.is_finite() && roof.pitch_run.is_finite() && roof.pitch_run > 0.0 { pitch / roof.pitch_run } else { default_slope }
        })
        .collect()
}

/// Highest level index owning an exterior wall, else the last level's index.
pub fn top_level(model: &ArchModel) -> u32 {
    let highest = model.walls.iter().filter(|w| w.exterior && !w.divider).map(|w| w.base_level).max();
    match highest {
        Some(h) => h,
        None => model.levels.last().map(|l| l.index).unwrap_or(0),
    }
}

/// Errors from building roof geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoofError {
    /// No exterior, non-divider walls on the level to build a roof from.
    NoWalls,
    /// The outer-face footprint could not be built (self-crossing, too many edges, non-simple).
    BadFootprint,
    /// The roof skeleton could not be built or failed its invariant checks.
    Skeleton,
    /// The resulting roof has more planes than [`cadcraft_doc::MAX_ROOF_PLANES`].
    TooComplex,
    /// A degenerate geometry: a non-finite node, or no usable roof planes.
    Degenerate,
}

impl std::fmt::Display for RoofError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                RoofError::NoWalls => "no exterior walls on the level to build a roof from",
                RoofError::BadFootprint => "the outer-face footprint could not be built",
                RoofError::Skeleton => "the roof skeleton could not be built or failed its checks",
                RoofError::TooComplex => "the roof has too many planes to be stored",
                RoofError::Degenerate => "the roof geometry is degenerate",
            }
        )
    }
}

impl std::error::Error for RoofError {}

/// The geometry of a roof built from the skeleton: the skeleton itself, the footprint it
/// was built from, the per-edge slopes, and the per-edge eave elevations.
///
/// `skel.nodes[i].time` is the rise above the eave (the skeleton is weighted by `1 / slope`),
/// so a node's elevation is `eave_z + time`.
#[derive(Clone, Debug, PartialEq)]
pub struct RoofGeom {
    /// The roof skeleton built from the footprint.
    pub skel: cadcraft_geom::Skeleton,
    /// The outer-face footprint the skeleton was built from.
    pub fp: Footprint,
    /// Per-edge slope (rise/run); `f64::INFINITY` for a gable end (zero-area face).
    pub slopes: Vec<f64>,
    /// Per-edge eave elevation: level elevation + owning wall height.
    pub eave_z: Vec<f64>,
}

/// Build the roof geometry for `level` from `model` and the seed `roof`.
///
/// A positive `roof.overhang` outsets the outer footprint before building the
/// skeleton, so the planes cover the eave line plus the overhang; each plane is
/// still attributed to the wall it sits over.
///
/// Returns [`RoofError::NoWalls`] when there are no exterior walls on the level,
/// [`RoofError::BadFootprint`] when the footprint cannot be built,
/// [`RoofError::Skeleton`] when the skeleton fails its build or invariant checks, and
/// [`RoofError::Degenerate`] when a node is non-finite.
pub fn roof_geometry(model: &ArchModel, level: u32, roof: &Roof) -> Result<RoofGeom, RoofError> {
    if !roof.pitch_rise.is_finite() || !roof.pitch_run.is_finite() || !roof.overhang.is_finite() || !roof.thickness.is_finite() {
        return Err(RoofError::Degenerate);
    }
    let fp = match outer_footprint(model, level) {
        Some(fp) => fp,
        None => {
            if !model.walls.iter().any(|w| w.exterior && w.base_level == level) {
                return Err(RoofError::NoWalls);
            }
            return Err(RoofError::BadFootprint);
        }
    };
    let fp = match outset_footprint(&fp, roof.overhang.max(0.0)) {
        Ok(fp) => fp,
        Err(e) => return Err(e),
    };

    let slopes = wall_slopes(model, level, &fp, roof);

    // If every edge is a gable end (infinite slope) there are no roof planes.
    if slopes.iter().all(|s| s.is_infinite()) {
        return Err(RoofError::Degenerate);
    }

    // roof_skeleton divides by slope to get weight; a 0.0 slope gives infinite weight
    // which the skeleton cannot handle. Use a tiny slope instead, then restore 0.0.
    let skel_slopes: Vec<f64> = slopes.iter().map(|s| if *s == 0.0 { 1e-6 } else { *s }).collect();

    let skel = match cadcraft_geom::roof_skeleton(&fp.outer, &[], &[skel_slopes]) {
        Ok(skel) => skel,
        Err(_) => return Err(RoofError::Skeleton),
    };

    if !cadcraft_geom::check(&skel, 1e-9).is_empty() {
        return Err(RoofError::Skeleton);
    }

    for node in &skel.nodes {
        if !node.pos.is_finite() || !node.time.is_finite() {
            return Err(RoofError::Degenerate);
        }
    }

    let elevation = model.levels.iter().find(|l| l.index == level).map(|l| l.elevation).unwrap_or(0.0);
    let eave_z = fp.edges.iter().map(|e| elevation + model.wall(e.wall).map(|w| w.height).unwrap_or(2.7)).collect();

    Ok(RoofGeom { skel, fp, slopes, eave_z })
}

/// Build a complete [`Roof`] for `level` from `model` and the seed `roof`.
///
/// Never returns a partial roof: on any failure it returns the corresponding
/// [`RoofError`] and the caller keeps the previous roof. The plane `edges` are left
/// empty (a later segment classifies them).
pub fn build_roof(model: &ArchModel, level: u32, roof: &Roof) -> Result<Roof, RoofError> {
    // A roof parameter that is not a finite number would leak a non-finite plane into
    // the model, so reject it up front.
    if !roof.pitch_rise.is_finite() || !roof.pitch_run.is_finite() || !roof.overhang.is_finite() || !roof.thickness.is_finite() {
        return Err(RoofError::Degenerate);
    }
    let geom = roof_geometry(model, level, roof)?;
    let skel = &geom.skel;
    let fp = &geom.fp;
    let slopes = &geom.slopes;
    let eave_z = &geom.eave_z;

    let mut planes = Vec::with_capacity(skel.faces.len());
    for face in &skel.faces {
        // Only ring 0 exists here (the footprint has no holes).
        if face.ring != 0 {
            continue;
        }
        // Face `f` uses ring edge `f.edge` of ring `f.ring`; the footprint ring is already
        // cleaned of straight-through vertices, so this indexes `fp.edges`/`slopes` directly.
        let ring_points: Vec<Vec2> = face.nodes.iter().map(|&i| skel.nodes[i].pos).collect();
        for p in &ring_points {
            if !p.is_finite() {
                return Err(RoofError::Degenerate);
            }
        }
        // Drop consecutive duplicates and a trailing point equal to the first.
        let mut ring: Vec<Vec2> = Vec::with_capacity(ring_points.len());
        for &p in &ring_points {
            if ring.is_empty() || !ring.last().map_or(true, |q| q.near(p, 1e-9)) {
                ring.push(p);
            }
        }
        if ring.len() > 1 && ring.last().map_or(false, |q| q.near(ring[0], 1e-9)) {
            ring.pop();
        }
        let ring = crate::poly::ensure_ccw(&ring);

        let projected_area = crate::poly::ring_area(&ring).abs();
        if projected_area < 1e-9 {
            // A gable end (weight 0) sweeps a zero-area face; it is not a roof plane.
            continue;
        }
        if slopes[face.edge].is_infinite() {
            // A gable end (infinite slope) is a vertical face; it is not a roof plane.
            continue;
        }

        let edge = &fp.edges[face.edge];
        planes.push(RoofPlane {
            ring,
            slope: slopes[face.edge],
            projected_area,
            eave_wall: Some(edge.wall),
            base_edge: [edge.a.x, edge.a.y, edge.b.x, edge.b.y],
            thickness: roof.thickness,
            overhang: roof.overhang.max(0.0),
            eave_z: eave_z[face.edge],
            ..Default::default()
        });
    }

    if planes.is_empty() {
        return Err(RoofError::Degenerate);
    }
    if planes.len() > cadcraft_doc::MAX_ROOF_PLANES {
        return Err(RoofError::TooComplex);
    }

    let edges = crate::roofedge::roof_edges_geom(&geom);

    Ok(Roof {
        id: roof.id,
        level,
        pitch_rise: roof.pitch_rise,
        pitch_run: roof.pitch_run,
        overhang: roof.overhang,
        thickness: roof.thickness,
        fascia: roof.fascia,
        soffit: roof.soffit,
        manual: roof.manual,
        hand_edited: roof.hand_edited,
        planes,
        edges,
        ..Roof::default()
    })
}

/// The apex elevation of a single roof plane: the eave elevation plus the rise to the
/// ridge. The ring's base edge is the overhang outline, so the run is reduced by the
/// overhang before multiplying by the slope (the eave — where `eave_z` applies — lies
/// `overhang` inside from the outline). Returns 0.0 for an empty ring or a
/// zero-length base edge.
pub fn plane_apex_z(plane: &RoofPlane) -> f64 {
    if plane.ring.is_empty() {
        return 0.0;
    }
    let a = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let b = Vec2::new(plane.base_edge[2], plane.base_edge[3]);
    let ab = b - a;
    if ab.len() < 1e-12 {
        return 0.0;
    }
    let run = plane.ring.iter().map(|p| (ab.cross(*p - a)).abs() / ab.len()).fold(0.0, f64::max);
    plane.eave_z + (run - plane.overhang) * plane.slope
}

/// The apex elevation of a whole roof: the maximum over its planes, or 0.0 when it
/// has none.
pub fn roof_apex_z(roof: &Roof) -> f64 {
    roof.planes.iter().map(plane_apex_z).fold(0.0, f64::max)
}

/// Sum of the finite projected areas of a roof's planes.
pub fn projected_area_sum(roof: &Roof) -> f64 {
    roof.planes.iter().filter(|p| p.projected_area.is_finite()).map(|p| p.projected_area).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{ArchModel, Level};
    use cadcraft_geom::Vec2;

    fn fe(i: u32, a: (f64, f64), b: (f64, f64)) -> FootprintEdge {
        FootprintEdge { wall: ArchId(i), a: Vec2::new(a.0, a.1), b: Vec2::new(b.0, b.1) }
    }

    fn square_edges() -> Vec<FootprintEdge> {
        vec![fe(0, (0.0, 0.0), (4.0, 0.0)), fe(1, (4.0, 0.0), (4.0, 4.0)), fe(2, (4.0, 4.0), (0.0, 4.0)), fe(3, (0.0, 4.0), (0.0, 0.0))]
    }

    #[test]
    fn vkey_quantises_to_microns() {
        assert_eq!(vkey(Vec2::new(1.0000004, 2.0), Vec2::ZERO), (1_000_000, 2_000_000));
        assert_eq!(vkey(Vec2::new(1.0000004 - 1e-7, 2.0), Vec2::ZERO), (1_000_000, 2_000_000));
    }

    #[test]
    fn walk_ring_orders_a_closed_ring() {
        let s = square_edges();
        let scrambled = vec![fe(2, (0.0, 4.0), (4.0, 4.0)), s[0].clone(), fe(3, (0.0, 0.0), (0.0, 4.0)), s[1].clone()];
        let origin = ring_origin(&scrambled);
        let ring = walk_ring(&scrambled, origin);
        let ring = ring.expect("closed ring");
        assert_eq!(ring.len(), 4);
        for i in 0..4 {
            assert!(ring[i].b.near(ring[(i + 1) % 4].a, 1e-6));
        }
        let pts: Vec<Vec2> = ring.iter().map(|e| e.a).collect();
        assert!(crate::poly::ring_area(&pts).abs() > 1.0);
    }

    #[test]
    fn walk_ring_prunes_a_dangling_stub() {
        let mut edges = square_edges();
        edges.push(fe(4, (4.0, 0.0), (6.0, 0.0)));
        let origin = ring_origin(&edges);
        prune_spurs(&mut edges, origin);
        assert_eq!(edges.len(), 4);
        assert!(walk_ring(&edges, origin).is_some());
    }

    #[test]
    fn walk_ring_rejects_disconnected_rings() {
        let mut edges = square_edges();
        for (i, e) in square_edges().into_iter().enumerate() {
            let sh = Vec2::new(10.0, 0.0);
            edges.push(FootprintEdge { wall: ArchId(4 + i as u32), a: e.a + sh, b: e.b + sh });
        }
        let origin = ring_origin(&edges);
        let comps = components(&edges, origin);
        assert_eq!(comps.len(), 2);
        assert!(walk_ring(&edges, origin).is_none());
    }

    #[test]
    fn merge_straight_merges_collinear_runs() {
        let mut ring = vec![
            fe(0, (0.0, 0.0), (2.0, 0.0)),
            fe(0, (2.0, 0.0), (4.0, 0.0)),
            fe(1, (4.0, 0.0), (4.0, 4.0)),
            fe(2, (4.0, 4.0), (0.0, 4.0)),
            fe(3, (0.0, 4.0), (0.0, 0.0)),
        ];
        merge_straight(&mut ring, Vec2::ZERO);
        assert_eq!(ring.len(), 4);
        assert!(ring[0].a.near(Vec2::new(0.0, 0.0), 1e-9));
        assert!(ring[0].b.near(Vec2::new(4.0, 0.0), 1e-9));
    }

    /// A 10 × 8 rectangular house, 0.2 m walls, level 0.
    fn rectangular_house(model: &mut ArchModel) {
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
    }

    fn ring_extents(fp: &Footprint) -> (f64, f64, f64, f64) {
        let xs: Vec<f64> = fp.outer.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = fp.outer.iter().map(|p| p.y).collect();
        (
            xs.iter().cloned().fold(f64::INFINITY, f64::min),
            ys.iter().cloned().fold(f64::INFINITY, f64::min),
            xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    #[test]
    fn footprint_of_the_phase1_house_is_10_2_by_8_2() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(fp.outer.len(), 4);
        assert_eq!(fp.edges.len(), 4);
        let (min_x, min_y, max_x, max_y) = ring_extents(&fp);
        assert!((max_x - min_x - 10.2).abs() < 1e-6, "width {} != 10.2", max_x - min_x);
        assert!((max_y - min_y - 8.2).abs() < 1e-6, "height {} != 8.2", max_y - min_y);
        assert!(crate::poly::ring_area(&fp.outer) > 0.0, "ring must be CCW");
    }

    #[test]
    fn footprint_merges_collinear_edges() {
        let mut model = ArchModel::default();
        // Split the bottom wall into two collinear runs.
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(fp.edges.len(), 4, "the two bottom runs must merge");
        let e = &fp.edges.iter().find(|e| (e.b.x - e.a.x).abs() > 5.0).unwrap();
        assert!((e.b.x - e.a.x - 10.2).abs() < 1e-6, "merged run length {} != 10.2", e.b.x - e.a.x);
    }

    #[test]
    fn footprint_rejects_self_crossing() {
        // A bowtie: two crossing diagonals plus two verticals produce a
        // non-simple outer-face ring.
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 20.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 20.0), 0.2));
        assert!(outer_footprint(&model, 0).is_none());

        // Fewer than 3 exterior walls is also rejected.
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        assert!(outer_footprint(&model, 0).is_none());
    }

    #[test]
    fn footprint_rejects_over_256_edges() {
        let mut model = ArchModel::default();
        // 257 walls around a circle → 257 edges > 256 cap.
        for i in 0..257 {
            let a = (i as f64) * (std::f64::consts::PI * 2.0 / 257.0);
            let b = ((i + 1) as f64) * (std::f64::consts::PI * 2.0 / 257.0);
            model.push_wall(Wall::new(Vec2::new(10.0 * a.cos(), 10.0 * a.sin()), Vec2::new(10.0 * b.cos(), 10.0 * b.sin()), 0.2));
        }
        assert!(outer_footprint(&model, 0).is_none());
    }

    #[test]
    fn gable_directive_gives_infinite_slope() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let roof = Roof::default();
        // Set the right wall (id 1) to Gable.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let fp = outer_footprint(&model, 0).unwrap();
        let slopes = wall_slopes(&model, 0, &fp, &roof);
        assert_eq!(slopes.len(), 4);
        assert!(slopes.iter().any(|s| s.is_infinite() && *s > 0.0), "expected an INFINITY slope for the gable wall");
        let default = roof.slope();
        assert!(slopes.iter().all(|s| *s == default || s.is_infinite()), "non-gable slopes must equal the roof default");
    }

    #[test]
    fn top_level_picks_the_highest_level_with_walls() {
        let mut model = ArchModel::default();
        // A wall on level 0 and a wall on level 2.
        let w0 = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let w2 = model.push_wall(Wall::new(Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0), 0.2));
        // Move the second wall to level 2.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == w2) {
            w.base_level = 2;
        }
        // Ensure a level 2 exists.
        if model.level(2).is_none() {
            model.levels.push(Level { index: 2, name: "Level 3".into(), elevation: 2.7, ..Level::default() });
        }
        assert_eq!(top_level(&model), 2);

        // No exterior walls → last level (0).
        let empty = ArchModel::default();
        assert_eq!(top_level(&empty), 0);
    }

    /// A fixture roof: no overhang, no thickness, default 6/12 pitch.
    fn test_roof() -> Roof {
        Roof { overhang: 0.0, thickness: 0.0, ..Roof::default() }
    }

    /// A 10 × 8 rectangular house, 0.2 m walls, all 2.4 m high, level 0.
    fn house_10_8(model: &mut ArchModel) {
        rectangular_house(model);
        for w in model.walls.iter_mut() {
            w.height = 2.4;
        }
    }

    fn closed_ring_walls(model: &mut ArchModel, pts: &[(f64, f64)]) {
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let mut w = Wall::new(Vec2::new(a.0, a.1), Vec2::new(b.0, b.1), 0.2);
            w.height = 2.4;
            model.push_wall(w);
        }
    }

    fn assert_closed_simple(fp: &Footprint) {
        assert!(crate::poly::is_simple(&fp.outer));
        let n = fp.edges.len();
        assert_eq!(n, fp.outer.len());
        for i in 0..n {
            assert!(fp.edges[i].b.near(fp.edges[(i + 1) % n].a, 1e-6), "edge {i} does not meet the next");
        }
    }

    #[test]
    fn t_footprint_is_a_closed_simple_ring() {
        let mut model = ArchModel::default();
        closed_ring_walls(&mut model, &[(0.0, 0.0), (12.0, 0.0), (12.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0), (4.0, 4.0), (0.0, 4.0)]);
        let fp = outer_footprint(&model, 0).expect("footprint");
        assert_eq!(fp.outer.len(), 8);
        assert_closed_simple(&fp);
    }

    #[test]
    fn l_footprint_is_a_closed_simple_ring() {
        let mut model = ArchModel::default();
        closed_ring_walls(&mut model, &[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (6.0, 8.0), (6.0, 12.0), (0.0, 12.0)]);
        let fp = outer_footprint(&model, 0).expect("footprint");
        assert_eq!(fp.outer.len(), 6);
        assert_closed_simple(&fp);
    }

    #[test]
    fn footprint_rejects_disconnected_walls() {
        let mut model = ArchModel::default();
        closed_ring_walls(&mut model, &[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        closed_ring_walls(&mut model, &[(20.0, 0.0), (24.0, 0.0), (24.0, 4.0), (20.0, 4.0)]);
        let fp = outer_footprint(&model, 0).expect("one building's ring");
        assert_eq!(fp.outer.len(), 4);
    }

    #[test]
    fn rectangle_footprint_is_the_outer_face_ring() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let fp = outer_footprint(&model, 0).expect("footprint");
        assert_eq!(fp.outer.len(), 4);
        let min_x = fp.outer.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = fp.outer.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = fp.outer.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = fp.outer.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x + 0.1).abs() < 1e-6 && (max_x - 10.1).abs() < 1e-6);
        assert!((min_y + 0.1).abs() < 1e-6 && (max_y - 8.1).abs() < 1e-6);
        assert!(crate::poly::ring_area(&fp.outer) > 0.0);
        assert_closed_simple(&fp);
    }

    #[test]
    fn footprint_prunes_a_dangling_exterior_stub() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let mut stub = Wall::new(Vec2::new(10.0, 0.0), Vec2::new(12.0, 0.0), 0.2);
        stub.height = 2.4;
        model.push_wall(stub);
        let fp = outer_footprint(&model, 0).expect("footprint");
        assert_eq!(fp.outer.len(), 4);
    }

    #[test]
    fn hip_roof_has_four_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 4, "a hip roof over a rectangle should have 4 planes");
    }

    #[test]
    fn hip_ridge_is_at_the_expected_height() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        // 2.4 + 4.1 * 0.5 = 4.45
        assert!((roof_apex_z(&roof) - 4.45).abs() < 1e-6, "roof_apex_z {} != 4.45", roof_apex_z(&roof));
        // The ridge nodes sit at y ≈ 4.0 and span x from 4.0 to 6.0.
        let geom = roof_geometry(&model, 0, &test_roof()).unwrap();
        let ridge_nodes: Vec<&cadcraft_geom::SkeletonNode> = geom.skel.nodes.iter().filter(|n| (n.pos.y - 4.0).abs() < 1e-6).collect();
        assert!(!ridge_nodes.is_empty(), "no ridge nodes found");
        let min_x = ridge_nodes.iter().map(|n| n.pos.x).fold(f64::INFINITY, f64::min);
        let max_x = ridge_nodes.iter().map(|n| n.pos.x).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - 4.0).abs() < 1e-6, "ridge min_x {} != 4.0", min_x);
        assert!((max_x - 6.0).abs() < 1e-6, "ridge max_x {} != 6.0", max_x);
    }

    #[test]
    fn ridge_height_tracks_pitch() {
        let pitches = [(4.0, 12.0, 3.766667), (6.0, 12.0, 4.45), (12.0, 12.0, 6.5)];
        for (rise, run, expected) in pitches {
            let mut model = ArchModel::default();
            house_10_8(&mut model);
            let roof = Roof { pitch_rise: rise, pitch_run: run, overhang: 0.0, thickness: 0.0, ..Roof::default() };
            let built = build_roof(&model, 0, &roof).unwrap();
            let apex = roof_apex_z(&built);
            assert!((apex - expected).abs() < 1e-6, "pitch {rise}/{run}: apex {} != {}", apex, expected);
        }
    }

    #[test]
    fn square_gives_a_pyramid() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 4.0), Vec2::new(0.0, 0.0), 0.2));
        for w in model.walls.iter_mut() {
            w.height = 2.4;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 4, "a 4x4 square hip roof should have 4 triangular planes");
        // All four planes meet at the centre (2.0, 2.0).
        for plane in &roof.planes {
            assert!(
                plane.ring.iter().any(|p| (p.x - 2.0).abs() < 1e-6 && (p.y - 2.0).abs() < 1e-6),
                "a plane ring should meet at the centre: {:?}",
                plane.ring
            );
        }
    }

    #[test]
    fn two_gables_give_two_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Left wall (x=0, wall 3) and right wall (x=10, wall 1) become gable ends.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(3)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 2, "two gable ends should give 2 planes");
        // The two planes' base edges span x from -0.1 to 10.1.
        let min_x = roof.planes.iter().flat_map(|p| [p.base_edge[0], p.base_edge[2]]).fold(f64::INFINITY, f64::min);
        let max_x = roof.planes.iter().flat_map(|p| [p.base_edge[0], p.base_edge[2]]).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - (-0.1)).abs() < 1e-6, "base edge min_x {} != -0.1", min_x);
        assert!((max_x - 10.1).abs() < 1e-6, "base edge max_x {} != 10.1", max_x);
    }

    #[test]
    fn gable_plus_hip_gives_three_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Only the right wall (x=10, wall 1) is a gable end.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 3, "one gable end should give 3 planes");
    }

    #[test]
    fn every_plane_slope_equals_its_pitch() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        for plane in &roof.planes {
            let Some(wall) = plane.eave_wall.as_ref().and_then(|id| model.wall(*id)) else {
                continue;
            };
            let pitch = wall.roof_pitch.unwrap_or(roof.pitch_rise);
            let run = roof.pitch_run;
            let expected = if pitch.is_finite() && run.is_finite() && run > 0.0 { pitch / run } else { roof.pitch_rise / roof.pitch_run };
            assert!((plane.slope - expected).abs() < 1e-12, "plane slope {} != pitch {} for wall {:?}", plane.slope, expected, wall.id);
        }
    }

    #[test]
    fn projected_areas_sum_to_the_footprint() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        let sum = projected_area_sum(&roof);
        assert!((sum - 83.64).abs() < 1e-6, "projected area sum {} != 83.64 (10.2 x 8.2)", sum);
    }

    #[test]
    fn overhang_projected_area_is_103_04() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        assert!(
            (projected_area_sum(&roof) - 103.04).abs() < 1e-6,
            "overhang projected area sum {} != 103.04 (11.2 x 9.2)",
            projected_area_sum(&roof),
        );
    }

    #[test]
    fn overhang_grows_the_footprint_not_the_apex() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let no_oh = build_roof(&model, 0, &Roof { overhang: 0.0, thickness: 0.0, ..Roof::default() }).unwrap();
        let oh = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        // Overhang grows the eaves but not the plane count.
        assert_eq!(no_oh.planes.len(), oh.planes.len(), "overhang must not change the plane count");
        assert_eq!(no_oh.planes.len(), 4, "hip roof should keep 4 planes with overhang");
        // The apex lies inside the wall line; the overhang only grows the eaves.
        assert!((roof_apex_z(&no_oh) - 4.45).abs() < 1e-6, "no-overhang apex {} != 4.45", roof_apex_z(&no_oh));
        assert!((roof_apex_z(&oh) - 4.45).abs() < 1e-6, "overhang apex {} != 4.45", roof_apex_z(&oh));
        assert!(
            projected_area_sum(&oh) > projected_area_sum(&no_oh),
            "overhang must grow the footprint: {} <= {}",
            projected_area_sum(&oh),
            projected_area_sum(&no_oh),
        );
    }

    #[test]
    fn outset_edges_are_attributed_to_the_nearest_wall() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let fp = outer_footprint(&model, 0).unwrap();
        let outset = outset_footprint(&fp, 0.5).unwrap();
        assert_eq!(outset.outer.len(), outset.edges.len(), "each outset edge must be a point pair");
        assert_eq!(outset.edges.len(), fp.edges.len(), "a rectangular outset must keep one edge per wall");
        // Every attribute is a real wall on the level.
        for e in &outset.edges {
            assert!(model.wall(e.wall).is_some(), "outset edge attributed to an unknown wall {:?}", e.wall);
        }
        // The outset ring preserves order; each side is nearest to its own footprint side,
        // so per-index the attribution equals the footprint's (the "nearest" rule).
        for (i, e) in outset.edges.iter().enumerate() {
            assert_eq!(e.wall, fp.edges[i].wall, "outset edge {i} attributed to wall {:?}, expected {:?} (nearest rule)", e.wall, fp.edges[i].wall,);
        }
    }

    #[test]
    fn zero_overhang_is_unchanged() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(outset_footprint(&fp, 0.0).unwrap(), fp, "zero overhang must return the footprint unchanged");
        let no_oh = build_roof(&model, 0, &Roof { overhang: 0.0, thickness: 0.0, ..Roof::default() }).unwrap();
        assert!(
            (projected_area_sum(&no_oh) - 83.64).abs() < 1e-6,
            "zero-overhang projected area sum {} != 83.64 (10.2 x 8.2)",
            projected_area_sum(&no_oh),
        );
    }

    #[test]
    fn bad_footprint_returns_err_not_a_partial_roof() {
        // No exterior walls on the level → NoWalls, no roof.
        let mut model = ArchModel::default();
        let mut w = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        w.exterior = false;
        w.divider = true;
        model.push_wall(w);
        assert!(matches!(build_roof(&model, 0, &test_roof()), Err(RoofError::NoWalls)));

        // A self-crossing (bowtie) ring → error, no partial roof.
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 20.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 20.0), 0.2));
        assert!(matches!(build_roof(&model, 0, &test_roof()), Err(_)));
    }

    #[test]
    fn outset_footprint_rejects_nan_overhang() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let fp = outer_footprint(&model, 0).unwrap();
        assert!(matches!(outset_footprint(&fp, f64::NAN), Err(RoofError::Degenerate)));
    }

    #[test]
    fn outset_footprint_rejects_negative_overhang() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let fp = outer_footprint(&model, 0).unwrap();
        assert!(matches!(outset_footprint(&fp, -0.1), Err(RoofError::Degenerate)));
    }

    #[test]
    fn overhang_with_gable_ends_keeps_infinite_slope() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(3)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let roof = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        assert_eq!(roof.planes.len(), 2, "two gable ends with overhang should give 2 planes");
        for plane in &roof.planes {
            assert!(plane.slope.is_finite(), "gable end planes should have finite slope");
        }
    }

    #[test]
    fn overhang_with_per_wall_pitch() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(0)) {
            w.roof_pitch = Some(4.0);
        }
        let roof = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, pitch_rise: 6.0, pitch_run: 12.0, ..Roof::default() }).unwrap();
        let south_plane = roof.planes.iter().find(|p| p.eave_wall == Some(ArchId(0))).unwrap();
        assert!((south_plane.slope - 4.0 / 12.0).abs() < 1e-12, "south wall plane slope {} != 4/12", south_plane.slope);
        for plane in &roof.planes {
            if plane.eave_wall != Some(ArchId(0)) {
                assert!((plane.slope - 6.0 / 12.0).abs() < 1e-12, "non-south plane slope {} != 6/12", plane.slope);
            }
        }
    }

    #[test]
    fn overhang_gable_roof_plane_count_unchanged() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(3)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let no_oh = build_roof(&model, 0, &Roof { overhang: 0.0, thickness: 0.0, ..Roof::default() }).unwrap();
        let oh = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        assert_eq!(no_oh.planes.len(), oh.planes.len(), "overhang must not change the plane count for a gable roof");
        assert_eq!(oh.planes.len(), 2, "gable roof should have 2 planes");
    }

    #[test]
    fn overhang_eave_z_uses_wall_height() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(0)) {
            w.height = 3.0;
        }
        let roof = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        let south_plane = roof.planes.iter().find(|p| p.eave_wall == Some(ArchId(0))).unwrap();
        assert!((south_plane.eave_z - 3.0).abs() < 1e-9, "eave_z {} != 3.0 for wall 0", south_plane.eave_z);
        for plane in &roof.planes {
            if plane.eave_wall != Some(ArchId(0)) {
                assert!((plane.eave_z - 2.4).abs() < 1e-9, "eave_z {} != 2.4 for wall {:?}", plane.eave_z, plane.eave_wall);
            }
        }
    }

    #[test]
    fn huge_overhang_still_builds() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &Roof { overhang: 10.0, thickness: 0.0, ..Roof::default() }).unwrap();
        assert_eq!(roof.planes.len(), 4, "a huge overhang should still give 4 planes");
        let sum = projected_area_sum(&roof);
        assert!((sum - 851.64).abs() < 1e-6, "huge overhang projected area sum {} != 851.64", sum);
    }

    #[test]
    fn overhang_with_no_level_defined() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        for plane in &roof.planes {
            assert!((plane.eave_z - 2.4).abs() < 1e-9, "eave_z {} != 2.4 when no level defined", plane.eave_z);
        }
    }

    #[test]
    fn overhang_on_l_shaped_footprint() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(6.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(6.0, 8.0), Vec2::new(6.0, 12.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(6.0, 12.0), Vec2::new(0.0, 12.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 12.0), Vec2::new(0.0, 0.0), 0.2));
        for w in model.walls.iter_mut() {
            w.height = 2.4;
        }
        let roof = build_roof(&model, 0, &Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() }).unwrap();
        assert!(!roof.planes.is_empty(), "L-shaped roof should have at least one plane");
        assert!(projected_area_sum(&roof) > 0.0, "L-shaped roof should have positive projected area");
    }

    #[test]
    fn outset_footprint_merges_collinear_edges() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(fp.edges.len(), 4, "the two bottom runs must merge");
        let outset = outset_footprint(&fp, 0.5).unwrap();
        assert_eq!(outset.edges.len(), 4, "the outset must keep 4 edges");
    }

    #[test]
    fn roof_with_nan_wall_coords_is_degenerate() {
        let mut model = ArchModel::default();
        let mut w = Wall::new(Vec2::new(f64::NAN, f64::NAN), Vec2::new(f64::NAN, f64::NAN), 0.2);
        w.height = 2.4;
        model.push_wall(w);
        assert!(build_roof(&model, 0, &test_roof()).is_err(), "a house with only NaN walls should fail to build a roof");
    }

    #[test]
    fn roof_with_per_wall_pitch_zero_is_flat() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(0)) {
            w.roof_pitch = Some(0.0);
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        let south_plane = roof.planes.iter().find(|p| p.eave_wall == Some(ArchId(0))).unwrap();
        assert!((south_plane.slope - 0.0).abs() < 1e-12, "south wall plane slope {} != 0.0", south_plane.slope);
    }

    #[test]
    fn roof_with_pitch_run_zero_uses_default() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = Roof { pitch_rise: 6.0, pitch_run: 0.0, overhang: 0.0, thickness: 0.0, ..Roof::default() };
        let built = build_roof(&model, 0, &roof).unwrap();
        for plane in &built.planes {
            assert!((plane.slope - 0.5).abs() < 1e-12, "plane slope {} != 0.5 (default)", plane.slope);
        }
    }

    #[test]
    fn roof_apex_z_with_no_planes_is_zero() {
        let roof = Roof { planes: Vec::new(), ..Roof::default() };
        assert_eq!(roof_apex_z(&roof), 0.0, "a roof with no planes should have apex z = 0.0");
    }

    #[test]
    fn plane_apex_z_with_empty_ring_is_zero() {
        let plane = RoofPlane { ring: Vec::new(), ..RoofPlane::default() };
        assert_eq!(plane_apex_z(&plane), 0.0, "a plane with an empty ring should have apex z = 0.0");
    }

    #[test]
    fn projected_area_sum_ignores_non_finite() {
        let mut roof = Roof::default();
        roof.planes.push(RoofPlane { projected_area: 10.0, ..RoofPlane::default() });
        roof.planes.push(RoofPlane { projected_area: f64::NAN, ..RoofPlane::default() });
        roof.planes.push(RoofPlane { projected_area: 20.0, ..RoofPlane::default() });
        assert_eq!(projected_area_sum(&roof), 30.0, "projected_area_sum should ignore non-finite values");
    }

    #[test]
    fn footprint_with_three_walls_is_a_triangle() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(5.0, 8.66), 0.2));
        model.push_wall(Wall::new(Vec2::new(5.0, 8.66), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(fp.outer.len(), 3, "triangle house should have 3 footprint edges");
        assert_eq!(fp.edges.len(), 3);
    }

    #[test]
    fn roof_with_level_elevation_includes_it_in_eave_z() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Update the existing level 0's elevation rather than pushing a duplicate.
        if let Some(l) = model.levels.iter_mut().find(|l| l.index == 0) {
            l.elevation = 2.7;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        for plane in &roof.planes {
            assert!((plane.eave_z - 5.1).abs() < 1e-9, "eave_z {} != 5.1 (2.7 + 2.4)", plane.eave_z);
        }
    }

    #[test]
    fn roof_with_zero_wall_height_uses_level_elevation() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        for w in model.walls.iter_mut() {
            w.height = 0.0;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        for plane in &roof.planes {
            assert!((plane.eave_z - 0.0).abs() < 1e-9, "eave_z {} != 0.0 for zero-height walls", plane.eave_z);
        }
    }

    #[test]
    fn footprint_with_exactly_256_edges_is_accepted() {
        let mut model = ArchModel::default();
        for i in 0..256 {
            let a = (i as f64) * (std::f64::consts::PI * 2.0 / 256.0);
            let b = ((i + 1) as f64) * (std::f64::consts::PI * 2.0 / 256.0);
            model.push_wall(Wall::new(Vec2::new(10.0 * a.cos(), 10.0 * a.sin()), Vec2::new(10.0 * b.cos(), 10.0 * b.sin()), 0.2));
        }
        let fp = outer_footprint(&model, 0);
        assert!(fp.is_some(), "footprint with exactly 256 edges should be accepted");
        assert_eq!(fp.unwrap().edges.len(), 256);
    }

    #[test]
    fn roof_with_all_gable_ends_has_no_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        for w in model.walls.iter_mut() {
            w.roof_directive = RoofDirective::Gable;
        }
        let result = build_roof(&model, 0, &test_roof());
        assert!(matches!(result, Err(RoofError::Degenerate)), "all gable ends should produce no planes (degenerate)");
    }

    #[test]
    fn footprint_with_huge_coordinates() {
        let mut model = ArchModel::default();
        let s = 1e9;
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(s, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(s, 0.0), Vec2::new(s, s), 0.2));
        model.push_wall(Wall::new(Vec2::new(s, s), Vec2::new(0.0, s), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, s), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0).expect("footprint with huge coordinates");
        assert_eq!(fp.outer.len(), 4);
        let area = crate::poly::ring_area(&fp.outer).abs();
        // The outer face extends 0.1 m beyond the center line on each side (wall thickness 0.2).
        let expected = (s + 0.2) * (s + 0.2);
        assert!((area - expected).abs() < 1e6, "area {} != {}", area, expected);
    }

    #[test]
    fn footprint_ignores_interior_divider_walls() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Add an interior divider wall across the middle.
        let mut div = Wall::new(Vec2::new(0.0, 4.0), Vec2::new(10.0, 4.0), 0.1);
        div.divider = true;
        div.exterior = false;
        model.push_wall(div);
        let fp = outer_footprint(&model, 0).expect("footprint");
        assert_eq!(fp.outer.len(), 4, "interior divider must not appear in the footprint");
    }

    #[test]
    fn footprint_is_independent_of_wall_heights() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Give each wall a different height.
        for (i, w) in model.walls.iter_mut().enumerate() {
            w.height = 2.0 + i as f64 * 0.5;
        }
        let fp = outer_footprint(&model, 0).expect("footprint");
        assert_eq!(fp.outer.len(), 4, "wall heights must not affect the footprint shape");
    }

    #[test]
    fn footprint_with_nan_wall_endpoint_is_rejected() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        // One endpoint is NaN.
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(f64::NAN, 0.0), 0.2));
        assert!(outer_footprint(&model, 0).is_none(), "a wall with a NaN endpoint must be rejected");
    }

    #[test]
    fn footprint_with_a_gap_is_rejected() {
        let mut model = ArchModel::default();
        // A square with a gap in the top edge
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(5.0, 8.0), 0.2));
        // Gap from (5,8) to (0,8)
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0);
        assert!(fp.is_none(), "a wall ring with a gap should be rejected");
    }

    #[test]
    fn footprint_with_nearly_collinear_walls_not_merged() {
        let mut model = ArchModel::default();
        // Two walls that are nearly collinear but not quite (a very small angle)
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(10.0, 1e-4), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 1e-4), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0).expect("footprint");
        // The two nearly collinear walls should not be merged (angle is too large)
        assert_eq!(fp.edges.len(), 5, "nearly collinear walls should not be merged");
    }

    #[test]
    fn footprint_with_thin_walls() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.01));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.01));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.01));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.01));
        let fp = outer_footprint(&model, 0).expect("footprint with thin walls");
        assert_eq!(fp.outer.len(), 4);
        let min_x = fp.outer.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = fp.outer.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = fp.outer.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = fp.outer.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x + 0.005).abs() < 1e-6, "min_x {} != -0.005", min_x);
        assert!((max_x - 10.005).abs() < 1e-6, "max_x {} != 10.005", max_x);
        assert!((min_y + 0.005).abs() < 1e-6, "min_y {} != -0.005", min_y);
        assert!((max_y - 8.005).abs() < 1e-6, "max_y {} != 8.005", max_y);
    }

    // --- hardening tests for the ring helpers ---

    #[test]
    fn ring_origin_empty_returns_zero() {
        let edges: Vec<FootprintEdge> = Vec::new();
        let origin = ring_origin(&edges);
        assert!(origin.near(Vec2::ZERO, 1e-12), "empty edges should give Vec2::ZERO origin");
    }

    #[test]
    fn ring_origin_ignores_nan_endpoints() {
        let edges = vec![fe(0, (f64::NAN, f64::NAN), (f64::NAN, f64::NAN)), fe(1, (3.0, 4.0), (5.0, 6.0))];
        let origin = ring_origin(&edges);
        assert!((origin.x - 3.0).abs() < 1e-12, "origin.x {} != 3.0", origin.x);
        assert!((origin.y - 4.0).abs() < 1e-12, "origin.y {} != 4.0", origin.y);
    }

    #[test]
    fn walk_ring_rejects_figure_eight() {
        // Two loops sharing a vertex: the shared vertex has 4 ends, so walk_ring must reject.
        let edges = vec![
            fe(0, (0.0, 0.0), (1.0, 0.0)),
            fe(1, (1.0, 0.0), (1.0, 1.0)),
            fe(2, (1.0, 1.0), (0.0, 0.0)),
            fe(3, (0.0, 0.0), (0.0, 1.0)),
            fe(4, (0.0, 1.0), (1.0, 1.0)),
        ];
        let origin = ring_origin(&edges);
        let result = walk_ring(&edges, origin);
        assert!(result.is_none(), "figure-8 (vertex with 4 ends) must be rejected");
    }

    #[test]
    fn walk_ring_rejects_fewer_than_three_edges() {
        let edges = vec![fe(0, (0.0, 0.0), (1.0, 0.0)), fe(1, (1.0, 0.0), (0.0, 0.0))];
        let origin = ring_origin(&edges);
        let result = walk_ring(&edges, origin);
        assert!(result.is_none(), "fewer than 3 edges must be rejected");
    }

    #[test]
    fn walk_ring_minimal_triangle() {
        let edges = vec![fe(0, (0.0, 0.0), (1.0, 0.0)), fe(1, (1.0, 0.0), (0.5, 1.0)), fe(2, (0.5, 1.0), (0.0, 0.0))];
        let origin = ring_origin(&edges);
        let ring = walk_ring(&edges, origin).expect("triangle should walk");
        assert_eq!(ring.len(), 3);
        for i in 0..3 {
            assert!(ring[i].b.near(ring[(i + 1) % 3].a, 1e-6), "edge {} does not meet the next", i);
        }
    }

    #[test]
    fn prune_spurs_drops_zero_length_edges() {
        // A square with a zero-length edge at one corner.
        let mut edges = vec![
            fe(0, (0.0, 0.0), (4.0, 0.0)),
            fe(1, (4.0, 0.0), (4.0, 4.0)),
            fe(2, (4.0, 4.0), (0.0, 4.0)),
            fe(3, (0.0, 4.0), (0.0, 0.0)),
            fe(4, (4.0, 4.0), (4.0, 4.0)), // zero-length
        ];
        let origin = ring_origin(&edges);
        prune_spurs(&mut edges, origin);
        assert_eq!(edges.len(), 4, "zero-length edge should be pruned");
        assert!(walk_ring(&edges, origin).is_some(), "ring should still walk after pruning");
    }

    #[test]
    fn components_single_edge() {
        let edges = vec![fe(0, (0.0, 0.0), (1.0, 0.0))];
        let origin = ring_origin(&edges);
        let comps = components(&edges, origin);
        assert_eq!(comps.len(), 1, "one edge should give one component");
        assert_eq!(comps[0], vec![0], "the component should contain edge 0");
    }

    #[test]
    fn components_deterministic_order() {
        // Two separate edges; components must be sorted by first index.
        let edges = vec![fe(0, (10.0, 10.0), (11.0, 10.0)), fe(1, (0.0, 0.0), (1.0, 0.0))];
        let origin = ring_origin(&edges);
        let comps = components(&edges, origin);
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0], vec![0], "first component must be edge 0 (lower index)");
        assert_eq!(comps[1], vec![1], "second component must be edge 1");
    }

    #[test]
    fn merge_straight_does_not_collapse_256_gon() {
        // A regular 256-gon of radius 10: turn angle ~0.0245 rad, far above the 1e-9 tolerance.
        let mut ring = Vec::with_capacity(256);
        for i in 0..256 {
            let a = (i as f64) * (std::f64::consts::PI * 2.0 / 256.0);
            let b = ((i + 1) as f64) * (std::f64::consts::PI * 2.0 / 256.0);
            ring.push(fe(i as u32, (10.0 * a.cos(), 10.0 * a.sin()), (10.0 * b.cos(), 10.0 * b.sin())));
        }
        let origin = ring_origin(&ring);
        merge_straight(&mut ring, origin);
        assert_eq!(ring.len(), 256, "a 256-gon must not be collapsed by merge_straight");
    }

    #[test]
    fn merge_straight_with_exact_collinear() {
        // Three collinear edges: (0,0)-(1,0), (1,0)-(2,0), (2,0)-(3,0) plus two to close.
        let mut ring = vec![
            fe(0, (0.0, 0.0), (1.0, 0.0)),
            fe(1, (1.0, 0.0), (2.0, 0.0)),
            fe(2, (2.0, 0.0), (3.0, 0.0)),
            fe(3, (3.0, 0.0), (3.0, 1.0)),
            fe(4, (3.0, 1.0), (0.0, 1.0)),
            fe(5, (0.0, 1.0), (0.0, 0.0)),
        ];
        let origin = ring_origin(&ring);
        merge_straight(&mut ring, origin);
        // The three bottom edges should merge into one.
        assert_eq!(ring.len(), 4, "three collinear edges should merge into one");
    }

    #[test]
    fn merge_straight_opposite_direction_not_merged() {
        // Two edges that meet at a point but go in opposite directions (dot < 0).
        // (0,0)-(1,0) and (1,0)-(0,0) would be degenerate; instead use a V shape.
        let mut ring = vec![fe(0, (0.0, 0.0), (1.0, 0.0)), fe(1, (1.0, 0.0), (0.5, 1.0)), fe(2, (0.5, 1.0), (0.0, 0.0))];
        let origin = ring_origin(&ring);
        merge_straight(&mut ring, origin);
        // None of these are collinear in the same direction, so no merge.
        assert_eq!(ring.len(), 3, "non-collinear edges must not be merged");
    }

    #[test]
    fn vkey_huge_coordinates_with_origin() {
        // Georeferenced coordinates: origin at (5e6, 4e6), point at (5e6 + 1.5, 4e6 + 2.5).
        let origin = Vec2::new(5_000_000.0, 4_000_000.0);
        let p = Vec2::new(5_000_001.5, 4_000_002.5);
        let key = vkey(p, origin);
        assert_eq!(key, (1_500_000, 2_500_000), "huge coords with origin should quantise correctly");
    }

    #[test]
    fn vkey_negative_coordinates() {
        let origin = Vec2::new(-10.0, -10.0);
        let p = Vec2::new(-9.999999, -9.999998);
        let key = vkey(p, origin);
        // (-9.999999 - (-10)) / 1e-6 = 1.0 -> 1
        // (-9.999998 - (-10)) / 1e-6 = 2.0 -> 2
        assert_eq!(key, (1, 2), "negative coords with origin should quantise correctly");
    }

    #[test]
    fn walk_ring_rejects_equal_end_keys() {
        // An edge whose two ends quantise to the same key (too short).
        let edges = vec![
            fe(0, (0.0, 0.0), (0.0, 0.0)), // zero-length
            fe(1, (0.0, 0.0), (1.0, 0.0)),
            fe(2, (1.0, 0.0), (0.0, 0.0)),
        ];
        let origin = ring_origin(&edges);
        let result = walk_ring(&edges, origin);
        assert!(result.is_none(), "edge with equal end keys must be rejected");
    }
}
