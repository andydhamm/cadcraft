//! Straight skeleton of a polygon with holes, and the offsets it gives.
//!
//! Assisted draft (2026-10-25): written ahead of phase 4 (automatic roofs) by Claude, not by the swarm.
//!
//! Every edge of the input moves inward, parallel to itself, at its weight (unit speed for the plain skeleton). The
//! skeleton is the set of paths the vertices trace until the shrinking wavefront vanishes. Each input edge sweeps one
//! face. A node's `time` is how far the wavefront had travelled when it got there, which is its distance to the edges of
//! the faces around it (times the edge's weight, for a [`weighted_skeleton`]). So:
//! - a hip roof over the polygon is the skeleton lifted by `time * slope` (each face is one roof plane);
//! - the inward offset by `d` is the wavefront at time `d` ([`inset`]);
//! - the outward offset by `d` is the same construction on the outside ([`outset`]).
//!
//! The functions never panic and never loop forever: bad input returns a [`SkeletonError`], and the event simulation
//! stops with [`SkeletonError::EventLimit`] if it ever exceeds its event budget.
//!
//! How it is computed: the wavefront (one loop of vertices per ring) is simulated event by event. An edge shrinking to
//! nothing and a reflex vertex reaching another wavefront edge are found afresh at every step, the earliest is applied,
//! and everything else that coincides at that instant (several events at one place, wedges of no width, loops that have
//! collapsed) is repaired together before time moves on. Tolerances are `EPS` times the extent of the input (at least
//! `EPS`): points closer than that are one, and a strip a few tolerances wide counts as zero-width. The cost grows with
//! the cube of the vertex count: milliseconds for hundreds of vertices, about a second for a thousand.
//!
//! Weights (one code path; the plain skeleton is the weighted one with every weight 1): an edge's offset line is
//! `normal . x = c + weight * time`, and a vertex between two edges moves so that it stays on both lines. A weight of 0
//! pins the edge (a gable end): its vertices slide along it and its face has no area in plan. Two parallel edges that
//! become neighbours at different speeds (one caught up with the other) lie on one line at that instant; from then on the
//! faster one carries the wavefront and the slower one is consumed, leaving a ridge arc along the slower one's last position.
//!
//! Limit: shapes whose features are only about ten to a hundred tolerances apart (`1e-8` of their size) are the one place
//! where the tolerances can disagree with each other; a very rare such input ends in `EventLimit` instead of a skeleton.
//! Weights add two: edges of weight 0 can enclose a part of the region that no moving edge ever reaches (`BadWeights`), and
//! weights more than about `1e4` apart on a shape whose features nearly coincide (within about `1e-6` of its size, but not
//! exactly) can end in `EventLimit` or in faces that do not add up (`check` reports it); a perturbed shape in a realistic
//! range of slopes is fine.

use crate::{Vec2, point_in_polygon, shoelace};

/// Length below which two points are the same and an edge is dropped (drawing units are metres in arch).
pub const EPS: f64 = 1e-9;

/// One node of the skeleton.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkeletonNode {
    pub pos: Vec2,
    /// Distance the wavefront travelled to reach this node; 0 for the polygon's own vertices.
    pub time: f64,
}

/// The face swept by one input edge: the roof plane over that edge.
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonFace {
    /// 0 for the outer ring, `1 + i` for hole `i` (indices into [`Skeleton::rings`]).
    pub ring: usize,
    /// The edge runs from `rings[ring][edge]` to `rings[ring][(edge + 1) % len]`.
    pub edge: usize,
    /// The face's boundary as node indices, counter-clockwise, starting with the edge's two end vertices. A face that
    /// encloses an island (the faces of a hole, when one edge sweeps all round it) has no single outline: the boundary goes
    /// out to the island along a bridge arc, round it clockwise and back along the bridge, so nodes repeat.
    pub nodes: Vec<usize>,
}

/// The straight skeleton of a polygon with holes.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    /// The input after cleaning: outer ring counter-clockwise, then each hole clockwise, with repeated points and
    /// straight-through (collinear) vertices removed. Edge and face indices refer to these rings.
    pub rings: Vec<Vec<Vec2>>,
    /// First the ring vertices in order (outer ring, then each hole), each with time 0, then the interior nodes.
    pub nodes: Vec<SkeletonNode>,
    /// Skeleton arcs between two nodes (input edges are not arcs).
    pub arcs: Vec<(usize, usize)>,
    /// One face per input edge, in ring order.
    pub faces: Vec<SkeletonFace>,
    /// The speed of each cleaned ring edge, `weights[ring][edge]` (same indexing as `rings`): 1.0 for every edge of a
    /// plain [`straight_skeleton`]; for a [`weighted_skeleton`], the input weights mapped onto the cleaned edges.
    pub weights: Vec<Vec<f64>>,
}

impl Skeleton {
    /// The largest node time: the inset distance at which the polygon disappears.
    pub fn max_time(&self) -> f64 {
        self.nodes.iter().map(|n| n.time).fold(0.0, f64::max)
    }

    /// The node's height on a roof of the given slope (rise per unit run).
    pub fn height(&self, node: usize, slope: f64) -> f64 {
        self.nodes[node].time * slope
    }
}

/// A region produced by an offset: an outer ring (counter-clockwise) with holes (clockwise).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OffsetRegion {
    pub outer: Vec<Vec2>,
    pub holes: Vec<Vec<Vec2>>,
}

/// Why a skeleton or offset could not be built.
#[derive(Clone, Debug, PartialEq)]
pub enum SkeletonError {
    /// A ring has fewer than 3 distinct, non-collinear vertices.
    TooFewVertices,
    /// A ring crosses itself or another ring.
    NotSimple,
    /// A hole is not strictly inside the outer ring.
    HoleOutside,
    /// A coordinate or distance is NaN or infinite, or a distance is negative.
    NotFinite,
    /// The event simulation went past its budget (a bug or a pathological input); nothing is returned.
    EventLimit,
    /// Weights (or slopes) do not match the rings edge for edge, are negative, NaN or infinite, or two collinear
    /// neighbouring edges carry different weights (no well-defined skeleton there). Also when the edges with weight 0
    /// enclose a part of the region that no moving edge ever reaches.
    BadWeights,
}

impl std::fmt::Display for SkeletonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SkeletonError::TooFewVertices => "a ring has fewer than 3 distinct vertices",
            SkeletonError::NotSimple => "the rings cross themselves or each other",
            SkeletonError::HoleOutside => "a hole is not inside the outer ring",
            SkeletonError::NotFinite => "a coordinate or distance is not a finite number (or a distance is negative)",
            SkeletonError::EventLimit => "the straight-skeleton event simulation exceeded its budget",
            SkeletonError::BadWeights => "the edge weights or slopes do not fit the rings, or are not finite and non-negative",
        })
    }
}

impl std::error::Error for SkeletonError {}

/// The straight skeleton of `outer` with `holes`. Rings may be given in either orientation and may repeat their first
/// point at the end; they are cleaned and oriented as described on [`Skeleton::rings`].
pub fn straight_skeleton(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Result<Skeleton, SkeletonError> {
    weighted_skeleton(outer, holes, &unit_weights(outer, holes))
}

/// The weighted straight skeleton: edge `i` of ring `r` moves inward at speed `weights[r][i]` instead of 1. Weights index
/// the INPUT rings exactly as given (`weights[0]` for `outer`, `weights[1 + h]` for `holes[h]`; edge `i` runs from vertex
/// `i` to vertex `i + 1`, before any cleaning or reorientation); each must be finite and >= 0. A weight of 0 freezes the
/// edge: its face has no area in plan (a gable end). All weights equal to 1 give exactly [`straight_skeleton`].
/// A node's `time` is when the wavefront reached it, so it lies at distance `weight * time` from each of its faces'
/// edges. Mismatched, negative or non-finite weights, or collinear neighbouring edges with different weights, give
/// [`SkeletonError::BadWeights`].
pub fn weighted_skeleton(outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>]) -> Result<Skeleton, SkeletonError> {
    let prep = prepare(outer, holes, weights)?;
    // Without a moving edge on the outer ring nothing ever happens: there is no skeleton, and no way to tell a gable roof
    // with no roof planes from a mistake.
    if prep.weights.first().is_none_or(|ring| ring.iter().all(|&w| w == 0.0)) {
        return Err(SkeletonError::BadWeights);
    }
    let mut sim = Sim::new(&prep.rings, &prep.weights, prep.origin, prep.tol, 0);
    sim.run(None)?;
    let faces = build_faces(&prep.rings, &sim.arcs, &sim.nodes);
    let arcs = sim.arcs.iter().map(|r| (r.a, r.b)).collect();
    Ok(Skeleton { rings: prep.rings, nodes: sim.nodes, arcs, faces, weights: prep.weights })
}

/// The skeleton of a pitched roof over `outer` with `holes`: `slopes[r][i]` is the rise per unit run of the roof plane
/// over input edge `i` of ring `r` (indexed like [`weighted_skeleton`]'s weights), `f64::INFINITY` for a gable end (a
/// vertical face). Each edge moves at speed `1 / slope`, so every node's `time` IS its roof height and each face is one
/// plane through its edge. Slopes must be > 0 (or infinite); anything else gives [`SkeletonError::BadWeights`].
pub fn roof_skeleton(outer: &[Vec2], holes: &[Vec<Vec2>], slopes: &[Vec<f64>]) -> Result<Skeleton, SkeletonError> {
    let mut weights = Vec::with_capacity(slopes.len());
    for ring in slopes {
        let mut w = Vec::with_capacity(ring.len());
        for &s in ring {
            if s == f64::INFINITY {
                w.push(0.0);
            } else if s.is_finite() && s > 0.0 {
                w.push(1.0 / s);
            } else {
                return Err(SkeletonError::BadWeights);
            }
        }
        weights.push(w);
    }
    weighted_skeleton(outer, holes, &weights)
}

/// The inward offset of `outer` with `holes` by `d >= 0`: the wavefront at time `d`. One region per piece the polygon
/// splits into; empty once `d` reaches [`Skeleton::max_time`]. `d == 0` returns the cleaned input.
pub fn inset(outer: &[Vec2], holes: &[Vec<Vec2>], d: f64) -> Result<Vec<OffsetRegion>, SkeletonError> {
    if !d.is_finite() || d < 0.0 {
        return Err(SkeletonError::NotFinite);
    }
    let prep = prepare(outer, holes, &unit_weights(outer, holes))?;
    if d == 0.0 {
        return Ok(vec![region_of(&prep.rings)]);
    }
    let loops = wavefront(&prep.rings, &prep, d, 0)?;
    Ok(assemble_regions(loops.into_iter().map(|(pts, _)| pts).collect(), prep.tol))
}

/// The outward offset of `outer` with `holes` by `d >= 0`, with mitred (sharp) corners as a straight skeleton gives.
/// Holes shrink and vanish once `d` is large enough.
pub fn outset(outer: &[Vec2], holes: &[Vec<Vec2>], d: f64) -> Result<OffsetRegion, SkeletonError> {
    if !d.is_finite() || d < 0.0 {
        return Err(SkeletonError::NotFinite);
    }
    let prep = prepare(outer, holes, &unit_weights(outer, holes))?;
    if d == 0.0 {
        return Ok(region_of(&prep.rings));
    }
    let Some((ring, hole_rings)) = prep.rings.split_first() else {
        return Err(SkeletonError::TooFewVertices);
    };
    // The outside of the outer ring is the polygon "frame minus outer ring": the ring, reversed, is a hole in a frame so
    // far away that its own wavefront cannot reach the ring's within time `d`. A mitred corner travels at most
    // `speed * d` from its vertex, so the frame is that far, plus the ring's size, beyond the bounding box.
    let Some((lo, hi)) = extent(ring.iter().copied()) else {
        return Err(SkeletonError::TooFewVertices);
    };
    let size = (hi.x - lo.x).max(hi.y - lo.y);
    let grow = size + d * (4.0 + 2.0 * max_corner_speed(ring));
    if !grow.is_finite() {
        return Err(SkeletonError::NotFinite);
    }
    let frame = vec![
        Vec2::new(lo.x - grow, lo.y - grow),
        Vec2::new(hi.x + grow, lo.y - grow),
        Vec2::new(hi.x + grow, hi.y + grow),
        Vec2::new(lo.x - grow, hi.y + grow),
    ];
    let mut reversed = ring.clone();
    reversed.reverse();
    let loops = wavefront(&[frame, reversed], &prep, d, 4)?;

    // Loops not touching the frame: the clockwise one is the offset outline, counter-clockwise ones are voids the
    // offset closed around (for example the inside of a C whose mouth is narrower than `2 d`).
    let mut outline: Option<(Vec<Vec2>, f64)> = None;
    let mut holes_out: Vec<Vec<Vec2>> = Vec::new();
    for (pts, touches_frame) in loops {
        if touches_frame {
            continue;
        }
        let Some(clean) = tidy_loop(&pts, prep.tol) else { continue };
        let area = shoelace(&clean);
        let mut flipped = clean;
        flipped.reverse();
        if area < 0.0 {
            if outline.as_ref().is_none_or(|(_, best)| -area > *best) {
                outline = Some((flipped, -area));
            }
        } else {
            holes_out.push(flipped);
        }
    }
    let Some((outer_out, _)) = outline else {
        return Err(SkeletonError::EventLimit);
    };
    // Each input hole shrinks like a polygon of its own; the offset outline never reaches it.
    for hole in hole_rings {
        let mut polygon = hole.clone();
        polygon.reverse();
        for (pts, _) in wavefront(&[polygon], &prep, d, 0)? {
            if let Some(mut clean) = tidy_loop(&pts, prep.tol)
                && shoelace(&clean) > 0.0
            {
                clean.reverse();
                holes_out.push(clean);
            }
        }
    }
    Ok(OffsetRegion { outer: outer_out, holes: holes_out })
}

/// Problems with a skeleton, for tests and debugging; empty when every invariant holds within `tol`:
/// each face's nodes lie at distance `weight * time` from the line of the face's edge, the faces' areas add up to the region's
/// area, every node lies inside the region or on its boundary, and every arc joins two existing nodes.
pub fn check(skeleton: &Skeleton, tol: f64) -> Vec<String> {
    let mut problems = Vec::new();
    let (rings, nodes) = (&skeleton.rings, &skeleton.nodes);
    let ring_vertices: usize = rings.iter().map(Vec::len).sum();
    if rings.is_empty() {
        problems.push("the skeleton has no rings".to_string());
        return problems;
    }
    check_ring_nodes(skeleton, ring_vertices, tol, &mut problems);
    check_weights(skeleton, &mut problems);
    for (i, &(a, b)) in skeleton.arcs.iter().enumerate() {
        if a >= nodes.len() || b >= nodes.len() {
            problems.push(format!("arc {i} joins node {a} and node {b}, but there are only {} nodes", nodes.len()));
        } else if a == b {
            problems.push(format!("arc {i} starts and ends at node {a}"));
        }
    }
    let perimeter: f64 = rings.iter().map(|r| ring_perimeter(r)).sum();
    let area_tol = tol * (1.0 + perimeter);
    let mut face_area = 0.0;
    if skeleton.faces.len() != ring_vertices {
        problems.push(format!("{} faces for {ring_vertices} input edges", skeleton.faces.len()));
    }
    for (fi, face) in skeleton.faces.iter().enumerate() {
        face_area += check_face(skeleton, fi, face, tol, area_tol, &mut problems);
    }
    let region_area: f64 = rings.iter().map(|r| shoelace(r)).sum();
    if (face_area - region_area).abs() > area_tol {
        problems.push(format!("the faces add up to an area of {face_area} but the region has an area of {region_area}"));
    }
    for (i, node) in nodes.iter().enumerate() {
        if !node.pos.is_finite() || !node.time.is_finite() {
            problems.push(format!("node {i} is not finite"));
        } else if !in_region_or_on_boundary(rings, node.pos, tol) {
            problems.push(format!("node {i} at ({}, {}) lies outside the region", node.pos.x, node.pos.y));
        }
    }
    problems
}

// ---------------------------------------------------------------------------------------------------------------------
// Input preparation
// ---------------------------------------------------------------------------------------------------------------------

/// Sine of the smallest turn away from a straight line that still counts as a corner: a vertex whose two edges double
/// back on each other within this angle is a zero-width spike (rejected on input, collapsed during the simulation). It
/// is `EPS` so that the width of such a wedge, at most this angle times the size of the input, is within the tolerance.
const SPIKE_SIN: f64 = EPS;
/// Two edges that point the same way within this sine are parallel: if their speeds differ there is no vertex between
/// them (their lines are one line at that instant and part for ever after), see `Sim::collapse_parallel`.
const PARALLEL_SIN: f64 = EPS;
/// A wedge between nearly antiparallel edges counts as a zero-width spike while it is at most this many tolerances wide.
/// Positions are only good to a few tolerances after merges, so the orientation of anything thinner is not reliable.
const THIN_WEDGE: f64 = 16.0;
/// ... and only edges that double back this closely (a sine, about half a degree) are candidates at all; a short edge
/// at a right angle is just a short edge.
const THIN_WEDGE_SIN: f64 = 1e-2;
/// Vertices are snapped onto the meeting point of their two lines only if the lines cross with at least this sine...
const SNAP_MIN_SIN: f64 = 0.05;
/// ...and the meeting point is within this many tolerances of where the vertex was placed.
const SNAP_REACH: f64 = 16.0;
/// A vertex is reflex (it can split the wavefront) when its edges turn right by more than this sine.
const REFLEX_SIN: f64 = 1e-12;
/// A length changing slower than this per unit time is treated as constant (for unit-speed edges: see `Sim::speed`).
const RATE_EPS: f64 = 1e-12;
/// A reflex vertex approaches a wavefront edge only if it closes the gap faster than this (a unit-speed edge moves at 1).
const CLOSE_EPS: f64 = 1e-9;

/// The cleaned, oriented and validated input, with the tolerance derived from its size.
struct Prepared {
    /// Outer ring counter-clockwise, then each hole clockwise, in the caller's coordinates.
    rings: Vec<Vec<Vec2>>,
    /// The speed of each edge of `rings` (edge `i` runs from vertex `i` to vertex `i + 1`).
    weights: Vec<Vec<f64>>,
    /// Centre of the bounding box. The simulation works relative to it so a drawing far from the origin keeps precision.
    origin: Vec2,
    /// Coincidence tolerance: [`EPS`] relative to the size of the input (at least [`EPS`] itself).
    tol: f64,
}

/// Bounding box corners of some points, or `None` for no points.
fn extent(points: impl Iterator<Item = Vec2>) -> Option<(Vec2, Vec2)> {
    points.fold(None, |acc, p| match acc {
        None => Some((p, p)),
        Some((lo, hi)) => Some((lo.min(p), hi.max(p))),
    })
}

/// Unit vector along `v`, or zero for a zero vector (unlike `Vec2::normalized`, no absolute length threshold).
fn unit(v: Vec2) -> Vec2 {
    v / v.len()
}

/// Distance from `p` to the segment `a`-`b`.
fn dist_point_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.len2();
    if l2 <= 0.0 {
        return p.dist(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

/// Smallest distance between the segments `a`-`b` and `c`-`d` (zero when they cross).
fn segment_distance(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f64 {
    let side = |p: Vec2, q: Vec2, r: Vec2| (q - p).cross(r - p);
    let (o1, o2, o3, o4) = (side(a, b, c), side(a, b, d), side(c, d, a), side(c, d, b));
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return 0.0;
    }
    dist_point_segment(a, c, d).min(dist_point_segment(b, c, d)).min(dist_point_segment(c, a, b)).min(dist_point_segment(d, a, b))
}

fn ring_perimeter(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i].dist(ring[(i + 1) % n])).sum()
}

/// What `clean_ring` does with a vertex where the ring doubles back on itself (a zero-width spike).
#[derive(Clone, Copy, PartialEq)]
enum Spikes {
    /// Input: the ring is not simple.
    Reject,
    /// Wavefront output at an instant just before an event: drop the tip and keep what is left.
    Remove,
}

/// Drop repeated points (including a repeated closing point) and straight-through vertices from a ring.
fn clean_ring(pts: &[Vec2], tol: f64, spikes: Spikes) -> Result<Vec<Vec2>, SkeletonError> {
    let unit = vec![1.0; pts.len()];
    clean_ring_weighted(pts, &unit, tol, spikes).map(|(ring, _, _)| ring)
}

