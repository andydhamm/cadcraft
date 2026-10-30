//! Operations on a [`Tin`]: a grid index for point location, clipping to a ring, and chaining
//! contour segments into polylines. Filled in by `p9-geom-tin-ops`; until then the index scans
//! every triangle, clipping keeps everything and every segment is its own line.

use crate::Vec2;
use crate::tin::{ContourSegment, Tin, barycentric};

/// A uniform grid over a TIN's triangles for fast point location.
#[derive(Clone, Debug, Default)]
pub struct TinIndex {
    triangles: usize,
}

impl TinIndex {
    /// Index `tin`'s triangles.
    pub fn new(tin: &Tin) -> TinIndex {
        TinIndex { triangles: tin.triangles.len() }
    }

    /// The triangle containing `p` and `p`'s barycentric weights for its three corners, or
    /// `None` outside the TIN.
    pub fn locate(&self, tin: &Tin, p: Vec2) -> Option<(usize, [f64; 3])> {
        for (i, t) in tin.triangles.iter().enumerate().take(self.triangles) {
            let q = |k: usize| tin.points.get(k).map(|v| Vec2::new(v.x, v.y));
            let (Some(a), Some(b), Some(c)) = (q(t[0]), q(t[1]), q(t[2])) else { continue };
            if let Some((u, v, w)) = barycentric(a, b, c, p) {
                return Some((i, [u, v, w]));
            }
        }
        None
    }
}

/// Keep only the triangles of `tin` whose centroid lies inside `ring` (even-odd).
pub fn retain_inside(tin: &mut Tin, ring: &[Vec2]) {
    let _ = (tin, ring);
}

/// A contour polyline at one elevation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContourLine {
    pub elevation: f64,
    pub points: Vec<Vec2>,
    /// Whether the last point joins the first.
    pub closed: bool,
}

/// Chain segments that share endpoints (same elevation) into polylines.
pub fn chain_contours(segs: &[ContourSegment]) -> Vec<ContourLine> {
    segs.iter().map(|s| ContourLine { elevation: s.elevation, points: vec![s.start, s.end], closed: false }).collect()
}
