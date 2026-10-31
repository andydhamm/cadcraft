//! Terrain algorithms (phase 9): the original surface (a constrained TIN of the survey points,
//! elevation lines and boundary), the modified surface (flat regions, hills and valleys, the
//! building pad, basement cuts), the site mesh, contours, cut and fill, CSV import and retaining
//! walls. Pure and deterministic; the data is `cadcraft_doc::arch::{Terrain, SiteLocation}`.

pub mod contour;
pub mod csv;
pub mod mesh;
pub mod pad;
pub mod retaining;
pub mod surface;
pub mod volume;

use cadcraft_doc::arch::{ElevationLine, ModifierKind, Terrain, TerrainModifier};
use cadcraft_geom::cdt::triangulate_constrained;
use cadcraft_geom::tin::Tin;
use cadcraft_geom::tin_ops::retain_inside;
use cadcraft_geom::{Vec2, Vec3};

/// Why a terrain operation failed; the text is shown to the user.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainError(pub String);

impl std::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TerrainError {}

/// Check a terrain against every cap and invariant before it is stored. Filled in by
/// `p9-arch-terrain-tin`.
pub fn validate_terrain(t: &Terrain) -> Result<(), TerrainError> {
    validate_ring("terrain boundary", &t.boundary, true)?;
    if !t.default_elevation.is_finite() {
        return Err(TerrainError("terrain default_elevation: must be finite".into()));
    }

    let total_points = t.points.len() + t.lines.iter().map(|line| line.points.len()).sum::<usize>();
    if total_points > cadcraft_doc::arch::MAX_TERRAIN_POINTS {
        return Err(TerrainError(format!("terrain points: {} exceeds the {} point cap", total_points, cadcraft_doc::arch::MAX_TERRAIN_POINTS)));
    }
    for (i, p) in t.points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
            return Err(TerrainError(format!("terrain points[{i}]: point coordinates must be finite")));
        }
    }

    if t.lines.len() > cadcraft_doc::arch::MAX_ELEVATION_LINES {
        return Err(TerrainError(format!("terrain lines: {} exceeds the {} line cap", t.lines.len(), cadcraft_doc::arch::MAX_ELEVATION_LINES)));
    }
    for (i, line) in t.lines.iter().enumerate() {
        validate_line(&format!("terrain lines[{i}]"), line)?;
    }

    if !t.contour_minor.is_finite() || t.contour_minor < cadcraft_doc::arch::MIN_CONTOUR_INTERVAL {
        return Err(TerrainError(format!("terrain contour_minor: must be finite and >= {}", cadcraft_doc::arch::MIN_CONTOUR_INTERVAL)));
    }
    if !t.contour_major.is_finite() || t.contour_major < cadcraft_doc::arch::MIN_CONTOUR_INTERVAL {
        return Err(TerrainError(format!("terrain contour_major: must be finite and >= {}", cadcraft_doc::arch::MIN_CONTOUR_INTERVAL)));
    }
    if t.contour_major < t.contour_minor {
        return Err(TerrainError("terrain contour_major: must be >= contour_minor".into()));
    }

    if !t.pad_margin.is_finite() || t.pad_margin < 0.0 {
        return Err(TerrainError("terrain pad_margin: must be finite and >= 0".into()));
    }

    if t.modifiers.len() > cadcraft_doc::arch::MAX_TERRAIN_MODIFIERS {
        return Err(TerrainError(format!(
            "terrain modifiers: {} exceeds the {} modifier cap",
            t.modifiers.len(),
            cadcraft_doc::arch::MAX_TERRAIN_MODIFIERS
        )));
    }
    for (i, modifier) in t.modifiers.iter().enumerate() {
        validate_modifier(&format!("terrain modifiers[{i}]"), modifier)?;
    }

    Ok(())
}