/// `clean_ring` for a ring whose edge `i` (from point `i` to point `i + 1`) has speed `weights[i]`. Returns the cleaned
/// ring, the weight of each of its edges, and whether a straight-through vertex had to go between two edges of different
/// weights (the merged edge then has no right weight: the caller refuses the input). A repeated point takes its zero-length
/// edge with it, and the edge that leaves it carries on from the point that was kept.
fn clean_ring_weighted(pts: &[Vec2], weights: &[f64], tol: f64, spikes: Spikes) -> Result<(Vec<Vec2>, Vec<f64>, bool), SkeletonError> {
    // Each vertex carries the weight of the edge that leaves it.
    let mut v: Vec<(Vec2, f64)> = Vec::with_capacity(pts.len());
    for (k, &p) in pts.iter().enumerate() {
        let w = weights.get(k).copied().unwrap_or(1.0);
        if v.last().is_some_and(|q| q.0.dist(p) <= tol) {
            if let Some(last) = v.last_mut() {
                last.1 = w;
            }
        } else {
            v.push((p, w));
        }
    }
    while v.len() > 1 && v.first().zip(v.last()).is_some_and(|(a, b)| a.0.dist(b.0) <= tol) {
        v.pop();
    }
    let mut clash = false;
    loop {
        let n = v.len();
        if n < 3 {
            return Err(SkeletonError::TooFewVertices);
        }
        // Straight-through vertices go first, so a ring that is entirely collinear ends up with too few vertices rather
        // than looking like a chain of spikes.
        let corner = |i: usize| (v[(i + n - 1) % n].0, v[i].0, v[(i + 1) % n].0);
        let straight = (0..n).find(|&i| {
            let (a, b, c) = corner(i);
            let (ab, bc) = (b - a, c - b);
            // Distance from `b` to the line `a`-`c` is `cross / |ac|`.
            ab.dot(bc) >= 0.0 && ab.cross(bc).abs() <= tol * (c - a).len().max(tol)
        });
        if let Some(i) = straight {
            clash |= v[(i + n - 1) % n].1 != v[i].1;
            v.remove(i);
            continue;
        }
        let spike = (0..n).find(|&i| {
            let (a, b, c) = corner(i);
            let (ab, bc) = (b - a, c - b);
            ab.dot(bc) < 0.0 && ab.cross(bc).abs() <= SPIKE_SIN * ab.len() * bc.len()
        });
        match (spike, spikes) {
            (None, _) => {
                let (ring, speeds) = v.into_iter().unzip();
                return Ok((ring, speeds, clash));
            }
            (Some(_), Spikes::Reject) => return Err(SkeletonError::NotSimple),
            (Some(i), Spikes::Remove) => {
                v.remove(i);
            }
        }
    }
}

/// Weight 1 for every edge of the input rings as given (outer ring first, then the holes).
fn unit_weights(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<Vec<f64>> {
    std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)).map(|ring| vec![1.0; ring.len()]).collect()
}

/// Clean, orient and validate the input. Errors in the order: bad weights (wrong number of rings, negative or not finite),
/// not finite, too few vertices, bad weights (wrong number for a ring), not simple, hole outside, bad weights (a straight-through
/// vertex between edges of different weights).
fn prepare(outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>]) -> Result<Prepared, SkeletonError> {
    let input = || std::iter::once(outer).chain(holes.iter().map(Vec::as_slice));
    if weights.len() != holes.len() + 1 || weights.iter().flatten().any(|w| !w.is_finite() || *w < 0.0) {
        return Err(SkeletonError::BadWeights);
    }
    if input().flatten().any(|p| !p.is_finite()) {
        return Err(SkeletonError::NotFinite);
    }
    let Some((lo, hi)) = extent(input().flatten().copied()) else {
        return Err(SkeletonError::TooFewVertices);
    };
    let size = (hi.x - lo.x).max(hi.y - lo.y);
    if !size.is_finite() {
        return Err(SkeletonError::NotFinite);
    }
    if size <= 0.0 {
        return Err(SkeletonError::TooFewVertices);
    }
    let tol = EPS * size.max(1.0);
    // One weight per input edge, the closing edge (last point back to the first) included. A ring that repeats its first
    // point at the end has an empty closing edge, and may leave its weight out.
    let mut speeds: Vec<Vec<f64>> = Vec::with_capacity(weights.len());
    for (pts, ws) in input().zip(weights) {
        let repeats_first = pts.first().zip(pts.last()).is_some_and(|(a, b)| pts.len() >= 2 && a.dist(*b) <= tol);
        let mut ws = ws.clone();
        if repeats_first && ws.len() + 1 == pts.len() {
            ws.push(ws.last().copied().unwrap_or(0.0));
        }
        if ws.len() != pts.len() {
            return Err(SkeletonError::BadWeights);
        }
        // -0.0 is a weight of 0 too.
        speeds.push(ws.into_iter().map(|w| if w == 0.0 { 0.0 } else { w }).collect());
    }
    let mut rings = Vec::with_capacity(speeds.len());
    let mut clash = false;
    for (pts, ws) in input().zip(&mut speeds) {
        let (ring, cleaned, straight_clash) = clean_ring_weighted(pts, ws, tol, Spikes::Reject)?;
        rings.push(ring);
        *ws = cleaned;
        clash |= straight_clash;
    }
    check_simple(&rings, tol)?;
    for (k, (ring, ws)) in rings.iter_mut().zip(&mut speeds).enumerate() {
        let area = shoelace(ring);
        if !area.is_finite() {
            return Err(SkeletonError::NotFinite);
        }
        if area.abs() <= tol * size {
            return Err(SkeletonError::TooFewVertices);
        }
        // The outer ring runs counter-clockwise, holes clockwise. Reversing a ring reverses its edges, so edge `j` of the
        // reversed ring is old edge `n - 2 - j`.
        if (k == 0) != (area > 0.0) {
            ring.reverse();
            ws.reverse();
            ws.rotate_left(1);
        }
    }
    check_holes(&rings)?;
    if clash {
        return Err(SkeletonError::BadWeights);
    }
    Ok(Prepared { rings, weights: speeds, origin: lo.mid(hi), tol })
}

/// No two edges of the rings may touch or cross, except consecutive edges of one ring at their shared vertex.
fn check_simple(rings: &[Vec<Vec2>], tol: f64) -> Result<(), SkeletonError> {
    let mut edges: Vec<(usize, usize, usize, Vec2, Vec2)> = Vec::new();
    for (r, ring) in rings.iter().enumerate() {
        let n = ring.len();
        for i in 0..n {
            edges.push((r, i, n, ring[i], ring[(i + 1) % n]));
        }
    }
    for (x, &(r1, i1, n1, a, b)) in edges.iter().enumerate() {
        for &(r2, i2, _, c, d) in &edges[x + 1..] {
            let adjacent = r1 == r2 && (i2 == i1 + 1 || (i1 == 0 && i2 + 1 == n1));
            if adjacent {
                continue;
            }
            if a.x.min(b.x) - tol > c.x.max(d.x) || c.x.min(d.x) - tol > a.x.max(b.x) {
                continue;
            }
            if a.y.min(b.y) - tol > c.y.max(d.y) || c.y.min(d.y) - tol > a.y.max(b.y) {
                continue;
            }
            if segment_distance(a, b, c, d) <= tol {
                return Err(SkeletonError::NotSimple);
            }
        }
    }
    Ok(())
}

/// Every hole lies inside the outer ring and outside every other hole. The rings do not touch (checked before), so one
/// vertex of a hole decides.
fn check_holes(rings: &[Vec<Vec2>]) -> Result<(), SkeletonError> {
    let Some((outer, holes)) = rings.split_first() else {
        return Ok(());
    };
    for (i, hole) in holes.iter().enumerate() {
        let Some(&p) = hole.first() else { continue };
        if !point_in_polygon(outer, p) {
            return Err(SkeletonError::HoleOutside);
        }
        if holes.iter().enumerate().any(|(j, other)| i != j && point_in_polygon(other, p)) {
            return Err(SkeletonError::HoleOutside);
        }
    }
    Ok(())
}

/// The cleaned input as one region.
fn region_of(rings: &[Vec<Vec2>]) -> OffsetRegion {
    let mut it = rings.iter();
    OffsetRegion { outer: it.next().cloned().unwrap_or_default(), holes: it.cloned().collect() }
}

/// The velocity of a wavefront vertex between two edges with inward unit normals `n1` and `n2` and speeds `w1` and `w2`:
/// the point where both edges' offset lines meet, which solves `n1 . v = w1` and `n2 . v = w2`. For equal speeds `w` and
/// edges that turn gently that is `w * (n1 + n2) / (1 + n1 . n2)`; for edges that nearly double back `1 + n1 . n2` cancels
/// away, so the system is solved by Cramer's rule instead (`cross(n1, n2)` stays accurate). Antiparallel edges, and
/// parallel edges of different speeds, have no velocity (zero).
fn corner_velocity(n1: Vec2, w1: f64, n2: Vec2, w2: f64) -> Vec2 {
    let (dot, cross) = (n1.dot(n2), n1.cross(n2));
    if w1 == w2 {
        if dot >= 0.0 {
            (n1 + n2) / (1.0 + dot) * w1
        } else if cross.abs() < SPIKE_SIN {
            Vec2::ZERO
        } else {
            (n1 - n2).perp() / cross * w1
        }
    } else if cross.abs() < SPIKE_SIN {
        Vec2::ZERO
    } else {
        (n1 * w2 - n2 * w1).perp() / cross
    }
}

/// The fastest vertex speed in a cleaned ring (the sharpest corner), with every edge at unit speed.
fn max_corner_speed(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    let normal = |i: usize| unit(ring[(i + 1) % n] - ring[i]).perp();
    (0..n).map(|i| corner_velocity(normal((i + n - 1) % n), 1.0, normal(i), 1.0).len()).filter(|s| s.is_finite()).fold(1.0, f64::max)
}

/// A wavefront loop as a clean ring: finite, without repeated or straight-through vertices, at least 3 vertices and a
/// real area. `None` for a loop that has degenerated.
fn tidy_loop(pts: &[Vec2], tol: f64) -> Option<Vec<Vec2>> {
    if pts.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let ring = clean_ring(pts, tol, Spikes::Remove).ok()?;
    let (lo, hi) = extent(ring.iter().copied())?;
    (shoelace(&ring).abs() > tol * (hi.x - lo.x).max(hi.y - lo.y)).then_some(ring)
}

/// Group wavefront loops into regions: counter-clockwise loops are outlines, clockwise ones are holes of the smallest
/// outline that contains them.
fn assemble_regions(loops: Vec<Vec<Vec2>>, tol: f64) -> Vec<OffsetRegion> {
    let mut outlines: Vec<(Vec<Vec2>, f64)> = Vec::new();
    let mut holes: Vec<Vec<Vec2>> = Vec::new();
    for pts in loops {
        let Some(ring) = tidy_loop(&pts, tol) else { continue };
        let area = shoelace(&ring);
        if area > 0.0 {
            outlines.push((ring, area));
        } else {
            holes.push(ring);
        }
    }
    let mut regions: Vec<OffsetRegion> = outlines.iter().map(|(ring, _)| OffsetRegion { outer: ring.clone(), holes: Vec::new() }).collect();
    for hole in holes {
        // The outline holding most of the hole's vertices, the smallest on a tie. A hole can touch its outline, which makes a
        // single vertex unreliable, but never most of them.
        let mut best: Option<(usize, usize)> = None;
        for (i, (ring, area)) in outlines.iter().enumerate() {
            let inside = hole.iter().filter(|p| point_in_polygon(ring, **p)).count();
            let better = best.is_none_or(|(count, b)| inside > count || (inside == count && *area < outlines[b].1));
            if inside > 0 && better {
                best = Some((inside, i));
            }
        }
        if let Some(region) = best.and_then(|(_, i)| regions.get_mut(i)) {
            region.holes.push(hole);
        }
    }
    regions
}

// ---------------------------------------------------------------------------------------------------------------------
// Wavefront simulation
// ---------------------------------------------------------------------------------------------------------------------
//
// The wavefront is a set of loops (LAVs). Every vertex moves along its bisector so that both of its edges move inward at
// unit speed. Each step finds the earliest event (an edge shrinking to nothing, or a reflex vertex reaching another
// wavefront edge), jumps to its time, applies it, and then repairs every other coincidence at that instant (several
// events at one place or time, zero-width spikes, loops that have collapsed) with local operations until the picture is
// consistent again. Events are recomputed from scratch each step: O(n^3) overall, but there is no queue to invalidate.

/// One input edge as a moving line: at time `t` the wavefront edge lies on `{ p : normal . p = c + w * t }`.
#[derive(Clone, Copy, Debug, Default)]
struct EdgeLine {
    /// Unit direction of the edge.
    dir: Vec2,
    /// Inward unit normal (left of the direction: the polygon interior for the outer ring, the material for holes).
    normal: Vec2,
    c: f64,
    /// Speed of the edge along its normal: its weight (0 for an edge that never moves).
    w: f64,
}

/// A wavefront vertex: where two wavefront edges meet. Positions are relative to the simulation origin.
#[derive(Clone, Copy, Debug)]
struct Wv {
    /// Position at time `t0`, when the vertex was born.
    p0: Vec2,
    t0: f64,
    /// Velocity along the bisector of its two edges.
    vel: Vec2,
    /// The input edge of the wavefront edge arriving at this vertex, and of the one leaving it.
    ein: usize,
    eout: usize,
    /// The skeleton node where this vertex (and its ray) started.
    node: usize,
    /// The edges turn right: the vertex can run into other wavefront edges.
    reflex: bool,
    /// The edges are antiparallel: a zero-width spike with no velocity, to be collapsed before anything moves.
    spike: bool,
    /// The edges point the same way at different speeds: their lines are one line at this instant and part after it, so
    /// there is no vertex; the faster edge consumes the slower one (see `Sim::collapse_parallel`).
    parallel: bool,
}

impl Wv {
    fn at(&self, t: f64) -> Vec2 {
        self.p0 + self.vel * (t - self.t0)
    }
}

/// A skeleton arc and the two input edges (faces) on its sides.
#[derive(Clone, Copy, Debug)]
struct ArcRec {
    a: usize,
    b: usize,
    sides: (usize, usize),
}

/// The wavefront and the skeleton recorded so far.
struct Sim {
    /// Every input edge of every ring, rings in order.
    edges: Vec<EdgeLine>,
    /// The wavefront loops; each is a cyclic list where `lav[i].eout == lav[i + 1].ein`.
    lavs: Vec<Vec<Wv>>,
    nodes: Vec<SkeletonNode>,
    arcs: Vec<ArcRec>,
    origin: Vec2,
    tol: f64,
    /// Edge ids below this belong to the outset frame, not to the shape; loops touching them are reported as such.
    marked: usize,
    time: f64,
    /// The fastest edge: `RATE_EPS` and `CLOSE_EPS` are for edges moving at 1 and scale with this (1 for unit weights).
    speed: f64,
    /// Times closer than this are one instant: `tol` is a distance, and the fastest edge covers it in this long.
    time_tol: f64,
    /// Splits applied at the current instant, as (incoming edge, outgoing edge, cut edge). Near the tolerance a split and
    /// another repair can keep recreating each other's input; applying each split once per instant guarantees the
    /// repairs of one instant end. Cleared whenever time moves on.
    splits_done: Vec<(usize, usize, usize)>,
    ops: usize,
    budget: usize,
}

/// What `find_action` found to repair at the current instant.
enum Action {
    /// A loop with at most two vertices is finished.
    Finish(usize),
    /// Vertices at one point whose edges should pair up differently: see `merge_cluster`.
    Cluster(Vec<Addr>),
    /// A vertex between antiparallel edges: the shorter of the two edges is consumed by the longer one.
    Spike { lav: usize, i: usize },
    /// A vertex between parallel edges of different speeds: the faster edge consumes the slower one.
    Parallel { lav: usize, i: usize },
    /// A reflex vertex lies on a wavefront edge: the loop splits (or two loops merge).
    Split { lav: usize, v: usize, target_lav: usize, target: usize },
}

/// The earliest thing that will happen, as predicted from the current wavefront.
enum Event {
    Edge { lav: usize, i: usize },
    Split { lav: usize, v: usize, target_lav: usize, target: usize },
}

impl Sim {
    /// Wavefront of the given cleaned rings (first counter-clockwise, rest clockwise), all vertices at time 0, with edge `i`
    /// of ring `r` moving at `weights[r][i]` (a missing weight is 1).
    fn new(rings: &[Vec<Vec2>], weights: &[Vec<f64>], origin: Vec2, tol: f64, marked: usize) -> Sim {
        let vertices: usize = rings.iter().map(Vec::len).sum();
        let mut sim = Sim {
            edges: Vec::with_capacity(vertices),
            lavs: Vec::with_capacity(rings.len()),
            nodes: Vec::with_capacity(vertices * 2),
            arcs: Vec::new(),
            origin,
            tol,
            marked,
            time: 0.0,
            speed: 1.0,
            time_tol: tol,
            splits_done: Vec::new(),
            ops: 0,
            budget: vertices.saturating_mul(vertices).saturating_mul(64).saturating_add(1024),
        };
        for (r, ring) in rings.iter().enumerate() {
            let n = ring.len();
            for (i, &p) in ring.iter().enumerate() {
                let (a, b) = (p - origin, ring[(i + 1) % n] - origin);
                let dir = unit(b - a);
                let normal = dir.perp();
                let w = weights.get(r).and_then(|ws| ws.get(i)).copied().unwrap_or(1.0);
                sim.edges.push(EdgeLine { dir, normal, c: normal.dot(a), w });
                sim.nodes.push(SkeletonNode { pos: p, time: 0.0 });
            }
        }
        let fastest = sim.edges.iter().map(|e| e.w).filter(|w| w.is_finite()).fold(0.0, f64::max);
        if fastest > 0.0 {
            sim.speed = fastest;
            sim.time_tol = tol / fastest;
        }
        let mut base = 0;
        for ring in rings {
            let n = ring.len();
            let lav = (0..n).map(|i| sim.new_wv(base + (i + n - 1) % n, base + i, ring[i] - origin, 0.0, base + i)).collect();
            sim.lavs.push(lav);
            base += n;
        }
        sim
    }

    fn edge(&self, id: usize) -> EdgeLine {
        self.edges.get(id).copied().unwrap_or_default()
    }

    /// A wavefront vertex at `pos` (relative to the origin) at time `t`, between input edges `ein` and `eout`.
    fn new_wv(&self, ein: usize, eout: usize, pos: Vec2, t: f64, node: usize) -> Wv {
        let (e1, e2) = (self.edge(ein), self.edge(eout));
        let spike = e1.dir.dot(e2.dir) < 0.0 && e1.dir.cross(e2.dir).abs() < SPIKE_SIN;
        let parallel = e1.dir.dot(e2.dir) > 0.0 && e1.dir.cross(e2.dir).abs() < PARALLEL_SIN && e1.w != e2.w;
        Wv {
            p0: self.on_lines(e1, e2, pos, t),
            t0: t,
            vel: corner_velocity(e1.normal, e1.w, e2.normal, e2.w),
            ein,
            eout,
            node,
            reflex: e1.dir.cross(e2.dir) < -REFLEX_SIN,
            spike,
            parallel,
        }
    }

    /// `pos` moved onto the point where the offset lines of `e1` and `e2` meet at time `t`, when those lines cross at a
    /// healthy angle and the point is where `pos` roughly is. Events place vertices to within a tolerance; snapping them
    /// to their lines keeps that error from adding up over a long run of events.
    fn on_lines(&self, e1: EdgeLine, e2: EdgeLine, pos: Vec2, t: f64) -> Vec2 {
        let det = e1.normal.cross(e2.normal);
        if det.abs() < SNAP_MIN_SIN {
            return pos;
        }
        let (a, b) = (e1.c + e1.w * t, e2.c + e2.w * t);
        let x = Vec2::new((a * e2.normal.y - b * e1.normal.y) / det, (b * e1.normal.x - a * e2.normal.x) / det);
        if x.dist(pos) <= SNAP_REACH * self.tol { x } else { pos }
    }

    /// Count one operation against the budget.
    fn spend(&mut self) -> Result<(), SkeletonError> {
        self.ops += 1;
        if self.ops > self.budget { Err(SkeletonError::EventLimit) } else { Ok(()) }
    }

    /// The skeleton node at `pos` (relative to the origin) at time `t`: an existing one within the tolerance (events at
    /// one place are one node), otherwise a new one. Nodes are appended in time order, so the search can stop early.
    fn node_at(&mut self, pos: Vec2, t: f64) -> usize {
        let world = pos + self.origin;
        for (i, node) in self.nodes.iter().enumerate().rev() {
            if node.time < t - self.time_tol {
                break;
            }
            if (node.time - t).abs() <= self.time_tol && node.pos.dist(world) <= self.tol {
                return i;
            }
        }
        self.nodes.push(SkeletonNode { pos: world, time: t });
        self.nodes.len() - 1
    }

    /// Record an arc between two nodes, unless it is a point or already there.
    fn add_arc(&mut self, a: usize, b: usize, sides: (usize, usize)) {
        if a == b {
            return;
        }
        let same = |r: &ArcRec| ((r.a == a && r.b == b) || (r.a == b && r.b == a)) && (r.sides == sides || r.sides == (sides.1, sides.0));
        if !self.arcs.iter().any(same) {
            self.arcs.push(ArcRec { a, b, sides });
        }
    }

    /// The ray of `v` ends at `node`: record the arc it traced.
    fn end_ray(&mut self, v: &Wv, node: usize) {
        self.add_arc(v.node, node, (v.ein, v.eout));
    }

