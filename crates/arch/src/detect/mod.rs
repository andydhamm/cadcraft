//! Detection of architectural elements from 2D linework.
//!
//! This module implements the `ARCHDETECT` pipeline (Phase 8). It is a pure algorithmic layer
//! operating on `Drawing` entities, producing candidate walls, openings, and rooms.

use cadcraft_doc::Drawing;
use cadcraft_doc::Handle;
use cadcraft_doc::arch::{Hinge, Swing};
use cadcraft_geom::Vec2;

/// Maximum number of primitives to process.
pub const MAX_PRIMITIVES: usize = 400_000;
/// Maximum number of labels to process.
pub const MAX_LABELS: usize = 20_000;
/// Minimum wall thickness in metres.
pub const MIN_THICKNESS: f64 = 0.04;
/// Maximum wall thickness in metres.
pub const MAX_THICKNESS: f64 = 0.6;
/// Default minimum confidence for accepting candidates.
pub const DEFAULT_MIN_CONFIDENCE: f64 = 0.5;

/// Error type for detection failures.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectError(pub String);

impl std::fmt::Display for DetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DetectError {}

/// Options for detection.
#[derive(Clone, Debug, Default)]
pub struct DetectOptions {
    /// Metres per drawing unit.
    pub scale: f64,
    /// If set, read only these layers (case-insensitive).
    pub layers: Option<Vec<String>>,
}

/// A geometric primitive in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prim {
    /// A line segment.
    Seg { a: Vec2, b: Vec2 },
    /// An arc.
    Arc { c: Vec2, r: f64, start: f64, sweep: f64 },
}

/// An item of linework with metadata.
#[derive(Clone, Debug)]
pub struct Item {
    /// The primitive.
    pub prim: Prim,
    /// The layer name.
    pub layer: String,
    /// The source entity handle.
    pub source: Handle,
    /// Polyline index if part of a polyline.
    pub poly: Option<u32>,
}

impl Item {
    /// An item of linework.
    pub fn new(prim: Prim, layer: impl Into<String>, source: Handle, poly: Option<u32>) -> Self {
        Item { prim, layer: layer.into(), source, poly }
    }
}

/// A text label in the drawing.
#[derive(Clone, Debug)]
pub struct Label {
    /// Position in metres.
    pub at: Vec2,
    /// Text content.
    pub text: String,
    /// Source entity handle.
    pub source: Handle,
}

impl Label {
    /// A text label.
    pub fn new(at: Vec2, text: String, source: Handle) -> Label {
        Label { at, text, source }
    }
}

/// Flattened linework from a drawing.
#[derive(Clone, Debug, Default)]
pub struct Linework {
    /// Geometric items.
    pub items: Vec<Item>,
    /// Text labels.
    pub labels: Vec<Label>,
    /// Number of model-space entities read.
    pub entities: usize,
}

/// Evidence supporting a detection candidate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evidence {
    /// Layers involved.
    pub layers: Vec<String>,
    /// Handles involved.
    pub handles: Vec<Handle>,
    /// Rules applied (kebab-case).
    pub rules: Vec<String>,
}

/// A detected wall run.
#[derive(Clone, Debug, Default)]
pub struct WallRun {
    /// Start point in metres.
    pub start: Vec2,
    /// End point in metres.
    pub end: Vec2,
    /// Thickness in metres.
    pub thickness: f64,
    /// Gaps along the run (start, end) in metres from start.
    pub gaps: Vec<(f64, f64)>,
    /// Confidence score.
    pub confidence: f64,
    /// Evidence.
    pub evidence: Evidence,
}

/// Kind of opening candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CandKind {
    /// Door.
    #[default]
    Door,
    /// Window.
    Window,
    /// Cased opening.
    Cased,
}

/// An opening candidate.
#[derive(Clone, Debug, Default)]
pub struct OpeningCand {
    /// Kind of opening.
    pub kind: CandKind,
    /// Index into the walls vector.
    pub host: usize,
    /// Distance from host start to opening centre in metres.
    pub offset: f64,
    /// Centre point in metres.
    pub center: Vec2,
    /// Finished width in metres.
    pub width: f64,
    /// Style string.
    pub style: String,
    /// Hinge side if applicable.
    pub hinge: Option<Hinge>,
    /// Swing direction if applicable.
    pub swing: Option<Swing>,
    /// Confidence score.
    pub confidence: f64,
    /// Evidence.
    pub evidence: Evidence,
}

/// A room candidate.
#[derive(Clone, Debug, Default)]
pub struct RoomCand {
    /// Polygon in metres.
    pub polygon: Vec<Vec2>,
    /// Name if detected.
    pub name: Option<String>,
    /// Area in square metres.
    pub area: f64,
    /// Label position if detected.
    pub label: Option<Vec2>,
    /// Confidence score.
    pub confidence: f64,
    /// Evidence.
    pub evidence: Evidence,
}

