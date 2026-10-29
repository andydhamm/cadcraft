//! Wall extrusion: turns a solved 2D `WallShape` into a 3D extruded mesh.

use crate::mesh::Mesh;
use crate::roof::underside_z;

use cadcraft_arch::join::WallShape;
use cadcraft_arch::opening::ROUGH_ALLOWANCE;
use cadcraft_doc::ArchId;
use cadcraft_doc::ArchModel;
use cadcraft_doc::arch::{Opening, OpeningKind, Wall};
use cadcraft_geom::{Vec2, point_in_polygon};

const HOLE_INSET: f64 = 1e-3;

/// Small offset added to the gable wall top so it extends slightly above the roof
/// underside, ensuring the wall is probeable at the ridge.
const GABLE_TOP_OFFSET: f64 = 1e-4;

/// A roof plane clipping the top of a wall.
///
/// The roof plane is defined by its base edge in plan (the eave line) and a
/// slope (rise/run). The height of the roof at any point `(x,y)` is
/// `eave_z + slope * distance_to_base_edge`.
#[derive(Debug, Clone, Copy, Default)]
pub struct RoofClip {
    /// The base edge in plan coordinates `[x0, y0, x1, y1]`.
    pub base_edge: [f64; 4],
    /// The slope (rise / run).
    pub slope: f64,
    /// The roof underside height at the eave line.
    pub eave_z: f64,
    /// The plan overhang from the roof outline to the eave line.
    pub overhang: f64,
}

/// A wall's elevation profile in wall-local coordinates.
///
/// `u` is the distance along the wall's centre line, `v` is the height above
/// the level elevation.
#[derive(Debug, Clone)]
pub struct WallProfile {
    /// The outer boundary of the profile (CCW).
    pub outer: Vec<Vec2>,
    /// Holes in the profile (CW).
    pub holes: Vec<Vec<Vec2>>,
}

/// Extrude a wall shape into a 3D mesh prism.
///
/// The shape's plan polygon is extruded from `level_elevation` to
/// `level_elevation + height`.
///
/// Returns `None` for degenerate rings (fewer than 3 vertices, zero area,
/// non-finite coordinates, or zero/negative height).
pub fn extrude_wall(shape: &WallShape, height: f64, level_elevation: f64, material: &str) -> Option<Mesh> {
    if !height.is_finite() || height < 1e-9 {
        return None;
    }
    if level_elevation.is_nan() || !level_elevation.is_finite() {
        return None;
    }
    let mut mesh = Mesh::new(material, shape.id);
    mesh.push_prism(&shape.polygon, level_elevation, level_elevation + height);
    (mesh.triangle_count() > 0).then(|| mesh)
}

/// Rough-opening holes for a wall profile `len` long and `max_v` tall, in profile
/// coords (`u` along the wall from `wall.start`, `v = z - base`).
/// Each hole is the rough opening (width + ROUGH_ALLOWANCE, head + ROUGH_ALLOWANCE,
/// no allowance below the sill), clipped against `outer`, kept only when its area
/// exceeds 1e-6 and every vertex lies at least `HOLE_INSET` inside `outer`.
/// Returned clockwise, as `push_prism_with_holes` expects.
pub(crate) fn opening_holes(openings: &[&Opening], base: f64, _len: f64, _max_v: f64, outer: &[Vec2]) -> Vec<Vec<Vec2>> {
    let mut holes = Vec::new();
    for op in openings {
        let half = (op.width + ROUGH_ALLOWANCE) / 2.0;
        let u0 = op.offset - half;
        let u1 = op.offset + half;
        let v0 = op.sill - base;
        let v1 = op.sill + op.height + ROUGH_ALLOWANCE - base;

        if !u0.is_finite() || !u1.is_finite() || !v0.is_finite() || !v1.is_finite() {
            continue;
        }

        let rect = vec![Vec2::new(u0, v0), Vec2::new(u1, v0), Vec2::new(u1, v1), Vec2::new(u0, v1)];

        let clipped = clip_polygon(&rect, outer);
        if clipped.len() < 3 || polygon_area(&clipped) <= 1e-6 {
            continue;
        }

        let mut all_inset = true;
        'vertex: for p in &clipped {
            for i in 0..outer.len() {
                let a = outer[i];
                let b = outer[(i + 1) % outer.len()];
                let edge_len = ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
                if edge_len < 1e-12 {
                    continue;
                }
                let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                if cross <= HOLE_INSET * edge_len {
                    all_inset = false;
                    break 'vertex;
                }
            }
        }
        if !all_inset {
            continue;
        }

        let mut cw = clipped;
        cw.reverse();
        holes.push(cw);
    }
    holes
}

fn bottom_door_notches(openings: &[&Opening], base: f64, len: f64) -> Vec<(f64, f64, f64)> {
    let mut cuts = Vec::new();
    for op in openings {
        if !matches!(op.kind, OpeningKind::Door | OpeningKind::GarageDoor | OpeningKind::CasedOpening) {
            continue;
        }
        if op.sill.abs() > 1e-9 {
            continue;
        }

        let half = (op.width + ROUGH_ALLOWANCE) / 2.0;
        let u0 = (op.offset - half).clamp(0.0, len);
        let u1 = (op.offset + half).clamp(0.0, len);
        if u1 <= u0 || !u0.is_finite() || !u1.is_finite() {
            continue;
        }

        let v1 = op.sill + op.height + ROUGH_ALLOWANCE - base;
        if !v1.is_finite() || v1 <= 0.0 {
            continue;
        }

        cuts.push((u0, u1, v1));
    }
    cuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut merged: Vec<(f64, f64, f64)> = Vec::new();
    for cut in cuts {
        if let Some(last) = merged.last_mut() {
            if cut.0 <= last.1 + 1e-9 {
                last.1 = last.1.max(cut.1);
                last.2 = last.2.max(cut.2);
                continue;
            }
        }
        merged.push(cut);
    }
    merged
}

fn outer_with_bottom_door_notches(outer: &[Vec2], len: f64, notches: &[(f64, f64, f64)]) -> Vec<Vec2> {
    if notches.is_empty() {
        return outer.to_vec();
    }

    let mut ring = Vec::new();
    ring.push(Vec2::new(0.0, 0.0));
    for &(u0, u1, v1) in notches {
        let left = u0.clamp(0.0, len);
        let right = u1.clamp(0.0, len);
        if right <= left + 1e-9 {
            continue;
        }
        if ring.last().is_some_and(|p| (p.x - left).abs() > 1e-9 || (p.y - 0.0).abs() > 1e-9) {
            ring.push(Vec2::new(left, 0.0));
        }
        ring.push(Vec2::new(left, v1));
        ring.push(Vec2::new(right, v1));
        ring.push(Vec2::new(right, 0.0));
    }

    if ring.last().map_or(true, |p| (p.x - len).abs() > 1e-9 || p.y.abs() > 1e-9) {
        ring.push(Vec2::new(len, 0.0));
    }

    for p in outer.iter().skip(1) {
        if ring.last().is_some_and(|last| (last.x - p.x).abs() < 1e-9 && (last.y - p.y).abs() < 1e-9) {
            continue;
        }
        ring.push(*p);
    }

    while ring.len() >= 2 && ring[0] == ring[ring.len() - 1] {
        ring.pop();
    }

    ring
}

