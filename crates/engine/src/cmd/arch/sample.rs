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

use serde_json::{json, Value};

use crate::Session;

/// Seeded xorshift64*, copied bit-for-bit from the CLI's `Rng`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
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
    let mut legs_left: usize = 4 + (rng.state() % 3);
    let mut k = 0usize; // 1-based wall index

    while walls.len() < n {
        // Length: 3.0 + (rng % 60) as f64 / 10.0 → [3.0, 8.9) in 0.1 steps.
        // Never zero with the seeded generator; if a degenerate value ever
        // comes back, skip it and retry with a fresh one, bounded retries.
        let mut length = 0.0;
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
            legs_left = 4 + (rng.next() as f64).floor() as usize % 3 + 4; // placeholder, fixed below
            // TODO: turn interval re-draw
        }
    }
    walls
}
```

Wait, I made an error in that sample generator — I left a placeholder. Let me fix it properly. The turn interval should be: `4 + (rng.next() as f64).floor() % 3`... no. Let me rework. The interval should be `4 + (something in 0..3)`. Since `rng.next()` returns `[0.0, 1.0)`, I need `(rng.next() * 3.0).floor()` to get 0, 1, or 2. So `4 + (rng.next() * 3.0).floor() as usize`. Let me fix this.

Also I referenced `rng.state()` which doesn't exist. I need to add a `state()` method or just use `next()` for the first interval too.

Let me rewrite sample.rs cleanly:

crates/engine/src/cmd/arch/sample.rs
```
<<<<<<< SEARCH
