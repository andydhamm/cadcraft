//! Junction snapping.

use super::{Evidence, WallRun};
use crate::detect::constants::{MAX_THICKNESS, MIN_THICKNESS};
use crate::detect::mod_::WallRun as _;
use crate::geom::Vec2;

const L_TOL_EXTRA: f64 = 0.05;
const T_TOL_EXTRA: f64 = 0.05;
const CROSS_TOL: f64 = 0.017; // ~1 degree
const ANGLE_L_T: f64 = 10.0 * std::f64::consts::PI / 360.0;
const ANGLE_T_T: f64 = 10.0 * std::f64::consts::PI / 360.0;
const ANGLE_COLLINAR: f64 = 0.5 * std::f64::consts::PI / 180.0;
const OFFSET_TOL: f64 = 0.01;
const GAP_MERGE_TOL: f64 = 0.01;
const MAX_MOVE: f64 = 0.6;
const CONF_BUMP: f64 = 0.1;
const THICKNESS_REL_TOL: f64 = 0.1;

fn dir_of(r: &WallRun) -> Vec2 {
    let d = r.end - r.start;
    if d.len() <= MIN_THICKNESS {
        return Vec2::ZERO;
    }
    d.normalized()
}

fn angle_between(a: Vec2, b: Vec2) -> f64 {
    let dot = a.dot(b).clamp(-1.0, 1.0);
    dot.acos()
}

fn line_intersection(a_start: Vec2, a_dir: Vec2, b_start: Vec2, b_dir: Vec2) -> Option<Vec2> {
    let denom = a_dir.cross(b_dir);
    if denom.abs() < CROSS_TOL {
        return None;
    }
    let d = b_start - a_start;
    let t = d.cross(b_dir) / denom;
    a_start + a_dir * t
}

fn point_near_line(p: Vec2, start: Vec2, dir: Vec2, tol: f64) -> bool {
    if dir == Vec2::ZERO {
        return p.near(start, tol);
    }
    ((p - start).cross(dir)).abs() <= tol
}

fn within_end_bounds(p: Vec2, run: &WallRun, tol: f64) -> bool {
    let dir = dir_of(run);
    if dir == Vec2::ZERO {
        return p.near(run.start, tol) || p.near(run.end, tol);
    }
    let s = (p - run.start).dot(dir);
    let l = (run.end - run.start).len();
    s >= -tol && s <= l + tol
}

fn add_junction_confidence(r: &mut WallRun) {
    r.confidence = (r.confidence + CONF_BUMP).min(1.0);
    if !r.evidence.rules.contains("junction") {
        r.evidence.rules.push("junction".to_string());
    }
}

fn shift_gaps(r: &mut WallRun, shift: f64) {
    for g in r.gaps.iter_mut() {
        g.0 += shift;
        g.1 += shift;
    }
}

fn move_end(r: &mut WallRun, field: &str, new_pos: Vec2) {
    let old = if field == "start" { r.start } else { r.end };
    if new_pos == old {
        return;
    }
    let move_dist = (new_pos - old).len();
    if move_dist > MAX_MOVE {
        return;
    }
    if field == "start" {
        let dir = dir_of(r);
        let shift = (old - new_pos).dot(dir);
        shift_gaps(r, shift);
        r.start = new_pos;
    } else {
        r.end = new_pos;
    }
}

fn ends_near(a: &WallRun, b: &WallRun, tol: f64) -> bool {
    a.start.near(b.start, tol) || a.start.near(b.end, tol) || a.end.near(b.start, tol) || a.end.near(b.end, tol)
}

