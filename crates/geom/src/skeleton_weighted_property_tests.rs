//! Property tests for the weighted straight skeleton in `skeleton.rs` (assisted draft, 2026-10-25): valid skeletons under
//! random weights, uniform weights as a pure rescaling of the plain skeleton, gables never speeding things up, roof
//! planes from `roof_skeleton`, and robustness against zero weights and degenerate rings.
//!
//! Deterministic: a fixed-seed xorshift64* generator per property, no external crate. Every failure names the property,
//! its seed and the case index, and prints the rings and weights so it can be reproduced.

use crate::Vec2;
use crate::skeleton::*;
use std::panic::{self, AssertUnwindSafe};

const CASES: usize = 200;
const ROBUST_CASES: usize = 100;

const SEED_RANDOM_WEIGHTS: u64 = 0x3E16_0001;
const SEED_UNIFORM_SCALE: u64 = 0x5CA1_0002;
const SEED_GABLES: u64 = 0x6AB1_0003;
const SEED_ROOF: u64 = 0x2007_0004;
const SEED_ROBUST: u64 = 0x9B0B_0005;

// ---------- generators (copied from skeleton_property_tests.rs; that module's items are private) ----------

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

/// One generated input: an outer ring and its holes, all counter-clockwise or clockwise as the generator made them.
struct Case {
    outer: Vec<Vec2>,
    holes: Vec<Vec<Vec2>>,
}

/// Star-shaped polygon: 5..=40 vertices at strictly increasing angles, radii 0.5..1.5.
fn star(rng: &mut Rng) -> Case {
    let n = 5 + (rng.next() % 36) as usize;
    let sector = std::f64::consts::TAU / n as f64;
    let mut outer = Vec::with_capacity(n);
    for i in 0..n {
        // Each vertex sits in its own sector, 30% to 70% of the way through it, so the origin sees every edge and the
        // polygon is simple. No vertices are collinear.
        let angle = (i as f64 + rng.range(0.3, 0.7)) * sector;
        let radius = rng.range(0.5, 1.5);
        outer.push(Vec2::new(radius * angle.cos(), radius * angle.sin()));
    }
    Case { outer, holes: Vec::new() }
}

/// A w x h rectangle (4..20 per side) with one axis-aligned clockwise hole whose sides are at least 1.0 and which keeps
/// at least 1.0 clear of every outer edge.
fn courtyard(rng: &mut Rng) -> Case {
    let w = rng.range(4.0, 20.0);
    let h = rng.range(4.0, 20.0);
    let hw = rng.range(1.0, w - 2.0);
    let hh = rng.range(1.0, h - 2.0);
    let x0 = rng.range(1.0, w - 1.0 - hw);
    let y0 = rng.range(1.0, h - 1.0 - hh);
    let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h)];
    let hole = vec![Vec2::new(x0, y0), Vec2::new(x0, y0 + hh), Vec2::new(x0 + hw, y0 + hh), Vec2::new(x0 + hw, y0)];
    Case { outer, holes: vec![hole] }
}

/// Rectilinear skyline over unit-width columns: k = 2..=12 columns with integer heights 1..=6, counter-clockwise.
/// Equal neighbours are merged, so every vertex is a true corner.
fn skyline(rng: &mut Rng) -> Vec<Vec2> {
    let k = 2 + (rng.next() % 11) as usize;
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
    let mut xs = vec![0usize];
    for &(w, _) in &runs {
        let next = xs[xs.len() - 1] + w;
        xs.push(next);
    }
    let m = runs.len();
    let mut outer = vec![Vec2::new(0.0, 0.0), Vec2::new(xs[m] as f64, 0.0)];
    for j in (0..m).rev() {
        let top = runs[j].1 as f64;
        outer.push(Vec2::new(xs[j + 1] as f64, top));
        outer.push(Vec2::new(xs[j] as f64, top));
    }
    outer
}

// ---------- helpers ----------

/// Turn at vertex `i` of a ring: positive for a left (convex, for counter-clockwise rings) turn.
fn turn_at(ring: &[Vec2], i: usize) -> f64 {
    let n = ring.len();
    let (p, c, q) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
    (c - p).cross(q - c)
}

/// Distance from `p` to the infinite line through `a` and `b`.
fn dist_to_line(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    (b - a).cross(p - a).abs() / (b - a).len()
}

