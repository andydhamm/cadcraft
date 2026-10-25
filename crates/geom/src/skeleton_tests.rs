//! Analytic fixtures for the straight skeleton (assisted draft, 2026-10-25).
//!
//! Every expected value here is derived by hand from the polygon's geometry; the tests are written ahead of the
//! implementation and fail with `todo!` until it lands.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::Vec2;
use crate::skeleton::*;

const SQRT3: f64 = 1.732_050_807_568_877_2;
const SQRT2: f64 = std::f64::consts::SQRT_2;

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

fn ring(points: &[(f64, f64)]) -> Vec<Vec2> {
    points.iter().map(|&(x, y)| v(x, y)).collect()
}

/// Signed shoelace area: positive for counter-clockwise rings.
fn area(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    let mut twice = 0.0;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        twice += a.x * b.y - b.x * a.y;
    }
    twice / 2.0
}

/// Outer area minus the absolute areas of the holes, summed over regions.
fn total_area(regions: &[OffsetRegion]) -> f64 {
    regions.iter().map(|r| area(&r.outer).abs() - r.holes.iter().map(|h| area(h).abs()).sum::<f64>()).sum()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn has_node(sk: &Skeleton, x: f64, y: f64, time: f64) -> bool {
    sk.nodes.iter().any(|n| close(n.pos.x, x) && close(n.pos.y, y) && close(n.time, time))
}

fn node_index(sk: &Skeleton, x: f64, y: f64) -> Option<usize> {
    sk.nodes.iter().position(|n| close(n.pos.x, x) && close(n.pos.y, y))
}

/// Interior nodes are everything after the ring vertices, which come first in `nodes`.
fn interior_nodes(sk: &Skeleton) -> Vec<SkeletonNode> {
    let ring_vertices: usize = sk.rings.iter().map(|r| r.len()).sum();
    sk.nodes[ring_vertices..].to_vec()
}

fn assert_clean(sk: &Skeleton) {
    let problems = check(sk, 1e-6);
    assert!(problems.is_empty(), "skeleton invariants failed:\n{}", problems.join("\n"));
}

fn ok<T>(r: Result<T, SkeletonError>) -> T {
    match r {
        Ok(t) => t,
        Err(e) => panic!("unexpected error: {e}"),
    }
}

fn square() -> Vec<Vec2> {
    ring(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])
}

fn rectangle_4x2() -> Vec<Vec2> {
    ring(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)])
}

fn l_shape() -> Vec<Vec2> {
    ring(&[(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)])
}

fn u_shape() -> Vec<Vec2> {
    ring(&[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0), (2.0, 3.0), (2.0, 1.0), (1.0, 1.0), (1.0, 3.0), (0.0, 3.0)])
}

fn dumbbell() -> Vec<Vec2> {
    ring(&[
        (0.0, 0.0),
        (4.0, 0.0),
        (4.0, 1.5),
        (8.0, 1.5),
        (8.0, 0.0),
        (12.0, 0.0),
        (12.0, 4.0),
        (8.0, 4.0),
        (8.0, 2.5),
        (4.0, 2.5),
        (4.0, 4.0),
        (0.0, 4.0),
    ])
}

#[test]
fn square_has_four_faces_one_centre_node_and_four_arcs() {
    let sk = ok(straight_skeleton(&square(), &[]));
    assert_clean(&sk);
    assert_eq!(sk.faces.len(), 4);
    let interior = interior_nodes(&sk);
    assert_eq!(interior.len(), 1);
    assert!(has_node(&sk, 0.5, 0.5, 0.5));
    assert!(close(sk.max_time(), 0.5));
    assert_eq!(sk.arcs.len(), 4);
}

#[test]
fn rectangle_4x2_has_two_interior_nodes_at_time_one() {
    let sk = ok(straight_skeleton(&rectangle_4x2(), &[]));
    assert_clean(&sk);
    assert_eq!(interior_nodes(&sk).len(), 2);
    assert!(has_node(&sk, 1.0, 1.0, 1.0));
    assert!(has_node(&sk, 3.0, 1.0, 1.0));
    assert!(close(sk.max_time(), 1.0));
}

#[test]
fn rectangle_4x2_has_five_arcs_and_four_faces() {
    let sk = ok(straight_skeleton(&rectangle_4x2(), &[]));
    assert_clean(&sk);
    assert_eq!(sk.arcs.len(), 5);
    assert_eq!(sk.faces.len(), 4);
}

