//! Constrained Delaunay triangulation: a [`Tin`] in which given segments (breaklines, boundaries)
//! are triangle edges. Filled in by `p9-geom-cdt`; until then it is the plain triangulation.

use crate::Vec3;
use crate::tin::{Tin, TinError, triangulate};

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

/// The Delaunay triangulation of `points` in which every `constraints` segment (a pair of point
/// indices) is a union of triangle edges.
pub fn triangulate_constrained(points: &[Vec3], constraints: &[[usize; 2]]) -> Result<Cdt, TinError> {
    let _ = constraints;
    Ok(Cdt { tin: triangulate(points)?, constrained: Vec::new() })
}