    /// Positions of every vertex at time `t`.
    fn positions(&self, t: f64) -> Vec<Vec<Vec2>> {
        self.lavs.iter().map(|lav| lav.iter().map(|v| v.at(t)).collect()).collect()
    }

    /// The wavefront at time `t` in the caller's coordinates; each loop is flagged when it touches a marked (frame) edge.
    fn loops_at(&self, t: f64) -> Vec<(Vec<Vec2>, bool)> {
        self.lavs
            .iter()
            .filter(|lav| !lav.is_empty())
            .map(|lav| (lav.iter().map(|v| v.at(t) + self.origin).collect(), lav.iter().any(|v| v.ein < self.marked)))
            .collect()
    }
}

/// Run the wavefront of `rings` for time `d` and return its loops (with the frame flag, see [`Sim::loops_at`]).
fn wavefront(rings: &[Vec<Vec2>], prep: &Prepared, d: f64, marked: usize) -> Result<Vec<(Vec<Vec2>, bool)>, SkeletonError> {
    let mut sim = Sim::new(rings, &[], prep.origin, prep.tol, marked);
    sim.run(Some(d))?;
    Ok(sim.loops_at(d))
}

// ---------------------------------------------------------------------------------------------------------------------
// Wavefront operations. Each takes the loop(s) it changes out of `lavs` (leaving an empty slot that `run` clears away),
// ends the rays of the vertices it removes at a skeleton node, and pushes the replacement loop(s).
// ---------------------------------------------------------------------------------------------------------------------

impl Sim {
    /// Take loop `l` out of the wavefront (an out-of-range index is a no-op: `None`).
    fn take_lav(&mut self, l: usize) -> Option<Vec<Wv>> {
        self.lavs.get_mut(l).map(std::mem::take)
    }

    /// A finished loop of at most two vertices: end every ray at the loop's final position. Two vertices that have not
    /// met are the two ends of a zero-width strip; the arc between them is the ridge shared by its two sides.
    fn finish(&mut self, l: usize, t: f64) -> Result<(), SkeletonError> {
        self.spend()?;
        let Some(lav) = self.take_lav(l) else { return Ok(()) };
        match lav.as_slice() {
            [a] => {
                let node = self.node_at(a.at(t), t);
                self.end_ray(a, node);
            }
            [a, b] => {
                let (na, nb) = (self.node_at(a.at(t), t), self.node_at(b.at(t), t));
                self.end_ray(a, na);
                self.end_ray(b, nb);
                self.add_arc(na, nb, (a.eout, b.eout));
            }
            _ => {}
        }
        Ok(())
    }

    /// The edge from `lav[i]` to `lav[i + 1]` has shrunk to nothing: both vertices become one, between their outer edges.
    fn collapse_edge(&mut self, l: usize, i: usize, t: f64) -> Result<(), SkeletonError> {
        self.spend()?;
        let Some(lav) = self.take_lav(l) else { return Ok(()) };
        let n = lav.len();
        if n < 3 || i >= n {
            self.lavs.push(lav);
            return Ok(());
        }
        let (a, b) = (lav[i], lav[(i + 1) % n]);
        let pos = a.at(t).mid(b.at(t));
        let node = self.node_at(pos, t);
        self.end_ray(&a, node);
        self.end_ray(&b, node);
        let merged = self.new_wv(a.ein, b.eout, pos, t, node);
        let mut out = Vec::with_capacity(n - 1);
        out.push(merged);
        out.extend((2..n).map(|k| lav[(i + k) % n]));
        self.lavs.push(out);
        Ok(())
    }

    /// `lav[i]` sits between antiparallel edges: the strip between them has no width, so the shorter edge is consumed by
    /// the longer one. The overlap becomes a ridge arc (shared by the faces of the two edges); the vertex at the end of
    /// the shorter edge now joins its other edge to the longer one. Equal edges consume each other and both neighbours
    /// become one vertex.
    fn collapse_spike(&mut self, l: usize, i: usize, t: f64) -> Result<(), SkeletonError> {
        self.spend()?;
        let Some(lav) = self.take_lav(l) else { return Ok(()) };
        let n = lav.len();
        if n < 3 || i >= n {
            self.lavs.push(lav);
            return Ok(());
        }
        let (p, v, q) = (lav[(i + n - 1) % n], lav[i], lav[(i + 1) % n]);
        let (pp, pv, pq) = (p.at(t), v.at(t), q.at(t));
        let (len_in, len_out) = (pp.dist(pv), pq.dist(pv));
        let tip = self.node_at(pv, t);
        self.end_ray(&v, tip);
        let ridge = (v.ein, v.eout);
        let out = if (len_in - len_out).abs() <= self.tol {
            let pos = pp.mid(pq);
            let node = self.node_at(pos, t);
            self.end_ray(&p, node);
            self.end_ray(&q, node);
            self.add_arc(tip, node, ridge);
            let merged = self.new_wv(p.ein, q.eout, pos, t, node);
            std::iter::once(merged).chain((2..n - 1).map(|k| lav[(i + k) % n])).collect()
        } else if len_in < len_out {
            // The incoming edge is the shorter: `p` lies on the outgoing edge and now starts it.
            let node = self.node_at(pp, t);
            self.end_ray(&p, node);
            self.add_arc(tip, node, ridge);
            let merged = self.new_wv(p.ein, v.eout, pp, t, node);
            std::iter::once(merged).chain((1..n - 1).map(|k| lav[(i + k) % n])).collect()
        } else {
            // The outgoing edge is the shorter: `q` lies on the incoming edge and now ends it.
            let node = self.node_at(pq, t);
            self.end_ray(&q, node);
            self.add_arc(tip, node, ridge);
            let merged = self.new_wv(v.ein, q.eout, pq, t, node);
            std::iter::once(merged).chain((2..n).map(|k| lav[(i + k) % n])).collect()
        };
        self.lavs.push(out);
        Ok(())
    }

    /// `lav[i]` sits between two parallel edges of different speeds, which lie on one line at this instant (the faster one
    /// has just caught up with the slower one) and part from now on. Where they overlap, the faster edge is the one still
    /// on the wavefront, so it consumes the slower one: the vertex at the far end of the slower edge now joins the faster
    /// edge to its other neighbour, at the same place (the lines coincide, so it is still on both). The slower edge's last
    /// position is a ridge arc between the two faces, as for a spike.
    fn collapse_parallel(&mut self, l: usize, i: usize, t: f64) -> Result<(), SkeletonError> {
        self.spend()?;
        let Some(lav) = self.take_lav(l) else { return Ok(()) };
        let n = lav.len();
        if n < 3 || i >= n {
            self.lavs.push(lav);
            return Ok(());
        }
        let v = lav[i];
        let tip = self.node_at(v.at(t), t);
        self.end_ray(&v, tip);
        let ridge = (v.ein, v.eout);
        let out = if self.edge(v.ein).w > self.edge(v.eout).w {
            // The incoming edge is the faster: it takes over the outgoing edge, whose far end `q` now ends it.
            let q = lav[(i + 1) % n];
            let pq = q.at(t);
            let node = self.node_at(pq, t);
            self.end_ray(&q, node);
            self.add_arc(tip, node, ridge);
            let merged = self.new_wv(v.ein, q.eout, pq, t, node);
            std::iter::once(merged).chain((2..n).map(|k| lav[(i + k) % n])).collect()
        } else {
            // The outgoing edge is the faster: it takes over the incoming edge, whose far end `p` now starts it.
            let p = lav[(i + n - 1) % n];
            let pp = p.at(t);
            let node = self.node_at(pp, t);
            self.end_ray(&p, node);
            self.add_arc(tip, node, ridge);
            let merged = self.new_wv(p.ein, v.eout, pp, t, node);
            std::iter::once(merged).chain((1..n - 1).map(|k| lav[(i + k) % n])).collect()
        };
        self.lavs.push(out);
        Ok(())
    }

