//! Outer-face footprint for the roof generator.
//!
//! Computes the CCW outer boundary of a building level from the arch model: each
//! exterior, non-divider wall contributes its outer-face edge (the long poché edge
//! facing away from the building interior). Collinear runs are merged into a single
//! edge attributed to the first wall. The result is a simple, finite, bounded ring
//! (≤ 256 edges) used by the roof skeleton (phase 4).

use cadcraft_doc::{ArchId, ArchModel, Roof, RoofDirective, RoofPlane, Wall};
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

/// Errors from building roof geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoofError {
    /// No exterior, non-divider walls on the level to build a roof from.
    NoWalls,
    /// The outer-face footprint could not be built (self-crossing, too many edges, non-simple).
    BadFootprint,
    /// The roof skeleton could not be built or failed its invariant checks.
    Skeleton,
    /// The resulting roof has more planes than [`cadcraft_doc::MAX_ROOF_PLANES`].
    TooComplex,
    /// A degenerate geometry: a non-finite node, or no usable roof planes.
    Degenerate,
}

impl std::fmt::Display for RoofError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                RoofError::NoWalls => "no exterior walls on the level to build a roof from",
                RoofError::BadFootprint => "the outer-face footprint could not be built",
                RoofError::Skeleton => "the roof skeleton could not be built or failed its checks",
                RoofError::TooComplex => "the roof has too many planes to be stored",
                RoofError::Degenerate => "the roof geometry is degenerate",
            }
        )
    }
}

impl std::error::Error for RoofError {}

/// The geometry of a roof built from the skeleton: the skeleton itself, the footprint it
/// was built from, the per-edge slopes, and the per-edge eave elevations.
///
/// `skel.nodes[i].time` is the rise above the eave (the skeleton is weighted by `1 / slope`),
/// so a node's elevation is `eave_z + time`.
#[derive(Clone, Debug, PartialEq)]
pub struct RoofGeom {
    /// The roof skeleton built from the footprint.
    pub skel: cadcraft_geom::Skeleton,
    /// The outer-face footprint the skeleton was built from.
    pub fp: Footprint,
    /// Per-edge slope (rise/run); `f64::INFINITY` for a gable end (zero-area face).
    pub slopes: Vec<f64>,
    /// Per-edge eave elevation: level elevation + owning wall height.
    pub eave_z: Vec<f64>,
}

/// Build the roof geometry for `level` from `model` and the seed `roof`.
///
/// Returns [`RoofError::NoWalls`] when there are no exterior walls on the level,
/// [`RoofError::BadFootprint`] when the footprint cannot be built,
/// [`RoofError::Skeleton`] when the skeleton fails its build or invariant checks, and
/// [`RoofError::Degenerate`] when a node is non-finite.
pub fn roof_geometry(model: &ArchModel, level: u32, roof: &Roof) -> Result<RoofGeom, RoofError> {
    let fp = match outer_footprint(model, level) {
        Some(fp) => fp,
        None => {
            if !model.walls.iter().any(|w| w.exterior && w.base_level == level) {
                return Err(RoofError::NoWalls);
            }
            return Err(RoofError::BadFootprint);
        }
    };

    let slopes = wall_slopes(model, level, &fp, roof);

    let skel = match cadcraft_geom::roof_skeleton(&fp.outer, &[], &[slopes.clone()]) {
        Ok(skel) => skel,
        Err(_) => return Err(RoofError::Skeleton),
    };

    if !cadcraft_geom::check(&skel, 1e-9).is_empty() {
        return Err(RoofError::Skeleton);
    }

    for node in &skel.nodes {
        if !node.pos.is_finite() || !node.time.is_finite() {
            return Err(RoofError::Degenerate);
        }
    }

    let elevation = model.level(level).map(|l| l.elevation).unwrap_or(0.0);
    let eave_z = fp.edges.iter().map(|e| elevation + model.wall(e.wall).map(|w| w.height).unwrap_or(2.7)).collect();

    Ok(RoofGeom { skel, fp, slopes, eave_z })
}

