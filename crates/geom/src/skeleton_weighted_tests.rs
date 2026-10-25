//! Analytic fixtures for the weighted straight skeleton and roof_skeleton (assisted draft, 2026-10-25).
//!
//! Every expected value is derived by hand. An edge of weight w moves inward at speed w, so a node at time t lies at
//! distance w * t from the line of each of its faces' edges. Until `weighted_skeleton` lands these tests fail with
//! "not yet implemented"; only the BadWeights checks that come from roof_skeleton's own slope validation pass early.

use crate::Vec2;
use crate::skeleton::*;

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// True when the skeleton has a node at (x, y) with the given time (within 1e-6).
fn has_node(sk: &Skeleton, x: f64, y: f64, t: f64) -> bool {
    sk.nodes.iter().any(|n| close(n.pos.x, x) && close(n.pos.y, y) && close(n.time, t))
}

/// Interior nodes (everything after the cleaned ring vertices), sorted by x then y, as (x, y, time).
fn sorted_nodes(sk: &Skeleton) -> Vec<(f64, f64, f64)> {
    let ring_vertices: usize = sk.rings.iter().map(Vec::len).sum();
    let mut out: Vec<(f64, f64, f64)> = sk.nodes.iter().skip(ring_vertices).map(|n| (n.pos.x, n.pos.y, n.time)).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out
}

fn assert_valid(sk: &Skeleton) {
    let problems = check(sk, 1e-6);
    assert!(problems.is_empty(), "invariant problems: {problems:#?}\n{sk:#?}");
}

fn expect_ok(result: Result<Skeleton, SkeletonError>, what: &str) -> Skeleton {
    // `panic_any` rather than `panic!`/`unreachable!`: the crate denies clippy::panic.
    result.unwrap_or_else(|e| std::panic::panic_any(format!("{what}: unexpected error {e:?}")))
}

fn assert_has(sk: &Skeleton, x: f64, y: f64, t: f64) {
    assert!(has_node(sk, x, y, t), "expected a node at ({x}, {y}) time {t}; interior nodes {:?}", sorted_nodes(sk));
}

fn rect4x2() -> Vec<Vec2> {
    vec![v(0.0, 0.0), v(4.0, 0.0), v(4.0, 2.0), v(0.0, 2.0)]
}

fn unit_square() -> Vec<Vec2> {
    vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)]
}

fn l_shape() -> Vec<Vec2> {
    vec![v(0.0, 0.0), v(2.0, 0.0), v(2.0, 1.0), v(1.0, 1.0), v(1.0, 2.0), v(0.0, 2.0)]
}

fn courtyard_outer() -> Vec<Vec2> {
    vec![v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)]
}

fn courtyard_hole() -> Vec<Vec2> {
    vec![v(3.0, 3.0), v(7.0, 3.0), v(7.0, 7.0), v(3.0, 7.0)]
}