/// Compute the elevation profile of a wall, clipped by a roof and cut by openings.
///
/// Returns `None` if the profile is degenerate.
pub fn elevation_profile(
    shape: &WallShape,
    wall: &Wall,
    base: f64,
    top: f64,
    clip: Option<&RoofClip>,
    roof_top: Option<&[Vec2]>,
    openings: &[&Opening],
) -> Option<WallProfile> {
    if !base.is_finite() || !top.is_finite() || top <= base {
        return None;
    }

    // Wall length in plan
    let dx = wall.end.x - wall.start.x;
    let dy = wall.end.y - wall.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }

    // Determine if the wall has a sloped top due to a roof clip
    let mut has_slope = false;
    let mut max_v = top - base;
    let mut v0 = max_v;
    let mut v1 = max_v;

    if let Some(c) = clip {
        // Calculate roof height at the wall's ends (u=0 and u=len)
        // The roof height depends on the perpendicular distance from the base edge.
        // We need the wall's direction vector and normal.
        let dir = Vec2::new(dx / len, dy / len);
        let normal = Vec2::new(-dir.y, dir.x); // Left normal

        // The wall's outer face is offset by thickness/2 along the normal?
        // Actually, the profile is in wall-local coords (u along centre, v up).
        // The roof clip is in world coords. We need to map (u, v) to world (x, y, z) to check intersection?
        // No, we compute the roof height at the wall's position.

        // The roof plane is defined by base_edge (x0,y0)-(x1,y1) and slope.
        // Height z = eave_z + slope * dist(p, line(base_edge)).

        // We need to find the intersection of the roof plane with the wall's vertical faces.
        // The wall's top edge in the profile is determined by the roof height at the wall's outer face.

        // Let's find the roof height at the wall's start and end points (on the outer face).
        // The outer face is offset from the centre line by thickness/2.
        // Which side is the outer face? It depends on the wall's orientation relative to the room.
        // However, `RoofClip` is provided by the caller, who knows which wall face is the eave.
        // We assume the `base_edge` provided is the line on the wall's *outer* face.

        // We calculate the roof height at u=0 and u=len on the outer face line.
        // The outer face line is parallel to the centre line, offset by thickness/2.
        // But wait, the `base_edge` in `RoofClip` is the *roof's* base edge.
        // If the roof clips the wall, the wall's top follows the roof plane.

        // We compute the roof height at the wall's two ends (projected onto the roof plane).
        // The wall's top in the profile (v) is the roof height minus the level elevation.

        let p0_world = Vec2::new(wall.start.x, wall.start.y);
        let p1_world = Vec2::new(wall.end.x, wall.end.y);

        // Distance from point to line segment (base_edge)
        let dist0 = point_to_line_dist(p0_world, Vec2::new(c.base_edge[0], c.base_edge[1]), Vec2::new(c.base_edge[2], c.base_edge[3]));
        let dist1 = point_to_line_dist(p1_world, Vec2::new(c.base_edge[0], c.base_edge[1]), Vec2::new(c.base_edge[2], c.base_edge[3]));

        let overhang = if c.overhang.is_finite() && c.overhang >= 0.0 { c.overhang } else { 0.0 };
        let z0 = c.eave_z + c.slope * (dist0 - overhang);
        let z1 = c.eave_z + c.slope * (dist1 - overhang);

        v0 = z0 - base;
        v1 = z1 - base;

        if v0.is_finite() && v1.is_finite() {
            // If the roof is lower than the wall top at either end, it clips.
            // If the roof height varies along the wall (perpendicular/angled wall),
            // the wall top follows the roof line even when the roof is above the
            // nominal wall height (gable wall).
            if v0 < max_v || v1 < max_v || (v0 - v1).abs() > 1e-9 {
                has_slope = true;
                max_v = v0.max(v1);
            }
        }
    }

    // Build outer profile
    let outer = if let Some(pts) = roof_top.filter(|pts| pts.len() >= 2 && pts.iter().all(|p| p.x.is_finite() && p.y.is_finite())) {
        // Gable wall: bottom left→right, then roof-top vertices right→left (CCW ring).
        // Add a small offset to the top vertices so the wall extends slightly above
        // the roof underside, ensuring the wall is probeable at the ridge.
        let mut o = vec![Vec2::new(0.0, 0.0), Vec2::new(len, 0.0)];
        for p in pts.iter().rev() {
            o.push(Vec2::new(p.x, p.y + GABLE_TOP_OFFSET));
        }
        o
    } else if has_slope {
        vec![Vec2::new(0.0, 0.0), Vec2::new(len, 0.0), Vec2::new(len, v1), Vec2::new(0.0, v0)]
    } else {
        vec![Vec2::new(0.0, 0.0), Vec2::new(len, 0.0), Vec2::new(len, max_v), Vec2::new(0.0, max_v)]
    };

    let notches = bottom_door_notches(openings, base, len);
    let outer = if notches.is_empty() { outer } else { outer_with_bottom_door_notches(&outer, len, &notches) };
    let door_holes: Vec<&Opening> = openings
        .iter()
        .copied()
        .filter(|op| !matches!(op.kind, OpeningKind::Door | OpeningKind::GarageDoor | OpeningKind::CasedOpening) || op.sill.abs() > 1e-9)
        .collect();
    let holes = opening_holes(&door_holes, base, len, max_v, &outer);

    Some(WallProfile { outer, holes })
}

/// Extrude a wall profile into a 3D mesh.
pub fn extrude_wall_profile(shape: &WallShape, profile: WallProfile, base: f64, material: &str) -> Option<Mesh> {
    if profile.outer.len() < 3 {
        return None;
    }

    // Derive the wall frame from the shape: centre line from `shape.centre`,
    // thickness from the solved polygon.
    let dx = shape.centre.1.x - shape.centre.0.x;
    let dy = shape.centre.1.y - shape.centre.0.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let thickness = ((shape.polygon[3].x - shape.polygon[0].x) * nx + (shape.polygon[3].y - shape.polygon[0].y) * ny).abs();
    if thickness < 1e-12 {
        return None;
    }
    let wall = Wall { start: shape.centre.0, end: shape.centre.1, thickness: thickness, ..Default::default() };

    // Fast path only for a true axis-aligned rectangle: 4 points, both bottom vertices at
    // v = 0, and both top vertices at the same v.  Any other shape (sloped quad, pentagon,
    // …) goes through the general `profile_prism` path.
    let is_rect = profile.outer.len() == 4
        && profile.outer[0].y.abs() < 1e-9
        && profile.outer[1].y.abs() < 1e-9
        && (profile.outer[2].y - profile.outer[3].y).abs() < 1e-9;

    if is_rect && profile.holes.is_empty() {
        let height = profile.outer[1].y;
        let mut mesh = Mesh::new(material, shape.id);
        mesh.level_elevation = base;
        mesh.push_prism(&shape.polygon, base, base + height);
        (mesh.triangle_count() > 0).then(|| mesh)
    } else {
        profile_prism(&profile.outer, &profile.holes, &wall, base, material, shape.id)
    }
}

