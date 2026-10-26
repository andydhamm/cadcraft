//! 3D mesh primitives: vertex data, triangulation, manifold checks, and prism building.
//!
//! This module is intentionally free of any `render` dependency; the ear-clipping
//! triangulator here is self-contained and used by the arch3d builder.

use std::collections::HashMap;

use cadcraft_doc::ArchId;
use cadcraft_geom::Vec2;

/// Axis-aligned bounding box in 3D world space.
///
/// Build it with [`Bounds3::new`], [`Bounds3::empty`] or `..Default::default()`, not an
/// exhaustive literal (see "Shared structs" in `docs/arch3d/DESIGN.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bounds3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds3 {
    /// A box with the given corners (used as given; not reordered or validated).
    pub const fn new(min: [f64; 3], max: [f64; 3]) -> Self {
        Bounds3 { min, max }
    }

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
///
/// Start from [`Mesh::new`] (material and owning object are the required fields) and set the
/// vertex data with `Mesh { positions, indices, ..Mesh::new(material, object) }`.
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
    /// The level elevation this mesh belongs to; `f64::NAN` if unknown.
    pub level_elevation: f64,
}

impl Mesh {
    /// Create an empty mesh.
    pub fn new(material: &str, object: ArchId) -> Self {
        Mesh {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            material: material.to_string(),
            object,
            level_elevation: f64::NAN,
        }
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
    /// edge are included. Side faces share vertices with the caps, so the
    /// resulting mesh is a closed 2-manifold.
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

        // Bottom cap vertices (outward normal: -Z).
        for &p in &ring {
            self.positions.push([p.x as f32, p.y as f32, z0f]);
            self.uvs.push([p.x as f32, z0f]);
            self.normals.push([0.0, 0.0, -1.0]);
        }
        // Top cap vertices (outward normal: +Z).
        for &p in &ring {
            self.positions.push([p.x as f32, p.y as f32, z1f]);
            self.uvs.push([p.x as f32, z1f]);
            self.normals.push([0.0, 0.0, 1.0]);
        }

        // Triangulate the ring once; both caps share the same triangulation.
        let tris = triangulate(&ring);
        // Bottom cap triangles: CW winding from above (reversed from the
        // CCW triangulation), giving outward -Z normals.
        for tri in &tris {
            self.indices.extend([(b_base + tri[0]) as u32, (b_base + tri[2]) as u32, (b_base + tri[1]) as u32]);
        }

        // Top cap triangles: CCW winding from above, giving +Z normals.
        for tri in &tris {
            self.indices.extend([(b_base + m + tri[0]) as u32, (b_base + m + tri[1]) as u32, (b_base + m + tri[2]) as u32]);
        }

        // Side faces: each ring edge becomes a quad split into two triangles.
        // Vertices are shared with the cap rings (no duplication), so the
        // mesh is a closed 2-manifold.
        for i in 0..m {
            let j = (i + 1) % m;
            // Triangle: bottom_i, bottom_j, top_j
            self.indices.extend([b_base + i, b_base + j, b_base + m + j]);
            // Triangle: bottom_i, top_j, top_i
            self.indices.extend([b_base + i, b_base + m + j, b_base + m + i]);
        }
    }

