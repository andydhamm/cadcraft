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

/// Shading style: faces shaded with PBR.
pub const STYLE_SHADED: u32 = 0;
/// Shading style: shaded; the edge lines come from `arch3d::edges`.
pub const STYLE_SHADED_EDGES: u32 = 1;
/// Shading style: faces in the clear colour with depth written (hidden-line).
pub const STYLE_HIDDEN_LINE: u32 = 2;
/// Shading style: translucent (alpha 0.35), no depth write.
pub const STYLE_GLASS_HOUSE: u32 = 3;

/// Material and sun uniforms uploaded to the GPU (WGSL `PbrUniforms`, group 1 binding 1).
///
/// 96 bytes, 16-byte align; the `_p*` fields are layout padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PbrUniform {
    /// Unit direction toward the sun.
    pub light_dir: [f32; 3],
    /// Sun radiance scale.
    pub sun_intensity: f32,
    /// Ambient fraction 0..=1.
    pub ambient: f32,
    /// 0 dielectric … 1 metal.
    pub metallic: f32,
    /// Perceptual roughness 0..=1.
    pub roughness: f32,
    /// Opacity; below 1.0 blends.
    pub alpha: f32,
    /// One of the `STYLE_*` constants.
    pub style: u32,
    pub _p0: [u32; 3],
    /// Eye position in world space.
    pub camera_pos: [f32; 3],
    /// Shadow-map depth range start (unused until shadows land).
    pub shadow_min: f32,
    /// Shadow-map depth range end.
    pub shadow_max: f32,
    pub _p1: [f32; 3],
    pub _p2: [f32; 4],
}
/// Byte size of [`PbrUniform`].
pub const PBR_SIZE: u64 = 96;

impl PbrUniform {
    /// Build the block from a light, a draw and a style, replacing every non-finite value and
    /// clamping `metallic`, `roughness`, `alpha` to 0..=1 and `ambient` to 0..=1.
    pub fn new(light: &crate::light::Light, sun_intensity: f32, draw: &Draw, style: u32, camera_pos: [f32; 3]) -> Self {
        let l = light.normalized();
        let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
        let mut cam = camera_pos;
        for c in cam.iter_mut() {
            *c = fin(*c, 0.0);
        }
        PbrUniform {
            light_dir: l.dir,
            sun_intensity: fin(sun_intensity, 1.0).max(0.0),
            ambient: l.ambient,
            metallic: fin(draw.metallic, 0.0).clamp(0.0, 1.0),
            roughness: fin(draw.roughness, 0.5).clamp(0.0, 1.0),
            alpha: fin(draw.alpha, 1.0).clamp(0.0, 1.0),
            style: style.min(STYLE_GLASS_HOUSE),
            _p0: [0; 3],
            camera_pos: cam,
            shadow_min: 0.0,
            shadow_max: 0.0,
            _p1: [0.0; 3],
            _p2: [0.0; 4],
        }
    }
}

/// MSAA sample counts we allow: 1 or 4 (the only counts WebGL2 guarantees). Anything else is 1.
pub fn valid_sample_count(n: u32) -> u32 {
    if n == 4 { 4 } else { 1 }
}

/// PBR parameters for an arch3d material name: `(albedo, metallic, roughness, alpha)`. Albedo is
/// [`material_albedo`]; unknown names are a neutral matte dielectric `([0.75; 3], 0.0, 0.5, 1.0)`.
pub fn material_pbr(material: &str) -> ([f32; 3], f32, f32, f32) {
    let albedo = material_albedo(material);
    let m = material.to_ascii_lowercase();
    let starts = |names: &[&str]| names.iter().any(|n| m.starts_with(n));
    if starts(&["glass"]) {
        return (albedo, 0.0, 0.05, 0.35);
    }
    let rough = if starts(&["frame", "sill"]) {
        0.6
    } else if starts(&["wall", "ceiling", "fascia", "soffit", "casing", "header"]) {
        0.9
    } else if starts(&["floor", "door", "leaf", "stair", "railing", "deck"]) {
        0.7
    } else if starts(&["roof", "ground"]) {
        0.95
    } else {
        return (albedo, 0.0, 0.5, 1.0);
    };
    (albedo, 0.0, rough, 1.0)
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
        let mut view_proj = view_proj;
        for row in view_proj.iter_mut() {
            for v in row.iter_mut() {
                if !v.is_finite() {
                    *v = 0.0;
                }
            }
        }
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
    /// 0 dielectric … 1 metal, from [`material_pbr`].
    pub metallic: f32,
    /// Perceptual roughness 0..=1.
    pub roughness: f32,
    /// Opacity; below 1.0 the draw blends (glass).
    pub alpha: f32,
}