/// Weights of 1 for every edge of `outer` and of each hole, in the input indexing.
fn ones_for(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<Vec<f64>> {
    std::iter::once(outer.len()).chain(holes.iter().map(Vec::len)).map(|n| vec![1.0; n]).collect()
}

fn assert_all_ones_match_plain(outer: &[Vec2], holes: &[Vec<Vec2>]) {
    let plain = expect_ok(straight_skeleton(outer, holes), "plain straight skeleton");
    let weighted = expect_ok(weighted_skeleton(outer, holes, &ones_for(outer, holes)), "all-ones weighted skeleton");
    assert_valid(&weighted);
    assert!(weighted.weights.iter().flatten().all(|&w| close(w, 1.0)), "weights field should be all 1.0, got {:?}", weighted.weights);
    let (a, b) = (sorted_nodes(&plain), sorted_nodes(&weighted));
    assert_eq!(a.len(), b.len(), "interior node count differs: plain {a:?}, weighted {b:?}");
    for (p, q) in a.iter().zip(&b) {
        assert!((p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9 && (p.2 - q.2).abs() < 1e-9, "node differs: plain {p:?}, weighted {q:?}");
    }
}

// Fixture 1: all-ones weights equal the plain skeleton.

#[test]
fn all_ones_weights_match_plain_unit_square() {
    assert_all_ones_match_plain(&unit_square(), &[]);
}

#[test]
fn all_ones_weights_match_plain_rectangle() {
    assert_all_ones_match_plain(&rect4x2(), &[]);
}

#[test]
fn all_ones_weights_match_plain_l_shape() {
    assert_all_ones_match_plain(&l_shape(), &[]);
}

#[test]
fn all_ones_weights_match_plain_courtyard() {
    assert_all_ones_match_plain(&courtyard_outer(), &[courtyard_hole()]);
}

// Fixture 2: gable ends (frozen left and right edges).

#[test]
fn gable_ends_give_ridge_between_frozen_side_edges() {
    let sk = expect_ok(weighted_skeleton(&rect4x2(), &[], &[vec![1.0, 0.0, 1.0, 0.0]]), "gable rectangle");
    assert_valid(&sk);
    assert_has(&sk, 0.0, 1.0, 1.0);
    assert_has(&sk, 4.0, 1.0, 1.0);
    assert!(close(sk.max_time(), 1.0), "max_time should be 1, got {}", sk.max_time());
}

// Fixture 3: mixed pitch (side edges at half speed).

#[test]
fn mixed_pitch_side_edges_at_half_weight_shorten_ridge() {
    let sk = expect_ok(weighted_skeleton(&rect4x2(), &[], &[vec![1.0, 0.5, 1.0, 0.5]]), "mixed-pitch rectangle");
    assert_valid(&sk);
    assert_has(&sk, 0.5, 1.0, 1.0);
    assert_has(&sk, 3.5, 1.0, 1.0);
}

// Fixture 4: half hip, half gable (right edge frozen, others at unit speed).

#[test]
fn half_hip_half_gable_with_frozen_right_edge() {
    let square = vec![v(0.0, 0.0), v(2.0, 0.0), v(2.0, 2.0), v(0.0, 2.0)];
    let sk = expect_ok(weighted_skeleton(&square, &[], &[vec![1.0, 0.0, 1.0, 1.0]]), "half hip square");
    assert_valid(&sk);
    assert_has(&sk, 1.0, 1.0, 1.0);
    assert_has(&sk, 2.0, 1.0, 1.0);
}

// Fixture 5: uniform weight 2.

#[test]
fn uniform_weight_two_on_unit_square_meets_at_quarter_time() {
    let sk = expect_ok(weighted_skeleton(&unit_square(), &[], &[vec![2.0; 4]]), "unit square at weight 2");
    assert_valid(&sk);
    assert_has(&sk, 0.5, 0.5, 0.25);
    assert!(close(sk.max_time(), 0.25), "max_time should be 0.25, got {}", sk.max_time());
}

#[test]
fn uniform_weight_two_on_rectangle_halves_plain_times() {
    let sk = expect_ok(weighted_skeleton(&rect4x2(), &[], &[vec![2.0; 4]]), "rectangle at weight 2");
    assert_valid(&sk);
    assert_has(&sk, 1.0, 1.0, 0.5);
    assert_has(&sk, 3.0, 1.0, 0.5);
    assert!(close(sk.max_time(), 0.5), "max_time should be 0.5, got {}", sk.max_time());
}

// Fixture 6: roof_skeleton (slope s maps to weight 1/s, infinite slope to 0; time is roof height).

#[test]
fn roof_gable_slopes_give_ridge_at_height_one() {
    let slopes = [vec![1.0, f64::INFINITY, 1.0, f64::INFINITY]];
    let sk = expect_ok(roof_skeleton(&rect4x2(), &[], &slopes), "gable roof");
    assert_valid(&sk);
    assert_has(&sk, 0.0, 1.0, 1.0);
    assert_has(&sk, 4.0, 1.0, 1.0);
}

#[test]
fn roof_mixed_slopes_give_ridge_ends_at_height_one() {
    let slopes = [vec![1.0, 2.0, 1.0, 2.0]];
    let sk = expect_ok(roof_skeleton(&rect4x2(), &[], &slopes), "mixed-slope roof");
    assert_valid(&sk);
    assert_has(&sk, 0.5, 1.0, 1.0);
    assert_has(&sk, 3.5, 1.0, 1.0);
}

// Fixture 7: clockwise input keeps input indexing.

#[test]
fn clockwise_rectangle_keeps_input_edge_indexing() {
    let clockwise = vec![v(0.0, 0.0), v(0.0, 2.0), v(4.0, 2.0), v(4.0, 0.0)];
    // Edges: left, top, right, bottom.
    let sk = expect_ok(weighted_skeleton(&clockwise, &[], &[vec![0.0, 1.0, 0.0, 1.0]]), "clockwise gable rectangle");
    assert_valid(&sk);
    assert_has(&sk, 0.0, 1.0, 1.0);
    assert_has(&sk, 4.0, 1.0, 1.0);
    assert!(close(sk.max_time(), 1.0), "max_time should be 1, got {}", sk.max_time());
}

// Fixture 8: collinear points.

#[test]
fn collinear_point_cleaned_away_with_matching_weights() {
    let outer = vec![v(0.0, 0.0), v(2.0, 0.0), v(4.0, 0.0), v(4.0, 2.0), v(0.0, 2.0)];
    let sk = expect_ok(weighted_skeleton(&outer, &[], &[vec![1.0; 5]]), "rectangle with collinear point");
    assert_valid(&sk);
    assert_eq!(sk.rings[0].len(), 4, "collinear point should be removed, rings {:?}", sk.rings);
    let plain = expect_ok(straight_skeleton(&rect4x2(), &[]), "plain rectangle");
    let (a, b) = (sorted_nodes(&plain), sorted_nodes(&sk));
    assert_eq!(a.len(), b.len(), "interior node count differs: plain {a:?}, weighted {b:?}");
    assert_has(&sk, 1.0, 1.0, 1.0);
    assert_has(&sk, 3.0, 1.0, 1.0);
}

#[test]
fn collinear_halves_with_different_weights_are_bad_weights() {
    let outer = vec![v(0.0, 0.0), v(2.0, 0.0), v(4.0, 0.0), v(4.0, 2.0), v(0.0, 2.0)];
    let result = weighted_skeleton(&outer, &[], &[vec![1.0, 0.5, 1.0, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

// Fixture 9: errors.

#[test]
fn weights_ring_with_wrong_edge_count_is_bad_weights() {
    let result = weighted_skeleton(&rect4x2(), &[], &[vec![1.0, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn negative_weight_is_bad_weights() {
    let result = weighted_skeleton(&rect4x2(), &[], &[vec![1.0, -1.0, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn nan_weight_is_bad_weights() {
    let result = weighted_skeleton(&rect4x2(), &[], &[vec![1.0, f64::NAN, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn infinite_weight_is_bad_weights() {
    let result = weighted_skeleton(&rect4x2(), &[], &[vec![1.0, f64::INFINITY, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn holes_without_weights_ring_are_bad_weights() {
    let result = weighted_skeleton(&courtyard_outer(), &[courtyard_hole()], &[vec![1.0; 4]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn roof_zero_slope_is_bad_weights() {
    let result = roof_skeleton(&rect4x2(), &[], &[vec![1.0, 0.0, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn roof_negative_slope_is_bad_weights() {
    let result = roof_skeleton(&rect4x2(), &[], &[vec![1.0, -2.0, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

#[test]
fn roof_nan_slope_is_bad_weights() {
    let result = roof_skeleton(&rect4x2(), &[], &[vec![1.0, f64::NAN, 1.0, 1.0]]);
    assert_eq!(result.err(), Some(SkeletonError::BadWeights));
}

// Fixture 10: courtyard with every edge at weight 1.

#[test]
fn courtyard_all_unit_weights_max_time_is_one_and_a_half() {
    let outer = courtyard_outer();
    let holes = [courtyard_hole()];
    let sk = expect_ok(weighted_skeleton(&outer, &holes, &ones_for(&outer, &holes)), "weighted courtyard");
    assert_valid(&sk);
    assert!(close(sk.max_time(), 1.5), "max_time should be 1.5, got {}", sk.max_time());
}