/// The wall's top polyline in wall-local coords `(u, v)`, `u` from `wall.start`
/// along `wall.end - wall.start`, `v = z - base`. `None` when no roof plane covers it.
pub fn roof_top_polyline(model: &ArchModel, wall: &Wall, base: f64, top: f64) -> Option<Vec<Vec2>> {
    if model.roofs.is_empty() {
        return None;
    }
    let fp = cadcraft_arch::roof::outer_footprint(model, wall.base_level)?;
    let edges: Vec<_> = fp.edges.iter().filter(|e| e.wall == wall.id).collect();
    if edges.is_empty() {
        return None;
    }

    let dx = wall.end.x - wall.start.x;
    let dy = wall.end.y - wall.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 || !len.is_finite() {
        return None;
    }
    let dir = Vec2::new(dx / len, dy / len);

    // Midpoint of the footprint edges: average of all `a`/`b` points.
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for e in &edges {
        sum_x += e.a.x + e.b.x;
        sum_y += e.a.y + e.b.y;
    }
    let edge_mid = Vec2::new(sum_x / (edges.len() as f64 * 2.0), sum_y / (edges.len() as f64 * 2.0));
    let wall_mid = Vec2::new((wall.start.x + wall.end.x) / 2.0, (wall.start.y + wall.end.y) / 2.0);
    let mut out_vec = edge_mid - wall_mid;
    let mut out_len = (out_vec.x * out_vec.x + out_vec.y * out_vec.y).sqrt();
    if out_len < 1e-9 {
        let mut roof_center = Vec2::ZERO;
        let mut count = 0usize;
        for roof in &model.roofs {
            if roof.level != wall.base_level {
                continue;
            }
            for plane in &roof.planes {
                for q in &plane.ring {
                    roof_center += *q;
                    count += 1;
                }
            }
        }
        if count == 0 {
            return None;
        }
        let roof_center = roof_center / count as f64;
        out_vec = roof_center - wall_mid;
        out_len = (out_vec.x * out_vec.x + out_vec.y * out_vec.y).sqrt();
        if out_len < 1e-9 {
            return None;
        }
    }
    let out = Vec2::new(out_vec.x / out_len, out_vec.y / out_len);

    // Candidate `u` values: 0, len, and each roof plane ring vertex projected.
    let mut us: Vec<f64> = vec![0.0, len];
    for roof in &model.roofs {
        if roof.level != wall.base_level {
            continue;
        }
        for plane in &roof.planes {
            for q in &plane.ring {
                let u = (*q - wall.start).dot(dir);
                let u = u.clamp(0.0, len);
                if u.is_finite() {
                    us.push(u);
                }
            }
        }
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut unique_us: Vec<f64> = Vec::new();
    for &u in &us {
        if unique_us.last().map_or(true, |&last| (u - last).abs() >= 1e-9) {
            unique_us.push(u);
        }
    }

    // Sample both wall faces. The roof may cover either side of a gable wall, but
    // the overhang band itself must not be treated as roof cover.
    let mut points: Vec<Vec2> = Vec::new();
    let top_v = top - base;
    for &u in &unique_us {
        let mut z_best = f64::NEG_INFINITY;
        let mut covered = false;
        for side in [-1.0, 1.0] {
            let p = wall.start + dir * u + out * (wall.thickness / 2.0) * side;
            for roof in &model.roofs {
                if roof.level != wall.base_level {
                    continue;
                }
                for plane in &roof.planes {
                    let a = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
                    let b = Vec2::new(plane.base_edge[2], plane.base_edge[3]);
                    let edge_len = ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
                    if edge_len < 1e-12 || plane.ring.len() < 3 {
                        continue;
                    }
                    let mut is_covered = point_in_polygon(&plane.ring, p);
                    if !is_covered {
                        let mut cx = 0.0;
                        let mut cy = 0.0;
                        for q in &plane.ring {
                            cx += q.x;
                            cy += q.y;
                        }
                        let c = Vec2::new(cx / plane.ring.len() as f64, cy / plane.ring.len() as f64);
                        let to_c = c - p;
                        let to_c_len = (to_c.x * to_c.x + to_c.y * to_c.y).sqrt();
                        if to_c_len >= 1e-12 {
                            let nudged = p + to_c / to_c_len * 1e-6;
                            is_covered = point_in_polygon(&plane.ring, nudged);
                        }
                    }
                    if !is_covered {
                        is_covered = plane.ring.windows(2).any(|w| {
                            let a = w[0];
                            let b = w[1];
                            let dx = b.x - a.x;
                            let dy = b.y - a.y;
                            let len_sq = dx * dx + dy * dy;
                            if len_sq < 1e-12 {
                                return false;
                            }
                            let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
                            let t = t.clamp(0.0, 1.0);
                            let closest = Vec2::new(a.x + t * dx, a.y + t * dy);
                            let d2 = (p.x - closest.x) * (p.x - closest.x) + (p.y - closest.y) * (p.y - closest.y);
                            d2 <= 1e-12
                        }) || (plane.ring.len() >= 2 && {
                            let a = plane.ring[0];
                            let b = plane.ring[plane.ring.len() - 1];
                            let dx = b.x - a.x;
                            let dy = b.y - a.y;
                            let len_sq = dx * dx + dy * dy;
                            if len_sq < 1e-12 {
                                false
                            } else {
                                let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
                                let t = t.clamp(0.0, 1.0);
                                let closest = Vec2::new(a.x + t * dx, a.y + t * dy);
                                let d2 = (p.x - closest.x) * (p.x - closest.x) + (p.y - closest.y) * (p.y - closest.y);
                                d2 <= 1e-12
                            }
                        });
                    }
                    if is_covered {
                        let dist = line_dist_to_base_edge(plane, p);
                        let overhang = if plane.overhang.is_finite() && plane.overhang >= 0.0 { plane.overhang } else { 0.0 };
                        let z_plane = underside_z(plane, plane.eave_z, p);
                        // The roof overhang band itself is not roof cover: the eave is `overhang`
                        // inside from the roof ring. Ignore only that band, not the wall thickness.
                        let wall_band = overhang;
                        if !dist.is_finite() || dist <= wall_band + 1e-9 {
                            continue;
                        }
                        if !z_plane.is_finite() || z_plane <= plane.eave_z + 1e-9 {
                            continue;
                        }
                        z_best = z_best.max(z_plane);
                        covered = true;
                    }
                }
            }
        }
        let v = z_best - base;
        if covered && v.is_finite() && v > top_v + 1e-9 {
            points.push(Vec2::new(u, v));
        }
    }

    if points.is_empty() {
        return None;
    }

    // Sort by `u`, then drop collinear points.
    points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    let mut result = Vec::new();
    for p in &points {
        if result.len() >= 2 {
            let a: Vec2 = result[result.len() - 2];
            let b: Vec2 = result[result.len() - 1];
            let cross = (b.x - a.x) * (p.y - b.y) - (b.y - a.y) * (p.x - b.x);
            if cross.abs() < 1e-9 {
                continue;
            }
        }
        result.push(*p);
    }

    // Return `None` if the top is unchanged (eave wall).
    let top_v = top - base;
    if result.iter().any(|p| (p.y - top_v).abs() > 1e-9) { Some(result) } else { None }
}

/// Extrude a profile into a closed solid: build the prism in profile space
/// (`u`, `v`, thickness axis `t` in {0,1}) with `Mesh::push_prism_with_holes`,
/// then map each vertex to world: `p = wall.start + u*dir + (t-0.5)*thickness*perp`,
/// `z = base + v`. The map has determinant -1, so every triangle's winding is
/// reversed after the remap. Normals are accumulated per vertex from the world
/// face normals and normalised (zero -> [0,0,1]); UVs are world box mapping
/// `[x, z]`. Returns `None` when the prism has no triangles.
pub(crate) fn profile_prism(outer: &[Vec2], holes: &[Vec<Vec2>], wall: &Wall, base: f64, material: &str, id: ArchId) -> Option<Mesh> {
    let dx = wall.end.x - wall.start.x;
    let dy = wall.end.y - wall.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 || !len.is_finite() {
        return None;
    }
    if !wall.thickness.is_finite() || wall.thickness <= 0.0 {
        return None;
    }
    if !base.is_finite() {
        return None;
    }

    let dir = Vec2::new(dx / len, dy / len);
    let perp = Vec2::new(-dir.y, dir.x);

    // Drop holes that touch or cross the outer boundary (any vertex within
    // HOLE_INSET of an outer edge). Such holes degenerate the prism and would
    // add spurious triangles; a boundary-touching hole is equivalent to no hole.
    let filtered_holes: Vec<Vec<Vec2>> = holes
        .iter()
        .filter_map(|h| {
            if h.len() < 3 {
                return None;
            }
            if !h.iter().all(|p| p.is_finite()) {
                return None;
            }
            let mut all_inset = true;
            'vertex: for p in h {
                for i in 0..outer.len() {
                    let a = &outer[i];
                    let b = &outer[(i + 1) % outer.len()];
                    let edge_len = ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
                    if edge_len < 1e-12 {
                        continue;
                    }
                    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    if cross <= HOLE_INSET * edge_len {
                        all_inset = false;
                        break 'vertex;
                    }
                }
            }
            if all_inset { Some(h.clone()) } else { None }
        })
        .collect();

    let mut mesh = Mesh::new(material, id);
    mesh.level_elevation = base;
    mesh.push_prism_with_holes(&outer, &filtered_holes, 0.0, 1.0);

    if mesh.indices.is_empty() {
        return None;
    }

    // Transform positions to world space.
    for i in 0..mesh.positions.len() {
        let u = mesh.positions[i][0] as f64;
        let v = mesh.positions[i][1] as f64;
        let t = mesh.positions[i][2] as f64;
        let x = wall.start.x + u * dir.x + (t - 0.5) * wall.thickness * perp.x;
        let y = wall.start.y + u * dir.y + (t - 0.5) * wall.thickness * perp.y;
        let z = base + v;
        mesh.positions[i] = [x as f32, y as f32, z as f32];
    }

    // Reverse all triangle windings (the world map has negative determinant).
    for i in (0..mesh.indices.len()).step_by(3) {
        let tmp = mesh.indices[i + 1];
        mesh.indices[i + 1] = mesh.indices[i + 2];
        mesh.indices[i + 2] = tmp;
    }

    // Accumulate and normalise per-vertex normals from world-space faces.
    let n = mesh.positions.len();
    let mut normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; n];
    for i in (0..mesh.indices.len()).step_by(3) {
        let a = mesh.indices[i] as usize;
        let b = mesh.indices[i + 1] as usize;
        let c = mesh.indices[i + 2] as usize;
        let pa = mesh.positions[a];
        let pb = mesh.positions[b];
        let pc = mesh.positions[c];
        let nx = (pb[1] - pa[1]) * (pc[2] - pa[2]) - (pb[2] - pa[2]) * (pc[1] - pa[1]);
        let ny = (pb[2] - pa[2]) * (pc[0] - pa[0]) - (pb[0] - pa[0]) * (pc[2] - pa[2]);
        let nz = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
        normals[a][0] += nx;
        normals[a][1] += ny;
        normals[a][2] += nz;
        normals[b][0] += nx;
        normals[b][1] += ny;
        normals[b][2] += nz;
        normals[c][0] += nx;
        normals[c][1] += ny;
        normals[c][2] += nz;
    }
    for i in 0..n {
        let l = (normals[i][0].powi(2) + normals[i][1].powi(2) + normals[i][2].powi(2)).sqrt();
        if l > 1e-12 {
            normals[i] = [normals[i][0] / l, normals[i][1] / l, normals[i][2] / l];
        } else {
            normals[i] = [0.0, 0.0, 1.0];
        }
    }
    mesh.normals = normals;

    // World box UVs: [x, z].
    for i in 0..n {
        mesh.uvs[i] = [mesh.positions[i][0], mesh.positions[i][2]];
    }

    (mesh.triangle_count() > 0).then(|| mesh)
}

/// Extrude a profile along the wall's centre-line frame (used when the top follows a roof).
///
/// Maps each profile vertex directly onto the wall's own centre line via
/// [`profile_prism`], making it suitable for walls whose top follows a roof plane
/// with more than four profile points.
pub fn extrude_profile_slab(shape: &WallShape, profile: &WallProfile, base: f64, wall: &Wall, material: &str) -> Option<Mesh> {
    profile_prism(&profile.outer, &profile.holes, wall, base, material, shape.id)
}

fn line_dist_to_base_edge(plane: &cadcraft_doc::arch::RoofPlane, p: Vec2) -> f64 {
    let a = Vec2::new(plane.base_edge[0], plane.base_edge[1]);
    let b = Vec2::new(plane.base_edge[2], plane.base_edge[3]);
    let ab = b - a;
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq < 1e-12 || !len_sq.is_finite() {
        return 0.0;
    }
    let len = len_sq.sqrt();
    let dir = Vec2::new(ab.x / len, ab.y / len);
    let left = Vec2::new(-dir.y, dir.x);
    let right = Vec2::new(dir.y, -dir.x);
    let centroid = plane.ring.iter().filter(|q| q.is_finite()).copied().reduce(|acc, q| acc + q).unwrap_or(Vec2::ZERO);
    let count = plane.ring.iter().filter(|q| q.is_finite()).count();
    let centroid = if count > 0 { centroid / count as f64 } else { a };
    let normal_xy = if (centroid - a).dot(left) >= 0.0 { left } else { right };
    let v = p - a;
    v.x * normal_xy.x + v.y * normal_xy.y
}

