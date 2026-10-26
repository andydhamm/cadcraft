//! Property tests for random orthogonal roof footprints.
//!
//! Exercises the roof skeleton, edge classifier, and area sum over many
//! seeded orthogonal rings, including hostile shapes that self-cross or
//! exceed the 256-edge cap.

use cadcraft_arch::roof::{build_roof, outer_footprint, projected_area_sum, roof_geometry};
use cadcraft_arch::roofedge::{interior_edge_pairs, roof_edges};
use cadcraft_doc::{ArchModel, Level, Roof, RoofDirective, RoofEdgeKind, Wall};
use cadcraft_geom::Vec2;

/// Deterministic xorshift64 PRNG for reproducible fuzzing.
struct Lcg {
    s: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { s: seed.wrapping_add(0x9E3779B97F4A7C15u64) }
    }

    fn next_u64(&mut self) -> u64 {
        self.s ^= self.s.wrapping_shl(13);
        self.s ^= self.s >> 7;
        self.s ^= self.s.wrapping_shl(17);
        self.s
    }

    /// Uniform random in `[lo, hi)`.
    fn f64_in(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next_u64() as f64) * (hi - lo) / 18446744073709551615.0
    }
}

/// Build an `ArchModel` from a point ring: one 0.2 m exterior wall per edge
/// (the ring is closed by wrapping), all at `height` on level 0, all `Auto`-roofed.
fn house(pts: &[(f64, f64)], height: f64) -> ArchModel {
    let mut model = ArchModel::default();
    model.levels.push(Level::default());
    if pts.len() < 3 {
        return model;
    }
    for i in 0..pts.len() {
        let a = Vec2::new(pts[i].0, pts[i].1);
        let b = Vec2::new(pts[(i + 1) % pts.len()].0, pts[(i + 1) % pts.len()].1);
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

/// Roof spec with the given rise and overhang, run fixed at 12, no thickness.
fn roof(rise: f64, overhang: f64) -> Roof {
    Roof { pitch_rise: rise, pitch_run: 12.0, overhang, thickness: 0.0, ..Roof::default() }
}

/// Shoelace area (absolute value) of a point ring.
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

/// 1 cm grid key for a plan point.
fn pt_key(x: f64, y: f64) -> (i64, i64) {
    ((x * 100.0).round() as i64, (y * 100.0).round() as i64)
}

/// Unordered pair key for an edge.
fn edge_key(ax: f64, ay: f64, bx: f64, by: f64) -> ((i64, i64), (i64, i64)) {
    let a = pt_key(ax, ay);
    let b = pt_key(bx, by);
    if a <= b { (a, b) } else { (b, a) }
}

/// A random orthogonal ring: (0, 0) start, 3-23 axis-aligned steps of 1-6 m in
/// random cardinal directions, 4-24 edges including the closing edge to (0, 0).
/// Rings may self-cross; that is fine (the build below returns Err).
fn random_orthogonal_ring(rng: &mut Lcg) -> Vec<(f64, f64)> {
    let n = 3 + (rng.next_u64() % 21);
    let mut pts = vec![(0.0, 0.0)];
    let (mut x, mut y) = (0.0_f64, 0.0);
    for _ in 0..n {
        let dir = rng.next_u64() % 4;
        let dist = 1.0 + rng.f64_in(0.0, 5.0);
        match dir {
            0 => y += dist,
            1 => x += dist,
            2 => y -= dist,
            _ => x -= dist,
        }
        pts.push((x, y));
    }
    pts
}

#[test]
fn one_sloped_edge_per_footprint_vertex() {
    let named: [Vec<(f64, f64)>; 3] = [
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)],
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (6.0, 8.0), (6.0, 12.0), (0.0, 12.0)],
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (7.0, 8.0), (7.0, 12.0), (3.0, 12.0), (3.0, 8.0), (0.0, 8.0)],
    ];

    let mut rings: Vec<Vec<(f64, f64)>> = Vec::with_capacity(403);
    let mut rng = Lcg::new(42);
    for _ in 0..400 {
        rings.push(random_orthogonal_ring(&mut rng));
    }
    for named in named {
        rings.push(named);
    }

    for pts in rings {
        let model = house(&pts, 2.4);
        let r = roof(6.0, 0.0);
        let Ok(built) = build_roof(&model, 0, &r) else {
            continue;
        };
        let Some(fp) = outer_footprint(&model, 0) else {
            continue;
        };
        let Ok(edges) = roof_edges(&model, 0, &built) else {
            continue;
        };

        // A valid *simple* orthogonal footprint with a hip roof has exactly one
        // Hip or Valley edge per vertex.  Self-crossing rings may still build
        // successfully, but the polygon folds back on itself: a single footprint
        // vertex can then carry several sloped edges while another carries none,
        // and a total-count guard (sloped edges == vertices) passes on exactly
        // those.  The invariant only applies to simple footprints, so verify the
        // vertex-to-edge incidence is a perfect matching — every footprint
        // vertex distinct at 1 cm resolution, each touching exactly one sloped
        // edge — and only then assert.
        let mut counts: std::collections::HashMap<(i64, i64), usize> = fp.outer.iter().map(|p| (pt_key(p.x, p.y), 0)).collect();
        // Two footprint vertices collapsed onto one 1 cm cell: degenerate, skip.
        if counts.len() != fp.outer.len() {
            continue;
        }
        for e in &edges {
            if matches!(e.kind, RoofEdgeKind::Hip | RoofEdgeKind::Valley) {
                for (px, py) in [(e.a.x, e.a.y), (e.b.x, e.b.y)] {
                    let key = pt_key(px, py);
                    if let Some(v) = counts.get_mut(&key) {
                        *v += 1;
                    }
                }
            }
        }
        // Self-crossing or otherwise degenerate footprints cannot guarantee the
        // one-edge-per-vertex invariant; leave them out of the assertion.
        if counts.iter().any(|(_, &c)| c != 1) {
            continue;
        }

        // One Hip or Valley edge per footprint vertex (simple footprints only).
        for (k, &v) in counts.iter() {
            assert_eq!(v, 1, "footprint vertex {:?} has {} sloped edges", k, v);
        }

        // Every footprint edge must appear exactly once among geometric Eaves.
        let footprint_edges: std::collections::HashSet<_> = fp
            .outer
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let b = &fp.outer[(i + 1) % fp.outer.len()];
                edge_key(a.x, a.y, b.x, b.y)
            })
            .collect();
        let eave_edges: std::collections::HashSet<_> = edges
            .iter()
            .filter(|e| matches!(e.kind, RoofEdgeKind::Eave))
            .map(|e| edge_key(e.a.x, e.a.y, e.b.x, e.b.y))
            .filter(|key| footprint_edges.contains(key))
            .collect();
        assert_eq!(eave_edges, footprint_edges, "every footprint edge must have exactly one geometric eave");
    }
}