/// Build a complete [`Roof`] for `level` from `model` and the seed `roof`.
///
/// Never returns a partial roof: on any failure it returns the corresponding
/// [`RoofError`] and the caller keeps the previous roof. The plane `edges` are left
/// empty (a later segment classifies them).
pub fn build_roof(model: &ArchModel, level: u32, roof: &Roof) -> Result<Roof, RoofError> {
    let geom = roof_geometry(model, level, roof)?;
    let skel = &geom.skel;
    let fp = &geom.fp;
    let slopes = &geom.slopes;
    let eave_z = &geom.eave_z;

    let mut planes = Vec::with_capacity(skel.faces.len());
    for face in &skel.faces {
        // Only ring 0 exists here (the footprint has no holes).
        if face.ring != 0 {
            continue;
        }
        // Face `f` uses ring edge `f.edge` of ring `f.ring`; the footprint ring is already
        // cleaned of straight-through vertices, so this indexes `fp.edges`/`slopes` directly.
        let ring_points: Vec<Vec2> = face.nodes.iter().map(|&i| skel.nodes[i].pos).collect();
        for p in &ring_points {
            if !p.is_finite() {
                return Err(RoofError::Degenerate);
            }
        }
        // Drop consecutive duplicates and a trailing point equal to the first.
        let mut ring: Vec<Vec2> = Vec::with_capacity(ring_points.len());
        for &p in &ring_points {
            if ring.is_empty() || !ring.last().map_or(true, |q| q.near(p, 1e-9)) {
                ring.push(p);
            }
        }
        if ring.len() > 1 && ring.last().map_or(false, |q| q.near(ring[0], 1e-9)) {
            ring.pop();
        }
        let ring = crate::poly::ensure_ccw(&ring);

        let projected_area = crate::poly::ring_area(&ring).abs();
        if projected_area < 1e-9 {
            // A gable end (weight 0) sweeps a zero-area face; it is not a roof plane.
            continue;
        }

        let edge = &fp.edges[face.edge];
        planes.push(RoofPlane {
            ring,
            slope: slopes[face.edge],
            projected_area,
            eave_wall: Some(edge.wall),
            base_edge: [edge.a.x, edge.a.y, edge.b.x, edge.b.y],
            thickness: roof.thickness,
            eave_z: eave_z[face.edge],
        });
    }

    if planes.is_empty() {
        return Err(RoofError::Degenerate);
    }
    if planes.len() > cadcraft_doc::MAX_ROOF_PLANES {
        return Err(RoofError::TooComplex);
    }

    Ok(Roof {
        id: roof.id,
        level,
        pitch_rise: roof.pitch_rise,
        pitch_run: roof.pitch_run,
        overhang: roof.overhang,
        thickness: roof.thickness,
        fascia: roof.fascia,
        soffit: roof.soffit,
        manual: roof.manual,
        hand_edited: roof.hand_edited,
        planes,
        edges: Vec::new(),
    })
}

/// The apex elevation of a single roof plane: the eave elevation plus the rise to the
/// farthest point of the plane's ring from its base edge. Returns 0.0 for an empty ring
/// or a zero-length base edge.
pub fn plane_apex_z(plane: &RoofPlane) -> f64 {
    if plane.ring.is_empty() {
        return 0.0;
    }
    let a = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let b = Vec2::new(plane.base_edge[2], plane.base_edge[3]);
    let ab = b - a;
    if ab.len() < 1e-12 {
        return 0.0;
    }
    let run = plane.ring.iter().map(|p| (ab.cross(*p - a)).abs() / ab.len()).fold(0.0, f64::max);
    plane.eave_z + run * plane.slope
}

/// The apex elevation of a whole roof: the maximum over its planes, or 0.0 when it
/// has none.
pub fn roof_apex_z(roof: &Roof) -> f64 {
    roof.planes.iter().map(plane_apex_z).fold(0.0, f64::max)
}

