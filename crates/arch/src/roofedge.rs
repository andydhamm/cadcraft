use crate::roof::{RoofError, RoofGeom};
use cadcraft_doc::{Roof, RoofEdge, RoofEdgeKind};
use cadcraft_geom::Vec2;

/// Classify skeleton arcs into roof edge kinds and append footprint eaves.
///
/// This function operates on the generated `RoofGeom`. It iterates through the skeleton arcs
/// to determine if they represent ridges, hips, valleys, or gable ends based on the adjacent
/// faces' slopes and geometry. It then appends the outer footprint edges as `Eave` edges.
pub fn roof_edges_geom(g: &RoofGeom) -> Vec<RoofEdge> {
    let mut edges = Vec::new();
    let n_rings = g.skel.rings.first().map(|r| r.len()).unwrap_or(0);

    for &(a, b) in &g.skel.arcs {
        if a == b {
            continue;
        }

        // Find adjacent faces
        let mut adjacent = Vec::new();
        for (f_idx, face) in g.skel.faces.iter().enumerate() {
            let nodes = &face.nodes;
            let len = nodes.len();
            if len < 2 {
                continue;
            }

            // Check if (a,b) or (b,a) are consecutive in the face ring
            let mut found = false;
            for i in 0..len {
                let j = (i + 1) % len;
                if (nodes[i] == a && nodes[j] == b) || (nodes[i] == b && nodes[j] == a) {
                    found = true;
                    break;
                }
            }
            if found {
                adjacent.push(f_idx);
            }
        }

        if adjacent.is_empty() {
            continue;
        }

        // Calculate eave z (min of adjacent faces' eave z)
        let mut ez = f64::INFINITY;
        let mut has_valid_eave = false;
        for &f_idx in &adjacent {
            if let Some(edge_idx) = g.skel.faces.get(f_idx).map(|f| f.edge) {
                if let Some(&z) = g.eave_z.get(edge_idx) {
                    if z < ez {
                        ez = z;
                    }
                    has_valid_eave = true;
                }
            }
        }

        if !has_valid_eave {
            continue;
        }

        // Get node positions and times
        let node_a = match g.skel.nodes.get(a) {
            Some(n) => n,
            None => continue,
        };
        let node_b = match g.skel.nodes.get(b) {
            Some(n) => n,
            None => continue,
        };

        let za = ez + node_a.time;
        let zb = ez + node_b.time;

        // Classify kind
        let kind = classify_kind(g, &adjacent, a, b, n_rings);

        edges.push(RoofEdge { kind, a: node_a.pos, b: node_b.pos, za, zb, ..Default::default() });
    }

    // Append Eave edges from footprint
    for (i, e) in g.fp.edges.iter().enumerate() {
        if let Some(&z) = g.eave_z.get(i) {
            edges.push(RoofEdge { kind: RoofEdgeKind::Eave, a: e.a, b: e.b, za: z, zb: z, ..Default::default() });
        }
    }

    edges
}

/// Quantise a position to 1 µm resolution for footprint vertex lookup.
fn pt_key_1e6(x: f64, y: f64) -> (i64, i64) {
    ((x * 1_000_000.0).round() as i64, (y * 1_000_000.0).round() as i64)
}

/// Return whether `pos` is a vertex of the footprint outer ring, and if so
/// whether that vertex is convex (`true`) or reflex (`false`).
///
/// The outer ring is assumed CCW; a left turn at vertex `v` (cross > 0) is
/// convex, a right turn (cross < 0) is reflex.  Returns `None` when `pos`
/// does not match any footprint vertex within 1 µm.
fn footprint_vertex_convexity(pos: Vec2, outer: &[Vec2]) -> Option<bool> {
    let n = outer.len();
    if n < 3 {
        return None;
    }
    let key = pt_key_1e6(pos.x, pos.y);
    for i in 0..n {
        let v = outer[i];
        if pt_key_1e6(v.x, v.y) != key {
            continue;
        }
        let u = outer[(i + n - 1) % n];
        let w = outer[(i + 1) % n];
        let cross = (v - u).cross(w - v);
        return Some(cross > 0.0);
    }
    None
}