    /// Extrude `outer` (with `holes` cut out) into a closed 2-manifold prism.
    ///
    /// The outer ring and each hole ring are validated and normalised; holes
    /// are bridged one at a time into a single running ring, then the merged
    /// ring is extruded exactly like [`Mesh::push_prism`]. The two bridge
    /// edges that join each hole to the outer ring are skipped in the side
    /// loop (detected as repeated undirected edges) so the resulting mesh stays
    /// a closed 2-manifold. If a hole cannot be bridged it is dropped.
    pub fn push_prism_with_holes(&mut self, outer: &[Vec2], holes: &[Vec<Vec2>], z0: f64, z1: f64) {
        // Filter holes: >=3 finite vertices and non-degenerate area, normalised to CW.
        let filtered_holes: Vec<Vec<Vec2>> = holes
            .iter()
            .filter_map(|h| {
                if h.len() < 3 {
                    return None;
                }
                if !h.iter().all(|p| p.is_finite()) {
                    return None;
                }
                let a = polygon_area(h);
                if a.abs() < 1e-12 {
                    return None;
                }
                Some(if a > 0.0 { h.iter().rev().cloned().collect() } else { h.to_vec() })
            })
            .collect();

        // Empty-hole path is byte-identical to push_prism (including its no-ops).
        if filtered_holes.is_empty() {
            self.push_prism(outer, z0, z1);
            return;
        }

        // Validate and normalise the outer ring to CCW.
        if outer.len() < 3 {
            return;
        }
        if !outer.iter().all(|p| p.is_finite()) {
            return;
        }
        let outer_area = polygon_area(outer);
        if outer_area.abs() < 1e-12 {
            return;
        }
        let ring_ccw = if outer_area < 0.0 { outer.iter().rev().cloned().collect::<Vec<Vec2>>() } else { outer.to_vec() };

        // Bridge each hole into the running ring, dropping any that can't bridge.
        let mut ring = ring_ccw;
        for hole in &filtered_holes {
            if let Some((p, q)) = find_bridge_pair(&ring, hole) {
                ring = merge_ring(ring, p, hole, q);
            }
        }

        // If no holes were bridged, the result is identical to push_prism.
        if ring.len() == outer.len() {
            self.push_prism(outer, z0, z1);
            return;
        }

        // Triangulate the merged ring once; both caps share it.
        let tris = triangulate_simple(&ring);
        if tris.is_empty() {
            return;
        }
        let m = ring.len() as u32;
        let z0f = z0 as f32;
        let z1f = z1 as f32;
        let b_base = self.positions.len() as u32;

        // Bottom cap vertices (outward normal: -Z).
        for &p in &ring {
            self.positions.push([p.x as f32, p.y as f32, z0f]);
            self.uvs.push([p.x as f32, z0f]);
            self.normals.push([0.0, 0.0, -1.0]);
        }
        // Top cap vertices (outward normal: +Z).
        for &p in &ring {
            self.positions.push([p.x as f32, p.y as f32, z1f]);
            self.uvs.push([p.x as f32, z1f]);
            self.normals.push([0.0, 0.0, 1.0]);
        }

        // Bottom cap triangles: reversed winding gives outward -Z normals.
        for tri in &tris {
            self.indices.extend([(b_base + tri[0]) as u32, (b_base + tri[2]) as u32, (b_base + tri[1]) as u32]);
        }
        // Top cap triangles: forward winding gives +Z normals.
        for tri in &tris {
            self.indices.extend([(b_base + m + tri[0]) as u32, (b_base + m + tri[1]) as u32, (b_base + m + tri[2]) as u32]);
        }

        // Detect bridge edges by position: an edge (ring[i], ring[j]) is a bridge
        // if another edge (ring[k], ring[l]) has the same endpoints. Both
        // occurrences are skipped in the side loop.
        let mut is_bridge = vec![false; m as usize];
        for i in 0..m {
            if is_bridge[i as usize] {
                continue;
            }
            let j = (i + 1) % m;
            for k in (i + 1)..m {
                if is_bridge[k as usize] {
                    continue;
                }
                let l = (k + 1) % m;
                if l == i {
                    continue;
                }
                if ring[i as usize].x == ring[k as usize].x
                    && ring[i as usize].y == ring[k as usize].y
                    && ring[j as usize].x == ring[l as usize].x
                    && ring[j as usize].y == ring[l as usize].y
                {
                    is_bridge[i as usize] = true;
                    is_bridge[k as usize] = true;
                }
            }
        }

        // Side faces, skipping bridge edges; vertices are shared with the caps
        // so the mesh is a closed 2-manifold.
        for i in 0..m {
            if is_bridge[i as usize] {
                continue;
            }
            let j = (i + 1) % m;
            // Triangle: bottom_i, bottom_j, top_j
            self.indices.extend([b_base + i, b_base + j, b_base + m + j]);
            // Triangle: bottom_i, top_j, top_i
            self.indices.extend([b_base + i, b_base + m + j, b_base + m + i]);
        }
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new("", ArchId(0))
    }
}

