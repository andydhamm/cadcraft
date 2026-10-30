//! Junction snapping.

use super::{Evidence, MAX_THICKNESS, MIN_THICKNESS, WallRun};
use cadcraft_geom::Vec2;

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
    Some(a_start + a_dir * t)
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
    if !r.evidence.rules.iter().any(|rule| rule == "junction") {
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
            let e_start = runs[i].start;
            let e_end = runs[i].end;
            let e_dir = dir_of(&runs[i]);
            let e_pos = if end_kind == "start" { e_start } else { e_end };
            let e_thickness = runs[i].thickness;
            if e_dir == Vec2::ZERO {
                continue;
            }
            for j in 0..n {
                if i == j {
                    continue;
                }
                let f_start = runs[j].start;
                let f_end = runs[j].end;
                let f_dir = dir_of(&runs[j]);
                let f_thickness = runs[j].thickness;
                if f_dir == Vec2::ZERO {
                    continue;
                }
                let angle = angle_between(e_dir, f_dir);
                let non_parallel = angle > ANGLE_L_T && angle < std::f64::consts::PI - ANGLE_L_T;

                if non_parallel {
                    let mut l_matched = false;

                    // L case: another run's end F is near E; move both to the shared
                    // intersection so the two centre-line ends coincide bit-for-bit.
                    {
                        let tol = e_thickness.max(f_thickness) + L_TOL_EXTRA;
                        let f_end_opt = if e_pos.near(f_start, tol) {
                            Some(f_start)
                        } else if e_pos.near(f_end, tol) {
                            Some(f_end)
                        } else {
                            None
                        };
                        if let Some(f_end_pos) = f_end_opt {
                            if let Some(p) = line_intersection(e_start, e_dir, f_start, f_dir) {
                                if p.near(e_pos, tol) && p.near(f_end_pos, tol) {
                                    let move_dist = (p - e_pos).len();
                                    if move_dist <= MAX_MOVE {
                                        if end_kind == "start" {
                                            let shift = (e_pos - p).dot(e_dir);
                                            shift_gaps(&mut runs[i], shift);
                                            runs[i].start = p;
                                        } else {
                                            runs[i].end = p;
                                        }
                                        let f_kind = if f_end_pos == f_start { "start" } else { "end" };
                                        if f_kind == "start" {
                                            runs[j].start = p;
                                        } else {
                                            runs[j].end = p;
                                        }
                                        add_junction_confidence(&mut runs[i]);
                                        add_junction_confidence(&mut runs[j]);
                                        l_matched = true;
                                    }
                                }
                            }
                        }
                    }

                    // T case: E sits on another run's centre line; slide E to the
                    // intersection so the T end lands exactly on the centre line.
                    if !l_matched {
                        let tol = e_thickness / 2.0 + f_thickness / 2.0 + T_TOL_EXTRA;
                        let s = (e_pos - f_start).dot(f_dir);
                        let l = (f_end - f_start).len();
                        let within_bounds = s >= -f_thickness / 2.0 && s <= l + f_thickness / 2.0;
                        if point_near_line(e_pos, f_start, f_dir, tol) && within_bounds {
                            if let Some(p) = line_intersection(e_start, e_dir, f_start, f_dir) {
                                if p.near(e_pos, tol) {
                                    let move_dist = (p - e_pos).len();
                                    if move_dist > 0.0 && move_dist <= MAX_MOVE {
                                        if end_kind == "start" {
                                            let shift = (e_pos - p).dot(e_dir);
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
                }

                // Collinear merge: f continues e's line with a small gap; splice them
                // into one run (gaps concatenated, evidence unioned).
                let angle_collin = angle_between(e_dir, f_dir);
                let collinear = angle_collin < ANGLE_COLLINAR || angle_collin > std::f64::consts::PI - ANGLE_COLLINAR;
                if collinear
                    && e_thickness > 0.0
                    && (e_thickness - f_thickness).abs() <= THICKNESS_REL_TOL * e_thickness
                    && point_near_line(f_start, e_start, e_dir, OFFSET_TOL)
                {
                    // f must extend e (not overlap it): the near end of f is the one
                    // opposite to the end of e we are processing.
                    let f_near_end = if e_pos.near(f_start, GAP_MERGE_TOL) {
                        Some(f_start)
                    } else if e_pos.near(f_end, GAP_MERGE_TOL) {
                        Some(f_end)
                    } else {
                        None
                    };
                    if let Some(f_near_end) = f_near_end {
                        let extends = if end_kind == "end" { f_near_end == f_start } else { f_near_end == f_end };
                        if extends {
                            let f_gaps = runs[j].gaps.clone();
                            let f_conf = runs[j].confidence;
                            let f_evidence = runs[j].evidence.clone();

                            let shift = (f_start - e_start).len();
                            let mut merged_gaps = runs[i].gaps.clone();
                            for g in f_gaps.iter() {
                                merged_gaps.push((g.0 + shift, g.1 + shift));
                            }
                            runs[i].gaps = merged_gaps;
                            runs[i].confidence = (runs[i].confidence + f_conf).min(1.0);
                            for rule in f_evidence.rules.iter() {
                                if !runs[i].evidence.rules.iter().any(|r| r == rule) {
                                    runs[i].evidence.rules.push(rule.clone());
                                }
                            }
                            for layer in f_evidence.layers.iter() {
                                if !runs[i].evidence.layers.iter().any(|l| l == layer) {
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
