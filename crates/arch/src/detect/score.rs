//! Scoring detection results against truth.

use super::{CandKind, Detection, OpeningCand, WallRun};
use cadcraft_geom::Vec2;

/// Truth data for scoring.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Truth {
    #[serde(default)]
    pub walls: Vec<TruthWall>,
    #[serde(default)]
    pub openings: Vec<TruthOpening>,
}

/// A truth wall.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct TruthWall {
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub thickness: f64,
}

/// A truth opening.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct TruthOpening {
    pub kind: String,
    pub center: [f64; 2],
    pub width: f64,
}

/// Precision and Recall metrics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pr {
    pub matched: usize,
    pub candidates: usize,
    pub truths: usize,
}

impl Pr {
    /// Precision: matched / candidates.
    pub fn precision(&self) -> f64 {
        if self.candidates == 0 && self.truths == 0 {
            1.0
        } else if self.candidates == 0 {
            0.0
        } else {
            self.matched as f64 / self.candidates as f64
        }
    }

    /// Recall: matched / truths.
    pub fn recall(&self) -> f64 {
        if self.truths == 0 { 1.0 } else { self.matched as f64 / self.truths as f64 }
    }
}

/// Score for a detection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Score {
    pub walls: Pr,
    pub doors: Pr,
    pub windows: Pr,
}

/// Calculate the Hausdorff distance between two line segments.
/// Returns the maximum of the four endpoint-to-segment distances.
fn hausdorff_segment(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> f64 {
    let d1 = dist_point_segment(a0, b0, b1);
    let d2 = dist_point_segment(a1, b0, b1);
    let d3 = dist_point_segment(b0, a0, a1);
    let d4 = dist_point_segment(b1, a0, a1);
    d1.max(d2).max(d3).max(d4)
}

/// Distance from point p to segment ab.
fn dist_point_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len_sq = ab.dot(ab);
    if len_sq < 1e-12 {
        return (p - a).len();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).len()
}

/// Score a detection against truth.
pub fn score(det: &Detection, truth: &Truth, min_confidence: f64) -> Score {
    let mut score = Score::default();

    // Walls
    let wall_cands: Vec<(usize, &WallRun)> = det.walls.iter().enumerate().filter(|(_, w)| w.confidence >= min_confidence).collect();
    score.walls.candidates = wall_cands.len();
    score.walls.truths = truth.walls.len();

    let mut wall_matches: Vec<(f64, usize, usize)> = Vec::new();
    for (ci, cand) in &wall_cands {
        for (ti, tw) in truth.walls.iter().enumerate() {
            let tw_start = Vec2::new(tw.start[0], tw.start[1]);
            let tw_end = Vec2::new(tw.end[0], tw.end[1]);
            let dist = hausdorff_segment(cand.start, cand.end, tw_start, tw_end);
            let tol = (tw.thickness / 2.0).max(0.05);
            let thick_diff = (cand.thickness - tw.thickness).abs();
            if dist <= tol && thick_diff <= 0.2 * tw.thickness {
                wall_matches.push((dist, *ci, ti));
            }
        }
    }
    wall_matches.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let mut used_cands = vec![false; wall_cands.len()];
    let mut used_truths = vec![false; truth.walls.len()];
    let mut matched = 0;
    for (_, ci, ti) in wall_matches {
        if !used_cands[ci] && !used_truths[ti] {
            used_cands[ci] = true;
            used_truths[ti] = true;
            matched += 1;
        }
    }
    score.walls.matched = matched;

    // Openings
    let door_cands: Vec<(usize, &OpeningCand)> =
        det.openings.iter().enumerate().filter(|(_, o)| o.confidence >= min_confidence && o.kind == CandKind::Door).collect();
    let win_cands: Vec<(usize, &OpeningCand)> =
        det.openings.iter().enumerate().filter(|(_, o)| o.confidence >= min_confidence && o.kind == CandKind::Window).collect();

    score.doors.candidates = door_cands.len();
    score.windows.candidates = win_cands.len();

    let truth_doors: Vec<(usize, &TruthOpening)> = truth.openings.iter().enumerate().filter(|(_, t)| t.kind.eq_ignore_ascii_case("door")).collect();
    let truth_windows: Vec<(usize, &TruthOpening)> =
        truth.openings.iter().enumerate().filter(|(_, t)| t.kind.eq_ignore_ascii_case("window")).collect();

    score.doors.truths = truth_doors.len();
    score.windows.truths = truth_windows.len();

    score.doors.matched = match_openings(&door_cands, &truth_doors);
    score.windows.matched = match_openings(&win_cands, &truth_windows);

    score
}

