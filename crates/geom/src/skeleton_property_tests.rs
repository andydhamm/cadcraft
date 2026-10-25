//! Property tests for the straight-skeleton API in `skeleton.rs`: skeleton validity, monotone insets, the skyline corner
//! formula, outset growth, and robustness against perturbed or degenerate rings.
//!
//! Deterministic: a fixed-seed xorshift64* generator per generator, no external crate. Every failure names the
//! generator, its seed and the case index, and prints the polygon's vertices so it can be reproduced.

use crate::Vec2;
use crate::skeleton::*;
use std::panic::{self, AssertUnwindSafe};

const CASES: usize = 200;

// ---------- generator ----------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    /// Uniform in `[lo, hi)`.
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One generated input. `convex` and `reflex` count the corners of a skyline (zero for the other generators).
struct Case {
    outer: Vec<Vec2>,
    holes: Vec<Vec<Vec2>>,
    convex: usize,
    reflex: usize,
}

#[derive(Clone, Copy, Debug)]
enum Gen {
    Star,
    Skyline,
    Courtyard,
}

const GENS: [Gen; 3] = [Gen::Star, Gen::Skyline, Gen::Courtyard];

impl Gen {
    fn name(self) -> &'static str {
        match self {
            Gen::Star => "star",
            Gen::Skyline => "skyline",
            Gen::Courtyard => "courtyard",
        }
    }
    fn seed(self) -> u64 {
        match self {
            Gen::Star => 0x57A2_0001,
            Gen::Skyline => 0x5C7_0002,
            Gen::Courtyard => 0xC0A7_0003,
        }
    }
    fn make(self, rng: &mut Rng) -> Case {
        match self {
            Gen::Star => star(rng),
            Gen::Skyline => skyline(rng),
            Gen::Courtyard => courtyard(rng),
        }
    }
}

/// Star-shaped polygon: 5..=40 vertices at strictly increasing angles, radii 0.5..1.5.
fn star(rng: &mut Rng) -> Case {
    let n = 5 + (rng.next() % 36) as usize;
    let sector = std::f64::consts::TAU / n as f64;
    let mut outer = Vec::with_capacity(n);
    for i in 0..n {
        // Each vertex sits in its own sector, 30% to 70% of the way through it. Consecutive angles then differ by
        // 0.6 to 1.4 sectors: at least 0.05 rad for n <= 75, and under pi everywhere, so the origin sees every edge
        // and the polygon is simple.
        let angle = (i as f64 + rng.range(0.3, 0.7)) * sector;
        let radius = rng.range(0.5, 1.5);
        outer.push(Vec2::new(radius * angle.cos(), radius * angle.sin()));
    }
    Case { outer, holes: Vec::new(), convex: 0, reflex: 0 }
}

/// Rectilinear skyline over unit-width columns: k = 2..=12 columns with integer heights 1..=6. Equal neighbours are
/// merged, so every vertex is a true corner.
fn skyline(rng: &mut Rng) -> Case {
    let k = 2 + (rng.next() % 11) as usize;
    // Runs of equal height as (width, height), left to right.
    let mut runs: Vec<(usize, u64)> = Vec::new();
    for _ in 0..k {
        let h = 1 + rng.next() % 6;
        let same = runs.last().is_some_and(|run| run.1 == h);
        if same {
            if let Some(run) = runs.last_mut() {
                run.0 += 1;
            }
        } else {
            runs.push((1, h));
        }
    }
    // x-boundaries of the runs: xs[j] is the left edge of run j, xs[m] the right edge of the last run.
    let mut xs = vec![0usize];
    for &(w, _) in &runs {
        let next = xs[xs.len() - 1] + w;
        xs.push(next);
    }
    let m = runs.len();
    let mut outer = vec![Vec2::new(0.0, 0.0), Vec2::new(xs[m] as f64, 0.0)];
    // Walk the top from right to left: each run contributes its top-right then top-left corner.
    for j in (0..m).rev() {
        let top = runs[j].1 as f64;
        outer.push(Vec2::new(xs[j + 1] as f64, top));
        outer.push(Vec2::new(xs[j] as f64, top));
    }
    // Classify each corner by the sign of the turn (the ring is counter-clockwise, so a left turn is convex).
    let (mut convex, mut reflex) = (0usize, 0usize);
    let n = outer.len();
    for i in 0..n {
        let (p, c, q) = (outer[(i + n - 1) % n], outer[i], outer[(i + 1) % n]);
        let turn = (c - p).x * (q - c).y - (c - p).y * (q - c).x;
        if turn > 0.0 {
            convex += 1;
        } else if turn < 0.0 {
            reflex += 1;
        }
    }
    assert_eq!(convex as i64 - reflex as i64, 4, "a rectilinear counter-clockwise ring has four more convex corners");
    Case { outer, holes: Vec::new(), convex, reflex }
}

