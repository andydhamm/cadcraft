//! Outer-face footprint for the roof generator.
//!
//! Computes the CCW outer boundary of a building level from the arch model: each
//! exterior, non-divider wall contributes its outer-face edge (the long poché edge
//! facing away from the building interior). Collinear runs are merged into a single
//! edge attributed to the first wall. The result is a simple, finite, bounded ring
//! (≤ 256 edges) used by the roof skeleton (phase 4).

use cadcraft_doc::{ArchId, ArchModel, Roof, RoofDirective, Wall};
use cadcraft_geom::Vec2;

/// One edge of the outer-face footprint, attributed to the wall it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct FootprintEdge {
    /// The wall this edge belongs to.
    pub wall: ArchId,
    /// Start point of the edge.
    pub a: Vec2,
    /// End point of the edge.
    pub b: Vec2,
}

/// The outer-face footprint of a building level: a CCW ring of points plus the
/// per-edge wall attribution.
#[derive(Clone, Debug, PartialEq)]
pub struct Footprint {
    /// CCW outer ring points, no repeated first point.
    pub outer: Vec<Vec2>,
    /// Edges in ring order; `edges[i]` runs `outer[i]` → `outer[(i+1) % n]`.
    pub edges: Vec<FootprintEdge>,
}

/// Build the outer-face footprint of `level`.
///
/// Returns `None` when there are fewer than 3 exterior non-divider walls on the level,
/// the merged ring is not simple, any point is non-finite, or the ring has more than
/// 256 edges.
pub fn outer_footprint(model: &ArchModel, level: u32) -> Option<Footprint> {
    // 1. Exterior, non-divider walls on the level.
    let walls: Vec<&Wall> = model.walls.iter().filter(|w| w.exterior && !w.divider && w.base_level == level).collect();
    if walls.is_empty() {
        return None;
    }

    // 2. Building centroid (centre of the level walls' bounds).
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for w in &walls {
        if !w.start.is_finite() || !w.end.is_finite() {
            continue;
        }
        min_x = min_x.min(w.start.x);
        min_y = min_y.min(w.start.y);
        max_x = max_x.max(w.end.x);
        max_y = max_y.max(w.end.y);
        min_x = min_x.min(w.end.x);
        min_y = min_y.min(w.end.y);
        max_x = max_x.max(w.start.x);
        max_y = max_y.max(w.start.y);
    }
    if min_x.is_infinite() {
        return None;
    }
    let centroid = Vec2::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);

    // 3. Solved poché rings.
    let shapes = crate::join::solve_joins(model);

    // 4. One outer-face edge per wall, tagged with its angle around the centroid.
    let mut tagged: Vec<(f64, FootprintEdge)> = Vec::with_capacity(walls.len());
    for wall in &walls {
        let Some(shape) = shapes.iter().find(|s| s.id == wall.id) else {
            continue;
        };
        let Some((a, b)) = outer_face_edge(&shape.polygon, wall, &centroid) else {
            continue;
        };
        let edge = FootprintEdge { wall: wall.id, a, b };
        let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let v = mid - centroid;
        tagged.push((v.y.atan2(v.x), edge));
    }
    if tagged.len() < 3 {
        return None;
    }

    // 5. CCW order around the centroid.
    tagged.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // 6. Merge collinear, contiguous runs (keep the first wall's attribution).
    let mut merged: Vec<FootprintEdge> = Vec::with_capacity(tagged.len());
    for (_angle, edge) in tagged {
        if let Some(last) = merged.last_mut() {
            if collinear_contiguous(last, &edge) {
                last.b = edge.b;
                continue;
            }
        }
        merged.push(edge);
    }
    if merged.len() < 3 {
        return None;
    }

    // 7. Ring from edge starts; validate; orient CCW.
    let raw: Vec<Vec2> = merged.iter().map(|e| e.a).collect();
    if raw.len() > 256 {
        return None;
    }
    if raw.iter().any(|p| !p.is_finite()) {
        return None;
    }
    if !crate::poly::is_simple(&raw) {
        return None;
    }
    let outer = crate::poly::ensure_ccw(&raw);

    // 8. Re-attribute each oriented edge to its source wall (match by midpoint).
    let n = outer.len();
    let mut edges: Vec<FootprintEdge> = Vec::with_capacity(n);
    for i in 0..n {
        let a = outer[i];
        let b = outer[(i + 1) % n];
        let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let wall_id = merged
            .iter()
            .find(|e| {
                let emid = Vec2::new((e.a.x + e.b.x) / 2.0, (e.a.y + e.b.y) / 2.0);
                emid.dist(mid) < 1.0
            })
            .map(|e| e.wall)
            .unwrap_or(merged.first().map(|e| e.wall).unwrap_or_else(|| ArchId(0)));
        edges.push(FootprintEdge { wall: wall_id, a, b });
    }

    Some(Footprint { outer, edges })
}