/// The result of detection.
#[derive(Clone, Debug, Default)]
pub struct Detection {
    /// Detected walls.
    pub walls: Vec<WallRun>,
    /// Detected openings.
    pub openings: Vec<OpeningCand>,
    /// Detected rooms.
    pub rooms: Vec<RoomCand>,
    /// Entities read.
    pub entities: usize,
    /// Primitives processed.
    pub primitives: usize,
}

/// Map DXF $INSUNITS code to (name, metres per unit).
pub fn units_scale(insunits: i64) -> Option<(&'static str, f64)> {
    match insunits {
        1 => Some(("in", 0.0254)),
        2 => Some(("ft", 0.3048)),
        4 => Some(("mm", 0.001)),
        5 => Some(("cm", 0.01)),
        6 => Some(("m", 1.0)),
        14 => Some(("dm", 0.1)),
        0 => Some(("unitless", 1.0)),
        _ => None,
    }
}

/// Map a unit name to (name, metres per unit).
pub fn units_named(name: &str) -> Option<(&'static str, f64)> {
    match name.to_lowercase().as_str() {
        "mm" => Some(("mm", 0.001)),
        "cm" => Some(("cm", 0.01)),
        "dm" => Some(("dm", 0.1)),
        "m" => Some(("m", 1.0)),
        "in" => Some(("in", 0.0254)),
        "ft" => Some(("ft", 0.3048)),
        _ => None,
    }
}

/// Run the detection pipeline.
pub fn detect(d: &Drawing, opts: &DetectOptions) -> Result<Detection, DetectError> {
    if !opts.scale.is_finite() || opts.scale <= 0.0 {
        return Err(DetectError("invalid scale".into()));
    }

    let lw = flatten::flatten(d, opts)?;
    let mut walls = walls::wall_runs(&lw);
    junctions::snap_junctions(&mut walls);
    let mut openings = openings::find_openings(&lw, &walls);

    split_unexplained_gaps(&mut walls, &mut openings);

    let mut rooms = rooms::find_rooms(&walls, &lw.labels);

    // Deterministic ordering
    walls.sort_by(|a, b| {
        let ax = a.start.x.min(a.end.x).round() as i64;
        let ay = a.start.y.min(a.end.y).round() as i64;
        let bx = b.start.x.min(b.end.x).round() as i64;
        let by = b.start.y.min(b.end.y).round() as i64;
        ax.cmp(&bx).then(ay.cmp(&by)).then(a.thickness.partial_cmp(&b.thickness).unwrap_or(std::cmp::Ordering::Equal))
    });

    openings.sort_by(|a, b| a.host.cmp(&b.host).then(a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal)));

    rooms.sort_by(|a, b| {
        let av = a
            .polygon
            .iter()
            .min_by(|p, q| {
                p.x.partial_cmp(&q.x).unwrap_or(std::cmp::Ordering::Equal).then(p.y.partial_cmp(&q.y).unwrap_or(std::cmp::Ordering::Equal))
            })
            .copied()
            .unwrap_or(Vec2::ZERO);
        let bv = b
            .polygon
            .iter()
            .min_by(|p, q| {
                p.x.partial_cmp(&q.x).unwrap_or(std::cmp::Ordering::Equal).then(p.y.partial_cmp(&q.y).unwrap_or(std::cmp::Ordering::Equal))
            })
            .copied()
            .unwrap_or(Vec2::ZERO);
        av.x.partial_cmp(&bv.x).unwrap_or(std::cmp::Ordering::Equal).then(av.y.partial_cmp(&bv.y).unwrap_or(std::cmp::Ordering::Equal))
    });

    Ok(Detection { walls, openings, rooms, entities: lw.entities, primitives: lw.items.len() })
}