/// Panics with the property, seed, case index and inputs, so the failure can be rebuilt by hand. `panic_any` rather than
/// `panic!`: the crate denies clippy::panic.
fn bail(prop: &str, seed: u64, case: usize, detail: String) -> ! {
    std::panic::panic_any(format!("{prop} (seed {seed:#x}) case {case}: {detail}"))
}

fn fmt_input(outer: &[Vec2], holes: &[Vec<Vec2>], weights: &[Vec<f64>]) -> String {
    format!("\n  outer: {outer:?}\n  holes: {holes:?}\n  weights: {weights:?}")
}

fn plain_or_bail(prop: &str, seed: u64, case: usize, c: &Case) -> Skeleton {
    match straight_skeleton(&c.outer, &c.holes) {
        Ok(sk) => sk,
        Err(e) => bail(prop, seed, case, format!("straight_skeleton returned Err({e}){}", fmt_input(&c.outer, &c.holes, &[]))),
    }
}

fn weighted_or_bail(prop: &str, seed: u64, case: usize, c: &Case, weights: &[Vec<f64>]) -> Skeleton {
    match weighted_skeleton(&c.outer, &c.holes, weights) {
        Ok(sk) => sk,
        Err(e) => bail(prop, seed, case, format!("weighted_skeleton returned Err({e}){}", fmt_input(&c.outer, &c.holes, weights))),
    }
}

/// Runs `f` on a ring. A `todo!` placeholder panic passes through unchanged; any other panic is re-raised with the
/// ring and weights, so a real crash can be reproduced.
fn must_not_panic(prop: &str, seed: u64, case: usize, ring: &[Vec2], weights: &[Vec<f64>], f: impl FnOnce()) {
    let Err(payload) = panic::catch_unwind(AssertUnwindSafe(f)) else {
        return;
    };
    let text = payload.downcast_ref::<String>().cloned().or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    if text.contains("not yet implemented") {
        panic::resume_unwind(payload);
    }
    bail(prop, seed, case, format!("panicked: {text}{}", fmt_input(ring, &[], weights)));
}

// ---------- properties ----------

/// Star polygons with weights uniform in [0.5, 2] give an Ok skeleton that passes `check`, has one face per cleaned
/// edge, and echoes the input weights for the outer ring (stars have no collinear vertices, so nothing is cleaned).
#[test]
fn random_positive_weights_on_stars_give_valid_skeletons() {
    let prop = "random_positive_weights_on_stars_give_valid_skeletons";
    let mut rng = Rng::new(SEED_RANDOM_WEIGHTS);
    for case in 0..CASES {
        let c = star(&mut rng);
        let w: Vec<f64> = (0..c.outer.len()).map(|_| rng.range(0.5, 2.0)).collect();
        let weights = vec![w.clone()];
        let sk = weighted_or_bail(prop, SEED_RANDOM_WEIGHTS, case, &c, &weights);
        let problems = check(&sk, 1e-6);
        if !problems.is_empty() {
            bail(prop, SEED_RANDOM_WEIGHTS, case, format!("check reports {problems:?}{}", fmt_input(&c.outer, &c.holes, &weights)));
        }
        let edges: usize = sk.rings.iter().map(Vec::len).sum();
        if sk.faces.len() != edges {
            bail(
                prop,
                SEED_RANDOM_WEIGHTS,
                case,
                format!("{} faces for {edges} cleaned edges{}", sk.faces.len(), fmt_input(&c.outer, &c.holes, &weights)),
            );
        }
        let echoed = sk.weights.first().map(Vec::as_slice).unwrap_or_default();
        let same = echoed.len() == w.len() && echoed.iter().zip(&w).all(|(a, b)| (a - b).abs() <= 1e-12);
        if sk.weights.len() != 1 || !same {
            bail(
                prop,
                SEED_RANDOM_WEIGHTS,
                case,
                format!("skeleton weights {:?} do not echo the input{}", sk.weights, fmt_input(&c.outer, &c.holes, &weights)),
            );
        }
    }
}

