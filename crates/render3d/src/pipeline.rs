//! GPU mesh upload and render pipeline for the 3D viewport.

use crate::types::Scene3d;
use wgpu::util::DeviceExt;

/// Depth format used by the 3D viewport.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Colour format used by the 3D viewport.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// WGSL source for the lit mesh pipeline.
const SHADER: &str = include_str!("shaders/mesh.wgsl");

/// Uniforms uploaded to the GPU for the lit pipeline.
///
/// Mirrors the WGSL `Uniforms` block (96 bytes, 16-byte align).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuUniform {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub ambient: f32,
    pub albedo: [f32; 3],
    pub pad: f32,
}
/// Byte size of [`GpuUniform`].
pub const UNIFORM_SIZE: u64 = 96;

/// Distance between per-draw uniform slots in one buffer: [`UNIFORM_SIZE`] rounded up to the
/// device's `min_uniform_buffer_offset_alignment` (256 on every backend we target).
pub fn uniform_stride(device: &wgpu::Device) -> u64 {
    let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
    UNIFORM_SIZE.div_ceil(align) * align
}

/// Linear albedo for an arch3d material name. Unknown names are a neutral grey, so a new
/// material still renders lit; names are matched by prefix (`wall-exterior` is a wall).
pub fn material_albedo(material: &str) -> [f32; 3] {
    const TABLE: [(&str, [f32; 3]); 11] = [
        ("wall", [0.80, 0.78, 0.74]),
        ("floor", [0.55, 0.42, 0.30]),
        ("ceiling", [0.92, 0.92, 0.90]),
        ("glass", [0.55, 0.70, 0.80]),
        ("frame", [0.90, 0.90, 0.88]),
        ("sill", [0.90, 0.90, 0.88]),
        ("door", [0.50, 0.35, 0.22]),
        ("leaf", [0.50, 0.35, 0.22]),
        ("roof", [0.35, 0.30, 0.28]),
        // Matte trim tones for the roof edge parts (phase 4).
        ("fascia", [0.50, 0.42, 0.34]),
        ("soffit", [0.80, 0.80, 0.76]),
    ];
    let m = material.to_ascii_lowercase();
    TABLE.iter().find(|(name, _)| m.starts_with(name)).map_or([0.75, 0.75, 0.75], |(_, c)| *c)
}

impl GpuUniform {
    /// Build a uniform, normalising `light_dir` and rejecting NaN in every field.
    pub fn new(view_proj: [[f32; 4]; 4], light_dir: [f32; 3], ambient: f32, albedo: [f32; 3]) -> Self {
        let mut ld = light_dir;
        for i in 0..3 {
            ld[i] = if ld[i].is_finite() { ld[i] } else { 0.0 };
        }
        let len = (ld[0] * ld[0] + ld[1] * ld[1] + ld[2] * ld[2]).sqrt();
        let light_dir = if len.is_finite() && len > 0.0 { [ld[0] / len, ld[1] / len, ld[2] / len] } else { [0.0, 0.0, 1.0] };
        let ambient = if ambient.is_finite() { ambient } else { 0.0 };
        let mut albedo = albedo;
        for i in 0..3 {
            albedo[i] = if albedo[i].is_finite() { albedo[i] } else { 0.0 };
        }
        GpuUniform { view_proj, light_dir, ambient, albedo, pad: 0.0 }
    }
}

/// Interleaved vertex layout: position(3) + normal(3) + uv(2) = 32-byte stride.
pub fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x2,
    ];
    wgpu::VertexBufferLayout { attributes: &ATTRS, array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex }
}

/// One indexed draw: a mesh's index range and its albedo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draw {
    /// First index in [`GpuMeshes::index_buffer`].
    pub first_index: u32,
    /// Number of indices (a multiple of three).
    pub index_count: u32,
    /// Linear albedo from [`material_albedo`].
    pub albedo: [f32; 3],
}

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
    /// One draw per non-empty mesh, in scene order.
    pub draws: Vec<Draw>,
}

