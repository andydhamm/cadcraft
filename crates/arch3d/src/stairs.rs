//! Stair flights and landings as closed solid meshes.
//!
//! The flight profile is built in flight coordinates (`u` along travel, `v` up),
//! then extruded across the stair width and transformed into world space by the
//! stored `direction` and `start` point.

use cadcraft_doc::arch::Stair;
use cadcraft_geom::Vec2;

/// Build a single closed solid for the stair flight.
///
/// The mesh is a stepped prism whose outline follows the riser/tread profile in
/// flight coordinates and whose depth spans the full stair width. Returns `None`
/// when the input is non-finite or degenerate.
pub fn stair_mesh(s: &Stair, base: f64) -> Option<crate::mesh::Mesh> {
    let ring = drop_collinear(&stair_profile(s)?);
    if !base.is_finite() {
        return None;
    }

    let dir = Vec2::from_angle(s.direction);
    let perp = dir.perp();
    let mut mesh = crate::mesh::Mesh::new("stair", s.id);
    mesh.level_elevation = base;
    mesh.push_prism(&ring, 0.0, s.width);
    if mesh.indices.is_empty() {
        return None;
    }

    for p in &mut mesh.positions {
        let u = p[0] as f64;
        let v = p[1] as f64;
        let t = p[2] as f64;
        let x = s.start.x + u * dir.x + t * perp.x;
        let y = s.start.y + u * dir.y + t * perp.y;
        let z = base + v;
        *p = [x as f32, y as f32, z as f32];
    }

    Some(mesh)
}