#[test]
fn rectangle_4x2_long_edge_faces_have_four_nodes_and_short_edge_faces_three() {
    let sk = ok(straight_skeleton(&rectangle_4x2(), &[]));
    assert_clean(&sk);
    // Edges of the outer ring in order: bottom (0), right (1), top (2), left (3).
    let counts: Vec<usize> = sk.faces.iter().map(|f| f.nodes.len()).collect();
    assert_eq!(sk.faces[0].edge, 0);
    assert_eq!(sk.faces[0].nodes.len(), 4, "bottom face counts: {counts:?}");
    assert_eq!(sk.faces[1].nodes.len(), 3, "right face counts: {counts:?}");
    assert_eq!(sk.faces[2].nodes.len(), 4, "top face counts: {counts:?}");
    assert_eq!(sk.faces[3].nodes.len(), 3, "left face counts: {counts:?}");
}

#[test]
fn rectangle_4x2_interior_node_height_on_slope_half_is_half() {
    let sk = ok(straight_skeleton(&rectangle_4x2(), &[]));
    assert_clean(&sk);
    let idx = node_index(&sk, 1.0, 1.0).expect("interior node at (1,1)");
    assert!(close(sk.height(idx, 0.5), 0.5));
}

#[test]
fn right_isosceles_triangle_has_incentre_at_two_minus_root_two() {
    let tri = ring(&[(0.0, 0.0), (2.0, 0.0), (0.0, 2.0)]);
    let sk = ok(straight_skeleton(&tri, &[]));
    assert_clean(&sk);
    let r = 2.0 - SQRT2;
    assert_eq!(interior_nodes(&sk).len(), 1);
    assert!(has_node(&sk, r, r, r));
}

#[test]
fn regular_hexagon_has_single_centre_node_at_apothem() {
    let hex = ring(&[(1.0, 0.0), (0.5, SQRT3 / 2.0), (-0.5, SQRT3 / 2.0), (-1.0, 0.0), (-0.5, -SQRT3 / 2.0), (0.5, -SQRT3 / 2.0)]);
    let sk = ok(straight_skeleton(&hex, &[]));
    assert_clean(&sk);
    assert_eq!(interior_nodes(&sk).len(), 1);
    assert!(has_node(&sk, 0.0, 0.0, SQRT3 / 2.0));
    assert!(close(sk.max_time(), SQRT3 / 2.0));
}

#[test]
fn l_shape_has_six_faces_and_max_time_half() {
    let sk = ok(straight_skeleton(&l_shape(), &[]));
    assert_clean(&sk);
    assert_eq!(sk.faces.len(), 6);
    assert!(close(sk.max_time(), 0.5));
}

#[test]
fn l_shape_inset_quarter_has_rectilinear_area() {
    // area - perimeter*d + (convex - reflex)*d^2 = 3 - 8*0.25 + 4*0.0625 = 1.25
    let regions = ok(inset(&l_shape(), &[], 0.25));
    assert_eq!(regions.len(), 1);
    assert!(close(total_area(&regions), 1.25));
}

#[test]
fn rectangle_4x2_inset_half_is_three_by_one() {
    let regions = ok(inset(&rectangle_4x2(), &[], 0.5));
    assert_eq!(regions.len(), 1);
    assert!(close(total_area(&regions), 3.0));
}

#[test]
fn rectangle_4x2_inset_at_and_beyond_max_time_is_empty() {
    let at_max = ok(inset(&rectangle_4x2(), &[], 1.0));
    assert!(at_max.is_empty(), "inset at max_time should vanish, got {} regions", at_max.len());
    let beyond = ok(inset(&rectangle_4x2(), &[], 1.5));
    assert!(beyond.is_empty(), "inset past max_time should vanish, got {} regions", beyond.len());
}

#[test]
fn rectangle_4x2_inset_zero_returns_cleaned_input_area_eight() {
    let regions = ok(inset(&rectangle_4x2(), &[], 0.0));
    assert_eq!(regions.len(), 1);
    assert!(close(total_area(&regions), 8.0));
}

#[test]
fn courtyard_has_eight_faces_and_max_time_one_and_half() {
    let outer = ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    let hole = ring(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]); // clockwise
    let sk = ok(straight_skeleton(&outer, std::slice::from_ref(&hole)));
    assert_clean(&sk);
    assert_eq!(sk.faces.len(), 8);
    assert!(close(sk.max_time(), 1.5));
}

#[test]
fn courtyard_hole_orientation_does_not_change_faces() {
    let outer = ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    let hole_ccw = ring(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)]);
    let sk = ok(straight_skeleton(&outer, std::slice::from_ref(&hole_ccw)));
    assert_clean(&sk);
    assert_eq!(sk.faces.len(), 8);
    assert!(close(sk.max_time(), 1.5));
}

