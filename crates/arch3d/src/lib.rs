//! 3D scene builder for architectural models.
//!
//! Converts a 2D [`ArchModel`] into a [`Scene3d`] of extruded wall meshes.
//! The builder is pure, deterministic, and incremental: `build` solves joins
//! once and extrudes every wall; `build_dirty` reuses unchanged meshes and
//! only re-extrudes walls whose id is in the `dirty` set.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod mesh;
pub mod walls;

pub use crate::mesh::{Bounds3, Mesh};

use std::collections::BTreeSet;

use cadcraft_arch::join::solve_joins;
use cadcraft_doc::{ArchId, ArchModel};

/// A 3D scene composed of extruded architectural meshes.
#[derive(Debug, Clone)]
pub struct Scene3d {
    pub meshes: Vec<Mesh>,
    pub bounds: Bounds3,
    pub revision: u64,
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

    /// Build the full 3D scene from an arch model.
    ///
    /// Solves joins once, then extrudes each wall.
    pub fn build(model: &ArchModel) -> Scene3d {
        let shapes = solve_joins(model);
        let mut meshes: Vec<Mesh> = Vec::with_capacity(shapes.len());

        for shape in &shapes {
            let height = model.walls.iter().find(|w| w.id == shape.id).map(|w| w.height).unwrap_or(0.0);
            if let Some(mesh) = walls::extrude_wall(shape, height, 0.0, "wall") {
                meshes.push(mesh);
            }
        }

        let bounds = meshes.iter().fold(Bounds3::empty(), |mut b, m| {
            b.extend(&m.bounds());
            b
        });

        Scene3d { meshes, bounds, revision: 0 }
    }

    /// Incremental rebuild: reuses meshes for ids not in `dirty`, recomputes bounds.
    pub fn build_dirty(model: &ArchModel, dirty: &BTreeSet<ArchId>, prev: &Scene3d) -> Scene3d {
        let shapes = solve_joins(model);
        let mut meshes: Vec<Mesh> = Vec::with_capacity(shapes.len());

        for shape in &shapes {
            if !dirty.contains(&shape.id) {
                if let Some(mesh) = prev.mesh_for(&shape.id) {
                    meshes.push(mesh.clone());
                    continue;
                }
            }
            let height = model.walls.iter().find(|w| w.id == shape.id).map(|w| w.height).unwrap_or(0.0);
            if let Some(mesh) = walls::extrude_wall(shape, height, 0.0, "wall") {
                meshes.push(mesh);
            }
        }

        let bounds = meshes.iter().fold(Bounds3::empty(), |mut b, m| {
            b.extend(&m.bounds());
            b
        });

        Scene3d { meshes, bounds, revision: 0 }
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
}
