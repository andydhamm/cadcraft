//! Shared 3D scene types used by the renderer.

use cadcraft_arch3d::mesh::Bounds3;

/// A 3D vertex: position, normal, and UV coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// A triangular mesh with an object ID for picking.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub object: u64,
}

/// A complete 3D scene: a collection of meshes and their bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene3d {
    pub meshes: Vec<Mesh>,
    pub bounds: Bounds3,
}

impl Default for Scene3d {
    fn default() -> Self {
        Scene3d { meshes: Vec::new(), bounds: Bounds3::empty() }
    }
}