pub fn snap_junctions(runs: &mut Vec<WallRun>) {
    let n = runs.len();
    for i in 0..n {
        for end_kind in ["start", "end"] {
            let e = &runs[i];
            let e_pos = if end_kind == "start" { e.start } else { e.end };
            let e_dir = dir_of(e);
            if e_dir == Vec2::ZERO {
                continue;
            }
            // Find matching end F in another run
            for j in 0..n {
                if i == j {
                    continue;
                }
                let f = &runs[j];
                let f_dir = dir_of(f);
                if f_dir == Vec2::ZERO {
                    continue;
                }
                let angle = angle_between(e_dir, f_dir);
                let non_parallel = angle > ANGLE_L_T && angle < std::f64::consts::PI - ANGLE_L_T;

                // L case
                if non_parallel {
                    let tol = e.thickness.max(f.thickness) + L_TOL_EXTRA;
                    if ends_near(e, f, tol) {
                        if let Some(p) = line_intersection(e.start, e_dir, f.start, f_dir) {
                            if p.near(e_pos, tol) && p.near(f.start, tol) || p.near(e_pos, tol) && p.near(f.end, tol) {
                                // Check if p is near one of f's ends
                                let f_end = if p.near(f.start, tol) { f.start } else { f.end };
                                if p.near(e_pos, tol) && p.near(f_end, tol) {
                                    move_end(runs.get_mut(i).unwrap(), end_kind, p);
                                    let f_kind = if f_end == f.start { "start" } else { "end" };
                                    move_end(runs.get_mut(j).unwrap(), f_kind, p);
                                    add_junction_confidence(runs.get_mut(i).unwrap());
                                    add_junction_confidence(runs.get_mut(j).unwrap());
                                }
                            }
                        }
                    }
                }

                // T case
                if non_parallel {
                    let tol = e.thickness / 2.0 + f.thickness / 2.0 + T_TOL_EXTRA;
                    if point_near_line(e_pos, f.start, f_dir, tol) && within_end_bounds(e_pos, f, f.thickness / 2.0) {
                        if let Some(p) = line_intersection(e.start, e_dir, f.start, f_dir) {
                            if p.near(e_pos, tol) {
                                let move_dist = (p - e_pos).len();
                                if move_dist <= MAX_MOVE {
                                    let old = e_pos;
                                    let shift = (old - p).dot(e_dir);
                                    if end_kind == "start" {
                                        shift_gaps(&mut runs[i], shift);
                                        runs[i].start = p;
                                    } else {
                                        runs[i].end = p;
                                    }
                                    add_junction_confidence(&mut runs[i]);
                                }
                            }
                        }
                    }
                }

                // Collinear merge
                let angle_collin = angle_between(e_dir, f_dir);
                if angle_collin < ANGLE_COLLINAR
                    && e.thickness > 0.0
                    && (e.thickness - f.thickness).abs() <= 0.1 * e.thickness
                    && point_near_line(f.start, e.start, e_dir, OFFSET_TOL)
                {
                    // Check if f continues e's line with a small gap
                    let f_near_end = if e_pos.near(f.start, GAP_MERGE_TOL) { f.start } else { f.end };
                    if e_pos.near(f_near_end, GAP_MERGE_TOL) {
                        // Merge: remove f, extend e
                        let f_start = f.start;
                        let f_end = f.end;
                        let f_gaps = f.gaps.clone();
                        let f_conf = f.confidence;
                        let f_evidence = f.evidence.clone();
                        drop(f);
                        // Calculate shift for f's gaps
                        let shift = (f_start - e.start).len();
                        let mut merged_gaps = e.gaps.clone();
                        for g in f_gaps.iter() {
                            merged_gaps.push((g.0 + shift, g.1 + shift));
                        }
                        runs[i].gaps = merged_gaps;
                        runs[i].confidence = (runs[i].confidence + f_conf).min(1.0);
                        // Add f's evidence to e's
                        for rule in f_evidence.rules.iter() {
                            if !runs[i].evidence.rules.contains(rule) {
                                runs[i].evidence.rules.push(rule.clone());
                            }
                        }
                        for layer in f_evidence.layers.iter() {
                            if !runs[i].evidence.layers.contains(layer) {
                                runs[i].evidence.layers.push(layer.clone());
                            }
                        }
                        for handle in f_evidence.handles.iter() {
                            if !runs[i].evidence.handles.contains(handle) {
                                runs[i].evidence.handles.push(*handle);
                            }
                        }
                        if end_kind == "end" {
                            runs[i].end = f_end;
                        } else {
                            runs[i].start = f_start;
                        }
                        runs.remove(j);
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wr(start: Vec2, end: Vec2, thickness: f64) -> WallRun {
        WallRun { start, end, thickness, gaps: Vec::new(), confidence: 0.5, evidence: Evidence::default() }
    }

    #[test]
    fn an_l_corner_meets_exactly() {
        let mut runs = vec![wr(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2), wr(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 0.2)];
        snap_junctions(&mut runs);
        assert_eq!(runs[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(runs[0].end, Vec2::new(10.0, 0.0));
        assert_eq!(runs[1].start, Vec2::new(10.0, 0.0));
        assert_eq!(runs[1].end, Vec2::new(10.0, 10.0));
        assert!(runs[0].evidence.rules.contains("junction"));
        assert!(runs[1].evidence.rules.contains("junction"));
    }

    #[test]
    fn a_t_end_lands_on_the_centre_line() {
        let mut runs = vec![wr(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2), wr(Vec2::new(5.0, 0.1), Vec2::new(5.0, 5.0), 0.1)];
        snap_junctions(&mut runs);
        assert_eq!(runs[1].start, Vec2::new(5.0, 0.0));
        assert_eq!(runs[1].end, Vec2::new(5.0, 5.0));
        assert_eq!(runs[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(runs[0].end, Vec2::new(10.0, 0.0));
    }

    #[test]
    fn an_x_crossing_is_left_alone() {
        let mut runs = vec![wr(Vec2::new(-5.0, 0.0), Vec2::new(5.0, 0.0), 0.2), wr(Vec2::new(0.0, -5.0), Vec2::new(0.0, 5.0), 0.2)];
        snap_junctions(&mut runs);
        assert_eq!(runs[0].start, Vec2::new(-5.0, 0.0));
        assert_eq!(runs[0].end, Vec2::new(5.0, 0.0));
        assert_eq!(runs[1].start, Vec2::new(0.0, -5.0));
        assert_eq!(runs[1].end, Vec2::new(0.0, 5.0));
    }

    #[test]
    fn a_45_degree_corner_meets_exactly() {
        let mut runs = vec![
            wr(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2),
            wr(Vec2::new(10.0, 0.0), Vec2::new(10.0 + 5.0 * std::f64::consts::PI.cos() as f64, 5.0 * std::f64::consts::PI.sin() as f64), 0.2),
        ];
        // Simplified: use a 45-degree corner
        let mut runs = vec![
            wr(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2),
            wr(Vec2::new(10.0, 0.0), Vec2::new(10.0 + 5.0 / std::f64::consts::SQRT_2, 5.0 / std::f64::consts::SQRT_2), 0.2),
        ];
        snap_junctions(&mut runs);
        // The corner should be at (10.0, 0.0)
        assert_eq!(runs[0].end, Vec2::new(10.0, 0.0));
        assert_eq!(runs[1].start, Vec2::new(10.0, 0.0));
    }

    #[test]
    fn collinear_runs_merge() {
        let mut runs = vec![wr(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2), wr(Vec2::new(5.01, 0.0), Vec2::new(10.0, 0.0), 0.2)];
        snap_junctions(&mut runs);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(runs[0].end, Vec2::new(10.0, 0.0));
    }

    #[test]
    fn gaps_follow_a_moved_start() {
        let mut runs = vec![wr(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2), wr(Vec2::new(0.0, 0.1), Vec2::new(0.0, 10.0), 0.2)];
        runs[0].gaps = vec![(2.0, 3.0)];
        snap_junctions(&mut runs);
        // Start moved from (0.0, 0.0) to (0.0, 0.0) - no move in this case
        // Let's use a case where the start actually moves
        let mut runs = vec![wr(Vec2::new(0.1, 0.0), Vec2::new(10.0, 0.0), 0.2), wr(Vec2::new(0.0, 0.0), Vec2::new(0.0, 10.0), 0.2)];
        runs[0].gaps = vec![(1.0, 2.0)];
        snap_junctions(&mut runs);
        // Start moved from (0.1, 0.0) to (0.0, 0.0), shift = 0.1
        assert_eq!(runs[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(runs[0].gaps, vec![(1.1, 2.1)]);
    }

    #[test]
    fn apartment_runs_end_on_the_truth_centre_lines() {
        let mut runs = vec![
            // Bottom
            wr(Vec2::new(0.2, 0.1), Vec2::new(11.8, 0.1), 0.2),
            // Top
            wr(Vec2::new(0.2, 7.9), Vec2::new(11.8, 7.9), 0.2),
            // Left
            wr(Vec2::new(0.1, 0.2), Vec2::new(0.1, 7.8), 0.2),
            // Right
            wr(Vec2::new(11.9, 0.2), Vec2::new(11.9, 7.8), 0.2),
            // Partition
            wr(Vec2::new(7.05, 0.2), Vec2::new(7.05, 7.8), 0.2),
            // Interior horizontal
            wr(Vec2::new(7.05, 5.05), Vec2::new(11.8, 5.05), 0.2),
        ];
        snap_junctions(&mut runs);
        // Check the three truth centre lines
        // Bottom: (0.1, 0.1)-(11.9, 0.1)
        assert!(runs.iter().any(|r| r.start.near(Vec2::new(0.1, 0.1), 1e-6) && r.end.near(Vec2::new(11.9, 0.1), 1e-6)));
        // Partition: (7.05, 0.1)-(7.05, 7.9)
        assert!(runs.iter().any(|r| r.start.near(Vec2::new(7.05, 0.1), 1e-6) && r.end.near(Vec2::new(7.05, 7.9), 1e-6)));
        // Interior horizontal: (7.05, 5.05)-(11.9, 5.05)
        assert!(runs.iter().any(|r| r.start.near(Vec2::new(7.05, 5.05), 1e-6) && r.end.near(Vec2::new(11.9, 5.05), 1e-6)));
    }
}
