//! Arc and polyline primitives.

use super::Prim;
use cadcraft_geom::Vec2;

const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;
const ARC_TOL: f64 = 1e-12;

fn finite_points(points: &[Vec2]) -> Vec<Vec2> {
    let mut filtered = Vec::with_capacity(points.len());
    for p in points {
        if p.is_finite() {
            filtered.push(*p);
        }
    }
    filtered
}

fn add_segment(out: &mut Vec<Prim>, a: Vec2, b: Vec2) {
    let delta = b - a;
    if delta.len() > ARC_TOL {
        out.push(Prim::Seg { a, b });
    }
}

fn straight_end(points: &[Vec2], start: usize) -> usize {
    let len = points.len();
    if start + 1 >= len {
        return start;
    }
    let a = match points.get(start).copied() {
        Some(v) => v,
        None => return start,
    };
    let b = match points.get(start + 1).copied() {
        Some(v) => v,
        None => return start,
    };
    let dir = b - a;
    if dir.len() <= ARC_TOL {
        return start;
    }

    let mut end = start + 1;
    while end + 1 < len {
        let current = match points.get(end).copied() {
            Some(v) => v,
            None => return end,
        };
        let next = match points.get(end + 1).copied() {
            Some(v) => v,
            None => return end,
        };
        let step = next - current;
        if step.len() <= ARC_TOL {
            end += 1;
            continue;
        }
        let cross = dir.cross(step);
        if cross.abs() > 1e-8 {
            break;
        }
        if step.dot(dir) < 0.0 {
            break;
        }
        end += 1;
    }
    end
}

fn circle_for_run(points: &[Vec2]) -> Option<(Vec2, f64, f64, f64)> {
    if points.len() < 5 {
        return None;
    }

    let a = *points.first()?;
    let mid_idx = points.len() / 2;
    let b = *points.get(mid_idx)?;
    let c = *points.last()?;

    let ab = b - a;
    let ac = c - a;
    let det = ab.cross(ac);
    if !det.is_finite() || det.abs() <= ARC_TOL {
        return None;
    }

    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if !d.is_finite() || d.abs() <= ARC_TOL {
        return None;
    }

    let ux = (a.x * a.x + a.y * a.y) * (b.y - c.y) + (b.x * b.x + b.y * b.y) * (c.y - a.y) + (c.x * c.x + c.y * c.y) * (a.y - b.y);
    let uy = (a.x * a.x + a.y * a.y) * (c.x - b.x) + (b.x * b.x + b.y * b.y) * (a.x - c.x) + (c.x * c.x + c.y * c.y) * (b.x - a.x);

    let center = Vec2::new(ux / d, uy / d);
    if !center.is_finite() {
        return None;
    }

    let radius = a.dist(center);
    if !radius.is_finite() || !(0.05..=50.0).contains(&radius) {
        return None;
    }

    let tol = 0.005_f64.max(0.01 * radius);
    let mut prev_len: Option<f64> = None;
    let mut orientation: Option<f64> = None;

    for idx in 1..points.len() {
        let p = match points.get(idx).copied() {
            Some(v) => v,
            None => return None,
        };
        if !p.is_finite() {
            return None;
        }
        let dist = p.dist(center);
        if !dist.is_finite() || (dist - radius).abs() > tol {
            return None;
        }

        if idx >= 2 {
            let left = match points.get(idx - 2).copied() {
                Some(v) => v,
                None => return None,
            };
            let mid = match points.get(idx - 1).copied() {
                Some(v) => v,
                None => return None,
            };
            let cross = (mid - left).cross(p - mid);
            if !cross.is_finite() {
                return None;
            }
            if cross.abs() > ARC_TOL {
                let turn = if cross > 0.0 { 1.0 } else { -1.0 };
                match orientation {
                    None => orientation = Some(turn),
                    Some(existing) if (existing - turn).abs() > ARC_TOL => return None,
                    _ => {}
                }
            }
        }

        let prev = match points.get(idx - 1).copied() {
            Some(v) => v,
            None => return None,
        };
        let chord = (p - prev).len();
        if !chord.is_finite() || chord <= ARC_TOL {
            return None;
        }
        if let Some(last_len) = prev_len {
            let ratio = if last_len > chord { chord / last_len } else { last_len / chord };
            if ratio < 0.8 {
                return None;
            }
        }
        prev_len = Some(chord);
    }

    let start = (a - center).angle();
    let end = (c - center).angle();
    let mut sweep = end - start;
    if sweep > PI {
        sweep -= TAU;
    } else if sweep <= -PI {
        sweep += TAU;
    }

    let dir = orientation?;
    // Adjust sweep to match the turning direction (handles arcs > 180°)
    if dir > 0.0 && sweep <= 0.0 {
        sweep += TAU;
    } else if dir < 0.0 && sweep >= 0.0 {
        sweep -= TAU;
    }

    if !sweep.is_finite() || sweep.abs() <= ARC_TOL || sweep.abs() > TAU + ARC_TOL {
        return None;
    }

    Some((center, radius, start, sweep))
}

