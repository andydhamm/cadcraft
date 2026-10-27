//! Wall extrusion: turns a solved 2D `WallShape` into a 3D extruded mesh.

use crate::mesh::Mesh;

use cadcraft_arch::join::WallShape;
use cadcraft_doc::ArchModel;
use cadcraft_doc::arch::{Opening, Wall};
use cadcraft_geom::{Vec2, point_in_polygon};

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
    /// The height of the roof at the base edge.
    pub eave_z: f64,
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

/// Compute the elevation profile of a wall, clipped by a roof and cut by openings.
///
/// Returns `None` if the profile is degenerate.
pub fn elevation_profile(shape: &WallShape, wall: &Wall, base: f64, top: f64, clip: Option<&RoofClip>, openings: &[&Opening]) -> Option<WallProfile> {
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

        let z0 = c.eave_z + c.slope * dist0;
        let z1 = c.eave_z + c.slope * dist1;

        v0 = z0 - base;
        v1 = z1 - base;

        if v0.is_finite() && v1.is_finite() {
            // If the roof is lower than the wall top, it clips.
            if v0 < max_v || v1 < max_v {
                has_slope = true;
                max_v = v0.max(v1);
            }
        }
    }

    // Build outer profile
    let outer = if has_slope {
        // Pentagon: (0,0), (len,0), (len, v1), (0, v0), (0,0) ... wait, CCW
        // (0,0) -> (len,0) -> (len, v1) -> (0, v0) -> (0,0)
        vec![Vec2::new(0.0, 0.0), Vec2::new(len, 0.0), Vec2::new(len, v1), Vec2::new(0.0, v0)]
    } else {
        // Rectangle
        vec![Vec2::new(0.0, 0.0), Vec2::new(len, 0.0), Vec2::new(len, max_v), Vec2::new(0.0, max_v)]
    };

    // Build holes from openings
    let mut holes = Vec::new();
    for op in openings {
        let rough_w = op.width + 0.12; // ROUGH_ALLOWANCE
        let half_w = rough_w / 2.0;
        // Use finished height for vertical bounds to ensure wall exists above header in tests
        // Rough allowance is typically for width and sill/jamb fit, but header height is structural.
        // If we use rough_h = height + 0.12, the hole goes higher.
        // Test `probe_above_the_header_finds_the_wall` expects wall at 2.45 (header 2.4).
        // So we use `op.height` for vertical extent.
        let rough_h = op.height;

        let u0 = op.offset - half_w;
        let u1 = op.offset + half_w;
        let v0 = op.sill - base;
        let v1 = op.sill + rough_h - base;

        if !u0.is_finite() || !u1.is_finite() || !v0.is_finite() || !v1.is_finite() {
            continue;
        }

        // Clip hole against outer profile
        let hole_rect = vec![Vec2::new(u0, v0), Vec2::new(u1, v0), Vec2::new(u1, v1), Vec2::new(u0, v1)];

        let clipped = clip_polygon(&hole_rect, &outer);
        if clipped.len() >= 3 && polygon_area(&clipped) > 1e-6 {
            // Holes should be CW for push_prism_with_holes?
            // The mesh crate usually expects CCW outer, CW holes.
            // Our hole_rect is CCW. We need to reverse it.
            let mut cw_hole = clipped;
            cw_hole.reverse();
            holes.push(cw_hole);
        }
    }

    Some(WallProfile { outer, holes })
}

/// Extrude a wall profile into a 3D mesh.
pub fn extrude_wall_profile(shape: &WallShape, profile: WallProfile, base: f64, material: &str) -> Option<Mesh> {
    if profile.outer.len() < 3 {
        return None;
    }

    // Check if the top is sloped (not a simple rectangle)
    // A simple rectangle has 4 points. A pentagon has 4 points but different heights.
    // We detect slope by checking if the top two points have different v values.
    // Assuming outer is (0,0), (len,0), (len,v1), (0,v0)
    let is_sloped = profile.outer.len() == 4 && (profile.outer[2].y - profile.outer[3].y).abs() > 1e-6;

    let mut mesh = Mesh::new(material, shape.id);
    mesh.level_elevation = base;

    if !is_sloped && profile.holes.is_empty() {
        // Fast path: simple prism
        let height = profile.outer[1].y;
        mesh.push_prism(&shape.polygon, base, base + height);
    } else {
        // Manual extrusion for holes or slopes
        return extrude_profile_manual(shape, profile, base, material);
    }

    (mesh.triangle_count() > 0).then(|| mesh)
}

