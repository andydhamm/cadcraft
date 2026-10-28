//! Stair calculator — pure 2D geometry for stair flights and their railings.
//!
//! Implements the IRC-based riser/tread calculation used to lay out a straight
//! flight between two levels. All limits are data (constants), not logic in
//! commands, so that commands/CLI/MCP/`ARCHDETECT` agree (design note 12).
//!
//! The flight is one closed stepped prism: the elevation profile in flight
//! coordinates is extruded across the width — the same trick used for walls.
//! No CSG.

use cadcraft_geom::Vec2;

/// Maximum allowable riser height (IRC §R311.7.2.1).
pub const MAX_RISER: f64 = 0.196;
/// Minimum tread depth (IRC §R311.7.2.1).
pub const MIN_TREAD: f64 = 0.254;
/// Minimum clear headroom above a stair (IRC §R311.1.1.6).
pub const MIN_HEADROOM: f64 = 2.032;
/// Target riser height the optimiser minimises distance to.
pub const TARGET_RISER: f64 = 0.16875;
/// Minimum handrail height above the nosing line.
pub const MIN_HANDRAIL: f64 = 0.864;
/// Maximum handrail height above the nosing line.
pub const MAX_HANDRAIL: f64 = 0.965;
/// Guard height at landings and stair wells (IRC §R311.19).
pub const GUARD_HEIGHT: f64 = 0.914;
/// Maximum clear gap between balusters (IRC §R312.11).
pub const MAX_BALUSTER_GAP: f64 = 0.1016;
/// Minimum total rise for a usable flight (rejects sub-usable steps).
pub const MIN_RISE: f64 = 0.15;

/// A fully-solved straight stair flight in plan coordinates.
///
/// `risers` is the count of vertical rises; `treads` is the number of
/// horizontal steps (`risers - 1`). `run` is the total plan depth of the
/// flight (`treads * tread_depth`); `landing_depth` is the landing depth
/// in front of the top of the flight, at least as wide as the stair is wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StairPlan {
    pub risers: usize,
    pub riser_height: f64,
    pub treads: usize,
    pub tread_depth: f64,
    pub run: f64,
    pub landing_depth: f64,
}

/// Compute the stair flight plan.
///
/// Picks the riser count `n` that minimises `|rise/n − TARGET_RISER|`
/// subject to `rise/n ≤ max_riser`, scanning a bounded 64-wide window
/// starting at `ceil(rise / max_riser)`. On an exact tie, the larger
/// `n` wins.
///
/// Returns `None` for non-finite inputs, `rise < MIN_RISE`, `width ≤ 0`,
/// `max_riser ≤ 0`, `min_tread ≤ 0`, or when the result would have zero
/// treads.
pub fn plan_stair(rise: f64, width: f64, tread: Option<f64>, max_riser: f64, min_tread: f64) -> Option<StairPlan> {
    // Reject non-finite inputs.
    if !rise.is_finite() || !width.is_finite() || !max_riser.is_finite() || !min_tread.is_finite() {
        return None;
    }
    if let Some(t) = tread {
        if !t.is_finite() {
            return None;
        }
    }
    // Reject out-of-range inputs.
    if rise < MIN_RISE || width <= 0.0 || max_riser <= 0.0 || min_tread <= 0.0 {
        return None;
    }

    // Smallest `n` such that `rise / n <= max_riser`.
    let ratio = rise / max_riser;
    if !ratio.is_finite() {
        return None;
    }

    // The candidate count must be representable and leave room for the 64-wide
    // scan window; otherwise the flight is unusable (avoids an out-of-range cast
    // and a debug overflow in the range end).
    if ratio >= usize::MAX as f64 {
        return None;
    }
    let n_min = ratio.ceil() as usize;
    if n_min == 0 {
        return None;
    }

    // Scan a bounded 64-wide window of candidate riser counts.
    let mut best_n: Option<usize> = None;
    let mut best_dist: Option<f64> = None;
    for n in n_min..n_min.saturating_add(64) {
        let riser_h = rise / (n as f64);
        if riser_h > max_riser {
            continue;
        }
        let dist = (riser_h - TARGET_RISER).abs();
        if let Some(b) = best_dist {
            if dist < b {
                best_dist = Some(dist);
                best_n = Some(n);
            } else if dist == b && n > best_n? {
                best_n = Some(n);
            }
        } else {
            best_dist = Some(dist);
            best_n = Some(n);
        }
    }
    let n = best_n?;
    let treads = n - 1;
    if treads == 0 {
        return None;
    }
    let tread_depth = tread.unwrap_or(min_tread);
    let riser_height = rise / (n as f64);
    let run = (treads as f64) * tread_depth;
    let landing_depth = width.max(tread_depth);
    Some(StairPlan { risers: n, riser_height, treads, tread_depth, run, landing_depth })
}

