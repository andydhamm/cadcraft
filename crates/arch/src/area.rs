//! Chief's area rules for architectural rooms.
//!
//! The model data (`Room`, `RoomType`, `Level`, `Wall`, `ArchModel`) lives in
//! `cadcraft-doc` (L1) and is stored on `Drawing`; this module holds the pure
//! 2D area/area-boundary algorithms that operate on it. It implements the three
//! reference-surface rules from the Chief reference:
//!
//! - **Interior** — inner surfaces of each wall (room polygon as-is).
//! - **Standard** — each room edge pushed to the wall's standard reference
//!   line (outer face for exterior walls, centre line for interior walls).
//! - **Living** — interior rooms, excluding Garage/Porch/Deck/OpenBelow and
//!   rooms with a rough ceiling below `MIN_LIVING_CEILING`.
//!
//! All area functions are pure: `interior_area`/`standard_area` are recomputed
//! from geometry, while `living_area` reads the stored `Room::standard_area`
//! (call `refresh_areas` after edits so the stored fields stay current).
//!
//! No `unwrap`/`expect`/`panic` here; every input is checked (non-finite
//! values rejected, empty/degenerate rings fall back to the interior geometry).

use crate::poly::{ensure_ccw, offset_polygon, region_area, region_perimeter, ring_area, ring_perimeter};
use crate::{ArchModel, Room, RoomType};
use cadcraft_doc::Level;
use cadcraft_geom::Vec2;

/// Minimum rough ceiling height for a room to count toward living area.
pub const MIN_LIVING_CEILING: f64 = 1.2;

/// The reference surface an area rule measures against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Perimeter {
    /// The outer ring of the standard reference surface.
    Standard,
    /// The interior geometry (room polygon and its holes).
    Interior,
    /// The outer boundary only (no hole contribution).
    Exterior,
}

/// Interior area: the room polygon minus its holes (inner surfaces).
pub fn interior_area(room: &Room) -> f64 {
    region_area(&room.polygon, &room.holes)
}

/// The effective rough ceiling height for a room.
///
/// Prefers the room's own `ceiling_height`, falling back to the level's
/// `ceiling_height`; returns `None` when the chosen value is non-finite so
/// the caller can treat it as "unknown" rather than "zero".
pub fn effective_ceiling(room: &Room, level: Option<&Level>) -> Option<f64> {
    let v = room.ceiling_height.or(level.map(|l| l.ceiling_height));
    v.filter(|v| v.is_finite())
}

/// Whether a room counts toward living area: it must be an interior room type
/// (not Garage/Porch/Deck/OpenBelow) and — if a ceiling height is known —
/// that height must be at least `MIN_LIVING_CEILING`.
///
/// A missing ceiling (`None`) does not exclude the room: unknown data is
/// treated as "at least the living minimum", not "too low".
pub fn is_living(room: &Room, level: Option<&Level>) -> bool {
    room.room_type.is_interior() && effective_ceiling(room, level).map_or(true, |c| c >= MIN_LIVING_CEILING)
}

/// Sum of `Room::standard_area` over every living room.
///
/// Reads the stored `Room::standard_area` fields (documented: call
/// `refresh_areas` after edits); it does not recompute areas.
pub fn living_area(model: &ArchModel) -> f64 {
    model.rooms.iter().filter(|r| is_living(r, model.level(r.level))).map(|r| r.standard_area).sum::<f64>()
}

