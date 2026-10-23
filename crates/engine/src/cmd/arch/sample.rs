//! `arch.sample` — inject a deterministic sample arch model (benchmark fixture).
//!
//! Replaces `Drawing::arch.walls` with a seeded xorshift64* wall chain (same
//! bit pattern as `apps/cadcraft-cli/src/main.rs::Rng`, seed
//! `0x9E37_79B9_7F4A_7C15`). The chain is connected: wall `i` starts where
//! wall `i-1` ended, the heading turns a right angle every `4 + (rng % 3)`
//! walls, length in [3.0, 8.9) drawing units, thickness alternating 0.2/0.1,
//! height 2.6, ids `w1..wN` (handles `arch_handle(i)`).
//!
//! The optional `openings` parameter (default `0`, capped at `MAX_OPENINGS`)
//! deterministically seeds openings on top of the chain: every 3rd wall gets
//! a hinged door (0.9 m × 2.1 m), and every 5th a sliding window (1.2 m ×
//! 1.2 m, sill 0.9 m), both at 40 % of the centre-line length. Walls
//! shorter than 2.0 m and any placement rejected by
//! [`cadcraft_arch::opening::validate_placement`] are skipped, so the command
//! never fails. Openings are pushed through `ArchModel::push_opening`, so
//! ids continue the shared counter after the wall ids. Same seed ⇒ same
//! output.
//!
//! This command *does* mutate the model and calls `touch_arch()`; it is
//! `.noundo()` only because it is a benchmark fixture, not a user-facing edit.

use serde_json::{Value, json};

use crate::Session;
use cadcraft_arch::opening::validate_placement;
use cadcraft_doc::arch::{ArchId, ArchModel, MAX_OPENINGS, Opening, OpeningKind};

/// Seeded xorshift64*, copied bit-for-bit from the CLI's `Rng`.
struct Rng(u64);

