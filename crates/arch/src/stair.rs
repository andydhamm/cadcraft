//! Stair calculator — pure 2D geometry for stair flights and their railings.
//!
//! Implements the IRC-based riser/tread calculation used to lay out a straight
//! flight between two levels. All limits are data (constants), not logic in
//! commands, so that commands/CLI/MCP/`ARCHDETECT` agree (design note 12).
//!
//! The flight is one closed stepped prism: the elevation profile in flight
//! coordinates is extruded across the width — the same trick used for walls.
//! No CSG.

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
    let n_min = (rise / max_riser).ceil() as usize;
    if n_min == 0 {
        return None;
    }

    // Scan a bounded 64-wide window of candidate riser counts.
    let mut best_n: Option<usize> = None;
    let mut best_dist: Option<f64> = None;
    for n in n_min..n_min + 64 {
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
    let clearance = slab_underside.checked_sub(stair_z);
    match clearance {
        Some(c) => stair_z.is_finite() && slab_underside.is_finite() && min_headroom.is_finite() && c >= min_headroom,
        None => false,
    }
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
}
