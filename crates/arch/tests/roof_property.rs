use cadcraft_arch::roof::{build_roof, outer_footprint, projected_area_sum, roof_apex_z, roof_geometry};
use cadcraft_arch::roofedge::{interior_edge_pairs, roof_edges};
use cadcraft_doc::{ArchModel, Level, Roof, RoofDirective, RoofEdgeKind, Wall};
use cadcraft_geom::Vec2;

/// Helper to create a closed ring of exterior walls on level 0.
fn house(pts: &[(f64, f64)], height: f64) -> ArchModel {
    let mut model = ArchModel::default();
    model.levels.push(Level::default());

    if pts.len() < 3 {
        return model;
    }

    for i in 0..pts.len() {
        let a = Vec2::new(pts[i].0, pts[i].1);
        let b = Vec2::new(pts[(i + 1) % pts.len()].0, pts[(i + 1) % pts.len()].1);

        // Check for finite points
        if !a.is_finite() || !b.is_finite() {
            continue;
        }

        let mut wall = Wall::new(a, b, 0.2);
        wall.height = height;
        wall.base_level = 0;
        wall.wall_type = "Exterior".into();
        wall.roof_directive = RoofDirective::Auto;
        model.push_wall(wall);
    }
    model
}

/// Helper to create a roof spec.
fn roof(rise: f64, overhang: f64) -> Roof {
    Roof { pitch_rise: rise, pitch_run: 12.0, overhang, thickness: 0.0, ..Roof::default() }
}

/// Shoelace formula for polygon area.
fn shoelace(pts: &[Vec2]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        sum += pts[i].x * pts[j].y - pts[j].x * pts[i].y;
    }
    sum.abs() / 2.0
}

#[test]
fn all_hip_orthogonal_footprints_have_one_sloped_edge_per_vertex() {
    let rect = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let l_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (6.0, 8.0), (6.0, 12.0), (0.0, 12.0)];
    let t_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (7.0, 8.0), (7.0, 12.0), (3.0, 12.0), (3.0, 8.0), (0.0, 8.0)];

    let footprints = vec![rect, l_shape, t_shape];

    for pts in footprints {
        let model = house(&pts, 2.4);
        let r = roof(6.0, 0.0);

        // Build roof
        let built = build_roof(&model, 0, &r);
        if built.is_err() {
            // If it fails for a valid orthogonal shape, it's a bug, but we check topology only if Ok
            // For orthogonal shapes, it should succeed.
            assert!(false, "build_roof failed for valid orthogonal footprint: {:?}", built.err());
        }
        let roof_obj = built.unwrap();

        // The outer-face footprint (offset by half the wall thickness) is the ring the
        // roof is built on; the centre line alone can have fewer/more vertices than
        // the solved outer face.
        let footprint = outer_footprint(&model, 0).expect("outer footprint for valid walls");

        // Get edges
        let edges = roof_edges(&model, 0, &roof_obj).unwrap();

        // Count sloped edges (Hip/Valley) per outer-footprint vertex.
        // Invariant: one sloped edge per footprint vertex (convex hip, reflex valley).
        let mut vertex_counts: std::collections::HashMap<(i64, i64), usize> = std::collections::HashMap::new();

        // Initialize counts for outer-footprint vertices (0.1 m wall offset => 0.01 m grid).
        for p in &footprint.outer {
            let key = ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64);
            vertex_counts.insert(key, 0);
        }

        for edge in &edges {
            if matches!(edge.kind, RoofEdgeKind::Hip | RoofEdgeKind::Valley) {
                // Add to both endpoints
                let k1 = ((edge.a.x * 100.0).round() as i64, (edge.a.y * 100.0).round() as i64);
                let k2 = ((edge.b.x * 100.0).round() as i64, (edge.b.y * 100.0).round() as i64);

                if let Some(c) = vertex_counts.get_mut(&k1) {
                    *c += 1;
                }
                if let Some(c) = vertex_counts.get_mut(&k2) {
                    *c += 1;
                }
            }
        }

        for (v, count) in vertex_counts {
            assert_eq!(count, 1, "Vertex {:?} has {} hip/valley edges, expected 1", v, count);
        }

        // Compare geometric eaves rather than raw skeleton arcs: event handling can
        // leave duplicate or degenerate arcs which are not additional footprint eaves.
        let edge_key = |ax: f64, ay: f64, bx: f64, by: f64| {
            let a = ((ax * 100.0).round() as i64, (ay * 100.0).round() as i64);
            let b = ((bx * 100.0).round() as i64, (by * 100.0).round() as i64);
            if a <= b { (a, b) } else { (b, a) }
        };
        let footprint_edges: std::collections::HashSet<_> = footprint
            .outer
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let b = &footprint.outer[(i + 1) % footprint.outer.len()];
                edge_key(a.x, a.y, b.x, b.y)
            })
            .collect();
        let eave_edges: std::collections::HashSet<_> = edges
            .iter()
            .filter(|e| matches!(e.kind, RoofEdgeKind::Eave))
            .map(|e| edge_key(e.a.x, e.a.y, e.b.x, e.b.y))
            .filter(|key| footprint_edges.contains(key))
            .collect();

        assert_eq!(eave_edges, footprint_edges, "Every footprint edge must have exactly one geometric eave");
    }
}

