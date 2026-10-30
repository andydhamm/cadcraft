//! Room detection: faces of the solved poché of the detected walls, named from the labels inside.

use cadcraft_doc::{ArchModel, Wall};
use cadcraft_geom::{Vec2, point_in_polygon};

use super::{Evidence, Label, RoomCand, WallRun};
use crate::poly::{region_area, ring_area};

/// Rooms smaller than this (m2) lose confidence.
const SMALL_ROOM_AREA: f64 = 1.0;

/// Find rooms from walls and labels.
///
/// The walls are solved into a poché and its enclosed faces become rooms. A room is named from
/// the label whose point lies inside it (outside every hole); of several, the one nearest the
/// polygon's centroid wins.
pub fn find_rooms(walls: &[WallRun], labels: &[Label]) -> Vec<RoomCand> {
    let mut model = ArchModel::default();
    for w in walls {
        if !w.start.is_finite() || !w.end.is_finite() || !w.thickness.is_finite() || w.thickness <= 0.0 {
            continue;
        }
        let wall = Wall { start: w.start, end: w.end, thickness: w.thickness, ..Wall::new(w.start, w.end, w.thickness) };
        model.push_wall(wall);
    }
    if model.walls.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for room in crate::room::detect_rooms(&model) {
        if room.polygon.len() < 3 {
            continue;
        }
        let area = if room.interior_area > 0.0 { room.interior_area } else { region_area(&room.polygon, &room.holes) };
        let centroid = centroid_of(&room.polygon);
        let mut best: Option<(&Label, f64)> = None;
        for l in labels {
            if !l.at.is_finite() || !point_in_polygon(&room.polygon, l.at) {
                continue;
            }
            if room.holes.iter().any(|h| point_in_polygon(h, l.at)) {
                continue;
            }
            let d = (l.at.x - centroid.x).hypot(l.at.y - centroid.y);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((l, d));
            }
        }
        let name = best.map(|(l, _)| l.text.clone());
        let mut confidence = 0.7;
        let mut rules = vec!["faces".to_string()];
        if name.is_some() {
            confidence += 0.2;
            rules.push("room-label".to_string());
        }
        if area < SMALL_ROOM_AREA {
            confidence -= 0.4;
        }
        let mut handles: Vec<_> = best.map(|(l, _)| l.source).into_iter().collect();
        handles.sort();
        handles.dedup();
        rules.sort();
        out.push(RoomCand {
            polygon: room.polygon.clone(),
            name,
            area,
            label: best.map(|(l, _)| l.at),
            confidence,
            evidence: Evidence { handles, rules, ..Default::default() },
            ..Default::default()
        });
    }
    out
}

/// Vertex average, or the area centroid when the ring has area.
fn centroid_of(pts: &[Vec2]) -> Vec2 {
    let a = ring_area(pts);
    let n = pts.len();
    if a.abs() > 1e-12 && n >= 3 {
        let (mut cx, mut cy) = (0.0, 0.0);
        for i in 0..n {
            let (Some(p), Some(q)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
            let cr = p.x * q.y - q.x * p.y;
            cx += (p.x + q.x) * cr;
            cy += (p.y + q.y) * cr;
        }
        let f = 1.0 / (6.0 * a);
        if (cx * f).is_finite() && (cy * f).is_finite() {
            return Vec2::new(cx * f, cy * f);
        }
    }
    let k = n.max(1) as f64;
    Vec2::new(pts.iter().map(|p| p.x).sum::<f64>() / k, pts.iter().map(|p| p.y).sum::<f64>() / k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Handle;

    fn run(a: (f64, f64), b: (f64, f64), t: f64) -> WallRun {
        WallRun { start: Vec2::new(a.0, a.1), end: Vec2::new(b.0, b.1), thickness: t, confidence: 1.0, ..Default::default() }
    }

    fn label(x: f64, y: f64, t: &str) -> Label {
        Label::new(Vec2::new(x, y), t.into(), Handle(1))
    }

    fn apartment() -> Vec<WallRun> {
        vec![
            run((0.1, 0.1), (11.9, 0.1), 0.2),
            run((11.9, 0.1), (11.9, 7.9), 0.2),
            run((11.9, 7.9), (0.1, 7.9), 0.2),
            run((0.1, 7.9), (0.1, 0.1), 0.2),
            run((7.05, 0.1), (7.05, 7.9), 0.1),
            run((7.05, 5.05), (11.9, 5.05), 0.1),
        ]
    }

    #[test]
    fn apartment_walls_give_three_named_rooms() {
        let labels = [label(3.5, 3.6, "LIVING / DINING"), label(9.45, 3.6, "BEDROOM"), label(8.5, 6.4, "BATH")];
        let rooms = find_rooms(&apartment(), &labels);
        assert_eq!(rooms.len(), 3, "{rooms:?}");
        for n in ["LIVING / DINING", "BEDROOM", "BATH"] {
            assert_eq!(rooms.iter().filter(|r| r.name.as_deref() == Some(n)).count(), 1, "{n}");
        }
        assert!(rooms.iter().all(|r| r.label.is_some() && r.area > 1.0));
    }

    #[test]
    fn a_label_outside_every_room_names_nothing() {
        let rooms = find_rooms(&apartment(), &[label(20.0, 20.0, "GHOST")]);
        assert_eq!(rooms.len(), 3);
        assert!(rooms.iter().all(|r| r.name.is_none() && r.label.is_none()));
    }

    #[test]
    fn no_walls_no_rooms() {
        assert!(find_rooms(&[], &[label(1.0, 1.0, "X")]).is_empty());
    }
}
