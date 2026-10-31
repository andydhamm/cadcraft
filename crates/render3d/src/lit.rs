//! The lit pipelines (phase 10): the mesh shader plus the sun's shadow and every fixture light
//! (spot cones, cube and spot shadow maps from `shadow_atlas`), with tone mapping for Finalize.
//! Used when `RenderOptions::lit` is set; without it the phase-6 pipelines draw exactly as before.
//!
//! Group 2 is described in `crate::lights`. Filled here: [`bind_group_layout`],
//! [`frame_uniform`], [`LitFrame::new`], the placeholder sun map. Stub filled by
//! `p10-r3d-lit-pipeline`: [`LitPipelines::new`] (and `shaders/lit.wgsl`).

use crate::lights::{FRAME_UNIFORM_SIZE, LIGHTS_UNIFORM_SIZE};
use crate::pipeline::{DEPTH_FORMAT, GpuPipeline};
use crate::shadow::ShadowMap;

/// How a lit frame shades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitParams {
    /// The sun lights the scene (false = night: no sun, `lights::NIGHT_AMBIENT`).
    pub sun_on: bool,
    /// Sun radiance scale when on (1 = the phase-6 sun).
    pub sun_intensity: f32,
    /// Ambient level.
    pub ambient: f32,
    /// Display units per lux of fixture light (`lights::DEFAULT_EXPOSURE`).
    pub exposure: f32,
    /// Tone-map the result (Finalize Render); the interactive view writes linear colour.
    pub tonemap: bool,
    /// PCF radius in texels for fixture and sun shadows (0 = one tap; Finalize uses 2).
    pub pcf_radius: u32,
}

impl Default for LitParams {
    fn default() -> Self {
        LitParams { sun_on: true, sun_intensity: 1.0, ambient: 0.25, exposure: crate::lights::DEFAULT_EXPOSURE, tonemap: false, pcf_radius: 0 }
    }
}

/// Group 2 of the lit pipelines: the frame uniform, the sun map, the lights uniform and the
/// fixture shadow atlas (see `crate::lights`).
pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let uniform = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZero::new(size),
        },
        count: None,
    };
    let depth = |binding: u32, view_dimension: wgpu::TextureViewDimension| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension, multisampled: false },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render3d lit group 2"),
        entries: &[
            uniform(0, FRAME_UNIFORM_SIZE),
            depth(1, wgpu::TextureViewDimension::D2),
            uniform(2, LIGHTS_UNIFORM_SIZE),
            depth(3, wgpu::TextureViewDimension::D2Array),
        ],
    })
}

/// The `FrameU` bytes: the sun map's matrix and bias when there is one, the sun flags and the
/// shading parameters; `atlas_size` is the fixture atlas's layer edge.
pub fn frame_uniform(sun: Option<&ShadowMap>, params: &LitParams, atlas_size: u32) -> Vec<u8> {
    let mut words: Vec<f32> = Vec::with_capacity(24);
    let vp = sun.map(|m| m.light_view_proj).unwrap_or([[0.0; 4]; 4]);
    for col in vp {
        words.extend_from_slice(&col);
    }
    let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
    words.extend_from_slice(&[sun.map(|m| m.bias).unwrap_or(0.0), if sun.is_some() { 1.0 } else { 0.0 }, if params.sun_on { 1.0 } else { 0.0 }, 0.0]);
    words.extend_from_slice(&[
        finite(params.exposure, crate::lights::DEFAULT_EXPOSURE).max(0.0),
        if params.tonemap { 1.0 } else { 0.0 },
        params.pcf_radius.min(8) as f32,
        atlas_size as f32,
    ]);
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    debug_assert_eq!(bytes.len() as u64, FRAME_UNIFORM_SIZE);
    bytes
}

/// One frame's group 2 and the sun values the per-draw PBR block takes.
pub struct LitFrame {
    pub bind_group: wgpu::BindGroup,
    /// `PbrUniform` sun intensity for this frame (0 at night).
    pub sun_intensity: f32,
    /// `PbrUniform` ambient for this frame.
    pub ambient: f32,
    /// Lights drawn and lights shadowed in this frame.
    pub lights: usize,
    pub shadowed: usize,
}

impl LitFrame {
    /// Upload the two uniforms and bind them with the sun map (`None`: a 1×1 placeholder) and the
    /// fixture atlas view.
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        frame: &[u8],
        sun: Option<&wgpu::TextureView>,
        lights: &[u8],
        atlas: &wgpu::TextureView,
        params: &LitParams,
        counts: (usize, usize),
    ) -> Self {
        use wgpu::util::DeviceExt;
        let buf = |label: &str, bytes: &[u8], size: u64| {
            let mut data = bytes.to_vec();
            data.resize(size as usize, 0);
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: &data, usage: wgpu::BufferUsages::UNIFORM })
        };
        let frame_buf = buf("render3d lit frame", frame, FRAME_UNIFORM_SIZE);
        let lights_buf = buf("render3d lit lights", lights, LIGHTS_UNIFORM_SIZE);
        let placeholder = sun.is_none().then(|| placeholder_depth(device));
        let sun_view = sun.or(placeholder.as_ref()).unwrap_or(atlas);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render3d lit group 2"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(sun_view) },
                wgpu::BindGroupEntry { binding: 2, resource: lights_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(atlas) },
            ],
        });
        let (sun_intensity, ambient) =
            if params.sun_on { (params.sun_intensity, params.ambient) } else { (0.0, crate::lights::NIGHT_AMBIENT.min(params.ambient)) };
        LitFrame { bind_group, sun_intensity, ambient, lights: counts.0, shadowed: counts.1 }
    }
}

/// A 1×1 `D2` depth view standing in for an absent sun map.
fn placeholder_depth(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("render3d lit sun placeholder"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

/// The lit variants of the main-pass pipelines (opaque and glass) over `gpu`'s group 0/1 layouts
/// plus [`bind_group_layout`].
pub(crate) struct LitPipelines {
    pub opaque: wgpu::RenderPipeline,
    pub glass: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
}

impl LitPipelines {
    /// Build the pipelines; `None` when the shader cannot be assembled.
    pub fn new(device: &wgpu::Device, gpu: &GpuPipeline) -> Option<Self> {
        let _ = (device, gpu);
        None
    }
}