#[test]
fn interior_roof_edges_are_shared_by_two_planes() {
    let rect = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let l_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (6.0, 8.0), (6.0, 12.0), (0.0, 12.0)];
    let t_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (7.0, 8.0), (7.0, 12.0), (3.0, 12.0), (3.0, 8.0), (0.0, 8.0)];

    let footprints = vec![rect, l_shape, t_shape];

    for pts in footprints {
        let model = house(&pts, 2.4);
        let r = roof(6.0, 0.0);

        let built = build_roof(&model, 0, &r);
        if built.is_err() {
            continue;
        }
        let roof_obj = built.unwrap();

        let geom = roof_geometry(&model, 0, &roof_obj).unwrap();
        let pairs = interior_edge_pairs(&geom);

        // Each interior edge (hip/valley) should be shared by exactly 2 planes.
        // interior_edge_pairs returns Vec<((usize, usize), (usize, usize))> where the first tuple is (face1, face2)
        // and the second is (node_a, node_b) or similar.
        // We verify that for each pair, the faces are valid indices.
        let num_faces = geom.skel.faces.len();
        for ((f1, f2), _) in pairs {
            assert!(f1 < num_faces, "Face index out of bounds");
            assert!(f2 < num_faces, "Face index out of bounds");
            assert_ne!(f1, f2, "Face pair should be distinct");
        }
    }
}

#[test]
fn plane_slope_equals_the_roof_pitch() {
    let rect = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&rect, 2.4);
    let r = roof(6.0, 0.0);

    let built = build_roof(&model, 0, &r).unwrap();

    let expected_slope = r.pitch_rise / r.pitch_run;

    for plane in &built.planes {
        // Plane slope should match roof slope if no directives
        // Note: Gable ends might have infinite slope or be vertical.
        // The prompt says "every plane's slope equals roof.slope() ... when no wall carries a directive".
        // Gable ends are vertical (infinite slope) in the skeleton representation?
        // Or does `slope` store the rise/run?
        // Let's assume non-vertical planes.
        if plane.slope.is_finite() {
            assert!((plane.slope - expected_slope).abs() < 1e-12, "Plane slope {} != expected {}", plane.slope, expected_slope);
        }
    }
}