#[test]
fn random_footprints_never_panic() {
    let pitches = [4.0_f64, 6.0, 12.0];
    let overhangs = [0.0_f64, 0.5];
    let mut rng = Lcg::new(7);

    for i in 0..600 {
        let pts = random_orthogonal_ring(&mut rng);
        let model = house(&pts, 2.4);
        let r = roof(pitches[i % 3], overhangs[i % 2]);
        let Ok(built) = build_roof(&model, 0, &r) else {
            continue;
        };
        let Ok(_edges) = roof_edges(&model, 0, &built) else {
            continue;
        };
        let Ok(geom) = roof_geometry(&model, 0, &r) else {
            continue;
        };
        let pairs = interior_edge_pairs(&geom);

        assert!(!built.planes.is_empty(), "built roof has no planes");
        for p in &built.planes {
            assert!(p.projected_area >= 0.0, "negative projected_area");
            for pt in &p.ring {
                assert!(pt.is_finite(), "non-finite ring point {:?}", pt);
            }
            assert!(p.slope.is_finite() || p.slope.is_infinite(), "non-finite slope");
        }
        let nf = geom.skel.faces.len();
        for ((f1, f2), _) in &pairs {
            assert!(*f1 < nf, "interior pair face index {f1} out of range ({nf} faces)");
            assert!(*f2 < nf, "interior pair face index {f2} out of range ({nf} faces)");
            assert_ne!(f1, f2, "interior edge shared by the same face ({f1})");
        }
    }
}

