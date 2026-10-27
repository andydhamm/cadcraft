use cadcraft_geom::Vec2;
use cadcraft_geom::skeleton::{Skeleton, SkeletonError, check, roof_skeleton};

/// Helper to generate a simple convex polygon (square).
fn square(size: f64) -> Vec<Vec2> {
    let h = size / 2.0;
    vec![Vec2::new(-h, -h), Vec2::new(h, -h), Vec2::new(h, h), Vec2::new(-h, h)]
}

/// Helper to generate an L-shaped polygon.
fn l_shape() -> Vec<Vec2> {
    // (0,0) (10,0) (10,8) (6,8) (6,12) (0,12)
    vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(6.0, 8.0), Vec2::new(6.0, 12.0), Vec2::new(0.0, 12.0)]
}

/// Helper to generate a T-shaped polygon.
fn t_shape() -> Vec<Vec2> {
    // A T shape:
    // (0,0) (10,0) (10,4) (6,4) (6,10) (4,10) (4,4) (0,4)
    vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(10.0, 4.0),
        Vec2::new(6.0, 4.0),
        Vec2::new(6.0, 10.0),
        Vec2::new(4.0, 10.0),
        Vec2::new(4.0, 4.0),
        Vec2::new(0.0, 4.0),
    ]
}

/// Helper to calculate polygon area (shoelace formula).
fn polygon_area(pts: &[Vec2]) -> f64 {
    let mut area = 0.0;
    let n = pts.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += pts[i].x * pts[j].y - pts[j].x * pts[i].y;
    }
    area.abs() / 2.0
}

/// Helper to check if a vertex is reflex in a CCW polygon.
/// Reflex if cross product of (v - prev) and (next - v) is negative (for CCW).
fn is_reflex(pts: &[Vec2], idx: usize) -> bool {
    let n = pts.len();
    let prev = pts[(idx + n - 1) % n];
    let curr = pts[idx];
    let next = pts[(idx + 1) % n];

    let a = curr - prev;
    let b = next - curr;

    let cross = a.x * b.y - a.y * b.x;
    cross < -1e-9
}

/// Helper to assert that a skeleton is valid.
fn assert_valid(skel: &Skeleton) {
    let problems = check(skel, 1e-7);
    assert!(problems.is_empty(), "Skeleton invalid: {problems:#?}");
}

/// Helper to get the number of ring vertices.
fn ring_vertex_count(skel: &Skeleton) -> usize {
    skel.rings.iter().map(|r| r.len()).sum()
}

#[test]
fn every_footprint_vertex_starts_exactly_one_sloped_edge() {
    let outer = square(10.0);
    let holes: Vec<Vec<Vec2>> = vec![];
    let slopes = vec![vec![1.0; outer.len()]];

    let skel = roof_skeleton(&outer, &holes, &slopes).expect("skeleton failed");
    assert_valid(&skel);

    let ring_vertices = ring_vertex_count(&skel);

    // Each input vertex (node index < ring_vertices) should be the start of exactly one arc.
    // In the skeleton, arcs are stored in `skel.arcs` as (start_node, end_node).
    // We count how many arcs start at each input vertex.
    let mut start_counts = vec![0; ring_vertices];

    for &(a, _b) in &skel.arcs {
        if a < ring_vertices {
            start_counts[a] += 1;
        }
    }

    for (i, count) in start_counts.iter().enumerate() {
        assert_eq!(*count, 1, "Vertex {i} starts {count} arcs, expected 1");
    }
}

#[test]
fn every_footprint_edge_has_exactly_one_plane() {
    let outer = square(10.0);
    let holes: Vec<Vec<Vec2>> = vec![];
    let slopes = vec![vec![1.0; outer.len()]];

    let skel = roof_skeleton(&outer, &holes, &slopes).expect("skeleton failed");
    assert_valid(&skel);

    // Each input edge should correspond to exactly one face.
    // Faces are stored in `skel.faces`. Each face has `ring` and `edge` indices.
    // We count how many faces refer to each edge.
    let mut edge_counts = vec![0; outer.len()];

    for face in &skel.faces {
        if face.ring == 0 {
            edge_counts[face.edge] += 1;
        }
    }

    for (i, count) in edge_counts.iter().enumerate() {
        assert_eq!(*count, 1, "Edge {i} has {count} faces, expected 1");
    }
}