/// Find the `(ring_index, hole_index)` pair that bridges `hole` into `ring`.
///
/// Returns `None` if no valid pair exists. A valid pair's open segment
/// `ring[p]..hole[q]` must cross no ring edge (except the edges incident to `p`)
/// or hole edge (except those incident to `q`), and its midpoint must lie
/// inside `ring` (even-odd test).
fn find_bridge_pair(ring: &[Vec2], hole: &[Vec2]) -> Option<(u32, u32)> {
    let m = ring.len() as u32;
    let h = hole.len() as u32;
    if m < 3 || h < 3 {
        return None;
    }
    let mut best: (f64, u32, u32) = (f64::INFINITY, 0, 0);
    for p in 0..m {
        for q in 0..h {
            let pr = ring[p as usize];
            let hq = hole[q as usize];
            let dist = (pr.x - hq.x).powi(2) + (pr.y - hq.y).powi(2);
            if dist > best.0 {
                continue;
            }
            // The bridge segment must not cross any ring edge except the two
            // edges incident to p, and no hole edge except the two incident to q.
            let ring_ok = (0..m).all(|i| {
                let a = i;
                let b = (i + 1) % m;
                if a == p && b == (p + 1) % m {
                    return true;
                }
                if b == p && a == (p + m - 1) % m {
                    return true;
                }
                !segs_properly_intersect(ring[a as usize], ring[b as usize], pr, hq)
            });
            if !ring_ok {
                continue;
            }
            let hole_ok = (0..h).all(|i| {
                let a = i;
                let b = (i + 1) % h;
                if a == q && b == (q + 1) % h {
                    return true;
                }
                if b == q && a == (q + h - 1) % h {
                    return true;
                }
                !segs_properly_intersect(hole[a as usize], hole[b as usize], pr, hq)
            });
            if !hole_ok {
                continue;
            }
            let mid = Vec2::new((pr.x + hq.x) / 2.0, (pr.y + hq.y) / 2.0);
            if !point_in_ring_even_odd(mid, ring) {
                continue;
            }
            if dist < best.0 {
                best = (dist, p, q);
            }
        }
    }
    if best.0.is_finite() { Some((best.1, best.2)) } else { None }
}

/// Splice `hole` into `ring` at the bridging pair `(p, q)`:
///
/// `merged = ring[0..=p] ++ hole[q..] ++ hole[0..=q] ++ ring[p..]`
///
/// `ring[p]` and `hole[q]` each appear twice in the result, which creates the
/// two bridge edges that `push_prism_with_holes` skips in the side loop.
fn merge_ring(ring: Vec<Vec2>, p: u32, hole: &[Vec2], q: u32) -> Vec<Vec2> {
    let mut merged = Vec::with_capacity(ring.len() + hole.len() + 2);
    merged.extend(&ring[..=p as usize]);
    merged.extend(&hole[q as usize..]);
    merged.extend(&hole[..=q as usize]);
    merged.extend(&ring[p as usize..]);
    merged
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

/// True if open segments a-b and c-d properly cross (not merely touch).
fn segs_properly_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    if !a.is_finite() || !b.is_finite() || !c.is_finite() || !d.is_finite() {
        return false;
    }
    let eps = 1e-12;
    let d1 = (b - a).cross(c - a);
    let d2 = (b - a).cross(d - a);
    let d3 = (d - c).cross(a - c);
    let d4 = (d - c).cross(b - c);
    (d1 > eps && d2 < -eps || d1 < -eps && d2 > eps) && (d3 > eps && d4 < -eps || d3 < -eps && d4 > eps)
}

/// Even-odd (ray cast, +x ray) inside test for polygon `ring`.
fn point_in_ring_even_odd(p: Vec2, ring: &[Vec2]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let xi = ring[i].x;
        let yi = ring[i].y;
        let xj = ring[(i + 1) % n].x;
        let yj = ring[(i + 1) % n].y;
        if (yi > p.y) != (yj > p.y) {
            let xint = xi + (p.y - yi) * (xj - xi) / (yj - yi);
            if p.x < xint - 1e-12 {
                inside = !inside;
            }
        }
    }
    inside
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

