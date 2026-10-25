//! 3D scene builder for architectural models.
//!
//! Converts a 2D [`ArchModel`] into a [`Scene3d`] of extruded wall meshes.
//! The builder is pure, deterministic, and incremental: `build` solves joins
//! once and extrudes every wall; `build_dirty` reuses unchanged meshes and
//! only re-extrudes walls whose id is in the `dirty` set.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod floors;
pub mod mesh;
pub mod openings;
pub mod probe;
pub mod ray;
pub mod walls;

pub use crate::mesh::{Bounds3, Mesh};
pub use cadcraft_doc::ArchId;

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_arch::join::solve_joins;
use cadcraft_doc::{ArchModel, Wall};
use cadcraft_geom::{Vec2, Vec3};

/// Build a simple extruded-rectangle mesh from a wall's raw start/end/thickness.
///
/// Used as a fallback when the join solver produced no shape for a wall id.
/// Returns `None` if the wall is degenerate (zero length).
fn raw_wall_mesh(wall: &Wall, base: f64, top: f64, object: ArchId) -> Option<Mesh> {
    let dx = wall.end.x - wall.start.x;
    let dy = wall.end.y - wall.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let half = wall.thickness / 2.0;
    let p0 = Vec2::new(wall.start.x + nx * half, wall.start.y + ny * half);
    let p1 = Vec2::new(wall.end.x + nx * half, wall.end.y + ny * half);
    let p2 = Vec2::new(wall.end.x - nx * half, wall.end.y - ny * half);
    let p3 = Vec2::new(wall.start.x - nx * half, wall.start.y - ny * half);

    let pos_f64: Vec<Vec3> = vec![
        Vec3::new(p0.x, p0.y, base),
        Vec3::new(p1.x, p1.y, base),
        Vec3::new(p2.x, p2.y, base),
        Vec3::new(p3.x, p3.y, base),
        Vec3::new(p0.x, p0.y, top),
        Vec3::new(p1.x, p1.y, top),
        Vec3::new(p2.x, p2.y, top),
        Vec3::new(p3.x, p3.y, top),
    ];

    let cx = (p0.x + p1.x + p2.x + p3.x) * 0.25;
    let cy = (p0.y + p1.y + p2.y + p3.y) * 0.25;
    let cz = (base + top) * 0.5;
    let normals_f64: Vec<Vec3> = pos_f64
        .iter()
        .map(|p| {
            let vx = p.x - cx;
            let vy = p.y - cy;
            let vz = p.z - cz;
            let l = (vx * vx + vy * vy + vz * vz).sqrt();
            if l < 1e-12 { Vec3::new(0.0, 0.0, 1.0) } else { Vec3::new(vx / l, vy / l, vz / l) }
        })
        .collect();

    let positions: Vec<[f32; 3]> = pos_f64.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
    let normals: Vec<[f32; 3]> = normals_f64.iter().map(|n| [n.x as f32, n.y as f32, n.z as f32]).collect();
    let uvs: Vec<[f32; 2]> = pos_f64.iter().map(|p| [p.x as f32, p.z as f32]).collect();
    let indices: Vec<u32> = vec![
        0, 1, 2, 0, 2, 3, // bottom
        4, 5, 6, 4, 6, 7, // top
        1, 2, 7, 1, 7, 6, // front
        3, 0, 4, 3, 4, 7, // back
        0, 1, 5, 0, 5, 4, // left
        2, 3, 4, 2, 4, 6, // right
    ];

    Some(Mesh { positions, normals, uvs, indices, ..Mesh::new("wall", object) })
}

/// The vertical span `(base, top)` for a wall, derived from its level.
///
/// `base` is the level's `elevation`; `top` is `base + (ceiling_height if
/// `to_ceiling` else `height)`. Returns `None` when the span is degenerate
/// (non-finite, or `top <= base`) — such walls are skipped.
pub fn wall_span(model: &ArchModel, w: &Wall) -> Option<(f64, f64)> {
    let level = model.levels.iter().rev().find(|l| l.index == w.base_level).or(model.levels.last());
    let base = level.map(|l| l.elevation).unwrap_or(0.0);
    let h = if w.to_ceiling { level.map(|l| l.ceiling_height).unwrap_or(w.height) } else { w.height };
    let top = base + h;
    if !base.is_finite() || !top.is_finite() || top <= base {
        return None;
    }
    Some((base, top))
}