/// The original surface: a constrained TIN of the data and the boundary, clipped to the
/// boundary. Filled in by `p9-arch-terrain-tin`.
pub fn original_tin(t: &Terrain) -> Result<Tin, TerrainError> {
    validate_terrain(t)?;

    let mut vertices: Vec<Vec3> = t.points.clone();
    for line in &t.lines {
        for p in &line.points {
            vertices.push(Vec3::new(p.x, p.y, line.elevation));
        }
    }

    let mut boundary_heights = vec![t.default_elevation; t.boundary.len()];
    let data_points: Vec<Vec3> =
        t.points.iter().copied().chain(t.lines.iter().flat_map(|line| line.points.iter().map(|p| Vec3::new(p.x, p.y, line.elevation)))).collect();
    if data_points.len() >= 3 {
        let data_tin = cadcraft_geom::tin::triangulate(&data_points).map_err(|e| TerrainError(format!("terrain data: {e}")))?;
        for (i, p) in t.boundary.iter().enumerate() {
            if let Some(z) = data_tin.elevation_at(p.x, p.y) {
                boundary_heights[i] = z;
            }
        }
    }

    for (i, p) in t.boundary.iter().enumerate() {
        vertices.push(Vec3::new(p.x, p.y, boundary_heights[i]));
    }

    let mut constraints = Vec::new();
    for line in &t.lines {
        let n = line.points.len();
        if n == 0 {
            continue;
        }
        for i in 0..n {
            let next = if line.closed { (i + 1) % n } else { i + 1 };
            if next < n {
                let a = index_for_point(&vertices, line.points[i].x, line.points[i].y, line.elevation);
                let b = index_for_point(&vertices, line.points[next].x, line.points[next].y, line.elevation);
                if let (Some(a), Some(b)) = (a, b) {
                    constraints.push([a, b]);
                }
            }
        }
        if line.closed && n >= 2 {
            let a = index_for_point(&vertices, line.points[0].x, line.points[0].y, line.elevation);
            let b = index_for_point(&vertices, line.points[n - 1].x, line.points[n - 1].y, line.elevation);
            if let (Some(a), Some(b)) = (a, b) {
                constraints.push([a, b]);
            }
        }
    }

    for i in 0..t.boundary.len() {
        let j = (i + 1) % t.boundary.len();
        let a = index_for_point(&vertices, t.boundary[i].x, t.boundary[i].y, boundary_heights[i]);
        let b = index_for_point(&vertices, t.boundary[j].x, t.boundary[j].y, boundary_heights[j]);
        if let (Some(a), Some(b)) = (a, b) {
            constraints.push([a, b]);
        }
    }

    let cdt = triangulate_constrained(&vertices, &constraints).map_err(|e| TerrainError(format!("terrain original_tin: {e}")))?;
    let mut tin = cdt.tin;
    retain_inside(&mut tin, &t.boundary);
    Ok(tin)
}

fn validate_line(field: &str, line: &ElevationLine) -> Result<(), TerrainError> {
    if line.points.len() < if line.closed { 3 } else { 2 } || line.points.len() > cadcraft_doc::arch::MAX_RING_POINTS {
        let min = if line.closed { 3 } else { 2 };
        return Err(TerrainError(format!("{field}: expected {}..={} points, got {}", min, cadcraft_doc::arch::MAX_RING_POINTS, line.points.len())));
    }
    if !line.elevation.is_finite() {
        return Err(TerrainError(format!("{field}: elevation must be finite")));
    }
    for (j, p) in line.points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(TerrainError(format!("{field}: point {j} must be finite")));
        }
    }
    for i in 0..line.points.len() {
        let next = if line.closed { (i + 1) % line.points.len() } else { i + 1 };
        if next < line.points.len() {
            let a = line.points[i];
            let b = line.points[next];
            if (a.x - b.x).hypot(a.y - b.y) <= 1e-12 {
                return Err(TerrainError(format!("{field}: consecutive points {i} and {next} are equal")));
            }
        }
    }
    if line.closed && line.points.first().is_some() && line.points.last().is_some() {
        let first = line.points[0];
        let last = line.points[line.points.len() - 1];
        if (first.x - last.x).hypot(first.y - last.y) <= 1e-12 {
            return Err(TerrainError(format!("{field}: first and last points must differ")));
        }
    }
    Ok(())
}