    /// Reflex vertex `lav[v]` has reached the wavefront edge from `target_lav[target]` to the next vertex. The edge is cut
    /// at that point and the vertex is replaced by two: one joining its incoming edge to the cut edge, the other joining the
    /// cut edge to its outgoing edge. In one loop that splits the loop in two; across two loops it merges them into one.
    fn split(&mut self, l: usize, v: usize, m: usize, a: usize, t: f64) -> Result<(), SkeletonError> {
        self.spend()?;
        let (Some(lv), Some(lm)) = (self.lavs.get(l).cloned(), self.lavs.get(m).cloned()) else { return Ok(()) };
        let (n, k) = (lv.len(), lm.len());
        if v >= n || a >= k || (l == m && (a == v || (a + 1) % n == v)) {
            return Ok(());
        }
        let (vert, edge_id) = (lv[v], lm[a].eout);
        self.splits_done.push((vert.ein, vert.eout, edge_id));
        let hit = vert.at(t);
        // At an end of the cut edge the two vertices coincide: `merge_cluster` pairs their edges (or leaves them as they are).
        if hit.dist(lm[a].at(t)) <= self.tol || hit.dist(lm[(a + 1) % k].at(t)) <= self.tol {
            return Ok(());
        }
        let node = self.node_at(hit, t);
        self.end_ray(&vert, node);
        // `before` ends on the cut edge (coming from `lm[a]`) and leaves on the vertex's outgoing edge; `after` arrives on
        // the vertex's incoming edge and leaves along the rest of the cut edge towards `lm[a + 1]`.
        let before = self.new_wv(edge_id, vert.eout, hit, t, node);
        let after = self.new_wv(vert.ein, edge_id, hit, t, node);
        if l == m {
            let u = (a + n - v) % n;
            let first: Vec<Wv> = std::iter::once(before).chain((1..=u).map(|j| lv[(v + j) % n])).collect();
            let second: Vec<Wv> = std::iter::once(after).chain((u + 1..n).map(|j| lv[(v + j) % n])).collect();
            self.take_lav(l);
            self.lavs.push(first);
            self.lavs.push(second);
        } else {
            let merged: Vec<Wv> = std::iter::once(after)
                .chain((1..=k).map(|j| lm[(a + j) % k]))
                .chain(std::iter::once(before))
                .chain((1..n).map(|j| lv[(v + j) % n]))
                .collect();
            self.take_lav(l);
            self.take_lav(m);
            self.lavs.push(merged);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Coincident vertices
// ---------------------------------------------------------------------------------------------------------------------
//
// Vertices of the wavefront that sit at one point (within the tolerance) are repaired together. Around the point the
// wavefront edges that enter and leave it are laid out by angle; each leaving edge pairs with the next entering edge
// counter-clockwise (the material between them is that vertex's wedge), with parenthesis matching when wedges overlap.
// The result depends only on the geometry, never on the pairing it started from, so repairing a group twice changes
// nothing. It covers an edge shrinking to nothing, two reflex vertices meeting and several micro edges collapsing at once.

/// A wavefront vertex address: loop index and index in that loop.
type Addr = (usize, usize);

/// What a group of coincident vertices turns into.
struct ClusterPlan {
    /// Each run of consecutive group members of one loop: (loop, first index, last index, length).
    runs: Vec<(usize, usize, usize, usize)>,
    /// The new vertices as (index of the run whose entering edge it takes, index of the run whose leaving edge it takes).
    pairs: Vec<(usize, usize)>,
    /// Whether this differs from the vertices the group already has.
    changed: bool,
}

impl Sim {
    /// The group of vertices `members` (all at one point) as runs of consecutive vertices and the planar re-pairing of
    /// the edges that enter and leave the point. `None` when the rays cannot be matched.
    fn plan_cluster(&self, members: &[Addr]) -> Option<ClusterPlan> {
        let inside = |l: usize, i: usize| members.contains(&(l, i));
        let mut runs = Vec::new();
        let mut internal = false;
        for &(l, i) in members {
            let n = self.lavs.get(l)?.len();
            if n == 0 {
                return None;
            }
            // A run starts at a member whose predecessor is not a member.
            if inside(l, (i + n - 1) % n) {
                internal = true;
                continue;
            }
            let (mut last, mut len) = (i, 1);
            while inside(l, (last + 1) % n) && len < n {
                last = (last + 1) % n;
                len += 1;
            }
            runs.push((l, i, last, len));
        }
        if runs.is_empty() {
            // Every member follows another one: the group is whole loops that have shrunk to a point.
            return Some(ClusterPlan { runs, pairs: Vec::new(), changed: true });
        }
        // Rays leaving the point, by angle: (angle key, leaving?, run index).
        let mut rays: Vec<(i64, bool, usize)> = Vec::with_capacity(runs.len() * 2);
        for (r, &(l, first, last, _)) in runs.iter().enumerate() {
            let lav = self.lavs.get(l)?;
            let (entering, leaving) = (self.edge(lav.get(first)?.ein).dir, self.edge(lav.get(last)?.eout).dir);
            let key = |v: Vec2| (v.y.atan2(v.x) * 1e9).round() as i64;
            rays.push((key(-entering), false, r));
            rays.push((key(leaving), true, r));
        }
        // At equal angles a leaving edge goes first: antiparallel edges are a spike of no width, not a full turn.
        rays.sort_by_key(|&(angle, leaving, run)| (angle, !leaving, run));
        // Parenthesis matching round the circle, starting where the running balance is lowest.
        let mut balance = 0i64;
        let (mut lowest, mut start) = (0i64, 0usize);
        for (k, &(_, leaving, _)) in rays.iter().enumerate() {
            balance += if leaving { 1 } else { -1 };
            if balance < lowest {
                (lowest, start) = (balance, k + 1);
            }
        }
        let (mut open, mut pairs) = (Vec::new(), Vec::new());
        for k in 0..rays.len() {
            let &(_, leaving, run) = rays.get((start + k) % rays.len())?;
            if leaving {
                open.push(run);
            } else {
                pairs.push((run, open.pop()?));
            }
        }
        if !open.is_empty() {
            return None;
        }
        // Unchanged when every member is a run of its own and the new vertices carry the same pairs of edges.
        let current = |&(l, i): &Addr| self.lavs.get(l).and_then(|lav| lav.get(i)).map(|v| (v.ein, v.eout));
        let mut old: Vec<_> = members.iter().filter_map(current).collect();
        let mut new: Vec<_> = pairs
            .iter()
            .filter_map(|&(i, o)| {
                let (&(li, fi, _, _), &(lo, _, lasto, _)) = (runs.get(i)?, runs.get(o)?);
                Some((self.lavs.get(li)?.get(fi)?.ein, self.lavs.get(lo)?.get(lasto)?.eout))
            })
            .collect();
        old.sort_unstable();
        new.sort_unstable();
        Some(ClusterPlan { changed: internal || old != new, runs, pairs })
    }

    /// Replace the coincident vertices `members` by the vertices of their plan, relinking the loops they belonged to.
    fn merge_cluster(&mut self, members: &[Addr], t: f64) -> Result<(), SkeletonError> {
        self.spend()?;
        let Some(plan) = self.plan_cluster(members) else { return Ok(()) };
        // Flatten the loops involved: `all` holds their vertices, `next` the cyclic successor of each.
        let mut touched: Vec<usize> = members.iter().map(|&(l, _)| l).collect();
        touched.sort_unstable();
        touched.dedup();
        let mut base = Vec::with_capacity(touched.len());
        let (mut all, mut next): (Vec<Wv>, Vec<usize>) = (Vec::new(), Vec::new());
        for &l in &touched {
            let Some(lav) = self.lavs.get(l) else { return Ok(()) };
            base.push((l, all.len()));
            let first = all.len();
            for (i, v) in lav.iter().enumerate() {
                all.push(*v);
                next.push(first + (i + 1) % lav.len());
            }
        }
        let flat = |l: usize, i: usize| base.iter().find(|&&(bl, _)| bl == l).map(|&(_, b)| b + i);
        let mut removed = vec![false; all.len()];
        let mut at = Vec2::ZERO;
        for &(l, i) in members {
            let Some(f) = flat(l, i) else { return Ok(()) };
            removed[f] = true;
            at += all[f].at(t);
        }
        at = at / members.len() as f64;
        let node = self.node_at(at, t);
        for &(l, i) in members {
            if let Some(f) = flat(l, i) {
                let v = all[f];
                self.end_ray(&v, node);
            }
        }
        // Where each run is attached: the vertex before its first and the vertex after its last member.
        let ends: Vec<(usize, usize, usize, usize)> = plan
            .runs
            .iter()
            .filter_map(|&(l, first, last, _)| {
                let (f, e) = (flat(l, first)?, flat(l, last)?);
                let before = (0..all.len()).find(|&p| next[p] == f && !removed[p]);
                Some((f, e, before.unwrap_or(usize::MAX), next[e]))
            })
            .collect();
        // New vertices: take the edge entering one run and the edge leaving another, and link their neighbours to them.
        for &(entering, leaving) in &plan.pairs {
            let (Some(&(fi, _, before, _)), Some(&(_, ej, _, after))) = (ends.get(entering), ends.get(leaving)) else { return Ok(()) };
            let (ein, eout) = (all[fi].ein, all[ej].eout);
            let fresh = self.new_wv(ein, eout, at, t, node);
            let id = all.len();
            all.push(fresh);
            next.push(after);
            removed.push(false);
            if let Some(slot) = next.get_mut(before) {
                *slot = id;
            }
        }
        // Rebuild the loops by following the links.
        for &l in &touched {
            self.take_lav(l);
        }
        let mut seen = vec![false; all.len()];
        for s in 0..all.len() {
            if removed[s] || seen[s] {
                continue;
            }
            let (mut lav, mut here) = (Vec::new(), s);
            while !removed[here] && !seen[here] {
                seen[here] = true;
                lav.push(all[here]);
                here = next[here];
            }
            if !lav.is_empty() {
                self.lavs.push(lav);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Events, repair and the main loop
// ---------------------------------------------------------------------------------------------------------------------

impl Sim {
    /// The first coincidence at time `t` that needs repairing, in priority order: finished loops, groups of coincident
    /// vertices, zero-width spikes, then reflex vertices lying on a wavefront edge.
    fn find_action(&self, t: f64) -> Option<Action> {
        let pos = self.positions(t);
        if let Some(lav) = self.lavs.iter().position(|lav| lav.len() <= 2) {
            return Some(Action::Finish(lav));
        }
        if let Some(group) = self.find_cluster(&pos) {
            return Some(Action::Cluster(group));
        }
        for (lav, vertices) in self.lavs.iter().enumerate() {
            let n = vertices.len();
            let spike = (0..n)
                .find(|&i| vertices[i].spike || self.is_thin_wedge(&vertices[i], pos[lav][(i + n - 1) % n], pos[lav][i], pos[lav][(i + 1) % n]));
            if let Some(i) = spike {
                return Some(Action::Spike { lav, i });
            }
        }
        for (lav, vertices) in self.lavs.iter().enumerate() {
            if let Some(i) = vertices.iter().position(|v| v.parallel) {
                return Some(Action::Parallel { lav, i });
            }
        }
        // Only vertices and edges made at this instant can newly coincide; any other simultaneous hit is an event that
        // `next_event` predicts. (At the start everything is new.)
        let fresh = |v: &Wv| v.t0 >= t;
        for (lav, vertices) in self.lavs.iter().enumerate() {
            let n = vertices.len();
            for (v, vert) in vertices.iter().enumerate().filter(|(_, vert)| vert.reflex) {
                for (target_lav, others) in self.lavs.iter().enumerate() {
                    let k = others.len();
                    for (target, other) in others.iter().enumerate() {
                        if target_lav == lav && (target == v || (target + 1) % n == v) {
                            continue;
                        }
                        if !(fresh(vert) || fresh(other) || fresh(&others[(target + 1) % k])) {
                            continue;
                        }
                        let line = self.edge(other.eout);
                        if line.w - line.normal.dot(vert.vel) <= CLOSE_EPS * self.speed
                            || self.splits_done.contains(&(vert.ein, vert.eout, other.eout))
                        {
                            continue;
                        }
                        let (a, b, p) = (pos[target_lav][target], pos[target_lav][(target + 1) % k], pos[lav][v]);
                        // Near an end of the edge the two vertices coincide: that is a group, repaired as one.
                        if dist_point_segment(p, a, b) <= self.tol && p.dist(a) > self.tol && p.dist(b) > self.tol {
                            return Some(Action::Split { lav, v, target_lav, target });
                        }
                    }
                }
            }
        }
        None
    }

    /// A group of vertices within the tolerance of each other whose edges pair up differently from the way they do now.
    fn find_cluster(&self, pos: &[Vec<Vec2>]) -> Option<Vec<Addr>> {
        let mut order: Vec<(f64, Addr)> = Vec::new();
        for (l, p) in pos.iter().enumerate() {
            order.extend(p.iter().enumerate().map(|(i, q)| (q.x, (l, i))));
        }
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = |k: usize| pos[order[k].1.0][order[k].1.1];
        // Union-find over the sorted vertices: only neighbours within the tolerance in x need a distance test.
        let mut parent: Vec<usize> = (0..order.len()).collect();
        for a in 0..order.len() {
            for b in a + 1..order.len() {
                if order[b].0 - order[a].0 > self.tol {
                    break;
                }
                if at(a).dist(at(b)) <= self.tol {
                    let (ra, rb) = (find_root(&mut parent, a), find_root(&mut parent, b));
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        let mut groups: Vec<Vec<Addr>> = vec![Vec::new(); order.len()];
        for (k, &(_, addr)) in order.iter().enumerate() {
            let r = find_root(&mut parent, k);
            groups[r].push(addr);
        }
        groups.into_iter().filter(|g| g.len() >= 2).find(|g| self.plan_cluster(g).is_some_and(|plan| plan.changed))
    }

    /// Whether the two edges at `v` (between the neighbours at `prev` and `next`) double back on each other so closely
    /// that the strip between them is nowhere wider than a few tolerances. Only the overlap counts, and that ends at the
    /// nearer neighbour: the far end of the shorter edge lies within that distance of the longer edge's line. Such a wedge
    /// is a zero-width spike whatever its orientation: its bisector speed is huge and, with the tolerance-sized errors in
    /// the position of `v`, may even point the wrong way along the strip, so it is collapsed instead of followed.
    fn is_thin_wedge(&self, v: &Wv, prev: Vec2, here: Vec2, next: Vec2) -> bool {
        let (d1, d2) = (self.edge(v.ein).dir, self.edge(v.eout).dir);
        let cross = d1.cross(d2);
        if d1.dot(d2) >= 0.0 || cross.abs() > THIN_WEDGE_SIN {
            return false;
        }
        // A vertex made by an event cannot truly turn right while doubling back: the strip has closed over by more than
        // the tolerance can resolve, and what is left of it is a spike whatever its width.
        let inverted = v.t0 > self.time_tol && cross < 0.0;
        inverted || cross.abs() * prev.dist(here).min(next.dist(here)) <= THIN_WEDGE * self.tol
    }

    /// Repair everything that coincides at time `t`, one local operation at a time, until nothing is left. Every
    /// operation removes a vertex or leaves a group in its planar pairing, so this ends; the budget guards the
    /// numerically odd cases.
    fn resolve(&mut self, t: f64) -> Result<(), SkeletonError> {
        loop {
            self.lavs.retain(|lav| !lav.is_empty());
            match self.find_action(t) {
                None => return Ok(()),
                Some(Action::Finish(lav)) => self.finish(lav, t)?,
                Some(Action::Cluster(group)) => self.merge_cluster(&group, t)?,
                Some(Action::Spike { lav, i }) => self.collapse_spike(lav, i, t)?,
                Some(Action::Parallel { lav, i }) => self.collapse_parallel(lav, i, t)?,
                Some(Action::Split { lav, v, target_lav, target }) => self.split(lav, v, target_lav, target, t)?,
            }
        }
    }

    /// The earliest event from time `now` on, with its time. Edge events: the two vertices of an edge meet. Split events:
    /// a reflex vertex reaches the line of a non-adjacent wavefront edge at a point within that edge. The strictly
    /// earlier one wins, however close the times are: vertices can be fast, and a time gap far below the tolerance can be
    /// a distance above it (an edge event first would let the reflex vertex run through its wall unnoticed). Exact ties
    /// go to the edge event, and `resolve` picks up whatever else happens at the same place.
    fn next_event(&self, now: f64) -> Option<(f64, Event)> {
        let pos = self.positions(now);
        let mut edge: Option<(f64, Event)> = None;
        let mut split: Option<(f64, Event)> = None;
        for (lav, vertices) in self.lavs.iter().enumerate() {
            let n = vertices.len();
            for (i, a) in vertices.iter().enumerate() {
                let b = &vertices[(i + 1) % n];
                let dir = self.edge(a.eout).dir;
                let rate = dir.dot(b.vel - a.vel);
                if rate >= -RATE_EPS * self.speed {
                    continue;
                }
                let length = dir.dot(pos[lav][(i + 1) % n] - pos[lav][i]);
                let time = now + length.max(0.0) / -rate;
                if time.is_finite() && edge.as_ref().is_none_or(|(best, _)| time < *best) {
                    edge = Some((time, Event::Edge { lav, i }));
                }
            }
        }
        for (lav, vertices) in self.lavs.iter().enumerate() {
            let n = vertices.len();
            for (v, vert) in vertices.iter().enumerate().filter(|(_, vert)| vert.reflex) {
                let pv = pos[lav][v];
                for (target_lav, others) in self.lavs.iter().enumerate() {
                    let k = others.len();
                    for (target, a) in others.iter().enumerate() {
                        if target_lav == lav && (target == v || (target + 1) % n == v) {
                            continue;
                        }
                        let line = self.edge(a.eout);
                        let closing = line.w - line.normal.dot(vert.vel);
                        let ahead = line.normal.dot(pv) - (line.c + line.w * now);
                        if closing <= CLOSE_EPS * self.speed || ahead < -self.tol || self.splits_done.contains(&(vert.ein, vert.eout, a.eout)) {
                            continue;
                        }
                        let dt = ahead.max(0.0) / closing;
                        let time = now + dt;
                        let b = &others[(target + 1) % k];
                        let s = line.dir.dot(pv + vert.vel * dt);
                        if !time.is_finite()
                            || s < line.dir.dot(a.at(time)) - self.tol
                            || s > line.dir.dot(b.at(time)) + self.tol
                            || split.as_ref().is_some_and(|(best, _)| time >= *best)
                        {
                            continue;
                        }
                        split = Some((time, Event::Split { lav, v, target_lav, target }));
                    }
                }
            }
        }
        match (edge, split) {
            (Some(e), Some(s)) => Some(if s.0 < e.0 { s } else { e }),
            (e, s) => e.or(s),
        }
    }

    /// Every edge still on the wavefront has weight 0.
    fn only_pinned_edges_left(&self) -> bool {
        self.lavs.iter().flatten().all(|v| self.edge(v.ein).w == 0.0 && self.edge(v.eout).w == 0.0)
    }

    /// Simulate until the wavefront has vanished, or, with `stop`, until time `stop`. Without `stop` a wavefront that
    /// stops having events before it has vanished is an error (`EventLimit`; `BadWeights` when what is left is only edges
    /// that never move).
    fn run(&mut self, stop: Option<f64>) -> Result<(), SkeletonError> {
        self.resolve(self.time)?;
        loop {
            self.lavs.retain(|lav| !lav.is_empty());
            if self.lavs.is_empty() {
                return Ok(());
            }
            self.spend()?;
            let Some((time, event)) = self.next_event(self.time) else {
                return match stop {
                    Some(_) => Ok(()),
                    // Only edges that never move are left: nothing will ever sweep what they enclose.
                    None if self.only_pinned_edges_left() => Err(SkeletonError::BadWeights),
                    None => Err(SkeletonError::EventLimit),
                };
            };
            if stop.is_some_and(|stop| time > stop) {
                return Ok(());
            }
            if time > self.time {
                self.splits_done.clear();
            }
            self.time = time.max(self.time);
            let t = self.time;
            match event {
                Event::Edge { lav, i } => self.collapse_edge(lav, i, t)?,
                Event::Split { lav, v, target_lav, target } => self.split(lav, v, target_lav, target, t)?,
            }
            self.resolve(t)?;
        }
    }
}

/// The representative of `k` in a union-find forest (with path halving).
fn find_root(parent: &mut [usize], mut k: usize) -> usize {
    while parent[k] != k {
        parent[k] = parent[parent[k]];
        k = parent[k];
    }
    k
}

// ---------------------------------------------------------------------------------------------------------------------
// Faces
// ---------------------------------------------------------------------------------------------------------------------

/// One face per input edge. A face is bounded by its edge and by every arc that has that edge on one of its sides; walked
/// from the edge's start vertex to its end vertex and onwards along those arcs it comes back to the start. An arc with the
/// edge on both sides (a bridge to an island, see `walk_face`) is walked twice.
fn build_faces(rings: &[Vec<Vec2>], arcs: &[ArcRec], nodes: &[SkeletonNode]) -> Vec<SkeletonFace> {
    let total: usize = rings.iter().map(Vec::len).sum();
    let mut bordering: Vec<Vec<usize>> = vec![Vec::new(); total];
    for (i, arc) in arcs.iter().enumerate() {
        if let Some(list) = bordering.get_mut(arc.sides.0) {
            list.push(i);
        }
        if let Some(list) = bordering.get_mut(arc.sides.1) {
            list.push(i);
        }
    }
    let mut faces = Vec::with_capacity(total);
    let (mut edge_id, mut base) = (0, 0);
    for (ring, pts) in rings.iter().enumerate() {
        let n = pts.len();
        for edge in 0..n {
            let list = bordering.get(edge_id).map(Vec::as_slice).unwrap_or_default();
            faces.push(SkeletonFace { ring, edge, nodes: walk_face(edge_id, base + edge, base + (edge + 1) % n, list, arcs, nodes) });
            edge_id += 1;
        }
        base += n;
    }
    faces
}

/// The node loop of a face: `start`, `end`, then the arcs in `list` followed from `end` until they lead back to `start`.
/// At a node where the face touches itself there is a choice. Taking the first unused arc is simplest, but can take a
/// shortcut round something the face encloses, or walk an enclosed island the wrong way round; so the face is also walked by
/// turning (at every fork the arc that keeps the face on the left: the first clockwise from the way back; an island is
/// walked clockwise). The simple walk is kept when it covers the same area, the turning walk otherwise. A face that
/// encloses an island hangs the island on a bridge arc with the face on both sides (in `list` twice, walked there and
/// back); such a face is always walked by turning.
fn walk_face(face: usize, start: usize, end: usize, list: &[usize], arcs: &[ArcRec], nodes: &[SkeletonNode]) -> Vec<usize> {
    let bridged = {
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted.windows(2).any(|pair| pair[0] == pair[1])
    };
    let (turning, turning_used) = walk_face_once(face, start, end, list, arcs, nodes, true);
    if bridged {
        return turning;
    }
    let (simple, simple_used) = walk_face_once(face, start, end, list, arcs, nodes, false);
    let outline = |walk: &[usize]| -> Vec<Vec2> { walk.iter().filter_map(|&n| nodes.get(n).map(|node| node.pos)).collect() };
    let (simple_outline, turning_outline) = (outline(&simple), outline(&turning));
    let reach = extent(turning_outline.iter().copied()).map_or(0.0, |(lo, hi)| (hi.x - lo.x).max(hi.y - lo.y));
    let same_area = (shoelace(&simple_outline) - shoelace(&turning_outline)).abs() <= EPS * reach * reach;
    if same_area || simple_used > turning_used { simple } else { turning }
}

/// The arcs of one face, for walking round it.
struct FaceArcs<'a> {
    /// The face (its edge, counted over all rings).
    face: usize,
    /// The arcs bordering the face (a bridge arc, with the face on both sides, twice).
    list: &'a [usize],
    arcs: &'a [ArcRec],
    nodes: &'a [SkeletonNode],
    /// Arcs this short have no direction: a pole, or the gap between two nodes that events at nearly one place left a hair apart.
    short: f64,
}

impl FaceArcs<'_> {
    fn position(&self, node: usize) -> Option<Vec2> {
        self.nodes.get(node).map(|n| n.pos)
    }

    /// The unused arcs of the face at `node`, as (index in `list`, node at the other end).
    fn leaving(&self, node: usize, used: &[bool]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (k, &ai) in self.list.iter().enumerate() {
            let Some(arc) = self.arcs.get(ai) else { continue };
            if used.get(k).copied().unwrap_or(true) {
                continue;
            }
            if arc.a == node {
                out.push((k, arc.b));
            } else if arc.b == node {
                out.push((k, arc.a));
            }
        }
        out
    }

    /// How far clockwise, in [0, 2 pi], the way on along list entry `k` from `at` to `to` lies from the way back `back`. An arc
    /// along the way back comes first (the face is the strip of no width between the two), the arc just walked (`arrived`)
    /// last. A short arc counts as the best way on from its far end. Arcs on one line (strips of no width between faces)
    /// are told apart by their sides: an arc's sides are (left, right) of its way from `a` to `b`, so going away from `at`
    /// the face we are walking must be on the left of the next arc (counter-clockwise side, the one towards the way back);
    /// a bridge (the face on both sides) comes before the arcs that have another face on their far side.
    fn turn(&self, (k, to): (usize, usize), at: usize, back: Vec2, arrived: Option<usize>, used: &[bool], depth: usize) -> f64 {
        let (Some(p), Some(q)) = (self.position(at), self.position(to)) else { return 0.0 };
        let arc = self.list.get(k).copied();
        if arrived.is_some() && arc == arrived {
            return crate::TAU;
        }
        if p.dist(q) <= self.short && depth < 4 {
            let beyond = self.leaving(to, used).into_iter().filter(|&(k2, _)| self.list.get(k2).copied() != arc);
            return beyond.map(|c| self.turn(c, to, back, None, used, depth + 1)).fold(crate::TAU, f64::min);
        }
        let bias = arc.and_then(|ai| self.arcs.get(ai)).map_or(0.0, |arc| {
            let (left, right) = if arc.a == at { arc.sides } else { (arc.sides.1, arc.sides.0) };
            match (left == self.face, right == self.face) {
                (true, true) => 2e-6,
                (true, false) => 1e-6,
                _ => 0.0,
            }
        });
        let on = q - p;
        if on.len() <= 0.0 || back.len() <= 0.0 {
            return -bias;
        }
        // On the line of the way back (the end of the arc within a tolerance of it) and in the same direction.
        if back.cross(on).abs() <= self.short * back.len() && back.dot(on) > 0.0 {
            return -bias;
        }
        let cw = -back.cross(on).atan2(back.dot(on));
        (if cw.is_finite() && cw > 0.0 { cw } else { cw + crate::TAU }) - bias
    }
}

/// One walk of a face (see `walk_face`), `turning` choosing the arc at forks by angle; and how many of the face's arcs it used.
fn walk_face_once(
    id: usize,
    start: usize,
    end: usize,
    list: &[usize],
    arcs: &[ArcRec],
    nodes: &[SkeletonNode],
    turning: bool,
) -> (Vec<usize>, usize) {
    let reach = extent(nodes.iter().map(|node| node.pos)).map_or(0.0, |(lo, hi)| (hi.x - lo.x).max(hi.y - lo.y));
    let face = FaceArcs { face: id, list, arcs, nodes, short: 1e-6 * reach };
    let mut used = vec![false; list.len()];
    let mut walk = vec![start, end];
    let mut back = face.position(start).zip(face.position(end)).map_or(Vec2::ZERO, |(s, e)| s - e);
    let (mut here, mut arrived) = (end, None);
    for _ in 0..=list.len() {
        let leaving = face.leaving(here, &used);
        let step = match leaving.as_slice() {
            [] => None,
            [first, ..] if !turning => Some(*first),
            many => many.iter().map(|&c| (face.turn(c, here, back, arrived, &used, 0), c)).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, c)| c),
        };
        let Some((k, next)) = step else { break };
        used[k] = true;
        if next == start {
            break;
        }
        walk.push(next);
        if let Some((p, q)) = face.position(here).zip(face.position(next))
            && p.dist(q) > face.short
        {
            back = p - q;
        }
        (here, arrived) = (next, list.get(k).copied());
    }
    let count = used.iter().filter(|&&u| u).count();
    (walk, count)
}

// ---------------------------------------------------------------------------------------------------------------------
// Invariant checks
// ---------------------------------------------------------------------------------------------------------------------

/// One finite weight >= 0 per ring edge.
fn check_weights(skeleton: &Skeleton, problems: &mut Vec<String>) {
    let shaped = skeleton.weights.len() == skeleton.rings.len() && skeleton.weights.iter().zip(&skeleton.rings).all(|(w, r)| w.len() == r.len());
    if !shaped {
        problems.push("the weights do not match the rings edge for edge".to_string());
    }
    if skeleton.weights.iter().flatten().any(|w| !w.is_finite() || *w < 0.0) {
        problems.push("a weight is negative or not finite".to_string());
    }
}

/// The first nodes are the ring vertices, in order, at time 0.
fn check_ring_nodes(skeleton: &Skeleton, ring_vertices: usize, tol: f64, problems: &mut Vec<String>) {
    if skeleton.nodes.len() < ring_vertices {
        problems.push(format!("{} nodes, fewer than the {ring_vertices} ring vertices", skeleton.nodes.len()));
        return;
    }
    let mut index = 0;
    for (r, ring) in skeleton.rings.iter().enumerate() {
        for (k, p) in ring.iter().enumerate() {
            if let Some(node) = skeleton.nodes.get(index)
                && !(node.pos.near(*p, tol) && node.time.abs() <= tol)
            {
                problems.push(format!(
                    "node {index} should be vertex {k} of ring {r} at ({}, {}) with time 0, but is at ({}, {}) with time {}",
                    p.x, p.y, node.pos.x, node.pos.y, node.time
                ));
            }
            index += 1;
        }
    }
}

/// Check one face (distance of every node from the edge's line equals the edge's weight times the node's time,
/// counter-clockwise, starts at the edge).
/// Returns the face's area for the total-area check.
fn check_face(skeleton: &Skeleton, fi: usize, face: &SkeletonFace, tol: f64, area_tol: f64, problems: &mut Vec<String>) -> f64 {
    let Some(ring) = skeleton.rings.get(face.ring) else {
        problems.push(format!("face {fi} refers to ring {}, which does not exist", face.ring));
        return 0.0;
    };
    let n = ring.len();
    let (Some(&a), Some(&b)) = (ring.get(face.edge), ring.get((face.edge + 1) % n.max(1))) else {
        problems.push(format!("face {fi} refers to edge {} of ring {}, which does not exist", face.edge, face.ring));
        return 0.0;
    };
    let base: usize = skeleton.rings.iter().take(face.ring).map(Vec::len).sum();
    if face.nodes.first() != Some(&(base + face.edge)) || face.nodes.get(1) != Some(&(base + (face.edge + 1) % n)) {
        problems.push(format!("face {fi} (ring {} edge {}) does not start with the two end vertices of its edge", face.ring, face.edge));
    }
    let dir = unit(b - a);
    let weight = skeleton.weights.get(face.ring).and_then(|ws| ws.get(face.edge)).copied().unwrap_or(1.0);
    let mut pts = Vec::with_capacity(face.nodes.len());
    for &ni in &face.nodes {
        let Some(node) = skeleton.nodes.get(ni) else {
            problems.push(format!("face {fi} refers to node {ni}, which does not exist"));
            continue;
        };
        // Signed distance to the left of the edge (the inside) must be the edge's weight times the node's time.
        let dist = dir.cross(node.pos - a);
        if (dist - weight * node.time).abs() > tol {
            problems.push(format!(
                "face {fi} (ring {} edge {}): node {ni} is {dist} from the edge's line but has time {} (edge weight {weight})",
                face.ring, face.edge, node.time
            ));
        }
        pts.push(node.pos);
    }
    let area = shoelace(&pts);
    if area < -area_tol {
        problems.push(format!("face {fi} (ring {} edge {}) runs clockwise (area {area})", face.ring, face.edge));
    }
    area
}

/// Whether `p` is in the region (inside the outer ring, outside every hole) or within `tol` of its boundary.
fn in_region_or_on_boundary(rings: &[Vec<Vec2>], p: Vec2, tol: f64) -> bool {
    let on_boundary = rings.iter().any(|ring| {
        let n = ring.len();
        (0..n).any(|i| dist_point_segment(p, ring[i], ring[(i + 1) % n]) <= tol)
    });
    if on_boundary {
        return true;
    }
    let Some((outer, holes)) = rings.split_first() else { return false };
    point_in_polygon(outer, p) && !holes.iter().any(|h| point_in_polygon(h, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f64, f64)]) -> Vec<Vec2> {
        v.iter().map(|&(x, y)| Vec2::new(x, y)).collect()
    }

    /// The nodes after the ring vertices, sorted by position.
    fn interior(sk: &Skeleton) -> Vec<SkeletonNode> {
        let ring_vertices: usize = sk.rings.iter().map(Vec::len).sum();
        let mut v: Vec<SkeletonNode> = sk.nodes.iter().skip(ring_vertices).copied().collect();
        v.sort_by(|a, b| a.pos.x.total_cmp(&b.pos.x).then(a.pos.y.total_cmp(&b.pos.y)));
        v
    }

    fn assert_valid(sk: &Skeleton) {
        let problems = check(sk, 1e-7);
        assert!(problems.is_empty(), "{problems:#?}\n{sk:#?}");
    }

    fn assert_node(node: &SkeletonNode, x: f64, y: f64, time: f64) {
        assert!(node.pos.near(Vec2::new(x, y), 1e-9) && (node.time - time).abs() < 1e-9, "expected ({x}, {y}) at {time}, got {node:?}");
    }

    fn area(region: &OffsetRegion) -> f64 {
        shoelace(&region.outer) + region.holes.iter().map(|h| shoelace(h)).sum::<f64>()
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Vec2> {
        pts(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }

    /// Two 4 x 4 rooms joined by a corridor 1 wide: the corridor closes at time 0.5 and the inset splits in two.
    fn dumbbell() -> Vec<Vec2> {
        pts(&[
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 1.5),
            (6.0, 1.5),
            (6.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (6.0, 4.0),
            (6.0, 2.5),
            (4.0, 2.5),
            (4.0, 4.0),
            (0.0, 4.0),
        ])
    }

    #[test]
    fn square_has_one_node_at_the_centre() {
        let sk = straight_skeleton(&square(0.0, 0.0, 1.0, 1.0), &[]).unwrap();
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 1);
        assert_node(&inner[0], 0.5, 0.5, 0.5);
        assert_eq!(sk.arcs.len(), 4);
        assert!(sk.faces.iter().all(|f| f.nodes.len() == 3));
        assert!((sk.max_time() - 0.5).abs() < 1e-12);
        assert!((sk.height(4, 2.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn rectangle_has_a_ridge_between_two_nodes() {
        let sk = straight_skeleton(&square(0.0, 0.0, 4.0, 2.0), &[]).unwrap();
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 2);
        assert_node(&inner[0], 1.0, 1.0, 1.0);
        assert_node(&inner[1], 3.0, 1.0, 1.0);
        assert_eq!(sk.arcs.len(), 5);
        // The long edges' faces are trapezoids, the short edges' faces triangles.
        let sizes: Vec<usize> = sk.faces.iter().map(|f| f.nodes.len()).collect();
        assert_eq!(sizes, vec![4, 3, 4, 3]);
        assert_eq!(&sk.faces[0].nodes[..2], &[0, 1]);
    }

    #[test]
    fn l_shape_meets_at_three_nodes() {
        let outer = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let sk = straight_skeleton(&outer, &[]).unwrap();
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 3);
        assert_node(&inner[0], 1.0, 1.0, 1.0);
        assert_node(&inner[1], 1.0, 3.0, 1.0);
        assert_node(&inner[2], 3.0, 1.0, 1.0);
        assert_eq!(sk.arcs.len(), 8);
    }

    #[test]
    fn square_with_a_centred_hole_is_a_picture_frame() {
        let hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        let sk = straight_skeleton(&square(0.0, 0.0, 10.0, 10.0), &hole).unwrap();
        assert_valid(&sk);
        assert!(shoelace(&sk.rings[0]) > 0.0 && shoelace(&sk.rings[1]) < 0.0);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 4);
        for (node, (x, y)) in inner.iter().zip([(1.5, 1.5), (1.5, 8.5), (8.5, 1.5), (8.5, 8.5)]) {
            assert_node(node, x, y, 1.5);
        }
        assert_eq!(sk.arcs.len(), 12);
        assert_eq!(sk.faces.len(), 8);
        assert!(sk.faces.iter().all(|f| f.nodes.len() == 4));
        assert_eq!(sk.faces[4].ring, 1);
    }

    #[test]
    fn input_is_cleaned_and_oriented() {
        // Clockwise, closing point repeated, a duplicate and a vertex in the middle of an edge.
        let messy = pts(&[(0.0, 0.0), (0.0, 1.0), (0.5, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]);
        let sk = straight_skeleton(&messy, &[]).unwrap();
        assert_eq!(sk.rings[0].len(), 4);
        assert!(shoelace(&sk.rings[0]) > 0.0);
        assert_valid(&sk);
        assert_node(&interior(&sk)[0], 0.5, 0.5, 0.5);
    }

    #[test]
    fn regular_polygon_collapses_to_its_centre() {
        let n = 12;
        let ring: Vec<Vec2> = (0..n).map(|i| Vec2::polar(Vec2::new(3.0, -2.0), 5.0, crate::TAU * i as f64 / n as f64)).collect();
        let sk = straight_skeleton(&ring, &[]).unwrap();
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 1);
        assert_node(&inner[0], 3.0, -2.0, 5.0 * (crate::PI / n as f64).cos());
    }

    #[test]
    fn shapes_far_from_the_origin_keep_their_skeleton() {
        let sk = straight_skeleton(&square(1.0e6, -2.0e6, 1.0e6 + 4.0, -2.0e6 + 2.0), &[]).unwrap();
        assert!(check(&sk, 1e-6).is_empty());
        let inner = interior(&sk);
        assert_eq!(inner.len(), 2);
        assert!((inner[0].pos.x - (1.0e6 + 1.0)).abs() < 1e-6 && (inner[0].time - 1.0).abs() < 1e-6);
    }

    #[test]
    fn inset_square() {
        let regions = inset(&square(0.0, 0.0, 4.0, 4.0), &[], 1.0).unwrap();
        assert_eq!(regions.len(), 1);
        assert!((area(&regions[0]) - 4.0).abs() < 1e-9);
        assert!(inset(&square(0.0, 0.0, 4.0, 4.0), &[], 2.0).unwrap().is_empty());
        assert!(inset(&square(0.0, 0.0, 4.0, 4.0), &[], 5.0).unwrap().is_empty());
        let same = inset(&square(0.0, 0.0, 4.0, 4.0), &[], 0.0).unwrap();
        assert_eq!(same, vec![OffsetRegion { outer: square(0.0, 0.0, 4.0, 4.0), holes: vec![] }]);
    }

    #[test]
    fn inset_splits_into_two_pieces() {
        let outer = dumbbell();
        let before = inset(&outer, &[], 0.4).unwrap();
        assert_eq!(before.len(), 1);
        let after = inset(&outer, &[], 0.75).unwrap();
        assert_eq!(after.len(), 2);
        for region in &after {
            assert!((area(region) - 6.25).abs() < 1e-9, "{region:?}");
            assert_eq!(region.outer.len(), 4, "straight-through vertices are dropped: {region:?}");
            assert!(shoelace(&region.outer) > 0.0);
        }
        assert!(inset(&outer, &[], 2.0).unwrap().is_empty());
        let sk = straight_skeleton(&outer, &[]).unwrap();
        assert_valid(&sk);
        assert!((sk.max_time() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn inset_grows_the_hole() {
        let hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        let regions = inset(&square(0.0, 0.0, 10.0, 10.0), &hole, 1.0).unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].holes.len(), 1);
        assert!((area(&regions[0]) - (64.0 - 36.0)).abs() < 1e-9);
        assert!(shoelace(&regions[0].holes[0]) < 0.0);
        assert!(inset(&square(0.0, 0.0, 10.0, 10.0), &hole, 1.5).unwrap().is_empty());
    }

    #[test]
    fn outset_square_is_mitred() {
        let out = outset(&square(0.0, 0.0, 2.0, 2.0), &[], 1.0).unwrap();
        assert_eq!(out.outer.len(), 4);
        assert!((shoelace(&out.outer) - 16.0).abs() < 1e-9);
        assert!(out.holes.is_empty());
    }

    #[test]
    fn outset_shrinks_and_drops_holes() {
        let hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        let outer = square(0.0, 0.0, 10.0, 10.0);
        let one = outset(&outer, &hole, 1.0).unwrap();
        assert!((shoelace(&one.outer) - 144.0).abs() < 1e-9);
        assert_eq!(one.holes.len(), 1);
        assert!((shoelace(&one.holes[0]) + 4.0).abs() < 1e-9);
        let gone = outset(&outer, &hole, 2.5).unwrap();
        assert!(gone.holes.is_empty());
        assert!((shoelace(&gone.outer) - 225.0).abs() < 1e-9);
    }

    #[test]
    fn outset_closing_a_narrow_mouth_leaves_a_void() {
        // A keyhole: a 4 x 6 cavity with a mouth 2 wide. At d = 1.2 the mouth has closed but the cavity has not.
        let keyhole = pts(&[
            (0.0, 0.0),
            (8.0, 0.0),
            (8.0, 8.0),
            (0.0, 8.0),
            (0.0, 5.0),
            (2.0, 5.0),
            (2.0, 7.0),
            (6.0, 7.0),
            (6.0, 1.0),
            (2.0, 1.0),
            (2.0, 3.0),
            (0.0, 3.0),
        ]);
        let out = outset(&keyhole, &[], 1.2).unwrap();
        assert_eq!(out.holes.len(), 1, "{out:?}");
        assert!((shoelace(&out.holes[0]) + 1.6 * 3.6).abs() < 1e-9, "{out:?}");
        assert!(shoelace(&out.outer) > 0.0);
        let open = outset(&keyhole, &[], 0.5).unwrap();
        assert!(open.holes.is_empty());
    }

    #[test]
    fn errors() {
        let sq = square(0.0, 0.0, 1.0, 1.0);
        assert_eq!(straight_skeleton(&pts(&[(0.0, 0.0), (f64::NAN, 1.0), (1.0, 0.0)]), &[]), Err(SkeletonError::NotFinite));
        assert_eq!(straight_skeleton(&pts(&[(0.0, 0.0), (f64::INFINITY, 1.0), (1.0, 0.0)]), &[]), Err(SkeletonError::NotFinite));
        assert_eq!(straight_skeleton(&pts(&[(0.0, 0.0), (1.0, 0.0)]), &[]), Err(SkeletonError::TooFewVertices));
        assert_eq!(straight_skeleton(&[], &[]), Err(SkeletonError::TooFewVertices));
        assert_eq!(straight_skeleton(&pts(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)]), &[]), Err(SkeletonError::TooFewVertices));
        assert_eq!(straight_skeleton(&pts(&[(0.0, 0.0), (0.0, 0.0), (0.0, 0.0)]), &[]), Err(SkeletonError::TooFewVertices));
        let bowtie = pts(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)]);
        assert_eq!(straight_skeleton(&bowtie, &[]), Err(SkeletonError::NotSimple));
        let spike = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (2.0, 4.0), (2.0, 6.0), (2.0, 4.0), (0.0, 4.0)]);
        assert_eq!(straight_skeleton(&spike, &[]), Err(SkeletonError::NotSimple));
        let outside = vec![square(2.0, 2.0, 3.0, 3.0)];
        assert_eq!(straight_skeleton(&sq, &outside), Err(SkeletonError::HoleOutside));
        let crossing = vec![square(0.5, 0.5, 1.5, 1.5)];
        assert_eq!(straight_skeleton(&sq, &crossing), Err(SkeletonError::NotSimple));
        let touching = vec![square(0.0, 0.2, 0.5, 0.5)];
        assert_eq!(straight_skeleton(&sq, &touching), Err(SkeletonError::NotSimple));
        let big = square(0.0, 0.0, 10.0, 10.0);
        let nested = vec![square(2.0, 2.0, 8.0, 8.0), square(4.0, 4.0, 5.0, 5.0)];
        assert_eq!(straight_skeleton(&big, &nested), Err(SkeletonError::HoleOutside));
        for d in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(inset(&sq, &[], d), Err(SkeletonError::NotFinite));
            assert_eq!(outset(&sq, &[], d), Err(SkeletonError::NotFinite));
        }
        assert_eq!(inset(&pts(&[(0.0, 0.0), (1.0, 0.0)]), &[], 0.1), Err(SkeletonError::TooFewVertices));
        assert_eq!(outset(&bowtie, &[], 0.1), Err(SkeletonError::NotSimple));
        assert!(!SkeletonError::EventLimit.to_string().is_empty());
    }

    #[test]
    fn check_reports_broken_skeletons() {
        let mut sk = straight_skeleton(&square(0.0, 0.0, 4.0, 2.0), &[]).unwrap();
        assert!(check(&sk, 1e-9).is_empty());
        sk.arcs.push((0, 99));
        sk.nodes[4].time = 1.5;
        let problems = check(&sk, 1e-9);
        assert!(problems.iter().any(|p| p.contains("arc 5")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("has time 1.5")), "{problems:?}");
    }

    /// Star-shaped polygons (always simple) from a deterministic generator: invariants, insets and outsets hold.
    #[test]
    fn random_star_polygons() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..300 {
            let n = 3 + (next() * 14.0) as usize;
            let ring: Vec<Vec2> = (0..n)
                .map(|i| {
                    let angle = crate::TAU * (i as f64 + 0.8 * next()) / n as f64;
                    Vec2::polar(Vec2::ZERO, 2.0 + 8.0 * next(), angle)
                })
                .collect();
            let sk = match straight_skeleton(&ring, &[]) {
                Ok(sk) => sk,
                Err(e) => panic!("case {case}: {e}\n{ring:?}"),
            };
            let problems = check(&sk, 1e-6);
            assert!(problems.is_empty(), "case {case}: {problems:#?}\n{ring:?}");
            let max = sk.max_time();
            let mut last = shoelace(&sk.rings[0]);
            for frac in [0.1, 0.3, 0.6, 0.9] {
                let regions = inset(&ring, &[], max * frac).unwrap();
                let total: f64 = regions.iter().map(area).sum();
                assert!(total <= last + 1e-6 && total > 0.0, "case {case}: inset area {total} after {last}\n{ring:?}");
                last = total;
            }
            assert!(inset(&ring, &[], max * 1.0001).unwrap().is_empty(), "case {case}\n{ring:?}");
            let grown = outset(&ring, &[], 0.7).unwrap();
            assert!(shoelace(&grown.outer) > shoelace(&sk.rings[0]), "case {case}\n{ring:?}");
        }
    }