/// How far a room edge at `a`->`b` must move outward to reach the standard
/// reference line of the wall it lies on; `0.0` when nothing matches.
///
/// `out` must be the edge's outward unit normal. For every non-divider wall
/// on `level` that the edge sits on (both endpoints within `1e-6` of a face
/// line), the reference line is the opposite face for exterior walls and the
/// centre line for interior walls. The projected distance is `s` (clamped at
/// `1e-9`; a reference on the room's own side gives `0.0`), and the largest
/// candidate over all matching walls is returned.
pub fn edge_standard_offset(model: &ArchModel, level: u32, a: Vec2, b: Vec2, out: Vec2) -> f64 {
    let dir = (b - a).normalized();
    if dir == Vec2::ZERO || !out.is_finite() {
        return 0.0;
    }
    let mut best: f64 = 0.0;
    for w in &model.walls {
        if w.base_level != level || !w.thickness.is_finite() || w.thickness <= 0.0 || w.divider {
            continue;
        }
        let Some(faces) = crate::join::face_lines(w) else {
            continue;
        };
        for (idx, face) in faces.iter().enumerate() {
            let (fa, fb) = face;
            if edge_on_face(a, b, *fa, *fb) {
                let (ref_a, ref_b) = if w.exterior {
                    // The room edge sits on the interior face; the standard
                    // reference is the opposite (outer) face.
                    let other = faces[(idx + 1) % 2];
                    (other.0, other.1)
                } else {
                    // Interior wall: standard reference is the centre line,
                    // i.e. the segment between the midpoints of the faces.
                    let m0 = (faces[0].0 + faces[0].1) / 2.0;
                    let m1 = (faces[1].0 + faces[1].1) / 2.0;
                    (m0, m1)
                };
                let ref_mid = (ref_a + ref_b) / 2.0;
                let s = (ref_mid - a).dot(out);
                let candidate = if s > 1e-9 { s } else { 0.0 };
                best = best.max(candidate);
            }
        }
    }
    best
}

/// Standard area of a room: outer ring offset to wall reference lines minus the
/// inward-offset hole rings, clamped to `>= 0.0`.
///
/// Falls back to `interior_area(room)` when the outer offset is degenerate
/// (`standard_rings` returns `None`).
pub fn standard_area(model: &ArchModel, room: &Room) -> f64 {
    match standard_rings(model, room) {
        Some((outer, holes)) => {
            let mut area = ring_area(&outer);
            for h in &holes {
                area -= ring_area(h);
            }
            area.max(0.0)
        }
        None => interior_area(room),
    }
}

/// Perimeter of a room on the reference surface `which`.
///
/// - `Interior`: the room polygon minus its holes.
/// - `Standard`: outer ring + each hole ring, falling back to interior when the offset is degenerate.
/// - `Exterior`: outer ring only; `0.0` when the offset is degenerate.
pub fn perimeter(model: &ArchModel, room: &Room, which: Perimeter) -> f64 {
    match which {
        Perimeter::Interior => region_perimeter(&room.polygon, &room.holes),
        Perimeter::Standard => match standard_rings(model, room) {
            Some((outer, holes)) => ring_perimeter(&outer) + holes.iter().map(|h| ring_perimeter(h)).sum::<f64>(),
            None => region_perimeter(&room.polygon, &room.holes),
        },
        Perimeter::Exterior => match standard_rings(model, room) {
            Some((outer, _)) => ring_perimeter(&outer),
            None => 0.0,
        },
    }
}

/// Re-stores `Room::interior_area` and `Room::standard_area` for every room,
/// computed from the free functions. Two passes (compute, then write) keep the
/// mutable borrow and the immutable `model`/`room` borrows from conflicting.
pub fn refresh_areas(model: &mut ArchModel) {
    let n = model.rooms.len();
    let mut values = Vec::with_capacity(n);
    for i in 0..n {
        let interior = interior_area(&model.rooms[i]);
        let standard = standard_area(model, &model.rooms[i]);
        values.push((interior, standard));
    }
    for (i, (interior, standard)) in values.into_iter().enumerate() {
        model.rooms[i].interior_area = interior;
        model.rooms[i].standard_area = standard;
    }
}

