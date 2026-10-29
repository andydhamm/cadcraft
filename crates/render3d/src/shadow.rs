//! One sun shadow map and the ground plane that receives it.
//!
//! [`build`] renders the scene's depth from the sun into a 1024² `Depth32Float` map with an
//! orthographic camera fitted to the scene bounds. The main pass binds the map in group 2
//! (uniform in binding 0, depth texture in binding 1) and shades fragments that lie behind the
//! stored depth with ambient light only. The map is read with `textureLoad`, so no sampler or
//! comparison filtering is needed (WebGL2-safe).

use crate::pipeline::{DEPTH_FORMAT, GpuMeshes, GpuPipeline, vertex_layout};
use crate::types::{ArchId, Bounds3, Mesh};

/// Edge length of the shadow map in texels.
pub const SHADOW_SIZE: u32 = 1024;

/// Margin applied to the light-space extent of the bounds.
const FIT_MARGIN: f64 = 1.2;

/// Depth bias in world units (metres), converted to normalised depth per map.
const BIAS_METRES: f64 = 0.05;

/// The object id of the ground quad; no real arch object uses it.
pub const GROUND_OBJECT: ArchId = ArchId(u32::MAX);

/// WGSL of the depth-only pass.
const DEPTH_SHADER: &str = "
struct Light { vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> light: Light;
@vertex fn vs(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return light.vp * vec4<f32>(pos, 1.0);
}
";

/// Shadow lookup appended to the mesh shader. The mesh `fs` is renamed `fs_base`; the new `fs`
/// returns its colour, or ambient only when the fragment is behind the stored depth.
const LOOKUP_WGSL: &str = "
struct ShadowU { vp: mat4x4<f32>, params: vec4<f32> };
@group(2) @binding(0) var<uniform> sh: ShadowU;
@group(2) @binding(1) var shadow_tex: texture_depth_2d;

@fragment fn fs(v: VOut) -> @location(0) vec4<f32> {
    let c = fs_base(v);
    if (p.style == 2u) {
        return c;
    }
    let lc = sh.vp * vec4<f32>(v.wpos, 1.0);
    let ndc = lc.xyz / lc.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.0 || uv.x >= 1.0 || uv.y < 0.0 || uv.y >= 1.0 || ndc.z < 0.0 || ndc.z > 1.0) {
        return c;
    }
    let dims = vec2<f32>(textureDimensions(shadow_tex));
    let stored = textureLoad(shadow_tex, vec2<i32>(uv * dims), 0);
    if (ndc.z > stored + sh.params.x) {
        let amb = u.albedo * p.ambient * (1.0 - clamp(p.metallic, 0.0, 1.0) * 0.5);
        return vec4<f32>(amb, c.a);
    }
    return c;
}
";

/// Uniform of the main pass: the light matrix and `params.x` = depth bias (normalised).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowUniform {
    light_view_proj: [[f32; 4]; 4],
    params: [f32; 4],
}

/// A built shadow map and the bind group (group 2) that lets the main pass sample it.
pub struct ShadowMap {
    /// The 1024² depth view the sun pass rendered.
    pub view: wgpu::TextureView,
    /// Group 2 of the shadowed main pipelines: uniform (binding 0) and depth view (binding 1).
    pub bind_group: wgpu::BindGroup,
    /// Column-major light view·projection (`clip = M · [p, 1]`, depth 0..1).
    pub light_view_proj: [[f32; 4]; 4],
    /// Edge length in texels.
    pub size: u32,
    #[cfg(test)]
    pub(crate) texture: wgpu::Texture,
}