#[test]
fn courtyard_inset_one_has_one_holed_region_of_area_twenty_eight() {
    // Outer 8x8 less the hole grown to 6x6: 64 - 36 = 28.
    let outer = ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    let hole = ring(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
    let regions = ok(inset(&outer, std::slice::from_ref(&hole), 1.0));
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].holes.len(), 1);
    assert!(close(total_area(&regions), 28.0));
}

#[test]
fn rectangle_with_collinear_point_and_repeated_first_point_is_cleaned() {
    let messy = ring(&[(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0), (0.0, 0.0)]);
    let sk = ok(straight_skeleton(&messy, &[]));
    assert_clean(&sk);
    assert_eq!(sk.rings[0].len(), 4);
    assert_eq!(interior_nodes(&sk).len(), 2);
    assert!(has_node(&sk, 1.0, 1.0, 1.0));
    assert!(has_node(&sk, 3.0, 1.0, 1.0));
}

#[test]
fn clockwise_rectangle_is_reoriented_counter_clockwise() {
    let cw = ring(&[(0.0, 0.0), (0.0, 2.0), (4.0, 2.0), (4.0, 0.0)]);
    let sk = ok(straight_skeleton(&cw, &[]));
    assert_clean(&sk);
    assert!(area(&sk.rings[0]) > 0.0, "outer ring must be counter-clockwise");
    assert!(close(area(&sk.rings[0]), 8.0));
    assert_eq!(interior_nodes(&sk).len(), 2);
}

#[test]
fn two_points_are_rejected_as_too_few_vertices() {
    let r = straight_skeleton(&ring(&[(0.0, 0.0), (1.0, 0.0)]), &[]);
    assert_eq!(r.err(), Some(SkeletonError::TooFewVertices));
}

#[test]
fn bow_tie_is_rejected_as_not_simple() {
    let bow = ring(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)]);
    let r = straight_skeleton(&bow, &[]);
    assert_eq!(r.err(), Some(SkeletonError::NotSimple));
}

#[test]
fn hole_outside_outer_ring_is_rejected() {
    let outer = square();
    let hole = ring(&[(5.0, 5.0), (6.0, 5.0), (6.0, 6.0), (5.0, 6.0)]);
    let r = straight_skeleton(&outer, std::slice::from_ref(&hole));
    assert_eq!(r.err(), Some(SkeletonError::HoleOutside));
}

#[test]
fn nan_coordinate_is_rejected_as_not_finite() {
    let bad = ring(&[(0.0, 0.0), (1.0, 0.0), (f64::NAN, 1.0), (0.0, 1.0)]);
    let r = straight_skeleton(&bad, &[]);
    assert_eq!(r.err(), Some(SkeletonError::NotFinite));
}

#[test]
fn negative_inset_distance_is_rejected_as_not_finite() {
    let r = inset(&square(), &[], -1.0);
    assert_eq!(r.err(), Some(SkeletonError::NotFinite));
}

#[test]
fn rectangle_4x2_outset_by_one_has_mitred_area_twenty_four_and_no_holes() {
    let r = ok(outset(&rectangle_4x2(), &[], 1.0));
    assert!(r.holes.is_empty());
    assert!(close(area(&r.outer).abs(), 24.0));
}

#[test]
fn unit_square_outset_by_half_has_area_four() {
    let r = ok(outset(&square(), &[], 0.5));
    assert!(r.holes.is_empty());
    assert!(close(area(&r.outer).abs(), 4.0));
}

#[test]
fn dumbbell_inset_quarter_is_one_region() {
    let regions = ok(inset(&dumbbell(), &[], 0.25));
    assert_eq!(regions.len(), 1);
}

#[test]
fn dumbbell_inset_point_six_splits_into_two_equal_regions() {
    let regions = ok(inset(&dumbbell(), &[], 0.6));
    assert_eq!(regions.len(), 2);
    let a0 = total_area(&regions[..1]);
    let a1 = total_area(&regions[1..]);
    assert!((a0 - a1).abs() < 1e-6, "halves differ: {a0} vs {a1}");
    for a in [a0, a1] {
        assert!((7.0..=8.5).contains(&a), "half area {a} outside 7.0..8.5");
    }
}

#[test]
fn u_shape_has_max_time_half_and_inset_quarter_area() {
    let sk = ok(straight_skeleton(&u_shape(), &[]));
    assert_clean(&sk);
    assert!(close(sk.max_time(), 0.5));
    // 7 - 16*0.25 + 4*0.0625 = 3.25
    let regions = ok(inset(&u_shape(), &[], 0.25));
    assert!(close(total_area(&regions), 3.25));
}