/// Generate primitives for a polyline.
pub fn polyline_prims(points: &[Vec2], closed: bool) -> Vec<Prim> {
    let finite = finite_points(points);
    if finite.len() < 2 {
        return Vec::new();
    }
    if points.len() > 100_000 {
        let mut out = Vec::new();
        for idx in 0..finite.len().saturating_sub(1) {
            let a = match finite.get(idx).copied() {
                Some(v) => v,
                None => continue,
            };
            let b = match finite.get(idx + 1).copied() {
                Some(v) => v,
                None => continue,
            };
            add_segment(&mut out, a, b);
        }
        if closed {
            // Skip the closing segment if the last primitive is a full-circle arc (sweep ≈ 2π).
            let is_full_circle = matches!(out.last(), Some(Prim::Arc { sweep, .. }) if (sweep - TAU).abs() < 1e-6);
            if !is_full_circle {
                if let (Some(first), Some(last)) = (finite.first().copied(), finite.last().copied()) {
                    add_segment(&mut out, last, first);
                }
            }
        }
        return out;
    }

    let mut out = Vec::new();
    let mut i = 0usize;
    while i < finite.len() {
        let remaining = finite.len().saturating_sub(i);
        if remaining < 2 {
            break;
        }

        let mut best_end = None;
        let mut best_arc = None;
        let mut j = i.checked_add(4).unwrap_or(finite.len());
        while j < finite.len() {
            let mut run = Vec::with_capacity(j.saturating_sub(i).saturating_add(1));
            let mut k = i;
            while k <= j {
                let p = *finite.get(k).unwrap_or(&Vec2::ZERO);
                run.push(p);
                k = k.checked_add(1).unwrap_or(finite.len());
            }
            if let Some(arc) = circle_for_run(&run) {
                best_end = Some(j);
                best_arc = Some(arc);
            } else if best_end.is_some() {
                break;
            }
            j = j.checked_add(1).unwrap_or(finite.len());
        }

        if let Some(end) = best_end {
            if let Some((center, radius, start, sweep)) = best_arc {
                out.push(Prim::Arc { c: center, r: radius, start, sweep });
            }
            i = end;
        } else {
            let seg_end = straight_end(&finite, i);
            let a = *finite.get(i).unwrap_or(&Vec2::ZERO);
            if seg_end > i {
                let b = *finite.get(seg_end).unwrap_or(&Vec2::ZERO);
                add_segment(&mut out, a, b);
                i = seg_end;
            } else {
                let next_idx = i.checked_add(1).unwrap_or(finite.len());
                if next_idx < finite.len() {
                    let b = *finite.get(next_idx).unwrap_or(&Vec2::ZERO);
                    add_segment(&mut out, a, b);
                }
                i = next_idx;
            }
        }
    }

    if closed {
        // Skip the closing segment if the last primitive is a full-circle arc (sweep ≈ 2π).
        let is_full_circle = matches!(out.last(), Some(Prim::Arc { sweep, .. }) if (sweep - TAU).abs() < 1e-6);
        if !is_full_circle {
            if let (Some(first), Some(last)) = (finite.first().copied(), finite.last().copied()) {
                add_segment(&mut out, last, first);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tessellated_quarter_circle_is_one_arc() {
        let radius = 0.9;
        let hinge = Vec2::ZERO;
        let dir = Vec2::X;
        let perp = Vec2::Y;
        let mut pts = Vec::with_capacity(17);
        for i in 0..=16usize {
            let t = i as f64 / 16.0;
            let ang = t * (PI / 2.0);
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 1, "quarter-circle polyline should collapse to one arc");
        match prims[0] {
            Prim::Arc { c, r, start, sweep } => {
                assert!((c - hinge).len() < 1e-9, "arc centre should be at the hinge");
                assert!((r - radius).abs() < 1e-9, "arc radius should be 0.9");
                assert!((start.abs()).abs() < 1e-9, "arc should start at angle 0");
                assert!((sweep.abs() - (PI / 2.0)).abs() <= 1e-9, "arc sweep should be pi/2, got {}", sweep);
            }
            Prim::Seg { .. } => panic!("quarter circle should be an arc"),
        }
    }

    #[test]
    fn a_rectangle_stays_four_segments() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0), Vec2::new(0.0, 0.0)];
        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 4);
        assert!(prims.iter().all(|p| matches!(p, Prim::Seg { .. })));
    }

    #[test]
    fn a_leaf_line_then_a_swing_gives_a_segment_and_an_arc() {
        let mut pts = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)];
        let radius = 0.9;
        let hinge = Vec2::new(2.0, 0.0);
        let dir = Vec2::X;
        let perp = Vec2::Y;
        for i in 0..=16usize {
            let t = i as f64 / 16.0;
            let ang = t * (PI / 2.0);
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 2);
        assert!(matches!(prims[0], Prim::Seg { .. }));
        assert!(matches!(prims[1], Prim::Arc { .. }));
    }

    #[test]
    fn a_clockwise_arc_has_negative_sweep() {
        let radius = 0.9;
        let hinge = Vec2::ZERO;
        let dir = Vec2::X;
        let perp = -Vec2::Y;
        let mut pts = Vec::with_capacity(17);
        for i in 0..=16usize {
            let t = i as f64 / 16.0;
            let ang = t * (PI / 2.0);
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 1, "CW quarter-circle should be one arc");
        match prims[0] {
            Prim::Arc { c, r, sweep, .. } => {
                assert!((c - hinge).len() < 1e-9);
                assert!((r - radius).abs() < 1e-9);
                assert!((sweep.abs() - (PI / 2.0)).abs() <= 1e-9, "sweep should be -pi/2, got {}", sweep);
                assert!(sweep < 0.0, "CW arc should have negative sweep, got {}", sweep);
            }
            Prim::Seg { .. } => panic!("should be an arc"),
        }
    }

    #[test]
    fn a_three_quarter_circle_is_one_arc() {
        let radius = 1.0;
        let hinge = Vec2::ZERO;
        let dir = Vec2::X;
        let perp = Vec2::Y;
        let mut pts = Vec::with_capacity(33);
        for i in 0..=32usize {
            let t = i as f64 / 32.0;
            let ang = t * (3.0 * PI / 2.0);
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 1, "270° arc should be one arc");
        match prims[0] {
            Prim::Arc { c, r, sweep, .. } => {
                assert!((c - hinge).len() < 1e-9);
                assert!((r - radius).abs() < 1e-9);
                assert!((sweep - (3.0 * PI / 2.0)).abs() <= 1e-9, "sweep should be 3pi/2, got {}", sweep);
                assert!(sweep > 0.0, "CCW arc should have positive sweep");
            }
            Prim::Seg { .. } => panic!("should be an arc"),
        }
    }

    #[test]
    fn a_small_radius_is_not_an_arc() {
        let radius = 0.04;
        let hinge = Vec2::ZERO;
        let dir = Vec2::X;
        let perp = Vec2::Y;
        let mut pts = Vec::with_capacity(17);
        for i in 0..=16usize {
            let t = i as f64 / 16.0;
            let ang = t * (PI / 2.0);
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 16, "r=0.04 is below the 0.05 minimum");
        assert!(prims.iter().all(|p| matches!(p, Prim::Seg { .. })));
    }

    #[test]
    fn a_large_radius_is_not_an_arc() {
        let radius = 51.0;
        let hinge = Vec2::ZERO;
        let dir = Vec2::X;
        let perp = Vec2::Y;
        let mut pts = Vec::with_capacity(17);
        for i in 0..=16usize {
            let t = i as f64 / 16.0;
            let ang = t * (PI / 2.0);
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 16, "r=51 is above the 50 maximum");
        assert!(prims.iter().all(|p| matches!(p, Prim::Seg { .. })));
    }

    #[test]
    fn collinear_points_are_one_segment() {
        let mut pts = Vec::with_capacity(10);
        for i in 0..10usize {
            pts.push(Vec2::new(i as f64 * 0.1, 0.0));
        }

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 1, "10 collinear points should be one segment");
        match prims[0] {
            Prim::Seg { a, b } => {
                assert!((a - Vec2::ZERO).len() < 1e-9);
                assert!((b - Vec2::new(0.9, 0.0)).len() < 1e-9);
            }
            Prim::Arc { .. } => panic!("collinear points should be a segment"),
        }
    }

    #[test]
    fn coincident_points_are_skipped() {
        let pts = vec![Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)];

        let prims = polyline_prims(&pts, false);
        assert_eq!(prims.len(), 2, "duplicates should be skipped");
        assert!(prims.iter().all(|p| matches!(p, Prim::Seg { .. })));
    }

    #[test]
    fn a_closed_circle_is_one_arc() {
        let radius = 1.0;
        let hinge = Vec2::new(2.0, 2.0);
        let dir = Vec2::X;
        let perp = Vec2::Y;
        let mut pts = Vec::with_capacity(33);
        for i in 0..=32usize {
            let t = i as f64 / 32.0;
            let ang = t * TAU;
            let p = hinge + dir * (radius * ang.cos()) + perp * (radius * ang.sin());
            pts.push(p);
        }

        let prims = polyline_prims(&pts, true);
        assert_eq!(prims.len(), 1, "closed circle should be one arc");
        match prims[0] {
            Prim::Arc { c, r, sweep, .. } => {
                assert!((c - hinge).len() < 1e-9);
                assert!((r - radius).abs() < 1e-9);
                assert!((sweep - TAU).abs() <= 1e-9, "sweep should be 2pi, got {}", sweep);
                assert!(sweep > 0.0, "CCW circle should have positive sweep");
            }
            Prim::Seg { .. } => panic!("closed circle should be an arc"),
        }
    }

    #[test]
    fn random_polylines_never_panic() {
        let mut state = 0x1234_5678_9abc_def0u64;
        for _ in 0..200usize {
            let mut pts = Vec::new();
            for _ in 0..32usize {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let x = ((state >> 11) as f64) / 10_000_000.0;
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let y = ((state >> 11) as f64) / 10_000_000.0;
                let px = x.rem_euclid(10.0) - 5.0;
                let py = y.rem_euclid(10.0) - 5.0;
                if (state & 3) == 0 {
                    pts.push(Vec2::new(f64::INFINITY, py));
                } else if (state & 3) == 1 {
                    pts.push(Vec2::new(px, f64::NAN));
                } else if (state & 3) == 2 {
                    pts.push(Vec2::new(f64::NEG_INFINITY, -py));
                } else {
                    pts.push(Vec2::new(px, py));
                }
            }
            let _ = polyline_prims(&pts, false);
            let _ = polyline_prims(&pts, true);
        }

        let mut huge = Vec::with_capacity(100_001);
        for i in 0..100_001usize {
            let x = (i as f64) * 0.01;
            huge.push(Vec2::new(x, 0.0));
        }
        let _ = polyline_prims(&huge, false);
    }
}