/// Layout of group 2 of the shadowed main pass.
pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render3d shadow bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZero::new(std::mem::size_of::<ShadowUniform>() as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

/// The shadowed variants of the main-pass pipelines (opaque and glass).
pub(crate) struct ShadowPipelines {
    pub opaque: wgpu::RenderPipeline,
    pub glass: wgpu::RenderPipeline,
}

impl ShadowPipelines {
    /// Build the pipelines over `gpu`'s group 0/1 layouts plus the shadow layout.
    pub fn new(device: &wgpu::Device, gpu: &GpuPipeline) -> Option<Self> {
        const SRC: &str = include_str!("shaders/mesh.wgsl");
        const FS: &str = "@fragment fn fs(v: VOut) -> @location(0) vec4<f32> {";
        if !SRC.contains(FS) {
            return None;
        }
        let source = format!("{}\n{}", SRC.replacen(FS, "fn fs_base(v: VOut) -> vec4<f32> {", 1), LOOKUP_WGSL);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render3d mesh+shadow"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let shadow_bgl = bind_group_layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render3d shadowed pipeline layout"),
            bind_group_layouts: &[Some(&gpu.bgl), Some(&gpu.pbr_bgl), Some(&shadow_bgl)],
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
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState { format: crate::pipeline::COLOR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Some(ShadowPipelines {
            opaque: make("render3d shadowed pipeline", true, None),
            glass: make("render3d shadowed glass pipeline", false, Some(wgpu::BlendState::ALPHA_BLENDING)),
        })
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f64; 3]) -> Option<[f64; 3]> {
    let len = dot(a, a).sqrt();
    (len.is_finite() && len > 1e-12).then(|| [a[0] / len, a[1] / len, a[2] / len])
}

/// Whether `bounds` is a finite box with a positive extent in at least one axis.
fn bounds_usable(b: &Bounds3) -> bool {
    let finite = b.min.iter().chain(b.max.iter()).all(|v| v.is_finite());
    finite && (0..3).all(|i| b.max[i] >= b.min[i]) && (0..3).any(|i| b.max[i] > b.min[i])
}

/// The sun's orthographic view·projection fitted to `bounds` (×[`FIT_MARGIN`]), and the
/// light-space depth range in world units. `None` for an unusable light or bounds.
fn fit(light_dir: [f32; 3], bounds: &Bounds3) -> Option<([[f32; 4]; 4], f64)> {
    if !light_dir.iter().all(|v| v.is_finite()) || !bounds_usable(bounds) {
        return None;
    }
    let d = norm([f64::from(light_dir[0]), f64::from(light_dir[1]), f64::from(light_dir[2])])?;
    if d[2] <= 0.01 {
        return None;
    }
    // The light looks from the sun toward the scene.
    let f = [-d[0], -d[1], -d[2]];
    let up = if d[2] > 0.99 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let r = norm(cross(f, up))?;
    let u = cross(r, f);
    let c = [(bounds.min[0] + bounds.max[0]) * 0.5, (bounds.min[1] + bounds.max[1]) * 0.5, (bounds.min[2] + bounds.max[2]) * 0.5];
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for corner in 0..8u32 {
        let p = [
            if corner & 1 == 0 { bounds.min[0] } else { bounds.max[0] },
            if corner & 2 == 0 { bounds.min[1] } else { bounds.max[1] },
            if corner & 4 == 0 { bounds.min[2] } else { bounds.max[2] },
        ];
        let q = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        for (k, axis) in [r, u, f].iter().enumerate() {
            let v = dot(*axis, q);
            lo[k] = lo[k].min(v);
            hi[k] = hi[k].max(v);
        }
    }
    let half_x = ((hi[0] - lo[0]) * 0.5 * FIT_MARGIN).max(1e-6);
    let half_y = ((hi[1] - lo[1]) * 0.5 * FIT_MARGIN).max(1e-6);
    let mid_x = (hi[0] + lo[0]) * 0.5;
    let mid_y = (hi[1] + lo[1]) * 0.5;
    // Pad depth so geometry just outside the box (the ground ring) still lands in range.
    let pad = ((hi[2] - lo[2]) * 0.1).max(1e-3);
    let near = lo[2] - pad;
    let range = (hi[2] - lo[2]) + 2.0 * pad;
    // clip = M · [p - c, 1]; fold the centre into the translation column.
    let row =
        |axis: [f64; 3], scale: f64, offset: f64| -> [f64; 4] { [axis[0] * scale, axis[1] * scale, axis[2] * scale, -dot(axis, c) * scale - offset] };
    let rows = [row(r, 1.0 / half_x, mid_x / half_x), row(u, 1.0 / half_y, mid_y / half_y), row(f, 1.0 / range, near / range), [0.0, 0.0, 0.0, 1.0]];
    let mut m = [[0.0f32; 4]; 4];
    for (ri, rw) in rows.iter().enumerate() {
        for (ci, v) in rw.iter().enumerate() {
            let v = *v as f32;
            if !v.is_finite() {
                return None;
            }
            m[ci][ri] = v;
        }
    }
    Some((m, range))
}

/// Render the depth of `meshes` from the sun into a [`ShadowMap`]. `light_dir` points from the
/// surface toward the sun. `None` when it is non-finite, at or below the horizon (`z <= 0.01`
/// after normalising) or `bounds` is empty or non-finite. Blended (glass) draws cast nothing.
pub fn build(device: &wgpu::Device, queue: &wgpu::Queue, meshes: &GpuMeshes, light_dir: [f32; 3], bounds: &Bounds3) -> Option<ShadowMap> {
    let (light_view_proj, range) = fit(light_dir, bounds)?;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render3d shadow map"),
        size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        #[cfg(test)]
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        #[cfg(not(test))]
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());

    let uniform = ShadowUniform { light_view_proj, params: [(BIAS_METRES / range) as f32, 0.0, 0.0, 0.0] };
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render3d shadow uniform"),
        size: std::mem::size_of::<ShadowUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniform));
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render3d shadow bind group"),
        layout: &bind_group_layout(device),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
        ],
    });

    // The depth pass has its own group so the map is never bound while it is the attachment.
    let pass_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render3d shadow pass layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZero::new(64),
            },
            count: None,
        }],
    });
    let pass_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render3d shadow pass bind group"),
        layout: &pass_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: std::num::NonZero::new(64) }),
        }],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render3d shadow depth"),
        source: wgpu::ShaderSource::Wgsl(DEPTH_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render3d shadow pass pipeline layout"),
        bind_group_layouts: &[Some(&pass_bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render3d shadow depth pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            buffers: &[Some(vertex_layout())],
            compilation_options: Default::default(),
        },
        // No culling: arch3d's winding is not consistent, and front faces must cast too.
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
        fragment: None,
        multiview_mask: None,
        cache: None,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render3d shadow") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render3d shadow depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if !meshes.draws.is_empty() {
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &pass_group, &[]);
            pass.set_vertex_buffer(0, meshes.vertex_buffer.slice(..));
            pass.set_index_buffer(meshes.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            for d in meshes.draws.iter().filter(|d| !d.is_blended()) {
                pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, 0..1);
            }
        }
    }
    queue.submit([encoder.finish()]);

    Some(ShadowMap {
        view,
        bind_group,
        light_view_proj,
        size: SHADOW_SIZE,
        #[cfg(test)]
        texture,
    })
}