/// Whether there is at least `min_headroom` of clear space between the top of
/// the stair at `stair_z` and the underside of the slab at `slab_underside`.
///
/// Both are absolute z-coordinates (meters, level datum). The reference for
/// headroom is the slab underside, not the floor level above.
pub fn headroom_ok(stair_z: f64, slab_underside: f64, min_headroom: f64) -> bool {
    if !stair_z.is_finite() || !slab_underside.is_finite() || !min_headroom.is_finite() {
        return false;
    }
    let clearance = slab_underside - stair_z;
    clearance.is_finite() && clearance >= min_headroom
}

/// Flight plan rectangle: the four corners of the flight, CCW.
///
/// `s.start` is the centre of the bottom nosing line; the rectangle runs `p.run`
/// along `s.direction` and is centred on `s.start` with width `s.width`.
/// Returns an empty vector when any input is non-finite or the width or run
/// is invalid (never clamps silently, never panics).
pub fn flight_polygon(s: &cadcraft_doc::arch::Stair, p: &StairPlan) -> Vec<Vec2> {
    if !s.start.is_finite() || !s.direction.is_finite() || !s.width.is_finite() || !p.run.is_finite() || s.width < 0.0 || p.run < 0.0 {
        return Vec::new();
    }
    let dir = Vec2::from_angle(s.direction);
    let perp = Vec2::new(-dir.y, dir.x);
    let half = s.width / 2.0;
    let near_lo = s.start - perp * half;
    let near_hi = s.start + perp * half;
    let far_hi = s.start + dir * p.run + perp * half;
    let far_lo = s.start + dir * p.run - perp * half;
    crate::poly::ensure_ccw(&[near_lo, near_hi, far_hi, far_lo])
}

/// Smallest riser index `i` with `i * riser_height > slab_underside -
/// min_headroom` (strict), returned as the tread distance `(i - 1) *
/// tread_depth` measured from the bottom nosing line. Returns `None` when
/// no such riser exists within the flight or the distance would exceed
/// `p.run`.
pub fn well_start(_s: &cadcraft_doc::arch::Stair, p: &StairPlan, slab_underside: f64, min_headroom: f64) -> Option<f64> {
    if !p.riser_height.is_finite() || !p.tread_depth.is_finite() || !p.run.is_finite() || !slab_underside.is_finite() || !min_headroom.is_finite() {
        return None;
    }
    if p.riser_height <= 0.0 || p.tread_depth <= 0.0 || p.treads < 1 {
        return None;
    }
    let threshold = slab_underside - min_headroom;
    if !threshold.is_finite() {
        return None;
    }
    let mut upper: usize = p.treads.saturating_add(1);
    // Backstop for hostile stored data: treads could be absurdly large.
    if upper > 1_000_000 {
        upper = 1_000_000;
    }
    for i in 1..=upper {
        if (i as f64) * p.riser_height > threshold {
            let dist = (i - 1) as f64 * p.tread_depth;
            if dist > p.run {
                return None;
            }
            return Some(dist);
        }
    }
    None
}

/// Rectangle ringed by the stair's width, from `well_start` to the flight end
/// (`p.run`). Returns `None` when `well_start` is `None` or when the
/// remaining flight length is smaller than `1e-9` m.
pub fn well_polygon(s: &cadcraft_doc::arch::Stair, p: &StairPlan, slab_underside: f64, min_headroom: f64) -> Option<Vec<Vec2>> {
    let start = well_start(s, p, slab_underside, min_headroom)?;
    if !s.start.is_finite() || !s.direction.is_finite() || !s.width.is_finite() || !p.run.is_finite() {
        return None;
    }
    if s.width <= 0.0 || p.run < 0.0 {
        return None;
    }
    if p.run - start < 1e-9 {
        return None;
    }
    let dir = Vec2::from_angle(s.direction);
    let perp = Vec2::new(-dir.y, dir.x);
    let half = s.width / 2.0;
    let near_lo = s.start + dir * start - perp * half;
    let near_hi = s.start + dir * start + perp * half;
    let far_hi = s.start + dir * p.run + perp * half;
    let far_lo = s.start + dir * p.run - perp * half;
    Some(crate::poly::ensure_ccw(&[near_lo, near_hi, far_hi, far_lo]))
}