/// The [`Level`] corresponding to `index`, preferring the last level
/// (or a default) when no level with that index exists.
fn level_for(model: &ArchModel, index: u32) -> cadcraft_doc::arch::Level {
    model.levels.iter().rev().find(|l| l.index == index).cloned().unwrap_or_else(|| model.levels.last().cloned().unwrap_or_default())
}

/// A 3D scene composed of extruded architectural meshes.
///
/// Build one with `Scene3d { meshes, ..Default::default() }` (or [`Scene3d::build`]), never an
/// exhaustive literal: other crates' tests construct scenes and must keep compiling when a field
/// is added here.
#[derive(Debug, Clone, Default)]
pub struct Scene3d {
    pub meshes: Vec<Mesh>,
    pub bounds: Bounds3,
    pub revision: u64,
    pub openings_built: usize,
    pub floors: usize,
    pub ceilings: usize,
}

impl Scene3d {
    /// Number of meshes in the scene.
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Total number of triangles across all meshes.
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.triangle_count()).sum()
    }

    /// Find a mesh by its object id (arch object).
    pub fn mesh_for(&self, id: &ArchId) -> Option<&Mesh> {
        self.meshes.iter().find(|m| &m.object == id)
    }

    /// Probe the scene at point `p`, returning the first mesh that contains it.
    ///
    /// Inherent impl lives here because `Scene3d` is defined in `lib.rs`;
    /// the free function `probe()` stays in `probe.rs`.
    pub fn probe(&self, p: Vec3) -> Option<crate::probe::Hit> {
        crate::probe::probe(self, p)
    }

    /// Build the full 3D scene from an arch model.
    ///
    /// Solves joins once, then extrudes each wall (falling back to a raw
    /// rectangular mesh when the join solver produced no shape for a wall).
    pub fn build(model: &ArchModel) -> Scene3d {
        let shapes = solve_joins(model);
        let shape_by_id: BTreeMap<ArchId, usize> = shapes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let mut meshes: Vec<Mesh> = Vec::with_capacity(model.walls.len());

        for wall in &model.walls {
            let Some((base, top)) = wall_span(model, wall) else {
                continue;
            };
            let height = top - base;
            let mesh = match shape_by_id.get(&wall.id).and_then(|&i| shapes.get(i)) {
                Some(shape) => walls::extrude_wall(shape, height, base, "wall"),
                None => raw_wall_mesh(wall, base, top, wall.id),
            };
            if let Some(m) = mesh {
                meshes.push(m);
            }
        }

        let mut openings_built = 0;
        let wall_map: BTreeMap<ArchId, &Wall> = model.walls.iter().map(|w| (w.id, w)).collect();
        for opening in &model.openings {
            let span = wall_map.get(&opening.host).and_then(|w| wall_span(model, w));
            let o_meshes: Vec<Mesh> = openings::opening_meshes(&wall_map, opening, span).into_iter().filter(|m| !m.positions.is_empty()).collect();
            if !o_meshes.is_empty() {
                openings_built += 1;
                meshes.extend(o_meshes);
            }
        }

        let mut floors = 0;
        let mut ceilings = 0;
        for room in &model.rooms {
            let level = level_for(model, room.level);
            if let Some(m) = floors::floor_slab(room, &level, "floor") {
                meshes.push(m);
                floors += 1;
            }
            if let Some(m) = floors::ceiling_surface(room, &level, "ceiling") {
                meshes.push(m);
                ceilings += 1;
            }
        }

        meshes.retain(|m| !m.positions.is_empty());

        let bounds = meshes.iter().fold(Bounds3::empty(), |mut b, m| {
            b.extend(&m.bounds());
            b
        });

        Scene3d { meshes, bounds, revision: 0, openings_built, floors, ceilings }
    }

    /// Incremental rebuild: reuses meshes for ids not in `dirty`, recomputes bounds.
    pub fn build_dirty(model: &ArchModel, dirty: &BTreeSet<ArchId>, prev: &Scene3d) -> Scene3d {
        let shapes = solve_joins(model);
        let shape_by_id: BTreeMap<ArchId, usize> = shapes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let mut meshes: Vec<Mesh> = Vec::with_capacity(model.walls.len());

        for wall in &model.walls {
            if !dirty.contains(&wall.id) {
                if let Some(mesh) = prev.mesh_for(&wall.id) {
                    meshes.push(mesh.clone());
                    continue;
                }
            }
            let Some((base, top)) = wall_span(model, wall) else {
                continue;
            };
            let mesh = match shape_by_id.get(&wall.id).and_then(|&i| shapes.get(i)) {
                Some(shape) => walls::extrude_wall(shape, top - base, base, "wall"),
                None => raw_wall_mesh(wall, base, top, wall.id),
            };
            if let Some(m) = mesh {
                meshes.push(m);
            }
        }

        let mut openings_built = 0;
        let wall_map: BTreeMap<ArchId, &Wall> = model.walls.iter().map(|w| (w.id, w)).collect();
        let prev_by_object: BTreeMap<ArchId, Vec<Mesh>> = prev.meshes.iter().fold(BTreeMap::new(), |mut m, mesh| {
            m.entry(mesh.object).or_default().push(mesh.clone());
            m
        });
        for opening in &model.openings {
            let span = wall_map.get(&opening.host).and_then(|w| wall_span(model, w));
            if dirty.contains(&opening.id) {
                let o_meshes: Vec<Mesh> =
                    openings::opening_meshes(&wall_map, opening, span).into_iter().filter(|m| !m.positions.is_empty()).collect();
                if !o_meshes.is_empty() {
                    openings_built += 1;
                    meshes.extend(o_meshes);
                }
            } else {
                if let Some(om) = prev_by_object.get(&opening.id) {
                    openings_built += 1;
                    meshes.extend(om.iter().map(|m| m.clone()));
                }
            }
        }

        let mut floors = 0;
        let mut ceilings = 0;
        for room in &model.rooms {
            let level = level_for(model, room.level);
            if dirty.contains(&room.id) {
                if let Some(m) = floors::floor_slab(room, &level, "floor") {
                    meshes.push(m);
                    floors += 1;
                }
                if let Some(m) = floors::ceiling_surface(room, &level, "ceiling") {
                    meshes.push(m);
                    ceilings += 1;
                }
            } else {
                if let Some(om) = prev_by_object.get(&room.id) {
                    for m in om {
                        if m.material == "floor" {
                            meshes.push(m.clone());
                            floors += 1;
                        } else if m.material == "ceiling" {
                            meshes.push(m.clone());
                            ceilings += 1;
                        }
                    }
                }
            }
        }

        meshes.retain(|m| !m.positions.is_empty());

        let bounds = meshes.iter().fold(Bounds3::empty(), |mut b, m| {
            b.extend(&m.bounds());
            b
        });

        Scene3d { meshes, bounds, revision: 0, openings_built, floors, ceilings }
    }
}