impl Default for Draw {
    fn default() -> Self {
        Draw { first_index: 0, index_count: 0, albedo: [0.75; 3], metallic: 0.0, roughness: 0.5, alpha: 1.0 }
    }
}

impl Draw {
    /// Whether the draw needs the blended, depth-write-free pipeline.
    pub fn is_blended(&self) -> bool {
        self.alpha < 1.0
    }
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
            let (albedo, metallic, roughness, alpha) = material_pbr(&mesh.material);
            draws.push(Draw { first_index, index_count, albedo, metallic, roughness, alpha });
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

/// Pipelines and bind group layouts built together.
struct Built {
    opaque: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    pbr_bgl: wgpu::BindGroupLayout,
}

/// Build the render pipelines and related resources.
fn create_all(device: &wgpu::Device, sample_count: u32) -> Built {
    let sample_count = valid_sample_count(sample_count);
    let module =
        device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("render3d mesh"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });

    let uniform_entry = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry {
        binding,
        count: None,
        // One slot per draw in a shared buffer, picked with a dynamic offset.
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: std::num::NonZero::new(size),
        },
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
    };
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render3d bind group layout"),
        entries: &[uniform_entry(0, UNIFORM_SIZE)],
    });
    let pbr_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render3d pbr bind group layout"),
        entries: &[uniform_entry(1, PBR_SIZE)],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render3d pipeline layout"),
        bind_group_layouts: &[Some(&bgl), Some(&pbr_bgl)],
        immediate_size: 0,
    });

    let make = |label: &str, depth_write: bool, blend: Option<wgpu::BlendState>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[Some(vertex_layout())],
                compilation_options: Default::default(),
            },
            // Two-sided: arch3d's caps and slabs are not all wound outward, and the shader flips
            // the normal of a back face so it is lit from the side the camera sees.
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let opaque = make("render3d pipeline", true, None);
    // Glass: alpha blended, depth tested but not written, drawn after the opaque draws.
    let blended = make("render3d glass pipeline", false, Some(wgpu::BlendState::ALPHA_BLENDING));

    Built { opaque, blended, bgl, pbr_bgl }
}

/// Create a lit render pipeline with a 32-bit float depth state. `sample_count` is 1 or 4
/// (anything else is 1).
pub fn create_pipeline(device: &wgpu::Device, sample_count: u32) -> wgpu::RenderPipeline {
    create_all(device, sample_count).opaque
}

/// All GPU resources for the 3D viewport: pipelines, bind group layouts,
/// uniform buffers and bind groups.
pub struct GpuPipeline {
    /// Opaque pipeline (depth written, no blending).
    pub pipeline: wgpu::RenderPipeline,
    /// Blended pipeline for draws with `alpha < 1.0` (no depth write).
    pub glass_pipeline: wgpu::RenderPipeline,
    /// Layout of group 0 (view/albedo block, binding 0, dynamic offset).
    pub bgl: wgpu::BindGroupLayout,
    /// Layout of group 1 (PBR block, binding 1, dynamic offset).
    pub pbr_bgl: wgpu::BindGroupLayout,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    /// Slot-0 PBR block buffer and its group-1 bind group.
    pub pbr_uniform: wgpu::Buffer,
    pub pbr_bind_group: wgpu::BindGroup,
}

impl GpuPipeline {
    /// Build the full pipeline with uniform buffers initialised to zero / default uniforms.
    /// `sample_count` is 1 or 4 (anything else is 1).
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let Built { opaque, blended, bgl, pbr_bgl } = create_all(device, sample_count);

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

