//! Property/fuzz tests for the wall join solver and neighbour stretch.
//!
//! Follows the seeded xorshift pattern from `geom/src/property_tests.rs` — no new
//! dependencies, only std. Each run is deterministic via the seed.

use std::collections::HashSet;

use cadcraft_arch::join::End;
use cadcraft_arch::join::WallShape;
use cadcraft_arch::join::{solve_joins, stretch_wall};
use cadcraft_arch::wall;
use cadcraft_doc::geom::Vec2;
use cadcraft_doc::{ArchId, ArchModel, Wall, WallType};

/// A minimal xorshift64* PRNG with a fixed non-zero initial state.
#[derive(Clone, Copy)]
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// Create a new PRNG from a 64-bit seed. A zero seed is mapped to a
    /// constant so the stream is well-defined for every seed value.
    fn new(seed: u64) -> Self {
        let s = if seed == 0 { 0xdeadbeefcafebabe } else { seed };
        Self { state: s }
    }

    /// Next 64-bit pseudo-random value.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// A uniform float in `[min, max)` (inclusive of min, exclusive of max).
    fn next_f64(&mut self, min: f64, max: f64) -> f64 {
        // 2^64 - 1 so the top bit is not always set.
        let unit = (self.next_u64() as f64) / 18_446_744_073_709_551_615.0;
        min + (max - min) * unit
    }
}

/// Build a random `ArchModel` with the given number of walls.
///
/// Endpoints lie in [-10, 10) on each axis; half of the coordinates are snapped
/// to the 1 m grid so ends really coincide.  Thickness is sampled in
/// [0.05, 0.40) and height in [2.4, 2.9).  About 5 % of coordinates are replaced
/// with `NaN` or `INFINITY`.  10 % of walls are collinear with their neighbour
/// (end is offset along the start direction), and 5 % overlap by up to 2.5 m, to
/// exercise the collinear/overlap join paths.
fn build_random_model(seed: u64, num_walls: usize) -> (ArchModel, Vec<ArchId>) {
    let mut rng = Xorshift64::new(seed);
    let mut model = ArchModel::default();
    let mut ids = Vec::with_capacity(num_walls);

    for _ in 0..num_walls {
        // 5 % chance of injecting NaN or inf in each coordinate.
        let corrupt = rng.next_u64().is_multiple_of(20);

        let pick = |rng: &mut Xorshift64| -> f64 {
            let v = if corrupt && rng.next_u64().is_multiple_of(3) {
                if rng.next_u64().is_multiple_of(2) { f64::NAN } else { f64::INFINITY }
            } else {
                rng.next_f64(-10.0, 10.0)
            };
            // Snap half of the coordinates to the 1 m grid so ends really
            // coincide and L/T/collinear joins fire.
            if rng.next_u64().is_multiple_of(2) { v.round() } else { v }
        };

        let x1 = pick(&mut rng);
        let y1 = pick(&mut rng);
        let x2 = pick(&mut rng);
        let y2 = pick(&mut rng);

        let thickness = rng.next_f64(0.05, 0.40);
        let height = rng.next_f64(2.4, 2.9);
        let exterior = rng.next_u64().is_multiple_of(3);

        // 10 % collinear: end is along the start→end direction by a small amount.
        // 5 % overlap: end is shifted perpendicular to the start→end vector.
        let collinear = rng.next_u64().is_multiple_of(10);
        let overlap = rng.next_u64().is_multiple_of(20);
        let shift = rng.next_f64(-2.5, 2.5);

        let (ex, ey) = if collinear || overlap {
            let dx = x2 - x1;
            let dy = y2 - y1;
            let len = dx * dx + dy * dy;
            if len > 1e-12 {
                // Walk along the centre line (collinear) or perpendicular (overlap).
                if collinear { (x1 + dx * 0.1, y1 + dy * 0.1) } else { (-dy * shift / len.sqrt() + x1, dx * shift / len.sqrt() + y1) }
            } else {
                (x1 + shift, y1)
            }
        } else {
            (x2, y2)
        };

        let wall = Wall {
            start: Vec2::new(x1, y1),
            end: Vec2::new(ex, ey),
            thickness,
            height,
            to_ceiling: true,
            exterior,
            wall_type: "Default".to_string(),
            ..Default::default()
        };

        let id = model.add_wall(wall);
        ids.push(id);
    }

    (model, ids)
}

