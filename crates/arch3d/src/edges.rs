//! Feature edges: the lines a drawing shows (phase 6 "shaded with edges", phase 7 generated elevations).
//!
//! An edge of a mesh is a feature when it bounds an open surface (one triangle uses it), is shared by more than two
//! triangles, or joins two triangles whose faces meet at more than the crease angle ([`CREASE_DEG`], 20°).
//! Triangulation diagonals of a planar face join two coplanar triangles, so they are never features. For one view,
//! [`EdgeKind::Silhouette`] adds the smooth edges where the surface turns away from the viewer (a round column's
//! outline), which no crease angle catches.
//!
//! Vertices are welded by position first ([`WELD`]): a builder may repeat a corner once per face, and unwelded copies
//! would turn every face border into a boundary. Pure CPU and deterministic: the same mesh gives the same edges in the
//! same order.

use std::collections::{BTreeMap, HashMap};

use crate::{ArchId, Mesh, Scene3d};

/// Faces meeting at more than this many degrees show their shared edge.
pub const CREASE_DEG: f64 = 20.0;

/// Vertices that round to the same multiple of this many metres on every axis are one vertex.
pub const WELD: f64 = 1e-5;

/// Why an edge is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// It bounds an open surface: one triangle uses it.
    Boundary,
    /// Two triangles meet here at more than the crease angle.
    Crease,
    /// More than two triangles share it.
    NonManifold,
    /// One of its two triangles faces the viewer and the other faces away (view-dependent).
    Silhouette,
}

/// One feature edge in world space (metres), tagged with the arch object it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureEdge {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub kind: EdgeKind,
    pub object: ArchId,
}

impl FeatureEdge {
    pub const fn new(a: [f64; 3], b: [f64; 3], kind: EdgeKind, object: ArchId) -> Self {
        FeatureEdge { a, b, kind, object }
    }
}

/// A mesh with welded vertices and one unit normal per triangle. Triangles with a non-finite corner, an index out of
/// range, two corners welded together or no area are left out.
pub(crate) struct Welded {
    pub points: Vec<[f64; 3]>,
    pub tris: Vec<[u32; 3]>,
    pub normals: Vec<[f64; 3]>,
}

impl Welded {
    pub(crate) fn point(&self, i: u32) -> [f64; 3] {
        self.points.get(i as usize).copied().unwrap_or([0.0; 3])
    }

    pub(crate) fn corners(&self, t: &[u32; 3]) -> [[f64; 3]; 3] {
        [self.point(t[0]), self.point(t[1]), self.point(t[2])]
    }
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `v` scaled to unit length, or `None` when it is (nearly) zero or not finite.
pub(crate) fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = dot(v, v).sqrt();
    (l.is_finite() && l > 1e-12).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

pub(crate) fn weld(mesh: &Mesh) -> Welded {
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let mut points: Vec<[f64; 3]> = Vec::new();
    let remap: Vec<Option<u32>> = mesh
        .positions
        .iter()
        .map(|p| {
            let q = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
            if !q.iter().all(|v| v.is_finite()) {
                return None;
            }
            // `as` saturates, so a huge coordinate gets a (shared) extreme key instead of panicking.
            let key = [(q[0] / WELD).round() as i64, (q[1] / WELD).round() as i64, (q[2] / WELD).round() as i64];
            Some(*ids.entry(key).or_insert_with(|| {
                points.push(q);
                (points.len() - 1) as u32
            }))
        })
        .collect();
    let mut tris = Vec::new();
    let mut normals = Vec::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        let [Some(i), Some(j), Some(k)] = t.map(|i| remap.get(i as usize).copied().flatten()) else { continue };
        if i == j || j == k || k == i {
            continue;
        }
        let (a, b, c) = (points[i as usize], points[j as usize], points[k as usize]);
        let Some(n) = unit(cross(sub(b, a), sub(c, a))) else { continue };
        tris.push([i, j, k]);
        normals.push(n);
    }
    Welded { points, tris, normals }
}

/// The triangles using each undirected edge `(low, high)` of a welded mesh, in edge order.
pub(crate) fn edge_triangles(w: &Welded) -> BTreeMap<(u32, u32), Vec<usize>> {
    let mut by_edge: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (n, t) in w.tris.iter().enumerate() {
        for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            by_edge.entry((i.min(j), i.max(j))).or_default().push(n);
        }
    }
    by_edge
}

/// The feature edges of a welded mesh; with `view_dir`, also its silhouette edges seen along that direction.
pub(crate) fn welded_edges(w: &Welded, crease_deg: f64, view_dir: Option<[f64; 3]>, object: ArchId) -> Vec<FeatureEdge> {
    let cos = crease_deg.to_radians().cos();
    let normal = |t: usize| w.normals.get(t).copied().unwrap_or([0.0; 3]);
    let mut out = Vec::new();
    for ((i, j), ts) in edge_triangles(w) {
        let kind = match ts.as_slice() {
            [_] => EdgeKind::Boundary,
            [s, t] => {
                let (ns, nt) = (normal(*s), normal(*t));
                if dot(ns, nt) < cos {
                    EdgeKind::Crease
                } else if view_dir.is_some_and(|d| {
                    let (fs, ft) = (dot(ns, d), dot(nt, d));
                    (fs > 1e-9 && ft < -1e-9) || (fs < -1e-9 && ft > 1e-9)
                }) {
                    EdgeKind::Silhouette
                } else {
                    continue;
                }
            }
            _ => EdgeKind::NonManifold,
        };
        out.push(FeatureEdge::new(w.point(i), w.point(j), kind, object));
    }
    out
}

/// The feature edges of one mesh (boundary, crease over `crease_deg`, non-manifold), in a fixed order.
pub fn feature_edges(mesh: &Mesh, crease_deg: f64) -> Vec<FeatureEdge> {
    welded_edges(&weld(mesh), crease_deg, None, mesh.object)
}

/// The feature edges of every mesh in the scene, mesh by mesh.
pub fn scene_feature_edges(scene: &Scene3d, crease_deg: f64) -> Vec<FeatureEdge> {
    scene.meshes.iter().flat_map(|m| feature_edges(m, crease_deg)).collect()
}
