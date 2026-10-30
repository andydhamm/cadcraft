//! Phase 8 corpus tests for `archdetect`.
//!
//! These tests measure the detector end to end (the same measurement the acceptance
//! checks run through the CLI) inside the workspace, so any later merge surfaces a drop
//! immediately. They build the plans from arch model entities, export them to DXF, strip
//! the `CADCRAFT` data exactly as check 03 does, read the plain linework back, and run
//! `arch::detect::detect` + `score::score`. Precision/recall are asserted against a
//! hand-derived truth (and reported independently) so the measurement is stable.

use std::fs;
use std::path::Path;

use cadcraft_arch::detect::score::score as arch_score;
use cadcraft_arch::detect::{
    CandKind, DEFAULT_MIN_CONFIDENCE, Detect, DetectOptions, Detection, Item, Linework, OpeningCand, RoomCand, Truth, Vec2, WallRun,
};
use cadcraft_doc::Drawing;
use cadcraft_engine::Session;
use cadcraft_geom::Vec2;
use serde_json::json;

const DEFAULT: &DetectOptions = DetectOptions { scale: 1.0, layers: None };

/// A 17×17 tiling of the apartment's walls, doors and windows.
///
/// The apartment is the proven base corpus: 6 walls, 2 doors and 5 windows. Tiling it
/// produces well over 10,000 entities, which the timing and per-tile assertions rely on.
fn apartment_tile(offset: (f64, f64), layer: &str, s: &mut Session) {
    let (dx, dy) = offset;
    let h = 2.4;
    // Outer rectangle, 12 × 8.
    let outer = [((0.0, 0.0), (12.0, 0.0)), ((12.0, 0.0), (12.0, 8.0)), ((12.0, 8.0), (0.0, 8.0)), ((0.0, 8.0), (0.0, 0.0))];
    for (a, b) in outer {
        issue_wall(s, (a.0 + dx, a.1 + dy), (b.0 + dx, b.1 + dy), 0.2, h);
    }
    // Vertical partition at x ≈ 7.05, from y=0.1 to y=5.05.
    issue_wall(s, (7.05 + dx, 0.1 + dy), (7.05 + dx, 5.05 + dy), 0.1, h);
    // Interior horizontal at y ≈ 5.05, x=7.05..11.9.
    issue_wall(s, (7.05 + dx, 5.05 + dy), (11.9 + dx, 5.05 + dy), 0.1, h);
    // Door on the partition (swing arc).
    issue_door(s, (7.05 + dx, 1.95 + dy), 0.9);
    // Door on the interior horizontal (sliding).
    issue_door(s, (9.4 + dx, 5.05 + dy), 0.8);
    // Five windows: three on the south wall, one on the west, one on the east.
    issue_window(s, (2.5 + dx, 0.1 + dy), 2.0);
    issue_window(s, (5.2 + dx, 0.1 + dy), 2.0);
    issue_window(s, (9.6 + dx, 0.1 + dy), 2.0);
    issue_window(s, (0.1 + dx, 5.3 + dy), 2.0);
    issue_window(s, (11.9 + dx, 6.5 + dy), 1.4);
    // Room labels (MTEXT) so the detector names the rooms.
    issue_room(s, [(7.0 + dx, 2.0 + dy), (7.0 + dx, 5.0 + dy)], "LIVING");
    issue_room(s, [(9.5 + dx, 2.0 + dy), (9.5 + dx, 5.0 + dy)], "DINING");
    issue_room(s, [(2.0 + dx, 3.0 + dy), (2.0 + dx, 3.0 + dy)], "BEDROOM");
    issue_room(s, [(10.0 + dx, 6.5 + dy), (10.0 + dx, 6.5 + dy)], "BATH");
}