/// Offset the outer ring outward and each hole inward to the walls' standard
/// reference lines. `None` when the outer offset is degenerate
/// (callers fall back to interior geometry).
fn standard_rings(model: &ArchModel, room: &Room) -> Option<(Vec<Vec2>, Vec<Vec<Vec2>>)> {
    if room.polygon.len() < 3 {
        return None;
    }
    let outer = ensure_ccw(&room.polygon);
    let n = outer.len();
    let mut dist = Vec::with_capacity(n);
    for i in 0..n {
        let a = outer[i];
        let b = outer[(i + 1) % n];
        // CCW outer ring: the interior is to the left, so the outward normal is the
        // clockwise rotation of the edge direction.
        let out = -(b - a).normalized().perp();
        dist.push(edge_standard_offset(model, room.level, a, b, out));
    }
    let off = crate::poly::offset_polygon(&outer, &dist, true)?;

    let mut holes = Vec::with_capacity(room.holes.len());
    for h in &room.holes {
        let hc = ensure_ccw(h);
        let hn = hc.len();
        let mut hd = Vec::with_capacity(hn);
        for i in 0..hn {
            let a = hc[i];
            let b = hc[(i + 1) % hn];
            // A CCW hole: the room is outside the hole, so the "away from the
            // room" direction is into the hole = the CCW (inward) normal.
            let out = (b - a).normalized().perp();
            hd.push(edge_standard_offset(model, room.level, a, b, out));
        }
        holes.push(crate::poly::offset_polygon(&hc, &hd, false).unwrap_or_else(|| hc.clone()));
    }
    Some((off, holes))
}