fn validate_ring(field: &str, ring: &[Vec2], allow_closed: bool) -> Result<(), TerrainError> {
    let min = if allow_closed { 3 } else { 2 };
    if ring.len() < min || ring.len() > cadcraft_doc::arch::MAX_RING_POINTS {
        return Err(TerrainError(format!("{field}: expected {}..={} points, got {}", min, cadcraft_doc::arch::MAX_RING_POINTS, ring.len())));
    }
    for (i, p) in ring.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(TerrainError(format!("{field}: point {i} is not finite")));
        }
    }
    for i in 0..ring.len() {
        let next = if allow_closed { (i + 1) % ring.len() } else { i + 1 };
        if next < ring.len() {
            let a = ring[i];
            let b = ring[next];
            if (a.x - b.x).hypot(a.y - b.y) <= 1e-12 {
                return Err(TerrainError(format!("{field}: consecutive points {i} and {next} are equal")));
            }
        }
    }
    if allow_closed {
        let first = ring[0];
        let last = ring[ring.len() - 1];
        if (first.x - last.x).hypot(first.y - last.y) <= 1e-12 {
            return Err(TerrainError(format!("{field}: first and last points must differ")));
        }
        if polygon_area(ring).abs() <= 1e-12 {
            return Err(TerrainError(format!("{field}: area must be non-zero")));
        }
        if !is_simple_ring(ring) {
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                for j in i + 1..ring.len() {
                    let k = if j + 1 == ring.len() { 0 } else { j + 1 };
                    if let Some((u, v)) = if k == i || k == (i + 1) % ring.len() || j == i || j == (i + 1) % ring.len() { None } else { Some((i, j)) }
                    {
                        if segments_intersect(a, b, ring[j], ring[k]) {
                            return Err(TerrainError(format!("{field}: edges {u} and {v} cross")));
                        }
                    }
                }
            }
            return Err(TerrainError(format!("{field}: edges cross")));
        }
    }
    Ok(())
}

fn validate_modifier(field: &str, modifier: &TerrainModifier) -> Result<(), TerrainError> {
    match &modifier.kind {
        ModifierKind::Flat { polygon, elevation, margin } => {
            validate_ring(&format!("{field}.polygon"), polygon, true)?;
            if !elevation.is_finite() {
                return Err(TerrainError(format!("{field}.elevation: must be finite")));
            }
            if !margin.is_finite() || *margin < 0.0 {
                return Err(TerrainError(format!("{field}.margin: must be finite and >= 0")));
            }
        }
        ModifierKind::Hill { center, height, radius } => {
            if !center.x.is_finite() || !center.y.is_finite() {
                return Err(TerrainError(format!("{field}.center: must be finite")));
            }
            if !height.is_finite() || *height == 0.0 || height.abs() > 1000.0 {
                return Err(TerrainError(format!("{field}.height: must be finite, non-zero and <= 1000")));
            }
            if !radius.is_finite() || !(*radius > 0.0 && *radius <= 10_000.0) {
                return Err(TerrainError(format!("{field}.radius: must be finite and in (0, 10000]")));
            }
        }
        ModifierKind::Path { kind: _, polygon, thickness, material } => {
            validate_ring(&format!("{field}.polygon"), polygon, true)?;
            if !thickness.is_finite() || !(*thickness > 0.0 && *thickness <= 5.0) {
                return Err(TerrainError(format!("{field}.thickness: must be finite and in (0, 5]")));
            }
            if material.trim().is_empty() || material.chars().count() > 64 {
                return Err(TerrainError(format!("{field}.material: must be non-empty and at most 64 chars")));
            }
        }
    }
    Ok(())
}

fn polygon_area(ring: &[Vec2]) -> f64 {
    let mut sum = 0.0;
    let n = ring.len();
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        sum += a.x * b.y - b.x * a.y;
    }
    0.5 * sum
}

fn orient(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn on_segment(a: Vec2, b: Vec2, p: Vec2) -> bool {
    let min_x = a.x.min(b.x) - 1e-9;
    let max_x = a.x.max(b.x) + 1e-9;
    let min_y = a.y.min(b.y) - 1e-9;
    let max_y = a.y.max(b.y) + 1e-9;
    p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y && orient(a, b, p).abs() <= 1e-9
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    let general = (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0);
    if general {
        return true;
    }
    (o1.abs() <= 1e-9 && on_segment(a, b, c))
        || (o2.abs() <= 1e-9 && on_segment(a, b, d))
        || (o3.abs() <= 1e-9 && on_segment(c, d, a))
        || (o4.abs() <= 1e-9 && on_segment(c, d, b))
}

fn is_simple_ring(ring: &[Vec2]) -> bool {
    let n = ring.len();
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        for j in i + 1..n {
            if j == i || j == (i + 1) % n || i == (j + 1) % n {
                continue;
            }
            let c = ring[j];
            let d = ring[(j + 1) % n];
            if segments_intersect(a, b, c, d) {
                return false;
            }
        }
    }
    true
}