fn point_to_line_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
        return ((p.x - a.x) * (p.x - a.x) + (p.y - a.y) * (p.y - a.y)).sqrt();
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = a.x + t * dx;
    let proj_y = a.y + t * dy;
    ((p.x - proj_x) * (p.x - proj_x) + (p.y - proj_y) * (p.y - proj_y)).sqrt()
}

fn clip_polygon(subject: &[Vec2], clip: &[Vec2]) -> Vec<Vec2> {
    // Sutherland-Hodgman
    let mut output = subject.to_vec();
    for i in 0..clip.len() {
        if output.is_empty() {
            break;
        }
        let mut input = output;
        output = Vec::new();
        let a = clip[i];
        let b = clip[(i + 1) % clip.len()];

        for j in 0..input.len() {
            let p = input[j];
            let q = input[(j + 1) % input.len()];

            let inside_p = is_inside(p, a, b);
            let inside_q = is_inside(q, a, b);

            if inside_p {
                output.push(p);
                if !inside_q {
                    output.push(intersect(p, q, a, b));
                }
            } else if inside_q {
                output.push(intersect(p, q, a, b));
            }
        }
    }
    output
}

fn is_inside(p: Vec2, a: Vec2, b: Vec2) -> bool {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= 0.0
}

fn intersect(p1: Vec2, p2: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let x1 = p1.x;
    let y1 = p1.y;
    let x2 = p2.x;
    let y2 = p2.y;
    let x3 = a.x;
    let y3 = a.y;
    let x4 = b.x;
    let y4 = b.y;

    let den = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if den.abs() < 1e-12 {
        return p1;
    }

    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / den;
    Vec2::new(x1 + t * (x2 - x1), y1 + t * (y2 - y1))
}