/// Whether the room edge `a`→`b` lies on the (possibly longer or shorter) face line
/// `fa`→`fb`: collinear within `1e-6` and with overlapping parameter intervals.
fn edge_on_face(a: Vec2, b: Vec2, fa: Vec2, fb: Vec2) -> bool {
    let dir = fb - fa;
    let len = dir.dist(Vec2::ZERO);
    if len < 1e-9 {
        return false;
    }
    let d = dir.normalized();
    let n = d.perp();
    if (a - fa).dot(n).abs() > 1e-6 || (b - fa).dot(n).abs() > 1e-6 {
        return false;
    }
    let t0 = (a - fa).dot(d);
    let t1 = (b - fa).dot(d);
    let (lo, hi) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
    hi > -1e-6 && lo < len + 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArchModel, Room, RoomType};
    use cadcraft_doc::{Level, Wall};
    use cadcraft_geom::Vec2;

    fn pt(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn push_exterior(model: &mut ArchModel, a: Vec2, b: Vec2) {
        let mut w = Wall::new(a, b, 0.2);
        w.base_level = 0;
        w.exterior = true;
        model.push_wall(w);
    }

    fn push_interior(model: &mut ArchModel, a: Vec2, b: Vec2, t: f64) {
        let mut w = Wall::new(a, b, t);
        w.base_level = 0;
        w.exterior = false;
        w.divider = false;
        model.push_wall(w);
    }

    fn south_room() -> Room {
        Room {
            polygon: vec![pt(0.1, 0.1), pt(9.9, 0.1), pt(9.9, 2.95), pt(0.1, 2.95)],
            holes: vec![],
            room_type: RoomType::Living,
            level: 0,
            ..Default::default()
        }
    }

    fn north_west_room() -> Room {
        Room {
            polygon: vec![pt(0.1, 3.05), pt(5.95, 3.05), pt(5.95, 7.9), pt(0.1, 7.9)],
            holes: vec![],
            room_type: RoomType::Living,
            level: 0,
            ..Default::default()
        }
    }

    fn north_east_room() -> Room {
        Room {
            polygon: vec![pt(6.05, 3.05), pt(9.9, 3.05), pt(9.9, 7.9), pt(6.05, 7.9)],
            holes: vec![],
            room_type: RoomType::Living,
            level: 0,
            ..Default::default()
        }
    }

    #[test]
    fn standard_areas_of_the_phase1_house() {
        let mut model = ArchModel::default();
        push_exterior(&mut model, pt(0.0, 0.0), pt(10.0, 0.0)); // south
        push_exterior(&mut model, pt(10.0, 0.0), pt(10.0, 8.0)); // east
        push_exterior(&mut model, pt(10.0, 8.0), pt(0.0, 8.0)); // north
        push_exterior(&mut model, pt(0.0, 8.0), pt(0.0, 0.0)); // west
        push_interior(&mut model, pt(0.0, 3.0), pt(10.0, 3.0), 0.1); // divider
        push_interior(&mut model, pt(6.0, 3.0), pt(6.0, 8.0), 0.1); // divider

        let south = south_room();
        let nw = north_west_room();
        let ne = north_east_room();

        assert!((standard_area(&model, &south) - 31.62).abs() < 1e-6);
        assert!((standard_area(&model, &nw) - 31.11).abs() < 1e-6);
        assert!((standard_area(&model, &ne) - 20.91).abs() < 1e-6);
    }

    #[test]
    fn standard_areas_sum_to_the_outer_face_footprint() {
        let mut model = ArchModel::default();
        push_exterior(&mut model, pt(0.0, 0.0), pt(10.0, 0.0));
        push_exterior(&mut model, pt(10.0, 0.0), pt(10.0, 8.0));
        push_exterior(&mut model, pt(10.0, 8.0), pt(0.0, 8.0));
        push_exterior(&mut model, pt(0.0, 8.0), pt(0.0, 0.0));
        push_interior(&mut model, pt(0.0, 3.0), pt(10.0, 3.0), 0.1);
        push_interior(&mut model, pt(6.0, 3.0), pt(6.0, 8.0), 0.1);

        let total = standard_area(&model, &south_room()) + standard_area(&model, &north_west_room()) + standard_area(&model, &north_east_room());
        assert!((total - 83.64).abs() < 1e-6);
    }

    #[test]
    fn interior_area_excludes_the_loop_hole() {
        let south = south_room();
        assert!((interior_area(&south) - 27.93).abs() < 1e-6);

        let mut room = south_room();
        room.holes.push(vec![pt(2.45, 0.95), pt(3.55, 0.95), pt(3.55, 2.05), pt(2.45, 2.05)]);
        assert!((interior_area(&room) - 26.72).abs() < 1e-6);
    }

    #[test]
    fn standard_area_shrinks_the_loop_hole_to_centre_lines() {
        let mut model = ArchModel::default();
        push_exterior(&mut model, pt(0.0, 0.0), pt(10.0, 0.0));
        push_exterior(&mut model, pt(10.0, 0.0), pt(10.0, 8.0));
        push_exterior(&mut model, pt(10.0, 8.0), pt(0.0, 8.0));
        push_exterior(&mut model, pt(0.0, 8.0), pt(0.0, 0.0));
        push_interior(&mut model, pt(0.0, 3.0), pt(10.0, 3.0), 0.1);
        push_interior(&mut model, pt(6.0, 3.0), pt(6.0, 8.0), 0.1);

        // four interior loop walls on centre lines 2.5..3.5 x 1.0..2.0
        push_interior(&mut model, pt(2.5, 1.0), pt(3.5, 1.0), 0.1); // bottom
        push_interior(&mut model, pt(2.5, 2.0), pt(3.5, 2.0), 0.1); // top
        push_interior(&mut model, pt(2.5, 1.0), pt(2.5, 2.0), 0.1); // left
        push_interior(&mut model, pt(3.5, 1.0), pt(3.5, 2.0), 0.1); // right

        let mut room = south_room();
        room.holes.push(vec![pt(2.45, 0.95), pt(3.55, 0.95), pt(3.55, 2.05), pt(2.45, 2.05)]);
        assert!((standard_area(&model, &room) - 30.62).abs() < 1e-6);
    }

    #[test]
    fn living_area_excludes_garage_porch_deck_open_below() {
        let mut model = ArchModel::default();
        let mut make = |rt: RoomType| Room {
            standard_area: 10.0,
            room_type: rt,
            level: 0,
            polygon: vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)],
            ..Default::default()
        };
        model.rooms.push(make(RoomType::Living));
        model.rooms.push(make(RoomType::Garage));
        model.rooms.push(make(RoomType::Porch));
        model.rooms.push(make(RoomType::Deck));
        model.rooms.push(make(RoomType::OpenBelow));

        assert!((living_area(&model) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn low_ceiling_room_is_not_living() {
        let level_low = Level { ceiling_height: 1.0, ..Default::default() };
        let level_ok = Level { ceiling_height: 1.2, ..Default::default() };
        let base = Room { polygon: vec![], room_type: RoomType::Living, level: 0, ..Default::default() };

        // ceiling 1.1 -> not living
        assert!(!is_living(&Room { ceiling_height: Some(1.1), ..base.clone() }, None));

        // ceiling 1.2 -> living
        assert!(is_living(&Room { ceiling_height: Some(1.2), ..base.clone() }, None));

        // ceiling None + level with ceiling 1.0 -> not living
        assert!(!is_living(&base, Some(&level_low)));

        // ceiling None + no level -> living
        assert!(is_living(&base, None));
    }

    #[test]
    fn perimeter_standard_is_larger_than_interior() {
        let mut model = ArchModel::default();
        push_exterior(&mut model, pt(0.0, 0.0), pt(10.0, 0.0));
        push_exterior(&mut model, pt(10.0, 0.0), pt(10.0, 8.0));
        push_exterior(&mut model, pt(10.0, 8.0), pt(0.0, 8.0));
        push_exterior(&mut model, pt(0.0, 8.0), pt(0.0, 0.0));
        push_interior(&mut model, pt(0.0, 3.0), pt(10.0, 3.0), 0.1);

        let r = south_room();
        let interior = perimeter(&model, &r, Perimeter::Interior);
        let standard = perimeter(&model, &r, Perimeter::Standard);
        assert!((interior - 25.3).abs() < 1e-6);
        assert!((standard - 26.6).abs() < 1e-6);
        assert!(standard > interior);
    }

    #[test]
    fn standard_area_falls_back_when_offset_degenerate() {
        let mut model = ArchModel::default();

        // thin spike: offsets are degenerate -> fall back to interior area.
        let spike = Room { polygon: vec![pt(0.0, 0.0), pt(0.0001, 0.0), pt(0.0, 1.0)], room_type: RoomType::Living, level: 0, ..Default::default() };
        let interior = interior_area(&spike);
        let standard = standard_area(&model, &spike);
        assert!(interior >= 0.0);
        assert!((standard - interior).abs() < 1e-6);

        // empty polygon: standard area is 0.0.
        let empty = Room { polygon: vec![], room_type: RoomType::Living, level: 0, ..Default::default() };
        assert_eq!(standard_area(&model, &empty), 0.0);
    }

    #[test]
    fn refresh_areas_stores_both_fields() {
        let mut model = ArchModel::default();
        push_exterior(&mut model, pt(0.0, 0.0), pt(10.0, 0.0));
        push_exterior(&mut model, pt(10.0, 0.0), pt(10.0, 8.0));
        push_exterior(&mut model, pt(10.0, 8.0), pt(0.0, 8.0));
        push_exterior(&mut model, pt(0.0, 8.0), pt(0.0, 0.0));
        push_interior(&mut model, pt(0.0, 3.0), pt(10.0, 3.0), 0.1);

        model.rooms.push(south_room());
        model.rooms.push(north_west_room());

        refresh_areas(&mut model);

        for r in &model.rooms {
            let expected_interior = interior_area(r);
            let expected_standard = standard_area(&model, r);
            assert!((r.interior_area - expected_interior).abs() < 1e-6);
            assert!((r.standard_area - expected_standard).abs() < 1e-6);
        }
    }
}