/// Ear clipping that tolerates duplicate *positions* in `ring` (a bridged
/// ring repeats the two bridge endpoints). Returns empty on <3 finite verts.
fn triangulate_simple(ring: &[Vec2]) -> Vec<[u32; 3]> {
    let n = ring.len();
    if n < 3 {
        return Vec::new();
    }
    for p in ring {
        if !p.is_finite() {
            return Vec::new();
        }
    }
    let mut tri: Vec<[u32; 3]> = Vec::new();
    let mut poly: Vec<u32> = (0..n as u32).collect();
    let max_iters = (n * 3) as u32;
    let mut iters: u32 = 0;

    fn near(a: &Vec2, b: &Vec2) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    while poly.len() >= 3 && iters < max_iters {
        iters += 1;
        let m = poly.len();
        let mut removed = false;

        // First pass: strict threshold.
        for i in 0..m {
            let j_idx = (i + 1) % m;
            let k_idx = (i + 2) % m;
            let pi = poly[i];
            let pj = poly[j_idx];
            let pk = poly[k_idx];

            let cx = ring[pj as usize].x - ring[pi as usize].x;
            let cy = ring[pj as usize].y - ring[pi as usize].y;
            let dx = ring[pk as usize].x - ring[pi as usize].x;
            let dy = ring[pk as usize].y - ring[pi as usize].y;
            let cross = cx * dy - cy * dx;
            if cross < 1e-15 {
                continue;
            }
            let mut inside = false;
            for l in 0..m {
                if l == i || l == j_idx || l == k_idx {
                    continue;
                }
                let pl = &ring[poly[l] as usize];
                if near(pl, &ring[pi as usize]) || near(pl, &ring[pj as usize]) || near(pl, &ring[pk as usize]) {
                    continue;
                }
                if point_in_triangle(pl, &ring[pi as usize], &ring[pj as usize], &ring[pk as usize]) {
                    inside = true;
                    break;
                }
            }
            if inside {
                continue;
            }
            tri.push([pi, pj, pk]);
            poly.remove(j_idx);
            removed = true;
            break;
        }

        // Second pass (only if the first found nothing): relaxed threshold.
        if !removed {
            for i in 0..m {
                let j_idx = (i + 1) % m;
                let k_idx = (i + 2) % m;
                let pi = poly[i];
                let pj = poly[j_idx];
                let pk = poly[k_idx];

                let cx = ring[pj as usize].x - ring[pi as usize].x;
                let cy = ring[pj as usize].y - ring[pi as usize].y;
                let dx = ring[pk as usize].x - ring[pi as usize].x;
                let dy = ring[pk as usize].y - ring[pi as usize].y;
                let cross = cx * dy - cy * dx;
                if cross < -1e-12 {
                    continue;
                }
                let mut inside = false;
                for l in 0..m {
                    if l == i || l == j_idx || l == k_idx {
                        continue;
                    }
                    let pl = &ring[poly[l] as usize];
                    if near(pl, &ring[pi as usize]) || near(pl, &ring[pj as usize]) || near(pl, &ring[pk as usize]) {
                        continue;
                    }
                    if point_in_triangle(pl, &ring[pi as usize], &ring[pj as usize], &ring[pk as usize]) {
                        inside = true;
                        break;
                    }
                }
                if inside {
                    continue;
                }
                tri.push([pi, pj, pk]);
                poly.remove(j_idx);
                removed = true;
                break;
            }
        }

        if !removed {
            break;
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
    let u = (ap_x * ac_y - ap_y * ac_x) / denom;
    let v = (ab_x * ap_y - ab_y * ap_x) / denom;
    u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9
}

/// A square ring (3,3)-(5,3)-(5,5)-(3,5), CCW.
fn square_ring() -> Vec<Vec2> {
    vec![Vec2::new(3.0, 3.0), Vec2::new(5.0, 3.0), Vec2::new(5.0, 5.0), Vec2::new(3.0, 5.0)]
}

/// A concave 5-gon with a reflex vertex at (1.5, 1.5).
fn concave_ring() -> Vec<Vec2> {
    vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(1.5, 1.5), Vec2::new(0.0, 4.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    use cadcraft_arch::join::WallShape;

    /// `triangulate` must split a concave (non-convex) 5-gon into 3 triangles.
    #[test]
    fn triangulate_handles_a_concave_ring() {
        let ring = concave_ring();
        let tri = triangulate(&ring);
        assert_eq!(tri.len(), 3, "expected 3 triangles for a 5-gon; got {tri:?}");
    }

    /// A rectangular wall prism must be a closed 2-manifold.
    #[test]
    fn wall_mesh_is_manifold() {
        let ring = square_ring();
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism(&ring, 0.0, 2.5);
        assert!(mesh.is_manifold(), "wall prism must be a closed manifold");
        assert!(mesh.triangle_count() > 0);
    }

    /// Every face normal must point outward from the prism interior.
    /// Verified by checking the dot product of each vertex normal with the
    /// vector from the box centroid to that vertex is non-negative.
    #[test]
    fn normals_face_outwards() {
        let ring = square_ring();
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism(&ring, 0.0, 2.5);

        let (cx, cy, cz) = (4.0f32, 4.0f32, 1.25f32);
        for (i, p) in mesh.positions.iter().enumerate() {
            let [x, y, z] = p;
            let [nx, ny, nz] = mesh.normals[i];
            let dot = (x - cx) * nx + (y - cy) * ny + (z - cz) * nz;
            assert!(nx.is_finite() && ny.is_finite() && nz.is_finite());
            assert!(dot > -1e-6, "normal at vertex {i} points inward: pos=({x}, {y}, {z}) normal=({nx}, {ny}, {nz}) dot={dot}");
        }
    }

    /// The 3D bounds must match the plan envelope (3–5 in x/y) and the height (0–h in z).
    #[test]
    fn bounds_match_the_plan_envelope_and_height() {
        let ring = square_ring();
        let h = 2.5;
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism(&ring, 0.0, h);
        let b = mesh.bounds();
        assert!(b.min[0] <= 3.0 + 1e-9 && b.min[1] <= 3.0 + 1e-9);
        assert!(b.max[0] >= 5.0 - 1e-9 && b.max[1] >= 5.0 - 1e-9);
        assert!(b.min[2] >= -1e-9 && b.max[2] <= h + 1e-9);
    }

    /// A degenerate ring (fewer than 3 vertices) yields `None`.
    #[test]
    fn degenerate_wall_produces_no_mesh() {
        let degenerate = WallShape::new(ArchId(1), vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)], (Vec2::ZERO, Vec2::ZERO));
        let m = crate::walls::extrude_wall(&degenerate, 2.5, 0.0, "wall");
        assert!(m.is_none(), "degenerate (2-vertex) wall must produce no mesh");
    }

    /// Properly-crossing segments return true; touching, collinear, or parallel
    /// segments return false.
    #[test]
    fn segs_properly_intersect_detects_crossing() {
        // (0,0)-(1,1) vs (0,1)-(1,0) properly cross.
        assert!(segs_properly_intersect(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0),));
        // Collinear overlapping (0,0)-(2,0) vs (1,0)-(3,0) does not properly cross.
        assert!(!segs_properly_intersect(Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(3.0, 0.0),));
        // Touching endpoint (0,0)-(1,0) vs (1,0)-(1,1) does not properly cross.
        assert!(!segs_properly_intersect(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0),));
        // Non-crossing (0,0)-(1,0) vs (0,1)-(1,1) does not properly cross.
        assert!(!segs_properly_intersect(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0),));
    }

    /// Even-odd ray-cast: a point inside the square is true; outside is false.
    #[test]
    fn point_in_ring_even_odd_inside_and_outside() {
        let ring = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        assert!(point_in_ring_even_odd(Vec2::new(1.0, 1.0), &ring));
        assert!(!point_in_ring_even_odd(Vec2::new(5.0, 5.0), &ring));
        assert!(!point_in_ring_even_odd(Vec2::new(-1.0, 2.0), &ring));
    }

    /// `triangulate_simple` on simple rings matches `triangulate` in triangle count
    /// and every returned index triple is in range with distinct entries.
    #[test]
    fn triangulate_simple_matches_triangulate_on_simple_rings() {
        let square = square_ring();
        let tri_simple = triangulate_simple(&square);
        let tri = triangulate(&square);
        assert_eq!(tri_simple.len(), 2, "square should triangulate to 2 triangles; got {tri_simple:?}");
        assert_eq!(tri_simple.len(), tri.len());
        for t in &tri_simple {
            assert!(t[0] < square.len() as u32);
            assert!(t[1] < square.len() as u32);
            assert!(t[2] < square.len() as u32);
            assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        }

        let concave = concave_ring();
        let tri_simple = triangulate_simple(&concave);
        let tri = triangulate(&concave);
        assert_eq!(tri_simple.len(), 3, "concave 5-gon should triangulate to 3 triangles; got {tri_simple:?}");
        assert_eq!(tri_simple.len(), tri.len());
        for t in &tri_simple {
            assert!(t[0] < concave.len() as u32);
            assert!(t[1] < concave.len() as u32);
            assert!(t[2] < concave.len() as u32);
            assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        }
    }

    /// With a degenerate outer (2 vertices) or a degenerate hole the output is
    /// a no-op: a degenerate outer yields no geometry at all, and a square
    /// outer with only a degenerate hole is byte-identical to `push_prism`.
    #[test]
    fn push_prism_with_holes_noop_on_degenerate_outer() {
        // Degenerate outer (2 vertices) must produce no geometry, even with a
        // valid hole present.
        let mut mesh = Mesh::new("wall", ArchId(1));
        let outer = &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)];
        let holes = &[vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(3.0, 1.5)]];
        mesh.push_prism_with_holes(outer, holes, 0.0, 2.5);
        assert!(mesh.positions.is_empty(), "degenerate outer must yield no positions");
        assert!(mesh.normals.is_empty(), "degenerate outer must yield no normals");
        assert!(mesh.indices.is_empty(), "degenerate outer must yield no indices");

        // A square outer with a degenerate hole (2 vertices) is identical to
        // `push_prism` on the same square.
        let ring = square_ring();
        let degenerate_hole = vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0)];
        let mut a = Mesh::new("wall", ArchId(1));
        a.push_prism_with_holes(&ring, &[degenerate_hole.clone()], 0.0, 2.5);
        let mut b = Mesh::new("wall", ArchId(1));
        b.push_prism(&ring, 0.0, 2.5);
        assert_eq!(a.positions, b.positions, "with holes (empty after filter) must equal push_prism: positions");
        assert_eq!(a.normals, b.normals, "with holes (empty after filter) must equal push_prism: normals");
        assert_eq!(a.indices, b.indices, "with holes (empty after filter) must equal push_prism: indices");
    }

    /// A ring with a duplicated vertex (square with first vertex repeated at
    /// the end) still produces valid triangulation with correct total area.
    #[test]
    fn triangulate_simple_handles_duplicate_ring_vertices() {
        let ring = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0), Vec2::new(0.0, 0.0)];
        let tri = triangulate_simple(&ring);
        assert!(tri.len() >= 2, "expected at least 2 triangles, got {tri:?}");

        let mut total_area = 0.0;
        for t in &tri {
            let ax = ring[t[0] as usize].x;
            let ay = ring[t[0] as usize].y;
            let bx = ring[t[1] as usize].x;
            let by = ring[t[1] as usize].y;
            let cx = ring[t[2] as usize].x;
            let cy = ring[t[2] as usize].y;
            let signed = (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by)) / 2.0;
            total_area += signed.abs();
        }
        assert!((total_area - 16.0).abs() < 1e-6, "sum of triangle areas {total_area} not within 1e-6 of 16.0");
    }

    /// Sum of |signed area| of triangles whose first vertex has normal [0,0,1].
    fn top_cap_area(mesh: &Mesh) -> f64 {
        let mut area = 0.0;
        for tri in mesh.indices.chunks(3) {
            if tri.len() != 3 {
                continue;
            }
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            if mesh.normals[a as usize] == [0.0, 0.0, 1.0] {
                let (x1, y1) = (mesh.positions[a as usize][0] as f64, mesh.positions[a as usize][1] as f64);
                let (x2, y2) = (mesh.positions[b as usize][0] as f64, mesh.positions[b as usize][1] as f64);
                let (x3, y3) = (mesh.positions[c as usize][0] as f64, mesh.positions[c as usize][1] as f64);
                area += ((x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2.0).abs();
            }
        }
        area
    }

    #[test]
    fn prism_with_holes_without_holes_matches_push_prism() {
        let r = square_ring();
        let mut a = Mesh::new("wall", ArchId(1));
        a.push_prism_with_holes(&r, &[], 0.0, 2.5);
        let mut b = Mesh::new("wall", ArchId(1));
        b.push_prism(&r, 0.0, 2.5);
        assert_eq!(a.positions, b.positions, "positions differ");
        assert_eq!(a.normals, b.normals, "normals differ");
        assert_eq!(a.indices, b.indices, "indices differ");
    }

    #[test]
    fn prism_with_hole_cap_area() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let hole = vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(1.0, 2.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 2.5);
        let area = top_cap_area(&mesh);
        assert!((area - 15.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 15.0");
        let b = mesh.bounds();
        assert!((b.min[0] - 0.0).abs() < 1e-6, "bounds min x");
        assert!((b.max[0] - 4.0).abs() < 1e-6, "bounds max x");
        assert!((b.min[1] - 0.0).abs() < 1e-6, "bounds min y");
        assert!((b.max[1] - 4.0).abs() < 1e-6, "bounds max y");
        assert!((b.min[2] - 0.0).abs() < 1e-6, "bounds min z");
        assert!((b.max[2] - 2.5).abs() < 1e-6, "bounds max z");
    }

    #[test]
    fn prism_with_hole_is_manifold() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let hole = vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(1.0, 2.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 2.5);
        assert!(mesh.is_manifold(), "prism with hole must be a closed manifold");
        assert!(mesh.triangle_count() > 0, "prism with hole must have triangles");
    }

    #[test]
    fn prism_with_two_holes_cap_area() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(6.0, 6.0), Vec2::new(0.0, 6.0)];
        let holes = vec![
            vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(1.0, 2.0)],
            vec![Vec2::new(4.0, 4.0), Vec2::new(5.0, 4.0), Vec2::new(5.0, 5.0), Vec2::new(4.0, 5.0)],
        ];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &holes, 0.0, 2.5);
        let area = top_cap_area(&mesh);
        assert!((area - 34.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 34.0");
        assert!(mesh.is_manifold(), "prism with two holes must be a closed manifold");
    }

    #[test]
    fn mesh_new_sets_level_elevation_to_nan() {
        let m = Mesh::new("wall", ArchId(1));
        assert!(m.level_elevation.is_nan(), "Mesh::new must set level_elevation to f64::NAN");
    }

    #[test]
    fn prism_with_hole_outside_the_outer_is_ignored() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let hole = vec![Vec2::new(20.0, 20.0), Vec2::new(21.0, 20.0), Vec2::new(21.0, 21.0), Vec2::new(20.0, 21.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 2.5);
        let area = top_cap_area(&mesh);
        assert!((area - 16.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 16.0");
        let mut ref_mesh = Mesh::new("wall", ArchId(1));
        ref_mesh.push_prism(&outer, 0.0, 2.5);
        assert_eq!(top_cap_area(&mesh), top_cap_area(&ref_mesh), "outside hole must produce same cap area as push_prism");
    }

    /// NaN in any outer vertex produces no geometry.
    #[test]
    fn prism_with_holes_nan_vertex_produces_no_geometry() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(f64::NAN, 4.0), Vec2::new(0.0, 4.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[], 0.0, 2.5);
        assert!(mesh.positions.is_empty(), "NaN vertex must yield no positions");
        assert!(mesh.indices.is_empty(), "NaN vertex must yield no indices");
    }

    /// Inf in any outer vertex produces no geometry.
    #[test]
    fn prism_with_holes_inf_vertex_produces_no_geometry() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(f64::INFINITY, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[], 0.0, 2.5);
        assert!(mesh.positions.is_empty(), "Inf vertex must yield no positions");
        assert!(mesh.indices.is_empty(), "Inf vertex must yield no indices");
    }

    /// Collinear points in the outer ring are handled correctly.
    #[test]
    fn prism_with_holes_collinear_outer_points() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[], 0.0, 2.5);
        assert!(mesh.triangle_count() > 0, "collinear points must still produce geometry");
        let area = top_cap_area(&mesh);
        assert!((area - 16.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 16.0");
    }

    /// A triangular hole works correctly.
    #[test]
    fn prism_with_holes_triangle_hole() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let hole = vec![Vec2::new(1.0, 1.0), Vec2::new(3.0, 1.0), Vec2::new(2.0, 3.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 2.5);
        let area = top_cap_area(&mesh);
        assert!((area - 14.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 14.0");
        assert!(mesh.is_manifold(), "prism with triangle hole must be a closed manifold");
    }

    /// A concave outer ring with a hole works correctly.
    #[test]
    fn prism_with_holes_concave_outer() {
        let outer = concave_ring();
        let hole = vec![Vec2::new(0.5, 0.5), Vec2::new(1.5, 0.5), Vec2::new(1.0, 1.5)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 2.5);
        assert!(mesh.triangle_count() > 0, "concave outer with hole must produce geometry");
        assert!(mesh.is_manifold(), "concave outer with hole must be a closed manifold");
    }

    /// A zero-height prism (z0 == z1) still produces a valid manifold.
    #[test]
    fn prism_with_holes_zero_height() {
        let outer = square_ring();
        let hole = vec![Vec2::new(3.5, 3.5), Vec2::new(4.5, 3.5), Vec2::new(4.5, 4.5), Vec2::new(3.5, 4.5)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 0.0);
        assert!(mesh.triangle_count() > 0, "zero-height prism must produce geometry");
        assert!(mesh.is_manifold(), "zero-height prism must be a closed manifold");
    }

    /// An inverted prism (z0 > z1) still works.
    #[test]
    fn prism_with_holes_inverted_height() {
        let outer = square_ring();
        let hole = vec![Vec2::new(3.5, 3.5), Vec2::new(4.5, 3.5), Vec2::new(4.5, 4.5), Vec2::new(3.5, 4.5)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 2.5, 0.0);
        assert!(mesh.triangle_count() > 0, "inverted prism must produce geometry");
        assert!(mesh.is_manifold(), "inverted prism must be a closed manifold");
        let area = top_cap_area(&mesh);
        assert!((area - 3.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 3.0");
    }

    /// A hole touching the outer boundary (sharing a vertex) is handled.
    #[test]
    fn prism_with_holes_hole_touching_outer() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let hole = vec![Vec2::new(0.0, 4.0), Vec2::new(1.0, 4.0), Vec2::new(1.0, 3.0), Vec2::new(0.0, 3.0)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, 2.5);
        assert!(mesh.triangle_count() > 0, "touching hole must produce geometry");
        let area = top_cap_area(&mesh);
        assert!((area - 15.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 15.0");
    }

    /// Many small holes are handled correctly.
    #[test]
    fn prism_with_holes_many_holes() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let mut holes = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                let x = 1.0 + i as f64 * 3.0;
                let y = 1.0 + j as f64 * 3.0;
                holes.push(vec![Vec2::new(x, y), Vec2::new(x + 1.0, y), Vec2::new(x + 1.0, y + 1.0), Vec2::new(x, y + 1.0)]);
            }
        }
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &holes, 0.0, 2.5);
        assert!(mesh.triangle_count() > 0, "many holes must produce geometry");
        let area = top_cap_area(&mesh);
        assert!((area - 91.0).abs() < 1e-6, "cap area {area} not within 1e-6 of 91.0");
        assert!(mesh.is_manifold(), "many holes must be a closed manifold");
    }

    /// Huge coordinate values are handled without overflow.
    #[test]
    fn prism_with_holes_huge_coordinates() {
        let s = 1e6;
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(s, 0.0), Vec2::new(s, s), Vec2::new(0.0, s)];
        let hole = vec![Vec2::new(s * 0.4, s * 0.4), Vec2::new(s * 0.6, s * 0.4), Vec2::new(s * 0.6, s * 0.6), Vec2::new(s * 0.4, s * 0.6)];
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.push_prism_with_holes(&outer, &[hole], 0.0, s * 0.01);
        assert!(mesh.triangle_count() > 0, "huge coordinates must produce geometry");
        let area = top_cap_area(&mesh);
        let expected = 0.96 * s * s;
        assert!((area - expected).abs() < 1e-6 * expected, "cap area {area} not within 1e-6 of {expected}");
    }
}
