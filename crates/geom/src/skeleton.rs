//! Straight skeleton of a polygon with holes, and the offsets it gives.
//!
//! Assisted draft (2026-10-25): written ahead of phase 4 (automatic roofs) by Claude, not by the swarm.
//!
//! Every edge of the input moves inward at unit speed, parallel to itself. The skeleton is the set of paths the
//! vertices trace until the shrinking wavefront vanishes. Each input edge sweeps one face. A node's `time` is how far
//! the wavefront had travelled when it got there, which is its distance to the edges of the faces around it. So:
//! - a hip roof over the polygon is the skeleton lifted by `time * slope` (each face is one roof plane);
//! - the inward offset by `d` is the wavefront at time `d` ([`inset`]);
//! - the outward offset by `d` is the same construction on the outside ([`outset`]).
//!
//! The functions never panic and never loop forever: bad input returns a [`SkeletonError`], and the event simulation
//! stops with [`SkeletonError::EventLimit`] if it ever exceeds its event budget.

use crate::Vec2;

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
    /// The face's boundary as node indices, counter-clockwise, starting with the edge's two end vertices.
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
}

impl std::fmt::Display for SkeletonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SkeletonError::TooFewVertices => "a ring has fewer than 3 distinct vertices",
            SkeletonError::NotSimple => "the rings cross themselves or each other",
            SkeletonError::HoleOutside => "a hole is not inside the outer ring",
            SkeletonError::NotFinite => "a coordinate or distance is not a finite number (or a distance is negative)",
            SkeletonError::EventLimit => "the straight-skeleton event simulation exceeded its budget",
        })
    }
}

impl std::error::Error for SkeletonError {}

/// The straight skeleton of `outer` with `holes`. Rings may be given in either orientation and may repeat their first
/// point at the end; they are cleaned and oriented as described on [`Skeleton::rings`].
pub fn straight_skeleton(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Result<Skeleton, SkeletonError> {
    let _ = (outer, holes);
    todo!("straight skeleton: wavefront simulation with edge and split events")
}

/// The inward offset of `outer` with `holes` by `d >= 0`: the wavefront at time `d`. One region per piece the polygon
/// splits into; empty once `d` reaches [`Skeleton::max_time`]. `d == 0` returns the cleaned input.
pub fn inset(outer: &[Vec2], holes: &[Vec<Vec2>], d: f64) -> Result<Vec<OffsetRegion>, SkeletonError> {
    let _ = (outer, holes, d);
    todo!("inset from the skeleton")
}

/// The outward offset of `outer` with `holes` by `d >= 0`, with mitred (sharp) corners as a straight skeleton gives.
/// Holes shrink and vanish once `d` is large enough.
pub fn outset(outer: &[Vec2], holes: &[Vec<Vec2>], d: f64) -> Result<OffsetRegion, SkeletonError> {
    let _ = (outer, holes, d);
    todo!("outset: the skeleton of the outside")
}

/// Problems with a skeleton, for tests and debugging; empty when every invariant holds within `tol`:
/// each face's nodes lie at distance `time` from the line of the face's edge, the faces' areas add up to the region's
/// area, every node lies inside the region or on its boundary, and every arc joins two existing nodes.
pub fn check(skeleton: &Skeleton, tol: f64) -> Vec<String> {
    let _ = (skeleton, tol);
    todo!("invariant checks")
}