/// With every edge weighted c in [0.25, 4], the interior nodes equal the plain straight skeleton's positions (within
/// 1e-6, compared after sorting by x then y) and every interior time equals the plain time divided by c (within 1e-6
/// relative). Star and courtyard polygons.
#[test]
fn uniform_weight_c_scales_interior_times_by_one_over_c() {
    let prop = "uniform_weight_c_scales_interior_times_by_one_over_c";
    let mut rng = Rng::new(SEED_UNIFORM_SCALE);
    for case in 0..CASES {
        for c in [star(&mut rng), courtyard(&mut rng)] {
            let speed = rng.range(0.25, 4.0);
            let weights: Vec<Vec<f64>> = std::iter::once(vec![speed; c.outer.len()]).chain(c.holes.iter().map(|h| vec![speed; h.len()])).collect();
            let plain = plain_or_bail(prop, SEED_UNIFORM_SCALE, case, &c);
            let scaled = weighted_or_bail(prop, SEED_UNIFORM_SCALE, case, &c, &weights);
            let ring_vertices: usize = plain.rings.iter().map(Vec::len).sum();
            let want: Vec<SkeletonNode> = plain.nodes.iter().skip(ring_vertices).copied().collect();
            let got: Vec<SkeletonNode> = scaled.nodes.iter().skip(ring_vertices).copied().collect();
            if want.len() != got.len() {
                bail(
                    prop,
                    SEED_UNIFORM_SCALE,
                    case,
                    format!("{} interior nodes for c = {speed}, plain has {}{}", got.len(), want.len(), fmt_input(&c.outer, &c.holes, &weights)),
                );
            }
            // Pair each plain node with an unused scaled node at the same position (within 1e-6). Sorting by exact x then
            // y does not work: on symmetric shapes two nodes share an x up to ~1e-15 and swap places between the lists.
            let mut used = vec![false; got.len()];
            for w in &want {
                let Some(k) = (0..got.len()).find(|&k| !used[k] && (w.pos.x - got[k].pos.x).abs() <= 1e-6 && (w.pos.y - got[k].pos.y).abs() <= 1e-6)
                else {
                    bail(
                        prop,
                        SEED_UNIFORM_SCALE,
                        case,
                        format!("c = {speed}: node at {:?} in plain has no scaled node there{}", w.pos, fmt_input(&c.outer, &c.holes, &weights)),
                    );
                };
                used[k] = true;
                let g = &got[k];
                let expected = w.time / speed;
                if (g.time - expected).abs() > 1e-6 * expected.abs().max(1e-12) {
                    bail(
                        prop,
                        SEED_UNIFORM_SCALE,
                        case,
                        format!(
                            "c = {speed}: node at {:?} has time {} but plain time / c = {expected}{}",
                            g.pos,
                            g.time,
                            fmt_input(&c.outer, &c.holes, &weights)
                        ),
                    );
                }
            }
        }
    }
}

/// Skyline gables: a random set of edges whose two corners are convex, with neither neighbouring edge chosen, gets
/// weight 0 and every other edge weight 1. The result is Ok, passes `check`, and its max_time is at least the plain
/// straight skeleton's max_time (a gable never speeds the wavefront up).
#[test]
fn gable_weights_never_shorten_max_time() {
    let prop = "gable_weights_never_shorten_max_time";
    let mut rng = Rng::new(SEED_GABLES);
    for case in 0..CASES {
        let outer = skyline(&mut rng);
        let n = outer.len();
        let c = Case { outer, holes: Vec::new() };
        let convex: Vec<bool> = (0..n).map(|i| turn_at(&c.outer, i) > 0.0).collect();
        let mut chosen = vec![false; n];
        for i in 0..n {
            let both_convex = convex[i] && convex[(i + 1) % n];
            let neighbours_free = !chosen[(i + n - 1) % n] && !chosen[(i + 1) % n];
            if both_convex && neighbours_free && rng.range(0.0, 1.0) < 0.5 {
                chosen[i] = true;
            }
        }
        let w: Vec<f64> = chosen.iter().map(|&g| if g { 0.0 } else { 1.0 }).collect();
        let weights = vec![w];
        let plain = plain_or_bail(prop, SEED_GABLES, case, &c);
        let sk = weighted_or_bail(prop, SEED_GABLES, case, &c, &weights);
        let problems = check(&sk, 1e-6);
        if !problems.is_empty() {
            bail(prop, SEED_GABLES, case, format!("check reports {problems:?}{}", fmt_input(&c.outer, &c.holes, &weights)));
        }
        if sk.max_time() < plain.max_time() - 1e-9 {
            bail(
                prop,
                SEED_GABLES,
                case,
                format!("max_time {} with gables is below the plain {}{}", sk.max_time(), plain.max_time(), fmt_input(&c.outer, &c.holes, &weights)),
            );
        }
    }
}