/// A w x h rectangle (4..20 per side) with one axis-aligned clockwise hole whose sides are at least 1.0 and which
/// keeps at least 1.0 clear of every outer edge.
fn courtyard(rng: &mut Rng) -> Case {
    let w = rng.range(4.0, 20.0);
    let h = rng.range(4.0, 20.0);
    let hw = rng.range(1.0, w - 2.0);
    let hh = rng.range(1.0, h - 2.0);
    let x0 = rng.range(1.0, w - 1.0 - hw);
    let y0 = rng.range(1.0, h - 1.0 - hh);
    let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h)];
    let hole = vec![Vec2::new(x0, y0), Vec2::new(x0, y0 + hh), Vec2::new(x0 + hw, y0 + hh), Vec2::new(x0 + hw, y0)];
    Case { outer, holes: vec![hole], convex: 0, reflex: 0 }
}

/// Runs the property over `CASES` cases from the generator's fixed seed.
fn for_each_case(g: Gen, mut f: impl FnMut(usize, &Case)) {
    let mut rng = Rng::new(g.seed());
    for case in 0..CASES {
        let c = g.make(&mut rng);
        f(case, &c);
    }
}

// ---------- geometry helpers ----------

/// Shoelace area of a ring, regardless of orientation.
fn area(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    let mut twice = 0.0;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        twice += a.x * b.y - b.x * a.y;
    }
    (twice / 2.0).abs()
}

/// Length of the closed ring.
fn perimeter(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i].dist(ring[(i + 1) % n])).sum()
}

/// Outer area minus hole areas, summed over the regions.
fn total_area(regions: &[OffsetRegion]) -> f64 {
    regions.iter().map(|r| area(&r.outer) - r.holes.iter().map(|h| area(h)).sum::<f64>()).sum()
}

/// Area of the input polygon with its holes removed.
fn input_area(c: &Case) -> f64 {
    area(&c.outer) - c.holes.iter().map(|h| area(h)).sum::<f64>()
}

// ---------- failure reporting ----------

/// Panics with the generator, seed, case index and polygon, so the failure can be rebuilt by hand.
fn bail(g: Gen, case: usize, c: &Case, msg: String) -> ! {
    // `panic_any` rather than `panic!`: the crate denies clippy::panic.
    std::panic::panic_any(format!("{} case {case} (seed {:#x}): {msg}\n  outer: {:?}\n  holes: {:?}", g.name(), g.seed(), c.outer, c.holes))
}

fn skeleton_of(g: Gen, case: usize, c: &Case) -> Skeleton {
    match straight_skeleton(&c.outer, &c.holes) {
        Ok(sk) => sk,
        Err(e) => bail(g, case, c, format!("straight_skeleton returned Err({e})")),
    }
}

fn inset_of(g: Gen, case: usize, c: &Case, d: f64) -> Vec<OffsetRegion> {
    match inset(&c.outer, &c.holes, d) {
        Ok(regions) => regions,
        Err(e) => bail(g, case, c, format!("inset(d = {d}) returned Err({e})")),
    }
}

fn outset_of(g: Gen, case: usize, c: &Case, d: f64) -> OffsetRegion {
    match outset(&c.outer, &c.holes, d) {
        Ok(region) => region,
        Err(e) => bail(g, case, c, format!("outset(d = {d}) returned Err({e})")),
    }
}

// ---------- properties ----------

/// Every generated polygon gives a skeleton that passes `check`, has one face per cleaned edge, and has a positive
/// maximum time.
#[test]
fn skeleton_passes_check_and_has_one_face_per_edge() {
    for g in GENS {
        for_each_case(g, |case, c| {
            let sk = skeleton_of(g, case, c);
            let problems = check(&sk, 1e-6);
            if !problems.is_empty() {
                bail(g, case, c, format!("check reports {problems:?}"));
            }
            let edges: usize = sk.rings.iter().map(Vec::len).sum();
            if sk.faces.len() != edges {
                bail(g, case, c, format!("{} faces for {edges} cleaned edges", sk.faces.len()));
            }
            // Also fails on NaN, which `<= 0.0` would let through.
            if sk.max_time().partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                bail(g, case, c, format!("max_time {} is not positive", sk.max_time()));
            }
        });
    }
}

