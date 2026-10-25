//! Room detection on a building level from the solved poché and divider walls.
//!
//! Walls on a level are fed to [`cadcraft_geom::region::boundary_at`]: non-divider
//! walls contribute their solved poché rings (as closed polylines) and divider
//! walls contribute their 2-point centre lines. Each seed (edge-midpoint
//! left-normal offset, or the vertex-average centroid) traces a closed face; the
//! face is normalised CCW, holes are taken from the boundary's islands, and the
//! area is checked against [`MIN_ROOM_AREA`] (plus a thin-strip guard against
//! counting a wall's own poché). Rooms are deduplicated by rounded
//! (centroid, area) and capped by [`MAX_ROOMS`].
//!
//! All input is treated as hostile: every `Vec2` is guarded with `is_finite`,
//! indexing is via `.get()`, and `boundary_at` returning `None` (unbounded
//! exterior face) is the normal case and is skipped. No `unwrap`/`expect`/
//! `panic!`/`unsafe`/`todo`/`unimplemented` anywhere.

use std::collections::HashSet;

use cadcraft_doc::{ArchModel, Room, RoomType, Wall};
use cadcraft_geom::Vec2;

use crate::poly::{ensure_ccw, region_area};

/// Minimum area (in m²) for a face to qualify as a room.
const MIN_ROOM_AREA: f64 = 0.1;

/// Maximum number of rooms detected per level.
const MAX_ROOMS: u32 = 4096;

/// Detect rooms on every level present in `model.levels`, concatenating per-level
/// results. Each returned room's `level` field is set to that level's index.
///
/// `detect_rooms(&ArchModel::default())` returns an empty `Vec`.
pub fn detect_rooms(model: &ArchModel) -> Vec<Room> {
    let mut rooms: Vec<Room> = Vec::new();
    for lvl in &model.levels {
        rooms.extend(detect_rooms_at_level(model, lvl.index));
    }
    rooms
}

/// Detect rooms on one building level from the solved poché and divider walls.
///
/// `level` is the `Level::index`. Walls on other levels are ignored. Returns an
/// empty `Vec` when the level has no walls or no enclosed area.
pub fn detect_rooms_at_level(model: &ArchModel, level: u32) -> Vec<Room> {
    // Step 1: collect walls on this level.
    let level_has_walls = model.walls.iter().any(|w| w.base_level == level);
    if !level_has_walls {
        return Vec::new();
    }

    // Step 2: build the boundary soup.
    let mut soup: Vec<Vec<Vec2>> = Vec::new();

    // Non-divider walls: solve the poché on a temporary model containing ONLY
    // this level's non-divider walls, then push each resulting ring as a closed
    // polyline.
    let mut temp = ArchModel::default();
    for w in &model.walls {
        if w.base_level == level && !w.divider {
            temp.push_wall((*w).clone());
        }
    }
    for shape in crate::join::solve_joins(&temp) {
        if shape.polygon.is_empty() || shape.polygon.iter().any(|p| !p.is_finite()) {
            continue;
        }
        let mut closed = shape.polygon.clone();
        if !closed.is_empty() && closed.last() != Some(&closed[0]) {
            closed.push(closed[0]); // append the first point to close the edge
        }
        if closed.len() >= 2 {
            soup.push(closed);
        }
    }

    // Divider walls (thickness 0, no poché): push the 2-point centre line as an
    // open polyline. They enclose rooms but produce no poché.
    for w in &model.walls {
        if w.base_level == level && w.divider && w.start.is_finite() && w.end.is_finite() {
            soup.push(vec![w.start, w.end]);
        }
    }

    if soup.is_empty() {
        return Vec::new();
    }

    // Step 3: seed points. For every closed ring, take the left (interior) normal of
    // each edge at its midpoint, plus the vertex-average centroid. Only finite
    // seeds are collected.
    let eps: f64 = 0.5;
    let mut seeds: Vec<Vec2> = Vec::new();
    for ring in &soup {
        if ring.len() < 2 {
            continue;
        }
        let n = ring.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let a = &ring[i];
            let b = &ring[j];
            if !a.is_finite() || !b.is_finite() {
                continue;
            }
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-12 || !len.is_finite() {
                continue;
            }
            let mx = (a.x + b.x) * 0.5;
            let my = (a.y + b.y) * 0.5;
            // Left and right normals of edge a -> b.
            // One of them points toward the room; the other toward the poché
            // or the exterior. With eps = 0.5 (well above half the max wall
            // thickness of ~0.2 m) the in-room seed lands cleanly in the face.
            let nx_l = -dy / len;
            let ny_l = dx / len;
            let nx_r = dy / len;
            let ny_r = -dx / len;
            let sl = Vec2::new(mx + nx_l * eps, my + ny_l * eps);
            let sr = Vec2::new(mx + nx_r * eps, my + ny_r * eps);
            if sl.is_finite() {
                seeds.push(sl);
            }
            if sr.is_finite() {
                seeds.push(sr);
            }
        }
        // Vertex-average centroid.
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut cnt: usize = 0;
        for p in ring {
            if p.is_finite() {
                cx += p.x;
                cy += p.y;
                cnt += 1;
            }
        }
        if cnt > 0 {
            let centroid = Vec2::new(cx / cnt as f64, cy / cnt as f64);
            if centroid.is_finite() {
                seeds.push(centroid);
            }
        }
    }
    if seeds.is_empty() {
        return Vec::new();
    }

    // Steps 4-8: for each seed, trace the face, normalise, compute area, discard
    // thin strips and sub-minimal areas, deduplicate, and cap by MAX_ROOMS.
    let tol: f64 = 1e-9;
    let mut rooms: Vec<Room> = Vec::new();
    let mut seen: HashSet<(i64, i64, i64)> = HashSet::new();

    for seed in &seeds {
        let Some(b) = cadcraft_geom::region::boundary_at(&soup, *seed, tol) else {
            // `None` => seed is in the unbounded exterior face: skip.
            continue;
        };
        if b.outer.len() < 3 {
            continue;
        }

        let outer = ensure_ccw(&b.outer);
        let mut holes: Vec<Vec<Vec2>> = Vec::new();
        for island in &b.islands {
            if island.outline.len() < 3 {
                continue;
            }
            if island.outline.iter().any(|p| !p.is_finite()) {
                continue;
            }
            holes.push(ensure_ccw(&island.outline));
        }

        let area = region_area(&outer, &holes);
        if !area.is_finite() || area <= MIN_ROOM_AREA {
            continue;
        }

        // A wall's own poché is a long, thin strip: its height (max distance from
        // the bounding box of a long edge to the opposite side) is essentially its
        // thickness. Reject faces that are this kind of strip, but keep genuine
        // rooms whose bounding extent is comfortably larger than wall thickness.
        if is_poché_strip(&outer) {
            continue;
        }

        // Dedup key: (outer centroid, area), each rounded to 1e-6.
        let oc = centroid(&outer);
        let key = (round6(oc.x), round6(oc.y), round6(area));
        if !seen.insert(key) {
            continue;
        }

        if rooms.len() as usize >= MAX_ROOMS as usize {
            break;
        }

        // `id` stays unassigned (the caller assigns the real one) and `standard_area` is filled by
        // the area module later.
        rooms.push(Room { room_type: RoomType::Other, level, polygon: outer, holes, interior_area: area, ..Default::default() });
    }

    rooms
}

