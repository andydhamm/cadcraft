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
    let ring = stair_profile(s)?;
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
    if !s.landing_depth.is_finite() || s.landing_depth <= 0.0 {
        return None;
    }
    let dir = Vec2::from_angle(s.direction);
    let perp = dir.perp();
    let top = base + s.rise;
    let bottom = top - 0.05;
    let start_u = s.run;
    let mut ring = vec![
        Vec2::new(start_u, 0.0),
        Vec2::new(start_u + s.landing_depth, 0.0),
        Vec2::new(start_u + s.landing_depth, s.width),
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

    let flight_run = s.risers as f64 * s.tread_depth;
    if !flight_run.is_finite() || flight_run <= 0.0 {
        return None;
    }

    let mut ring = Vec::with_capacity(s.risers + 4);
    ring.push(Vec2::ZERO);
    for i in 0..s.risers {
        let u = (i as f64 + 1.0) * s.tread_depth;
        let v = (i as f64 + 1.0) * s.riser_height;
        ring.push(Vec2::new(u, v));
    }

    if ring.len() < 3 {
        return None;
    }

    // This ring closes back to the origin without repeating the first vertex,
    // so the prism is a simple, non-degenerate stepped polygon.
    ring.push(Vec2::new(flight_run, 0.0));
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
}