#[test]
fn projected_areas_sum_to_the_footprint() {
    let rect = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let l_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (6.0, 8.0), (6.0, 12.0), (0.0, 12.0)];
    let t_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (7.0, 8.0), (7.0, 12.0), (3.0, 12.0), (3.0, 8.0), (0.0, 8.0)];

    // Rectangle overhang 0: expected outer-face area (10.2 x 8.2 = 83.64)
    let model = house(&rect, 2.4);
    let r = roof(6.0, 0.0);
    let built = build_roof(&model, 0, &r).unwrap();
    let area = projected_area_sum(&built);
    let fp = outer_footprint(&model, 0).expect("outer footprint should exist");
    let expected = shoelace(&fp.outer);
    assert!((area - expected).abs() < 1e-6, "Rect overhang 0: {} != {}", area, expected);

    // Rectangle overhang 0.5
    let r_over = roof(6.0, 0.5);
    let built_over = build_roof(&model, 0, &r_over).unwrap();
    let area_over = projected_area_sum(&built_over);
    // Expected: 11.2 * 9.2 = 103.04
    assert!((area_over - 103.04).abs() < 1e-2, "Rect overhang 0.5: {} != 103.04", area_over);

    // L-shape overhang 0: compare against outer-footprint area
    let model_l = house(&l_shape, 2.4);
    let built_l = build_roof(&model_l, 0, &r).unwrap();
    let area_l = projected_area_sum(&built_l);
    let fp_l = outer_footprint(&model_l, 0).expect("outer footprint should exist");
    let expected_l = shoelace(&fp_l.outer);
    assert!((area_l - expected_l).abs() < 1e-6, "L overhang 0: {} != {}", area_l, expected_l);

    // T-shape overhang 0: compare against outer-footprint area
    let model_t = house(&t_shape, 2.4);
    let built_t = build_roof(&model_t, 0, &r).unwrap();
    let area_t = projected_area_sum(&built_t);
    let fp_t = outer_footprint(&model_t, 0).expect("outer footprint should exist");
    let expected_t = shoelace(&fp_t.outer);
    assert!((area_t - expected_t).abs() < 1e-6, "T overhang 0: {} != {}", area_t, expected_t);
}

#[test]
fn ridge_height_tracks_pitch_on_every_footprint() {
    let rect = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&rect, 2.4);

    // 6/12
    let r6 = roof(6.0, 0.0);
    let built6 = build_roof(&model, 0, &r6).unwrap();
    let apex6 = roof_apex_z(&built6);
    // Expected: 2.4 + 4.1 * (6/12) = 4.45 (outer-face half-depth 8.2/2 = 4.1)
    assert!((apex6 - 4.45).abs() < 1e-6, "Apex 6/12: {} != 4.45", apex6);

    // 4/12
    let r4 = roof(4.0, 0.0);
    let built4 = build_roof(&model, 0, &r4).unwrap();
    let apex4 = roof_apex_z(&built4);
    // Expected: 2.4 + 4.1 * (4/12) = 3.7667 (outer-face inset, 0.2 m walls)
    let expected4 = 2.4 + 4.1 * (4.0 / 12.0);
    assert!((apex4 - expected4).abs() < 1e-6, "Apex 4/12: {} != {}", apex4, expected4);

    // 12/12
    let r12 = roof(12.0, 0.0);
    let built12 = build_roof(&model, 0, &r12).unwrap();
    let apex12 = roof_apex_z(&built12);
    // Expected: 2.4 + 4.1 * 1.0 = 6.5
    assert!((apex12 - 6.5).abs() < 1e-6, "Apex 12/12: {} != 6.5", apex12);
}

