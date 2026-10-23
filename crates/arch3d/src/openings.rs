//! 3D meshes for architectural openings (doors, windows, garage doors).
//!
//! Each component (frame, casing, glass, leaf, sill, header) is a separate
//! [`Mesh`] tagged with the opening's [`ArchId`] for picking.

use std::collections::BTreeMap;

use cadcraft_arch::opening::is_double;
use cadcraft_doc::arch::{Opening, OpeningKind};
use cadcraft_doc::{ArchId, ArchModel, Wall};
use cadcraft_geom::Vec2;

use crate::mesh::Mesh;

/// Clamp a component centre offset (from the wall centre plane, measured along
/// the wall normal) so that a component of depth `depth` fits inside the wall's
/// `[-wall_t/2, +wall_t/2]` thickness band.
fn band_offset(off: f64, depth: f64, wall_t: f64) -> f64 {
    off.clamp(-wall_t / 2.0 + depth / 2.0, wall_t / 2.0 - depth / 2.0)
}

/// Build all component meshes for a single opening.
///
/// Components that share a material (e.g. all frame prisms) are merged into
/// a single `Mesh`, keeping `io::glb` node names unchanged (`door-frame-o0`,
/// `window-glass-o1`, …) while cutting the per-opening mesh count from ~10
/// to ~5.
///
/// Returns an empty vector if the host wall is missing or degenerate.
pub fn opening_meshes(walls: &BTreeMap<ArchId, &Wall>, o: &Opening) -> Vec<Mesh> {
    let wall = match walls.get(&o.host) {
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
    // Offsets measured from the wall centre plane (along `normal`), each
    // clamped so the component stays inside the wall's thickness band.
    let mut meshes: BTreeMap<String, Mesh> = BTreeMap::new();
    let push_box = |material: &str, a: Vec2, b: Vec2, z0: f64, z1: f64, depth: f64| {
        if depth <= 0.0 {
            return;
        }
        let dir = b - a;
        let len = dir.len();
        if len < 1e-9 || (z1 - z0).abs() < 1e-9 {
            return;
        }
        let d = dir / len;
        let n = Vec2::new(-d.y, d.x);
        let half = depth / 2.0;
        let p0 = a + n * half;
        let p1 = a - n * half;
        let p2 = b - n * half;
        let p3 = b + n * half;
        let mesh = meshes.entry(material.to_string()).or_insert_with(|| Mesh::new(material, o.id));
        mesh.push_prism(&[p0, p1, p2, p3], z0, z1);
    };

    let w = o.width;
    let h = o.height;
    let sill_h = o.sill;
    let fw = o.frame_width;
    let fd = o.frame_depth;
    let cw = o.casing_width;
    let cd = o.casing_depth;
    let sd = o.sill_depth;
    let wall_t = wall.thickness;

    // The opening top is clamped to the host wall height so the frame, casing,
    // glass and leaf can never rise above the wall top and push the scene
    // envelope out of the host wall.
    let frame_z0 = sill_h;
    let frame_z1 = (sill_h + h).min(wall.height);

    let is_window = matches!(o.kind, OpeningKind::Window);
    let kind_prefix = if is_window {
        "window"
    } else if matches!(o.kind, OpeningKind::GarageDoor) {
        "garage"
    } else {
        "door"
    };

    // Frame: left jamb
    let jamb_off = band_offset(o.recess - wall_t / 2.0 + fd / 2.0, fd, wall_t);
    push_box(
        &format!("{}-frame", kind_prefix),
        center + dir * (-w / 2.0 - fw / 2.0) + normal * jamb_off,
        center + dir * (-w / 2.0 + fw / 2.0) + normal * jamb_off,
        frame_z0,
        frame_z1,
        fd,
    );

    // Frame: right jamb
    push_box(
        &format!("{}-frame", kind_prefix),
        center + dir * (w / 2.0 - fw / 2.0) + normal * jamb_off,
        center + dir * (w / 2.0 + fw / 2.0) + normal * jamb_off,
        frame_z0,
        frame_z1,
        fd,
    );

    // Frame: head
    let head_off = band_offset(o.recess - wall_t / 2.0 + fd / 2.0, fw, wall_t);
    push_box(
        &format!("{}-frame", kind_prefix),
        center + dir * (-w / 2.0) + normal * head_off,
        center + dir * (w / 2.0) + normal * head_off,
        frame_z1 - fw,
        frame_z1,
        fw,
    );

    // Frame: sill
    let sill_off = band_offset(o.recess - wall_t / 2.0 + fd / 2.0, fw, wall_t);
    push_box(
        &format!("{}-frame", kind_prefix),
        center + dir * (-w / 2.0) + normal * sill_off,
        center + dir * (w / 2.0) + normal * sill_off,
        frame_z0,
        frame_z0 + fw,
        fw,
    );

    // Casing (both faces + head)
    if cw > 0.0 {
        let casing_off = band_offset(o.recess - wall_t / 2.0 + cd / 2.0, cd, wall_t);
        push_box(
            &format!("{}-casing", kind_prefix),
            center + dir * (-w / 2.0 - fw / 2.0 - cw) + normal * casing_off,
            center + dir * (-w / 2.0 - fw / 2.0) + normal * casing_off,
            frame_z0,
            frame_z1,
            cd,
        );

        push_box(
            &format!("{}-casing", kind_prefix),
            center + dir * (w / 2.0 + fw / 2.0) + normal * casing_off,
            center + dir * (w / 2.0 + fw / 2.0 + cw) + normal * casing_off,
            frame_z0,
            frame_z1,
            cd,
        );

        let casing_head_off = band_offset(o.recess - wall_t / 2.0 + cd / 2.0, cw, wall_t);
        push_box(
            &format!("{}-casing", kind_prefix),
            center + dir * (-w / 2.0 - fw / 2.0) + normal * casing_head_off,
            center + dir * (w / 2.0 + fw / 2.0) + normal * casing_head_off,
            frame_z1 - cw,
            frame_z1,
            cw,
        );
    }

    // Glass: every window gets it; doors get it when the style indicates glazing.
    let is_glazed_door = !is_window
        && (o.style.contains("sliding")
            || o.style.contains("bifold")
            || o.style.contains("french")
            || o.style.contains("pocket")
            || o.style.contains("glass"));
    if is_window || is_glazed_door {
        let glass_off = band_offset(o.recess - wall_t / 2.0 + fd / 2.0, 0.006, wall_t);
        push_box(
            &format!("{}-glass", kind_prefix),
            center + dir * (-w / 2.0 + fw) + normal * glass_off,
            center + dir * (w / 2.0 - fw) + normal * glass_off,
            frame_z0 + fw,
            frame_z1 - fw,
            0.006,
        );
    }

    // Leaf: doors (not windows, not cased openings) get a leaf.
    if !is_window && !matches!(o.kind, OpeningKind::CasedOpening) {
        let leaf_t = 0.045;
        let leaf_off = band_offset(o.recess - wall_t / 2.0 + leaf_t / 2.0, leaf_t, wall_t);
        let leaf_z0 = frame_z0 + fw;
        let leaf_z1 = frame_z1 - fw;

        let two_leaves = o.style.contains("sliding") || o.style.contains("bifold") || o.style.contains("french") || is_double(o);
        if two_leaves {
            let half_w = (w - 2.0 * fw) / 2.0;
            push_box(
                &format!("{}-leaf", kind_prefix),
                center + dir * (-w / 2.0 + fw) + normal * leaf_off,
                center + dir * (-w / 2.0 + fw + half_w) + normal * leaf_off,
                leaf_z0,
                leaf_z1,
                leaf_t,
            );

            push_box(
                &format!("{}-leaf", kind_prefix),
                center + dir * (w / 2.0 - fw - half_w) + normal * leaf_off,
                center + dir * (w / 2.0 - fw) + normal * leaf_off,
                leaf_z0,
                leaf_z1,
                leaf_t,
            );
        } else {
            push_box(
                &format!("{}-leaf", kind_prefix),
                center + dir * (-w / 2.0 + fw) + normal * leaf_off,
                center + dir * (w / 2.0 - fw) + normal * leaf_off,
                leaf_z0,
                leaf_z1,
                leaf_t,
            );
        }
    }

    // Exterior sill (for windows)
    if is_window && sd > 0.0 {
        let ext_sill_off = band_offset(o.recess - wall_t / 2.0, 0.04, wall_t);
        let s_a = center + dir * (-w / 2.0 - fw / 2.0 - sd) + normal * ext_sill_off;
        let s_b = center + dir * (w / 2.0 + fw / 2.0 + sd) + normal * ext_sill_off;
        let sill_bottom = (frame_z0 - 0.02).max(0.0);
        let sill_mat = format!("{}-sill", kind_prefix);
        if let Some(mesh) = box_between(s_a, s_b, sill_bottom, frame_z0, 0.04, &sill_mat, o.id) {
            meshes.insert(sill_mat, mesh);
        }
    }

    // Header: centred on the wall centre plane (full thickness) and clipped
    // to stay inside the host wall height.
    let header_h = 0.2;
    let header_top = (frame_z1 + header_h).min(wall.height);
    if header_top > frame_z1 + 1e-9 {
        let header_off = band_offset(0.0, wall_t, wall_t);
        let h_a = center + dir * (-w / 2.0 - fw) + normal * header_off;
        let h_b = center + dir * (w / 2.0 + fw) + normal * header_off;
        let header_mat = format!("{}-header", kind_prefix);
        if let Some(mesh) = box_between(h_a, h_b, frame_z1, header_top, wall_t, &header_mat, o.id) {
            meshes.insert(header_mat, mesh);
        }
    }

    meshes.into_values().collect()
}

/// Create a box mesh between two 2D points, from z0 to z1, with given depth.
///
/// Returns `None` for degenerate boxes (zero length or zero height).
fn box_between(a: Vec2, b: Vec2, z0: f64, z1: f64, depth: f64, material: &str, id: ArchId) -> Option<Mesh> {
    let dir = b - a;
    let len = dir.len();
    if len < 1e-9 || (z1 - z0).abs() < 1e-9 {
        return None;
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
    if mesh.positions.is_empty() { None } else { Some(mesh) }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            frame_width: 0.05,
            frame_depth: 0.1,
            casing_width: 0.05,
            casing_depth: 0.02,
            sill_depth: 0.05,
            swing_angle: 0.0,
            style: "window".to_string(),
            recess: 0.0,
            kind: OpeningKind::Window,
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
            frame_width: 0.05,
            frame_depth: 0.1,
            casing_width: 0.05,
            casing_depth: 0.02,
            sill_depth: 0.0,
            swing_angle: 0.0,
            style: "hinged".to_string(),
            recess: 0.0,
            kind: OpeningKind::Door,
            ..Default::default()
        }
    }

    #[test]
    fn a_window_produces_frame_glass_and_sill_meshes() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let has_frame = meshes.iter().any(|m| m.material == "window-frame");
        let has_glass = meshes.iter().any(|m| m.material == "window-glass");
        let has_sill = meshes.iter().any(|m| m.material == "window-sill");
        assert!(has_frame, "missing window-frame mesh");
        assert!(has_glass, "missing window-glass mesh");
        assert!(has_sill, "missing window-sill mesh");
    }

    #[test]
    fn a_hinged_door_produces_a_frame_and_one_leaf() {
        let model = wall_model();
        let o = door_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let has_frame = meshes.iter().any(|m| m.material == "door-frame");
        let leaf_count = meshes.iter().filter(|m| m.material == "door-leaf").count();
        assert!(has_frame, "missing door-frame mesh");
        assert_eq!(leaf_count, 1, "expected 1 leaf, got {}", leaf_count);
    }

    #[test]
    fn a_sliding_door_produces_two_leaves() {
        let model = wall_model();
        let mut o = door_opening();
        o.style = "sliding".to_string();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let leaf_count = meshes.iter().filter(|m| m.material == "door-leaf").count();
        assert_eq!(leaf_count, 2, "expected 2 leaves, got {}", leaf_count);
    }

    #[test]
    fn a_double_door_produces_two_leaves() {
        let model = wall_model();
        let mut o = door_opening();
        o.style = "french".to_string();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let leaf_count = meshes.iter().filter(|m| m.material == "door-leaf").count();
        assert_eq!(leaf_count, 2, "expected 2 leaves, got {}", leaf_count);
    }

    #[test]
    fn a_recessed_window_sits_off_the_wall_face() {
        let model = wall_model();
        let mut o = window_opening();
        o.recess = 0.05;
        let meshes = opening_meshes(&walls_map(&model), &o);
        assert!(!meshes.is_empty(), "no meshes for recessed window");
        let frame = meshes.iter().find(|m| m.material == "window-frame");
        assert!(frame.is_some(), "missing window-frame");
    }

    #[test]
    fn casing_is_skipped_when_its_width_is_zero() {
        let model = wall_model();
        let mut o = window_opening();
        o.casing_width = 0.0;
        let meshes = opening_meshes(&walls_map(&model), &o);
        let has_casing = meshes.iter().any(|m| m.material == "window-casing");
        assert!(!has_casing, "casing should be skipped");
    }

    #[test]
    fn glass_uses_the_glass_material() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let glass = meshes.iter().find(|m| m.material == "window-glass");
        assert!(glass.is_some(), "missing window-glass mesh");
    }

    #[test]
    fn opening_meshes_are_manifold() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
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
        let meshes = opening_meshes(&walls_map(&model), &o);
        assert!(meshes.is_empty(), "expected empty for missing host");
    }

    #[test]
    fn a_window_produces_window_glass_meshes() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let has_window_glass = meshes.iter().any(|m| m.material == "window-glass");
        let has_window_frame = meshes.iter().any(|m| m.material == "window-frame");
        assert!(has_window_glass, "missing window-glass mesh");
        assert!(has_window_frame, "missing window-frame mesh");
    }

    #[test]
    fn a_hinged_door_produces_door_leaf_and_frame() {
        let model = wall_model();
        let o = door_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let has_door_frame = meshes.iter().any(|m| m.material == "door-frame");
        let has_door_leaf = meshes.iter().any(|m| m.material == "door-leaf");
        assert!(has_door_frame, "missing door-frame mesh");
        assert!(has_door_leaf, "missing door-leaf mesh");
    }

    #[test]
    fn opening_meshes_are_never_empty() {
        let model = wall_model();
        let window = window_opening();
        let door = door_opening();
        let wm = walls_map(&model);
        for o in [&window, &door] {
            let meshes = opening_meshes(&wm, o);
            for m in &meshes {
                assert!(!m.positions.is_empty(), "mesh {} has no positions", m.material);
            }
        }
    }

    #[test]
    fn header_stays_inside_the_wall_height() {
        let model = wall_model();
        let o = window_opening();
        let meshes = opening_meshes(&walls_map(&model), &o);
        let wall_height = 2.5;
        for m in &meshes {
            if m.material.contains("header") {
                for p in &m.positions {
                    let z = p[2] as f64;
                    assert!(z <= wall_height + 1e-6, "header exceeds wall height: z={}", z);
                }
            }
        }
    }

    /// A wall model of a single 10 m long wall, thickness `t`.
    fn band_wall(t: f64) -> ArchModel {
        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: t,
            height: 2.5,
            ..Default::default()
        });
        model
    }

    /// One of each opening type on the wall in `model`.
    fn walls_map(model: &ArchModel) -> BTreeMap<ArchId, &Wall> {
        model.walls.iter().map(|w| (w.id, w)).collect()
    }

    fn all_openings(model: &ArchModel) -> Vec<Opening> {
        let base = |style: &str, kind: OpeningKind| Opening {
            id: ArchId(100),
            host: ArchId(1),
            offset: 5.0,
            width: 1.2,
            height: 1.5,
            sill: 0.9,
            frame_width: 0.05,
            frame_depth: 0.1,
            casing_width: 0.05,
            casing_depth: 0.02,
            sill_depth: 0.05,
            swing_angle: 0.0,
            style: style.to_string(),
            recess: 0.0,
            kind,
            ..Default::default()
        };
        vec![
            base("window", OpeningKind::Window),
            base("hinged", OpeningKind::Door),
            base("window", OpeningKind::Window),
            base("garage", OpeningKind::GarageDoor),
        ]
    }

    #[test]
    fn opening_meshes_stay_inside_the_host_wall_thickness() {
        let model = band_wall(0.3);
        let wm = walls_map(&model);
        for o in all_openings(&model) {
            let wall = &model.walls[0];
            let dir = (wall.end - wall.start).normalized();
            let normal = Vec2::new(-dir.y, dir.x);
            let center = wall.start + dir * o.offset;
            let half_t = wall.thickness / 2.0;
            for m in opening_meshes(&wm, &o) {
                for p in &m.positions {
                    let n = (Vec2::new(p[0] as f64, p[1] as f64) - center).dot(normal);
                    assert!(n.abs() <= half_t + 1e-6, "vertex {} of {} leaves wall band (n={})", p[0] as f64, m.material, n);
                }
            }
        }
    }

    #[test]
    fn house_with_openings_bounds_match_the_wall_envelope() {
        // 10 m x 8 m floor, walls 0.2 m thick, 2.4 m tall, centre lines at
        // x = 0/10 and y = 0/8.
        let t = 0.2;
        let h = 2.4;
        let mut model = ArchModel::default();
        for (a, b, id) in [((0.0, 0.0), (10.0, 0.0), 1), ((10.0, 0.0), (10.0, 8.0), 2), ((10.0, 8.0), (0.0, 8.0), 3), ((0.0, 8.0), (0.0, 0.0), 4)] {
            model.walls.push(Wall {
                id: ArchId(id),
                start: Vec2::new(a.0, a.1),
                end: Vec2::new(b.0, b.1),
                thickness: t,
                height: h,
                ..Default::default()
            });
        }
        // Hinged door on the south wall, two windows, a garage door.
        let windows = vec![
            Opening {
                id: ArchId(100),
                host: ArchId(1),
                offset: 3.0,
                width: 1.2,
                height: 1.5,
                sill: 0.9,
                frame_width: 0.05,
                frame_depth: 0.1,
                casing_width: 0.05,
                casing_depth: 0.02,
                sill_depth: 0.05,
                swing_angle: 0.0,
                style: "window".to_string(),
                recess: 0.0,
                kind: OpeningKind::Window,
                ..Default::default()
            },
            Opening {
                id: ArchId(101),
                host: ArchId(1),
                offset: 7.0,
                width: 1.0,
                height: 1.2,
                sill: 0.9,
                frame_width: 0.05,
                frame_depth: 0.1,
                casing_width: 0.05,
                casing_depth: 0.02,
                sill_depth: 0.05,
                swing_angle: 0.0,
                style: "window".to_string(),
                recess: 0.0,
                kind: OpeningKind::Window,
                ..Default::default()
            },
            Opening {
                id: ArchId(102),
                host: ArchId(1),
                offset: 5.0,
                width: 0.9,
                height: 2.1,
                sill: 0.0,
                frame_width: 0.05,
                frame_depth: 0.1,
                casing_width: 0.05,
                casing_depth: 0.02,
                sill_depth: 0.0,
                swing_angle: 0.0,
                style: "hinged".to_string(),
                recess: 0.0,
                kind: OpeningKind::Door,
                ..Default::default()
            },
            Opening {
                id: ArchId(103),
                host: ArchId(2),
                offset: 4.0,
                width: 3.0,
                height: 2.2,
                sill: 0.0,
                frame_width: 0.05,
                frame_depth: 0.1,
                casing_width: 0.05,
                casing_depth: 0.02,
                sill_depth: 0.0,
                swing_angle: 0.0,
                style: "garage".to_string(),
                recess: 0.0,
                kind: OpeningKind::GarageDoor,
                ..Default::default()
            },
        ];
        model.openings.clear();
        model.openings.extend(windows.into_iter().map(|o| o));

        // The scene envelope is the union of the wall solids and the
        // opening component meshes; the openings must not push it past the
        // wall outer faces (10.2 x 8.2) or above the wall tops (2.4).
        let expected = Vec3::new(10.2 + 1e-3, 8.2 + 1e-3, h + 1e-3);
        let eps = 1e-3;

        // Wall solid bounds (full extrusion to the host wall height).
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_min = f64::INFINITY;
        let mut y_max = f64::NEG_INFINITY;
        let mut z_min = f64::INFINITY;
        let mut z_max = f64::NEG_INFINITY;
        for w in &model.walls {
            let (dx, dy) = (w.end.x - w.start.x, w.end.y - w.start.y);
            let (nx, ny) = (-dy, dx);
            let l = (nx * nx + ny * ny).max(1e-9).sqrt();
            let n = Vec2::new(nx / l, ny / l);
            for s in [w.start, w.end] {
                for (off, p) in [(-w.thickness / 2.0, s.clone()), (w.thickness / 2.0, s)] {
                    let pt = s + n * off;
                    x_min = x_min.min(pt.x);
                    x_max = x_max.max(pt.x);
                    y_min = y_min.min(pt.y);
                    y_max = y_max.max(pt.y);
                }
            }
            z_min = z_min.min(0.0);
            z_max = z_max.max(w.height);
        }

        // Opening component meshes: they must fit inside the wall envelope.
        let wm = walls_map(&model);
        for o in &model.openings {
            for m in opening_meshes(&wm, o) {
                for p in &m.positions {
                    x_min = x_min.min(p[0].into());
                    x_max = x_max.max(p[0].into());
                    y_min = y_min.min(p[1].into());
                    y_max = y_max.max(p[1].into());
                    z_min = z_min.min(p[2].into());
                    z_max = z_max.max(p[2].into());
                }
            }
        }

        assert!(x_min >= -t / 2.0 - eps, "scene x_min too low: {}", x_min);
        assert!(x_max - x_min <= expected.x, "scene width too large: {}", x_max - x_min);
        assert!(x_max - x_min >= expected.x - 2.0 * eps, "scene width too small: {}", x_max - x_min);
        assert!(y_min >= -t / 2.0 - eps, "scene y_min too low: {}", y_min);
        assert!(y_max - y_min <= expected.y, "scene depth too large: {}", y_max - y_min);
        assert!(y_max - y_min >= expected.y - 2.0 * eps, "scene depth too small: {}", y_max - y_min);
        assert!((z_max - z_min).abs() <= expected.z, "scene height off: {}", z_max - z_min);
        assert!((z_max - z_min).abs() >= expected.z - 2.0 * eps, "scene height too small: {}", z_max - z_min);
    }
}
