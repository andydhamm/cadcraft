//! Railing rules: handrail/guard heights, baluster spacing and path helpers (metres).
//!
//! `stair.rs` is built by a sibling segment, so the code-limit constants are mirrored here
//! with identical values until it lands; swap to `crate::stair::*` then.

use cadcraft_geom::Vec2;

/// Lowest stair handrail height (864 mm).
pub const MIN_HANDRAIL: f64 = 0.864;
/// Highest stair handrail height (965 mm).
pub const MAX_HANDRAIL: f64 = 0.965;
/// Lowest guard height (914 mm).
pub const GUARD_HEIGHT: f64 = 0.914;
/// Largest clear gap between balusters (101.6 mm, the 4-inch sphere rule).
pub const MAX_BALUSTER_GAP: f64 = 0.1016;
/// Highest accepted guard height.
const MAX_GUARD: f64 = 1.2;
/// Largest accepted path size.
const MAX_POINTS: usize = 512;
/// Cap on balusters per railing run.
const MAX_BALUSTERS: usize = 100_000;

/// `stair=true` -> handrail clamped into `MIN_HANDRAIL..MAX_HANDRAIL`; `stair=false` -> guard, must be
/// `GUARD_HEIGHT..=1.2`. `None` outside the band (never silently lowered).
pub fn validate_height(h: f64, stair: bool) -> Option<f64> {
    if !h.is_finite() {
        return None;
    }
    if stair {
        Some(h.clamp(MIN_HANDRAIL, MAX_HANDRAIL))
    } else if (GUARD_HEIGHT..=MAX_GUARD).contains(&h) {
        Some(h)
    } else {
        None
    }
}

/// Evenly spaced balusters with clear gap < `MAX_BALUSTER_GAP` -> `(gap, count)`; `None` if degenerate.
pub fn baluster_spacing(run: f64, baluster_width: f64) -> Option<(f64, usize)> {
    if !run.is_finite() || !baluster_width.is_finite() || run <= 0.0 || baluster_width <= 0.0 {
        return None;
    }
    // n balusters leave n+1 gaps: n*w + (n+1)*g = run. Need g < MAX, so n > (run - MAX)/(w + MAX).
    let min_n = ((run - MAX_BALUSTER_GAP) / (baluster_width + MAX_BALUSTER_GAP)).max(0.0);
    if !min_n.is_finite() || min_n >= MAX_BALUSTERS as f64 {
        return None;
    }
    let mut n = min_n.floor() as usize + 1;
    let gap = |n: usize| (run - n as f64 * baluster_width) / (n as f64 + 1.0);
    while gap(n) >= MAX_BALUSTER_GAP && n < MAX_BALUSTERS {
        n += 1;
    }
    let g = gap(n);
    (g > 0.0 && g < MAX_BALUSTER_GAP).then_some((g, n))
}

fn path_ok(path: &[Vec2]) -> bool {
    path.len() <= MAX_POINTS && path.iter().all(|p| p.x.is_finite() && p.y.is_finite())
}

/// Total polyline length; 0 for a hostile (non-finite or over-long) path.
pub fn path_length(path: &[Vec2]) -> f64 {
    if !path_ok(path) {
        return 0.0;
    }
    let l: f64 = path.windows(2).map(|w| (w[1] - w[0]).len()).sum();
    if l.is_finite() { l } else { 0.0 }
}

