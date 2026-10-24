//! GPU mesh upload and render pipeline for the 3D viewport.

use crate::types::Scene3d;
use wgpu::util::DeviceExt;

/// GPU-side mesh buffers for a scene.
pub struct GpuMeshes {
    /// Vertex buffer: interleaved position(f32x3) + normal(f32x3) + uv(f32x2).
    pub vertex_buffer: wgpu::Buffer,
    /// Index buffer: u32 indices.
    pub index_buffer: wgpu::Buffer,
    /// Total vertex count across all meshes.
    pub vertex_count: u32,
    /// Total index count across all meshes.
    pub index_count: u32,
}

/// Upload a scene's meshes to GPU buffers.
///
/// Packs eight `f32`s per vertex (position, normal, UV) with a 32-byte stride.
pub fn upload(device: &wgpu::Device, scene: &Scene3d) -> GpuMeshes {
    let mut vertices: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex_offset: u32 = 0;

    for mesh in &scene.meshes {
        for i in 0..mesh.positions.len() {
            let position = mesh.positions.get(i).copied().unwrap_or([0.0, 0.0, 0.0]);
            let normal = mesh.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            vertices.extend_from_slice(&position);
            vertices.extend_from_slice(&normal);
            vertices.extend_from_slice(&uv);
        }

        indices.extend(mesh.indices.iter().map(|&i| i + vertex_offset));
        vertex_offset += mesh.positions.len() as u32;
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("render3d vertex buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("render3d index buffer"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    GpuMeshes { vertex_buffer, index_buffer, vertex_count: (vertices.len() / 8) as u32, index_count: indices.len() as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArchId, Bounds3, Mesh};
    use futures::FutureExt;

    #[test]
    fn upload_counts_vertices_from_arch3d_meshes() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        for i in 0..3 {
            let x = i as f32;
            mesh.positions.push([x, 0.0, 0.0]);
            mesh.normals.push([0.0, 0.0, 1.0]);
            mesh.uvs.push([x, 0.0]);
        }
        mesh.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![mesh], bounds: Bounds3::empty(), revision: 0, openings_built: 0 };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Some(adapter) = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).now_or_never().and_then(Result::ok) else {
            return;
        };

        let Some((device, _queue)) = adapter.request_device(&wgpu::DeviceDescriptor::default()).now_or_never().and_then(Result::ok) else {
            return;
        };

        let meshes = upload(&device, &scene);
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
    }

    #[test]
    fn gpu_mesh_vertex_count_matches_the_scene() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        for i in 0..3 {
            let x = i as f32;
            mesh.positions.push([x, 0.0, 0.0]);
            mesh.normals.push([0.0, 0.0, 1.0]);
            mesh.uvs.push([x, 0.0]);
        }
        mesh.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![mesh], bounds: Bounds3::empty(), revision: 0, openings_built: 0 };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Some(adapter) = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).now_or_never().and_then(Result::ok) else {
            return;
        };

        let Some((device, _queue)) = adapter.request_device(&wgpu::DeviceDescriptor::default()).now_or_never().and_then(Result::ok) else {
            return;
        };

        let meshes = upload(&device, &scene);
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
    }
}