fn classify_kind(g: &RoofGeom, adjacent: &[usize], a: usize, b: usize, _n_rings: usize) -> RoofEdgeKind {
    // GableEnd if any adjacent face has infinite slope
    for &f_idx in adjacent {
        if let Some(face) = g.skel.faces.get(f_idx) {
            if let Some(&slope) = g.slopes.get(face.edge) {
                if slope.is_infinite() {
                    return RoofEdgeKind::GableEnd;
                }
            }
        }
    }

    // If not exactly two faces, it's a boundary edge
    if adjacent.len() != 2 {
        return RoofEdgeKind::Eave;
    }

    let node_a = match g.skel.nodes.get(a) {
        Some(n) => n,
        None => return RoofEdgeKind::Eave,
    };
    let node_b = match g.skel.nodes.get(b) {
        Some(n) => n,
        None => return RoofEdgeKind::Eave,
    };

    // Primary signal: footprint vertex convexity.
    // Skeleton nodes that originate at a footprint corner have time == 0 and their
    // position exactly equals the corresponding `fp.outer` vertex.  A reflex corner
    // (right turn in CCW ring) marks the top of a valley; a convex corner (left
    // turn) marks the foot of a hip.  This is deterministic and requires no sampling.
    let conv_a = footprint_vertex_convexity(node_a.pos, &g.fp.outer);
    let conv_b = footprint_vertex_convexity(node_b.pos, &g.fp.outer);

    // Use vertex a if it is a footprint vertex; otherwise b; if both, a takes precedence.
    let primary = if conv_a.is_some() { conv_a } else { conv_b };
    if let Some(is_convex) = primary {
        return if is_convex { RoofEdgeKind::Hip } else { RoofEdgeKind::Valley };
    }

    // Neither endpoint is a footprint vertex: use the sample/height test for
    // interior arcs (ridge, or hips on complex polygons with collinear eaves).
    let f0_idx = adjacent[0];
    let f1_idx = adjacent[1];

    let m_pt = (node_a.pos + node_b.pos) * 0.5;
    let e = (node_b.pos - node_a.pos).normalized();
    let m = e.perp();

    let surface_height = |face_idx: usize, pt: Vec2| -> Option<f64> {
        let face = g.skel.faces.get(face_idx)?;
        let slope = g.slopes.get(face.edge)?;
        if slope.is_infinite() {
            return None;
        }
        let eave_z = g.eave_z.get(face.edge)?;
        let edge = g.fp.edges.get(face.edge)?;
        let v = edge.b - edge.a;
        if v.len() < 1e-12 {
            return Some(*eave_z);
        }
        let dist = v.cross(pt - edge.a).abs() / v.len();
        Some(*eave_z + slope * dist)
    };

    let sample_point = |face_idx: usize| -> Option<Vec2> {
        let ring = face_ring(g, face_idx);
        if ring.is_empty() {
            return None;
        }
        let mut s = m_pt + m * 0.05;
        if point_in_polygon(&s, &ring) {
            return Some(s);
        }
        s = m_pt - m * 0.05;
        if point_in_polygon(&s, &ring) {
            return Some(s);
        }
        let c = face_centroid(g, face_idx);
        let dir = (c - m_pt).normalized();
        s = m_pt + dir * 0.05;
        if point_in_polygon(&s, &ring) {
            return Some(s);
        }
        for step in [0.01, 0.001] {
            s = m_pt + dir * step;
            if point_in_polygon(&s, &ring) {
                return Some(s);
            }
        }
        None
    };

    let s0 = sample_point(f0_idx);
    let s1 = sample_point(f1_idx);
    let z0 = s0.and_then(|p| surface_height(f0_idx, p));
    let z1 = s1.and_then(|p| surface_height(f1_idx, p));

    let get_node_z = |node_idx: usize, face_idx: usize| -> Option<f64> {
        let node = g.skel.nodes.get(node_idx)?;
        let face = g.skel.faces.get(face_idx)?;
        let eave_z = g.eave_z.get(face.edge)?;
        Some(eave_z + node.time)
    };

    let za0 = get_node_z(a, f0_idx);
    let zb0 = get_node_z(b, f0_idx);
    let za1 = get_node_z(a, f1_idx);
    let zb1 = get_node_z(b, f1_idx);

    let eps = 1e-9;

    let (za, zb) = match (za0, zb0, za1, zb1) {
        (Some(a0), Some(b0), Some(a1), Some(b1)) => ((a0 + a1) * 0.5, (b0 + b1) * 0.5),
        _ => {
            // Cannot get node heights; fall back on raw node times.
            return if (node_a.time - node_b.time).abs() <= eps { RoofEdgeKind::Ridge } else { RoofEdgeKind::Hip };
        }
    };

    let z_mid = (za + zb) * 0.5;

    if let (Some(z0), Some(z1)) = (z0, z1) {
        let above0 = z0 > z_mid + eps;
        let above1 = z1 > z_mid + eps;
        let below0 = z0 < z_mid - eps;
        let below1 = z1 < z_mid - eps;

        if above0 && above1 {
            return RoofEdgeKind::Valley;
        }
        if below0 && below1 {
            if (za - zb).abs() <= eps {
                return RoofEdgeKind::Ridge;
            }
            return RoofEdgeKind::Hip;
        }
        if (above0 && below1) || (below0 && above1) {
            return RoofEdgeKind::Hip;
        }
    }

    // Final fallback: ridge if both ends are at equal height, hip otherwise.
    if (node_a.time - node_b.time).abs() <= eps { RoofEdgeKind::Ridge } else { RoofEdgeKind::Hip }
}