/// The long poché edge of `ring` that faces away from the building `centroid`.
///
/// A candidate edge must be roughly parallel to the wall's centre line and lie on the
/// outer side of that line (the side opposite the centroid). The most parallel, most
/// outer candidate wins.
fn outer_face_edge(ring: &[Vec2], wall: &Wall, centroid: &Vec2) -> Option<(Vec2, Vec2)> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let Some(dir) = crate::wall::wall_dir(wall) else {
        return None;
    };
    let centre_mid = Vec2::new((wall.start.x + wall.end.x) / 2.0, (wall.start.y + wall.end.y) / 2.0);
    let to_centroid = *centroid - centre_mid;
    let perp = dir.perp();
    // Outer side = the side of the centre line opposite the centroid.
    let outer_side = if to_centroid.dot(perp) > 0.0 { -perp } else { perp };

    let mut best: Option<(f64, (Vec2, Vec2))> = None;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        if !a.is_finite() || !b.is_finite() {
            continue;
        }
        let edge_vec = b - a;
        let len = edge_vec.len();
        if len < 1e-12 {
            continue;
        }
        let edge_dir = edge_vec / len;
        let parallel = edge_dir.dot(dir);
        let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let side = (mid - centre_mid).dot(outer_side);
        if parallel > 0.5 && side > 0.0 {
            let score = parallel * side;
            if best.map_or(true, |b| score > b.0) {
                best = Some((score, (a, b)));
            }
        }
    }
    best.map(|(_, e)| e)
}

/// True when `first` and `second` are contiguous (`first.b` ≈ `second.a`) and on the
/// same line (cross product within tolerance).
fn collinear_contiguous(first: &mut FootprintEdge, second: &FootprintEdge) -> bool {
    if !first.b.near(second.a, 1e-6) {
        return false;
    }
    let ab = first.b - first.a;
    let cd = second.b - second.a;
    if ab.len() < 1e-12 || cd.len() < 1e-12 {
        return false;
    }
    ab.cross(cd).abs() < 1e-9
}

/// One slope per `fp.edges`.
///
/// A `RoofDirective::Gable` wall yields a vertical face (`f64::INFINITY`). Otherwise
/// the wall's `roof_pitch` (rise) over the roof's run, falling back to `roof.slope()`
/// when the pitch is non-finite or the run is not positive.
pub fn wall_slopes(model: &ArchModel, level: u32, fp: &Footprint, roof: &Roof) -> Vec<f64> {
    let default_slope = roof.slope();
    fp.edges
        .iter()
        .map(|edge| {
            let Some(wall) = model.wall(edge.wall) else {
                return default_slope;
            };
            if wall.roof_directive == RoofDirective::Gable {
                return f64::INFINITY;
            }
            let pitch = wall.roof_pitch.unwrap_or(roof.pitch_rise);
            if pitch.is_finite() && roof.pitch_run.is_finite() && roof.pitch_run > 0.0 { pitch / roof.pitch_run } else { default_slope }
        })
        .collect()
}