/// Upload a scene's meshes to GPU buffers.
///
/// Packs eight `f32`s per vertex (position, normal, UV) with a 32-byte stride.
pub fn upload(device: &wgpu::Device, scene: &Scene3d) -> GpuMeshes {
    let mut vertices: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex_offset: u32 = 0;
    let mut draws = Vec::new();

    for mesh in &scene.meshes {
        for i in 0..mesh.positions.len() {
            let position = mesh.positions.get(i).copied().unwrap_or([0.0, 0.0, 0.0]);
            let normal = mesh.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            vertices.extend_from_slice(&position);
            vertices.extend_from_slice(&normal);
            vertices.extend_from_slice(&uv);
        }

        // Drop triangles that point past this mesh's vertices instead of reading another mesh's.
        let n = mesh.positions.len() as u32;
        let first_index = indices.len() as u32;
        for tri in mesh.indices.chunks_exact(3) {
            if tri.iter().all(|&i| i < n) {
                indices.extend(tri.iter().map(|&i| i + vertex_offset));
            }
        }
        let index_count = indices.len() as u32 - first_index;
        if index_count > 0 {
            draws.push(Draw { first_index, index_count, albedo: material_albedo(&mesh.material) });
        }
        vertex_offset += n;
    }
    let vertex_count = (vertices.len() / 8) as u32;
    let index_count = indices.len() as u32;
    // wgpu refuses zero-sized vertex and index buffers on some backends; an empty scene keeps a
    // placeholder and draws nothing.
    if vertices.is_empty() {
        vertices.resize(8, 0.0);
    }
    if indices.is_empty() {
        indices.push(0);
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

    GpuMeshes { vertex_buffer, index_buffer, vertex_count, index_count, draws }
}

/// Build the render pipeline and related resources.
fn create_all(device: &wgpu::Device, sample_count: u32) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let module =
        device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("render3d mesh"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render3d bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            count: None,
            // One slot per draw in a shared buffer, picked with a dynamic offset.
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: std::num::NonZero::new(UNIFORM_SIZE),
            },
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        }],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render3d pipeline layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let vertex =
        wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[Some(vertex_layout())], compilation_options: Default::default() };

    let depth_stencil = wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    };

    let multisample = wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false };

    let fragment = wgpu::FragmentState {
        module: &module,
        entry_point: Some("fs"),
        targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        compilation_options: Default::default(),
    };

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render3d pipeline"),
        layout: Some(&layout),
        vertex: vertex,
        // Two-sided: arch3d's caps and slabs are not all wound outward, and the shader flips the
        // normal of a back face so it is lit from the side the camera sees.
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: Some(depth_stencil),
        multisample: multisample,
        fragment: Some(fragment),
        multiview_mask: None,
        cache: None,
    });

    (pipeline, bgl)
}

/// Create a lit render pipeline with a 32-bit float depth state.
pub fn create_pipeline(device: &wgpu::Device, sample_count: u32) -> wgpu::RenderPipeline {
    let (pipeline, _bgl) = create_all(device, sample_count);
    pipeline
}

/// All GPU resources for the 3D viewport: pipeline, bind group layout,
/// uniform buffer and bind group.
pub struct GpuPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bgl: wgpu::BindGroupLayout,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl GpuPipeline {
    /// Build the full pipeline with a uniform buffer initialised to a zero uniform.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let (pipeline, bgl) = create_all(device, sample_count);

        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("render3d uniform buffer"),
            contents: bytemuck::bytes_of(&GpuUniform::new([[0.0; 4]; 4], [0.0; 3], 0.0, [0.0; 3])),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render3d bind group instance"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: std::num::NonZero::new(UNIFORM_SIZE),
                }),
            }],
        });

        GpuPipeline { pipeline, bgl, uniform, bind_group }
    }

    /// Upload a new uniform to the GPU (slot 0; bind with dynamic offset 0).
    pub fn write_uniform(&self, queue: &wgpu::Queue, u: &GpuUniform) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(u));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArchId, Mesh};

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

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

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

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
    }

    #[test]
    fn gpu_uniform_is_96_bytes() {
        assert_eq!(std::mem::size_of::<GpuUniform>(), 96);
    }

    #[test]
    fn vertex_layout_stride_matches_upload() {
        assert_eq!(vertex_layout().array_stride, 32);
    }

    #[test]
    fn lit_pipeline_compiles_with_depth_state() {
        let Some((device, _queue)) = crate::gputest::shared_device() else { return };
        let _pipeline = create_pipeline(&device, 1);
    }

    #[test]
    fn material_albedo_covers_roof_parts() {
        // Phase-4 roof meshes are named `roof`, `fascia`, and `soffit`; each must map
        // to a finite, distinct matte albedo rather than the neutral-grey fallback.
        assert_eq!(material_albedo("roof"), [0.35, 0.30, 0.28]);
        assert_eq!(material_albedo("fascia"), [0.50, 0.42, 0.34]);
        assert_eq!(material_albedo("soffit"), [0.80, 0.80, 0.76]);
        // Matching is case-insensitive and prefix-based (`fascia-north`).
        assert_eq!(material_albedo("Roof"), [0.35, 0.30, 0.28]);
        assert_eq!(material_albedo("Fascia-North"), [0.50, 0.42, 0.34]);
        assert_eq!(material_albedo("Soffit"), [0.80, 0.80, 0.76]);
    }
}
