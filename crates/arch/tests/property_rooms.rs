use std::cmp::Ordering;

use cadcraft_arch::join::solve_joins;
use cadcraft_arch::label::label_point;
use cadcraft_arch::poly::{is_simple, region_area};
use cadcraft_arch::room::detect_rooms;
use cadcraft_doc::{ArchModel, Room, Wall};
use cadcraft_geom::Vec2;

/// Tiny deterministic LCG (Numerical-Rewards constants).
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    /// Uniform random in `[lo, hi)`.
    fn rand_f64(&mut self, lo: f64, hi: f64) -> f64 {
        let t = self.next_u64() as f64 / 18_446_744_073_709_551_616.0;
        lo + t * (hi - lo)
    }
    /// Pick an element from `list`.
    fn rand_from<T: AsRef<[f64]>>(&mut self, list: T) -> f64 {
        list.as_ref()[self.next_u64() as usize % list.as_ref().len()]
    }
}

fn push_wall(model: &mut ArchModel, a: Vec2, b: Vec2, t: f64, exterior: bool) {
    let divider = t == 0.0;
    model.push_wall(Wall { a, b, thickness: t, exterior, divider, ..Default::default() });
}

/// Add a closed axis-aligned rectangle (4 walls).
fn add_rectangle(model: &mut ArchModel, rng: &mut Lcg) {
    let x0 = rng.rand_f64(-20.0, 15.0);
    let y0 = rng.rand_f64(-20.0, 15.0);
    let w = rng.rand_f64(0.5, 5.0);
    let h = rng.rand_f64(0.5, 5.0);
    let x1 = x0 + w;
    let y1 = y0 + h;
    let t = rng.rand_from(&[0.0f64, 0.05, 0.1, 0.2, 0.3]);
    push_wall(model, Vec2::new(x0, y0), Vec2::new(x1, y0), t, false);
    push_wall(model, Vec2::new(x1, y0), Vec2::new(x1, y1), t, false);
    push_wall(model, Vec2::new(x1, y1), Vec2::new(x0, y1), t, false);
    push_wall(model, Vec2::new(x0, y1), Vec2::new(x0, y0), t, false);
}

/// Add one random axis-aligned stray segment.
fn add_segment(model: &mut ArchModel, rng: &mut Lcg) {
    let t = rng.rand_from(&[0.0f64, 0.05, 0.1, 0.2, 0.3]);
    if rng.next_u64() % 2 == 0 {
        // horizontal
        let y = rng.rand_f64(-20.0, 20.0);
        let x0 = rng.rand_f64(-20.0, 20.0);
        let x1 = rng.rand_f64(-20.0, 20.0);
        push_wall(model, Vec2::new(x0, y), Vec2::new(x1, y), t, false);
    } else {
        // vertical
        let x = rng.rand_f64(-20.0, 20.0);
        let y0 = rng.rand_f64(-20.0, 20.0);
        let y1 = rng.rand_f64(-20.0, 20.0);
        push_wall(model, Vec2::new(x, y0), Vec2::new(x, y1), t, false);
    }
}

#[test]
fn random_wall_graphs_never_panic_in_room_detection() {
    for seed in 0..40u64 {
        let mut rng = Lcg::new(seed);
        let mut model = ArchModel::default();

        // Build a random model of 2..=12 axis-aligned walls (mix of
        // closed rectangles and stray segments).
        let total = 2 + (rng.next_u64() % 11) as usize;
        let mut placed = 0usize;
        while placed < total {
            if placed + 4 <= total && rng.next_u64() % 10 < 4 {
                add_rectangle(&mut model, &mut rng);
                placed += 4;
            } else {
                add_segment(&mut model, &mut rng);
                placed += 1;
            }
        }

        // Room detection must never panic.
        let rooms = detect_rooms(&model);
        assert!(rooms.len() <= cadcraft_doc::MAX_ROOMS, "rooms.len() {} > MAX_ROOMS", rooms.len());

        for room in &rooms {
            // Every room polygon is closed and simple.
            assert!(is_simple(&room.polygon), "non-simple room polygon (seed {:?})", seed);
            // Every hole ring is simple.
            for hole in &room.holes {
                assert!(is_simple(hole), "non-simple hole (seed {:?})", seed);
            }
            // Area must exceed MIN_ROOM_AREA.
            let area = region_area(&room.polygon, &room.holes);
            assert!(area > cadcraft_doc::MIN_ROOM_AREA, "room area {} <= MIN_ROOM_AREA (seed {:?})", area, seed);
            // label_point returns a valid point inside the polygon.
            let label = label_point(&room.polygon, &room.holes);
            assert!(label.is_some(), "label_point returned None (seed {:?})", seed);
            let p = label.unwrap();
            assert!(cadcraft_geom::point_in_polygon(&room.polygon, p), "label point not in polygon (seed {:?})", seed);
        }

        // solve_joins must never panic on the same model.
        let _ = solve_joins(&model);
    }
}