/// Roof planes: for every face (ring r, edge e, slope s) and every node n of that face, s * distance(n.pos, line through
/// the edge) equals n.time within 1e-6, since `roof_skeleton` gives each edge speed 1/s and time is the roof height.
#[test]
fn roof_faces_are_planes_of_their_slope() {
    let prop = "roof_faces_are_planes_of_their_slope";
    let mut rng = Rng::new(SEED_ROOF);
    for case in 0..CASES {
        let c = star(&mut rng);
        let slopes: Vec<Vec<f64>> = vec![(0..c.outer.len()).map(|_| rng.range(0.3, 2.0)).collect()];
        let sk = match roof_skeleton(&c.outer, &c.holes, &slopes) {
            Ok(sk) => sk,
            Err(e) => bail(prop, SEED_ROOF, case, format!("roof_skeleton returned Err({e}){}", fmt_input(&c.outer, &c.holes, &slopes))),
        };
        for face in &sk.faces {
            let ring = match sk.rings.get(face.ring) {
                Some(r) => r,
                None => bail(prop, SEED_ROOF, case, format!("face refers to missing ring {}", face.ring)),
            };
            let len = ring.len();
            let s = slopes.get(face.ring).and_then(|r| r.get(face.edge)).copied();
            let (Some(s), true) = (s, face.edge < len) else {
                bail(prop, SEED_ROOF, case, format!("face (ring {}, edge {}) has no slope or edge", face.ring, face.edge));
            };
            let (a, b) = (ring[face.edge], ring[(face.edge + 1) % len]);
            for &nid in &face.nodes {
                let node = sk.nodes[nid];
                let height = s * dist_to_line(node.pos, a, b);
                if (height - node.time).abs() > 1e-6 * (1.0 + node.time.abs()) {
                    bail(
                        prop,
                        SEED_ROOF,
                        case,
                        format!(
                            "face (ring {}, edge {}, slope {s}): node at {:?} has time {} but slope * distance = {height}{}",
                            face.ring,
                            face.edge,
                            node.pos,
                            node.time,
                            fmt_input(&c.outer, &c.holes, &slopes)
                        ),
                    );
                }
            }
        }
    }
}

/// Robustness: stars with random weights in [0, 2], where zero weights never sit on two neighbouring edges (wrap-around
/// included). Each star is also given as a ring with duplicated consecutive points, a repeated closing point, or both.
/// Every input edge gets a weight, including the zero-length ones. Each call returns Ok or Err; it never panics.
#[test]
fn weighted_skeleton_never_panics_on_zero_weights_or_degenerate_rings() {
    let prop = "weighted_skeleton_never_panics_on_zero_weights_or_degenerate_rings";
    let mut rng = Rng::new(SEED_ROBUST);
    for case in 0..ROBUST_CASES {
        let base = star(&mut rng).outer;

        let mut doubled = Vec::new();
        for p in &base {
            doubled.push(*p);
            if rng.range(0.0, 1.0) < 0.5 {
                doubled.push(*p);
            }
        }

        let mut closed = base.clone();
        closed.push(base[0]);

        let mut both = doubled.clone();
        both.push(base[0]);

        for ring in [base.as_slice(), doubled.as_slice(), closed.as_slice(), both.as_slice()] {
            let weights = vec![robust_weights(&mut rng, ring.len())];
            must_not_panic(prop, SEED_ROBUST, case, ring, &weights, || {
                // Ok or Err are both acceptable here; only a panic fails the test.
                let _ = weighted_skeleton(ring, &[], &weights);
            });
        }
    }
}

/// `m` weights in [0, 2) with exact zeros, never on two cyclically neighbouring indices.
fn robust_weights(rng: &mut Rng, m: usize) -> Vec<f64> {
    let mut zero = vec![false; m];
    for i in 0..m {
        let prev_zero = i > 0 && zero[i - 1];
        let wraps_to_zero = i + 1 == m && zero[0];
        if rng.range(0.0, 1.0) < 0.25 && !prev_zero && !wraps_to_zero {
            zero[i] = true;
        }
    }
    zero.iter().map(|&z| if z { 0.0 } else { rng.range(0.0, 2.0) }).collect()
}