/// Reject a face that is a long, thin wall poché strip rather than a room.
///
/// A wall poché is roughly `length × thickness`; its bounding-box dimension
/// perpendicular to its long edges is ≈ its thickness. A genuine room has a
/// perpendicular extent far larger than any wall thickness. This guard keeps the
/// 0.1/0.2 m poché strips out without excluding small real rooms (e.g. the
/// 0.9 m × 0.9 m loop interior, whose extent is ~0.9 m, comfortably above the
/// poché threshold).
fn is_poché_strip(pts: &[Vec2]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    // Bounding box of the outer ring.
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in pts {
        if !p.is_finite() {
            return false;
        }
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite() {
        return false;
    }
    let w = max_x - min_x;
    let h = max_y - min_y;
    // The smaller of the two bounding extents. For a wall strip this is ≈ the
    // wall thickness (0.1–0.2 m). For a real room it's several metres.
    let small = w.min(h);
    // Threshold: a face whose narrow dimension is under ~0.5 m is a poché strip.
    // 0.5 m sits between the max poché thickness (~0.2) and the smallest test
    // room (the 0.9 m loop interior, narrow extent ~0.9 m).
    small < 0.5
}

/// Arithmetic mean of the points in `pts`; `Vec2::ZERO` when there are no finite points.
fn centroid(pts: &[Vec2]) -> Vec2 {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cnt: usize = 0;
    for p in pts {
        if p.is_finite() {
            cx += p.x;
            cy += p.y;
            cnt += 1;
        }
    }
    if cnt > 0 { Vec2::new(cx / cnt as f64, cy / cnt as f64) } else { Vec2::ZERO }
}

/// Round `v` to the nearest multiple of 1e-6 and return it as an `i64` key.
///
/// A non-finite or out-of-range input maps to `0`. Used only as a dedup key.
fn round6(v: f64) -> i64 {
    if !v.is_finite() {
        return 0;
    }
    let scaled = v * 1_000_000.0;
    if !scaled.is_finite() {
        return 0;
    }
    let rounded = scaled.round();
    if rounded.is_finite() && rounded <= (i64::MAX as f64) && rounded >= (i64::MIN as f64) { rounded as i64 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{ArchId, JoinKind, Wall};
    use cadcraft_geom::Vec2;

    /// A non-divider wall on `level` with the given thickness.
    #[inline]
    fn wall(id: u32, level: u32, x0: f64, y0: f64, x1: f64, y1: f64, t: f64) -> Wall {
        Wall { id: ArchId(id), start: Vec2::new(x0, y0), end: Vec2::new(x1, y1), base_level: level, thickness: t, ..Default::default() }
    }

    /// A divider wall (thickness 0, `divider: true`) on `level`.
    #[inline]
    fn divider(level: u32, x0: f64, y0: f64, x1: f64, y1: f64) -> Wall {
        Wall {
            id: ArchId(u32::MAX),
            start: Vec2::new(x0, y0),
            end: Vec2::new(x1, y1),
            base_level: level,
            exterior: false,
            wall_type: "Interior".into(),
            join_start: JoinKind::Auto,
            join_end: JoinKind::Auto,
            thickness: 0.0,
            divider: true,
            ..Default::default()
        }
    }

    /// The phase-1 house: a 10×8 exterior (0.2 m) split by two 0.1 m interior walls.
    fn make_phase1_house() -> ArchModel {
        let mut m = ArchModel::default();
        // Exterior centre lines, closed, 0.2 m.
        m.walls.push(wall(0, 0, 0.0, 0.0, 10.0, 0.0, 0.2));
        m.walls.push(wall(1, 0, 10.0, 0.0, 10.0, 8.0, 0.2));
        m.walls.push(wall(2, 0, 10.0, 8.0, 0.0, 8.0, 0.2));
        m.walls.push(wall(3, 0, 0.0, 8.0, 0.0, 0.0, 0.2));
        // Interior walls, 0.1 m.
        m.walls.push(wall(4, 0, 0.0, 3.0, 10.0, 3.0, 0.1));
        m.walls.push(wall(5, 0, 6.0, 3.0, 6.0, 8.0, 0.1));
        m
    }

    #[test]
    fn detects_three_rooms_in_phase1_house() {
        let m = make_phase1_house();
        let rooms = detect_rooms_at_level(&m, 0);
        assert_eq!(rooms.len(), 3);
    }

    #[test]
    fn interior_areas_match_the_brief() {
        let m = make_phase1_house();
        let rooms = detect_rooms_at_level(&m, 0);
        assert_eq!(rooms.len(), 3);
        let mut areas: Vec<f64> = rooms.iter().map(|r| r.interior_area).collect();
        areas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        assert!((areas[0] - 18.6725).abs() < 1e-3, "smallest room: {} != 18.6725", areas[0]);
        assert!((areas[1] - 27.93).abs() < 1e-3, "middle room: {} != 27.93", areas[1]);
        assert!((areas[2] - 28.3725).abs() < 1e-3, "largest room: {} != 28.3725", areas[2]);
    }

    #[test]
    fn closed_loop_makes_a_hole_and_a_room() {
        let mut m = make_phase1_house();
        // Add a 0.1 m closed loop, centre lines x in [2.5, 3.5], y in [1.0, 2.0].
        // Four walls forming the loop's centre lines.
        m.walls.push(wall(6, 0, 2.5, 1.0, 3.5, 1.0, 0.1));
        m.walls.push(wall(7, 0, 3.5, 1.0, 3.5, 2.0, 0.1));
        m.walls.push(wall(8, 0, 3.5, 2.0, 2.5, 2.0, 0.1));
        m.walls.push(wall(9, 0, 2.5, 2.0, 2.5, 1.0, 0.1));
        let rooms = detect_rooms_at_level(&m, 0);
        // The south room now has one hole and the loop's interior is a new room.
        let south = rooms.iter().find(|r| (r.interior_area - 26.72).abs() < 1e-3);
        assert!(south.is_some(), "no room with area ≈ 26.72");
        assert_eq!(south.unwrap().holes.len(), 1, "south room should have exactly one hole");
        // The loop's interior: a new room of area 0.81.
        let inner = rooms.iter().find(|r| (r.interior_area - 0.81).abs() < 1e-3);
        assert!(inner.is_some(), "no room with area ≈ 0.81");
        assert_eq!(inner.unwrap().holes.len(), 0, "loop interior should have no holes");
    }

    #[test]
    fn divider_wall_encloses_rooms() {
        let mut m = ArchModel::default();
        // 5×5 square enclosure, 0.2 m exterior walls (centre lines).
        m.walls.push(wall(0, 0, 0.0, 0.0, 5.0, 0.0, 0.2));
        m.walls.push(wall(1, 0, 5.0, 0.0, 5.0, 5.0, 0.2));
        m.walls.push(wall(2, 0, 5.0, 5.0, 0.0, 5.0, 0.2));
        m.walls.push(wall(3, 0, 0.0, 5.0, 0.0, 0.0, 0.2));
        // One divider splitting the square horizontally at y = 2.5.
        m.walls.push(divider(0, 0.0, 2.5, 5.0, 2.5));
        let rooms = detect_rooms_at_level(&m, 0);
        assert_eq!(rooms.len(), 2, "divider should produce two rooms");
    }

    #[test]
    fn empty_model_yields_no_rooms() {
        let m = ArchModel::default();
        assert!(detect_rooms(&m).is_empty());
    }
}
