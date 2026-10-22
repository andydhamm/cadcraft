//! 3D mesh primitives: vertex data, triangulation, manifold checks, and prism building.
//!
//! This module is intentionally free of any `render` dependency; the ear-clipping
//! triangulator here is self-contained and used by the arch3d builder.

use std::collections::HashMap;

use cadcraft_doc::ArchId;
use cadcraft_geom::Vec2;

/// Axis-aligned bounding box in 3D world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds3 {
    /// A zero-sized box centred on the origin.
    pub const fn empty() -> Self {
        Bounds3 { min: [0.0; 3], max: [0.0; 3] }
    }

    /// Grow `self` to include `other`.
    pub fn extend(&mut self, other: &Bounds3) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(other.min[i]);
            self.max[i] = self.max[i].max(other.max[i]);
        }
    }
}

/// A triangulated 3D mesh with per-vertex attributes.
#[derive(Debug, Clone)]
pub struct Mesh {
    /// World-space vertex positions (x, y, z).
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex unit normals.
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex texture coordinates (world-box UV: u = x, v = z).
    pub uvs: Vec<[f32; 2]>,
    /// Triangle vertex indices (groups of three).
    pub indices: Vec<u32>,
    /// Material name.
    pub material: String,
    /// The arch object this mesh belongs to.
    pub object: ArchId,
}

impl Mesh {
    /// Create an empty mesh.
    pub fn new(material: &str, object: ArchId) -> Self {
        Mesh { positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), indices: Vec::new(), material: material.to_string(), object }
    }

    /// Number of triangles in this mesh.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Axis-aligned bounds of the mesh vertices.
    pub fn bounds(&self) -> Bounds3 {
        if self.positions.is_empty() {
            return Bounds3::empty();
        }
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for p in &self.positions {
            for i in 0..3 {
                let v = p[i] as f64;
                if v < min[i] {
                    min[i] = v;
                }
                if v > max[i] {
                    max[i] = v;
                }
            }
        }
        Bounds3 { min, max }
    }

    /// Check if the mesh is a valid closed 2-manifold.
    ///
    /// Every directed edge must appear the same number of times as its reverse
    /// (each undirected edge is shared by exactly two triangles).
    pub fn is_manifold(&self) -> bool {
        let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
        for tri in self.indices.chunks(3) {
            if tri.len() != 3 {
                return false;
            }
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            if a == b || b == c || c == a {
                return false;
            }
            for edge in [(a, b), (b, c), (c, a)] {
                *edge_count.entry(edge).or_insert(0) += 1;
            }
        }
        for (&(a, b), &count) in edge_count.iter() {
            match edge_count.get(&(b, a)) {
                Some(&r) if r != count => return false,
                Some(_) => {}
                None => return false,
            }
        }
        true
    }

    /// Append a closed prism (ring extruded from z0 to z1) to this mesh.
    ///
    /// The ring must be a polygon of at least 3 finite vertices with non-zero
    /// area. A clockwise ring is automatically flipped to counter-clockwise so
    /// that normals point outward. Both end caps and all four side faces per
    /// edge are included.
    ///
    /// World-box UVs: u = x (world), v = z (world).
    pub fn push_prism(&mut self, ring: &[Vec2], z0: f64, z1: f64) {
        let n = ring.len() as u32;
        if n < 3 {
            return;
        }
        for p in ring {
            if !p.is_finite() {
                return;
            }
        }
        let area = polygon_area(ring);
        if area.abs() < 1e-12 {
            return;
        }
        // Ensure CCW ordering so outward normals are correct.
        let ring: Vec<Vec2> = if area < 0.0 { ring.iter().rev().cloned().collect() } else { ring.to_vec() };
        let m = ring.len() as u32;

        let z0f = z0 as f32;
        let z1f = z1 as f32;
        let b_base = self.positions.len() as u32;

        // ── Cap vertices ──────────────────────────────────────────────
        // Bottom ring (normal: 0, -1, 0)
        for &p in &ring {
            self.positions.push([p.x as f32, p.y as f32, z0f]);
            self.uvs.push([p.x as f32, z0f]);
            self.normals.push([0.0, -1.0, 0.0]);
        }
        // Top ring (normal: 0, 1, 0)
        for &p in &ring {
            self.positions.push([p.x as f32, p.y as f32, z1f]);
            self.uvs.push([p.x as f32, z1f]);
            self.normals.push([0.0, 1.0, 0.0]);
        }

        // ── Side vertices (duplicated for correct per-face normals) ───
        let s_base = self.positions.len() as u32;
        // Bottom side ring
        for i in 0..m {
            let (nx, ny) = outward_normal(&ring[(i % m) as usize], &ring[((i + 1) % m) as usize]);
            for _ in 0..2 {
                let p = ring[(i % m) as usize];
                self.positions.push([p.x as f32, p.y as f32, z0f]);
                self.uvs.push([p.x as f32, z0f]);
                self.normals.push([nx, ny, 0.0]);
            }
        }
        // Top side ring
        for i in 0..m {
            let (nx, ny) = outward_normal(&ring[(i % m) as usize], &ring[((i + 1) % m) as usize]);
            for _ in 0..2 {
                let p = ring[(i % m) as usize];
                self.positions.push([p.x as f32, p.y as f32, z1f]);
                self.uvs.push([p.x as f32, z1f]);
                self.normals.push([nx, ny, 0.0]);
            }
        }
        let sb_base = b_base + m; // bottom cap base
        let st_base = sb_base + m; // top cap base
        let ssb_base = st_base + m; // side bottom base
        let sst_base = ssb_base + m; // side top base

        // ── Bottom cap triangles ──────────────────────────────────────
        for tri in triangulate(&ring) {
            let a = (sb_base + tri[0]) as u32;
            let b = (sb_base + tri[1]) as u32;
            let c = (sb_base + tri[2]) as u32;
            self.indices.extend([a, b, c]);
        }

        // ── Top cap triangles ─────────────────────────────────────────
        for tri in triangulate(&ring) {
            let a = (st_base + tri[0]) as u32;
            let b = (st_base + tri[1]) as u32;
            let c = (st_base + tri[2]) as u32;
            self.indices.extend([a, b, c]);
        }

        // ── Side triangles ────────────────────────────────────────────
        for i in 0..m {
            let j = (i + 1) % m;
            // Side bottom: two triangles per edge (CCW outward)
            self.indices.extend([ssb_base + i * 2, ssb_base + j * 2, sst_base + i * 2]);
            self.indices.extend([ssb_base + j * 2, sst_base + i * 2, sst_base + j * 2]);
        }
    }
}

