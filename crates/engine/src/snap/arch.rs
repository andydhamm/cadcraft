//! Snapping to architectural objects (walls, openings).

use super::SnapHit;
use super::mode;
use cadcraft_arch::{join::face_lines, wall::wall_dir};
use cadcraft_doc::arch::Opening;
use cadcraft_doc::{Drawing, Wall};
use cadcraft_geom::{Segment, Vec2};

/// Maximum walls scanned by a single arch-snap query.
const MAX_ARCH_WALLS: usize = 4096;
/// Maximum openings scanned by a single arch-snap query.
const MAX_ARCH_OPENINGS: usize = 8192;

/// Snap to architectural objects (walls, openings).
///
/// Returns the closest hit among, in priority order:
/// 1. Wall face-line endpoints (`mode::END`, name `"wall end"`).
/// 2. Wall face-line midpoints (`mode::MID`, name `"wall mid"`).
/// 3. Opening rough-opening edges on both wall faces (`mode::END`, name `"opening edge"`).
/// 4. Nearest point on a wall face line (`mode::NEA`, name `"wall end"`).
///
/// Only `END`, `MID`, `NEA` OSMODE bits are respected; `OFF` disables everything.
/// Non-finite points are skipped. Returns `None` when no candidate is within `aperture`.
pub fn arch_snap(d: &Drawing, cursor: Vec2, aperture: f64, osmode: u32) -> Option<SnapHit> {
    if osmode == 0 || osmode & mode::OFF != 0 || !cursor.is_finite() || !(aperture.is_finite() && aperture > 0.0) {
        return None;
    }

    let model = &d.arch;
    let walls: Vec<&Wall> = model.walls.iter().take(MAX_ARCH_WALLS).collect();

    let mut cands: Vec<(f64, u8, u32, Vec2, &'static str)> = Vec::with_capacity(256);
    // (dist, priority, osmode, point, name)

    // 1. Wall face-line endpoints.
    if osmode & mode::END != 0 {
        for w in &walls {
            if let Some([(s1, e1), (s2, e2)]) = face_lines(w) {
                for p in [s1, e1, s2, e2] {
                    if p.is_finite() && p.dist(cursor) <= aperture {
                        cands.push((p.dist(cursor), 0, mode::END, p, "wall end"));
                    }
                }
            }
        }
    }

    // 2. Wall face-line midpoints.
    if osmode & mode::MID != 0 {
        for w in &walls {
            if let Some([(s1, e1), (s2, e2)]) = face_lines(w) {
                for (s, e) in [(s1, e1), (s2, e2)] {
                    let m = (s + e) * 0.5;
                    if m.is_finite() && m.dist(cursor) <= aperture {
                        cands.push((m.dist(cursor), 1, mode::MID, m, "wall mid"));
                    }
                }
            }
        }
    }

    // 3. Opening rough-opening edges on both wall faces.
    if osmode & mode::END != 0 {
        for op in model.openings.iter().take(MAX_ARCH_OPENINGS) {
            if !op.offset.is_finite() || !op.width.is_finite() {
                continue;
            }
            let Some(wl) = walls.iter().find(|w| w.id == op.host) else {
                continue;
            };
            let start = wl.start;
            let Some(dir) = wall_dir(wl) else {
                continue;
            };
            let perp = Vec2::new(-dir.y, dir.x);
            let half_t = wl.thickness / 2.0;
            let o1 = op.offset;
            let o2 = o1 + op.width;
            for o in [o1, o2] {
                if !o.is_finite() {
                    continue;
                }
                for sign in [1.0f64, -1.0f64] {
                    let p = start + dir * o + perp * (half_t * sign);
                    if p.is_finite() && p.dist(cursor) <= aperture {
                        cands.push((p.dist(cursor), 2, mode::END, p, "opening edge"));
                    }
                }
            }
        }
    }

    // 4. Nearest point on any wall face line.
    if osmode & mode::NEA != 0 {
        for w in &walls {
            if let Some([(s1, e1), (s2, e2)]) = face_lines(w) {
                for (s, e) in [(s1, e1), (s2, e2)] {
                    let seg = Segment::Line(cadcraft_geom::Line::new(s, e));
                    let closest = seg.closest(cursor);
                    if closest.is_finite() && closest.dist(cursor) <= aperture {
                        cands.push((closest.dist(cursor), 3, mode::NEA, closest, "wall nearest"));
                    }
                }
            }
        }
    }

    // Pick the closest, breaking ties by lower priority index (wall end > wall mid >
    // opening edge > nearest).
    let best = cands.iter().min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    best.map(|(_, _, m, point, name)| SnapHit { point: *point, mode: *m, name: *name, viewport: None, deferred: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snap::osnap;
    use cadcraft_doc::arch::Opening;
    use cadcraft_doc::{Common, Drawing, EntityKind, Line, Space};
    use cadcraft_geom::{Vec2, Vec3};

    /// A drawing with one wall (0,0)→(10,0), thickness 0.3.
    fn wall_drawing() -> Drawing {
        let mut d = Drawing::new_imperial();
        let wall = cadcraft_doc::Wall {
            id: cadcraft_arch::ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.3,
            ..Default::default()
        };
        d.arch.walls.push(wall);
        d
    }

    /// A drawing with one wall and one opening (offset 2.0, width 1.0).
    fn opening_drawing() -> Drawing {
        let mut d = wall_drawing();
        let opening = Opening { id: cadcraft_arch::ArchId(100), host: cadcraft_arch::ArchId(1), offset: 2.0, width: 1.0, ..Default::default() };
        d.arch.openings.push(opening);
        d
    }

    #[test]
    fn snaps_to_a_wall_endpoint() {
        let d = wall_drawing();
        // Top face-line endpoint at (10.0, 0.15).
        let hit = arch_snap(&d, Vec2::new(10.0, 0.15), 0.1, mode::END).unwrap();
        assert_eq!(hit.name, "wall end");
        assert!((hit.point - Vec2::new(10.0, 0.15)).len() < 0.01);
    }

    #[test]
    fn snaps_to_a_wall_face_midpoint() {
        let d = wall_drawing();
        // Top face-line midpoint at (5.0, 0.15).
        let hit = arch_snap(&d, Vec2::new(5.0, 0.15), 0.1, mode::MID).unwrap();
        assert_eq!(hit.name, "wall mid");
        assert!((hit.point - Vec2::new(5.0, 0.15)).len() < 0.01);
    }

    #[test]
    fn snaps_to_an_opening_edge() {
        let d = opening_drawing();
        // Opening edge on top face at offset 2.0: (2.0, 0.15).
        let hit = arch_snap(&d, Vec2::new(2.0, 0.15), 0.1, mode::END).unwrap();
        assert_eq!(hit.name, "opening edge");
        assert!((hit.point - Vec2::new(2.0, 0.15)).len() < 0.01);
    }

    #[test]
    fn arch_snap_ignores_non_finite_walls() {
        let mut d = Drawing::new_imperial();
        let bad_wall = cadcraft_doc::Wall {
            id: cadcraft_arch::ArchId(1),
            start: Vec2::new(f64::NAN, 0.0),
            end: Vec2::new(f64::INFINITY, 0.0),
            thickness: f64::NAN,
            ..Default::default()
        };
        d.arch.walls.push(bad_wall);
        assert!(arch_snap(&d, Vec2::new(0.0, 0.0), 1.0, mode::END | mode::MID | mode::NEA).is_none());
    }

    #[test]
    fn entity_snapping_is_unchanged() {
        let mut d = Drawing::new_imperial();
        let line = Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 0.0, 0.0) };
        d.add(&Space::Model, Common::default(), EntityKind::Line(line)).unwrap();
        // Arch objects far away – must not affect entity snap results.
        let wall = cadcraft_doc::Wall {
            id: cadcraft_arch::ArchId(1),
            start: Vec2::new(100.0, 100.0),
            end: Vec2::new(110.0, 100.0),
            thickness: 0.3,
            ..Default::default()
        };
        d.arch.walls.push(wall);

        let hit = osnap(&d, &Space::Model, Vec2::new(9.8, 0.1), 0.5, mode::END, None, false).unwrap();
        assert!((hit.point - Vec2::new(10.0, 0.0)).len() < 1e-9);
        assert_eq!(hit.name, "Endpoint");
    }
}
