//! Property/fuzz tests for the wall join solver and neighbour stretch.
//!
//! Follows the seeded xorshift pattern from `geom/src/property_tests.rs` — no new
//! dependencies, only std. Each run is deterministic via the seed.

use cadcraft_arch::join::WallShape;
use cadcraft_arch::join::{solve_joins, stretch_wall};
use cadcraft_doc::geom::Vec2;
use cadcraft_doc::{ArchId, ArchModel, JoinKind, Wall, WallType};

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
/// Endpoints lie on a 20×20 m grid ([-10, 10) in each axis).  Thickness is
/// sampled in [0.05, 0.40).  About 5 % of coordinates are replaced with `NaN`
/// or `INFINITY`.  10 % of walls are collinear with their neighbour (end is
/// offset along the start direction), and 5 % overlap by up to 2.5 m, to
/// exercise the collinear/overlap join paths.
fn build_random_model(seed: u64, num_walls: usize) -> (ArchModel, Vec<ArchId>) {
    let mut rng = Xorshift64::new(seed);
    let mut model = ArchModel::new();
    let mut ids = Vec::with_capacity(num_walls);

    for _ in 0..num_walls {
        // 5 % chance of injecting NaN or inf in each coordinate.
        let corrupt = rng.next_u64() % 20 == 0;

        let pick = |rng: &mut Xorshift64| {
            if corrupt && rng.next_u64() % 3 == 0 {
                if rng.next_u64() % 2 == 0 { f64::NAN } else { f64::INFINITY }
            } else {
                (rng.next_u64() as f64) % 20.0 - 10.0
            }
        };

        let x1 = pick(&mut rng);
        let y1 = pick(&mut rng);
        let x2 = pick(&mut rng);
        let y2 = pick(&mut rng);

        let thickness = 0.05 + (rng.next_u64() as f64) % 35.0 / 100.0; // [0.05, 0.40)
        let height = 2.4 + (rng.next_u64() as f64) % 0.5; // [2.4, 2.9)
        let exterior = rng.next_u64() % 3 == 0;

        // 10 % collinear: end is along the start→end direction by a small amount.
        // 5 % overlap: end is shifted perpendicular to the start→end vector.
        let collinear = rng.next_u64() % 10 == 0;
        let overlap = rng.next_u64() % 20 == 0;
        let shift = (rng.next_u64() as f64) % 5.0 - 2.5;

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
            base_level: 0.0,
            to_ceiling: true,
            exterior,
            wall_type: WallType::default(),
            join_start: JoinKind::default(),
            join_end: JoinKind::default(),
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
    let w = model.walls(&shape.id).expect("wall id missing from model");
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
        let (model, ids) = build_random_model(seed, 50);

        // solve_joins: must return without panicking on hostile input.
        let _shapes = solve_joins(&model);

        // stretch_wall: transitive BFS over a random pair of walls.
        if ids.len() >= 2 {
            let target = Vec2::new(1.0, 1.0);
            let _affected = stretch_wall(&model, ids[0], ids[1], target);
        }
    }
}

/// 2000 random wall graphs; every resolved ring must be finite, ≥ 4 vertices,
/// and stay inside the wall's own bounds envelope (± tolerance).
#[test]
fn random_wall_rings_stay_in_their_envelope() {
    const TOL: f64 = 1.0;
    for seed in 0..2000u64 {
        let (model, _ids) = build_random_model(seed, 50);
        let shapes = solve_joins(&model);

        let rings: Vec<&Vec<Vec2>> = shapes.iter().map(|s| &s.ring).collect();
        assert_rings_well_formed(&rings);

        for shape in &shapes {
            assert_in_envelope(&model, shape, TOL);
        }
    }
}