/// Outward unit normal for the edge from `a` to `b` on a CCW polygon.
fn outward_normal(a: &Vec2, b: &Vec2) -> (f32, f32) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    // For a CCW polygon, the outward normal of edge a→b is (dy, -dx).
    let nx = dy as f32;
    let ny = -(dx as f32);
    let len = (nx * nx + ny * ny).sqrt();
    if len < 1e-15 { (0.0, 0.0) } else { (nx / len, ny / len) }
}

/// Signed area of a polygon (shoelace formula).
///
/// Positive for CCW, negative for CW.
fn polygon_area(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += ring[i].x * ring[j].y - ring[i].y * ring[j].x;
    }
    area / 2.0
}

/// Ear-clipping triangulation of a planar polygon ring (CCW expected).
///
/// Returns a list of triangle index triples referencing vertices of `ring`.
/// Returns an empty vector if the polygon cannot be triangulated (degenerate,
/// self-intersecting, or non-finite inputs).
fn triangulate(ring: &[Vec2]) -> Vec<[u32; 3]> {
    let n = ring.len();
    if n < 3 {
        return Vec::new();
    }
    // Reject non-finite vertices early.
    for p in ring {
        if !p.is_finite() {
            return Vec::new();
        }
    }
    let mut tri: Vec<[u32; 3]> = Vec::new();
    let mut poly: Vec<u32> = (0..n as u32).collect();
    let max_iters = (n * 3) as u32;
    let mut iters: u32 = 0;

    while poly.len() >= 3 && iters < max_iters {
        iters += 1;
        let m = poly.len();
        let mut removed = false;
        for i in 0..m {
            let j_idx = (i + 1) % m;
            let k_idx = (i + 2) % m;
            let pi = poly[i];
            let pj = poly[j_idx];
            let pk = poly[k_idx];

            // Cross product (b - a) x (c - a): positive = CCW ear.
            let cx = ring[pj as usize].x - ring[pi as usize].x;
            let cy = ring[pj as usize].y - ring[pi as usize].y;
            let dx = ring[pk as usize].x - ring[pi as usize].x;
            let dy = ring[pk as usize].y - ring[pi as usize].y;
            let cross = cx * dy - cy * dx;
            if cross < 1e-15 {
                continue;
            }
            // No other vertex may lie strictly inside the ear triangle.
            let mut inside = false;
            for l in 0..m {
                if l == i || l == j_idx || l == k_idx {
                    continue;
                }
                if point_in_triangle(&ring[poly[l] as usize], &ring[pi as usize], &ring[pj as usize], &ring[pk as usize]) {
                    inside = true;
                    break;
                }
            }
            if inside {
                continue;
            }
            // Ear (pi, pj, pk) is valid; remove the middle vertex pj.
            tri.push([pi, pj, pk]);
            poly.remove(j_idx);
            removed = true;
            break;
        }
        if !removed {
            break; // no ear found → non-triangulable
        }
    }
    tri
}

/// Barycentric point-in-triangle test (strictly interior, with tolerance).
fn point_in_triangle(p: &Vec2, a: &Vec2, b: &Vec2, c: &Vec2) -> bool {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let ac_x = c.x - a.x;
    let ac_y = c.y - a.y;
    let ap_x = p.x - a.x;
    let ap_y = p.y - a.y;

    let denom = ab_x * ac_y - ab_y * ac_x;
    if denom.abs() < 1e-15 {
        return false;
    }
    let u = (ac_x * ap_y - ac_y * ap_x) / denom;
    let v = (ab_x * ap_y - ab_y * ap_x) / denom;
    u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9
}