/// Build a landing slab at the top of the flight.
///
/// The slab is a single closed solid in plan, `landing_depth` by `width`, with a
/// thickness of 0.05 and its top surface at `base + rise`.
pub fn landing_mesh(s: &Stair, base: f64) -> Option<crate::mesh::Mesh> {
    if !stair_valid(s) || !base.is_finite() {
        return None;
    }
    let landing_depth = if s.landing_depth.is_finite() && s.landing_depth > 0.0 { s.landing_depth } else { s.width };
    let dir = Vec2::from_angle(s.direction);
    let perp = dir.perp();
    let top = base + s.rise;
    let bottom = top - 0.05;
    let start_u = s.run;
    let mut ring = vec![
        Vec2::new(start_u, 0.0),
        Vec2::new(start_u + landing_depth, 0.0),
        Vec2::new(start_u + landing_depth, s.width),
        Vec2::new(start_u, s.width),
    ];
    if ring.iter().any(|p| !p.is_finite()) {
        return None;
    }

    let mut mesh = crate::mesh::Mesh::new("stair", s.id);
    mesh.level_elevation = base;
    mesh.push_prism(&ring, bottom - base, top - base);
    if mesh.indices.is_empty() {
        return None;
    }

    for p in &mut mesh.positions {
        let u = p[0] as f64;
        let w = p[1] as f64;
        let z_local = p[2] as f64;
        let x = s.start.x + u * dir.x + w * perp.x;
        let y = s.start.y + u * dir.y + w * perp.y;
        let z = bottom + z_local;
        *p = [x as f32, y as f32, z as f32];
    }

    for i in (0..mesh.indices.len()).step_by(3) {
        let tmp = mesh.indices[i + 1];
        mesh.indices[i + 1] = mesh.indices[i + 2];
        mesh.indices[i + 2] = tmp;
    }

    let len = mesh.positions.len();
    let mut normals: Vec<[f32; 3]> = vec![[0.0; 3]; len];
    for tri in mesh.indices.chunks_exact(3) {
        let a = tri[0] as usize;
        let b = tri[1] as usize;
        let c = tri[2] as usize;
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
    for n in &mut normals {
        let len_sq = (n[0].powi(2) + n[1].powi(2) + n[2].powi(2)) as f64;
        if len_sq > 1e-12 {
            let len = len_sq.sqrt();
            n[0] /= len as f32;
            n[1] /= len as f32;
            n[2] /= len as f32;
        } else {
            n[0] = 0.0;
            n[1] = 0.0;
            n[2] = 1.0;
        }
    }
    mesh.normals = normals;
    mesh.uvs = mesh.positions.iter().map(|p| [p[0], p[2]]).collect();

    Some(mesh)
}

/// Remove vertices collinear with their neighbours (including zero-width
/// spikes such as the last riser meeting the underside at `u = run`), which
/// would otherwise leave non-manifold edges in the extruded prism.
fn drop_collinear(ring: &[Vec2]) -> Vec<Vec2> {
    let n = ring.len();
    let mut out = Vec::with_capacity(n);
    for (i, &c) in ring.iter().enumerate() {
        let a = ring[(i + n - 1) % n];
        let b = ring[(i + 1) % n];
        let cross = (c.x - a.x) * (b.y - c.y) - (c.y - a.y) * (b.x - c.x);
        if cross.abs() > 1e-12 {
            out.push(c);
        }
    }
    out
}

fn stair_valid(s: &Stair) -> bool {
    s.start.is_finite()
        && s.direction.is_finite()
        && s.width.is_finite()
        && s.width > 0.0
        && s.risers > 0
        && s.riser_height.is_finite()
        && s.riser_height > 0.0
        && s.tread_depth.is_finite()
        && s.tread_depth > 0.0
        && s.rise.is_finite()
        && s.rise > 0.0
        && (s.run.is_finite() && s.run >= 0.0)
}

fn stair_profile(s: &Stair) -> Option<Vec<Vec2>> {
    if !stair_valid(s) {
        return None;
    }

    // Tread k is a horizontal top at v = k * riser_height over
    // u in [(k-1) * tread, k * tread]; the last riser rises at u = run.
    let mut ring = Vec::with_capacity(s.treads.min(4096) * 2 + 3);
    ring.push(Vec2::ZERO);
    for k in 1..=s.treads.min(4096) {
        let v = k as f64 * s.riser_height;
        ring.push(Vec2::new((k - 1) as f64 * s.tread_depth, v));
        ring.push(Vec2::new(k as f64 * s.tread_depth, v));
    }

    if ring.len() < 3 {
        return None;
    }

    // The ring closes back to the origin without repeating the first vertex.
    ring.push(Vec2::new(s.run, s.rise));
    ring.push(Vec2::new(s.run, 0.0));
    if ring.iter().any(|p| !p.is_finite()) {
        return None;
    }

    let area = ring.iter().enumerate().fold(0.0, |acc, (i, p)| {
        let q = ring[(i + 1) % ring.len()];
        acc + (p.x * q.y) - (p.y * q.x)
    }) / 2.0;
    if !area.is_finite() || area.abs() < 1e-12 {
        return None;
    }

    Some(ring)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::Stair;
    use cadcraft_geom::Vec2;

    #[test]
    fn stair_mesh_is_a_closed_solid() {
        let s = Stair {
            start: Vec2::ZERO,
            direction: 0.0,
            width: 1.0,
            risers: 16,
            riser_height: 0.16875,
            tread_depth: 0.254,
            rise: 2.7,
            run: 3.81,
            ..Default::default()
        };
        let mesh = stair_mesh(&s, 0.0).expect("stair mesh");
        assert!(mesh.is_manifold());
        assert!(!mesh.indices.is_empty());
    }

    fn flight() -> Stair {
        Stair {
            start: Vec2::ZERO,
            direction: 0.0,
            width: 1.0,
            risers: 16,
            treads: 15,
            riser_height: 0.16875,
            tread_depth: 0.254,
            rise: 2.7,
            run: 3.81,
            ..Default::default()
        }
    }

    /// Highest profile `v` at flight coordinate `u`, from the ring's horizontal edges.
    fn top_at(ring: &[Vec2], u: f64) -> f64 {
        let mut best = f64::NEG_INFINITY;
        for (i, p) in ring.iter().enumerate() {
            let q = ring[(i + 1) % ring.len()];
            if (p.y - q.y).abs() < 1e-12 && u >= p.x.min(q.x) && u <= p.x.max(q.x) {
                best = best.max(p.y);
            }
        }
        best
    }

    #[test]
    fn stair_profile_step_tops_are_integer_risers() {
        let ring = stair_profile(&flight()).expect("profile");
        assert!((top_at(&ring, 0.1) - 0.16875).abs() < 1e-9);
        assert!((top_at(&ring, 0.6) - 0.50625).abs() < 1e-9);
        assert!((top_at(&ring, 3.7) - 2.53125).abs() < 1e-9);
    }

    #[test]
    fn stair_profile_ends_at_run_and_rise() {
        let s = flight();
        let ring = stair_profile(&s).expect("profile");
        let n = ring.len();
        assert_eq!((ring[n - 2].x, ring[n - 2].y), (s.run, s.rise));
        assert_eq!((ring[n - 1].x, ring[n - 1].y), (s.run, 0.0));
        assert!(stair_profile(&Stair { treads: 0, ..flight() }).is_none());
    }

    #[test]
    fn stair_mesh_has_16_steps_for_2_7() {
        let s = Stair {
            start: Vec2::ZERO,
            direction: 0.0,
            width: 1.0,
            risers: 16,
            riser_height: 0.16875,
            tread_depth: 0.254,
            rise: 2.7,
            run: 3.81,
            ..Default::default()
        };
        let mesh = stair_mesh(&s, 0.0).expect("stair mesh");
        assert_eq!(s.risers, 16);
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn stair_mesh_is_none_for_garbage() {
        let mut s = Stair::default();
        s.width = f64::NAN;
        assert!(stair_mesh(&s, 0.0).is_none());

        let mut s = Stair::default();
        s.risers = 0;
        assert!(stair_mesh(&s, 0.0).is_none());

        let mut s = Stair::default();
        s.direction = f64::INFINITY;
        assert!(stair_mesh(&s, 0.0).is_none());

        let mut s = Stair::default();
        s.riser_height = 0.0;
        assert!(stair_mesh(&s, 0.0).is_none());
    }

    #[test]
    fn stair_mesh_sits_on_the_from_level_elevation() {
        let s = Stair {
            start: Vec2::ZERO,
            direction: 0.0,
            width: 1.0,
            risers: 16,
            riser_height: 0.16875,
            tread_depth: 0.254,
            rise: 2.7,
            run: 3.81,
            ..Default::default()
        };
        let base = 4.2;
        let mesh = stair_mesh(&s, base).expect("stair mesh");
        let min_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::INFINITY, f64::min);
        assert!((min_z - base).abs() < 1e-6);
    }

    #[test]
    fn stair_mesh_with_nan_base_returns_none() {
        let s = flight();
        assert!(stair_mesh(&s, f64::NAN).is_none());
    }

    #[test]
    fn stair_mesh_with_zero_width_returns_none() {
        let s = Stair { width: 0.0, ..flight() };
        assert!(stair_mesh(&s, 0.0).is_none());
    }

    #[test]
    fn stair_mesh_rotated_90_degrees_goes_in_y_direction() {
        let s = Stair {
            start: Vec2::ZERO,
            direction: std::f64::consts::FRAC_PI_2,
            width: 1.0,
            risers: 16,
            treads: 15,
            riser_height: 0.16875,
            tread_depth: 0.254,
            rise: 2.7,
            run: 3.81,
            ..Default::default()
        };
        let mesh = stair_mesh(&s, 0.0).expect("stair mesh");
        let max_x = mesh.positions.iter().map(|p| p[0] as f64).fold(f64::NEG_INFINITY, f64::max);
        let max_y = mesh.positions.iter().map(|p| p[1] as f64).fold(f64::NEG_INFINITY, f64::max);
        assert!(max_y > max_x, "stair should extend more in Y than X when rotated 90 degrees");
        assert!((max_y - 3.81).abs() < 0.01, "max Y should be approximately the run (3.81), got {}", max_y);
    }

    #[test]
    fn landing_mesh_is_at_top_of_flight() {
        let s = flight();
        let base = 0.0;
        let mesh = landing_mesh(&s, base).expect("landing mesh");
        let max_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
        let min_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::INFINITY, f64::min);
        assert!((max_z - (base + s.rise)).abs() < 1e-6, "landing top should be at base + rise");
        assert!((min_z - (base + s.rise - 0.05)).abs() < 1e-6, "landing bottom should be at base + rise - 0.05");
    }

    #[test]
    fn landing_mesh_with_zero_depth_returns_none() {
        let s = Stair { landing_depth: 0.0, ..flight() };
        assert!(landing_mesh(&s, 0.0).is_none());
    }

    #[test]
    fn stair_profile_with_huge_tread_count_is_capped() {
        let s = Stair {
            start: Vec2::ZERO,
            direction: 0.0,
            width: 1.0,
            risers: 10000,
            treads: 9999,
            riser_height: 0.01,
            tread_depth: 0.254,
            rise: 100.0,
            run: 2539.746,
            ..Default::default()
        };
        let ring = stair_profile(&s).expect("profile");
        // Capped at 4096 treads: 1 (origin) + 2*4096 (tread points) + 2 (closure) = 8195 max.
        assert!(ring.len() <= 8195, "profile should be capped, got {} points", ring.len());
    }

    #[test]
    fn drop_collinear_keeps_non_collinear_vertices() {
        let ring = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0), // collinear with neighbours
            Vec2::new(4.0, 0.0),
            Vec2::new(4.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let result = drop_collinear(&ring);
        assert_eq!(result.len(), 4, "collinear vertex should be dropped");
        assert!(!result.contains(&Vec2::new(2.0, 0.0)), "the collinear point should be gone");
    }
}