impl Rng {
    /// Advance the state and return the next 64-bit word.
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// The next [0.0, 1.0) value (same as the CLI's `next`).
    fn next(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Generate `n` connected walls. The chain is fully determined by `n` and the
/// fixed seed, so two fresh runs with the same `n` produce identical data.
fn generate_walls(n: usize) -> Vec<cadcraft_doc::Wall> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut walls: Vec<cadcraft_doc::Wall> = Vec::with_capacity(n);
    let mut pos = cadcraft_geom::Vec2::ZERO;
    let mut dir = 0usize; // 0:+x 1:+y 2:-x 3:-y (right turn = clockwise)
    // Right-angle turn every 4 + (rng % 3) walls (4..=6 walls per leg).
    let mut legs_left: usize = (4 + (rng.next_u64() % 3)) as usize;
    let mut k = 0usize; // 1-based wall index

    while walls.len() < n {
        // Length: 3.0 + (rng % 60) as f64 / 10.0 → [3.0, 8.9) in 0.1 steps.
        // Never zero with the seeded generator; if a degenerate value ever
        // comes back, skip it and retry with a fresh one, bounded retries.
        let mut length: f64 = 0.0;
        let mut tried = 0usize;
        while length <= 0.0 || !length.is_finite() || tried < 32 {
            length = 3.0 + ((rng.next() * 60.0).floor() as f64) / 10.0;
            tried += 1;
        }
        if !(length > 0.0) {
            length = 3.0;
        }

        let (dx, dy) = match dir {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
        let end = pos + cadcraft_geom::Vec2::new(dx * length, dy * length);

        walls.push(cadcraft_doc::Wall {
            id: cadcraft_doc::ArchId((k + 1) as u32),
            start: pos,
            end,
            thickness: if (k + 1) % 2 == 1 { 0.2 } else { 0.1 },
            height: 2.6,
            ..cadcraft_doc::Wall::default()
        });

        pos = end;
        k += 1;

        legs_left = legs_left.saturating_sub(1);
        if legs_left == 0 {
            dir = (dir + 1) % 4;
            legs_left = (4 + (rng.next_u64() % 3)) as usize;
        }
    }

    walls
}

/// Minimum wall length (m) for a sample opening to be seeded on a wall.
const MIN_SAMPLE_WALL: f64 = 2.0;

/// One opening attempt to seed: kind, finished width/height, sill, style.
type OpeningSpec = (OpeningKind, f64, f64, f64, &'static str);

/// Deterministically seed up to `max` openings on the given model.
///
/// Every 3rd wall (1-based) gets a hinged door (0.9 m × 2.1 m) at 40 % of the
/// centre-line length; every 5th gets a sliding window (1.2 m × 1.2 m, sill
/// 0.9 m) at the same 40 % mark. Walls shorter than [`MIN_SAMPLE_WALL`] and
/// any placement rejected by [`validate_placement`] are skipped (in particular,
/// the door and window on the same wall 15/30/… are at the same offset, so
/// the window's separation check rejects it). Returns the number actually
/// placed; never fails.
fn seed_openings(model: &mut ArchModel, max: usize) -> usize {
    const DOOR: OpeningSpec = (OpeningKind::Door, 0.9, 2.1, 0.0, "hinged");
    const WINDOW: OpeningSpec = (OpeningKind::Window, 1.2, 1.2, 0.9, "slider");

    let mut placed = 0usize;
    // Index-based loop: the only thing copied out is `wid` (a `Copy` ArchId)
    // and a plain `f64`, so no borrow of `model.walls` is live when
    // `model.push_opening(o)` mutates the model.
    for i in 0..model.walls.len() {
        if placed >= max {
            break;
        }
        let wid = model.walls[i].id;
        let length = model.walls[i].start.dist(model.walls[i].end);
        if !length.is_finite() || length < MIN_SAMPLE_WALL {
            continue;
        }
        let offset = 0.4 * length;

        let mut attempts = Vec::new();
        if (i + 1) % 3 == 0 {
            attempts.push(DOOR);
        }
        if (i + 1) % 5 == 0 {
            attempts.push(WINDOW);
        }
        for (kind, width, height, sill, style) in attempts {
            if validate_placement(model, wid, offset, width, sill, height, kind, style, None).is_err() {
                continue;
            }
            let mut o = Opening::new(wid, offset, width, height, kind);
            o.sill = sill;
            o.style = style.to_string();
            model.push_opening(o);
            placed += 1;
            if placed >= max {
                return placed;
            }
        }
    }
    placed
}

/// Run `arch.sample`: inject a deterministic wall chain into the drawing.
///
/// `walls` defaults to 64, clamped to `1..=MAX_WALLS`; `openings` defaults to
/// 0 and is clamped to `MAX_OPENINGS`. This replaces the existing walls
/// *and* openings, calls `touch_arch()`; it is `.noundo()` because it is a
/// benchmark fixture.
fn run_sample(s: &mut Session, params: &Value) -> crate::Result<Value> {
    let n = params.get("walls").and_then(|v| v.as_u64()).unwrap_or(64).clamp(1, crate::cmd::arch::MAX_WALLS as u64) as usize;
    let count = params.get("openings").and_then(|v| v.as_u64()).unwrap_or(0).clamp(0, MAX_OPENINGS as u64) as usize;

    let d = s.doc_mut()?;
    d.arch.walls = generate_walls(n);
    // The generated chain hardcodes ids 1..=n; keep the shared counter in
    // step so `push_opening` hands out ids that continue after the walls.
    d.arch.next_id = n as u32;
    d.arch.openings.clear();
    let placed = seed_openings(&mut d.arch, count);
    d.touch_arch();

    Ok(json!({ "walls": n, "openings": placed, "revision": d.arch_revision }))
}

/// Command spec for `arch.sample`.
pub fn specs() -> Vec<crate::cmd::CommandSpec> {
    vec![crate::cmd::CommandSpec::new("arch.sample", "Arch Sample", run_sample).menu(&[]).noundo().params("{walls?: N, openings?: N}")]
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    /// A session with a fresh empty drawing.
    fn fresh_session() -> Session {
        let mut s = Session::empty();
        s.new_drawing(false);
        s
    }

    #[test]
    fn arch_sample_is_deterministic_and_capped() {
        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        let p = json!({ "walls": 2000 });
        let r1 = s1.execute("arch.sample", &p).unwrap();
        let r2 = s2.execute("arch.sample", &p).unwrap();

        assert_eq!(r1["walls"].as_u64(), Some(2000));
        assert_eq!(r2["walls"].as_u64(), Some(2000));

        // Byte-identical wall data on fresh documents.
        for (a, b) in s1.doc().unwrap().arch.walls.iter().zip(s2.doc().unwrap().arch.walls.iter()) {
            assert_eq!(a, b);
        }

        // Revision was bumped.
        assert!(s1.doc().unwrap().arch_revision > 0);

        // N is capped at MAX_WALLS.
        let mut big = fresh_session();
        let r3 = big.execute("arch.sample", &json!({ "walls": 99999 })).unwrap();
        assert_eq!(r3["walls"].as_u64(), Some(crate::cmd::arch::MAX_WALLS as u64));
        assert_eq!(big.doc().unwrap().arch.walls.len(), crate::cmd::arch::MAX_WALLS);
    }

    #[test]
    fn arch_sample_clamps_a_hostile_wall_count() {
        let mut s = fresh_session();
        let r = s.execute("arch.sample", &json!({ "walls": 999999999999i64 })).unwrap();
        assert_eq!(r["walls"].as_u64(), Some(crate::cmd::arch::MAX_WALLS as u64));
    }

    #[test]
    fn arch_sample_walls_are_connected() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "walls": 64 })).unwrap();
        let walls = &s.doc().unwrap().arch.walls;
        for i in 0..walls.len().saturating_sub(1) {
            assert_eq!(walls[i].end, walls[i + 1].start, "wall {} end != wall {} start", i, i + 1);
        }
    }

    /// The seed is the only input: two fresh sessions with the same
    /// `walls`/`openings` params must produce byte-identical openings
    /// (ids, hosts, offsets, dimensions, styles, kinds) in the same order.
    #[test]
    fn arch_sample_places_deterministic_openings() {
        const N_WALLS: usize = 32;
        const N_OPENINGS: usize = 32;
        let p = json!({ "walls": N_WALLS, "openings": N_OPENINGS });

        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        let r1 = s1.execute("arch.sample", &p).unwrap();
        let r2 = s2.execute("arch.sample", &p).unwrap();
        assert_eq!(r1, r2);

        let o1 = s1.doc().unwrap().arch.openings.clone();
        let o2 = s2.doc().unwrap().arch.openings.clone();
        assert_eq!(o1, o2);

        // 10 doors (walls 3..=30, every 3rd) + 4 windows (walls 5, 10, 20, 25;
        // walls 15 and 30 would take a door and a window at the same 40 %
        // mark, so the window's separation check rejects it).
        assert_eq!(o1.len(), 14, "expected 14 seeded openings");
        assert!(o1.iter().all(|o| o.id.0 >= N_WALLS as u32), "opening ids must continue after wall ids");

        let d = s1.doc().unwrap();
        // Every opening sits at 40 % of its host wall, in the door/window
        // styles the command seeds.
        for o in &o1 {
            let w = d.arch.wall(o.host).expect("opening host missing");
            let len = w.start.dist(w.end);
            assert!((o.offset - 0.4 * len).abs() < 1e-9, "offset of {:?} not 40 % of its wall", o.id);
            if o.style.contains("hinged") {
                assert_eq!(o.kind, OpeningKind::Door);
                assert_eq!((o.width, o.height, o.sill), (0.9, 2.1, 0.0));
            } else {
                assert_eq!(o.style, "slider");
                assert_eq!(o.kind, OpeningKind::Window);
                assert_eq!((o.width, o.height, o.sill), (1.2, 1.2, 0.9));
            }
        }
        // 2nd wall: no opening; 3rd: exactly the door; 5th: exactly the window.
        assert!(d.arch.openings_on(ArchId(2)).is_empty());
        assert_eq!(d.arch.openings_on(ArchId(3)).len(), 1);
        assert!(d.arch.openings_on(ArchId(5)).len() == 1);
    }

    /// Every seeded opening must still pass placement validation against the
    /// final model (rough/finished dimensions, wall height, segment and
    /// separation limits), i.e. the fixture is a set of legal placements.
    #[test]
    fn arch_sample_openings_are_valid_placements() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "walls": 64, "openings": 16 })).unwrap();
        let d = s.doc().unwrap();
        for o in &d.arch.openings {
            assert!(d.arch.wall(o.host).is_some(), "host wall missing for {:?}", o.id);
            assert!(
                validate_placement(&d.arch, o.host, o.offset, o.width, o.sill, o.height, o.kind, &o.style, Some(o.id),).is_ok(),
                "opening {:?} fails re-validation on the final model",
                o.id
            );
        }
    }

    /// Without the `openings` parameter (or with 0) the model must contain no
    /// openings, exactly as the command behaved before this parameter.
    #[test]
    fn arch_sample_without_openings_is_unchanged() {
        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        s1.execute("arch.sample", &json!({})).unwrap();
        s2.execute("arch.sample", &json!({ "walls": 24, "openings": 0 })).unwrap();

        assert!(s1.doc().unwrap().arch.openings.is_empty());
        assert_eq!(s1.doc().unwrap().arch.walls.len(), 64);
        assert!(s2.doc().unwrap().arch.openings.is_empty());
        assert_eq!(s2.doc().unwrap().arch.walls.len(), 24);
    }
}