fn index_for_point(points: &[Vec3], x: f64, y: f64, z: f64) -> Option<usize> {
    points.iter().position(|p| (p.x - x).abs() <= 1e-9 && (p.y - y).abs() <= 1e-9 && (p.z - z).abs() <= 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{ElevationLine, MAX_TERRAIN_MODIFIERS, ModifierKind, Terrain, TerrainModifier};

    #[test]
    fn original_tin_of_four_corners_is_the_plane() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let terrain = Terrain {
            boundary: boundary.clone(),
            default_elevation: 0.0,
            points,
            lines: Vec::new(),
            modifiers: Vec::new(),
            next_modifier_id: 0,
            contour_minor: 0.25,
            contour_major: 1.0,
            pad: false,
            pad_margin: 0.3,
        };
        let tin = original_tin(&terrain).unwrap();
        let z = tin.elevation_at(25.0, 12.0).unwrap();
        assert!((z - 1.25).abs() < 1e-6);
    }

    #[test]
    fn original_tin_keeps_elevation_lines_as_edges() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)];
        let terrain = Terrain {
            boundary: boundary.clone(),
            default_elevation: 0.0,
            points,
            lines: vec![ElevationLine { points: vec![Vec2::new(2.0, 15.0), Vec2::new(38.0, 15.0)], elevation: 3.0, closed: false }],
            modifiers: Vec::new(),
            next_modifier_id: 0,
            contour_minor: 0.25,
            contour_major: 1.0,
            pad: false,
            pad_margin: 0.3,
        };
        let tin = original_tin(&terrain).unwrap();
        let z = tin.elevation_at(20.0, 15.0).unwrap();
        assert!((z - 3.0).abs() < 1e-6);
    }

    #[test]
    fn original_tin_takes_boundary_heights_from_the_data() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(40.0, 0.0, 2.0),
            Vec3::new(40.0, 30.0, 2.0),
            Vec3::new(0.0, 30.0, 0.0),
            Vec3::new(20.0, 13.0, 0.0),
            Vec3::new(20.0, 17.0, 0.0),
        ];
        let terrain = Terrain {
            boundary: boundary.clone(),
            default_elevation: 0.0,
            points,
            lines: vec![ElevationLine { points: vec![Vec2::new(2.0, 15.0), Vec2::new(38.0, 15.0)], elevation: 3.0, closed: false }],
            modifiers: Vec::new(),
            next_modifier_id: 0,
            contour_minor: 0.25,
            contour_major: 1.0,
            pad: false,
            pad_margin: 0.3,
        };
        let tin = original_tin(&terrain).unwrap();
        let z = tin.elevation_at(20.0, 14.0).unwrap();
        assert!((z - 1.5).abs() < 1e-3);
    }

    #[test]
    fn original_tin_with_no_data_is_flat_at_the_default() {
        let boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)];
        let terrain = Terrain {
            boundary: boundary.clone(),
            default_elevation: 7.25,
            points: Vec::new(),
            lines: Vec::new(),
            modifiers: Vec::new(),
            next_modifier_id: 0,
            contour_minor: 0.25,
            contour_major: 1.0,
            pad: false,
            pad_margin: 0.3,
        };
        let tin = original_tin(&terrain).unwrap();
        assert!((tin.elevation_at(10.0, 10.0).unwrap() - 7.25).abs() < 1e-6);
        let corner = tin.elevation_at(0.0, 0.0).unwrap();
        assert!((corner - 7.25).abs() < 1e-6);
    }

    #[test]
    fn validate_terrain_refuses_a_self_crossing_boundary() {
        let terrain = Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(0.0, 30.0), Vec2::new(40.0, 30.0)],
            default_elevation: 0.0,
            points: Vec::new(),
            lines: Vec::new(),
            modifiers: Vec::new(),
            next_modifier_id: 0,
            contour_minor: 0.25,
            contour_major: 1.0,
            pad: false,
            pad_margin: 0.3,
        };
        assert!(validate_terrain(&terrain).is_err());
    }

    #[test]
    fn validate_terrain_refuses_bad_modifiers_and_caps() {
        let mut terrain = Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)],
            default_elevation: 0.0,
            points: Vec::new(),
            lines: Vec::new(),
            modifiers: vec![TerrainModifier {
                id: 0,
                kind: ModifierKind::Path {
                    kind: cadcraft_doc::arch::PathKind::Road,
                    polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)],
                    thickness: 0.2,
                    material: "x".repeat(65),
                },
            }],
            next_modifier_id: 1,
            contour_minor: 0.25,
            contour_major: 1.0,
            pad: false,
            pad_margin: 0.3,
        };
        assert!(validate_terrain(&terrain).is_err());

        terrain.modifiers.clear();
        terrain.modifiers.resize(
            MAX_TERRAIN_MODIFIERS + 1,
            TerrainModifier {
                id: 0,
                kind: ModifierKind::Flat {
                    polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)],
                    elevation: 1.0,
                    margin: 0.0,
                },
            },
        );
        assert!(validate_terrain(&terrain).is_err());
    }
}
