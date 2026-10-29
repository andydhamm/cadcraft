//! Feature-edge lines drawn over the mesh for shaded-edges and hidden-line. The edges come from
//! `arch3d::edges` (crease over 20° and boundary edges); nothing is detected in a shader.

use cadcraft_arch3d::Scene3d;
use cadcraft_arch3d::edges::{CREASE_DEG, EdgeKind, scene_feature_edges};
use wgpu::util::DeviceExt;

use crate::pipeline::{COLOR_FORMAT, DEPTH_FORMAT, valid_sample_count};

/// WGSL source of the line pipeline.
const SHADER: &str = include_str!("shaders/line.wgsl");

/// Edge colour of the shaded-edges and hidden-line styles (linear RGBA).
pub const EDGE_COLOR: [f32; 4] = [0.08, 0.08, 0.08, 1.0];

/// Uniform block of the line shader (WGSL `Line`): the view-projection matrix and an RGBA colour.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineUniform {
    /// Column-major view-projection matrix.
    pub view_proj: [[f32; 4]; 4],
    /// Line colour, linear RGBA.
    pub color: [f32; 4],
}

/// Byte size of [`LineUniform`].
pub const LINE_UNIFORM_SIZE: u64 = 80;

/// Line-list vertices (two per edge) of the scene's Boundary, Crease and NonManifold edges.
pub fn edge_lines(scene: &Scene3d) -> Vec<[f32; 3]> {
    let edges = scene_feature_edges(scene, CREASE_DEG);
    let mut out = Vec::with_capacity(edges.len().saturating_mul(2));
    for e in edges.iter().filter(|e| matches!(e.kind, EdgeKind::Boundary | EdgeKind::Crease | EdgeKind::NonManifold)) {
        for p in [e.a, e.b] {
            out.push([p[0] as f32, p[1] as f32, p[2] as f32]);
        }
    }
    out
}

/// The line pipeline and its resident vertex buffer.
pub struct EdgePass {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Option<(u64, wgpu::Buffer, u32)>,
}

impl EdgePass {
    /// Build the pipeline for a target of `sample_count` samples (1 or 4; anything else is 1).
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let module = device
            .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("render3d line"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render3d line bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZero::new(LINE_UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render3d line pipeline layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let attrs = [wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 }];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render3d line pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: valid_sample_count(sample_count), mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render3d line uniform"),
            size: LINE_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render3d line uniform"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        EdgePass { pipeline, uniform, bind_group, vertices: None }
    }

    /// Upload the edge lines of `scene` unless `scene_key` is already resident, and write the
    /// uniform for this frame.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene3d,
        scene_key: u64,
        view_proj: [[f32; 4]; 4],
        color: [f32; 4],
    ) {
        if self.vertices.as_ref().is_none_or(|(k, _, _)| *k != scene_key) {
            let lines = edge_lines(scene);
            let count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("render3d edge vertices"),
                contents: bytemuck::cast_slice(&lines),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.vertices = Some((scene_key, buffer, count));
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&LineUniform { view_proj, color }));
    }

    /// Draw the resident lines into `pass` (which must match the sample count and depth format).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some((_, buffer, count)) = &self.vertices else { return };
        if *count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..*count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArchId, Mesh};

    fn mesh_of(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Scene3d {
        let mut m = Mesh::new("m", ArchId(1));
        m.normals = vec![[0.0, 0.0, 1.0]; positions.len()];
        m.uvs = vec![[0.0, 0.0]; positions.len()];
        m.positions = positions;
        m.indices = indices;
        Scene3d { meshes: vec![m], ..Default::default() }
    }

    #[test]
    fn edge_lines_of_a_box_are_its_twelve_edges() {
        let p = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let idx = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
        assert_eq!(edge_lines(&mesh_of(p, idx)).len(), 24);
    }

    #[test]
    fn a_planar_quad_diagonal_is_not_an_edge_line() {
        let p = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let lines = edge_lines(&mesh_of(p, vec![0, 1, 2, 0, 2, 3]));
        assert_eq!(lines.len(), 8);
        for seg in lines.chunks(2) {
            let d = [(seg[0][0] - seg[1][0]).abs(), (seg[0][1] - seg[1][1]).abs()];
            assert!(d[0] < 1e-6 || d[1] < 1e-6, "diagonal segment {seg:?}");
        }
    }

    #[test]
    fn line_uniform_block_size_is_fixed() {
        assert_eq!(std::mem::size_of::<LineUniform>(), 80);
        assert_eq!(LINE_UNIFORM_SIZE, 80);
    }
}
