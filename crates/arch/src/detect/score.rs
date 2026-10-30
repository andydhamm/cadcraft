//! Scoring detection results against truth.

use super::Detection;

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

/// Score a detection against truth.
pub fn score(_det: &Detection, _truth: &Truth, _min_confidence: f64) -> Score {
    Score::default()
}