        let pbr = PbrUniform::new(&crate::light::Light::DEFAULT, 1.0, &Draw::default(), STYLE_SHADED, [0.0; 3]);
        let pbr_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("render3d pbr uniform buffer"),
            contents: bytemuck::bytes_of(&pbr),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let pbr_bind_group = pbr_bind_group(device, &pbr_bgl, &pbr_uniform);

        GpuPipeline { pipeline: opaque, glass_pipeline: blended, bgl, pbr_bgl, uniform, bind_group, pbr_uniform, pbr_bind_group }
    }

    /// Upload a new uniform to the GPU (slot 0; bind with dynamic offset 0).
    pub fn write_uniform(&self, queue: &wgpu::Queue, u: &GpuUniform) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(u));
    }

    /// Upload a new PBR block (slot 0; bind group 1 with dynamic offset 0).
    pub fn write_pbr(&self, queue: &wgpu::Queue, u: &PbrUniform) {
        queue.write_buffer(&self.pbr_uniform, 0, bytemuck::bytes_of(u));
    }
}

/// A group-1 bind group over `buffer` (one [`PBR_SIZE`] slot, dynamic offset).
pub fn pbr_bind_group(device: &wgpu::Device, pbr_bgl: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render3d pbr bind group"),
        layout: pbr_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer, offset: 0, size: std::num::NonZero::new(PBR_SIZE) }),
        }],
    })
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
    fn material_pbr_gives_glass_alpha_below_one() {
        let (_, metallic, roughness, alpha) = material_pbr("glass");
        assert_eq!(alpha, 0.35);
        assert_eq!(roughness, 0.05);
        assert_eq!(metallic, 0.0);
        for name in ["wall", "floor", "ceiling", "roof", "frame", "casing", "leaf", "sill", "header", "stair", "railing", "deck", "ground"] {
            let (_, m, r, a) = material_pbr(name);
            assert_eq!((m, a), (0.0, 1.0), "{name}");
            assert!((0.6..=0.95).contains(&r), "{name} roughness {r}");
        }
    }

    #[test]
    fn material_pbr_falls_back_to_neutral_for_unknown_names() {
        assert_eq!(material_pbr("unobtainium"), ([0.75, 0.75, 0.75], 0.0, 0.5, 1.0));
        assert_eq!(material_pbr(""), ([0.75, 0.75, 0.75], 0.0, 0.5, 1.0));
        let d = Draw::default();
        assert_eq!((d.metallic, d.roughness, d.alpha), (0.0, 0.5, 1.0));
    }

    #[test]
    fn pbr_uniform_block_is_96_bytes() {
        assert_eq!(std::mem::size_of::<PbrUniform>(), 96);
        assert_eq!(std::mem::offset_of!(PbrUniform, sun_intensity), 12);
        assert_eq!(std::mem::offset_of!(PbrUniform, style), 32);
        assert_eq!(std::mem::offset_of!(PbrUniform, camera_pos), 48);
        assert_eq!(std::mem::offset_of!(PbrUniform, shadow_min), 60);
        assert_eq!(std::mem::offset_of!(PbrUniform, shadow_max), 64);
        assert_eq!(std::mem::offset_of!(PbrUniform, _p2), 80);
        for field in [
            "struct PbrUniforms {",
            "light_dir:     vec3<f32>",
            "sun_intensity: f32",
            "metallic:      f32",
            "roughness:     f32",
            "alpha:         f32",
            "style:         u32",
            "camera_pos:    vec3<f32>",
            "shadow_min:    f32",
            "shadow_max:    f32",
            "@group(1) @binding(1) var<uniform> p: PbrUniforms;",
        ] {
            assert!(SHADER.contains(field), "{field}");
        }
    }

    #[test]
    fn msaa_sample_count_is_one_or_four() {
        assert_eq!(valid_sample_count(1), 1);
        assert_eq!(valid_sample_count(4), 4);
        for bad in [0, 2, 8, 16, u32::MAX] {
            assert_eq!(valid_sample_count(bad), 1);
        }
        let Some((device, _queue)) = crate::gputest::shared_device() else { return };
        let _ = GpuPipeline::new(&device, 4);
        let _ = GpuPipeline::new(&device, 2);
    }

    #[test]
    fn flat_normal_still_drives_shading() {
        // The face normal comes from the screen-space derivatives, never the vertex normal.
        let fs = SHADER.split("@fragment").nth(1).unwrap_or("");
        assert!(fs.contains("cross(dpdy(v.wpos), dpdx(v.wpos))"));
        assert!(!fs.contains("v.nrm"));
        assert!(SHADER.contains("arch3d's boxes share 8 corners with averaged normals"));
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

    #[test]
    fn gpu_uniform_new_sanitizes_nan_in_view_proj() {
        let mut vp = [[0.0f32; 4]; 4];
        vp[0][0] = f32::NAN;
        vp[1][2] = f32::INFINITY;
        vp[3][3] = f32::NEG_INFINITY;
        let u = GpuUniform::new(vp, [0.0, 0.0, 0.0], f32::NAN, [f32::NAN, 0.0, 0.0]);
        assert!(u.view_proj[0][0].is_finite());
        assert_eq!(u.view_proj[0][0], 0.0);
        assert!(u.view_proj[1][2].is_finite());
        assert_eq!(u.view_proj[1][2], 0.0);
        assert!(u.view_proj[3][3].is_finite());
        assert_eq!(u.view_proj[3][3], 0.0);
        assert_eq!(u.ambient, 0.0);
        assert_eq!(u.albedo, [0.0, 0.0, 0.0]);
        // Zero light_dir falls back to [0,0,1].
        assert_eq!(u.light_dir, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn upload_empty_scene_produces_placeholder_buffers() {
        let scene = Scene3d::default();
        let Some((device, _queue)) = crate::gputest::shared_device() else { return };
        let meshes = upload(&device, &scene);
        // Empty scene keeps a placeholder vertex (8 floats) and one index.
        assert_eq!(meshes.vertex_count, 0);
        assert_eq!(meshes.index_count, 0);
        assert!(meshes.draws.is_empty());
    }

    #[test]
    fn upload_multiple_meshes_offsets_vertices_correctly() {
        let mut m1 = Mesh::new("wall", ArchId(1));
        for i in 0..3 {
            let x = i as f32;
            m1.positions.push([x, 0.0, 0.0]);
            m1.normals.push([0.0, 0.0, 1.0]);
            m1.uvs.push([x, 0.0]);
        }
        m1.indices = vec![0, 1, 2];

        let mut m2 = Mesh::new("floor", ArchId(2));
        for i in 0..3 {
            let x = i as f32;
            m2.positions.push([x, 1.0, 0.0]);
            m2.normals.push([0.0, 0.0, 1.0]);
            m2.uvs.push([x, 0.0]);
        }
        m2.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![m1, m2], ..Default::default() };
        let Some((device, _queue)) = crate::gputest::shared_device() else { return };
        let meshes = upload(&device, &scene);
        assert_eq!(meshes.vertex_count, 6);
        assert_eq!(meshes.index_count, 6);
        assert_eq!(meshes.draws.len(), 2);
        // Second mesh's indices must be offset by the first mesh's vertex count.
        assert_eq!(meshes.draws[1].first_index, 3);
    }

    #[test]
    fn uniform_stride_is_at_least_uniform_size() {
        let Some((device, _queue)) = crate::gputest::shared_device() else { return };
        let stride = uniform_stride(&device);
        assert!(stride >= UNIFORM_SIZE);
        // Stride must be a multiple of the device's alignment.
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        assert_eq!(stride % align, 0);
    }

    #[test]
    fn gpu_pipeline_new_and_write_uniform() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let gp = GpuPipeline::new(&device, 1);
        let u = GpuUniform::new(
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
            [0.0, 0.0, -1.0],
            0.2,
            [1.0, 1.0, 1.0],
        );
        gp.write_uniform(&queue, &u);
        // Light dir should be normalised to [0,0,-1].
        assert_eq!(u.light_dir, [0.0, 0.0, -1.0]);
    }

    #[test]
    fn material_albedo_unknown_falls_back_to_grey() {
        let albedo = material_albedo("unknown-material");
        assert_eq!(albedo, [0.75, 0.75, 0.75]);
        let empty = material_albedo("");
        assert_eq!(empty, [0.75, 0.75, 0.75]);
    }

    #[test]
    fn upload_handles_missing_normals_and_uvs() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.positions.push([0.0, 0.0, 0.0]);
        mesh.positions.push([1.0, 0.0, 0.0]);
        mesh.positions.push([0.0, 1.0, 0.0]);
        // No normals, no uvs — defaults should be used
        mesh.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
        assert_eq!(meshes.draws.len(), 1);
    }

    #[test]
    fn upload_drops_out_of_range_indices() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.positions.push([0.0, 0.0, 0.0]);
        mesh.positions.push([1.0, 0.0, 0.0]);
        mesh.positions.push([0.0, 1.0, 0.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.uvs.push([0.0, 0.0]);
        mesh.uvs.push([1.0, 0.0]);
        mesh.uvs.push([0.0, 1.0]);
        // First triangle valid, second has index 99 (out of range)
        mesh.indices = vec![0, 1, 2, 0, 1, 99];

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        // Only the first triangle should be kept
        assert_eq!(meshes.index_count, 3);
    }

    #[test]
    fn upload_drops_incomplete_triangle() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.positions.push([0.0, 0.0, 0.0]);
        mesh.positions.push([1.0, 0.0, 0.0]);
        mesh.positions.push([0.0, 1.0, 0.0]);
        mesh.positions.push([1.0, 1.0, 0.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.uvs.push([0.0, 0.0]);
        mesh.uvs.push([1.0, 0.0]);
        mesh.uvs.push([0.0, 1.0]);
        mesh.uvs.push([1.0, 1.0]);
        // 4 indices: first triangle valid, second incomplete (only 1 index)
        mesh.indices = vec![0, 1, 2, 3];

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        // Only the first triangle should be kept
        assert_eq!(meshes.index_count, 3);
    }

    #[test]
    fn upload_handles_nan_and_inf_coordinates() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.positions.push([f32::NAN, 0.0, 0.0]);
        mesh.positions.push([1.0, f32::INFINITY, 0.0]);
        mesh.positions.push([0.0, 0.0, f32::NEG_INFINITY]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.uvs.push([f32::NAN, 0.0]);
        mesh.uvs.push([0.0, f32::NAN]);
        mesh.uvs.push([0.0, 0.0]);
        mesh.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        // Should not crash; NaN/Inf values are passed through to the GPU
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
    }

    #[test]
    fn upload_empty_mesh_in_scene_contributes_nothing() {
        let empty_mesh = Mesh::new("wall", ArchId(1));
        let mut mesh2 = Mesh::new("floor", ArchId(2));
        mesh2.positions.push([0.0, 0.0, 0.0]);
        mesh2.positions.push([1.0, 0.0, 0.0]);
        mesh2.positions.push([0.0, 1.0, 0.0]);
        mesh2.normals.push([0.0, 0.0, 1.0]);
        mesh2.normals.push([0.0, 0.0, 1.0]);
        mesh2.normals.push([0.0, 0.0, 1.0]);
        mesh2.uvs.push([0.0, 0.0]);
        mesh2.uvs.push([1.0, 0.0]);
        mesh2.uvs.push([0.0, 1.0]);
        mesh2.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![empty_mesh, mesh2], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        // Empty mesh contributes 0 vertices; mesh2 contributes 3
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
        // Only mesh2 should have a draw
        assert_eq!(meshes.draws.len(), 1);
        assert_eq!(meshes.draws[0].albedo, material_albedo("floor"));
    }

    #[test]
    fn upload_large_coordinates() {
        let mut mesh = Mesh::new("wall", ArchId(1));
        mesh.positions.push([1e10, 2e10, 3e10]);
        mesh.positions.push([1e10 + 1.0, 2e10, 3e10]);
        mesh.positions.push([1e10, 2e10 + 1.0, 3e10]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.uvs.push([0.0, 0.0]);
        mesh.uvs.push([1.0, 0.0]);
        mesh.uvs.push([0.0, 1.0]);
        mesh.indices = vec![0, 1, 2];

        let scene = Scene3d { meshes: vec![mesh], ..Default::default() };

        let Some((device, _queue)) = crate::gputest::shared_device() else { return };

        let meshes = upload(&device, &scene);
        assert_eq!(meshes.vertex_count, 3);
        assert_eq!(meshes.index_count, 3);
    }
}