/// Helper to get the ring of vertices for a face.
fn face_ring(g: &RoofGeom, face_idx: usize) -> Vec<Vec2> {
    let face = match g.skel.faces.get(face_idx) {
        Some(f) => f,
        None => return Vec::new(),
    };
    face.nodes.iter().filter_map(|&n| g.skel.nodes.get(n).map(|node| node.pos)).collect()
}

/// Helper to calculate the centroid of a face's vertices.
fn face_centroid(g: &RoofGeom, face_idx: usize) -> Vec2 {
    let ring = face_ring(g, face_idx);
    if ring.is_empty() {
        return Vec2::ZERO;
    }
    let sum = ring.iter().fold(Vec2::ZERO, |acc, p| acc + *p);
    sum * (1.0 / ring.len() as f64)
}

/// Check if point is inside polygon (ray casting).
fn point_in_polygon(p: &Vec2, poly: &[Vec2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let pi = poly[i];
        let pj = poly[j];
        if ((pi.y > p.y) != (pj.y > p.y)) && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Compute roof edges for a model level and roof configuration.
///
/// Builds the roof geometry and classifies its edges. Returns an error if the
/// geometry cannot be built.
pub fn roof_edges(model: &cadcraft_doc::ArchModel, level: u32, roof: &Roof) -> Result<Vec<RoofEdge>, RoofError> {
    crate::roof::roof_geometry(model, level, roof).map(|g| roof_edges_geom(&g))
}

/// Returns pairs of adjacent face indices and the node indices of the shared arc.
///
/// Only includes arcs where `a != b` and exactly two faces are adjacent.
pub fn interior_edge_pairs(g: &RoofGeom) -> Vec<((usize, usize), (usize, usize))> {
    let mut pairs = Vec::new();

    for &(a, b) in &g.skel.arcs {
        if a == b {
            continue;
        }

        let mut adjacent = Vec::new();
        for (f_idx, face) in g.skel.faces.iter().enumerate() {
            let nodes = &face.nodes;
            let len = nodes.len();
            if len < 2 {
                continue;
            }

            let mut found = false;
            for i in 0..len {
                let j = (i + 1) % len;
                if (nodes[i] == a && nodes[j] == b) || (nodes[i] == b && nodes[j] == a) {
                    found = true;
                    break;
                }
            }
            if found {
                adjacent.push(f_idx);
            }
        }

        if adjacent.len() == 2 {
            pairs.push(((adjacent[0], adjacent[1]), (a, b)));
        }
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{ArchModel, Roof, RoofDirective, Wall};
    use cadcraft_geom::Vec2;

    fn house_10_8(m: &mut ArchModel) {
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));

        for w in m.walls.iter_mut() {
            w.height = 2.4;
        }
    }

    fn l_house(m: &mut ArchModel) {
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(6.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(6.0, 8.0), Vec2::new(6.0, 12.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(6.0, 12.0), Vec2::new(0.0, 12.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(0.0, 12.0), Vec2::new(0.0, 0.0), 0.2));

        for w in m.walls.iter_mut() {
            w.height = 2.4;
        }
    }

    fn t_house(m: &mut ArchModel) {
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(7.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(7.0, 8.0), Vec2::new(7.0, 12.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(7.0, 12.0), Vec2::new(3.0, 12.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(3.0, 12.0), Vec2::new(3.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(3.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));

        for w in m.walls.iter_mut() {
            w.height = 2.4;
        }
    }

    fn test_roof() -> Roof {
        Roof { overhang: 0.0, thickness: 0.0, ..Roof::default() }
    }

    fn geom(m: &ArchModel) -> RoofGeom {
        crate::roof::roof_geometry(m, 0, &test_roof()).unwrap()
    }

    #[test]
    fn hip_roof_has_one_ridge_and_four_hips() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let ridges: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Ridge).collect();
        assert_eq!(ridges.len(), 1, "Expected exactly one ridge");

        let ridge = &ridges[0];
        // Endpoints should be (4.0, 4.0) and (6.0, 4.0)
        let p1 = Vec2::new(4.0, 4.0);
        let p2 = Vec2::new(6.0, 4.0);

        let match1 = (ridge.a - p1).len() < 1e-6 && (ridge.b - p2).len() < 1e-6;
        let match2 = (ridge.a - p2).len() < 1e-6 && (ridge.b - p1).len() < 1e-6;
        assert!(match1 || match2, "Ridge endpoints mismatch: {:?} {:?}", ridge.a, ridge.b);

        // Height 4.45
        assert!((ridge.za - 4.45).abs() < 1e-6);
        assert!((ridge.zb - 4.45).abs() < 1e-6);

        let hips: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Hip).collect();
        assert_eq!(hips.len(), 4, "Expected exactly four hips");

        let valleys: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Valley).collect();
        assert_eq!(valleys.len(), 0, "Expected no valleys");

        // Check hip endpoints are footprint corners
        let corners = [Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(10.1, 8.1), Vec2::new(-0.1, 8.1)];

        for hip in hips {
            let is_corner_a = corners.iter().any(|c| (hip.a - *c).len() < 1e-6);
            let is_corner_b = corners.iter().any(|c| (hip.b - *c).len() < 1e-6);
            assert!(is_corner_a || is_corner_b, "Hip endpoint not a corner: {:?} {:?}", hip.a, hip.b);
        }
    }

    #[test]
    fn l_footprint_has_one_valley() {
        let mut m = ArchModel::default();
        l_house(&mut m);
        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let valleys: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Valley).collect();
        assert_eq!(valleys.len(), 1, "Expected exactly one valley");

        let valley = &valleys[0];
        // Reflex vertex at (6.1, 8.1) (outer face of wall at x=6 with 0.2 thickness/overhang)
        let reflex = Vec2::new(6.1, 8.1);

        let has_reflex_a = (valley.a - reflex).len() < 1e-6;
        let has_reflex_b = (valley.b - reflex).len() < 1e-6;

        assert!(has_reflex_a || has_reflex_b, "Valley endpoint not reflex vertex: {:?} {:?}", valley.a, valley.b);
    }

    #[test]
    fn t_footprint_has_two_valleys() {
        let mut m = ArchModel::default();
        t_house(&mut m);
        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let valleys: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Valley).collect();
        assert_eq!(valleys.len(), 2, "Expected exactly two valleys");

        let v1 = Vec2::new(7.1, 8.1);
        let v2 = Vec2::new(2.9, 8.1);

        let mut found_v1 = false;
        let mut found_v2 = false;

        for valley in valleys {
            if (valley.a - v1).len() < 1e-6 || (valley.b - v1).len() < 1e-6 {
                found_v1 = true;
            }
            if (valley.a - v2).len() < 1e-6 || (valley.b - v2).len() < 1e-6 {
                found_v2 = true;
            }
        }

        assert!(found_v1, "Valley 1 not found");
        assert!(found_v2, "Valley 2 not found");
    }

    /// The brief's T: a 12 x 4 bar with a 4 x 4 stem on top.
    fn t_house_brief(m: &mut ArchModel) {
        let ring = [(0.0, 0.0), (12.0, 0.0), (12.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0), (4.0, 4.0), (0.0, 4.0)];
        for (i, &(x, y)) in ring.iter().enumerate() {
            let (nx, ny) = ring[(i + 1) % ring.len()];
            m.push_wall(Wall::new(Vec2::new(x, y), Vec2::new(nx, ny), 0.2));
        }
        for w in m.walls.iter_mut() {
            w.height = 2.4;
        }
    }

    fn brief_roof() -> Roof {
        Roof { pitch_rise: 6.0, pitch_run: 12.0, overhang: 0.0, thickness: 0.0, ..Default::default() }
    }

    /// Whether `p` is a reflex vertex of the ring `outer`.
    fn is_reflex_vertex(outer: &[Vec2], p: Vec2) -> bool {
        let n = outer.len();
        let area2: f64 = (0..n)
            .map(|i| {
                let (a, b) = (outer[i], outer[(i + 1) % n]);
                a.x * b.y - b.x * a.y
            })
            .sum();
        let sign = if area2 >= 0.0 { 1.0 } else { -1.0 };
        (0..n).any(|i| {
            if (outer[i] - p).len() >= 1e-6 {
                return false;
            }
            let prev = outer[(i + n - 1) % n];
            let next = outer[(i + 1) % n];
            let (d1, d2) = (outer[i] - prev, next - outer[i]);
            (d1.x * d2.y - d1.y * d2.x) * sign < -1e-9
        })
    }

    #[test]
    fn t_roof_has_two_valleys() {
        let mut m = ArchModel::default();
        t_house_brief(&mut m);
        let g = crate::roof::roof_geometry(&m, 0, &brief_roof()).unwrap();
        let edges = roof_edges_geom(&g);

        let valleys: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Valley).collect();
        assert_eq!(valleys.len(), 2, "Expected exactly two valleys");
        for v in valleys {
            assert!(is_reflex_vertex(&g.fp.outer, v.a) || is_reflex_vertex(&g.fp.outer, v.b), "Valley has no reflex endpoint: {:?} {:?}", v.a, v.b);
        }
    }

    #[test]
    fn l_roof_has_one_valley_brief() {
        let mut m = ArchModel::default();
        l_house(&mut m);
        let g = crate::roof::roof_geometry(&m, 0, &brief_roof()).unwrap();
        let edges = roof_edges_geom(&g);

        let valleys: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Valley).collect();
        assert_eq!(valleys.len(), 1, "Expected exactly one valley");
        let v = valleys[0];
        assert!(is_reflex_vertex(&g.fp.outer, v.a) || is_reflex_vertex(&g.fp.outer, v.b), "Valley has no reflex endpoint: {:?} {:?}", v.a, v.b);
    }

    #[test]
    fn hip_roof_valleys_follow_reflex_vertices() {
        let mut m = ArchModel::default();
        l_house(&mut m);
        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let valleys: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Valley).collect();
        assert_eq!(valleys.len(), 1, "L footprint should have exactly one valley");

        // The reflex outer-face vertex of the L footprint sits at (6.1, 8.1).
        let reflex = Vec2::new(6.1, 8.1);
        let valley = &valleys[0];
        let anchored = (valley.a - reflex).len() < 1e-6 || (valley.b - reflex).len() < 1e-6;
        assert!(anchored, "Valley must start at the reflex vertex (6.1,8.1): a={:?} b={:?}", valley.a, valley.b);
    }

    #[test]
    fn convex_footprint_vertices_start_a_hip() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let corners = [Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(10.1, 8.1), Vec2::new(-0.1, 8.1)];

        let hips: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Hip).collect();

        // Every hip must have one endpoint at a convex footprint corner.
        for hip in &hips {
            let at_corner = corners.iter().any(|c| (hip.a - *c).len() < 1e-6 || (hip.b - *c).len() < 1e-6);
            assert!(at_corner, "Hip not anchored at a corner: a={:?} b={:?}", hip.a, hip.b);
        }

        // Every convex corner must have a hip starting from it.
        for corner in &corners {
            let has_hip = hips.iter().any(|h| (h.a - *corner).len() < 1e-6 || (h.b - *corner).len() < 1e-6);
            assert!(has_hip, "Convex corner {:?} has no hip edge", corner);
        }
    }

    #[test]
    fn gable_ridge_spans_the_whole_house() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);

        // Set gable directive on walls at x=0 and x=10
        // Walls are pushed in order:
        // 0: (0,0)-(10,0) South
        // 1: (10,0)-(10,8) East
        // 2: (10,8)-(0,8) North
        // 3: (0,8)-(0,0) West

        if let Some(w) = m.walls.get_mut(1) {
            w.roof_directive = RoofDirective::Gable;
        }
        if let Some(w) = m.walls.get_mut(3) {
            w.roof_directive = RoofDirective::Gable;
        }

        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let ridges: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Ridge).collect();
        assert_eq!(ridges.len(), 1, "Expected exactly one ridge");

        let ridge = &ridges[0];
        // Endpoints (-0.1, 4.0) and (10.1, 4.0)
        let p1 = Vec2::new(-0.1, 4.0);
        let p2 = Vec2::new(10.1, 4.0);

        let match1 = (ridge.a - p1).len() < 1e-6 && (ridge.b - p2).len() < 1e-6;
        let match2 = (ridge.a - p2).len() < 1e-6 && (ridge.b - p1).len() < 1e-6;
        assert!(match1 || match2, "Ridge endpoints mismatch: {:?} {:?}", ridge.a, ridge.b);

        assert!((ridge.za - 4.45).abs() < 1e-6);
        assert!((ridge.zb - 4.45).abs() < 1e-6);

        let gable_ends: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::GableEnd).collect();
        assert!(!gable_ends.is_empty(), "Expected at least one gable end");
    }

    #[test]
    fn ridge_end_heights_are_the_apex() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);

        // Pitch 4/12
        let roof_4 = Roof { pitch_rise: 4.0, pitch_run: 12.0, overhang: 0.0, thickness: 0.0, ..Roof::default() };
        let g4 = crate::roof::roof_geometry(&m, 0, &roof_4).unwrap();
        let edges4 = roof_edges_geom(&g4);
        let ridge4 = edges4.iter().find(|e| e.kind == RoofEdgeKind::Ridge).unwrap();
        assert!((ridge4.za - 3.766667).abs() < 1e-5, "Pitch 4/12 height mismatch: {}", ridge4.za);
        assert!((ridge4.zb - 3.766667).abs() < 1e-5);

        // Pitch 6/12 (default)
        let roof_6 = Roof { pitch_rise: 6.0, pitch_run: 12.0, overhang: 0.0, thickness: 0.0, ..Roof::default() };
        let g6 = crate::roof::roof_geometry(&m, 0, &roof_6).unwrap();
        let edges6 = roof_edges_geom(&g6);
        let ridge6 = edges6.iter().find(|e| e.kind == RoofEdgeKind::Ridge).unwrap();
        assert!((ridge6.za - 4.45).abs() < 1e-5, "Pitch 6/12 height mismatch: {}", ridge6.za);
        assert!((ridge6.zb - 4.45).abs() < 1e-5);

        // Pitch 12/12
        let roof_12 = Roof { pitch_rise: 12.0, pitch_run: 12.0, overhang: 0.0, thickness: 0.0, ..Roof::default() };
        let g12 = crate::roof::roof_geometry(&m, 0, &roof_12).unwrap();
        let edges12 = roof_edges_geom(&g12);
        let ridge12 = edges12.iter().find(|e| e.kind == RoofEdgeKind::Ridge).unwrap();
        assert!((ridge12.za - 6.5).abs() < 1e-5, "Pitch 12/12 height mismatch: {}", ridge12.za);
        assert!((ridge12.zb - 6.5).abs() < 1e-5);
    }

    #[test]
    fn interior_edges_are_shared_by_two_planes() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let g = geom(&m);
        let pairs = interior_edge_pairs(&g);

        let arc_count = g.skel.arcs.iter().filter(|(a, b)| a != b).count();
        assert_eq!(pairs.len(), arc_count, "Interior edge pairs count mismatch");

        let mut m_l = ArchModel::default();
        l_house(&mut m_l);
        let g_l = geom(&m_l);
        let pairs_l = interior_edge_pairs(&g_l);
        let arc_count_l = g_l.skel.arcs.iter().filter(|(a, b)| a != b).count();
        assert_eq!(pairs_l.len(), arc_count_l, "L-shape interior edge pairs count mismatch");

        let mut m_t = ArchModel::default();
        t_house(&mut m_t);
        let g_t = geom(&m_t);
        let pairs_t = interior_edge_pairs(&g_t);
        let arc_count_t = g_t.skel.arcs.iter().filter(|(a, b)| a != b).count();
        assert_eq!(pairs_t.len(), arc_count_t, "T-shape interior edge pairs count mismatch");
    }

    #[test]
    fn eave_edges_match_the_footprint() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let g = geom(&m);
        let edges = roof_edges_geom(&g);

        let eaves: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Eave).collect();
        assert_eq!(eaves.len(), g.fp.edges.len(), "Eave count mismatch");

        for (i, e) in eaves.iter().enumerate() {
            let fp_edge = &g.fp.edges[i];
            assert!((e.a - fp_edge.a).len() < 1e-6, "Eave a mismatch at {}", i);
            assert!((e.b - fp_edge.b).len() < 1e-6, "Eave b mismatch at {}", i);
            assert!((e.za - g.eave_z[i]).abs() < 1e-6, "Eave za mismatch at {}", i);
            assert!((e.zb - g.eave_z[i]).abs() < 1e-6, "Eave zb mismatch at {}", i);
        }
    }

    #[test]
    fn roof_edges_wrapper_matches_geom() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let roof = test_roof();
        let edges_wrapper = roof_edges(&m, 0, &roof).unwrap();
        let g = geom(&m);
        let edges_geom = roof_edges_geom(&g);
        assert_eq!(edges_wrapper.len(), edges_geom.len());
        for (w, g) in edges_wrapper.iter().zip(edges_geom.iter()) {
            assert_eq!(w.kind, g.kind);
            assert!((w.a - g.a).len() < 1e-9);
            assert!((w.b - g.b).len() < 1e-9);
            assert!((w.za - g.za).abs() < 1e-9);
            assert!((w.zb - g.zb).abs() < 1e-9);
        }
    }

    #[test]
    fn build_roof_fills_edges() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let roof = crate::roof::build_roof(&m, 0, &test_roof()).unwrap();

        let ridges: Vec<_> = roof.edges.iter().filter(|e| e.kind == RoofEdgeKind::Ridge).collect();
        assert_eq!(ridges.len(), 1, "Expected exactly one ridge in built roof edges");

        let ridge = &ridges[0];
        let p1 = Vec2::new(4.0, 4.0);
        let p2 = Vec2::new(6.0, 4.0);

        let match1 = (ridge.a - p1).len() < 1e-6 && (ridge.b - p2).len() < 1e-6;
        let match2 = (ridge.a - p2).len() < 1e-6 && (ridge.b - p1).len() < 1e-6;
        assert!(match1 || match2, "Ridge endpoints mismatch: {:?} {:?}", ridge.a, ridge.b);

        assert!((ridge.za - 4.45).abs() < 1e-6);
        assert!((ridge.zb - 4.45).abs() < 1e-6);

        let hips: Vec<_> = roof.edges.iter().filter(|e| e.kind == RoofEdgeKind::Hip).collect();
        assert_eq!(hips.len(), 4, "Expected exactly four hips in built roof edges");
    }

    #[test]
    fn roof_edges_errors_on_no_walls() {
        let m = ArchModel::default();
        let res = roof_edges(&m, 0, &test_roof());
        assert!(matches!(res, Err(RoofError::NoWalls)));
    }

    #[test]
    fn roof_edges_with_overhang() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let roof = Roof { overhang: 0.5, thickness: 0.0, ..Roof::default() };
        let edges = roof_edges(&m, 0, &roof).unwrap();
        let eaves: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::Eave).collect();
        assert_eq!(eaves.len(), 4, "Expected 4 eave edges with overhang");
        // Eave edges should span the overhang footprint: -0.6 to 10.6 in x, -0.6 to 8.6 in y
        let min_x = eaves.iter().map(|e| e.a.x.min(e.b.x)).fold(f64::INFINITY, f64::min);
        let max_x = eaves.iter().map(|e| e.a.x.max(e.b.x)).fold(f64::NEG_INFINITY, f64::max);
        let min_y = eaves.iter().map(|e| e.a.y.min(e.b.y)).fold(f64::INFINITY, f64::min);
        let max_y = eaves.iter().map(|e| e.a.y.max(e.b.y)).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - (-0.6)).abs() < 1e-6, "min_x {} != -0.6", min_x);
        assert!((max_x - 10.6).abs() < 1e-6, "max_x {} != 10.6", max_x);
        assert!((min_y - (-0.6)).abs() < 1e-6, "min_y {} != -0.6", min_y);
        assert!((max_y - 8.6).abs() < 1e-6, "max_y {} != 8.6", max_y);
    }

    #[test]
    fn roof_edges_errors_on_nan_pitch() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let roof = Roof { pitch_rise: f64::NAN, pitch_run: 12.0, overhang: 0.0, thickness: 0.0, ..Roof::default() };
        let result = roof_edges(&m, 0, &roof);
        assert!(matches!(result, Err(RoofError::Degenerate)), "Expected Degenerate error for NaN pitch");
    }

    #[test]
    fn roof_edge_heights_are_consistent() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let edges = roof_edges(&m, 0, &test_roof()).unwrap();
        for edge in &edges {
            // All edge heights should be >= eave height (2.4)
            assert!(edge.za >= 2.4 - 1e-9, "Edge za {} < eave height 2.4", edge.za);
            assert!(edge.zb >= 2.4 - 1e-9, "Edge zb {} < eave height 2.4", edge.zb);
            // Edge heights should be finite
            assert!(edge.za.is_finite(), "Edge za is not finite");
            assert!(edge.zb.is_finite(), "Edge zb is not finite");
        }
    }

    #[test]
    fn single_gable_end_gives_three_planes_and_edges() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        // Only the right wall (x=10, wall 1) is a gable end.
        if let Some(w) = m.walls.get_mut(1) {
            w.roof_directive = RoofDirective::Gable;
        }
        let roof = crate::roof::build_roof(&m, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 3, "one gable end should give 3 planes");
        let edges = roof_edges(&m, 0, &test_roof()).unwrap();
        let gable_ends: Vec<_> = edges.iter().filter(|e| e.kind == RoofEdgeKind::GableEnd).collect();
        assert!(!gable_ends.is_empty(), "Expected at least one gable end edge");
    }

    #[test]
    fn interior_edge_pairs_consistent_with_roof_edges() {
        let mut m = ArchModel::default();
        house_10_8(&mut m);
        let g = geom(&m);
        let pairs = interior_edge_pairs(&g);
        let edges = roof_edges_geom(&g);
        // Interior edges (non-eave) should match the number of interior edge pairs
        let non_eave: Vec<_> = edges.iter().filter(|e| e.kind != RoofEdgeKind::Eave).collect();
        assert_eq!(pairs.len(), non_eave.len(), "Interior edge pairs should match non-eave edges");
    }
}