/// Offset a polyline sideways by `d` (left of travel when positive; railings are centred on their
/// path). Interior vertices move along the mitre; empty for hostile input.
pub fn offset_path(path: &[Vec2], d: f64) -> Vec<Vec2> {
    if !d.is_finite() || !path_ok(path) {
        return Vec::new();
    }
    let normal = |a: Vec2, b: Vec2| {
        let v = b - a;
        let l = v.len();
        if l > 1e-12 { Some(Vec2::new(-v.y / l, v.x / l)) } else { None }
    };
    // Drop repeated points so every segment has a direction.
    let mut pts: Vec<Vec2> = Vec::with_capacity(path.len());
    for p in path {
        if pts.last().is_none_or(|q| (*p - *q).len() > 1e-12) {
            pts.push(*p);
        }
    }
    let mut out = Vec::with_capacity(pts.len());
    for (i, p) in pts.iter().enumerate() {
        let before = i.checked_sub(1).and_then(|j| pts.get(j)).and_then(|q| normal(*q, *p));
        let after = pts.get(i + 1).and_then(|q| normal(*p, *q));
        let n = match (before, after) {
            (Some(a), Some(b)) => {
                let s = a + b;
                let k = 1.0 + a.x * b.x + a.y * b.y; // 2cos²(θ/2)
                if k > 1e-6 { Vec2::new(s.x / k, s.y / k) } else { a }
            }
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => continue,
        };
        out.push(Vec2::new(p.x + n.x * d, p.y + n.y * d));
    }
    if out.iter().all(|p| p.x.is_finite() && p.y.is_finite()) { out } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stair_handrail_band_is_864_to_965() {
        assert_eq!(validate_height(0.5, true), Some(MIN_HANDRAIL));
        assert_eq!(validate_height(2.0, true), Some(MAX_HANDRAIL));
        assert_eq!(validate_height(0.9, true), Some(0.9));
        assert_eq!(validate_height(f64::NAN, true), None);
    }

    #[test]
    fn guard_below_914_is_refused() {
        assert_eq!(validate_height(0.9, false), None);
        assert_eq!(validate_height(0.914, false), Some(0.914));
        assert_eq!(validate_height(1.2, false), Some(1.2));
        assert_eq!(validate_height(1.3, false), None);
    }

    #[test]
    fn baluster_gap_stays_under_101_6_mm() {
        for run in [0.5, 1.0, 2.4384, 3.0, 10.0] {
            let (g, n) = baluster_spacing(run, 0.019).unwrap_or((f64::NAN, 0));
            assert!(g < MAX_BALUSTER_GAP && g > 0.0, "run {run} gap {g}");
            assert!((n as f64 * 0.019 + (n as f64 + 1.0) * g - run).abs() < 1e-9);
        }
        assert!(baluster_spacing(0.0, 0.02).is_none());
        assert!(baluster_spacing(1.0, -1.0).is_none());
        assert!(baluster_spacing(f64::INFINITY, 0.02).is_none());
        assert!(baluster_spacing(1e12, 0.02).is_none());
        assert!(baluster_spacing(0.05, 0.1).is_none());
    }

    #[test]
    fn path_length_and_offset_are_finite() {
        let p = [Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0)];
        assert!((path_length(&p) - 7.0).abs() < 1e-12);
        let o = offset_path(&p, 0.5);
        assert_eq!(o.len(), 3);
        assert!((o[0].y - 0.5).abs() < 1e-12);
        assert!((o[1].x - 3.5).abs() < 1e-12 && (o[1].y - 0.5).abs() < 1e-12);
        assert!(o.iter().all(|q| q.x.is_finite() && q.y.is_finite()));
        let bad = [Vec2::new(f64::NAN, 0.0), Vec2::new(1.0, 0.0)];
        assert_eq!(path_length(&bad), 0.0);
        assert!(offset_path(&bad, 1.0).is_empty());
        assert!(offset_path(&p, f64::NAN).is_empty());
        let long = vec![Vec2::ZERO; 513];
        assert!(offset_path(&long, 1.0).is_empty());
        assert_eq!(path_length(&long), 0.0);
    }

    #[test]
    fn validate_height_exactly_at_boundaries() {
        // Stair handrail: exactly at MIN and MAX should pass through unchanged.
        assert_eq!(validate_height(MIN_HANDRAIL, true), Some(MIN_HANDRAIL));
        assert_eq!(validate_height(MAX_HANDRAIL, true), Some(MAX_HANDRAIL));
        // Guard: exactly at GUARD_HEIGHT and 1.2 should pass.
        assert_eq!(validate_height(GUARD_HEIGHT, false), Some(GUARD_HEIGHT));
        assert_eq!(validate_height(1.2, false), Some(1.2));
        // Just outside the guard band.
        assert_eq!(validate_height(GUARD_HEIGHT - 0.001, false), None);
        assert_eq!(validate_height(1.2 + 0.001, false), None);
    }

    #[test]
    fn baluster_spacing_narrow_run() {
        // Run just barely large enough for one baluster (width 0.019) with gaps < 101.6 mm.
        // n=1: gap = (run - 0.019) / 2 < 0.1016 => run < 0.2222.
        let (g, n) = baluster_spacing(0.1, 0.019).unwrap();
        assert_eq!(n, 1);
        assert!((g - 0.0405).abs() < 1e-9);
        // Run so small that even zero balusters would need a gap >= 101.6 mm.
        // n=0: gap = run. If run >= 0.1016, n=0 works. But the code always uses at least n=1.
        // Actually the code computes min_n and adds 1, so n is always >= 1.
        // For run = 0.05, w = 0.019: gap = (0.05 - 0.019)/2 = 0.0155 < 0.1016. OK.
        let (g, n) = baluster_spacing(0.05, 0.019).unwrap();
        assert_eq!(n, 1);
        assert!((g - 0.0155).abs() < 1e-9);
        // Baluster width nearly the entire run.
        let (g, n) = baluster_spacing(0.021, 0.02).unwrap();
        assert_eq!(n, 1);
        assert!((g - 0.0005).abs() < 1e-9);
    }

    #[test]
    fn path_length_coincident_points() {
        // Coincident points contribute zero length.
        let p = [Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(3.0, 4.0)];
        assert!((path_length(&p) - 7.0).abs() < 1e-12);
        // All points coincident.
        let q = [Vec2::new(1.0, 2.0), Vec2::new(1.0, 2.0), Vec2::new(1.0, 2.0)];
        assert_eq!(path_length(&q), 0.0);
        // Empty path.
        let empty: [Vec2; 0] = [];
        assert_eq!(path_length(&empty), 0.0);
        // Single point.
        let single = [Vec2::new(5.0, 5.0)];
        assert_eq!(path_length(&single), 0.0);
    }

    #[test]
    fn offset_path_collinear() {
        // All points on a horizontal line; offset should shift all by d in y.
        let p = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(3.0, 0.0)];
        let o = offset_path(&p, 0.5);
        assert_eq!(o.len(), 4);
        for (i, q) in o.iter().enumerate() {
            assert!((q.x - p[i].x).abs() < 1e-12);
            assert!((q.y - 0.5).abs() < 1e-12);
        }
        // Zero offset returns the same points.
        let o0 = offset_path(&p, 0.0);
        assert_eq!(o0.len(), 4);
        for (i, q) in o0.iter().enumerate() {
            assert!((q.x - p[i].x).abs() < 1e-12);
            assert!((q.y - p[i].y).abs() < 1e-12);
        }
    }

    #[test]
    fn path_length_exactly_512_points() {
        // 512 points is allowed (the limit); 513 is rejected.
        let p: Vec<Vec2> = (0..512).map(|i| Vec2::new(i as f64, 0.0)).collect();
        // Length = 511 * 1.0 = 511.
        assert!((path_length(&p) - 511.0).abs() < 1e-9);
        let o = offset_path(&p, 1.0);
        assert_eq!(o.len(), 512);
    }
}