/// Build the full 3D scene from an arch model.
///
/// Top-level convenience wrapping [`Scene3d::build`].
pub fn build(model: &ArchModel) -> Scene3d {
    Scene3d::build(model)
}

/// Incremental rebuild that reuses unchanged meshes.
///
/// Top-level convenience wrapping [`Scene3d::build_dirty`].
pub fn build_dirty(model: &ArchModel, dirty: &BTreeSet<ArchId>, prev: &Scene3d) -> Scene3d {
    Scene3d::build_dirty(model, dirty, prev)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeSet;

    use cadcraft_doc::Wall;
    use cadcraft_doc::arch::{Level, Room};
    use cadcraft_geom::Vec2;

    /// A model with two non-intersecting walls.
    /// Wall 1: (0,0)→(4,0), t=0.3, h=2.5.  Wall 2: (10,0)→(14,0), t=0.3, h=3.0.
    fn two_wall_model() -> ArchModel {
        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });
        model.walls.push(Wall {
            id: ArchId(2),
            start: Vec2::new(10.0, 0.0),
            end: Vec2::new(14.0, 0.0),
            thickness: 0.3,
            height: 3.0,
            ..Default::default()
        });
        model
    }

    #[test]
    fn build_extrudes_a_rectangle_to_the_wall_height() {
        let model = two_wall_model();
        let scene = Scene3d::build(&model);
        assert_eq!(scene.mesh_count(), 2, "expected 2 meshes, got {}", scene.mesh_count());
        // Tallest wall is 3.0 (wall 2), so bounds max z ≈ 3.0.
        assert!(scene.bounds.max[2] > 0.0, "z extent is zero");
        assert!((scene.bounds.max[2] - 3.0).abs() < 1e-6, "z max = {}", scene.bounds.max[2]);
        for id in [ArchId(1), ArchId(2)] {
            assert!(scene.mesh_for(&id).is_some(), "no mesh for wall {}", id.0);
        }
    }

    #[test]
    fn build_is_deterministic_across_two_calls() {
        let model = two_wall_model();
        let s1 = Scene3d::build(&model);
        let s2 = Scene3d::build(&model);
        assert_eq!(s1.mesh_count(), s2.mesh_count());
        assert_eq!(s1.bounds, s2.bounds);
        for (a, b) in s1.meshes.iter().zip(s2.meshes.iter()) {
            assert_eq!(a.object, b.object);
            assert_eq!(a.positions, b.positions);
            assert_eq!(a.normals, b.normals);
            assert_eq!(a.uvs, b.uvs);
            assert_eq!(a.indices, b.indices);
            assert_eq!(a.material, b.material);
        }
    }

    #[test]
    fn build_dirty_rebuilds_only_the_touched_wall() {
        let model = two_wall_model();
        let full = Scene3d::build(&model);
        assert_eq!(full.mesh_count(), 2);
        let dirty: BTreeSet<ArchId> = BTreeSet::from([ArchId(2)]);
        let partial = Scene3d::build_dirty(&model, &dirty, &full);
        assert_eq!(partial.mesh_count(), 2);
        assert_eq!(partial.bounds, full.bounds);
        // Wall 1 (not dirty) must be bit-for-bit identical.
        match (full.mesh_for(&ArchId(1)), partial.mesh_for(&ArchId(1))) {
            (Some(a), Some(b)) => {
                assert_eq!(a.positions, b.positions);
                assert_eq!(a.normals, b.normals);
                assert_eq!(a.uvs, b.uvs);
                assert_eq!(a.indices, b.indices);
                assert_eq!(a.material, b.material);
            }
            _ => assert!(false, "wall 1 mesh must exist in both scenes"),
        }
        // Wall 2 (dirty) must also be present.
        assert!(partial.mesh_for(&ArchId(2)).is_some(), "wall 2 mesh missing from partial");
    }

    #[test]
    fn crate_root_denies_unwrap_and_panic() {
        let src = include_str!("lib.rs");
        assert!(src.contains("deny(clippy::unwrap_used"));
        assert!(src.contains("forbid(unsafe_code)"));
    }

    #[test]
    fn build_emits_one_mesh_per_wall() {
        let mut model = ArchModel::default();
        for i in 0..5 {
            let x0 = i as f64 * 2.0;
            let x1 = x0 + 1.5;
            model.walls.push(Wall {
                id: ArchId(i),
                start: Vec2::new(x0, 0.0),
                end: Vec2::new(x1, 0.0),
                thickness: 0.3,
                height: 2.5,
                ..Default::default()
            });
        }
        let scene = Scene3d::build(&model);
        for i in 0..5 {
            assert!(scene.mesh_for(&ArchId(i)).is_some(), "no mesh for wall {}", i);
        }
        assert_eq!(scene.mesh_count(), 5);
    }

    #[test]
    fn build_dirty_after_an_opening_edit_equals_a_full_build() {
        use cadcraft_doc::arch::Opening;

        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(0),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });

        let open_id = ArchId(1);
        let mut opening = Opening::default();
        opening.id = open_id;
        opening.host = ArchId(0);
        opening.offset = 2.0;
        opening.width = 0.8;
        opening.height = 2.0;
        model.openings.push(opening);

        let full = Scene3d::build(&model);

        let mut edited = model.clone();
        if let Some(o) = edited.openings.iter_mut().find(|o| o.id == open_id) {
            o.width = 1.2;
        }

        let dirty: BTreeSet<ArchId> = BTreeSet::from([open_id]);
        let partial = Scene3d::build_dirty(&edited, &dirty, &full);
        let full_edited = Scene3d::build(&edited);

        assert_eq!(partial.mesh_count(), full_edited.mesh_count());
        assert_eq!(partial.openings_built, full_edited.openings_built);
        for (a, b) in partial.meshes.iter().zip(full_edited.meshes.iter()) {
            assert_eq!(a.object, b.object);
            assert_eq!(a.positions, b.positions);
            assert_eq!(a.normals, b.normals);
            assert_eq!(a.uvs, b.uvs);
            assert_eq!(a.indices, b.indices);
            assert_eq!(a.material, b.material);
        }
    }

    #[test]
    fn scene_skips_empty_meshes() {
        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(0),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(2.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });
        model.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 2.0),
            end: Vec2::new(2.0, 2.0),
            thickness: 0.3,
            height: 0.0,
            ..Default::default()
        });
        let scene = Scene3d::build(&model);
        assert!(scene.mesh_for(&ArchId(0)).is_some(), "valid wall mesh missing");
        assert!(scene.mesh_for(&ArchId(1)).is_none(), "degenerate wall mesh should be skipped");
    }

    #[test]
    fn opening_components_merge_into_one_mesh_per_material() {
        use cadcraft_doc::arch::{Opening, OpeningKind};

        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(0),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });

        let mut opening = Opening::default();
        opening.id = ArchId(100);
        opening.host = ArchId(0);
        opening.offset = 5.0;
        opening.width = 1.2;
        opening.height = 1.5;
        opening.sill = 0.9;
        opening.frame_width = 0.05;
        opening.frame_depth = 0.1;
        opening.casing_width = 0.05;
        opening.casing_depth = 0.02;
        opening.sill_depth = 0.05;
        opening.style = "window".to_string();
        opening.kind = OpeningKind::Window;

        let wall_map: BTreeMap<ArchId, &Wall> = model.walls.iter().map(|w| (w.id, w)).collect();
        let span = wall_span(&model, &model.walls[0]);
        let meshes = openings::opening_meshes(&wall_map, &opening, span);

        // Each material should appear in at most one mesh.
        let materials: Vec<&str> = meshes.iter().map(|m| m.material.as_str()).collect();
        let unique: BTreeSet<&str> = materials.iter().copied().collect();
        assert_eq!(materials.len(), unique.len(), "duplicate materials in meshes: {materials:?}");

        // Expected materials for a window with frame, glass, casing, sill, header.
        assert!(materials.contains(&"window-frame"));
        assert!(materials.contains(&"window-glass"));
        assert!(materials.contains(&"window-casing"));
        assert!(materials.contains(&"window-sill"));

        // No per-box material proliferation.
        assert!(meshes.len() <= 8, "too many meshes: {} (expected <= 8 merged components)", meshes.len());

        // Every mesh is non-empty and has matching material count.
        for m in &meshes {
            assert!(!m.positions.is_empty(), "empty mesh {}", m.material);
            assert!(m.indices.len() % 3 == 0, "bad index count in {}", m.material);
        }
    }

    #[test]
    fn build_dirty_matches_full_build_after_an_opening_edit() {
        use std::collections::BTreeSet;

        use cadcraft_doc::arch::{Opening, OpeningKind};
        use cadcraft_geom::Vec2;

        let mut model = ArchModel::default();
        model.walls.push(Wall {
            id: ArchId(0),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });

        let open_id = ArchId(1);
        let mut opening = Opening::default();
        opening.id = open_id;
        opening.host = ArchId(0);
        opening.offset = 2.0;
        opening.width = 0.8;
        opening.height = 1.5;
        opening.sill = 0.9;
        opening.frame_width = 0.05;
        opening.frame_depth = 0.1;
        opening.casing_width = 0.05;
        opening.casing_depth = 0.02;
        opening.style = "window".to_string();
        opening.kind = OpeningKind::Window;
        model.openings.push(opening.clone());
        opening = Opening { height: 2.0, ..opening };
        model.openings = vec![opening];

        let full = Scene3d::build(&model);

        // Edit the opening: widen it.
        let mut edited = model.clone();
        if let Some(o) = edited.openings.iter_mut().find(|o| o.id == open_id) {
            o.width = 1.2;
        }

        let dirty: BTreeSet<ArchId> = BTreeSet::from([open_id]);
        let partial = Scene3d::build_dirty(&edited, &dirty, &full);
        let full_edited = Scene3d::build(&edited);

        assert_eq!(partial.mesh_count(), full_edited.mesh_count(), "partial={} full_edited={}", partial.mesh_count(), full_edited.mesh_count());
        for (a, b) in partial.meshes.iter().zip(full_edited.meshes.iter()) {
            assert_eq!(a.object, b.object, "object mismatch: {} vs {}", a.object.0, b.object.0);
            assert_eq!(a.positions, b.positions, "position mismatch: {}", a.material);
            assert_eq!(a.normals, b.normals, "normal mismatch: {}", a.material);
            assert_eq!(a.uvs, b.uvs, "uv mismatch: {}", a.material);
            assert_eq!(a.indices, b.indices, "index mismatch: {}", a.material);
            assert_eq!(a.material, b.material, "material mismatch: {} vs {}", a.material, b.material);
        }
    }

    #[test]
    fn wall_is_built_at_its_level_elevation() {
        let mut model = ArchModel::default();
        model.levels.push(Level { elevation: 3.0, ceiling_height: 2.5, ..Default::default() });
        model.walls.push(Wall {
            id: ArchId(1),
            base_level: 0,
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.0,
            ..Default::default()
        });
        let scene = Scene3d::build(&model);
        let mesh = scene.mesh_for(&ArchId(1)).unwrap();
        let z_min = mesh.positions.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min);
        let z_max = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
        assert!((z_min - 3.0f32).abs() < 1e-3f32, "z_min = {z_min}, expected 3.0");
        assert!((z_max - 5.0f32).abs() < 1e-3f32, "z_max = {z_max}, expected 5.0");
    }

    #[test]
    fn to_ceiling_stops_the_wall_at_the_ceiling() {
        let mut model = ArchModel::default();
        model.levels.push(Level { elevation: 0.0, ceiling_height: 2.2, ..Default::default() });
        model.walls.push(Wall {
            id: ArchId(1),
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            to_ceiling: true,
            ..Default::default()
        });
        let scene = Scene3d::build(&model);
        let mesh = scene.mesh_for(&ArchId(1)).unwrap();
        let z_max = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
        assert!((z_max - 2.2f32).abs() < 1e-3f32, "to_ceiling wall z_max = {z_max}, expected ceiling_height 2.2");
    }

    #[test]
    fn second_level_wall_sits_on_the_first() {
        let mut model = ArchModel::default();
        model.levels.push(Level { elevation: 0.0, ceiling_height: 2.5, ..Default::default() });
        model.levels.push(Level { elevation: 2.5, ceiling_height: 2.5, ..Default::default() });
        model.walls.push(Wall {
            id: ArchId(1),
            base_level: 1,
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });
        let scene = Scene3d::build(&model);
        let mesh = scene.mesh_for(&ArchId(1)).unwrap();
        let z_min = mesh.positions.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min);
        assert!((z_min - 2.5f32).abs() < 1e-3f32, "level-1 wall z_min = {z_min}, expected 2.5");
        let z_max = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
        assert!((z_max - 5.0f32).abs() < 1e-3f32, "level-1 wall z_max = {z_max}, expected 5.0");
    }

    #[test]
    fn wall_with_bad_elevation_is_skipped() {
        let mut model = ArchModel::default();
        model.levels.push(Level { elevation: f64::INFINITY, ceiling_height: 2.5, ..Default::default() });
        model.walls.push(Wall {
            id: ArchId(1),
            base_level: 0,
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(4.0, 0.0),
            thickness: 0.3,
            height: 2.5,
            ..Default::default()
        });
        let scene = Scene3d::build(&model);
        assert!(scene.mesh_for(&ArchId(1)).is_none(), "wall with infinite elevation should be skipped");
    }

    #[test]
    fn three_hundred_walls_150_openings_rebuild_under_50ms() {
        use std::collections::BTreeSet;
        use std::time::Instant;

        use cadcraft_doc::arch::{Opening, OpeningKind};
        use cadcraft_geom::Vec2;

        let mut model = ArchModel::default();

        // 300 walls in a grid.
        for i in 0..300u32 {
            let x0 = (i / 30) as f64 * 4.0;
            let y0 = (i % 30) as f64 * 2.0;
            model.walls.push(Wall {
                id: ArchId(i),
                start: Vec2::new(x0, y0),
                end: Vec2::new(x0 + 2.5, y0),
                thickness: 0.3,
                height: 2.5,
                ..Default::default()
            });
        }

        // 150 window openings, each on a distinct wall.
        for i in 0..150u32 {
            let mut opening = Opening::default();
            opening.id = ArchId(1000 + i);
            opening.host = ArchId(i * 2);
            opening.offset = 1.2;
            opening.width = 1.0;
            opening.height = 1.5;
            opening.sill = 0.9;
            opening.frame_width = 0.05;
            opening.frame_depth = 0.1;
            opening.casing_width = 0.05;
            opening.casing_depth = 0.02;
            opening.style = "window".to_string();
            opening.kind = OpeningKind::Window;
            model.openings.push(opening);
        }

        let full = Scene3d::build(&model);

        let dirty: BTreeSet<ArchId> = (1000..1150).map(ArchId).collect();
        let t0 = Instant::now();
        let _partial = Scene3d::build_dirty(&model, &dirty, &full);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;

        assert!(ms < 50.0, "build_dirty 300w/150o took {:.1} ms (budget 50 ms)", ms);
    }

    /// Build a square room at the given level.
    fn square_room(id: u32, level: u32, origin: (f64, f64), size: f64) -> Room {
        let (x, y) = origin;
        let pts = vec![Vec2::new(x, y), Vec2::new(x + size, y), Vec2::new(x + size, y + size), Vec2::new(x, y + size)];
        Room { id: ArchId(id), level, polygon: pts, ..Default::default() }
    }

    #[test]
    fn floor_counts_match_room_counts() {
        let mut model = ArchModel::default();
        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
        model.rooms.push(square_room(1, 0, (0.0, 0.0), 3.0));
        model.rooms.push(square_room(2, 0, (4.0, 0.0), 3.0));
        model.rooms.push(square_room(3, 0, (8.0, 0.0), 3.0));

        let scene = Scene3d::build(&model);
        assert_eq!(scene.floors, 3, "floors={} expected 3", scene.floors);
        assert_eq!(scene.ceilings, 3, "ceilings={} expected 3", scene.ceilings);
        let floor_meshes: Vec<&Mesh> = scene.meshes.iter().filter(|m| m.material == "floor").collect();
        assert_eq!(floor_meshes.len(), 3, "floor mesh count={}", floor_meshes.len());
    }

    #[test]
    fn slab_mesh_carries_the_room_id() {
        let mut model = ArchModel::default();
        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
        model.rooms.push(square_room(7, 0, (0.0, 0.0), 3.0));

        let scene = Scene3d::build(&model);
        assert!(scene.meshes.iter().any(|m| m.object == ArchId(7) && m.material == "floor"), "no floor mesh for room ArchId(7)",);
    }

    #[test]
    fn build_dirty_rebuilds_only_the_touched_room() {
        let mut model = ArchModel::default();
        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
        model.rooms.push(square_room(1, 0, (0.0, 0.0), 3.0));
        model.rooms.push(square_room(2, 0, (4.0, 0.0), 3.0));

        let full = Scene3d::build(&model);
        let dirty: BTreeSet<ArchId> = BTreeSet::from([ArchId(2)]);
        let partial = Scene3d::build_dirty(&model, &dirty, &full);

        assert_eq!(partial.floors, full.floors, "floors {} != {}", partial.floors, full.floors);
        assert_eq!(partial.ceilings, full.ceilings, "ceilings {} != {}", partial.ceilings, full.ceilings);

        let full_floor = full.meshes.iter().find(|m| m.object == ArchId(1) && m.material == "floor");
        let partial_floor = partial.meshes.iter().find(|m| m.object == ArchId(1) && m.material == "floor");
        match (full_floor, partial_floor) {
            (Some(fa), Some(pa)) => {
                assert_eq!(fa.positions, pa.positions);
                assert_eq!(fa.normals, pa.normals);
                assert_eq!(fa.uvs, pa.uvs);
                assert_eq!(fa.indices, pa.indices);
                assert_eq!(fa.material, pa.material);
            }
            _ => assert!(false, "room 1 floor mesh missing from full or partial"),
        }
    }

    #[test]
    fn build_dirty_room_counts_match_full_build() {
        let mut model = ArchModel::default();
        model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
        model.rooms.push(square_room(1, 0, (0.0, 0.0), 3.0));
        model.rooms.push(square_room(2, 0, (4.0, 0.0), 3.0));

        let full = Scene3d::build(&model);

        let mut edited = model.clone();
        if let Some(room) = edited.rooms.iter_mut().find(|r| r.id == ArchId(2)) {
            room.polygon[1].x += 1.0;
        }

        let dirty: BTreeSet<ArchId> = BTreeSet::from([ArchId(2)]);
        let partial = Scene3d::build_dirty(&edited, &dirty, &full);
        let full_edited = Scene3d::build(&edited);

        assert_eq!(partial.floors, full_edited.floors, "floors {} != {}", partial.floors, full_edited.floors);
        assert_eq!(partial.ceilings, full_edited.ceilings, "ceilings {} != {}", partial.ceilings, full_edited.ceilings);
    }
}