/// Inset area never grows as d rises from 0 to max_time; inset(0) has the input's area; past max_time it is empty.
#[test]
fn inset_area_never_grows_and_vanishes_past_max_time() {
    for g in GENS {
        for_each_case(g, |case, c| {
            let input = input_area(c);
            let t = skeleton_of(g, case, c).max_time();
            let mut prev = f64::INFINITY;
            for step in 0..=10 {
                let d = t * step as f64 / 10.0;
                let a = total_area(&inset_of(g, case, c, d));
                if step == 0 && (a - input).abs() > 1e-6 * input.abs() {
                    bail(g, case, c, format!("inset(0) area {a} != input area {input}"));
                }
                if a > prev + 1e-9 {
                    bail(g, case, c, format!("inset area grew from {prev} to {a} at d = {d} (step {step}/10)"));
                }
                prev = a;
            }
            let past = inset_of(g, case, c, t * 1.001);
            if !past.is_empty() {
                bail(g, case, c, format!("inset past max_time {t} still has {} regions", past.len()));
            }
        });
    }
}

/// Skyline rule: below half the smallest feature (1), the inset area is area - perimeter*d + (convex - reflex)*d^2.
#[test]
fn skyline_inset_area_follows_the_corner_formula() {
    for_each_case(Gen::Skyline, |case, c| {
        let d = 0.2;
        let got = total_area(&inset_of(Gen::Skyline, case, c, d));
        let want = area(&c.outer) - perimeter(&c.outer) * d + (c.convex as f64 - c.reflex as f64) * d * d;
        if (got - want).abs() > 1e-6 {
            bail(Gen::Skyline, case, c, format!("inset(d = {d}) area {got} != {want} (area - P*d + (convex - reflex)*d^2)"));
        }
    });
}

/// Mitred outset adds at least perimeter*d to the outer area (the corners only add area). Holes are ignored here:
/// only the outer ring is compared.
///
/// Caveat for whoever reads a failure: the exact mitred area is area + perimeter*d + d^2 * sum(cot(alpha_i / 2)),
/// where alpha_i are the interior angles. That sum is non-negative for rectilinear shapes (it equals convex - reflex)
/// but can dip below zero for spiky stars with sharp reflex corners. A failure on such a star points at that term,
/// not necessarily at a bug in the offset code.
#[test]
fn outset_outer_area_grows_by_at_least_perimeter_times_d() {
    for g in [Gen::Star, Gen::Courtyard] {
        for_each_case(g, |case, c| {
            for d in [0.1, 0.5] {
                let out = outset_of(g, case, c, d);
                let want = area(&c.outer) + perimeter(&c.outer) * d;
                let got = area(&out.outer);
                if got + 1e-6 < want {
                    bail(g, case, c, format!("outset(d = {d}) outer area {got} < input area + P*d = {want}"));
                }
            }
        });
    }
}

/// Runs `f` for a ring. A `todo!` placeholder panic passes through unchanged; any other panic is re-raised with the
/// ring, so a real crash can be reproduced.
fn must_not_panic(g: Gen, case: usize, ring: &[Vec2], f: impl FnOnce()) {
    let Err(payload) = panic::catch_unwind(AssertUnwindSafe(f)) else {
        return;
    };
    let text = payload.downcast_ref::<String>().cloned().or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    if text.contains("not yet implemented") {
        panic::resume_unwind(payload);
    }
    std::panic::panic_any(format!(
        "{} case {case} (seed {:#x}): panicked on a perturbed or degenerate ring: {text}\n  ring: {ring:?}",
        g.name(),
        g.seed()
    ));
}

/// Robustness: rings with 1e-7 vertex noise, duplicated consecutive points, a repeated closing point, or all three
/// together. Each call must return Ok or Err; it must never panic.
#[test]
fn straight_skeleton_never_panics_on_perturbed_or_degenerate_rings() {
    let g = Gen::Star;
    let mut rng = Rng::new(g.seed() ^ 0x9E37_79B9);
    for case in 0..100 {
        let base = g.make(&mut rng).outer;

        let mut perturbed = Vec::with_capacity(base.len());
        for p in &base {
            let dx = rng.range(-1e-7, 1e-7);
            let dy = rng.range(-1e-7, 1e-7);
            perturbed.push(Vec2::new(p.x + dx, p.y + dy));
        }

        let mut doubled = Vec::new();
        for p in &base {
            doubled.push(*p);
            if rng.range(0.0, 1.0) < 0.5 {
                doubled.push(*p);
            }
        }

        let mut closed = base.clone();
        closed.push(base[0]);

        let mut all = perturbed.clone();
        for p in &doubled {
            all.push(*p);
        }
        all.push(base[0]);

        for ring in [perturbed.as_slice(), doubled.as_slice(), closed.as_slice(), all.as_slice()] {
            must_not_panic(g, case, ring, || {
                // Ok or Err are both acceptable here; only a panic fails the test.
                let _ = straight_skeleton(ring, &[]);
                let _ = inset(ring, &[], 0.05);
            });
        }
    }
}