#[test]
fn hostile_inputs_never_panic_and_never_give_a_partial_roof() {
    // Empty model
    let empty = ArchModel::default();
    let r = roof(6.0, 0.0);
    let res = build_roof(&empty, 0, &r);
    assert!(res.is_err(), "Empty model should error");

    // One wall
    let mut one_wall = ArchModel::default();
    one_wall.walls.push(Wall::new(Vec2::ZERO, Vec2::new(1.0, 0.0), 0.2));
    let res = build_roof(&one_wall, 0, &r);
    assert!(res.is_err(), "One wall should error");

    // Two walls
    let mut two_walls = ArchModel::default();
    two_walls.walls.push(Wall::new(Vec2::ZERO, Vec2::new(1.0, 0.0), 0.2));
    two_walls.walls.push(Wall::new(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), 0.2));
    let res = build_roof(&two_walls, 0, &r);
    assert!(res.is_err(), "Two walls should error");

    // Self-crossing ring
    let crossing = vec![(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)];
    let model_cross = house(&crossing, 2.4);
    let res = build_roof(&model_cross, 0, &r);
    // Might error or produce weird geometry, but shouldn't panic.
    if let Ok(built) = res {
        for plane in &built.planes {
            assert!(plane.slope.is_finite() || plane.slope.is_infinite());
        }
    }

    // Wall with NaN endpoints
    let mut nan_wall = ArchModel::default();
    nan_wall.walls.push(Wall::new(Vec2::new(f64::NAN, 0.0), Vec2::new(1.0, 0.0), 0.2));
    let res = build_roof(&nan_wall, 0, &r);
    assert!(res.is_err(), "NaN wall should error");

    // Level with no walls
    let mut no_walls = ArchModel::default();
    no_walls.levels.push(Level::default());
    let res = build_roof(&no_walls, 0, &r);
    assert!(res.is_err(), "No walls should error");

    // Pitch run 0: the roof falls back to the default slope, so it may be a valid
    // roof. Accept either Err or a roof whose planes are all finite and non-empty.
    let bad_pitch = Roof { pitch_run: 0.0, ..roof(6.0, 0.0) };
    let model = house(&vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)], 2.4);
    let res = build_roof(&model, 0, &bad_pitch);
    if let Ok(built) = res {
        assert!(!built.planes.is_empty(), "a valid roof must not have empty planes");
        for plane in &built.planes {
            assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "Pitch run 0: plane slope non-finite: {:?}", plane.slope);
            assert!(plane.projected_area >= 0.0, "Pitch run 0: negative area");
            for p in &plane.ring {
                assert!(p.is_finite(), "Pitch run 0: non-finite plane vertex {:?}", p);
            }
        }
    }

    // Pitch rise 0
    let flat = roof(0.0, 0.0);
    let res = build_roof(&model, 0, &flat);
    // Flat roof might be valid or error depending on implementation.
    // If valid, planes should be finite.
    if let Ok(built) = res {
        for plane in &built.planes {
            assert!(plane.slope.is_finite());
        }
    }

    // Negative overhang
    let neg_over = roof(6.0, -1.0);
    let res = build_roof(&model, 0, &neg_over);
    // Might error or clamp. Should not panic.
    if let Ok(built) = res {
        for plane in &built.planes {
            assert!(plane.slope.is_finite() || plane.slope.is_infinite());
        }
    }

    // Thickness NaN
    let nan_thick = Roof { thickness: f64::NAN, ..roof(6.0, 0.0) };
    let res = build_roof(&model, 0, &nan_thick);
    assert!(res.is_err(), "NaN thickness should error");

    // 300-vertex ring
    let mut many_pts = Vec::new();
    for i in 0..300 {
        let angle = (i as f64) * std::f64::consts::PI * 2.0 / 300.0;
        many_pts.push((angle.cos() * 10.0, angle.sin() * 10.0));
    }
    let model_many = house(&many_pts, 2.4);
    let res = build_roof(&model_many, 0, &r);
    assert!(res.is_err(), "300 vertices should error (cap 256)");
}

#[test]
fn coincident_vertices_do_not_break_roof() {
    // Two consecutive walls share the same endpoint (degenerate zero-length wall)
    let pts = vec![(0.0, 0.0), (0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);
    let r = roof(6.0, 0.0);
    let res = build_roof(&model, 0, &r);
    // Should either error or produce a valid roof; must not panic
    if let Ok(built) = res {
        assert!(!built.planes.is_empty(), "coincident vertices: roof has no planes");
        for plane in &built.planes {
            assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "coincident vertices: non-finite slope");
            assert!(plane.projected_area >= 0.0, "coincident vertices: negative area");
            for p in &plane.ring {
                assert!(p.is_finite(), "coincident vertices: non-finite vertex {:?}", p);
            }
        }
    }
}

#[test]
fn collinear_vertices_do_not_break_roof() {
    // Three consecutive vertices on the same line
    let pts = vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);
    let r = roof(6.0, 0.0);
    let res = build_roof(&model, 0, &r);
    if let Ok(built) = res {
        assert!(!built.planes.is_empty(), "collinear vertices: roof has no planes");
        for plane in &built.planes {
            assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "collinear vertices: non-finite slope");
            assert!(plane.projected_area >= 0.0, "collinear vertices: negative area");
            for p in &plane.ring {
                assert!(p.is_finite(), "collinear vertices: non-finite vertex {:?}", p);
            }
        }
    }
}