    /// A tiny deterministic generator for the stress tests.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn below(&mut self, n: usize) -> usize {
            ((self.next() * n as f64) as usize).min(n - 1)
        }
    }

    fn rotated(ring: &[Vec2], angle: f64) -> Vec<Vec2> {
        ring.iter().map(|p| p.rotate(angle) + Vec2::new(7.0, -3.0)).collect()
    }

    /// Check the skeleton of `outer` and `holes`, and that insets shrink and eventually vanish.
    fn exercise(label: &str, outer: &[Vec2], holes: &[Vec<Vec2>]) {
        let sk = match straight_skeleton(outer, holes) {
            Ok(sk) => sk,
            Err(e) => panic!("{label}: {e}\nouter {outer:?}\nholes {holes:?}"),
        };
        let problems = check(&sk, 1e-6);
        assert!(problems.is_empty(), "{label}: {problems:#?}\nouter {outer:?}\nholes {holes:?}");
        let region_area: f64 = sk.rings.iter().map(|r| shoelace(r)).sum();
        let mut last = region_area;
        for frac in [0.2, 0.5, 0.8] {
            let regions = inset(outer, holes, sk.max_time() * frac).unwrap();
            let total: f64 = regions.iter().map(area).sum();
            assert!(total <= last + 1e-6 && total > 0.0, "{label}: inset area {total} after {last}\nouter {outer:?}\nholes {holes:?}");
            last = total;
        }
        assert!(inset(outer, holes, sk.max_time() * 1.0001).unwrap().is_empty(), "{label}: not empty after max_time");
        roof_height_matches_inset(label, &sk, outer, holes);
    }

    /// The roof height at a point (its distance to the line of the face it lies in) decides whether the point is in the
    /// inset: `inset(d)` contains `p` exactly when `height(p) > d`.
    fn roof_height_matches_inset(label: &str, sk: &Skeleton, outer: &[Vec2], holes: &[Vec<Vec2>]) {
        let (lo, hi) = extent(sk.rings[0].iter().copied()).unwrap();
        let mut rng = Lcg(99);
        let face_polygons: Vec<Vec<Vec2>> = sk.faces.iter().map(|f| f.nodes.iter().map(|&i| sk.nodes[i].pos).collect()).collect();
        let max = sk.max_time();
        let levels: Vec<(f64, Vec<OffsetRegion>)> = [0.15, 0.4, 0.65, 0.9].iter().map(|f| (max * f, inset(outer, holes, max * f).unwrap())).collect();
        let mut checked = 0;
        for _ in 0..200 {
            let p = Vec2::new(lo.x + (hi.x - lo.x) * rng.next(), lo.y + (hi.y - lo.y) * rng.next());
            let Some((fi, _)) = face_polygons.iter().enumerate().find(|(_, poly)| point_in_polygon(poly, p)) else { continue };
            let face = &sk.faces[fi];
            let ring = &sk.rings[face.ring];
            let (a, b) = (ring[face.edge], ring[(face.edge + 1) % ring.len()]);
            let height = unit(b - a).cross(p - a);
            for (d, regions) in &levels {
                if (height - d).abs() < 1e-5 {
                    continue;
                }
                let inside = regions.iter().any(|r| point_in_polygon(&r.outer, p) && !r.holes.iter().any(|h| point_in_polygon(h, p)));
                assert_eq!(inside, height > *d, "{label}: point {p:?} has height {height} but inset({d}) says {inside}");
                checked += 1;
            }
        }
        assert!(checked > 10, "{label}: only {checked} samples");
    }

    /// Histogram polygons: a base with columns of integer heights. Equal widths and heights make events coincide.
    #[test]
    fn histogram_polygons_with_many_simultaneous_events() {
        let mut rng = Lcg(7);
        for case in 0..200 {
            let columns = 2 + rng.below(8);
            let heights: Vec<f64> = (0..columns).map(|_| 1.0 + rng.below(4) as f64).collect();
            let mut ring = vec![Vec2::new(0.0, 0.0), Vec2::new(columns as f64, 0.0)];
            for (i, h) in heights.iter().enumerate().rev() {
                ring.push(Vec2::new(i as f64 + 1.0, *h));
                ring.push(Vec2::new(i as f64, *h));
            }
            exercise(&format!("histogram {case} {heights:?}"), &ring, &[]);
            exercise(&format!("rotated histogram {case} {heights:?}"), &rotated(&ring, 0.3 + case as f64 * 0.01), &[]);
        }
    }

    /// A big room with several rectangular holes on a grid (some touching the same lines, some symmetric).
    #[test]
    fn rooms_with_rectangular_holes() {
        let mut rng = Lcg(11);
        for case in 0..100 {
            let (cols, rows) = (2 + rng.below(3), 2 + rng.below(3));
            let cell = 6.0;
            let outer = square(0.0, 0.0, cols as f64 * cell, rows as f64 * cell);
            let mut holes = Vec::new();
            for cx in 0..cols {
                for cy in 0..rows {
                    if rng.next() < 0.5 {
                        let (w, h) = (1.0 + rng.below(3) as f64, 1.0 + rng.below(3) as f64);
                        let (x, y) = (cx as f64 * cell + 1.5 + rng.below(2) as f64, cy as f64 * cell + 1.5 + rng.below(2) as f64);
                        holes.push(square(x, y, x + w, y + h));
                    }
                }
            }
            exercise(&format!("room {case} with {} holes", holes.len()), &outer, &holes);
            let turned: Vec<Vec<Vec2>> = holes.iter().map(|h| rotated(h, 0.7)).collect();
            exercise(&format!("rotated room {case}"), &rotated(&outer, 0.7), &turned);
        }
    }

    /// Combs, crosses and stars with integer coordinates: parallel walls closing at once all along their length.
    #[test]
    fn combs_crosses_and_stars() {
        let comb = pts(&[
            (0.0, 0.0),
            (9.0, 0.0),
            (9.0, 5.0),
            (8.0, 5.0),
            (8.0, 2.0),
            (7.0, 2.0),
            (7.0, 5.0),
            (6.0, 5.0),
            (6.0, 2.0),
            (5.0, 2.0),
            (5.0, 5.0),
            (4.0, 5.0),
            (4.0, 2.0),
            (3.0, 2.0),
            (3.0, 5.0),
            (0.0, 5.0),
        ]);
        exercise("comb", &comb, &[]);
        let cross = pts(&[
            (2.0, 0.0),
            (4.0, 0.0),
            (4.0, 2.0),
            (6.0, 2.0),
            (6.0, 4.0),
            (4.0, 4.0),
            (4.0, 6.0),
            (2.0, 6.0),
            (2.0, 4.0),
            (0.0, 4.0),
            (0.0, 2.0),
            (2.0, 2.0),
        ]);
        exercise("cross", &cross, &[]);
        exercise("rotated cross", &rotated(&cross, 1.1), &[]);
        let t = pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 2.0), (6.0, 2.0), (6.0, 6.0), (4.0, 6.0), (4.0, 2.0), (0.0, 2.0)]);
        exercise("T", &t, &[]);
        let h = pts(&[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 4.0),
            (6.0, 4.0),
            (6.0, 0.0),
            (8.0, 0.0),
            (8.0, 10.0),
            (6.0, 10.0),
            (6.0, 6.0),
            (2.0, 6.0),
            (2.0, 10.0),
            (0.0, 10.0),
        ]);
        exercise("H", &h, &[]);
        exercise("rotated H", &rotated(&h, 2.0), &[]);
        let u = pts(&[(0.0, 0.0), (6.0, 0.0), (6.0, 6.0), (4.0, 6.0), (4.0, 2.0), (2.0, 2.0), (2.0, 6.0), (0.0, 6.0)]);
        exercise("U", &u, &[]);
        let star: Vec<Vec2> = (0..16).map(|i| Vec2::polar(Vec2::ZERO, if i % 2 == 0 { 10.0 } else { 4.0 }, crate::TAU * i as f64 / 16.0)).collect();
        exercise("regular star", &star, &[]);
        exercise("star with a square hole", &star, &[square(-1.0, -1.0, 1.0, 1.0)]);
        let skinny = pts(&[(0.0, 0.0), (100.0, 0.0), (100.0, 0.5), (0.0, 0.5)]);
        exercise("skinny", &skinny, &[]);
        let sliver = pts(&[(0.0, 0.0), (50.0, 0.0), (0.0, 0.01)]);
        exercise("sliver triangle", &sliver, &[]);
    }

    /// Larger random shapes (star polygons with up to 150 vertices, with and without a hole) stay valid and fast.
    #[test]
    fn large_random_polygons() {
        let mut rng = Lcg(5);
        for case in 0..12 {
            let n = 40 + rng.below(110);
            let ring: Vec<Vec2> = (0..n)
                .map(|i| {
                    let angle = crate::TAU * (i as f64 + 0.9 * rng.next()) / n as f64;
                    Vec2::polar(Vec2::ZERO, 20.0 + 80.0 * rng.next(), angle)
                })
                .collect();
            exercise(&format!("large star {case} ({n} vertices)"), &ring, &[]);
            let hole = vec![square(-6.0, -4.0, 5.0, 7.0)];
            exercise(&format!("large star {case} with a hole"), &ring, &hole);
            let grown = outset(&ring, &hole, 3.0).unwrap();
            assert!(shoelace(&grown.outer) > shoelace(&ring));
        }
    }

    /// Outsets of random rotated shapes: the outline grows, stays counter-clockwise, and holes keep clockwise.
    #[test]
    fn outset_of_degenerate_shapes() {
        let mut rng = Lcg(21);
        for case in 0..100 {
            let columns = 2 + rng.below(6);
            let mut ring = vec![Vec2::new(0.0, 0.0), Vec2::new(columns as f64, 0.0)];
            for i in (0..columns).rev() {
                let h = 1.0 + rng.below(4) as f64;
                ring.push(Vec2::new(i as f64 + 1.0, h));
                ring.push(Vec2::new(i as f64, h));
            }
            let ring = if case % 2 == 0 { rotated(&ring, 0.37) } else { ring };
            let base = shoelace(&clean_ring(&ring, 1e-9, Spikes::Reject).unwrap());
            for d in [0.1, 0.5, 1.0, 3.0] {
                let out = outset(&ring, &[], d).unwrap();
                let a = shoelace(&out.outer);
                assert!(a > base.abs() && out.holes.iter().all(|h| shoelace(h) < 0.0), "case {case} d {d}: {a} vs {base}\n{ring:?}");
            }
        }
    }

    /// Degenerate shapes with their vertices nudged: events that were simultaneous are now nearly so (below, around and
    /// above the tolerance), which is where tolerance handling goes wrong.
    #[test]
    fn nearly_degenerate_shapes() {
        let mut rng = Lcg(31);
        for case in 0..900 {
            let columns = 2 + rng.below(6);
            let mut ring = vec![Vec2::new(0.0, 0.0), Vec2::new(columns as f64, 0.0)];
            for i in (0..columns).rev() {
                let h = 1.0 + rng.below(3) as f64;
                ring.push(Vec2::new(i as f64 + 1.0, h));
                ring.push(Vec2::new(i as f64, h));
            }
            let noise = [1e-4, 1e-6, 1e-8, 1e-9, 1e-10, 1e-12][case % 6];
            let nudged: Vec<Vec2> = ring.iter().map(|p| *p + Vec2::new((rng.next() - 0.5) * noise, (rng.next() - 0.5) * noise)).collect();
            // Nudging can fold the outline onto itself closer than the tolerance: that input is rightly refused.
            if let Err(SkeletonError::NotSimple | SkeletonError::TooFewVertices) = straight_skeleton(&nudged, &[]) {
                continue;
            }
            exercise(&format!("nudged histogram {case} (noise {noise})"), &nudged, &[]);
        }
    }

    /// The same shapes at very different sizes give the same skeleton structure.
    #[test]
    fn scale_does_not_change_the_structure() {
        let l = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let hole = [square(3.0, 3.0, 7.0, 7.0)];
        let room = square(0.0, 0.0, 10.0, 10.0);
        for scale in [1e-6, 1e-3, 1.0, 1e3, 1e6] {
            let big = |v: &[Vec2]| -> Vec<Vec2> { v.iter().map(|p| *p * scale + Vec2::new(5.0 * scale, -3.0 * scale)).collect() };
            let sk = straight_skeleton(&big(&l), &[]).unwrap();
            assert_eq!((sk.nodes.len(), sk.arcs.len()), (9, 8), "L at scale {scale}");
            assert!(check(&sk, 1e-6 * scale.max(1.0)).is_empty(), "L at scale {scale}");
            let holes: Vec<Vec<Vec2>> = hole.iter().map(|h| big(h)).collect();
            let sk = straight_skeleton(&big(&room), &holes).unwrap();
            assert_eq!((sk.nodes.len(), sk.arcs.len()), (12, 12), "frame at scale {scale}");
            assert!(check(&sk, 1e-6 * scale.max(1.0)).is_empty(), "frame at scale {scale}");
            assert!((sk.max_time() - 1.5 * scale).abs() < 1e-9 * scale);
        }
    }

    /// Very sharp corners (a speed of 1 / sin(half angle)) are fine down to tiny angles.
    #[test]
    fn sharp_corners() {
        for angle in [0.5_f64, 0.1, 1e-2, 1e-3, 1e-4, 1e-5] {
            let tri = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0 * angle.cos(), 10.0 * angle.sin())];
            exercise(&format!("triangle with a {angle} rad corner"), &tri, &[]);
            let kite = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, -10.0 * angle), Vec2::new(12.0, 0.0), Vec2::new(10.0, 10.0 * angle)];
            exercise(&format!("kite with a {angle} slope"), &kite, &[]);
        }
    }

    /// The boundary loops of a set of grid cells: counter-clockwise outlines and clockwise holes. `None` when two loops
    /// pinch together at a corner (that is not a simple polygon).
    fn cell_loops(cells: &std::collections::HashSet<(i32, i32)>) -> Option<Vec<Vec<Vec2>>> {
        let mut next: std::collections::HashMap<(i32, i32), (i32, i32)> = std::collections::HashMap::new();
        for &(x, y) in cells {
            let sides = [
                ((x, y), (x + 1, y), (x, y - 1)),
                ((x + 1, y), (x + 1, y + 1), (x + 1, y)),
                ((x + 1, y + 1), (x, y + 1), (x, y + 1)),
                ((x, y + 1), (x, y), (x - 1, y)),
            ];
            for (from, to, neighbour) in sides {
                if !cells.contains(&neighbour) && next.insert(from, to).is_some() {
                    return None;
                }
            }
        }
        let mut loops = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut starts: Vec<_> = next.keys().copied().collect();
        starts.sort();
        for start in starts {
            if seen.contains(&start) {
                continue;
            }
            let (mut here, mut ring) = (start, Vec::new());
            while seen.insert(here) {
                ring.push(Vec2::new(here.0 as f64, here.1 as f64));
                here = *next.get(&here)?;
            }
            loops.push(ring);
        }
        Some(loops)
    }

    /// Random connected grid regions with holes, as they come out of room layouts: every wall is axis-parallel and events
    /// coincide everywhere. Rotated copies keep the coincidences only to rounding.
    #[test]
    fn random_polyomino_regions() {
        let mut rng = Lcg(77);
        let mut tested = 0;
        for case in 0..400 {
            let size = 3 + rng.below(6) as i32;
            let mut cells = std::collections::HashSet::new();
            for x in 0..size {
                for y in 0..size {
                    if rng.next() < 0.7 {
                        cells.insert((x, y));
                    }
                }
            }
            // Keep the component of one cell.
            let Some(&seed) = cells.iter().min() else { continue };
            let mut component = std::collections::HashSet::from([seed]);
            let mut frontier = vec![seed];
            while let Some((x, y)) = frontier.pop() {
                for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                    if cells.contains(&n) && component.insert(n) {
                        frontier.push(n);
                    }
                }
            }
            let Some(loops) = cell_loops(&component) else { continue };
            let (mut outer, mut holes) = (Vec::new(), Vec::new());
            for ring in loops {
                if shoelace(&ring) > 0.0 { outer.push(ring) } else { holes.push(ring) }
            }
            let Ok([outer]) = <[Vec<Vec2>; 1]>::try_from(outer) else { continue };
            tested += 1;
            exercise(&format!("polyomino {case} ({} cells, {} holes)", component.len(), holes.len()), &outer, &holes);
            let angle = 0.1 + case as f64 * 0.37;
            let turned: Vec<Vec<Vec2>> = holes.iter().map(|h| rotated(h, angle)).collect();
            exercise(&format!("rotated polyomino {case}"), &rotated(&outer, angle), &turned);
            let grown = outset(&outer, &holes, 0.6).unwrap();
            assert!(shoelace(&grown.outer) > shoelace(&outer), "polyomino {case}");
        }
        assert!(tested > 100, "only {tested} usable polyominoes");
    }

    /// Whatever the input, the functions return (a result or an error) without panicking or looping.
    #[test]
    fn hostile_input_never_panics() {
        let mut rng = Lcg(1234);
        let specials = [
            0.0,
            1.0,
            -1.0,
            0.5,
            1e-300,
            -1e-300,
            1e300,
            -1e300,
            1e15,
            1e-9,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            f64::MIN_POSITIVE,
        ];
        let coordinate = |rng: &mut Lcg| {
            if rng.next() < 0.5 { specials[rng.below(specials.len())] } else { (rng.next() - 0.5) * [1.0, 1e3, 1e-6, 1e9][rng.below(4)] }
        };
        let (mut ok, mut refused) = (0, 0);
        for _ in 0..3000 {
            let ring = |rng: &mut Lcg| -> Vec<Vec2> { (0..rng.below(9)).map(|_| Vec2::new(coordinate(rng), coordinate(rng))).collect() };
            let outer = ring(&mut rng);
            let holes: Vec<Vec<Vec2>> = (0..rng.below(3)).map(|_| ring(&mut rng)).collect();
            let d = coordinate(&mut rng);
            match straight_skeleton(&outer, &holes) {
                Ok(sk) => {
                    ok += 1;
                    let _ = check(&sk, 1e-6);
                }
                Err(_) => refused += 1,
            }
            let _ = inset(&outer, &holes, d);
            let _ = outset(&outer, &holes, d);
        }
        // Tiny and huge but valid shapes, and big offset distances.
        for scale in [1e-12, 1e-6, 1e6, 1e12, 1e100] {
            let ring: Vec<Vec2> = square(0.0, 0.0, 3.0, 2.0).iter().map(|p| *p * scale).collect();
            for d in [0.0, 0.3 * scale, scale, 100.0 * scale, 1e300, f64::MAX] {
                let _ = inset(&ring, &[], d);
                let _ = outset(&ring, &[], d);
            }
            let _ = straight_skeleton(&ring, &[]);
        }
        assert!(ok > 0 && refused > 0, "{ok} accepted, {refused} refused");
    }

    /// A few hundred vertices is routine; this takes a fraction of a second.
    #[test]
    fn a_few_hundred_vertices() {
        let mut rng = Lcg(5);
        for n in [200usize, 400] {
            let ring: Vec<Vec2> =
                (0..n).map(|i| Vec2::polar(Vec2::ZERO, 50.0 + 50.0 * rng.next(), crate::TAU * (i as f64 + 0.9 * rng.next()) / n as f64)).collect();
            let sk = straight_skeleton(&ring, &[]).unwrap();
            let problems = check(&sk, 1e-6);
            assert!(problems.is_empty(), "n={n}: {problems:#?}");
        }
    }

    /// Run `exercise` on a shape, returning the panic message when it fails; shapes the input check refuses for folding
    /// onto themselves (nudging can do that) are skipped.
    fn try_exercise(label: &str, outer: &[Vec2], holes: &[Vec<Vec2>]) -> Option<String> {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        if let Err(SkeletonError::NotSimple | SkeletonError::TooFewVertices | SkeletonError::HoleOutside) = straight_skeleton(outer, holes) {
            return None;
        }
        let failure = catch_unwind(AssertUnwindSafe(|| exercise(label, outer, holes))).err()?;
        let message = failure.downcast_ref::<String>().cloned().or_else(|| failure.downcast_ref::<&str>().map(|s| s.to_string()));
        Some(message.unwrap_or_else(|| label.to_string()).chars().take(1500).collect())
    }

    /// Randomized shapes whose events nearly coincide: rotated and nudged histograms, polyominoes (with holes), stars
    /// snapped to a grid, slivers. The seeds are fixed so a failure repeats; it reports the first shapes that failed.
    #[test]
    fn randomized_campaign() {
        let mut failures: Vec<String> = Vec::new();
        let mut shapes = 0;
        for seed in [11u64, 22, 33] {
            let mut rng = Lcg(seed);
            for case in 0..300 {
                // Polyominoes: a random connected set of grid cells, its outline and holes.
                let size = 3 + rng.below(7) as i32;
                let mut cells = std::collections::HashSet::new();
                for x in 0..size {
                    for y in 0..size {
                        if rng.next() < 0.45 + 0.4 * rng.next() {
                            cells.insert((x, y));
                        }
                    }
                }
                if let Some(&first) = cells.iter().min() {
                    let mut component = std::collections::HashSet::from([first]);
                    let mut frontier = vec![first];
                    while let Some((x, y)) = frontier.pop() {
                        for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                            if cells.contains(&n) && component.insert(n) {
                                frontier.push(n);
                            }
                        }
                    }
                    let (mut outlines, mut holes) = (Vec::new(), Vec::new());
                    for ring in cell_loops(&component).unwrap_or_default() {
                        if shoelace(&ring) > 0.0 { outlines.push(ring) } else { holes.push(ring) }
                    }
                    if let Ok([outer]) = <[Vec<Vec2>; 1]>::try_from(outlines) {
                        let angle = if case % 3 == 0 { 0.0 } else { rng.next() * crate::TAU };
                        let noise = [0.0, 0.0, 1e-3, 1e-5, 1e-7, 1e-9, 1e-11][case % 7];
                        let mut nudge = |p: &Vec2| *p + Vec2::new((rng.next() - 0.5) * noise, (rng.next() - 0.5) * noise);
                        let o: Vec<Vec2> = rotated(&outer, angle).iter().map(&mut nudge).collect();
                        let h: Vec<Vec<Vec2>> = holes.iter().map(|h| rotated(h, angle).iter().map(&mut nudge).collect()).collect();
                        shapes += 1;
                        failures.extend(try_exercise(&format!("polyomino {case} (seed {seed}, angle {angle}, noise {noise})"), &o, &h));
                    }
                }
                // Stars with coordinates snapped to a grid (collinear and coincident events), with and without a hole.
                let n = 3 + rng.below(25);
                let snap = [0.0, 1.0, 0.5, 0.25][case % 4];
                let star: Vec<Vec2> = (0..n)
                    .map(|i| {
                        let p = Vec2::polar(Vec2::ZERO, 2.0 + 8.0 * rng.next(), crate::TAU * (i as f64 + 0.9 * rng.next()) / n as f64);
                        if snap > 0.0 { Vec2::new((p.x / snap).round() * snap, (p.y / snap).round() * snap) } else { p }
                    })
                    .collect();
                let star = tidy(star);
                let holes = if case % 5 == 0 { vec![square(-1.0, -1.0, 1.0, 1.0)] } else { vec![] };
                shapes += 1;
                failures.extend(try_exercise(&format!("star {case} (seed {seed}, snap {snap})"), &star, &holes));
                // Triangles and quadrilaterals, slivers included.
                let k = 3 + rng.below(2);
                let poly: Vec<Vec2> = (0..k)
                    .map(|i| Vec2::polar(Vec2::ZERO, 1.0 + 9.0 * rng.next().powi(3), crate::TAU * (i as f64 + 0.5 * rng.next()) / k as f64))
                    .collect();
                shapes += 1;
                failures.extend(try_exercise(&format!("triangle/quad {case} (seed {seed})"), &poly, &[]));
            }
            // Histograms with every vertex nudged by 1e-3 .. 1e-9: events that were simultaneous are now only nearly so.
            for case in 0..600 {
                let columns = 2 + rng.below(5);
                let mut ring = vec![Vec2::new(0.0, 0.0), Vec2::new(columns as f64, 0.0)];
                for i in (0..columns).rev() {
                    let h = 1.0 + rng.below(3) as f64;
                    ring.push(Vec2::new(i as f64 + 1.0, h));
                    ring.push(Vec2::new(i as f64, h));
                }
                let noise = [1e-3, 1e-4, 1e-5, 1e-6, 1e-8, 1e-9][case % 6];
                let angle = if case % 2 == 0 { 0.0 } else { rng.next() * crate::TAU };
                let nudged: Vec<Vec2> =
                    rotated(&ring, angle).iter().map(|p| *p + Vec2::new((rng.next() - 0.5) * noise, (rng.next() - 0.5) * noise)).collect();
                shapes += 1;
                failures.extend(try_exercise(&format!("histogram {case} (seed {seed}, angle {angle}, noise {noise})"), &nudged, &[]));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {shapes} shapes failed; the first:\n{}",
            failures.len(),
            failures.iter().take(2).cloned().collect::<Vec<_>>().join("\n\n")
        );
    }
    // -----------------------------------------------------------------------------------------------------------------
    // Weighted skeleton
    // -----------------------------------------------------------------------------------------------------------------

    fn weighted(outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>]) -> Skeleton {
        match weighted_skeleton(outer, holes, weights) {
            Ok(sk) => sk,
            Err(e) => panic!("{e}\nouter {outer:?}\nholes {holes:?}\nweights {weights:?}"),
        }
    }

    /// The plan area of each face.
    fn face_areas(sk: &Skeleton) -> Vec<f64> {
        sk.faces.iter().map(|f| shoelace(&f.nodes.iter().map(|&i| sk.nodes[i].pos).collect::<Vec<_>>())).collect()
    }

    #[test]
    fn unit_weights_give_the_plain_skeleton() {
        let l = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let frame_hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        let clockwise_l: Vec<Vec2> = l.iter().rev().copied().collect();
        let cases: Vec<(Vec<Vec2>, Vec<Vec<Vec2>>)> = vec![
            (square(0.0, 0.0, 4.0, 2.0), vec![]),
            (l, vec![]),
            (clockwise_l, vec![]),
            (dumbbell(), vec![]),
            (square(0.0, 0.0, 10.0, 10.0), frame_hole),
        ];
        for (outer, holes) in &cases {
            let plain = straight_skeleton(outer, holes).unwrap();
            let unit = weighted(outer, holes, &unit_weights(outer, holes));
            assert_eq!(plain, unit);
            assert!(plain.weights.iter().flatten().all(|&w| w == 1.0));
            assert_eq!(plain.weights.iter().map(Vec::len).collect::<Vec<_>>(), plain.rings.iter().map(Vec::len).collect::<Vec<_>>());
            assert_valid(&unit);
        }
    }

    #[test]
    fn gable_ends_put_the_ridge_on_the_short_edges() {
        // Edges: bottom, right, top, left. The short edges (right and left) never move, so the ridge runs the full length.
        let sk = weighted(&square(0.0, 0.0, 4.0, 2.0), &[], &[vec![1.0, 0.0, 1.0, 0.0]]);
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 2);
        assert_node(&inner[0], 0.0, 1.0, 1.0);
        assert_node(&inner[1], 4.0, 1.0, 1.0);
        assert_eq!(sk.weights, vec![vec![1.0, 0.0, 1.0, 0.0]]);
        // The gable faces have no area in plan and every node of theirs lies on their edge; the long faces are rectangles.
        let areas = face_areas(&sk);
        assert!(areas[1].abs() < 1e-9 && areas[3].abs() < 1e-9, "{areas:?}");
        assert!((areas[0] - 4.0).abs() < 1e-9 && (areas[2] - 4.0).abs() < 1e-9, "{areas:?}");
        for face in [&sk.faces[1], &sk.faces[3]] {
            let x = if face.edge == 1 { 4.0 } else { 0.0 };
            assert!(face.nodes.iter().all(|&n| (sk.nodes[n].pos.x - x).abs() < 1e-9), "{face:?}");
        }
    }

    #[test]
    fn slower_short_edges_pull_the_ridge_in() {
        let sk = weighted(&square(0.0, 0.0, 4.0, 2.0), &[], &[vec![1.0, 0.5, 1.0, 0.5]]);
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 2);
        assert_node(&inner[0], 0.5, 1.0, 1.0);
        assert_node(&inner[1], 3.5, 1.0, 1.0);
        // Different long edges: the top moves at half speed, so the ridge sits where bottom y = t meets top y = 2 - t/2.
        let uneven = weighted(&square(0.0, 0.0, 4.0, 2.0), &[], &[vec![1.0, 1.0, 0.5, 1.0]]);
        assert_valid(&uneven);
        let inner = interior(&uneven);
        assert_eq!(inner.len(), 2);
        assert_node(&inner[0], 4.0 / 3.0, 4.0 / 3.0, 4.0 / 3.0);
        assert_node(&inner[1], 8.0 / 3.0, 4.0 / 3.0, 4.0 / 3.0);
    }

    #[test]
    fn one_gable_on_a_square() {
        // Right edge pinned: the two corners on it slide up and down it and meet in the middle of it.
        let sk = weighted(&square(0.0, 0.0, 2.0, 2.0), &[], &[vec![1.0, 0.0, 1.0, 1.0]]);
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 2);
        assert_node(&inner[0], 1.0, 1.0, 1.0);
        assert_node(&inner[1], 2.0, 1.0, 1.0);
        assert_eq!(sk.arcs.len(), 5);
        let areas = face_areas(&sk);
        assert!(areas[1].abs() < 1e-9, "{areas:?}");
        // Bottom, top and left are trapezoid / triangle shaped: 1.5 + 1.5 + 1 of the square's 4.
        assert!((areas.iter().sum::<f64>() - 4.0).abs() < 1e-9, "{areas:?}");
    }

    #[test]
    fn uniform_weight_scales_every_time() {
        let l = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        for (outer, holes) in [(square(0.0, 0.0, 4.0, 2.0), vec![]), (l, vec![]), (square(0.0, 0.0, 10.0, 10.0), hole)] {
            let plain = straight_skeleton(&outer, &holes).unwrap();
            for scale in [2.0, 0.25, 7.0, 1e-6, 1e6] {
                let weights: Vec<Vec<f64>> = plain.rings.iter().map(|r| vec![scale; r.len()]).collect();
                let fast = weighted(&outer, &holes, &weights);
                assert_valid(&fast);
                assert_eq!(fast.arcs, plain.arcs);
                assert_eq!(fast.faces, plain.faces);
                assert_eq!(fast.nodes.len(), plain.nodes.len());
                for (a, b) in fast.nodes.iter().zip(&plain.nodes) {
                    assert!(a.pos.near(b.pos, 1e-9) && (a.time * scale - b.time).abs() < 1e-9, "{a:?} vs {b:?} at weight {scale}");
                }
                assert!((fast.max_time() * scale - plain.max_time()).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn roof_over_a_rectangle_with_gable_ends() {
        let rect = square(0.0, 0.0, 4.0, 2.0);
        let sk = roof_skeleton(&rect, &[], &[vec![1.0, f64::INFINITY, 1.0, f64::INFINITY]]).unwrap();
        assert_valid(&sk);
        assert_eq!(sk.weights, vec![vec![1.0, 0.0, 1.0, 0.0]]);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 2);
        assert_node(&inner[0], 0.0, 1.0, 1.0);
        assert_node(&inner[1], 4.0, 1.0, 1.0);
        // A shallower roof is lower: slope 1/2 gives the same ridge at half the height.
        let low = roof_skeleton(&rect, &[], &[vec![0.5, f64::INFINITY, 0.5, f64::INFINITY]]).unwrap();
        assert_valid(&low);
        let inner = interior(&low);
        assert_node(&inner[0], 0.0, 1.0, 0.5);
        assert_node(&inner[1], 4.0, 1.0, 0.5);
        assert!((low.height(4, 2.0) - 1.0).abs() < 1e-12);
        // Without gables it is a hip roof, whose ridge is as high as the rectangle is wide / 2 times the slope.
        let hip = roof_skeleton(&rect, &[], &[vec![0.5; 4]]).unwrap();
        assert_valid(&hip);
        let inner = interior(&hip);
        assert_node(&inner[0], 1.0, 1.0, 0.5);
        assert_node(&inner[1], 3.0, 1.0, 0.5);
    }

    /// A step in the outline whose lower side moves three times as fast: it catches up with the upper side when the edge
    /// between them (the riser) has shrunk to nothing, and from then on carries the whole wavefront.
    #[test]
    fn a_faster_edge_consumes_the_slower_parallel_edge_it_catches() {
        // Region [0, 7] x [0, 6] without the block [3, 7] x [0, 1]. Edges: 0 bottom of the left part (speed 3), 1 riser, 2 top of
        // the block-shaped notch (the slow parallel edge), 3 right, 4 top, 5 left.
        let outer = pts(&[(0.0, 0.0), (3.0, 0.0), (3.0, 1.0), (7.0, 1.0), (7.0, 6.0), (0.0, 6.0)]);
        let sk = weighted(&outer, &[], &[vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0]]);
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 4, "{inner:?}");
        assert_node(&inner[0], 1.5, 4.5, 1.5);
        assert_node(&inner[1], 2.5, 1.5, 0.5);
        assert_node(&inner[2], 5.5, 4.5, 1.5);
        assert_node(&inner[3], 6.5, 1.5, 0.5);
        // The ridge along the consumed edge's last position joins the faces of the two parallel edges.
        let y = node_index_at(&sk, 6.5, 1.5);
        let x = node_index_at(&sk, 2.5, 1.5);
        assert!(sk.arcs.iter().any(|&(a, b)| (a, b) == (x, y) || (a, b) == (y, x)), "{:?}", sk.arcs);
        // The mirror image (the faster edge on the right, the step going down) does the same.
        let mirrored = pts(&[(0.0, 1.0), (3.0, 1.0), (3.0, 0.0), (7.0, 0.0), (7.0, 6.0), (0.0, 6.0)]);
        let sk = weighted(&mirrored, &[], &[vec![1.0, 1.0, 3.0, 1.0, 1.0, 1.0]]);
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 4, "{inner:?}");
        assert_node(&inner[0], 0.5, 1.5, 0.5);
        assert_node(&inner[1], 1.5, 4.5, 1.5);
        assert_node(&inner[2], 3.5, 1.5, 0.5);
        assert_node(&inner[3], 5.5, 4.5, 1.5);
        // The same step at equal speeds never meets: the riser keeps its length.
        let flat = weighted(&outer, &[], &[vec![1.0; 6]]);
        assert_valid(&flat);
        assert!(flat.max_time() > 0.0);
    }

    fn node_index_at(sk: &Skeleton, x: f64, y: f64) -> usize {
        sk.nodes.iter().position(|n| n.pos.near(Vec2::new(x, y), 1e-9) && n.time > 0.0).unwrap()
    }

    #[test]
    fn weights_follow_their_edges_through_cleaning_and_reorientation() {
        // Clockwise, with a repeated point, a vertex in the middle of the bottom edge and a repeated closing point.
        // Edges: 0 left, 1 top, 2 empty, 3 right, 4 and 5 the bottom in two halves, 6 the empty closing edge.
        let messy = pts(&[(0.0, 0.0), (0.0, 2.0), (4.0, 2.0), (4.0, 2.0), (4.0, 0.0), (2.0, 0.0), (0.0, 0.0)]);
        let sk = weighted(&messy, &[], &[vec![0.0, 1.0, 9.0, 0.0, 1.0, 1.0, 9.0]]);
        assert_valid(&sk);
        assert_eq!(sk.rings, vec![pts(&[(4.0, 0.0), (4.0, 2.0), (0.0, 2.0), (0.0, 0.0)])]);
        // Right, top, left, bottom of the counter-clockwise ring.
        assert_eq!(sk.weights, vec![vec![0.0, 1.0, 0.0, 1.0]]);
        let inner = interior(&sk);
        assert_node(&inner[0], 0.0, 1.0, 1.0);
        assert_node(&inner[1], 4.0, 1.0, 1.0);
        // A repeated closing point may leave its empty edge's weight out.
        let sk = weighted(&messy, &[], &[vec![0.0, 1.0, 9.0, 0.0, 1.0, 1.0]]);
        assert_eq!(sk.weights, vec![vec![0.0, 1.0, 0.0, 1.0]]);
        // Reversing the ring reverses which edge is which: left 2, top 3, right 5, bottom 7 become right, top, left, bottom.
        let clockwise = pts(&[(0.0, 0.0), (0.0, 2.0), (4.0, 2.0), (4.0, 0.0)]);
        let sk = weighted(&clockwise, &[], &[vec![2.0, 3.0, 5.0, 7.0]]);
        assert_valid(&sk);
        assert_eq!(sk.weights, vec![vec![5.0, 3.0, 2.0, 7.0]]);
        // Holes are the second and later rings; a counter-clockwise hole is reversed too.
        let hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        let sk = weighted(&square(0.0, 0.0, 10.0, 10.0), &hole, &[vec![1.0; 4], vec![2.0, 3.0, 4.0, 5.0]]);
        assert_valid(&sk);
        assert_eq!(sk.weights, vec![vec![1.0; 4], vec![4.0, 3.0, 2.0, 5.0]]);
        // A vertex in the middle of an edge goes when both halves have the same weight, and -0.0 is 0.
        let straight = pts(&[(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)]);
        let sk = weighted(&straight, &[], &[vec![1.0, 1.0, -0.0, 1.0, 0.0]]);
        assert_valid(&sk);
        assert_eq!(sk.weights, vec![vec![1.0, 0.0, 1.0, 0.0]]);
        assert!(sk.weights[0][1].is_sign_positive());
    }

    #[test]
    fn bad_weights_are_refused() {
        let rect = square(0.0, 0.0, 4.0, 2.0);
        let bad = |w: Vec<Vec<f64>>| weighted_skeleton(&rect, &[], &w);
        assert!(bad(vec![vec![1.0; 4]]).is_ok());
        // One vector per ring, one weight per edge (the closing edge counts).
        assert_eq!(bad(vec![]), Err(SkeletonError::BadWeights));
        assert_eq!(bad(vec![vec![1.0; 4], vec![1.0; 4]]), Err(SkeletonError::BadWeights));
        for len in [0, 1, 3, 5, 8] {
            assert_eq!(bad(vec![vec![1.0; len]]), Err(SkeletonError::BadWeights), "{len} weights");
        }
        let hole = vec![square(1.0, 0.5, 2.0, 1.5)];
        assert_eq!(weighted_skeleton(&rect, &hole, &[vec![1.0; 4]]), Err(SkeletonError::BadWeights));
        assert_eq!(weighted_skeleton(&rect, &hole, &[vec![1.0; 4], vec![1.0; 3]]), Err(SkeletonError::BadWeights));
        assert!(weighted_skeleton(&rect, &hole, &[vec![1.0; 4], vec![1.0; 4]]).is_ok());
        // Finite and not negative.
        for w in [-1.0, -1e-300, f64::NEG_INFINITY, f64::NAN, f64::INFINITY] {
            assert_eq!(bad(vec![vec![1.0, w, 1.0, 1.0]]), Err(SkeletonError::BadWeights), "weight {w}");
        }
        // Two collinear edges with different weights; with the same weight the vertex between them is dropped.
        let straight = pts(&[(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)]);
        assert_eq!(weighted_skeleton(&straight, &[], &[vec![1.0, 2.0, 1.0, 1.0, 1.0]]), Err(SkeletonError::BadWeights));
        assert_eq!(weighted_skeleton(&straight, &[], &[vec![1.0, 0.0, 1.0, 1.0, 1.0]]), Err(SkeletonError::BadWeights));
        assert_eq!(weighted_skeleton(&straight, &[], &[vec![1.0, 1.0, 1.0, 1.0, 1.0]]).unwrap().rings[0].len(), 4);
        // Nothing moves when every edge is pinned.
        assert_eq!(bad(vec![vec![0.0; 4]]), Err(SkeletonError::BadWeights));
        // Slopes: positive or infinite.
        for s in [0.0, -1.0, f64::NAN, f64::NEG_INFINITY] {
            assert_eq!(roof_skeleton(&rect, &[], &[vec![1.0, s, 1.0, 1.0]]), Err(SkeletonError::BadWeights), "slope {s}");
        }
        assert_eq!(roof_skeleton(&rect, &[], &[vec![f64::INFINITY; 4]]), Err(SkeletonError::BadWeights));
        assert_eq!(roof_skeleton(&rect, &[], &[vec![1.0; 3]]), Err(SkeletonError::BadWeights));
        // Geometry is still judged by its own errors.
        let bowtie = pts(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)]);
        assert_eq!(weighted_skeleton(&bowtie, &[], &[vec![1.0; 4]]), Err(SkeletonError::NotSimple));
        assert_eq!(weighted_skeleton(&pts(&[(0.0, 0.0), (1.0, 0.0)]), &[], &[vec![1.0; 2]]), Err(SkeletonError::TooFewVertices));
        assert_eq!(weighted_skeleton(&[], &[], &[vec![]]), Err(SkeletonError::TooFewVertices));
        let nan = pts(&[(0.0, 0.0), (f64::NAN, 1.0), (1.0, 0.0)]);
        assert_eq!(weighted_skeleton(&nan, &[], &[vec![1.0; 3]]), Err(SkeletonError::NotFinite));
    }

    /// The wavefront at time `d` of `sk`'s rings and weights, as loops.
    fn wavefront_loops(outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>], d: f64) -> Vec<Vec<Vec2>> {
        let prep = prepare(outer, holes, weights).unwrap();
        let mut sim = Sim::new(&prep.rings, &prep.weights, prep.origin, prep.tol, 0);
        sim.run(Some(d)).unwrap();
        sim.loops_at(d).into_iter().map(|(pts, _)| pts).collect()
    }

    /// The height of a point (its distance to the line of the face it lies in, over that edge's weight) decides whether the
    /// point is inside the wavefront: it is at time `d` exactly when its height exceeds `d`.
    fn heights_match_wavefront(label: &str, outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>], sk: &Skeleton) {
        let (lo, hi) = extent(sk.rings[0].iter().copied()).unwrap();
        let mut rng = Lcg(99);
        let face_polygons: Vec<Vec<Vec2>> = sk.faces.iter().map(|f| f.nodes.iter().map(|&i| sk.nodes[i].pos).collect()).collect();
        let max = sk.max_time();
        let levels: Vec<(f64, Vec<Vec<Vec2>>)> =
            [0.137, 0.389, 0.641, 0.913].iter().map(|f| (max * f, wavefront_loops(outer, holes, weights, max * f))).collect();
        // The wavefront never crosses itself (a missed event would make it). Touching is allowed: pinned edges can leave two
        // parts of it resting against each other for good.
        let tol = prepare(outer, holes, weights).unwrap().tol * 10.0;
        for (d, loops) in &levels {
            let rings: Vec<Vec<Vec2>> = loops.iter().filter_map(|l| tidy_loop(l, tol)).collect();
            let edges: Vec<(usize, usize, Vec2, Vec2)> =
                rings.iter().enumerate().flat_map(|(r, ring)| (0..ring.len()).map(move |i| (r, i, ring[i], ring[(i + 1) % ring.len()]))).collect();
            // Which side of the line `a`-`b` the point lies on, if it is further than the tolerance from it.
            let side = |a: Vec2, b: Vec2, p: Vec2| -> i8 {
                let dist = (b - a).cross(p - a) / (b - a).len().max(f64::MIN_POSITIVE);
                if dist > tol {
                    1
                } else if dist < -tol {
                    -1
                } else {
                    0
                }
            };
            for (x, &(r1, i1, a, b)) in edges.iter().enumerate() {
                for &(r2, i2, c, e) in &edges[x + 1..] {
                    let neighbours = r1 == r2 && (i2 == i1 + 1 || (i1 == 0 && i2 + 1 == rings[r1].len()));
                    let crossing = side(a, b, c) * side(a, b, e) < 0 && side(c, e, a) * side(c, e, b) < 0;
                    assert!(neighbours || !crossing, "{label}: the wavefront at {d} crosses itself: {a:?}-{b:?} and {c:?}-{e:?}");
                }
            }
        }
        for _ in 0..200 {
            let p = Vec2::new(lo.x + (hi.x - lo.x) * rng.next(), lo.y + (hi.y - lo.y) * rng.next());
            let Some((fi, _)) = face_polygons.iter().enumerate().find(|(_, poly)| point_in_polygon(poly, p)) else { continue };
            let face = &sk.faces[fi];
            let ring = &sk.rings[face.ring];
            let (a, b) = (ring[face.edge], ring[(face.edge + 1) % ring.len()]);
            let w = sk.weights[face.ring][face.edge];
            if w <= 0.0 {
                continue;
            }
            let height = unit(b - a).cross(p - a) / w;
            for (d, loops) in &levels {
                if (height - d).abs() < 1e-5 * (1.0 + height) {
                    continue;
                }
                let inside = loops.iter().filter(|l| point_in_polygon(l, p)).count() % 2 == 1;
                assert_eq!(inside, height > *d, "{label}: point {p:?} (face {fi}) has height {height} but the wavefront at {d} says {inside}");
            }
        }
    }

    /// Check a weighted skeleton thoroughly; `None` when it passes.
    fn weighted_problem(label: &str, outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>]) -> Option<String> {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let trim = |m: String| -> String { m.chars().take(2500).collect() };
        let sk = match catch_unwind(AssertUnwindSafe(|| weighted_skeleton(outer, holes, weights))) {
            Err(_) => return Some(format!("{label}: panicked\nouter {outer:?}\nholes {holes:?}\nweights {weights:?}")),
            Ok(Err(SkeletonError::NotSimple | SkeletonError::TooFewVertices | SkeletonError::HoleOutside)) => return None,
            // Pinned edges can enclose a region nothing sweeps.
            Ok(Err(SkeletonError::BadWeights)) if weights.iter().flatten().any(|&w| w == 0.0) => return None,
            Ok(Err(e)) => return Some(trim(format!("{label}: {e}\nouter {outer:?}\nholes {holes:?}\nweights {weights:?}"))),
            Ok(Ok(sk)) => sk,
        };
        let problems = check(&sk, 1e-6);
        if !problems.is_empty() {
            return Some(trim(format!("{label}: {problems:#?}\nouter {outer:?}\nholes {holes:?}\nweights {weights:?}")));
        }
        if sk.nodes.iter().any(|n| n.time < -1e-9) || !sk.max_time().is_finite() {
            return Some(trim(format!("{label}: a node has a negative or infinite time\nouter {outer:?}\nweights {weights:?}")));
        }
        let failure = catch_unwind(AssertUnwindSafe(|| heights_match_wavefront(label, outer, holes, weights, &sk))).err()?;
        let message = failure.downcast_ref::<String>().cloned().or_else(|| failure.downcast_ref::<&str>().map(|s| s.to_string()));
        Some(trim(format!("{}\nouter {outer:?}\nholes {holes:?}\nweights {weights:?}", message.unwrap_or_else(|| label.to_string()))))
    }

    /// Random weights for rings of the given sizes: each edge is pinned (a gable) with probability `gable`, else drawn by
    /// `mode` (0: uniform in [0.5, 2], 1: one of 0.5, 1, 2, 2: one of 1, 2, 3, 3 and 4: from 0.1 to 10 and from 0.01 to 100, evenly
    /// in the logarithm, 5: as 0 but all of them scaled by one factor from 1e-6 to 1e6). The outer ring always has a moving edge.
    fn random_weights(rng: &mut Lcg, lens: &[usize], gable: f64, mode: usize) -> Vec<Vec<f64>> {
        let scale = if mode == 5 { 10f64.powf(rng.next() * 12.0 - 6.0) } else { 1.0 };
        let mut weights: Vec<Vec<f64>> = lens
            .iter()
            .map(|&n| {
                (0..n)
                    .map(|_| match mode {
                        _ if rng.next() < gable => 0.0,
                        0 => 0.5 + 1.5 * rng.next(),
                        1 => [0.5, 1.0, 2.0][rng.below(3)],
                        2 => 1.0 + rng.below(3) as f64,
                        3 => 10f64.powf(rng.next() * 2.0 - 1.0),
                        4 => 10f64.powf(rng.next() * 4.0 - 2.0),
                        _ => scale * (0.5 + 1.5 * rng.next()),
                    })
                    .collect()
            })
            .collect();
        if let Some(outer) = weights.first_mut()
            && outer.iter().all(|&w| w == 0.0)
            && let Some(w) = outer.first_mut()
        {
            *w = 1.0;
        }
        weights
    }

    fn lens(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<usize> {
        std::iter::once(outer.len()).chain(holes.iter().map(Vec::len)).collect()
    }

    /// Combs, crosses, Ts, Hs, Us, L-shapes, a dumbbell and stars: many parallel edges.
    fn fixed_shapes() -> Vec<Vec<Vec2>> {
        let comb = pts(&[
            (0.0, 0.0),
            (9.0, 0.0),
            (9.0, 5.0),
            (8.0, 5.0),
            (8.0, 2.0),
            (7.0, 2.0),
            (7.0, 5.0),
            (6.0, 5.0),
            (6.0, 2.0),
            (5.0, 2.0),
            (5.0, 5.0),
            (4.0, 5.0),
            (4.0, 2.0),
            (3.0, 2.0),
            (3.0, 5.0),
            (0.0, 5.0),
        ]);
        let cross = pts(&[
            (2.0, 0.0),
            (4.0, 0.0),
            (4.0, 2.0),
            (6.0, 2.0),
            (6.0, 4.0),
            (4.0, 4.0),
            (4.0, 6.0),
            (2.0, 6.0),
            (2.0, 4.0),
            (0.0, 4.0),
            (0.0, 2.0),
            (2.0, 2.0),
        ]);
        let t = pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 2.0), (6.0, 2.0), (6.0, 6.0), (4.0, 6.0), (4.0, 2.0), (0.0, 2.0)]);
        let h = pts(&[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 4.0),
            (6.0, 4.0),
            (6.0, 0.0),
            (8.0, 0.0),
            (8.0, 10.0),
            (6.0, 10.0),
            (6.0, 6.0),
            (2.0, 6.0),
            (2.0, 10.0),
            (0.0, 10.0),
        ]);
        let u = pts(&[(0.0, 0.0), (6.0, 0.0), (6.0, 6.0), (4.0, 6.0), (4.0, 2.0), (2.0, 2.0), (2.0, 6.0), (0.0, 6.0)]);
        let l = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let star: Vec<Vec2> = (0..16).map(|i| Vec2::polar(Vec2::ZERO, if i % 2 == 0 { 10.0 } else { 4.0 }, crate::TAU * i as f64 / 16.0)).collect();
        vec![comb, cross, t, h, u, l, dumbbell(), star, square(0.0, 0.0, 100.0, 0.5), pts(&[(0.0, 0.0), (50.0, 0.0), (0.0, 0.01)])]
    }

    /// The ring without repeated or straight-through vertices (which could not carry different weights).
    fn tidy(ring: Vec<Vec2>) -> Vec<Vec2> {
        clean_ring(&ring, 1e-9, Spikes::Reject).unwrap_or(ring)
    }

    /// A convex polygon: points on an ellipse at random angles.
    fn random_convex(rng: &mut Lcg) -> Vec<Vec2> {
        let n = 3 + rng.below(9);
        let mut angles: Vec<f64> = (0..n).map(|_| rng.next() * crate::TAU).collect();
        angles.sort_by(f64::total_cmp);
        let (rx, ry) = (2.0 + 6.0 * rng.next(), 2.0 + 6.0 * rng.next());
        angles.iter().map(|&a| Vec2::new(rx * a.cos(), ry * a.sin())).collect()
    }

    /// On a convex polygon the wavefront at time `t` is the intersection of the edges' moving half-planes, so a point's
    /// height is the smallest `distance / weight` over the moving edges: every node's time must be exactly that.
    fn convex_envelope_problem(label: &str, ring: &[Vec2], weights: &[f64]) -> Option<String> {
        let sk = match weighted_skeleton(ring, &[], &[weights.to_vec()]) {
            Ok(sk) => sk,
            Err(SkeletonError::NotSimple | SkeletonError::TooFewVertices) => return None,
            Err(e) => return Some(format!("{label}: {e}\nring {ring:?}\nweights {weights:?}")),
        };
        let problems = check(&sk, 1e-6);
        if !problems.is_empty() {
            return Some(format!("{label}: {problems:#?}\nring {ring:?}\nweights {weights:?}"));
        }
        let cleaned = &sk.rings[0];
        let n = cleaned.len();
        for (i, node) in sk.nodes.iter().enumerate().skip(n) {
            let height = (0..n)
                .filter(|&e| sk.weights[0][e] > 0.0)
                .map(|e| unit(cleaned[(e + 1) % n] - cleaned[e]).cross(node.pos - cleaned[e]) / sk.weights[0][e])
                .fold(f64::INFINITY, f64::min);
            if (height - node.time).abs() > 1e-6 * (1.0 + node.time) {
                return Some(format!(
                    "{label}: node {i} has time {} but the lowest plane over it is at {height}\nring {ring:?}\nweights {weights:?}",
                    node.time
                ));
            }
        }
        None
    }

    /// Weighted skeletons of random shapes (with random weights, some of them gables) satisfy every invariant. `seeds` x
    /// `cases` rounds; returns how many shapes were tried and the failures.
    fn weighted_campaign(seeds: &[u64], cases: usize) -> (usize, Vec<String>) {
        let mut failures: Vec<String> = Vec::new();
        let mut shapes = 0;
        for &seed in seeds {
            let mut rng = Lcg(seed);
            for case in 0..cases {
                let mode = case % 6;
                let gable = [0.0, 0.0, 0.15, 0.3, 0.5][rng.below(5)];
                // Polyominoes (axis-parallel outlines with holes, so parallel edges meet at different speeds).
                let size = 3 + rng.below(6) as i32;
                let mut cells = std::collections::HashSet::new();
                for x in 0..size {
                    for y in 0..size {
                        if rng.next() < 0.45 + 0.4 * rng.next() {
                            cells.insert((x, y));
                        }
                    }
                }
                if let Some(&first) = cells.iter().min() {
                    let mut component = std::collections::HashSet::from([first]);
                    let mut frontier = vec![first];
                    while let Some((x, y)) = frontier.pop() {
                        for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                            if cells.contains(&n) && component.insert(n) {
                                frontier.push(n);
                            }
                        }
                    }
                    let (mut outlines, mut holes) = (Vec::new(), Vec::new());
                    for ring in cell_loops(&component).unwrap_or_default() {
                        let ring = tidy(ring);
                        if shoelace(&ring) > 0.0 { outlines.push(ring) } else { holes.push(ring) }
                    }
                    if let Ok([outer]) = <[Vec<Vec2>; 1]>::try_from(outlines) {
                        let angle = if case % 2 == 0 { 0.0 } else { rng.next() * crate::TAU };
                        let noise = [0.0, 0.0, 1e-3, 1e-6, 1e-9][case % 5];
                        let mut nudge = |p: &Vec2| *p + Vec2::new((rng.next() - 0.5) * noise, (rng.next() - 0.5) * noise);
                        let o: Vec<Vec2> = rotated(&outer, angle).iter().map(&mut nudge).collect();
                        let h: Vec<Vec<Vec2>> = holes.iter().map(|h| rotated(h, angle).iter().map(&mut nudge).collect()).collect();
                        let weights = random_weights(&mut rng, &lens(&o, &h), gable, mode);
                        shapes += 1;
                        failures.extend(weighted_problem(
                            &format!("polyomino {case} (seed {seed}, angle {angle}, noise {noise}, mode {mode})"),
                            &o,
                            &h,
                            &weights,
                        ));
                    }
                }
                // Stars, snapped to a grid half of the time, with and without a hole.
                let n = 3 + rng.below(20);
                let snap = [0.0, 1.0, 0.5, 0.0][case % 4];
                let star: Vec<Vec2> = (0..n)
                    .map(|i| {
                        let p = Vec2::polar(Vec2::ZERO, 2.0 + 8.0 * rng.next(), crate::TAU * (i as f64 + 0.9 * rng.next()) / n as f64);
                        if snap > 0.0 { Vec2::new((p.x / snap).round() * snap, (p.y / snap).round() * snap) } else { p }
                    })
                    .collect();
                let star = tidy(star);
                let holes = if case % 5 == 0 { vec![square(-1.0, -1.0, 1.0, 1.0)] } else { vec![] };
                let weights = random_weights(&mut rng, &lens(&star, &holes), gable, mode);
                shapes += 1;
                failures.extend(weighted_problem(&format!("star {case} (seed {seed}, snap {snap}, mode {mode})"), &star, &holes, &weights));
                // Convex polygons, checked against the closed form.
                let convex = random_convex(&mut rng);
                let weights = random_weights(&mut rng, &[convex.len()], gable, mode);
                shapes += 1;
                failures.extend(convex_envelope_problem(&format!("convex {case} (seed {seed}, mode {mode})"), &convex, &weights[0]));
                // Histograms: columns of integer heights, so many events coincide.
                let columns = 2 + rng.below(5);
                let mut ring = vec![Vec2::new(0.0, 0.0), Vec2::new(columns as f64, 0.0)];
                for i in (0..columns).rev() {
                    let h = 1.0 + rng.below(3) as f64;
                    ring.push(Vec2::new(i as f64 + 1.0, h));
                    ring.push(Vec2::new(i as f64, h));
                }
                let ring = tidy(ring);
                let ring = if case % 2 == 0 { ring } else { rotated(&ring, rng.next() * crate::TAU) };
                let weights = random_weights(&mut rng, &[ring.len()], gable, mode);
                shapes += 1;
                failures.extend(weighted_problem(&format!("histogram {case} (seed {seed}, mode {mode})"), &ring, &[], &weights));
                // A room with rectangular holes on a grid.
                let (cols, rows) = (2 + rng.below(3), 2 + rng.below(3));
                let cell = 6.0;
                let room = square(0.0, 0.0, cols as f64 * cell, rows as f64 * cell);
                let mut holes = Vec::new();
                for cx in 0..cols {
                    for cy in 0..rows {
                        if rng.next() < 0.5 {
                            let (w, h) = (1.0 + rng.below(3) as f64, 1.0 + rng.below(3) as f64);
                            let (x, y) = (cx as f64 * cell + 1.5 + rng.below(2) as f64, cy as f64 * cell + 1.5 + rng.below(2) as f64);
                            holes.push(square(x, y, x + w, y + h));
                        }
                    }
                }
                let angle = if case % 3 == 0 { 0.0 } else { rng.next() * crate::TAU };
                let turned: Vec<Vec<Vec2>> = holes.iter().map(|h| rotated(h, angle)).collect();
                let weights = random_weights(&mut rng, &lens(&room, &holes), gable, mode);
                shapes += 1;
                failures.extend(weighted_problem(
                    &format!("room {case} with {} holes (seed {seed}, mode {mode})", holes.len()),
                    &rotated(&room, angle),
                    &turned,
                    &weights,
                ));
                // The fixed shapes of the unweighted tests, with their own random weights.
                let shapes_list = fixed_shapes();
                let fixed = &shapes_list[case % shapes_list.len()];
                let fixed = if case % 2 == 0 { fixed.clone() } else { rotated(fixed, rng.next() * crate::TAU) };
                let weights = random_weights(&mut rng, &[fixed.len()], gable, mode);
                shapes += 1;
                failures.extend(weighted_problem(
                    &format!("fixed shape {} (case {case}, seed {seed}, mode {mode})", case % shapes_list.len()),
                    &fixed,
                    &[],
                    &weights,
                ));
            }
        }
        (shapes, failures)
    }

    fn assert_campaign(seeds: &[u64], cases: usize) {
        let (shapes, failures) = weighted_campaign(seeds, cases);
        assert!(
            failures.is_empty(),
            "{} of {shapes} weighted shapes failed; the first:\n{}",
            failures.len(),
            failures.iter().take(3).cloned().collect::<Vec<_>>().join("\n\n")
        );
    }

    #[test]
    fn weighted_randomized_campaign() {
        assert_campaign(&[101, 202, 303], 200);
    }

    /// A longer run of the same campaign (about a minute): `cargo test -p cadcraft-geom weighted_soak -- --ignored`.
    #[test]
    #[ignore]
    fn weighted_soak() {
        let seeds: Vec<u64> = (1..=100).collect();
        assert_campaign(&seeds, 400);
    }

    /// Whatever the weights, the function returns (a result or an error) without panicking or looping.
    #[test]
    fn hostile_weights_never_panic() {
        let mut rng = Lcg(4321);
        let specials = [0.0, -0.0, 1.0, 2.0, 0.5, 1e-300, -1.0, 1e300, f64::MAX, f64::MIN_POSITIVE, f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        let mut accepted = 0;
        for _ in 0..1500 {
            let ring = |rng: &mut Lcg, closed: bool| -> Vec<Vec2> {
                let mut r: Vec<Vec2> = (0..3 + rng.below(6)).map(|_| Vec2::new((rng.next() - 0.5) * 10.0, (rng.next() - 0.5) * 10.0)).collect();
                if closed && let Some(&first) = r.first() {
                    r.push(first);
                }
                r
            };
            let closed = rng.next() < 0.3;
            let outer = if rng.next() < 0.5 { square(0.0, 0.0, 4.0, 3.0) } else { ring(&mut rng, closed) };
            let hole_count = rng.below(3);
            let holes: Vec<Vec<Vec2>> = (0..hole_count).map(|_| ring(&mut rng, false)).collect();
            let rings = 1 + if rng.next() < 0.8 { holes.len() } else { rng.below(4) };
            let weights: Vec<Vec<f64>> = (0..rings)
                .map(|r| {
                    let n = if r == 0 { outer.len() } else { holes.get(r - 1).map_or(3, Vec::len) };
                    let n = if rng.next() < 0.85 { n } else { rng.below(n + 3) };
                    (0..n).map(|_| if rng.next() < 0.7 { 0.5 + rng.next() } else { specials[rng.below(specials.len())] }).collect()
                })
                .collect();
            if weighted_skeleton(&outer, &holes, &weights).is_ok() {
                accepted += 1;
            }
        }
        assert!(accepted > 20, "{accepted} accepted");
    }
    #[test]
    fn a_vertex_between_two_pinned_edges_does_not_move() {
        // Top and left pinned: the corner between them stays put, and everything meets there at time 2.
        let sk = weighted(&square(0.0, 0.0, 2.0, 2.0), &[], &[vec![1.0, 1.0, 0.0, 0.0]]);
        assert_valid(&sk);
        let inner = interior(&sk);
        assert_eq!(inner.len(), 1, "{inner:?}");
        assert_node(&inner[0], 0.0, 2.0, 2.0);
        let areas = face_areas(&sk);
        assert!((areas[0] - 2.0).abs() < 1e-9 && (areas[1] - 2.0).abs() < 1e-9 && areas[2].abs() < 1e-9 && areas[3].abs() < 1e-9, "{areas:?}");
        // The corner's own ray rises straight up the roof: its two ends are at the same place.
        let top_left = 3;
        assert!(sk.arcs.iter().any(|&(a, b)| (a == top_left && b == 4) || (a == 4 && b == top_left)), "{:?}", sk.arcs);
    }

    /// A face can enclose an island: only the bottom edge moves, and it sweeps the whole region round a pinned hole. The face
    /// then runs round the outer boundary, crosses to the hole on a bridge arc and walks round the hole clockwise.
    #[test]
    fn a_face_can_enclose_an_island() {
        let hole = vec![square(3.0, 3.0, 7.0, 7.0)];
        let weights = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.0; 4]];
        let sk = weighted(&square(0.0, 0.0, 10.0, 10.0), &hole, &weights);
        assert_valid(&sk);
        let areas = face_areas(&sk);
        assert!((areas[0] - 84.0).abs() < 1e-9, "{areas:?}");
        assert!(areas[1..].iter().all(|a| a.abs() < 1e-9), "{areas:?}");
        // The island's four corners are on the swept face's boundary (all the hole's nodes appear in it).
        let bottom = &sk.faces[0].nodes;
        assert!(bottom.len() > 8, "{bottom:?}");
        heights_match_wavefront("island", &square(0.0, 0.0, 10.0, 10.0), &hole, &weights, &sk);
    }

    /// The structure of a weighted skeleton does not depend on the scale or position of the drawing.
    #[test]
    fn weighted_structure_is_scale_free() {
        let l = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let weights = vec![vec![1.0, 2.0, 0.5, 0.0, 3.0, 1.5]];
        let base = weighted(&l, &[], &weights);
        for scale in [1e-6, 1e-3, 1.0, 1e3, 1e6] {
            let big: Vec<Vec2> = l.iter().map(|p| *p * scale + Vec2::new(5.0 * scale, -3.0 * scale)).collect();
            let sk = weighted(&big, &[], &weights);
            assert_eq!((sk.nodes.len(), sk.arcs.len(), sk.faces.len()), (base.nodes.len(), base.arcs.len(), base.faces.len()), "scale {scale}");
            assert!(check(&sk, 1e-6 * scale.max(1.0)).is_empty(), "scale {scale}: {:?}", check(&sk, 1e-6 * scale.max(1.0)));
            assert!((sk.max_time() - base.max_time() * scale).abs() < 1e-8 * scale, "scale {scale}");
        }
        // Very steep and very shallow roofs: slopes from 1e-3 to 1e3.
        let rect = square(0.0, 0.0, 4.0, 2.0);
        for slope in [1e-3, 0.1, 1.0, 10.0, 1e3] {
            let sk = roof_skeleton(&rect, &[], &[vec![slope, f64::INFINITY, slope, f64::INFINITY]]).unwrap();
            assert!(check(&sk, 1e-6 * slope.max(1.0)).is_empty(), "slope {slope}");
            assert!((sk.max_time() - slope).abs() < 1e-9 * slope.max(1.0), "slope {slope}: {}", sk.max_time());
        }
    }

    /// The same weighted polygon given in different ways (any starting vertex, either direction, repeated points, vertices
    /// in the middle of edges, a repeated closing point) has the same skeleton, and the weights follow their edges.
    #[test]
    fn weighted_skeleton_does_not_depend_on_how_the_input_is_written() {
        let mut rng = Lcg(2024);
        for case in 0..300 {
            // A star polygon with weights (some pinned) as the reference, counter-clockwise.
            let n = 4 + rng.below(8);
            let ring: Vec<Vec2> =
                (0..n).map(|i| Vec2::polar(Vec2::ZERO, 3.0 + 6.0 * rng.next(), crate::TAU * (i as f64 + 0.8 * rng.next()) / n as f64)).collect();
            let weights = random_weights(&mut rng, &[n], 0.2, case % 3).remove(0);
            let Ok(reference) = weighted_skeleton(&ring, &[], std::slice::from_ref(&weights)) else { continue };
            // Rewrite it: rotate the start, then maybe add repeated points and midpoints (the edge's weight on both halves).
            let start = rng.below(n);
            let mut points = Vec::new();
            let mut ws = Vec::new();
            for k in 0..n {
                let i = (start + k) % n;
                let j = (i + 1) % n;
                points.push(ring[i]);
                if rng.next() < 0.3 {
                    // A repeated point: the edge between the two copies is empty and carries any weight.
                    points.push(ring[i]);
                    ws.push(7.0 + rng.next());
                }
                ws.push(weights[i]);
                if rng.next() < 0.3 {
                    points.push(ring[i].mid(ring[j]));
                    ws.push(weights[i]);
                }
            }
            if rng.next() < 0.3 {
                // A repeated closing point.
                if let Some(&first) = points.first() {
                    points.push(first);
                    ws.push(5.0);
                }
            }
            let reversed = rng.next() < 0.5;
            if reversed {
                // Reversing the point order sends point m to len - 1 - m; edge e (from point e to e + 1) becomes the edge from point
                // len - 1 - e - 1 + 1..., i.e. new edge (len - 2 - e) mod len.
                let len = points.len();
                points.reverse();
                let old = ws.clone();
                for (e, w) in old.iter().enumerate() {
                    ws[(2 * len - 2 - e) % len] = *w;
                }
            }
            let sk = match weighted_skeleton(&points, &[], &[ws.clone()]) {
                Ok(sk) => sk,
                Err(e) => panic!("case {case}: {e}\npoints {points:?}\nweights {ws:?}"),
            };
            assert!(check(&sk, 1e-6).is_empty(), "case {case}: {:?}", check(&sk, 1e-6));
            assert_eq!(sk.rings[0].len(), reference.rings[0].len(), "case {case}");
            // Same edges with the same weights: every cleaned edge is an edge of the reference with its weight.
            let edges_of = |s: &Skeleton| -> Vec<(i64, i64, i64, i64, i64)> {
                let r = &s.rings[0];
                let key = |x: f64| (x * 1e6).round() as i64;
                let mut v: Vec<_> = (0..r.len())
                    .map(|e| (key(r[e].x), key(r[e].y), key(r[(e + 1) % r.len()].x), key(r[(e + 1) % r.len()].y), key(s.weights[0][e])))
                    .collect();
                v.sort_unstable();
                v
            };
            assert_eq!(edges_of(&sk), edges_of(&reference), "case {case}");
            let times = |s: &Skeleton| -> Vec<(i64, i64, i64)> {
                let key = |x: f64| (x * 1e6).round() as i64;
                let mut v: Vec<_> = s.nodes.iter().map(|n| (key(n.pos.x), key(n.pos.y), key(n.time))).collect();
                v.sort_unstable();
                v
            };
            assert_eq!(times(&sk), times(&reference), "case {case}");
        }
    }

    /// A few hundred vertices with random weights (some pinned) is as routine as without.
    #[test]
    fn weighted_few_hundred_vertices() {
        let mut rng = Lcg(5);
        for (n, gable) in [(200usize, 0.0), (400, 0.0), (200, 0.1), (400, 0.05)] {
            let ring: Vec<Vec2> =
                (0..n).map(|i| Vec2::polar(Vec2::ZERO, 50.0 + 50.0 * rng.next(), crate::TAU * (i as f64 + 0.9 * rng.next()) / n as f64)).collect();
            let weights = random_weights(&mut rng, &[n], gable, 0);
            match weighted_skeleton(&ring, &[], &weights) {
                Ok(sk) => assert!(check(&sk, 1e-6).is_empty(), "n={n} gable={gable}: {:?}", check(&sk, 1e-6)),
                // Pinned edges can leave a part of the region nothing sweeps.
                Err(SkeletonError::BadWeights) if gable > 0.0 => {}
                Err(e) => panic!("n={n} gable={gable}: {e}"),
            }
        }
    }
}