/// Guard path ringing the well: an open polyline of three sides (the two long
/// sides and the near end) with the arrival (top) end left open.
pub fn well_guard_path(s: &cadcraft_doc::arch::Stair, p: &StairPlan, slab_underside: f64, min_headroom: f64) -> Option<Vec<Vec2>> {
    let start = well_start(s, p, slab_underside, min_headroom)?;
    if !s.start.is_finite() || !s.direction.is_finite() || !s.width.is_finite() || !p.run.is_finite() {
        return None;
    }
    if s.width <= 0.0 || p.run < 0.0 {
        return None;
    }
    if p.run - start < 1e-9 {
        return None;
    }
    let dir = Vec2::from_angle(s.direction);
    let perp = Vec2::new(-dir.y, dir.x);
    let half = s.width / 2.0;
    let near_lo = s.start + dir * start - perp * half;
    let near_hi = s.start + dir * start + perp * half;
    let far_hi = s.start + dir * p.run + perp * half;
    let far_lo = s.start + dir * p.run - perp * half;
    // Three consecutive sides of the well rectangle: far_hi -> near_hi ->
    // near_lo -> far_lo. The side joining the two arrival corners (far_lo ->
    // far_hi) is omitted, leaving the arrival end open.
    Some(vec![far_hi, near_hi, near_lo, far_lo])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_2_7_gives_16_risers_of_168_75() {
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        assert_eq!(p.risers, 16);
        assert_eq!(p.riser_height, 0.16875);
        assert_eq!(p.treads, 15);
        assert_eq!(p.tread_depth, MIN_TREAD);
        assert_eq!(p.run, 15.0 * MIN_TREAD);
        assert_eq!(p.landing_depth, 1.0);
    }

    #[test]
    fn plan_0_9_gives_5_risers_of_180() {
        let p = plan_stair(0.9, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        assert_eq!(p.risers, 5);
        assert_eq!(p.riser_height, 0.18);
        assert_eq!(p.treads, 4);
        assert_eq!(p.tread_depth, MIN_TREAD);
        assert_eq!(p.run, 4.0 * MIN_TREAD);
        assert_eq!(p.landing_depth, 1.0);
    }

    #[test]
    fn never_exceeds_the_max_riser() {
        for (rise, max_riser) in [(0.15, 0.196), (0.3, 0.196), (0.9, 0.196), (2.7, 0.196), (5.4, 0.196), (0.3, 0.05), (2.7, 0.1)] {
            if let Some(p) = plan_stair(rise, 1.0, None, max_riser, MIN_TREAD) {
                assert!(p.riser_height <= max_riser, "riser {0} exceeds max {1} for rise {2}", p.riser_height, max_riser, rise,);
            }
        }
    }

    #[test]
    fn plan_stair_refuses_garbage() {
        assert_eq!(plan_stair(f64::NAN, 1.0, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(f64::INFINITY, 1.0, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, f64::NAN, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, 1.0, None, f64::INFINITY, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, 1.0, None, f64::NAN, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, 1.0, None, MAX_RISER, f64::NAN), None);
        assert_eq!(plan_stair(2.7, 1.0, Some(f64::NAN), MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(0.0, 1.0, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(-1.0, 1.0, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, 0.0, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, -1.0, None, MAX_RISER, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, 1.0, None, 0.0, MIN_TREAD), None);
        assert_eq!(plan_stair(2.7, 1.0, None, MAX_RISER, 0.0), None);
    }

    #[test]
    fn tread_depth_override_is_honoured() {
        let p = plan_stair(2.7, 1.0, Some(0.3), MAX_RISER, MIN_TREAD).unwrap();
        assert_eq!(p.tread_depth, 0.3);
        assert_eq!(p.run, 15.0 * 0.3);
    }

    #[test]
    fn headroom_uses_the_slab_underside() {
        assert!(headroom_ok(0.0, 2.032, MIN_HEADROOM));
        assert!(!headroom_ok(0.0, 2.031, MIN_HEADROOM));
        assert!(headroom_ok(0.5, 2.6, MIN_HEADROOM));
        assert!(!headroom_ok(0.6, 2.6, MIN_HEADROOM));
        assert!(!headroom_ok(f64::NAN, 3.0, MIN_HEADROOM));
        assert!(!headroom_ok(0.0, f64::NAN, MIN_HEADROOM));
        assert!(!headroom_ok(0.0, 3.0, f64::NAN));
        assert!(!headroom_ok(5.0, 3.0, MIN_HEADROOM));
    }

    #[test]
    fn plan_stair_is_bounded_for_huge_ratios() {
        // Ratio overflowing to infinity must return None.
        assert_eq!(plan_stair(1e300, 1.0, None, 1.0, MIN_TREAD), None);
        assert_eq!(plan_stair(1e300, 1.0, None, 1e-300, MIN_TREAD), None);
        assert_eq!(plan_stair(f64::MAX, 1.0, None, 1e-10, MIN_TREAD), None);
    }

    fn approx_poly(poly: &[Vec2], p: &Vec2) -> bool {
        poly.iter().any(|q| (q.x - p.x).abs() < 1e-9 && (q.y - p.y).abs() < 1e-9)
    }

    fn poly_area(poly: &[Vec2]) -> f64 {
        if poly.len() < 3 {
            return 0.0;
        }
        let n = poly.len();
        let mut s = 0.0;
        for i in 0..n {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            s += a.x * b.y - a.y * b.x;
        }
        s.abs() / 2.0
    }

    /// Stair from the phase-5 brief: start (2, 1), direction 0, width 1.
    fn sample_stair() -> cadcraft_doc::arch::Stair {
        let mut s = cadcraft_doc::arch::Stair::default();
        s.start = Vec2::new(2.0, 1.0);
        s.direction = 0.0;
        s.width = 1.0;
        s
    }

    #[test]
    fn flight_polygon_is_centred_on_the_start() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        let poly = flight_polygon(&s, &p);
        // Width centred on start (y = 1.0 +/- 0.5); the flight runs from
        // start.x to start.x + run along direction 0.
        assert_eq!(poly.len(), 4);
        assert!(approx_poly(&poly, &Vec2::new(2.0, 0.5)));
        assert!(approx_poly(&poly, &Vec2::new(2.0, 1.5)));
        assert!(approx_poly(&poly, &Vec2::new(5.81, 1.5)));
        assert!(approx_poly(&poly, &Vec2::new(5.81, 0.5)));
    }

    #[test]
    fn well_starts_at_the_third_riser() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        // i = 3 (top z = 3 * 0.16875 = 0.50625) is the first riser above
        // slab_underside - min_headroom = 2.4 - 2.032 = 0.368.
        let w = well_start(&s, &p, 2.4, MIN_HEADROOM);
        assert_eq!(w, Some((3 - 1) as f64 * p.tread_depth));
    }

    #[test]
    fn well_polygon_area_is_3_302() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        let poly = well_polygon(&s, &p, 2.4, MIN_HEADROOM).expect("well present");
        assert_eq!(poly.len(), 4);
        // Well x 2.508..5.81 (length 3.302) at width 1.0 -> area 3.302.
        assert!((poly_area(&poly) - 3.302).abs() < 1e-9);
        assert!(approx_poly(&poly, &Vec2::new(2.508, 0.5)));
        assert!(approx_poly(&poly, &Vec2::new(2.508, 1.5)));
        assert!(approx_poly(&poly, &Vec2::new(5.81, 1.5)));
        assert!(approx_poly(&poly, &Vec2::new(5.81, 0.5)));
    }

    #[test]
    fn well_guard_leaves_the_arrival_end_open() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        let path = well_guard_path(&s, &p, 2.4, MIN_HEADROOM).expect("well present");
        // An open polyline of three sides: four points, each consecutive pair
        // joined along a rectangle side (same x or same y); the side that
        // would close the arrival end is absent.
        assert_eq!(path.len(), 4);
        assert!(approx_poly(&path, &Vec2::new(2.508, 0.5)));
        assert!(approx_poly(&path, &Vec2::new(2.508, 1.5)));
        assert!(approx_poly(&path, &Vec2::new(5.81, 0.5)));
        assert!(approx_poly(&path, &Vec2::new(5.81, 1.5)));
        for (a, b) in path.iter().zip(path.iter().skip(1)) {
            assert!((a.x - b.x).abs() < 1e-9 || (a.y - b.y).abs() < 1e-9);
        }
    }

    #[test]
    fn well_is_none_when_headroom_is_clear() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        // Slab underside well above the top of the flight: no riser top is
        // within min_headroom of the underside, so there is no well.
        assert_eq!(well_start(&s, &p, 5.0, MIN_HEADROOM), None);
        assert_eq!(well_polygon(&s, &p, 5.0, MIN_HEADROOM), None);
        assert_eq!(well_guard_path(&s, &p, 5.0, MIN_HEADROOM), None);
    }

    #[test]
    fn flight_polygon_with_vertical_direction() {
        let mut s = sample_stair();
        s.direction = std::f64::consts::FRAC_PI_2;
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        let poly = flight_polygon(&s, &p);
        assert_eq!(poly.len(), 4);
        // Direction up (0, 1): flight runs from (2,1) to (2, 1+3.81)
        // Width 1 centred: x in [1.5, 2.5]
        assert!(approx_poly(&poly, &Vec2::new(1.5, 1.0)));
        assert!(approx_poly(&poly, &Vec2::new(2.5, 1.0)));
        assert!(approx_poly(&poly, &Vec2::new(2.5, 4.81)));
        assert!(approx_poly(&poly, &Vec2::new(1.5, 4.81)));
    }

    #[test]
    fn well_polygon_with_left_direction() {
        let mut s = sample_stair();
        s.direction = std::f64::consts::PI;
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        let poly = well_polygon(&s, &p, 2.4, MIN_HEADROOM).expect("well present");
        assert_eq!(poly.len(), 4);
        // Direction left (-1, 0): well runs from x=2-0.508=1.492 to x=2-3.81=-1.81
        // Width 1 centred: y in [0.5, 1.5]
        assert!(approx_poly(&poly, &Vec2::new(1.492, 0.5)));
        assert!(approx_poly(&poly, &Vec2::new(1.492, 1.5)));
        assert!(approx_poly(&poly, &Vec2::new(-1.81, 1.5)));
        assert!(approx_poly(&poly, &Vec2::new(-1.81, 0.5)));
    }

    #[test]
    fn well_start_at_bottom_when_slab_is_low() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        // Slab underside at 0.0: threshold = 0.0 - 2.032 = -2.032
        // First riser top 0.16875 > -2.032, so well starts at (1-1)*tread = 0
        assert_eq!(well_start(&s, &p, 0.0, MIN_HEADROOM), Some(0.0));
    }

    #[test]
    fn well_start_boundary_is_strict() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        // Make riser 3 top exactly at threshold: 3*0.16875 = 0.50625
        // threshold = slab_underside - 2.032 = 0.50625 -> slab_underside = 2.53825
        // i=3: 0.50625 > 0.50625? No (strict). i=4: 0.675 > 0.50625? Yes.
        // So well starts at (4-1)*tread = 3*0.254 = 0.762
        let slab = 3.0 * p.riser_height + MIN_HEADROOM;
        assert_eq!(well_start(&s, &p, slab, MIN_HEADROOM), Some(3.0 * p.tread_depth));
    }

    #[test]
    fn flight_polygon_rejects_negative_width() {
        let mut s = sample_stair();
        s.width = -1.0;
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        assert!(flight_polygon(&s, &p).is_empty());
    }

    #[test]
    fn flight_polygon_is_counterclockwise() {
        let s = sample_stair();
        let p = plan_stair(2.7, 1.0, None, MAX_RISER, MIN_TREAD).unwrap();
        let poly = flight_polygon(&s, &p);
        // Signed area > 0 means CCW
        let n = poly.len();
        let mut signed = 0.0;
        for i in 0..n {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            signed += a.x * b.y - a.y * b.x;
        }
        assert!(signed > 0.0, "flight polygon should be CCW, signed area = {}", signed);
    }
}