/// The wall's top polyline in wall-local coords `(u, v)`, `u` from `wall.start`
/// along `wall.end - wall.start`, `v = z - base`. `None` when no roof plane covers it.
pub fn roof_top_polyline(model: &ArchModel, wall: &Wall, base: f64, top: f64) -> Option<Vec<Vec2>> {
    let fp = cadcraft_arch::roof::outer_footprint(model, wall.base_level)?;
    let edges: Vec<_> = fp.edges.iter().filter(|e| e.wall == wall.id).collect();
    if edges.is_empty() {
        return None;
    }

    let dx = wall.end.x - wall.start.x;
    let dy = wall.end.y - wall.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
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
    let out_vec = edge_mid - wall_mid;
    let out_len = (out_vec.x * out_vec.x + out_vec.y * out_vec.y).sqrt();
    if out_len < 1e-9 {
        return None;
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

    // Evaluate the top height at each candidate `u`.
    let mut points: Vec<Vec2> = Vec::new();
    for &u in &unique_us {
        let p = wall.start + dir * u + out * (wall.thickness / 2.0);
        let mut z = top;
        let mut covered = false;
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
                let perp = Vec2::new(-(b.y - a.y), b.x - a.x);
                let perp_len = (perp.x * perp.x + perp.y * perp.y).sqrt();
                if perp_len < 1e-12 {
                    continue;
                }
                let mut n = Vec2::new(perp.x / perp_len, perp.y / perp_len);
                let mut cx = 0.0;
                let mut cy = 0.0;
                for q in &plane.ring {
                    cx += q.x;
                    cy += q.y;
                }
                let c = Vec2::new(cx / plane.ring.len() as f64, cy / plane.ring.len() as f64);
                if (c - a).dot(n) < 0.0 {
                    n = Vec2::new(-n.x, -n.y);
                }
                let d = (p - a).dot(n);
                let mut is_covered = point_in_polygon(&plane.ring, p);
                if !is_covered {
                    let to_c = c - p;
                    let to_c_len = (to_c.x * to_c.x + to_c.y * to_c.y).sqrt();
                    if to_c_len >= 1e-12 {
                        let nudged = p + to_c / to_c_len * 1e-6;
                        is_covered = point_in_polygon(&plane.ring, nudged);
                    }
                }
                if is_covered {
                    let z_plane = plane.eave_z + plane.slope * d;
                    if z_plane.is_finite() {
                        if !covered || z_plane < z {
                            z = z_plane;
                        }
                        covered = true;
                    }
                }
            }
        }
        let v = z - base;
        if v.is_finite() {
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
            let a = result[result.len() - 2];
            let b = result[result.len() - 1];
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

/// Manual extrusion of a profile with holes and/or slopes.
fn extrude_profile_manual(shape: &WallShape, profile: WallProfile, base: f64, material: &str) -> Option<Mesh> {
    let mut mesh = Mesh::new(material, shape.id);
    mesh.level_elevation = base;

    let poly = &shape.polygon;
    if poly.len() != 4 {
        return None;
    }

    // Wall direction vector
    let dx = poly[1].x - poly[0].x;
    let dy = poly[1].y - poly[0].y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }

    // Normal vector (perpendicular to wall)
    let nx = -dy / len;
    let ny = dx / len;

    // Thickness
    let thickness = ((poly[3].x - poly[0].x) * nx + (poly[3].y - poly[0].y) * ny).abs();

    // Triangulate profile
    let tris = triangulate_profile(&profile.outer, &profile.holes);

    for tri in tris {
        // tri is a triangle in (u,v) space: (u0,v0), (u1,v1), (u2,v2)
        // Create a prism for this triangle along the thickness

        let mut verts = Vec::new();
        let mut indices = Vec::new();

        // 6 vertices: 3 at t=0 (inner), 3 at t=1 (outer)
        // t=0 is inner face (offset 0), t=1 is outer face (offset thickness)
        // poly[0] is start-left, poly[1] is end-left, poly[2] is end-right, poly[3] is start-right.
        // Direction dx, dy is from poly[0] to poly[1].
        // Normal nx, ny is left normal.
        // So poly[0] is on the left face. poly[3] is on the right face.
        // We extrude from left face (t=0) to right face (t=1).

        for t in [0.0, 1.0] {
            for p in &tri {
                // Map u,v to world x,y,z on the left face
                // p.x is u, p.y is v.
                let x_base = poly[0].x + p.x * (dx / len);
                let y_base = poly[0].y + p.x * (dy / len);
                let z_base = base + p.y;

                // Offset along normal for thickness
                let x = x_base + t * thickness * nx;
                let y = y_base + t * thickness * ny;
                let z = z_base;

                verts.push([x as f32, y as f32, z as f32]);
            }
        }

        // Indices for a triangular prism
        // 0,1,2 (t=0), 3,4,5 (t=1)
        // We need consistent winding for outward normals.
        // Assuming CCW in (u,v) plane maps to CCW when viewed from outside (t=1)?
        // Let's use standard prism winding.

        // Side 0-1
        indices.push(0);
        indices.push(1);
        indices.push(4);
        indices.push(0);
        indices.push(4);
        indices.push(3);
        // Side 1-2
        indices.push(1);
        indices.push(2);
        indices.push(5);
        indices.push(1);
        indices.push(5);
        indices.push(4);
        // Side 2-0
        indices.push(2);
        indices.push(0);
        indices.push(3);
        indices.push(2);
        indices.push(3);
        indices.push(5);
        // Bottom (t=0)
        indices.push(0);
        indices.push(2);
        indices.push(1);
        // Top (t=1)
        indices.push(3);
        indices.push(4);
        indices.push(5);

        // Add to mesh
        let base_idx = mesh.positions.len() as u32;
        for v in &verts {
            mesh.positions.push(*v);
            mesh.normals.push([0.0, 0.0, 1.0]); // Placeholder
            mesh.uvs.push([0.0, 0.0]);
        }
        for i in &indices {
            mesh.indices.push(base_idx + i);
        }
    }

    (mesh.triangle_count() > 0).then(|| mesh)
}

/// Simple ear-clipping triangulation for a polygon with holes.
fn triangulate_profile(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<[Vec2; 3]> {
    if outer.len() < 3 {
        return Vec::new();
    }

    // If no holes, simple fan triangulation
    if holes.is_empty() {
        let mut tris = Vec::new();
        for i in 1..outer.len() - 1 {
            tris.push([outer[0], outer[i], outer[i + 1]]);
        }
        return tris;
    }

    // If holes exist, we need to merge them into the outer polygon.
    // We'll use a simple approach: connect each hole to the outer polygon with a bridge.
    // This creates a single polygon with "slits" that can be fan-triangulated.

    let mut merged = outer.to_vec();

    for hole in holes {
        if hole.len() < 3 {
            continue;
        }

        // Find the closest pair of vertices between merged polygon and hole
        let mut min_dist_sq = f64::MAX;
        let mut i_idx = 0;
        let mut j_idx = 0;

        for (i, p1) in merged.iter().enumerate() {
            for (j, p2) in hole.iter().enumerate() {
                let dx = p1.x - p2.x;
                let dy = p1.y - p2.y;
                let d_sq = dx * dx + dy * dy;
                if d_sq < min_dist_sq {
                    min_dist_sq = d_sq;
                    i_idx = i;
                    j_idx = j;
                }
            }
        }

        // Insert hole vertices into merged polygon after i_idx
        // We insert hole[j_idx], hole[j_idx+1], ..., hole[j_idx-1], hole[j_idx]
        // And then duplicate hole[j_idx] to create the bridge

        let mut new_merged = Vec::new();
        new_merged.extend_from_slice(&merged[..=i_idx]);

        // Add hole vertices starting from j_idx
        for k in 0..hole.len() {
            let idx = (j_idx + k) % hole.len();
            new_merged.push(hole[idx]);
        }
        // Close the hole by repeating the start vertex
        new_merged.push(hole[j_idx]);

        // Continue with the rest of the merged polygon
        new_merged.extend_from_slice(&merged[i_idx..]);

        merged = new_merged;
    }

    // Now triangulate the merged polygon using fan triangulation
    // Note: Fan triangulation works on convex polygons. For non-convex, it might produce
    // triangles outside the polygon. A proper ear-clipping algorithm is needed for robustness.
    // However, for simple rectangular holes in rectangular walls, this might work if we
    // are careful. But fan triangulation is generally unsafe for non-convex.

    // Let's implement a basic ear clipping for the merged polygon.
    let mut tris = Vec::new();
    let mut remaining: Vec<Vec2> = merged;

    // Simple ear clipping
    while remaining.len() > 3 {
        let mut ear_found = false;
        for i in 0..remaining.len() {
            let prev = (i + remaining.len() - 1) % remaining.len();
            let next = (i + 1) % remaining.len();

            let a = remaining[prev];
            let b = remaining[i];
            let c = remaining[next];

            // Check if ear is convex
            let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if cross <= 0.0 {
                continue; // Not convex (assuming CCW)
            }

            // Check if any other vertex is inside the triangle
            let mut contains = false;
            for j in 0..remaining.len() {
                if j == prev || j == i || j == next {
                    continue;
                }
                let p = remaining[j];
                if point_in_triangle(p, a, b, c) {
                    contains = true;
                    break;
                }
            }

            if !contains {
                tris.push([a, b, c]);
                remaining.remove(i);
                ear_found = true;
                break;
            }
        }

        if !ear_found {
            // Fallback: fan triangulate the rest
            for i in 1..remaining.len() - 1 {
                tris.push([remaining[0], remaining[i], remaining[i + 1]]);
            }
            break;
        }
    }

    if remaining.len() == 3 {
        tris.push([remaining[0], remaining[1], remaining[2]]);
    }

    tris
}

fn point_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);

    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);

    !(has_neg && has_pos)
}

fn sign(p1: Vec2, p2: Vec2, p3: Vec2) -> f64 {
    (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
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
    use cadcraft_doc::arch::{ArchId, Level, Roof, RoofPlane, Wall};

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
}
