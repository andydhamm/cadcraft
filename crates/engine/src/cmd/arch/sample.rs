//! `arch.sample` — inject a deterministic sample arch model (benchmark fixture).
//!
//! Replaces `Drawing::arch.walls` with a seeded xorshift64* wall chain (same
//! bit pattern as `apps/cadcraft-cli/src/main.rs::Rng`, seed
//! `0x9E37_79B9_7F4A_7C15`). The chain is connected: wall `i` starts where
//! wall `i-1` ended, the heading turns a right angle every `4 + (rng % 3)`
//! walls, length in [3.0, 8.9) drawing units, thickness alternating 0.2/0.1,
//! height 2.6, ids `w1..wN` (handles `arch_handle(i)`).
//!
//! This command *does* mutate the model and calls `touch_arch()`; it is
//! `.noundo()` only because it is a benchmark fixture, not a user-facing edit.

use serde_json::{Value, json};

use crate::Session;

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

/// Run `arch.sample`: inject a deterministic wall chain into the drawing.
///
/// `walls` defaults to 64, clamped to `1..=MAX_WALLS`. This mutates the
/// arch model and calls `touch_arch()`; it is `.noundo()` because it is a
/// benchmark fixture.
fn run_sample(s: &mut Session, params: &Value) -> crate::Result<Value> {
    let n = params.get("walls").and_then(|v| v.as_u64()).unwrap_or(64).clamp(1, crate::cmd::arch::MAX_WALLS as u64) as usize;

    let d = s.doc_mut()?;
    d.arch.walls = generate_walls(n);
    d.touch_arch();

    Ok(json!({ "walls": n, "revision": d.arch_revision }))
}

/// Command spec for `arch.sample`.
pub fn specs() -> Vec<crate::cmd::CommandSpec> {
    vec![crate::cmd::CommandSpec::new("arch.sample", "Arch Sample", run_sample).menu(&[]).noundo().params("{walls?: N}")]
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
}