fn match_openings(cands: &[(usize, &OpeningCand)], truths: &[(usize, &TruthOpening)]) -> usize {
    let mut matches: Vec<(f64, usize, usize)> = Vec::new();
    for (ci, cand) in cands {
        for (ti, truth) in truths {
            let tc = Vec2::new(truth.center[0], truth.center[1]);
            let dist = (cand.center - tc).len();
            let width_diff = (cand.width - truth.width).abs();
            if dist <= 0.15 && width_diff <= 0.1 * truth.width {
                matches.push((dist, *ci, *ti));
            }
        }
    }
    matches.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let mut used_cands = vec![false; cands.len()];
    let mut used_truths = vec![false; truths.len()];
    let mut matched = 0;
    for (_, ci, ti) in matches {
        if !used_cands[ci] && !used_truths[ti] {
            used_cands[ci] = true;
            used_truths[ti] = true;
            matched += 1;
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::super::{CandKind, Detection, OpeningCand, WallRun};
    use super::*;
    use cadcraft_geom::Vec2;

    #[test]
    fn the_apartment_truth_parses() {
        let json = include_str!("../../../../tests/fixtures/archdetect/apartment.truth.json");
        let truth: Truth = serde_json::from_str(json).expect("parse truth");
        assert_eq!(truth.walls.len(), 6);
        assert_eq!(truth.openings.len(), 7);
    }

    #[test]
    fn a_perfect_detection_scores_one() {
        let json = include_str!("../../../../tests/fixtures/archdetect/apartment.truth.json");
        let truth: Truth = serde_json::from_str(json).expect("parse truth");

        let mut det = Detection::default();
        for tw in &truth.walls {
            det.walls.push(WallRun {
                start: Vec2::new(tw.start[0], tw.start[1]),
                end: Vec2::new(tw.end[0], tw.end[1]),
                thickness: tw.thickness,
                ..Default::default()
            });
        }
        for to in &truth.openings {
            let kind = if to.kind.eq_ignore_ascii_case("door") { CandKind::Door } else { CandKind::Window };
            det.openings.push(OpeningCand {
                kind,
                center: Vec2::new(to.center[0], to.center[1]),
                width: to.width,
                confidence: 1.0,
                ..Default::default()
            });
        }

        let s = score(&det, &truth, 0.5);
        assert_eq!(s.walls.precision(), 1.0);
        assert_eq!(s.walls.recall(), 1.0);
        assert_eq!(s.doors.precision(), 1.0);
        assert_eq!(s.doors.recall(), 1.0);
        assert_eq!(s.windows.precision(), 1.0);
        assert_eq!(s.windows.recall(), 1.0);
    }

    #[test]
    fn matching_is_one_to_one() {
        let truth = Truth { walls: vec![TruthWall { start: [0.0, 0.0], end: [1.0, 0.0], thickness: 0.2 }], openings: vec![] };
        let mut det = Detection::default();
        // Two candidates for one truth
        det.walls.push(WallRun { start: Vec2::new(0.0, 0.0), end: Vec2::new(1.0, 0.0), thickness: 0.2, ..Default::default() });
        det.walls.push(WallRun { start: Vec2::new(0.0, 0.0), end: Vec2::new(1.0, 0.0), thickness: 0.2, ..Default::default() });

        let s = score(&det, &truth, 0.5);
        assert_eq!(s.walls.matched, 1);
        assert_eq!(s.walls.candidates, 2);
        assert_eq!(s.walls.truths, 1);
    }

    #[test]
    fn a_wall_matches_within_half_its_thickness() {
        // Truth thickness 0.2 -> tol 0.1
        let truth = Truth { walls: vec![TruthWall { start: [0.0, 0.0], end: [1.0, 0.0], thickness: 0.2 }], openings: vec![] };
        let mut det = Detection::default();
        // Offset 0.06 (within 0.1)
        det.walls.push(WallRun { start: Vec2::new(0.0, 0.06), end: Vec2::new(1.0, 0.06), thickness: 0.2, ..Default::default() });
        let s = score(&det, &truth, 0.5);
        assert_eq!(s.walls.matched, 1);

        // Offset 0.11 (outside 0.1)
        det.walls.clear();
        det.walls.push(WallRun { start: Vec2::new(0.0, 0.11), end: Vec2::new(1.0, 0.11), thickness: 0.2, ..Default::default() });
        let s = score(&det, &truth, 0.5);
        assert_eq!(s.walls.matched, 0);
    }

    #[test]
    fn an_opening_needs_its_kind_centre_and_width() {
        let truth = Truth { walls: vec![], openings: vec![TruthOpening { kind: "door".into(), center: [0.0, 0.0], width: 0.9 }] };
        let mut det = Detection::default();
        // Wrong kind (window)
        det.openings.push(OpeningCand { kind: CandKind::Window, center: Vec2::new(0.0, 0.0), width: 0.9, confidence: 1.0, ..Default::default() });
        let s = score(&det, &truth, 0.5);
        assert_eq!(s.doors.matched, 0);

        // Right kind, wrong center
        det.openings.clear();
        det.openings.push(OpeningCand { kind: CandKind::Door, center: Vec2::new(0.2, 0.0), width: 0.9, confidence: 1.0, ..Default::default() });
        let s = score(&det, &truth, 0.5);
        assert_eq!(s.doors.matched, 0);

        // Right kind, right center, wrong width
        det.openings.clear();
        det.openings.push(OpeningCand {
            kind: CandKind::Door,
            center: Vec2::new(0.0, 0.0),
            width: 1.0, // 0.1 diff, tol 0.09
            confidence: 1.0,
            ..Default::default()
        });
        let s = score(&det, &truth, 0.5);
        assert_eq!(s.doors.matched, 0);

        // Correct
        det.openings.clear();
        det.openings.push(OpeningCand { kind: CandKind::Door, center: Vec2::new(0.0, 0.0), width: 0.9, confidence: 1.0, ..Default::default() });
        let s = score(&det, &truth, 0.5);
        assert_eq!(s.doors.matched, 1);
    }
}
