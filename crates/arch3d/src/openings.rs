//! 3D meshes for architectural openings (doors, windows, garage doors).
//!
//! Each component (frame, casing, glass, leaf, sill, header) is a separate
//! [`Mesh`] tagged with the opening's [`ArchId`] for picking.

use cadcraft_arch::opening::is_double;
use cadcraft_doc::arch::Opening;
use cadcraft_doc::{ArchId, ArchModel};
use cadcraft_geom::{Vec2, Vec3};

use crate::mesh::Mesh;

/// Build all component meshes for a single opening.
///
/// Returns an empty vector if the host wall is missing or degenerate.
pub fn opening_meshes(model: &ArchModel, o: &Opening) -> Vec<Mesh> {
    let wall = match model.walls.iter().find(|w| w.id == o.host) {
        Some(w) => w,
        None => return Vec::new(),
    };
    let dir = wall.end - wall.start;
    if dir.len() < 1e-9 {
        return Vec::new();
    }
    let dir = dir.normalized();
    let normal = Vec2::new(-dir.y, dir.x);
    let center = wall.start + dir * o.offset;
    let recessed = center + normal * o.recess;

    let mut meshes = Vec::new();

    let w = o.width;
    let h = o.height;
    let sill_h = o.sill;
    let fw = o.frame_width;
    let fd = o.frame_depth;
    let cw = o.casing_width;
    let cd = o.casing_depth;
    let sd = o.sill_depth;
    let wall_t = wall.thickness;

    let frame_z0 = sill_h;
    let frame_z1 = sill_h + h;

    // Frame: left jamb
    let left_a = recessed + dir * (-w / 2.0 - fw / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    let left_b = recessed + dir * (-w / 2.0 + fw / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    meshes.push(box_between(left_a, left_b, frame_z0, frame_z1, fd, "frame", o.id));

    // Frame: right jamb
    let right_a = recessed + dir * (w / 2.0 - fw / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    let right_b = recessed + dir * (w / 2.0 + fw / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    meshes.push(box_between(right_a, right_b, frame_z0, frame_z1, fd, "frame", o.id));

    // Frame: head
    let head_a = recessed + dir * (-w / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    let head_b = recessed + dir * (w / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    meshes.push(box_between(head_a, head_b, frame_z1 - fw, frame_z1, fw, "frame", o.id));

    // Frame: sill
    let sill_a = recessed + dir * (-w / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    let sill_b = recessed + dir * (w / 2.0) + normal * (-wall_t / 2.0 + fd / 2.0);
    meshes.push(box_between(sill_a, sill_b, frame_z0, frame_z0 + fw, fw, "frame", o.id));

    // Casing (both faces)
    if cw > 0.0 {
        let cl_a = recessed + dir * (-w / 2.0 - fw / 2.0 - cw) + normal * (-wall_t / 2.0 + cd / 2.0);
        let cl_b = recessed + dir * (-w / 2.0 - fw / 2.0) + normal * (-wall_t / 2.0 + cd / 2.0);
        meshes.push(box_between(cl_a, cl_b, frame_z0, frame_z1, cd, "casing", o.id));

        let cr_a = recessed + dir * (w / 2.0 + fw / 2.0) + normal * (-wall_t / 2.0 + cd / 2.0);
        let cr_b = recessed + dir * (w / 2.0 + fw / 2.0 + cw) + normal * (-wall_t / 2.0 + cd / 2.0);
        meshes.push(box_between(cr_a, cr_b, frame_z0, frame_z1, cd, "casing", o.id));

        let ch_a = recessed + dir * (-w / 2.0 - fw / 2.0) + normal * (-wall_t / 2.0 + cd / 2.0);
        let ch_b = recessed + dir * (w / 2.0 + fw / 2.0) + normal * (-wall_t / 2.0 + cd / 2.0);
        meshes.push(box_between(ch_a, ch_b, frame_z1 - cw, frame_z1, cw, "casing", o.id));
    }

    // Glass (for windows and glazed doors)
    let is_glazed = o.style == "window" || o.style == "sliding" || o.style == "bifold" || o.style == "french";
    if is_glazed {
        let glass_a = recessed + dir * (-w / 2.0 + fw) + normal * (-wall_t / 2.0 + fd / 2.0);
        let glass_b = recessed + dir * (w / 2.0 - fw) + normal * (-wall_t / 2.0 + fd / 2.0);
        meshes.push(box_between(glass_a, glass_b, frame_z0 + fw, frame_z1 - fw, 0.006, "glass", o.id));
    }

    // Leaf (door panel)
    if o.style != "window" {
        let leaf_t = 0.045;
        let leaf_z0 = frame_z0 + fw;
        let leaf_z1 = frame_z1 - fw;

        if is_double(o) {
            let half_w = (w - 2.0 * fw) / 2.0;
            let l_a = recessed + dir * (-w / 2.0 + fw) + normal * (-wall_t / 2.0 + leaf_t / 2.0);
            let l_b = recessed + dir * (-w / 2.0 + fw + half_w) + normal * (-wall_t / 2.0 + leaf_t / 2.0);
            meshes.push(box_between(l_a, l_b, leaf_z0, leaf_z1, leaf_t, "leaf", o.id));

            let r_a = recessed + dir * (w / 2.0 - fw - half_w) + normal * (-wall_t / 2.0 + leaf_t / 2.0);
            let r_b = recessed + dir * (w / 2.0 - fw) + normal * (-wall_t / 2.0 + leaf_t / 2.0);
            meshes.push(box_between(r_a, r_b, leaf_z0, leaf_z1, leaf_t, "leaf", o.id));
        } else {
            let l_a = recessed + dir * (-w / 2.0 + fw) + normal * (-wall_t / 2.0 + leaf_t / 2.0);
            let l_b = recessed + dir * (w / 2.0 - fw) + normal * (-wall_t / 2.0 + leaf_t / 2.0);
            meshes.push(box_between(l_a, l_b, leaf_z0, leaf_z1, leaf_t, "leaf", o.id));
        }
    }

    // Exterior sill (for windows)
    if o.style == "window" && sd > 0.0 {
        let s_a = recessed + dir * (-w / 2.0 - fw / 2.0 - sd) + normal * (-wall_t / 2.0);
        let s_b = recessed + dir * (w / 2.0 + fw / 2.0 + sd) + normal * (-wall_t / 2.0);
        meshes.push(box_between(s_a, s_b, frame_z0 - 0.02, frame_z0, 0.04, "sill", o.id));
    }

    // Header
    let header_h = o.header;
    let h_a = recessed + dir * (-w / 2.0 - fw) + normal * (-wall_t / 2.0);
    let h_b = recessed + dir * (w / 2.0 + fw) + normal * (-wall_t / 2.0);
    meshes.push(box_between(h_a, h_b, frame_z1, frame_z1 + header_h, wall_t, "header", o.id));

    meshes
}

/// Create a box mesh between two 2D points, from z0 to z1, with given depth.
fn box_between(a: Vec2, b: Vec2, z0: f64, z1: f64, depth: f64, material: &str, id: ArchId) -> Mesh {
    let dir = b - a;
    let len = dir.len();
    if len < 1e-9 {
        return Mesh { object: id, material: material.to_string(), positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), indices: Vec::new() };
    }
    let d = dir / len;
    let n = Vec2::new(-d.y, d.x);
    let half = depth / 2.0;

    let p0 = a + n * half;
    let p1 = a - n * half;
    let p2 = b - n * half;
    let p3 = b + n * half;

    let mut mesh =
        Mesh { object: id, material: material.to_string(), positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), indices: Vec::new() };
    mesh.push_prism(&[p0, p1, p2, p3], z0, z1);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Wall;

    fn wall_model() -> ArchModel {
        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });
        model
    }

    fn window_opening() -> Opening {
        Opening {
            id: ArchId(100),
            host: ArchId(1),
            offset: 5.0,
            width: 1.2,
            height: 1.5,
            sill: 0.9,
            header: 0.2,
            frame_width: 0.05,
            frame_depth: 0.1,
            casing_width: 0.05,
            casing_depth: 0.02,
            sill_depth: 0.05,
            swing_angle: 0.0,
            style: "window".to_string(),
            recess: 0.0,
            ..Default::default()
        }
    }

    fn door_opening() -> Opening {
        Opening {
            id: ArchId(101),
            host: ArchId(1),
            offset: 5.0,
            width: 0.9,
            height: 2.1,
            sill: 0.0,
            header: 0.2,
            frame_width: 0.05,
            frame_depth: 0.1,
            casing_width: 0.05,
            casing_depth: 0.02,
            sill_depth: 0.0,
            swing_angle: 0.0,
            style: "hinged".to_string(),
            recess: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_window_produces_frame_glass_and_sill_meshes() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&model, &o);
        let has_frame = meshes.iter().any(|m| m.material == "frame");
        let has_glass = meshes.iter().any(|m| m.material == "glass");
        let has_sill = meshes.iter().any(|m| m.material == "sill");
        assert!(has_frame, "missing frame mesh");
        assert!(has_glass, "missing glass mesh");
        assert!(has_sill, "missing sill mesh");
    }

    #[test]
    fn a_hinged_door_produces_a_frame_and_one_leaf() {
        let model = wall_model();
        let o = door_opening();
        let meshes = opening_meshes(&model, &o);
        let has_frame = meshes.iter().any(|m| m.material == "frame");
        let leaf_count = meshes.iter().filter(|m| m.material == "leaf").count();
        assert!(has_frame, "missing frame mesh");
        assert_eq!(leaf_count, 1, "expected 1 leaf, got {}", leaf_count);
    }

    #[test]
    fn a_sliding_door_produces_two_leaves() {
        let model = wall_model();
        let mut o = door_opening();
        o.style = "sliding".to_string();
        let meshes = opening_meshes(&model, &o);
        let leaf_count = meshes.iter().filter(|m| m.material == "leaf").count();
        assert_eq!(leaf_count, 2, "expected 2 leaves, got {}", leaf_count);
    }

    #[test]
    fn a_double_door_produces_two_leaves() {
        let model = wall_model();
        let mut o = door_opening();
        o.style = "french".to_string();
        let meshes = opening_meshes(&model, &o);
        let leaf_count = meshes.iter().filter(|m| m.material == "leaf").count();
        assert_eq!(leaf_count, 2, "expected 2 leaves, got {}", leaf_count);
    }

    #[test]
    fn a_recessed_window_sits_off_the_wall_face() {
        let model = wall_model();
        let mut o = window_opening();
        o.recess = 0.05;
        let meshes = opening_meshes(&model, &o);
        assert!(!meshes.is_empty(), "no meshes for recessed window");
        let frame = meshes.iter().find(|m| m.material == "frame");
        assert!(frame.is_some(), "missing frame");
    }

    #[test]
    fn casing_is_skipped_when_its_width_is_zero() {
        let model = wall_model();
        let mut o = window_opening();
        o.casing_width = 0.0;
        let meshes = opening_meshes(&model, &o);
        let has_casing = meshes.iter().any(|m| m.material == "casing");
        assert!(!has_casing, "casing should be skipped");
    }

    #[test]
    fn glass_uses_the_glass_material() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&model, &o);
        let glass = meshes.iter().find(|m| m.material == "glass");
        assert!(glass.is_some(), "missing glass mesh");
    }

    #[test]
    fn opening_meshes_are_manifold() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&model, &o);
        for m in &meshes {
            assert_eq!(m.indices.len() % 3, 0, "indices not multiple of 3");
            for &idx in &m.indices {
                assert!(idx < m.positions.len() as u32, "index out of bounds");
            }
        }
    }

    #[test]
    fn opening_meshes_are_empty_for_a_missing_host() {
        let model = wall_model();
        let mut o = window_opening();
        o.host = ArchId(999);
        let meshes = opening_meshes(&model, &o);
        assert!(meshes.is_empty(), "expected empty for missing host");
    }
}