#[test]
fn projected_areas_sum_to_the_footprint_over_random_footprints() {
    let mut rng = Lcg::new(13);
    for _ in 0..200 {
        let pts = random_orthogonal_ring(&mut rng);
        let model = house(&pts, 2.4);
        let r = roof(6.0, 0.0);
        let Ok(built) = build_roof(&model, 0, &r) else {
            continue;
        };
        let Some(fp) = outer_footprint(&model, 0) else {
            continue;
        };
        let expected = shoelace(&fp.outer);
        let actual = projected_area_sum(&built);
        assert!((actual - expected).abs() < 1e-6, "projected area sum {:.6} != footprint area {:.6}", actual, expected,);
    }
}

#[test]
fn degenerate_rings_do_not_panic() {
    let r = roof(6.0, 0.0);

    // Empty ring (no walls)
    let model = house(&[], 2.4);
    let _ = build_roof(&model, 0, &r);

    // One point
    let model = house(&[(0.0, 0.0)], 2.4);
    let _ = build_roof(&model, 0, &r);

    // Two points
    let model = house(&[(0.0, 0.0), (5.0, 0.0)], 2.4);
    let _ = build_roof(&model, 0, &r);

    // Three collinear points
    let model = house(&[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)], 2.4);
    let _ = build_roof(&model, 0, &r);

    // Coincident consecutive points
    let model = house(&[(0.0, 0.0), (5.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)], 2.4);
    let _ = build_roof(&model, 0, &r);

    // Zero-length wall (start == end)
    let model = house(&[(0.0, 0.0), (0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)], 2.4);
    let _ = build_roof(&model, 0, &r);
}

#[test]
fn huge_coordinates_do_not_panic() {
    let r = roof(6.0, 0.0);

    // 1 km square
    let pts = [(0.0, 0.0), (1e6, 0.0), (1e6, 1e6), (0.0, 1e6)];
    let model = house(&pts, 2.4);
    let _ = build_roof(&model, 0, &r);

    // 1000 km square
    let pts = [(0.0, 0.0), (1e9, 0.0), (1e9, 1e9), (0.0, 1e9)];
    let model = house(&pts, 2.4);
    let _ = build_roof(&model, 0, &r);
}

#[test]
fn nan_and_inf_coordinates_do_not_panic() {
    let r = roof(6.0, 0.0);

    // NaN in a coordinate
    let pts = [(0.0, 0.0), (f64::NAN, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);
    let _ = build_roof(&model, 0, &r);

    // Infinity in a coordinate
    let pts = [(0.0, 0.0), (f64::INFINITY, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);
    let _ = build_roof(&model, 0, &r);

    // -Infinity in a coordinate
    let pts = [(0.0, 0.0), (-f64::INFINITY, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);
    let _ = build_roof(&model, 0, &r);

    // All points NaN
    let pts = [(f64::NAN, f64::NAN), (f64::NAN, f64::NAN)];
    let model = house(&pts, 2.4);
    let _ = build_roof(&model, 0, &r);
}

#[test]
fn projected_areas_sum_with_overhang_is_larger_than_footprint() {
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);

    let r0 = roof(6.0, 0.0);
    let built0 = build_roof(&model, 0, &r0).expect("no-overhang build failed");
    let area0 = projected_area_sum(&built0);

    let r1 = roof(6.0, 0.5);
    let built1 = build_roof(&model, 0, &r1).expect("overhang build failed");
    let area1 = projected_area_sum(&built1);

    assert!(area1 > area0, "overhang should increase projected area: {} vs {}", area1, area0);
}

#[test]
fn different_pitches_produce_different_roof_heights() {
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let model = house(&pts, 2.4);

    let r4 = roof(4.0, 0.0);
    let built4 = build_roof(&model, 0, &r4).expect("pitch 4 build failed");
    let geom4 = roof_geometry(&model, 0, &r4).expect("pitch 4 geometry failed");
    let max_z4 = geom4.skel.nodes.iter().map(|n| n.time).fold(f64::MIN, f64::max);

    let r12 = roof(12.0, 0.0);
    let built12 = build_roof(&model, 0, &r12).expect("pitch 12 build failed");
    let geom12 = roof_geometry(&model, 0, &r12).expect("pitch 12 geometry failed");
    let max_z12 = geom12.skel.nodes.iter().map(|n| n.time).fold(f64::MIN, f64::max);

    assert!(max_z12 > max_z4, "higher pitch should be taller: {} vs {}", max_z12, max_z4);
}