/// Highest level index owning an exterior wall, else the last level's index.
pub fn top_level(model: &ArchModel) -> u32 {
    let highest = model.walls.iter().filter(|w| w.exterior && !w.divider).map(|w| w.base_level).max();
    match highest {
        Some(h) => h,
        None => model.levels.last().map(|l| l.index).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{ArchModel, Level};
    use cadcraft_geom::Vec2;

    /// A 10 × 8 rectangular house, 0.2 m walls, level 0.
    fn rectangular_house(model: &mut ArchModel) {
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
    }

    fn ring_extents(fp: &Footprint) -> (f64, f64, f64, f64) {
        let xs: Vec<f64> = fp.outer.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = fp.outer.iter().map(|p| p.y).collect();
        (
            xs.iter().cloned().fold(f64::INFINITY, f64::min),
            ys.iter().cloned().fold(f64::INFINITY, f64::min),
            xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    #[test]
    fn footprint_of_the_phase1_house_is_10_2_by_8_2() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(fp.outer.len(), 4);
        assert_eq!(fp.edges.len(), 4);
        let (min_x, min_y, max_x, max_y) = ring_extents(&fp);
        assert!((max_x - min_x - 10.2).abs() < 1e-6, "width {} != 10.2", max_x - min_x);
        assert!((max_y - min_y - 8.2).abs() < 1e-6, "height {} != 8.2", max_y - min_y);
        assert!(crate::poly::ring_area(&fp.outer) > 0.0, "ring must be CCW");
    }

    #[test]
    fn footprint_merges_collinear_edges() {
        let mut model = ArchModel::default();
        // Split the bottom wall into two collinear runs.
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
        let fp = outer_footprint(&model, 0).unwrap();
        assert_eq!(fp.edges.len(), 4, "the two bottom runs must merge");
        let e = &fp.edges.iter().find(|e| (e.b.x - e.a.x).abs() > 5.0).unwrap();
        assert!((e.b.x - e.a.x - 10.2).abs() < 1e-6, "merged run length {} != 10.2", e.b.x - e.a.x);
    }

    #[test]
    fn footprint_rejects_self_crossing() {
        // A bowtie: two crossing diagonals plus two verticals produce a
        // non-simple outer-face ring.
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 20.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 20.0), 0.2));
        assert!(outer_footprint(&model, 0).is_none());

        // Fewer than 3 exterior walls is also rejected.
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        assert!(outer_footprint(&model, 0).is_none());
    }

    #[test]
    fn footprint_rejects_over_256_edges() {
        let mut model = ArchModel::default();
        // 257 walls around a circle → 257 edges > 256 cap.
        for i in 0..257 {
            let a = (i as f64) * (std::f64::consts::PI * 2.0 / 257.0);
            let b = ((i + 1) as f64) * (std::f64::consts::PI * 2.0 / 257.0);
            model.push_wall(Wall::new(Vec2::new(10.0 * a.cos(), 10.0 * a.sin()), Vec2::new(10.0 * b.cos(), 10.0 * b.sin()), 0.2));
        }
        assert!(outer_footprint(&model, 0).is_none());
    }

    #[test]
    fn gable_directive_gives_infinite_slope() {
        let mut model = ArchModel::default();
        rectangular_house(&mut model);
        let roof = Roof::default();
        // Set the right wall (id 1) to Gable.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let fp = outer_footprint(&model, 0).unwrap();
        let slopes = wall_slopes(&model, 0, &fp, &roof);
        assert_eq!(slopes.len(), 4);
        assert!(slopes.iter().any(|s| s.is_infinite() && s > 0.0), "expected an INFINITY slope for the gable wall");
        let default = roof.slope();
        assert!(slopes.iter().all(|s| s == default || s.is_infinite()), "non-gable slopes must equal the roof default");
    }

    #[test]
    fn top_level_picks_the_highest_level_with_walls() {
        let mut model = ArchModel::default();
        // A wall on level 0 and a wall on level 2.
        let w0 = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let w2 = model.push_wall(Wall::new(Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0), 0.2));
        // Move the second wall to level 2.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == w2) {
            w.base_level = 2;
        }
        // Ensure a level 2 exists.
        if model.level(2).is_none() {
            model.levels.push(Level { index: 2, name: "Level 3".into(), elevation: 2.7, ..Level::default() });
        }
        assert_eq!(top_level(&model), 2);

        // No exterior walls → last level (0).
        let empty = ArchModel::default();
        assert_eq!(top_level(&empty), 0);
    }
}