/// Sum of the finite projected areas of a roof's planes.
pub fn projected_area_sum(roof: &Roof) -> f64 {
    roof.planes.iter().filter(|p| p.projected_area.is_finite()).map(|p| p.projected_area).sum()
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
        assert!(slopes.iter().any(|s| s.is_infinite() && *s > 0.0), "expected an INFINITY slope for the gable wall");
        let default = roof.slope();
        assert!(slopes.iter().all(|s| *s == default || s.is_infinite()), "non-gable slopes must equal the roof default");
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

    /// A fixture roof: no overhang, no thickness, default 6/12 pitch.
    fn test_roof() -> Roof {
        Roof { overhang: 0.0, thickness: 0.0, ..Roof::default() }
    }

    /// A 10 × 8 rectangular house, 0.2 m walls, all 2.4 m high, level 0.
    fn house_10_8(model: &mut ArchModel) {
        rectangular_house(model);
        for w in model.walls.iter_mut() {
            w.height = 2.4;
        }
    }

    #[test]
    fn hip_roof_has_four_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 4, "a hip roof over a rectangle should have 4 planes");
    }

    #[test]
    fn hip_ridge_is_at_the_expected_height() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        // 2.4 + 4.1 * 0.5 = 4.45
        assert!((roof_apex_z(&roof) - 4.45).abs() < 1e-6, "roof_apex_z {} != 4.45", roof_apex_z(&roof));
        // The ridge nodes sit at y ≈ 4.0 and span x from 4.0 to 6.0.
        let geom = roof_geometry(&model, 0, &test_roof()).unwrap();
        let ridge_nodes: Vec<&cadcraft_geom::SkeletonNode> = geom.skel.nodes.iter().filter(|n| (n.pos.y - 4.0).abs() < 1e-6).collect();
        assert!(!ridge_nodes.is_empty(), "no ridge nodes found");
        let min_x = ridge_nodes.iter().map(|n| n.pos.x).fold(f64::INFINITY, f64::min);
        let max_x = ridge_nodes.iter().map(|n| n.pos.x).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - 4.0).abs() < 1e-6, "ridge min_x {} != 4.0", min_x);
        assert!((max_x - 6.0).abs() < 1e-6, "ridge max_x {} != 6.0", max_x);
    }

    #[test]
    fn ridge_height_tracks_pitch() {
        let pitches = [(4.0, 12.0, 3.766667), (6.0, 12.0, 4.45), (12.0, 12.0, 6.5)];
        for (rise, run, expected) in pitches {
            let mut model = ArchModel::default();
            house_10_8(&mut model);
            let roof = Roof { pitch_rise: rise, pitch_run: run, overhang: 0.0, thickness: 0.0, ..Roof::default() };
            let built = build_roof(&model, 0, &roof).unwrap();
            let apex = roof_apex_z(&built);
            assert!((apex - expected).abs() < 1e-6, "pitch {rise}/{run}: apex {} != {}", apex, expected);
        }
    }

    #[test]
    fn square_gives_a_pyramid() {
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 4.0), Vec2::new(0.0, 0.0), 0.2));
        for w in model.walls.iter_mut() {
            w.height = 2.4;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 4, "a 4x4 square hip roof should have 4 triangular planes");
        // All four planes meet at the centre (2.0, 2.0).
        for plane in &roof.planes {
            assert!(
                plane.ring.iter().any(|p| (p.x - 2.0).abs() < 1e-6 && (p.y - 2.0).abs() < 1e-6),
                "a plane ring should meet at the centre: {:?}",
                plane.ring
            );
        }
    }

    #[test]
    fn two_gables_give_two_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Left wall (x=0, wall 3) and right wall (x=10, wall 1) become gable ends.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(3)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 2, "two gable ends should give 2 planes");
        // The two planes' base edges span x from -0.1 to 10.1.
        let min_x = roof.planes.iter().flat_map(|p| [p.base_edge[0], p.base_edge[2]]).fold(f64::INFINITY, f64::min);
        let max_x = roof.planes.iter().flat_map(|p| [p.base_edge[0], p.base_edge[2]]).fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x - (-0.1)).abs() < 1e-6, "base edge min_x {} != -0.1", min_x);
        assert!((max_x - 10.1).abs() < 1e-6, "base edge max_x {} != 10.1", max_x);
    }

    #[test]
    fn gable_plus_hip_gives_three_planes() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        // Only the right wall (x=10, wall 1) is a gable end.
        if let Some(w) = model.walls.iter_mut().find(|w| w.id == ArchId(1)) {
            w.roof_directive = RoofDirective::Gable;
        }
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        assert_eq!(roof.planes.len(), 3, "one gable end should give 3 planes");
    }

    #[test]
    fn every_plane_slope_equals_its_pitch() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        for plane in &roof.planes {
            let Some(wall) = plane.eave_wall.as_ref().and_then(|id| model.wall(*id)) else {
                continue;
            };
            let pitch = wall.roof_pitch.unwrap_or(roof.pitch_rise);
            let run = roof.pitch_run;
            let expected = if pitch.is_finite() && run.is_finite() && run > 0.0 { pitch / run } else { roof.pitch_rise / roof.pitch_run };
            assert!((plane.slope - expected).abs() < 1e-12, "plane slope {} != pitch {} for wall {:?}", plane.slope, expected, wall.id);
        }
    }

    #[test]
    fn projected_areas_sum_to_the_footprint() {
        let mut model = ArchModel::default();
        house_10_8(&mut model);
        let roof = build_roof(&model, 0, &test_roof()).unwrap();
        let sum = projected_area_sum(&roof);
        assert!((sum - 83.64).abs() < 1e-6, "projected area sum {} != 83.64 (10.2 x 8.2)", sum);
    }

    #[test]
    fn bad_footprint_returns_err_not_a_partial_roof() {
        // No exterior walls on the level → NoWalls, no roof.
        let mut model = ArchModel::default();
        let mut w = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2);
        w.exterior = false;
        w.divider = true;
        model.push_wall(w);
        assert!(matches!(build_roof(&model, 0, &test_roof()), Err(RoofError::NoWalls)));

        // A self-crossing (bowtie) ring → error, no partial roof.
        let mut model = ArchModel::default();
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 20.0), 0.2));
        model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 20.0), 0.2));
        assert!(matches!(build_roof(&model, 0, &test_roof()), Err(_)));
    }
}