/// Split wall runs at unexplained gaps and re-index openings.
fn split_unexplained_gaps(walls: &mut Vec<WallRun>, openings: &mut Vec<OpeningCand>) {
    let mut new_walls = Vec::new();
    // Map old wall index to list of (new_wall_index, start_offset, end_offset)
    let mut wall_pieces: Vec<Vec<(usize, f64, f64)>> = vec![Vec::new(); walls.len()];

    for (i, w) in walls.iter().enumerate() {
        let len = w.start.dist(w.end);
        if len <= 0.0 {
            let idx = new_walls.len();
            new_walls.push(w.clone());
            wall_pieces[i].push((idx, 0.0, len));
            continue;
        }

        let dir = (w.end - w.start).normalized();

        // Collect unexplained gaps to remove
        let mut gaps_to_remove: Vec<(f64, f64)> = Vec::new();
        for (g_start, g_end) in &w.gaps {
            if g_end - g_start > 0.3 {
                let mut covered = false;
                for o in openings.iter() {
                    if o.host == i {
                        let rough_half = (o.width + 0.12) / 2.0;
                        let o_start = o.offset - rough_half;
                        let o_end = o.offset + rough_half;
                        // Check overlap
                        if o_start < *g_end && o_end > *g_start {
                            covered = true;
                            break;
                        }
                    }
                }
                if !covered {
                    gaps_to_remove.push((*g_start, *g_end));
                }
            }
        }

        if gaps_to_remove.is_empty() {
            let idx = new_walls.len();
            new_walls.push(w.clone());
            wall_pieces[i].push((idx, 0.0, len));
        } else {
            // Sort gaps by start
            gaps_to_remove.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let mut prev = 0.0;
            for (g_start, g_end) in gaps_to_remove {
                // Create a wall piece from prev to g_start
                if g_start > prev + 1e-6 {
                    let start = w.start + dir * prev;
                    let end = w.start + dir * g_start;
                    let idx = new_walls.len();
                    new_walls.push(WallRun {
                        start,
                        end,
                        thickness: w.thickness,
                        gaps: Vec::new(),
                        confidence: w.confidence,
                        evidence: w.evidence.clone(),
                    });
                    wall_pieces[i].push((idx, prev, g_start));
                }
                // Move prev to the end of the gap
                prev = g_end;
            }

            // Create the final piece from the last gap end to the wall end
            if prev < len - 1e-6 {
                let start = w.start + dir * prev;
                let end = w.end;
                let idx = new_walls.len();
                new_walls.push(WallRun {
                    start,
                    end,
                    thickness: w.thickness,
                    gaps: Vec::new(),
                    confidence: w.confidence,
                    evidence: w.evidence.clone(),
                });
                wall_pieces[i].push((idx, prev, len));
            }
        }
    }

    // Re-index openings
    for o in openings.iter_mut() {
        if o.host < wall_pieces.len() {
            let pieces = &wall_pieces[o.host];
            // Find which piece contains the center
            let mut found = false;
            for &(new_idx, start_off, end_off) in pieces {
                if o.offset >= start_off - 1e-6 && o.offset <= end_off + 1e-6 {
                    o.host = new_idx;
                    found = true;
                    break;
                }
            }
            if !found && !pieces.is_empty() {
                // Fallback to first piece if not found (shouldn't happen with valid data)
                o.host = pieces[0].0;
            }
        }
    }

    *walls = new_walls;
}

pub mod arcs;
pub mod flatten;
pub mod junctions;
pub mod layers;
pub mod openings;
pub mod rooms;
pub mod score;
pub mod walls;

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Drawing;

    #[test]
    fn units_scale_reads_the_dxf_unit_codes() {
        assert_eq!(units_scale(1), Some(("in", 0.0254)));
        assert_eq!(units_scale(2), Some(("ft", 0.3048)));
        assert_eq!(units_scale(4), Some(("mm", 0.001)));
        assert_eq!(units_scale(5), Some(("cm", 0.01)));
        assert_eq!(units_scale(6), Some(("m", 1.0)));
        assert_eq!(units_scale(14), Some(("dm", 0.1)));
        assert_eq!(units_scale(0), Some(("unitless", 1.0)));
        assert_eq!(units_scale(99), None);
    }

    #[test]
    fn detect_on_an_empty_drawing_finds_nothing() {
        let d = Drawing::new_metric();
        let opts = DetectOptions { scale: 1.0, layers: None };
        let res = detect(&d, &opts);
        assert!(res.is_ok());
        let det = res.unwrap();
        assert_eq!(det.walls.len(), 0);
        assert_eq!(det.openings.len(), 0);
        assert_eq!(det.rooms.len(), 0);
    }

    #[test]
    fn a_bad_scale_is_an_error_not_a_panic() {
        let d = Drawing::new_metric();
        let opts = DetectOptions { scale: 0.0, layers: None };
        assert!(detect(&d, &opts).is_err());

        let opts = DetectOptions { scale: f64::NAN, layers: None };
        assert!(detect(&d, &opts).is_err());
    }

    #[test]
    fn an_unexplained_gap_splits_a_wall_run() {
        // Hand-made run with a gap
        let mut walls = vec![WallRun {
            start: Vec2::ZERO,
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            gaps: vec![(4.0, 5.0)], // 1m gap
            confidence: 1.0,
            evidence: Evidence::default(),
        }];
        let mut openings = Vec::new();

        split_unexplained_gaps(&mut walls, &mut openings);

        // Should split into two walls: 0-4 and 5-10
        assert_eq!(walls.len(), 2);
        assert_eq!(walls[0].end.x, 4.0);
        assert_eq!(walls[1].start.x, 5.0);
    }
}