/// The bounds that receive shadows: `scene`, joined with the extent of the
/// [`ground_mesh`] quad when `ground` is drawn. Fit the sun's frustum to this so ground beyond
/// the scene's own box (where a low sun throws long shadows) lands inside the shadow map.
/// Non-finite `scene` bounds are returned unchanged.
pub(crate) fn receiver_bounds(scene: &Bounds3, ground: bool) -> Bounds3 {
    let mut b = Bounds3::new(scene.min, scene.max);
    if !ground {
        return b;
    }
    for p in ground_mesh(scene).positions {
        let q = Bounds3::new([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])], [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        b.extend(&q);
    }
    b
}

/// One upward-facing quad at `z = bounds.min[2]` spanning `bounds` expanded by twice its largest
/// side on every edge, material `"ground"`, object [`GROUND_OBJECT`]. Empty for non-finite bounds.
pub fn ground_mesh(bounds: &Bounds3) -> Mesh {
    let mut m = Mesh::new("ground", GROUND_OBJECT);
    if !bounds.min.iter().chain(bounds.max.iter()).all(|v| v.is_finite()) {
        return m;
    }
    let side = (0..3).map(|i| (bounds.max[i] - bounds.min[i]).abs()).fold(0.0f64, f64::max).max(1.0);
    let grow = 2.0 * side;
    let (x0, x1) = (bounds.min[0].min(bounds.max[0]) - grow, bounds.min[0].max(bounds.max[0]) + grow);
    let (y0, y1) = (bounds.min[1].min(bounds.max[1]) - grow, bounds.min[1].max(bounds.max[1]) + grow);
    let z = bounds.min[2] as f32;
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        let (x, y) = (x as f32, y as f32);
        m.positions.push([x, y, z]);
        m.normals.push([0.0, 0.0, 1.0]);
        m.uvs.push([x, y]);
    }
    m.indices = vec![0, 1, 2, 0, 2, 3];
    m.level_elevation = bounds.min[2];
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offscreen::Offscreen;
    use crate::types::Scene3d;

    fn cuboid(material: &str, min: [f32; 3], max: [f32; 3]) -> Mesh {
        let mut m = Mesh::new(material, ArchId(1));
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let faces: [[[f32; 3]; 4]; 6] = [
            [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
            [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
            [[x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]],
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [[x0, y1, z0], [x1, y1, z0], [x1, y0, z0], [x0, y0, z0]],
        ];
        for quad in faces {
            let base = m.positions.len() as u32;
            for p in quad {
                m.positions.push(p);
                m.normals.push([0.0, 0.0, 1.0]);
                m.uvs.push([p[0], p[2]]);
            }
            m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        m
    }

    /// A 12 m slab with a 2 m × 2 m × 3 m block in the middle.
    fn block_scene() -> Scene3d {
        let mut s = Scene3d::default();
        s.meshes.push(cuboid("floor", [-6.0, -6.0, -0.1], [6.0, 6.0, 0.0]));
        s.meshes.push(cuboid("wall", [-1.0, -1.0, 0.0], [1.0, 1.0, 3.0]));
        s.bounds = Bounds3::new([-6.0, -6.0, -0.1], [6.0, 6.0, 3.0]);
        s
    }

    fn sun_from_south() -> [f32; 3] {
        [0.0, -0.5, 0.8]
    }

    /// Read the whole shadow map back as depths.
    fn read_depth(device: &wgpu::Device, queue: &wgpu::Queue, sm: &ShadowMap) -> Vec<f32> {
        let size = sm.size;
        let row = size * 4;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow readback"),
            size: u64::from(row) * u64::from(size),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &sm.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::DepthOnly },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(size) },
            },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
        let idx = queue.submit([enc.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: Some(std::time::Duration::from_secs(20)) });
        let data = slice.get_mapped_range().map(|r| bytemuck::cast_slice::<u8, f32>(&r).to_vec()).unwrap_or_default();
        buffer.unmap();
        data
    }

    fn depth_at(sm: &ShadowMap, depths: &[f32], p: [f32; 3]) -> f32 {
        let m = &sm.light_view_proj;
        let mut c = [0.0f32; 4];
        for (i, v) in c.iter_mut().enumerate() {
            *v = m[0][i] * p[0] + m[1][i] * p[1] + m[2][i] * p[2] + m[3][i];
        }
        let x = ((c[0] / c[3]) * 0.5 + 0.5) * sm.size as f32;
        let y = (0.5 - (c[1] / c[3]) * 0.5) * sm.size as f32;
        depths.get(y as usize * sm.size as usize + x as usize).copied().unwrap_or(f32::NAN)
    }

    #[test]
    fn shadow_map_is_built_for_a_sun_above_the_horizon() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let scene = block_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        let sm = build(&device, &queue, &meshes, sun_from_south(), &scene.bounds).expect("shadow map");
        assert_eq!(sm.size, 1024);
        assert!(sm.light_view_proj.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn no_shadow_map_when_the_sun_is_at_or_below_the_horizon() {
        let b = Bounds3::new([0.0; 3], [4.0; 3]);
        for dir in [[1.0, 0.0, 0.0], [1.0, 1.0, -1.0], [1.0, 0.0, 0.01], [f32::NAN, 0.0, 1.0], [0.0, f32::INFINITY, 1.0], [0.0; 3]] {
            assert!(fit(dir, &b).is_none(), "{dir:?}");
        }
        assert!(fit([0.0, 0.0, 1.0], &b).is_some());
        assert!(fit([0.0, 0.0, 1.0], &Bounds3::empty()).is_none());
        assert!(fit([0.0, 0.0, 1.0], &Bounds3::new([f64::NAN; 3], [1.0; 3])).is_none());
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let scene = block_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        assert!(build(&device, &queue, &meshes, [1.0, 0.0, 0.0], &scene.bounds).is_none());
        assert!(build(&device, &queue, &meshes, [0.3, 0.3, -0.5], &scene.bounds).is_none());
        assert!(build(&device, &queue, &meshes, sun_from_south(), &Bounds3::empty()).is_none());
    }

    #[test]
    fn shadow_depth_differs_between_the_north_and_south_ground() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let scene = block_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        let sm = build(&device, &queue, &meshes, sun_from_south(), &scene.bounds).expect("shadow map");
        let depths = read_depth(&device, &queue, &sm);
        assert_eq!(depths.len(), (sm.size * sm.size) as usize);
        // The sun is in the south: the block shades the ground just north of it, not the south.
        let north = depth_at(&sm, &depths, [0.0, 2.5, 0.0]);
        let south = depth_at(&sm, &depths, [0.0, -2.5, 0.0]);
        assert!(north.is_finite() && south.is_finite());
        assert!(north < south - 0.01, "north {north} should hold nearer (blocker) depth than south {south}");
    }

    #[test]
    fn ground_mesh_is_one_quad_below_the_scene() {
        let b = Bounds3::new([0.0, 0.0, 0.5], [10.0, 4.0, 3.0]);
        let g = ground_mesh(&b);
        assert_eq!(g.material, "ground");
        assert_eq!(g.object, GROUND_OBJECT);
        assert_eq!((g.positions.len(), g.indices.len()), (4, 6));
        assert!(g.positions.iter().all(|p| p[2] == 0.5));
        let xs: Vec<f32> = g.positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = g.positions.iter().map(|p| p[1]).collect();
        // Largest side is 10 (x): grown by 20 on every edge.
        assert_eq!((xs.iter().cloned().fold(f32::MAX, f32::min), xs.iter().cloned().fold(f32::MIN, f32::max)), (-20.0, 30.0));
        assert_eq!((ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max)), (-20.0, 24.0));
        assert!(ground_mesh(&Bounds3::new([f64::NAN; 3], [1.0; 3])).positions.is_empty());
    }

    #[test]
    fn a_shadowed_render_with_a_ground_plane_darkens_the_shadow() {
        use crate::camera::{Camera, Orbit};
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let mut off = Offscreen::with_device(device, queue, (96, 96));
        let scene = block_scene();
        let light = crate::light::Light::new(sun_from_south(), 0.25);
        let camera = Camera::Orbit(Orbit::at_origin(0.0, 80.0, 30.0, 45.0));
        let map = off.build_shadow(&scene, 1, &light, true).expect("shadow map");
        let opts = crate::offscreen::RenderOptions { shadow: Some(&map), ground: true, ..Default::default() };
        off.render_with(&scene, 1, &camera, &light, &opts);
        let lit = off.read_rgba().expect("readback");
        off.render_with(&scene, 1, &camera, &light, &crate::offscreen::RenderOptions { ground: true, ..Default::default() });
        let plain = off.read_rgba().expect("readback");
        let sum = |p: &[u8]| p.iter().map(|&b| u64::from(b)).sum::<u64>();
        assert!(sum(&lit) < sum(&plain), "the shadow must darken the image");
    }

    #[test]
    fn receiver_bounds_include_the_ground_quad() {
        let b = Bounds3::new([0.0, 0.0, 0.5], [10.0, 4.0, 3.0]);
        let plain = receiver_bounds(&b, false);
        assert_eq!((plain.min, plain.max), (b.min, b.max));
        let r = receiver_bounds(&b, true);
        assert_eq!((r.min, r.max), ([-20.0, -20.0, 0.5], [30.0, 24.0, 3.0]));
        let nan = Bounds3::new([f64::NAN; 3], [1.0; 3]);
        assert!(receiver_bounds(&nan, true).min[0].is_nan());
    }

    /// Mean brightness of the pixels covering the world rectangle `x0..x1`, `y0..y1` at z = 0.
    fn mean_brightness(rgba: &[u8], camera: &crate::camera::Camera, size: (u32, u32), x: (f64, f64), y: (f64, f64)) -> f64 {
        let px = |wx: f64, wy: f64| crate::camera::project_point(camera, size, [wx, wy, 0.0]).expect("on screen");
        let (a, b) = (px(x.0, y.0), px(x.1, y.1));
        let (u0, u1) = (a[0].min(b[0]) as u32, a[0].max(b[0]) as u32);
        let (v0, v1) = (a[1].min(b[1]) as u32, a[1].max(b[1]) as u32);
        let (mut sum, mut n) = (0.0f64, 0.0f64);
        for v in v0..=v1 {
            for u in u0..=u1 {
                let i = (v * size.0 + u) as usize * 4;
                if let Some(p) = rgba.get(i..i + 3) {
                    sum += p.iter().map(|&c| f64::from(c)).sum::<f64>() / 3.0;
                    n += 1.0;
                }
            }
        }
        sum / n.max(1.0)
    }

    /// The check-03 house: 10 × 8 m, 2.4 m walls, as one block on the ground.
    fn house_scene() -> Scene3d {
        let mut s = Scene3d::default();
        s.meshes.push(cuboid("wall", [0.0, 0.0, 0.0], [10.0, 8.0, 2.4]));
        s.bounds = Bounds3::new([0.0, 0.0, 0.0], [10.0, 8.0, 2.4]);
        s
    }

    /// Mean ground brightness (north, south) of the house, top-down, for a sun due south at
    /// `elevation` degrees.
    fn house_ground_brightness(device: wgpu::Device, queue: wgpu::Queue, elevation: f32) -> (f64, f64) {
        use crate::camera::{Camera, Orbit};
        use cadcraft_geom::Vec3;
        let size = (256, 256);
        let mut off = Offscreen::with_device(device, queue, size);
        let scene = house_scene();
        let e = elevation.to_radians();
        // A low ambient: a shadow only removes the direct term, and a 15 degree sun's is small.
        let light = crate::light::Light::new([0.0, -e.cos(), e.sin()], 0.1);
        let camera = Camera::Orbit(Orbit::new(Vec3::new(5.0, 4.0, 0.0), 0.0, 89.0, 60.0, 45.0));
        let map = off.build_shadow(&scene, 1, &light, true).expect("shadow map");
        off.render_with(&scene, 1, &camera, &light, &crate::offscreen::RenderOptions { shadow: Some(&map), ground: true, ..Default::default() });
        let rgba = off.read_rgba().expect("readback");
        let north = mean_brightness(&rgba, &camera, size, (2.0, 8.0), (9.0, 10.0));
        let south = mean_brightness(&rgba, &camera, size, (2.0, 8.0), (-2.0, -1.0));
        (north, south)
    }

    #[test]
    fn shadow_depth_is_one_where_nothing_is_drawn() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        // A small block far from the origin; most of the map should be empty (depth 1.0).
        let mut s = Scene3d::default();
        s.meshes.push(cuboid("wall", [100.0, 100.0, 0.0], [102.0, 102.0, 2.0]));
        s.bounds = Bounds3::new([100.0, 100.0, 0.0], [102.0, 102.0, 2.0]);
        let meshes = crate::pipeline::upload(&device, &s);
        let sm = build(&device, &queue, &meshes, sun_from_south(), &s.bounds).expect("shadow map");
        let depths = read_depth(&device, &queue, &sm);
        // Sample a point within the fitted region but off the block.
        // The fit margin (1.2x) extends the region beyond the block in light space;
        // [101, 99.5, 1] is 0.5 m south of the block, well inside the fitted region.
        let far = depth_at(&sm, &depths, [101.0, 99.5, 1.0]);
        assert!(far.is_finite(), "far point should be finite");
        // The far point should be near the clear depth of 1.0 (nothing drawn there).
        assert!((far - 1.0).abs() < 0.01, "far point depth should be ~1.0, got {far}");
    }

    #[test]
    fn shadow_map_with_empty_meshes() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let s = Scene3d::default();
        let meshes = crate::pipeline::upload(&device, &s);
        // Empty meshes: the map should still be built (all clear depth).
        let sm = build(&device, &queue, &meshes, sun_from_south(), &s.bounds);
        // With empty bounds, fit returns None, so build returns None.
        assert!(sm.is_none(), "empty bounds should yield no shadow map");
        // With non-empty bounds but no meshes, the map should be built.
        let b = Bounds3::new([0.0; 3], [4.0; 3]);
        let sm2 = build(&device, &queue, &meshes, sun_from_south(), &b);
        assert!(sm2.is_some(), "non-empty bounds with no meshes should still build a map");
        let depths = read_depth(&device, &queue, &sm2.unwrap());
        // All depths should be the clear value 1.0.
        for d in depths.iter().take(100) {
            assert!((d - 1.0).abs() < 0.01, "empty map should be clear depth 1.0, got {d}");
        }
    }

    #[test]
    fn a_low_winter_sun_shadows_the_ground_north_of_the_house() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        // Lat 51.05, 21 December, solar noon: about 15.5 degrees above the south horizon.
        let (north, south) = house_ground_brightness(device, queue, 15.5);
        assert!(north < 0.75 * south, "north {north} should be well under south {south}");
    }

    #[test]
    fn a_high_sun_still_shadows_inside_the_house_bounds() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let mut off = Offscreen::with_device(device.clone(), queue.clone(), (96, 96));
        let scene = block_scene();
        let light = crate::light::Light::new(sun_from_south(), 0.25);
        let map = off.build_shadow(&scene, 1, &light, true).expect("shadow map");
        let depths = read_depth(&device, &queue, &map);
        let north = depth_at(&map, &depths, [0.0, 2.5, 0.0]);
        let south = depth_at(&map, &depths, [0.0, -2.5, 0.0]);
        assert!(north < south - 0.01, "north {north} should hold nearer (blocker) depth than south {south}");
    }

    #[test]
    fn shadow_maps_built_counts_builds() {
        let Some((device, queue)) = crate::gputest::shared_device() else { return };
        let mut off = Offscreen::with_device(device, queue, (32, 32));
        let scene = block_scene();
        assert_eq!(off.shadow_maps_built(), 0);
        let light = crate::light::Light::new(sun_from_south(), 0.25);
        assert!(off.build_shadow(&scene, 1, &light, false).is_some());
        assert_eq!(off.shadow_maps_built(), 1);
        let low = crate::light::Light::new([1.0, 0.0, 0.0], 0.25);
        assert!(off.build_shadow(&scene, 1, &low, false).is_none());
        assert_eq!(off.shadow_maps_built(), 1);
        assert!(off.build_shadow(&scene, 1, &light, true).is_some());
        assert_eq!(off.shadow_maps_built(), 2);
    }

    #[test]
    fn ground_mesh_with_coincident_bounds() {
        let b = Bounds3::new([5.0, 3.0, 2.0], [5.0, 3.0, 2.0]);
        let g = ground_mesh(&b);
        assert_eq!(g.positions.len(), 4);
        assert_eq!(g.indices.len(), 6);
        // side = 0, max(1.0) = 1.0, grow = 2.0
        let xs: Vec<f32> = g.positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = g.positions.iter().map(|p| p[1]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), 3.0);
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 7.0);
        assert_eq!(ys.iter().cloned().fold(f32::MAX, f32::min), 1.0);
        assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 5.0);
        assert!(g.positions.iter().all(|p| p[2] == 2.0));
    }

    #[test]
    fn ground_mesh_with_negative_bounds() {
        let b = Bounds3::new([-10.0, -5.0, -2.0], [-2.0, 3.0, 1.0]);
        let g = ground_mesh(&b);
        assert_eq!(g.positions.len(), 4);
        // Largest side is 8 (x: -10 to -2), grow = 16
        let xs: Vec<f32> = g.positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = g.positions.iter().map(|p| p[1]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), -26.0);
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 14.0);
        assert_eq!(ys.iter().cloned().fold(f32::MAX, f32::min), -21.0);
        assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 19.0);
        assert!(g.positions.iter().all(|p| p[2] == -2.0));
    }

    #[test]
    fn fit_with_very_elongated_bounds() {
        let b = Bounds3::new([0.0, 0.0, 0.0], [1000.0, 1.0, 1.0]);
        let (m, range) = fit([0.0, -0.5, 0.8], &b).expect("fit should succeed");
        assert!(m.iter().flatten().all(|v| v.is_finite()));
        assert!(range > 0.0);
    }

    #[test]
    fn fit_with_sun_directly_overhead() {
        let b = Bounds3::new([0.0; 3], [4.0; 3]);
        let (m, range) = fit([0.0, 0.0, 1.0], &b).expect("fit should succeed for overhead sun");
        assert!(m.iter().flatten().all(|v| v.is_finite()));
        assert!(range > 0.0);
    }

    #[test]
    fn ground_mesh_with_zero_extent_in_one_axis() {
        // A flat plane: zero extent in z
        let b = Bounds3::new([0.0, 0.0, 0.0], [10.0, 5.0, 0.0]);
        let g = ground_mesh(&b);
        assert_eq!(g.positions.len(), 4);
        assert_eq!(g.indices.len(), 6);
        // Largest side is 10 (x), grow = 20
        let xs: Vec<f32> = g.positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = g.positions.iter().map(|p| p[1]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), -20.0);
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 30.0);
        assert_eq!(ys.iter().cloned().fold(f32::MAX, f32::min), -20.0);
        assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 25.0);
        assert!(g.positions.iter().all(|p| p[2] == 0.0));
    }

    #[test]
    fn shadow_light_view_proj_projects_center_to_origin() {
        let b = Bounds3::new([-2.0, -2.0, 0.0], [2.0, 2.0, 4.0]);
        let (m, _range) = fit([0.0, -0.5, 0.8], &b).expect("fit should succeed");
        let c = [0.0f32, 0.0f32, 2.0f32];
        let mut clip = [0.0f32; 4];
        for (i, v) in clip.iter_mut().enumerate() {
            *v = m[0][i] * c[0] + m[1][i] * c[1] + m[2][i] * c[2] + m[3][i];
        }
        assert!((clip[0] / clip[3]).abs() < 0.1, "center x should be near 0, got {}", clip[0] / clip[3]);
        assert!((clip[1] / clip[3]).abs() < 0.1, "center y should be near 0, got {}", clip[1] / clip[3]);
        let z = clip[2] / clip[3];
        assert!(z >= 0.0 && z <= 1.0, "center z should be in [0,1], got {}", z);
    }
}