#[test]
fn every_plane_slope_equals_its_pitch() {
    let outer = square(10.0);
    let holes: Vec<Vec<Vec2>> = vec![];
    let slope_val = 0.5;
    let slopes = vec![vec![slope_val; outer.len()]];

    let skel = roof_skeleton(&outer, &holes, &slopes).expect("skeleton failed");
    assert_valid(&skel);

    // For each face, check that the slope matches the input slope.
    // The face's nodes should satisfy: distance_to_edge_line == weight * time.
    // For roof skeleton, weight = 1 / slope.
    // So distance == time / slope => slope == time / distance.

    for face in &skel.faces {
        if face.ring != 0 {
            continue;
        }

        let ring = &skel.rings[face.ring];
        let n = ring.len();
        let a = ring[face.edge];
        let b = ring[(face.edge + 1) % n];

        // Edge direction
        let dir = (b - a).normalized();
        // Normal (left of direction, inward for CCW)
        let normal = Vec2::new(-dir.y, dir.x);

        let input_slope = slopes[face.ring][face.edge];

        for &node_idx in &face.nodes {
            let node = &skel.nodes[node_idx];
            // Distance from node.pos to the line defined by a and dir.
            // dist = dot(node.pos - a, normal)
            let dist = (node.pos - a).dot(normal);

            // Expected: dist == node.time * weight == node.time / input_slope
            // So input_slope == node.time / dist (if dist > 0)

            if dist > 1e-9 {
                let calculated_slope = node.time / dist;
                assert!(
                    (calculated_slope - input_slope).abs() < 1e-6,
                    "Face edge {}: node {} has slope {} but expected {}",
                    face.edge,
                    node_idx,
                    calculated_slope,
                    input_slope
                );
            }
        }
    }
}

#[test]
fn interior_arcs_are_shared_by_exactly_two_faces() {
    let outer = square(10.0);
    let holes: Vec<Vec<Vec2>> = vec![];
    let slopes = vec![vec![1.0; outer.len()]];

    let skel = roof_skeleton(&outer, &holes, &slopes).expect("skeleton failed");
    assert_valid(&skel);

    // Interior arcs are those not on the boundary of the polygon.
    // An arc is on the boundary if one of its endpoints is an input vertex AND the arc is part of the face boundary?
    // Actually, simpler: An arc is "interior" if it is not an edge of the input polygon.
    // But arcs are skeleton edges.
    // We need to count how many faces share each arc.
    // Faces are defined by their node loops.
    // We can iterate over faces and their node loops to find shared edges.

    let mut arc_face_counts: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();

    for face in &skel.faces {
        let nodes = &face.nodes;
        let n = nodes.len();
        for i in 0..n {
            let a = nodes[i];
            let b = nodes[(i + 1) % n];
            // Normalize edge key
            let key = if a < b { (a, b) } else { (b, a) };
            *arc_face_counts.entry(key).or_insert(0) += 1;
        }
    }

    // Now, identify which arcs are "interior".
    // An arc is interior if it is not an edge of the input polygon.
    // Input polygon edges connect input vertices (indices < ring_vertex_count).
    // But skeleton arcs can connect input vertices too (e.g. if a vertex moves along an edge? No, vertices move along bisectors).
    // Actually, the skeleton arcs are the ridges/valleys.
    // The "boundary" of the skeleton is the input polygon.
    // So any arc connecting two input vertices that is NOT an input edge?
    // Or simply: any arc that is not part of the input polygon boundary.

    // Let's collect input edges.
    let ring_vertices = ring_vertex_count(&skel);
    let mut input_edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for ring in &skel.rings {
        let n = ring.len();
        for i in 0..n {
            let a = i; // Assuming ring 0 vertices are 0..n-1, ring 1 are n.. etc.
            // Wait, node indices for ring vertices are sequential across rings.
            // We need to map ring/edge to node indices.
            // This is complex.
        }
    }

    // Alternative: Just check that all arcs in `skel.arcs` are shared by exactly 2 faces,
    // EXCEPT those that are on the boundary.
    // How to identify boundary arcs?
    // Boundary arcs are those that lie on the input polygon edges? No, skeleton arcs are internal.
    // The input polygon edges are NOT skeleton arcs.
    // So ALL skeleton arcs should be shared by exactly 2 faces?
    // Let's verify this hypothesis.

    for (key, count) in &arc_face_counts {
        // If an arc is in skel.arcs, it should be shared by 2 faces.
        // But some arcs might be on the boundary if the skeleton touches the boundary?
        // In a straight skeleton, the skeleton is internal.
        // So all arcs should be shared by 2 faces.

        // Check if this arc is in skel.arcs
        let is_skel_arc = skel.arcs.iter().any(|&(a, b)| {
            let k = if a < b { (a, b) } else { (b, a) };
            k == *key
        });

        if is_skel_arc {
            assert_eq!(*count, 2, "Arc {:?} is shared by {} faces, expected 2", key, count);
        }
    }
}

