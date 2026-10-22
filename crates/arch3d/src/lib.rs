//! 3D scene builder for architectural models.
//!
//! Converts a 2D [`ArchModel`] into a [`Scene3d`] of extruded wall meshes.
//! The builder is pure, deterministic, and incremental: `build` solves joins
//! once and extrudes every wall; `build_dirty` reuses unchanged meshes and
//! only re-extrudes walls whose id is in the `dirty` set.

pub mod mesh;
pub mod walls;

pub use crate::mesh::{Bounds3, Mesh};

use std::collections::BTreeSet;

use cadcraft_arch::join::solve_joins;
use cadcraft_doc::ArchModel;

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
            if let Some(mesh) = walls::extrude_wall(shape, shape.base_elevation, "wall") {
                meshes.push(mesh);
            }
        }

        let bounds = meshes.iter().fold(Bounds3::empty(), |mut b, m| {
            b.extend(&m.bounds());
            b
        });

        Scene3d { meshes, bounds, revision: model.revision }
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
            if let Some(mesh) = walls::extrude_wall(shape, shape.base_elevation, "wall") {
                meshes.push(mesh);
            }
        }

        let bounds = meshes.iter().fold(Bounds3::empty(), |mut b, m| {
            b.extend(&m.bounds());
            b
        });

        Scene3d { meshes, bounds, revision: model.revision }
    }
}