/// Verify every ring: finite vertices, ≥ 4 vertices.
fn assert_rings_well_formed(rings: &[&Vec<Vec2>]) {
    for (i, ring) in rings.iter().enumerate() {
        assert!(ring.len() >= 4, "ring {i} has only {len} vertices (expected >= 4)", len = ring.len());
        for (j, v) in ring.iter().enumerate() {
            assert!(v.is_finite(), "non-finite vertex ({i}:{j}) = {:?}", v);
        }
    }
}

/// Each vertex of a resolved shape must lie within the original wall's
/// bounding envelope (extended by `tol`).
fn assert_in_envelope(model: &ArchModel, shape: &WallShape, tol: f64) {
    let w = model.wall(shape.id).expect("wall id missing from model");
    let min_x = w.start.x.min(w.end.x) - w.thickness - tol;
    let max_x = w.end.x.max(w.start.x) + w.thickness + tol;
    let min_y = w.start.y.min(w.end.y) - w.thickness - tol;
    let max_y = w.end.y.max(w.start.y) + w.thickness + tol;

    for v in &shape.polygon {
        assert!(v.is_finite(), "non-finite vertex in shape {0}", shape.id);
        assert!(v.x >= min_x && v.x <= max_x, "vertex x {:.4} outside [{:.4}..{:.4}]", v.x, min_x, max_x);
        assert!(v.y >= min_y && v.y <= max_y, "vertex y {:.4} outside [{:.4}..{:.4}]", v.y, min_y, max_y);
    }
}

/// 2000 random wall graphs; `solve_joins` and `stretch_wall` must never
/// panic, not even on NaN/inf input.
#[test]
fn random_wall_graphs_never_panic() {
    for seed in 0..2000u64 {
        let (mut model, ids) = build_random_model(seed, 50);

        // solve_joins: must return without panicking on hostile input.
        let _shapes = solve_joins(&model);

        // stretch_wall: transitive BFS over a random pair of walls.
        if ids.len() >= 2 {
            let target = Vec2::new(1.0, 1.0);
            let _affected = stretch_wall(&mut model, ids[0], End::Start, target);
        }
    }
}

/// 2000 random wall graphs; every resolved ring must be finite, ≥ 4 vertices,
/// and stay inside the wall's own bounds envelope (± tolerance).
#[test]
fn random_wall_rings_stay_in_their_envelope() {
    // Tight tolerance: the join solver trims/extends a wall's centre line to meet
    // a neighbour, but only within its mitre limit (≤ 0.8 m) and at most 3.2 m
    // for a T reach. Anything beyond this would be a genuine spike, so this
    // still catches NaN/infinity and any polygon exploded off its own wall.
    const TOL: f64 = 3.5;
    for seed in 0..2000u64 {
        let (model, _ids) = build_random_model(seed, 50);
        let shapes = solve_joins(&model);

        let rings: Vec<&Vec<Vec2>> = shapes.iter().map(|s| &s.polygon).collect();
        assert_rings_well_formed(&rings);

        for shape in &shapes {
            assert_in_envelope(&model, shape, TOL);
        }
    }
}

/// 2000 random wall graphs; a wall with a valid polygon must never disappear
/// after `solve_joins`. Joins may reshape a wall, but they must not delete it:
/// every valid wall must still appear among the resolved shapes.
#[test]
fn random_wall_graphs_keep_every_valid_wall() {
    for seed in 0..2000u64 {
        let (model, ids) = build_random_model(seed, 50);
        let shapes = solve_joins(&model);

        let shape_ids: HashSet<ArchId> = shapes.iter().map(|s| s.id).collect();

        for id in &ids {
            let Some(w) = model.wall(*id) else {
                continue;
            };
            if wall::wall_polygon(w).is_some() {
                assert!(shape_ids.contains(&w.id), "wall {:?} has a valid polygon but no shape after solve_joins", w.id);
            }
        }
    }
}
