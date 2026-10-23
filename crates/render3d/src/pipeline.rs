//! GPU mesh upload and render pipeline for the 3D viewport.

use crate::types::{Mesh, Scene3d};
use wgpu::util::DeviceExt;

/// GPU-side mesh buffers for a scene.
pub struct GpuMeshes {
    /// Vertex buffer: interleaved position(f32x3) + normal(f32x3) + uv(f32x2) + object(u32).
    pub vertex_buffer: wgpu::Buffer,
    /// Index buffer: u32 indices.
    pub index_buffer: wgpu::Buffer,
    /// Total vertex count across all meshes.
    pub vertex_count: u32,
    /// Total index count across all meshes.
    pub index_count: u32,
}

/// Upload a scene's meshes to GPU buffers.
pub fn upload(device: &wgpu::Device, scene: &Scene3d) -> GpuMeshes {
    let mut vertices: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex_offset: u32 = 0;

    for mesh in &scene.meshes {
        upload_mesh(mesh, &mut vertices, &mut indices, &mut vertex_offset);
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

    GpuMeshes {
        vertex_buffer,
        index_buffer,
        vertex_count: u32::try_from(vertices.len() / 10).unwrap_or(u32::MAX), // 10 f32s per vertex
        index_count: u32::try_from(indices.len()).unwrap_or(u32::MAX),
    }
}

fn upload_mesh(mesh: &Mesh, vertices: &mut Vec<f32>, indices: &mut Vec<u32>, vertex_offset: &mut u32) {
    for v in &mesh.vertices {
        vertices.push(v.position[0]);
        vertices.push(v.position[1]);
        vertices.push(v.position[2]);
        vertices.push(v.normal[0]);
        vertices.push(v.normal[1]);
        vertices.push(v.normal[2]);
        vertices.push(v.uv[0]);
        vertices.push(v.uv[1]);
        vertices.push(0.0); // padding for alignment
        vertices.push(0.0); // padding for alignment
    }

    // Saturating: a scene past u32::MAX vertices cannot be drawn with u32 indices anyway,
    // and an overflow must not panic (debug) or wrap into other meshes' vertices (release).
    for &i in &mesh.indices {
        indices.push(i.saturating_add(*vertex_offset));
    }

    *vertex_offset = vertex_offset.saturating_add(u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Vertex;
    use futures::FutureExt;

    #[test]
    fn gpu_mesh_vertex_count_matches_the_scene() {
        // Create a minimal scene with known vertex count
        let scene = Scene3d {
            meshes: vec![Mesh {
                vertices: vec![
                    Vertex { position: [0.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
                    Vertex { position: [1.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
                    Vertex { position: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
                ],
                indices: vec![0, 1, 2],
                object: 1,
            }],
            bounds: cadcraft_arch3d::mesh::Bounds3::empty(),
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).now_or_never().and_then(Result::ok);

        if let Some(adapter) = adapter {
            let device = adapter.request_device(&wgpu::DeviceDescriptor::default()).now_or_never().and_then(Result::ok);
            assert!(device.is_some(), "an adapter without a device");
            let Some((device, _queue)) = device else { return };
            let meshes = upload(&device, &scene);
            assert_eq!(meshes.vertex_count, 3);
            assert_eq!(meshes.index_count, 3);
        }
        // If no adapter is available (CI), skip the GPU-dependent check
    }
}