fn polygon_area(pts: &[Vec2]) -> f64 {
    let mut area = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        area += pts[i].x * pts[j].y - pts[j].x * pts[i].y;
    }
    area.abs() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;
    use cadcraft_doc::arch::{ArchId, Level, Opening, Roof, RoofPlane, Wall};

    fn rect_outer() -> Vec<Vec2> {
        vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)]
    }

    #[test]
    fn rough_opening_hole_uses_the_allowance() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert_eq!(holes.len(), 1);
        let h = &holes[0];
        let u_min = h.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let u_max = h.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let v_min = h.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let v_max = h.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        assert!((u_max - u_min - 1.02).abs() < 1e-9, "width = {}", u_max - u_min);
        assert!((v_min - 0.9).abs() < 1e-9, "v_min = {}", v_min);
        assert!((v_max - 2.22).abs() < 1e-9, "v_max = {}", v_max);
    }

    #[test]
    fn holes_are_clockwise() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert_eq!(holes.len(), 1);
        let h = &holes[0];
        let mut area = 0.0f64;
        for i in 0..h.len() {
            let j = (i + 1) % h.len();
            area += h[i].x * h[j].y - h[j].x * h[i].y;
        }
        assert!(area < 0.0, "shoelace area should be negative (clockwise), got {area}");
    }

    #[test]
    fn hole_touching_the_profile_edge_is_dropped() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.5, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "rough head 2.52 > 2.4 should be dropped, got {} holes", holes.len());
    }

    #[test]
    fn hole_at_the_wall_end_is_dropped() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 0.05, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "rough rect crossing u=0 should be dropped, got {} holes", holes.len());
    }

    #[test]
    fn no_openings_no_holes() {
        let outer = rect_outer();
        let holes = opening_holes(&[], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty());
    }

    fn gable_house() -> ArchModel {
        let mut model = ArchModel::default();
        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
        let _ = model.push_wall(Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        });
        let _ = model.push_wall(Wall {
            start: Vec2::new(10.0, 0.0),
            end: Vec2::new(10.0, 8.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        });
        let _ = model.push_wall(Wall {
            start: Vec2::new(10.0, 8.0),
            end: Vec2::new(0.0, 8.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        });
        let _ = model.push_wall(Wall {
            start: Vec2::new(0.0, 8.0),
            end: Vec2::new(0.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        });
        let mut roof = Roof::default();
        roof.id = ArchId(100);
        roof.level = 0;
        let south = RoofPlane {
            base_edge: [-0.1, -0.1, 10.1, -0.1],
            ring: vec![Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(10.1, 4.0), Vec2::new(-0.1, 4.0)],
            slope: 0.5,
            eave_z: 2.4,
            thickness: 0.25,
            ..Default::default()
        };
        let north = RoofPlane {
            base_edge: [-0.1, 8.1, 10.1, 8.1],
            ring: vec![Vec2::new(-0.1, 8.1), Vec2::new(-0.1, 4.0), Vec2::new(10.1, 4.0), Vec2::new(10.1, 8.1)],
            slope: 0.5,
            eave_z: 2.4,
            thickness: 0.25,
            ..Default::default()
        };
        roof.planes.push(south);
        roof.planes.push(north);
        model.roofs.push(roof);
        model
    }

    #[test]
    fn roof_top_polyline_rises_to_the_ridge() {
        let model = gable_house();
        let wall = &model.walls[3]; // west wall (0,8)->(0,0)
        assert!(roof_top_polyline(&model, wall, 0.0, 2.4).is_some());
        let poly = roof_top_polyline(&model, wall, 0.0, 2.4).unwrap();
        assert!(poly.iter().any(|p| (p.x - 4.0).abs() < 1e-3 && (p.y - 4.45).abs() < 1e-3));
        assert!((poly.first().unwrap().y - 2.45).abs() < 1e-3);
        assert!((poly.last().unwrap().y - 2.45).abs() < 1e-3);
        for i in 1..poly.len() {
            assert!(poly[i].x >= poly[i - 1].x);
        }
    }

    #[test]
    fn roof_top_polyline_is_none_for_an_eave_wall() {
        let model = gable_house();
        let wall = &model.walls[0]; // south wall (0,0)->(10,0)
        assert!(roof_top_polyline(&model, wall, 0.0, 2.4).is_none());
    }

    #[test]
    fn roof_top_polyline_is_none_without_a_roof() {
        let mut model = gable_house();
        model.roofs.clear();
        for wall in &model.walls {
            assert!(roof_top_polyline(&model, wall, 0.0, 2.4).is_none());
        }
    }

    #[test]
    fn roof_top_polyline_zero_length_wall_returns_none() {
        let model = gable_house();
        let wall = Wall {
            start: Vec2::new(5.0, 4.0),
            end: Vec2::new(5.0, 4.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };
        assert!(roof_top_polyline(&model, &wall, 0.0, 2.4).is_none());
    }

    #[test]
    fn roof_top_polyline_ignores_roofs_at_other_levels() {
        let mut model = gable_house();
        // Add a second level and a roof at that level
        model.levels.push(Level { index: 1, elevation: 2.7, ceiling_height: 2.4, ..Default::default() });
        let mut roof2 = Roof::default();
        roof2.id = ArchId(200);
        roof2.level = 1;
        let plane = RoofPlane {
            base_edge: [-0.1, -0.1, 10.1, -0.1],
            ring: vec![Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(10.1, 4.0), Vec2::new(-0.1, 4.0)],
            slope: 0.5,
            eave_z: 2.7,
            thickness: 0.25,
            ..Default::default()
        };
        roof2.planes.push(plane);
        model.roofs.push(roof2);

        // The level-0 walls should still see the level-0 roof (gable wall rises)
        let wall = &model.walls[3]; // west wall
        let poly = roof_top_polyline(&model, wall, 0.0, 2.4);
        assert!(poly.is_some(), "level-0 wall should still see level-0 roof");
        let poly = poly.unwrap();
        assert!(poly.iter().any(|p| (p.x - 4.0).abs() < 1e-3 && (p.y - 4.45).abs() < 1e-3));
    }

    #[test]
    fn roof_top_polyline_handles_roof_with_no_planes() {
        let mut model = gable_house();
        let mut roof = Roof::default();
        roof.id = ArchId(300);
        roof.level = 0;
        model.roofs.push(roof);

        // The gable wall should still see the original roof's planes
        let wall = &model.walls[3]; // west wall
        let poly = roof_top_polyline(&model, wall, 0.0, 2.4);
        assert!(poly.is_some(), "wall should still see planes from the other roof");
    }

    #[test]
    fn roof_top_polyline_min_z_from_multiple_planes() {
        let mut model = gable_house();
        // Add a third plane with a steeper slope covering the same area as the south plane
        let steep = RoofPlane {
            base_edge: [-0.1, -0.1, 10.1, -0.1],
            ring: vec![Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(10.1, 4.0), Vec2::new(-0.1, 4.0)],
            slope: 1.0, // steeper than 0.5
            eave_z: 2.4,
            thickness: 0.25,
            ..Default::default()
        };
        model.roofs[0].planes.push(steep);

        // The south wall is an eave wall; its outer face is the base edge.
        // The minimum z from the two planes at the base edge is still 2.4 (eave_z).
        // So the south wall should still return None.
        let south_wall = &model.walls[0];
        assert!(roof_top_polyline(&model, south_wall, 0.0, 2.4).is_none());

        // The west wall should still rise to the ridge, but now the steeper plane
        // also covers it. At the ridge (y=4), the steeper plane gives z = 2.4 + 1.0*4.1 = 6.5,
        // while the original gives 4.45. The minimum is 4.45.
        let west_wall = &model.walls[3];
        let poly = roof_top_polyline(&model, west_wall, 0.0, 2.4).unwrap();
        // The ridge point should still be at v=4.45 (the minimum of the two planes)
        assert!(poly.iter().any(|p| (p.x - 4.0).abs() < 1e-3 && (p.y - 4.45).abs() < 1e-3));
    }

    #[test]
    fn roof_top_polyline_nan_wall_returns_none() {
        let model = gable_house();
        let wall = Wall {
            start: Vec2::new(f64::NAN, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };
        assert!(roof_top_polyline(&model, &wall, 0.0, 2.4).is_none());
    }

    #[test]
    fn roof_top_polyline_degenerate_base_edge_skipped() {
        let mut model = gable_house();
        // Add a plane with a degenerate base edge (both endpoints the same)
        let degenerate = RoofPlane {
            base_edge: [5.0, 4.0, 5.0, 4.0],
            ring: vec![Vec2::new(4.0, 3.0), Vec2::new(6.0, 3.0), Vec2::new(5.0, 5.0)],
            slope: 0.5,
            eave_z: 2.4,
            thickness: 0.25,
            ..Default::default()
        };
        model.roofs[0].planes.push(degenerate);

        // The gable wall should still work as before
        let wall = &model.walls[3]; // west wall
        let poly = roof_top_polyline(&model, wall, 0.0, 2.4);
        assert!(poly.is_some(), "degenerate plane should be skipped, not break the function");
    }

    #[test]
    fn roof_top_polyline_few_ring_points_skipped() {
        let mut model = gable_house();
        // Add a plane with only 2 ring points
        let few_points = RoofPlane {
            base_edge: [0.0, 0.0, 10.0, 0.0],
            ring: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)],
            slope: 0.5,
            eave_z: 2.4,
            thickness: 0.25,
            ..Default::default()
        };
        model.roofs[0].planes.push(few_points);

        // The gable wall should still work as before
        let wall = &model.walls[3]; // west wall
        let poly = roof_top_polyline(&model, wall, 0.0, 2.4);
        assert!(poly.is_some(), "plane with < 3 ring points should be skipped");
    }

    /// A pentagon profile (5 outer vertices) must go through `extrude_profile_manual`,
    /// not the fast `push_prism` path, and produce geometry that spans the correct z range.
    #[test]
    fn extrude_wall_profile_extrudes_a_pentagon_profile() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(8.0, -0.1), Vec2::new(8.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        let profile = WallProfile {
            outer: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 2.4), Vec2::new(4.0, 4.45), Vec2::new(0.0, 2.4)],
            holes: vec![],
        };

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall");
        assert!(mesh.is_some(), "pentagon profile should produce a mesh");
        let mesh = mesh.unwrap();
        assert!(mesh.triangle_count() > 0, "pentagon mesh must have triangles");

        let b = mesh.bounds();
        assert!((b.max[2] - 4.45).abs() < 1e-3, "max z should be ≈ 4.45, got {}", b.max[2]);
        assert!(b.min[2].abs() < 1e-3, "min z should be ≈ 0.0, got {}", b.min[2]);
    }

    /// `extrude_profile_slab` maps vertices onto the wall centre-line frame;
    /// a vertex at u = 0 on a horizontal wall must land at x ≈ 0.
    #[test]
    fn extrude_profile_slab_extrudes_a_pentagon() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(8.0, -0.1), Vec2::new(8.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        let profile = WallProfile {
            outer: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 2.4), Vec2::new(4.0, 4.45), Vec2::new(0.0, 2.4)],
            holes: vec![],
        };

        // Horizontal wall from (0,0) to (8,0): dir=(1,0), perp=(0,1).
        // For u = 0 vertices: x = wall.start.x + 0*1 + s*0.1*0 = 0.0.
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(8.0, 0.0), thickness: 0.2, height: 4.45, ..Default::default() };

        let mesh = extrude_profile_slab(&shape, &profile, 0.0, &wall, "wall");
        assert!(mesh.is_some(), "pentagon slab should produce a mesh");
        let mesh = mesh.unwrap();
        assert!(mesh.triangle_count() > 0, "pentagon slab mesh must have triangles");

        let b = mesh.bounds();
        assert!((b.max[2] - 4.45).abs() < 1e-3, "max z should be ≈ 4.45, got {}", b.max[2]);
        assert!(b.min[2].abs() < 1e-3, "min z should be ≈ 0.0, got {}", b.min[2]);

        // A vertex at u = 0 on a horizontal wall must be at x ≈ 0 (centre-line frame).
        let has_x_near_zero = mesh.positions.iter().any(|p| (p[0] as f64).abs() < 1e-3);
        assert!(has_x_near_zero, "slab mesh must have a vertex at |x - 0.0| < 1e-3");
    }

    /// A 4-point rectangle profile with no holes must take the fast `push_prism` path,
    /// yielding exactly 12 triangles (the closed-box count for a 4-vertex ring).
    #[test]
    fn rectangle_profile_still_uses_the_prism_path() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(8.0, -0.1), Vec2::new(8.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        let profile = WallProfile { outer: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 2.4), Vec2::new(0.0, 2.4)], holes: vec![] };

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall");
        assert!(mesh.is_some(), "rectangle profile should produce a mesh");
        let mesh = mesh.unwrap();
        assert_eq!(mesh.triangle_count(), 12, "rectangle fast path (push_prism on 4-vertex ring) must yield exactly 12 triangles");
    }

    /// A wall with a window and a door (both openings) must produce a closed
    /// manifold whose triangles wind consistently outward: no directed edge
    /// is repeated and the signed volume is positive.
    #[test]
    fn wall_with_openings_mesh_is_consistently_oriented() {
        let id = ArchId(7);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(10.0, -0.1), Vec2::new(10.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let wall = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };

        let profile = elevation_profile(
            &shape,
            &wall,
            0.0,
            2.4,
            None,
            None,
            &[
                &Opening { offset: 3.0, width: 1.0, height: 2.0, sill: 0.9, ..Default::default() },
                &Opening { offset: 7.0, width: 0.9, height: 2.1, sill: 0.05, ..Default::default() },
            ],
        )
        .unwrap();

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall").unwrap();
        assert!(mesh.triangle_count() > 0, "wall with openings must have triangles");

        // 1) Every directed edge occurs at most once (consistent orientation).
        let mut edges: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
        for tri in mesh.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            for edge in [(a, b), (b, c), (c, a)] {
                *edges.entry(edge).or_insert(0) += 1;
            }
        }
        assert!(edges.iter().all(|(_, &n)| n <= 1), "directed edge repeated: {edges:?}");

        // 2) Signed volume of the closed mesh is positive (outward winding).
        let mut vol = 0.0;
        for tri in mesh.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let pa = mesh.positions[a as usize].map(|v| v as f64);
            let pb = mesh.positions[b as usize].map(|v| v as f64);
            let pc = mesh.positions[c as usize].map(|v| v as f64);
            vol += pa[0] * (pb[1] * pc[2] - pb[2] * pc[1]) + pa[1] * (pb[2] * pc[0] - pb[0] * pc[2]) + pa[2] * (pb[0] * pc[1] - pb[1] * pc[0]);
        }
        vol /= 6.0;
        assert!(vol > 0.0, "signed volume must be > 0, got {vol}");
    }

    /// A room's floor top faces must have +z normals and the ceiling underside
    /// faces -z, computed from the triangle vertices (not stored normals).
    #[test]
    fn room_floor_and_ceiling_meshes_face_outward() {
        let ring = vec![Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(6.0, 5.0), Vec2::new(0.0, 5.0)];

        // Floor slab: extruded from z=0 to z=0.3. Top faces (z=0.3) must be +z.
        let mut floor = Mesh::new("floor", ArchId(100));
        floor.push_prism_with_holes(&ring, &[], 0.0, 0.3);
        assert!(floor.triangle_count() > 0, "floor must have triangles");
        for tri in floor.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let pa = floor.positions[a as usize].map(|v| v as f64);
            let pb = floor.positions[b as usize].map(|v| v as f64);
            let pc = floor.positions[c as usize].map(|v| v as f64);
            // Only horizontal faces (all vertices at the same z) have a ±z normal.
            if (pa[2] - pb[2]).abs() > 1e-9 || (pa[2] - pc[2]).abs() > 1e-9 {
                continue;
            }
            let nx = (pb[1] - pa[1]) * (pc[2] - pa[2]) - (pb[2] - pa[2]) * (pc[1] - pa[1]);
            let nz = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
            let len = (nx * nx + nz * nz).sqrt();
            if len < 1e-12 {
                continue;
            }
            // Horizontal faces: top (z≈0.3) → +z, bottom (z≈0) → -z.
            if pa[2] > 0.15 {
                assert!(nz / len > 0.9, "floor top face normal must be +z: {nz}");
            } else {
                assert!(nz / len < -0.9, "floor bottom face normal must be -z: {nz}");
            }
        }

        // Ceiling slab: extruded from z=2.4 to z=2.7. Underside faces (z≈2.4) must be -z.
        let mut ceil = Mesh::new("ceiling", ArchId(101));
        ceil.push_prism_with_holes(&ring, &[], 2.4, 2.7);
        assert!(ceil.triangle_count() > 0, "ceiling must have triangles");
        for tri in ceil.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let pa = ceil.positions[a as usize].map(|v| v as f64);
            let pb = ceil.positions[b as usize].map(|v| v as f64);
            let pc = ceil.positions[c as usize].map(|v| v as f64);
            // Only horizontal faces (all vertices at the same z) have a ±z normal.
            if (pa[2] - pb[2]).abs() > 1e-9 || (pa[2] - pc[2]).abs() > 1e-9 {
                continue;
            }
            let nx = (pb[1] - pa[1]) * (pc[2] - pa[2]) - (pb[2] - pa[2]) * (pc[1] - pa[1]);
            let nz = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
            let len = (nx * nx + nz * nz).sqrt();
            if len < 1e-12 {
                continue;
            }
            // Underside (z≈2.4) must be -z; top (z≈2.7) must be +z.
            if pa[2] < 2.55 {
                assert!((nz / len) < -0.9, "ceiling underside normal must be -z: {nz}");
            } else {
                assert!((nz / len) > 0.9, "ceiling top normal must be +z: {nz}");
            }
        }
    }

    /// A profile prism with a rough hole must still be a closed manifold and have a
    /// positive (outward) signed volume.
    #[test]
    fn profile_prism_with_a_hole_is_a_closed_manifold() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)];
        // 0.9 x 1.2 rough hole, clockwise.
        let hole = vec![Vec2::new(4.5, 0.5), Vec2::new(4.5, 1.7), Vec2::new(5.4, 1.7), Vec2::new(5.4, 0.5)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[hole.clone()], &wall, 0.0, "wall", ArchId(7)).unwrap();
        assert!(mesh.is_manifold(), "profile prism with a hole must be a closed manifold");

        // Positive signed volume from the indices (outward winding).
        let mut vol = 0.0;
        for tri in mesh.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let pa = mesh.positions[a as usize].map(|v| v as f64);
            let pb = mesh.positions[b as usize].map(|v| v as f64);
            let pc = mesh.positions[c as usize].map(|v| v as f64);
            vol += pa[0] * (pb[1] * pc[2] - pb[2] * pc[1]) + pa[1] * (pb[2] * pc[0] - pb[0] * pc[2]) + pa[2] * (pb[0] * pc[1] - pb[1] * pc[0]);
        }
        vol /= 6.0;
        assert!(vol > 0.0, "signed volume must be > 0, got {vol}");
    }

    /// An opening at exactly the wall start (offset 0) is clipped and dropped.
    #[test]
    fn opening_at_exact_wall_start_is_clipped_and_dropped() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 0.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "opening at exact wall start should be clipped and dropped");
    }

    /// An opening at exactly the wall end (offset = length) is clipped and dropped.
    #[test]
    fn opening_at_exact_wall_end_is_clipped_and_dropped() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 10.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "opening at exact wall end should be clipped and dropped");
    }

    /// A diagonal wall (45 degrees) with an opening produces a correct profile and manifold mesh.
    #[test]
    fn diagonal_wall_with_opening_produces_correct_profile() {
        let id = ArchId(7);
        let polygon = vec![Vec2::new(-0.07, 0.07), Vec2::new(7.0, 7.14), Vec2::new(7.14, 7.0), Vec2::new(0.07, -0.07)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(7.07, 7.07));
        let shape = WallShape::new(id, polygon, centre);
        let wall = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(7.07, 7.07),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };

        let profile = elevation_profile(
            &shape,
            &wall,
            0.0,
            2.4,
            None,
            None,
            &[&Opening { offset: 3.5, width: 0.9, height: 1.2, sill: 0.9, ..Default::default() }],
        )
        .unwrap();

        assert_eq!(profile.outer.len(), 4, "diagonal wall profile should be a rectangle");
        assert_eq!(profile.holes.len(), 1, "diagonal wall should have one hole");

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall").unwrap();
        assert!(mesh.triangle_count() > 0, "diagonal wall with opening must have triangles");
        assert!(mesh.is_manifold(), "diagonal wall with opening must be a closed manifold");
    }

    /// elevation_profile with a roof clip where the wall is perpendicular to the base edge.
    #[test]
    fn elevation_profile_with_perpendicular_roof_clip() {
        let id = ArchId(1);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(4.0, -0.1), Vec2::new(4.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let wall = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(0.0, 4.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };
        let clip = RoofClip { base_edge: [0.0, 0.0, 10.0, 0.0], slope: 0.5, eave_z: 2.4, overhang: 0.0 };
        let profile = elevation_profile(&shape, &wall, 0.0, 2.4, Some(&clip), None, &[]).unwrap();
        assert_eq!(profile.outer.len(), 4, "perpendicular wall with roof clip should have 4 vertices");
        assert!((profile.outer[0].y - 0.0).abs() < 1e-9);
        assert!((profile.outer[1].y - 0.0).abs() < 1e-9);
        assert!((profile.outer[2].y - 4.4).abs() < 1e-9, "top at wall end should be 4.4");
        assert!((profile.outer[3].y - 2.4).abs() < 1e-9, "top at wall start should be 2.4");
    }

    /// A pentagon profile must land on the wall's centre-line frame: for a horizontal
    /// wall the x extent is the wall length and the thickness appears in y.
    #[test]
    fn profile_prism_maps_the_profile_onto_the_wall_frame() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 2.4), Vec2::new(4.0, 4.45), Vec2::new(0.0, 2.4)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(8.0, 0.0), thickness: 0.2, height: 4.45, ..Default::default() };
        let mesh = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(7)).unwrap();
        let b = mesh.bounds();
        assert!((b.min[2] - 0.0).abs() < 1e-3, "z min = {}", b.min[2]);
        assert!((b.max[2] - 4.45).abs() < 1e-3, "z max = {}", b.max[2]);
        assert!(b.min[0] >= -0.1 && b.max[0] <= 8.1, "x bounds out of range: {b:?}");
    }

    /// A hole that spans the full height touches the boundary and is dropped, so the
    /// triangle count matches the hole-free case.
    #[test]
    fn profile_prism_drops_a_hole_that_touches_the_boundary() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)];
        // Hole spanning the full height (v = 0..2.4), touching the boundary.
        let hole = vec![Vec2::new(4.5, 0.0), Vec2::new(4.5, 2.4), Vec2::new(5.4, 2.4), Vec2::new(5.4, 0.0)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let no_hole = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(7)).unwrap();
        let with_hole = profile_prism(&outer, &[hole], &wall, 0.0, "wall", ArchId(7)).unwrap();
        assert_eq!(
            no_hole.triangle_count(),
            with_hole.triangle_count(),
            "boundary-touching hole should not change triangle count: no_hole={} with_hole={}",
            no_hole.triangle_count(),
            with_hole.triangle_count()
        );
    }

    /// A zero-length wall (start == end) must return None from profile_prism.
    #[test]
    fn profile_prism_returns_none_for_zero_length_wall() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.4), Vec2::new(0.0, 2.4)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(0.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(1));
        assert!(mesh.is_none(), "zero-length wall should return None");
    }

    /// A wall with zero thickness must return None from profile_prism.
    #[test]
    fn profile_prism_returns_none_for_zero_thickness() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.4), Vec2::new(0.0, 2.4)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(4.0, 0.0), thickness: 0.0, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(1));
        assert!(mesh.is_none(), "zero-thickness wall should return None");
    }

    /// A non-finite base must return None from profile_prism.
    #[test]
    fn profile_prism_returns_none_for_nan_base() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.4), Vec2::new(0.0, 2.4)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(4.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[], &wall, f64::NAN, "wall", ArchId(1));
        assert!(mesh.is_none(), "NaN base should return None");
    }

    /// A vertical wall (going up in Y) must produce a mesh with correct bounds.
    #[test]
    fn profile_prism_handles_vertical_wall() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.4), Vec2::new(0.0, 2.4)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(0.0, 4.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(1));
        assert!(mesh.is_some(), "vertical wall should produce a mesh");
        let mesh = mesh.unwrap();
        let b = mesh.bounds();
        // Wall goes from (0,0) to (0,4), thickness 0.2 in X direction
        assert!(b.min[1] >= -0.1 && b.max[1] <= 4.1, "y bounds out of range: {b:?}");
        assert!(b.min[2] >= -0.1 && b.max[2] <= 2.5, "z bounds out of range: {b:?}");
    }

    /// A hole with fewer than 3 vertices must be dropped.
    #[test]
    fn profile_prism_drops_holes_with_fewer_than_three_vertices() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)];
        let hole = vec![Vec2::new(4.0, 0.5), Vec2::new(6.0, 0.5)]; // Only 2 vertices
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[hole], &wall, 0.0, "wall", ArchId(1));
        assert!(mesh.is_some(), "should produce a mesh");
        let mesh = mesh.unwrap();
        // Same triangle count as no holes (the 2-vertex hole was dropped)
        let no_holes = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(2)).unwrap();
        assert_eq!(mesh.triangle_count(), no_holes.triangle_count(), "2-vertex hole should be dropped");
    }

    /// A hole with a NaN vertex must be dropped.
    #[test]
    fn profile_prism_drops_holes_with_nan_vertices() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)];
        let hole = vec![Vec2::new(4.0, 0.5), Vec2::new(6.0, 0.5), Vec2::new(6.0, 1.5), Vec2::new(f64::NAN, 1.5)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[hole], &wall, 0.0, "wall", ArchId(1));
        assert!(mesh.is_some(), "should produce a mesh");
        let mesh = mesh.unwrap();
        let no_holes = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(2)).unwrap();
        assert_eq!(mesh.triangle_count(), no_holes.triangle_count(), "NaN hole should be dropped");
    }

    /// Multiple holes must all be extruded correctly.
    #[test]
    fn profile_prism_handles_multiple_holes() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)];
        let hole1 = vec![Vec2::new(1.0, 0.5), Vec2::new(1.0, 1.5), Vec2::new(2.0, 1.5), Vec2::new(2.0, 0.5)];
        let hole2 = vec![Vec2::new(8.0, 0.5), Vec2::new(8.0, 1.5), Vec2::new(9.0, 1.5), Vec2::new(9.0, 0.5)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, height: 2.4, ..Default::default() };
        let mesh = profile_prism(&outer, &[hole1, hole2], &wall, 0.0, "wall", ArchId(1));
        assert!(mesh.is_some(), "should produce a mesh");
        let mesh = mesh.unwrap();
        assert!(mesh.is_manifold(), "multi-hole prism must be a closed manifold");
        // More triangles than no holes
        let no_holes = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(2)).unwrap();
        assert!(mesh.triangle_count() > no_holes.triangle_count(), "two holes should add triangles");
    }

    /// An opening with NaN in any dimension field must be skipped entirely.
    #[test]
    fn opening_with_nan_dimensions_is_skipped() {
        let outer = rect_outer();
        let op = Opening { width: f64::NAN, height: 1.2, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "NaN width should skip the opening");

        let op2 = Opening { width: 0.9, height: f64::NAN, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes2 = opening_holes(&[&op2], 0.0, 10.0, 2.4, &outer);
        assert!(holes2.is_empty(), "NaN height should skip the opening");

        let op3 = Opening { width: 0.9, height: 1.2, sill: f64::NAN, offset: 4.0, ..Default::default() };
        let holes3 = opening_holes(&[&op3], 0.0, 10.0, 2.4, &outer);
        assert!(holes3.is_empty(), "NaN sill should skip the opening");

        let op4 = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: f64::NAN, ..Default::default() };
        let holes4 = opening_holes(&[&op4], 0.0, 10.0, 2.4, &outer);
        assert!(holes4.is_empty(), "NaN offset should skip the opening");
    }

    /// An opening with infinite dimensions must be skipped.
    #[test]
    fn opening_with_infinite_dimensions_is_skipped() {
        let outer = rect_outer();
        let op = Opening { width: f64::INFINITY, height: 1.2, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "infinite width should skip the opening");

        let op2 = Opening { width: 0.9, height: f64::INFINITY, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes2 = opening_holes(&[&op2], 0.0, 10.0, 2.4, &outer);
        assert!(holes2.is_empty(), "infinite height should skip the opening");
    }

    /// A zero-width opening still produces a hole (rough allowance).
    #[test]
    fn zero_width_opening_still_produces_a_hole() {
        let outer = rect_outer();
        let op = Opening { width: 0.0, height: 1.2, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert_eq!(holes.len(), 1, "zero-width opening should still produce a hole (rough allowance)");
    }

    /// A zero-height opening still produces a hole (rough allowance).
    #[test]
    fn zero_height_opening_still_produces_a_hole() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 0.0, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert_eq!(holes.len(), 1, "zero-height opening should still produce a hole (rough allowance)");
    }

    /// Multiple openings produce multiple holes, each independently.
    #[test]
    fn multiple_openings_produce_multiple_holes() {
        let outer = rect_outer();
        let op1 = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 2.0, ..Default::default() };
        let op2 = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 8.0, ..Default::default() };
        let holes = opening_holes(&[&op1, &op2], 0.0, 10.0, 2.4, &outer);
        assert_eq!(holes.len(), 2, "two openings should produce two holes");
        // Both holes should have the correct width
        for h in &holes {
            let u_min = h.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let u_max = h.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            assert!((u_max - u_min - 1.02).abs() < 1e-9, "hole width should be 1.02");
        }
    }

    /// An opening larger than the wall profile is clipped and dropped.
    #[test]
    fn oversized_opening_is_clipped_and_dropped() {
        let outer = rect_outer();
        // Opening wider than the wall (width 15 on a 10m wall)
        let op = Opening { width: 15.0, height: 1.2, sill: 0.9, offset: 5.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "oversized opening clipped to all edges should be dropped");
    }

    /// An opening with a very high sill (above the wall top) produces no hole.
    #[test]
    fn opening_above_wall_top_produces_no_hole() {
        let outer = rect_outer();
        // Sill at 3.0m, wall is only 2.4m tall
        let op = Opening { width: 0.9, height: 1.2, sill: 3.0, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "opening above wall top should produce no hole");
    }

    /// An opening with negative sill (below the wall base) is clipped and dropped.
    #[test]
    fn opening_below_wall_base_is_clipped_and_dropped() {
        let outer = rect_outer();
        // Sill at -0.5m, wall base at 0; after clipping the bottom edge sits
        // on the boundary and the inset check drops it.
        let op = Opening { width: 0.9, height: 1.2, sill: -0.5, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "opening clipped to base should be dropped");
    }

    /// A gable pentagon profile clips a hole to the sloped roofline.
    #[test]
    fn opening_hole_clipped_by_gable_pentagon() {
        // Gable pentagon: bottom-left, bottom-right, right-top, peak, left-top
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(5.0, 4.45), Vec2::new(0.0, 2.4)];
        // Small opening near the peak, fully inside the pentagon with margin.
        let op = Opening { width: 0.5, height: 0.5, sill: 3.5, offset: 5.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 4.45, &outer);
        assert_eq!(holes.len(), 1, "small opening near peak should produce one hole");
        let h = &holes[0];
        // All vertices must be inside the pentagon (v <= roofline at that u)
        for p in h {
            // The roofline at u is: 2.4 + 0.5 * (5.0 - |u - 5.0|) for u in [0,10]
            let roof_v = 2.4 + 0.5 * (5.0 - (p.x - 5.0).abs());
            assert!(p.y <= roof_v + 1e-9, "hole vertex at ({}, {}) above roofline {}", p.x, p.y, roof_v);
        }
    }

    /// A hole clipped to the wall boundary is dropped by the inset check.
    #[test]
    fn hole_clipped_to_wall_boundary_is_dropped() {
        let outer = rect_outer();
        // Opening whose rough rect crosses u=0; after clipping the left edge
        // sits on the boundary and the inset check drops it.
        let op = Opening { width: 0.118, height: 1.2, sill: 0.9, offset: 0.001, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "hole clipped to boundary should be dropped");
    }

    /// A hole just outside the inset boundary is dropped.
    #[test]
    fn hole_just_outside_inset_boundary_is_dropped() {
        let outer = rect_outer();
        // Rough edge at u=0.0005, less than HOLE_INSET (0.001)
        // half = 0.0005 => width = 0.12 - 0.001 = 0.119
        // offset = 0.0005
        let op = Opening { width: 0.119, height: 1.2, sill: 0.9, offset: 0.0005, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "hole just outside inset boundary should be dropped");
    }

    /// A very small opening still produces a hole (rough allowance dominates).
    #[test]
    fn tiny_opening_still_produces_a_hole() {
        let outer = rect_outer();
        // Width 0.0005, height 0.0005; rough opening is 0.1205 x 0.1205, area ~0.0145
        let op = Opening { width: 0.0005, height: 0.0005, sill: 0.9, offset: 4.0, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert_eq!(holes.len(), 1, "tiny opening should still produce a hole (rough allowance)");
    }

    /// A negative offset (opening before the wall start) is clipped and dropped.
    #[test]
    fn opening_with_negative_offset_is_clipped_and_dropped() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: -0.5, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "opening clipped to wall start should be dropped");
    }

    /// An opening past the wall end is clipped and dropped.
    #[test]
    fn opening_past_wall_end_is_clipped_and_dropped() {
        let outer = rect_outer();
        let op = Opening { width: 0.9, height: 1.2, sill: 0.9, offset: 10.5, ..Default::default() };
        let holes = opening_holes(&[&op], 0.0, 10.0, 2.4, &outer);
        assert!(holes.is_empty(), "opening clipped to wall end should be dropped");
    }

    /// A 4-point profile with a sloped top (not a rectangle) must use the profile_prism path,
    /// not the fast push_prism path, and produce correct geometry.
    #[test]
    fn sloped_quad_profile_uses_profile_prism_path() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(8.0, -0.1), Vec2::new(8.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        // Sloped top: left side 2.4, right side 4.45 (not a rectangle)
        let profile = WallProfile { outer: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 4.45), Vec2::new(0.0, 2.4)], holes: vec![] };

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall");
        assert!(mesh.is_some(), "sloped quad should produce a mesh");
        let mesh = mesh.unwrap();
        assert!(mesh.triangle_count() > 0, "sloped quad mesh must have triangles");
        // Not the fast path (which would give exactly 12 triangles)
        let b = mesh.bounds();
        assert!((b.max[2] - 4.45).abs() < 1e-3, "max z should be ≈ 4.45, got {}", b.max[2]);
        assert!(b.min[2].abs() < 1e-3, "min z should be ≈ 0.0, got {}", b.min[2]);
    }

    /// A rectangle profile with holes must use the profile_prism path, not the fast path.
    #[test]
    fn rectangle_profile_with_holes_uses_profile_prism_path() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(10.0, -0.1), Vec2::new(10.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        let profile = WallProfile {
            outer: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 2.4), Vec2::new(0.0, 2.4)],
            holes: vec![vec![Vec2::new(4.5, 0.5), Vec2::new(4.5, 1.7), Vec2::new(5.4, 1.7), Vec2::new(5.4, 0.5)]],
        };

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall");
        assert!(mesh.is_some(), "rectangle with hole should produce a mesh");
        let mesh = mesh.unwrap();
        assert!(mesh.triangle_count() > 12, "hole should add triangles beyond the 12 of the fast path");
    }

    /// A profile with fewer than 3 outer vertices must return None from extrude_wall_profile.
    #[test]
    fn extrude_wall_profile_returns_none_for_too_few_outer_vertices() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(8.0, -0.1), Vec2::new(8.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        let profile = WallProfile { outer: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0)], holes: vec![] };

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall");
        assert!(mesh.is_none(), "profile with 2 vertices should return None");
    }

    /// extrude_profile_slab with a zero-thickness wall must return None.
    #[test]
    fn extrude_profile_slab_returns_none_for_zero_thickness() {
        let id = ArchId(42);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(8.0, -0.1), Vec2::new(8.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);

        let profile = WallProfile { outer: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 2.4), Vec2::new(0.0, 2.4)], holes: vec![] };

        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(8.0, 0.0), thickness: 0.0, height: 2.4, ..Default::default() };
        let mesh = extrude_profile_slab(&shape, &profile, 0.0, &wall, "wall");
        assert!(mesh.is_none(), "zero-thickness wall should return None");
    }

    /// extrude_wall with zero height must return None.
    #[test]
    fn extrude_wall_zero_height_returns_none() {
        let id = ArchId(1);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(4.0, -0.1), Vec2::new(4.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let mesh = extrude_wall(&shape, 0.0, 0.0, "wall");
        assert!(mesh.is_none(), "zero height should return None");
    }

    /// extrude_wall with NaN height must return None.
    #[test]
    fn extrude_wall_nan_height_returns_none() {
        let id = ArchId(1);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(4.0, -0.1), Vec2::new(4.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let mesh = extrude_wall(&shape, f64::NAN, 0.0, "wall");
        assert!(mesh.is_none(), "NaN height should return None");
    }

    /// extrude_wall with non-finite z0 must return None.
    #[test]
    fn extrude_wall_non_finite_z0_returns_none() {
        let id = ArchId(1);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(4.0, -0.1), Vec2::new(4.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let mesh = extrude_wall(&shape, 2.4, f64::NAN, "wall");
        assert!(mesh.is_none(), "NaN z0 should return None");
    }

    /// elevation_profile with no openings and no roof clip must be a simple rectangle.
    #[test]
    fn elevation_profile_no_openings_is_rectangle() {
        let id = ArchId(1);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(10.0, -0.1), Vec2::new(10.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let wall = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };
        let profile = elevation_profile(&shape, &wall, 0.0, 2.4, None, None, &[]).unwrap();
        assert_eq!(profile.outer.len(), 4, "no openings should give a 4-vertex rectangle");
        assert!(profile.holes.is_empty(), "no openings should give no holes");
        // Verify the rectangle dimensions
        assert!((profile.outer[0].x - 0.0).abs() < 1e-9);
        assert!((profile.outer[0].y - 0.0).abs() < 1e-9);
        assert!((profile.outer[1].x - 10.0).abs() < 1e-9);
        assert!((profile.outer[1].y - 0.0).abs() < 1e-9);
        assert!((profile.outer[2].x - 10.0).abs() < 1e-9);
        assert!((profile.outer[2].y - 2.4).abs() < 1e-9);
        assert!((profile.outer[3].x - 0.0).abs() < 1e-9);
        assert!((profile.outer[3].y - 2.4).abs() < 1e-9);
    }

    /// elevation_profile with a roof clip that is above the wall top must not clip.
    #[test]
    fn elevation_profile_with_roof_above_wall_is_rectangle() {
        let id = ArchId(1);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(10.0, -0.1), Vec2::new(10.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let wall = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };
        // Roof with eave_z well above the wall top (2.4)
        let clip = RoofClip { base_edge: [0.0, 0.0, 10.0, 0.0], slope: 0.5, eave_z: 5.0, overhang: 0.0 };
        let profile = elevation_profile(&shape, &wall, 0.0, 2.4, Some(&clip), None, &[]).unwrap();
        // The roof is above the wall, so the profile should still be a rectangle at height 2.4
        assert_eq!(profile.outer.len(), 4, "roof above wall should not change profile");
        assert!((profile.outer[2].y - 2.4).abs() < 1e-9, "top should be at wall height 2.4");
        assert!((profile.outer[3].y - 2.4).abs() < 1e-9, "top should be at wall height 2.4");
    }

    /// opening_holes with two overlapping openings must produce two separate holes.
    #[test]
    fn opening_holes_with_overlapping_openings() {
        let outer = rect_outer();
        // Two openings that overlap in u (offset 4.0 width 1.5 and offset 4.5 width 1.5)
        let op1 = Opening { width: 1.5, height: 1.2, sill: 0.9, offset: 4.0, ..Default::default() };
        let op2 = Opening { width: 1.5, height: 1.2, sill: 0.9, offset: 4.5, ..Default::default() };
        let holes = opening_holes(&[&op1, &op2], 0.0, 10.0, 2.4, &outer);
        // Both openings are fully inside the wall, so both should produce holes
        assert_eq!(holes.len(), 2, "two overlapping openings should produce two holes");
    }

    /// A wall with many openings must still produce a closed manifold mesh.
    #[test]
    fn wall_with_many_openings_is_manifold() {
        let id = ArchId(7);
        let polygon = vec![Vec2::new(0.0, -0.1), Vec2::new(12.0, -0.1), Vec2::new(12.0, 0.1), Vec2::new(0.0, 0.1)];
        let centre = (Vec2::new(0.0, 0.0), Vec2::new(12.0, 0.0));
        let shape = WallShape::new(id, polygon, centre);
        let wall = Wall {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(12.0, 0.0),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };

        // Six evenly spaced openings
        let openings: Vec<Opening> =
            (0..6).map(|i| Opening { offset: 1.0 + i as f64 * 2.0, width: 0.9, height: 1.2, sill: 0.9, ..Default::default() }).collect();
        let opening_refs: Vec<&Opening> = openings.iter().collect();

        let profile = elevation_profile(&shape, &wall, 0.0, 2.4, None, None, &opening_refs).unwrap();
        assert_eq!(profile.holes.len(), 6, "six openings should produce six holes");

        let mesh = extrude_wall_profile(&shape, profile, 0.0, "wall").unwrap();
        assert!(mesh.triangle_count() > 0, "wall with six openings must have triangles");
        assert!(mesh.is_manifold(), "wall with six openings must be a closed manifold");

        // Positive signed volume (outward winding)
        let mut vol = 0.0;
        for tri in mesh.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let pa = mesh.positions[a as usize].map(|v| v as f64);
            let pb = mesh.positions[b as usize].map(|v| v as f64);
            let pc = mesh.positions[c as usize].map(|v| v as f64);
            vol += pa[0] * (pb[1] * pc[2] - pb[2] * pc[1]) + pa[1] * (pb[2] * pc[0] - pb[0] * pc[2]) + pa[2] * (pb[0] * pc[1] - pb[1] * pc[0]);
        }
        vol /= 6.0;
        assert!(vol > 0.0, "signed volume must be > 0, got {vol}");
    }

    /// profile_prism with a pentagon profile and a hole must produce a closed manifold.
    #[test]
    fn profile_prism_with_pentagon_and_hole() {
        // Gable pentagon profile
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 2.4), Vec2::new(4.0, 4.45), Vec2::new(0.0, 2.4)];
        // Small hole in the lower part of the wall
        let hole = vec![Vec2::new(3.0, 0.5), Vec2::new(3.0, 1.5), Vec2::new(5.0, 1.5), Vec2::new(5.0, 0.5)];
        let wall = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(8.0, 0.0), thickness: 0.2, height: 4.45, ..Default::default() };
        let mesh = profile_prism(&outer, &[hole], &wall, 0.0, "wall", ArchId(7)).unwrap();
        assert!(mesh.triangle_count() > 0, "pentagon with hole must have triangles");
        assert!(mesh.is_manifold(), "pentagon prism with a hole must be a closed manifold");

        // Positive signed volume
        let mut vol = 0.0;
        for tri in mesh.indices.chunks(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let pa = mesh.positions[a as usize].map(|v| v as f64);
            let pb = mesh.positions[b as usize].map(|v| v as f64);
            let pc = mesh.positions[c as usize].map(|v| v as f64);
            vol += pa[0] * (pb[1] * pc[2] - pb[2] * pc[1]) + pa[1] * (pb[2] * pc[0] - pb[0] * pc[2]) + pa[2] * (pb[0] * pc[1] - pb[1] * pc[0]);
        }
        vol /= 6.0;
        assert!(vol > 0.0, "signed volume must be > 0, got {vol}");

        // More triangles than without the hole
        let no_hole = profile_prism(&outer, &[], &wall, 0.0, "wall", ArchId(8)).unwrap();
        assert!(
            mesh.triangle_count() > no_hole.triangle_count(),
            "hole should add triangles: with={} without={}",
            mesh.triangle_count(),
            no_hole.triangle_count()
        );
    }
}