#[test]
fn projected_face_areas_sum_to_the_polygon_area() {
    let outer = square(10.0);
    let holes: Vec<Vec<Vec2>> = vec![];
    let slopes = vec![vec![1.0; outer.len()]];

    let skel = roof_skeleton(&outer, &holes, &slopes).expect("skeleton failed");
    assert_valid(&skel);

    let expected_area = polygon_area(&outer);
    let mut total_area = 0.0;

    for face in &skel.faces {
        let pts: Vec<Vec2> = face.nodes.iter().map(|&i| skel.nodes[i].pos).collect();
        total_area += polygon_area(&pts);
    }

    assert!((total_area - expected_area).abs() < 1e-6, "Total face area {} != polygon area {}", total_area, expected_area);
}

#[test]
fn an_l_footprint_has_one_valley_arc_and_a_t_has_two() {
    // L shape
    let l_outer = l_shape();
    let l_holes: Vec<Vec<Vec2>> = vec![];
    let l_slopes = vec![vec![1.0; l_outer.len()]];

    let l_skel = roof_skeleton(&l_outer, &l_holes, &l_slopes).expect("L skeleton failed");
    assert_valid(&l_skel);

    // Count valley arcs.
    // Valley arc = arc whose footprint endpoint is a reflex vertex.
    // We need to identify arcs (edges) that are valleys.
    // And check their endpoints.

    // Identify reflex vertices in the input ring.
    let mut reflex_indices = vec![];
    for i in 0..l_outer.len() {
        if is_reflex(&l_outer, i) {
            reflex_indices.push(i);
        }
    }

    // Map input vertex index to node index.
    // For ring 0, node index is just the vertex index.
    let l_valley_arcs = count_valley_arcs(&l_skel, &reflex_indices);
    assert_eq!(l_valley_arcs, 1, "L shape should have 1 valley arc");

    // T shape
    let t_outer = t_shape();
    let t_holes: Vec<Vec<Vec2>> = vec![];
    let t_slopes = vec![vec![1.0; t_outer.len()]];

    let t_skel = roof_skeleton(&t_outer, &t_holes, &t_slopes).expect("T skeleton failed");
    assert_valid(&t_skel);

    let mut t_reflex_indices = vec![];
    for i in 0..t_outer.len() {
        if is_reflex(&t_outer, i) {
            t_reflex_indices.push(i);
        }
    }

    let t_valley_arcs = count_valley_arcs(&t_skel, &t_reflex_indices);
    assert_eq!(t_valley_arcs, 2, "T shape should have 2 valley arcs");
}

/// Count arcs that start or end at a reflex vertex.
/// Note: An arc might be counted twice if both ends are reflex?
/// The prompt says "valley arc = arc whose footprint endpoint is a reflex vertex".
/// Usually a valley arc starts at a reflex vertex and ends at another node.
/// If both ends are reflex, it's still one arc.
fn count_valley_arcs(skel: &Skeleton, reflex_indices: &[usize]) -> usize {
    let mut valley_arcs = std::collections::HashSet::new();

    for &(a, b) in &skel.arcs {
        // Check if a or b is a reflex vertex (node index matches input vertex index for ring 0)
        // Assuming ring 0 vertices are nodes 0..len-1
        let a_is_reflex = reflex_indices.contains(&a);
        let b_is_reflex = reflex_indices.contains(&b);

        if a_is_reflex || b_is_reflex {
            let key = if a < b { (a, b) } else { (b, a) };
            valley_arcs.insert(key);
        }
    }

    valley_arcs.len()
}