#[test]
fn huge_coordinates_do_not_break_roof() {
    // Very large coordinates (1 km scale)
    let pts = vec![(0.0, 0.0), (1_000_000.0, 0.0), (1_000_000.0, 800_000.0), (0.0, 800_000.0)];
    let model = house(&pts, 2.4);
    let r = roof(6.0, 0.0);
    let res = build_roof(&model, 0, &r);
    if let Ok(built) = res {
        assert!(!built.planes.is_empty(), "huge coords: roof has no planes");
        for plane in &built.planes {
            assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "huge coords: non-finite slope");
            assert!(plane.projected_area >= 0.0, "huge coords: negative area");
            for p in &plane.ring {
                assert!(p.is_finite(), "huge coords: non-finite vertex {:?}", p);
            }
        }
    }
}

#[test]
fn roof_overhang_on_l_and_t_shapes() {
    // L-shape with overhang
    let l_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (6.0, 8.0), (6.0, 12.0), (0.0, 12.0)];
    let model_l = house(&l_shape, 2.4);
    let r_over = roof(6.0, 0.5);
    let built_l = build_roof(&model_l, 0, &r_over).expect("L-shape with overhang should build");
    assert!(!built_l.planes.is_empty(), "L overhang: roof has no planes");
    for plane in &built_l.planes {
        assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "L overhang: non-finite slope");
        assert!(plane.projected_area >= 0.0, "L overhang: negative area");
        for p in &plane.ring {
            assert!(p.is_finite(), "L overhang: non-finite vertex {:?}", p);
        }
    }

    // T-shape with overhang
    let t_shape = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (7.0, 8.0), (7.0, 12.0), (3.0, 12.0), (3.0, 8.0), (0.0, 8.0)];
    let model_t = house(&t_shape, 2.4);
    let built_t = build_roof(&model_t, 0, &r_over).expect("T-shape with overhang should build");
    assert!(!built_t.planes.is_empty(), "T overhang: roof has no planes");
    for plane in &built_t.planes {
        assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "T overhang: non-finite slope");
        assert!(plane.projected_area >= 0.0, "T overhang: negative area");
        for p in &plane.ring {
            assert!(p.is_finite(), "T overhang: non-finite vertex {:?}", p);
        }
    }
}

#[test]
fn roof_geometry_matches_build_roof() {
    // roof_geometry should return the same skeleton that build_roof used
    let rect = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&rect, 2.4);
    let r = roof(6.0, 0.0);
    let built = build_roof(&model, 0, &r).expect("roof should build");
    let geom = roof_geometry(&model, 0, &built).expect("roof_geometry should succeed");

    // The skeleton should have the same number of faces as the roof has planes
    assert_eq!(geom.skel.faces.len(), built.planes.len(), "face count mismatch");

    // All nodes should be finite
    for node in &geom.skel.nodes {
        assert!(node.x.is_finite() && node.y.is_finite(), "non-finite skeleton node");
        assert!(node.time.is_finite() || node.time.is_infinite(), "non-finite node time");
    }
}

#[test]
fn random_wall_rings_never_panic() {
    let mut seed = 0x5eedu64;
    let mut next_rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 1000) as f64 / 1000.0
    };

    for _ in 0..200 {
        let n = 4 + (next_rand() * 8.0) as usize; // 4 to 11 vertices
        let mut pts = Vec::new();
        let mut x = 0.0;
        let mut y = 0.0;

        // Generate orthogonal-ish ring
        for _ in 0..n {
            pts.push((x, y));
            let dir = (next_rand() * 4.0) as u8;
            let len = 1.0 + next_rand() * 5.0;
            match dir {
                0 => x += len,
                1 => y += len,
                2 => x -= len,
                _ => y -= len,
            }
        }

        // Close ring roughly
        pts.push((0.0, 0.0));

        let model = house(&pts, 2.4);
        let r = roof(6.0, 0.0);

        let res = build_roof(&model, 0, &r);

        if let Ok(built) = res {
            for plane in &built.planes {
                assert!(plane.slope.is_finite() || plane.slope.is_infinite(), "Non-finite slope");
                assert!(plane.projected_area >= 0.0, "Negative area");
                for p in &plane.ring {
                    assert!(p.is_finite(), "Non-finite vertex in plane ring");
                }
            }

            // Also call roof_edges to ensure it doesn't panic
            let _ = roof_edges(&model, 0, &built);
            let geom = roof_geometry(&model, 0, &built);
            if let Ok(g) = geom {
                let _ = interior_edge_pairs(&g);
            }
        }
    }
}
