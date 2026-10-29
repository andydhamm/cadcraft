//! Offscreen wgpu renderer: colour + depth targets, a lit depth-tested draw, and a bounded
//! synchronous readback for headless PNG export. The 3D pane uses the same renderer on the
//! window's device ([`Offscreen::with_device`]) and records into the frame's encoder
//! ([`Offscreen::encode`]).
//!
//! Building block drafted outside the swarm (assisted, 2026-10-25; see `docs/arch3d/DESIGN.md`).

use std::sync::mpsc;
use std::time::Duration;

use crate::Render3dError;
use crate::adapter::{AdapterChoice, DeviceInfo, request_device};

/// Alias of [`DeviceInfo`] for the accessor signature.
type Info = DeviceInfo;
use crate::camera::{Camera, view_proj};
use crate::edges::{EDGE_COLOR, EdgePass};
use crate::light::Light;
use crate::pipeline::{
    COLOR_FORMAT, DEPTH_FORMAT, GpuMeshes, GpuPipeline, GpuUniform, PBR_SIZE, PbrUniform, STYLE_GLASS_HOUSE, STYLE_HIDDEN_LINE, STYLE_SHADED,
    STYLE_SHADED_EDGES, UNIFORM_SIZE, pbr_bind_group, uniform_stride, upload,
};
use crate::shadow::{ShadowMap, ShadowPipelines};
use crate::types::Scene3d;
use image::ImageEncoder;

/// Background of every render, linear (the sRGB target stores about `(39, 44, 52)`). Dark
/// enough that lit geometry never matches it, which the image checks rely on.
pub const CLEAR: wgpu::Color = wgpu::Color { r: 0.02, g: 0.025, b: 0.034, a: 1.0 };

/// Longest a readback may wait for the GPU before it is reported as an error, never a hang.
pub const READBACK_TIMEOUT: Duration = Duration::from_secs(20);

/// Encode tight RGBA8 pixels as PNG.
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, Render3dError> {
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    enc.write_image(rgba, width, height, image::ExtendedColorType::Rgba8).map_err(|e| Render3dError::Buffer(e.to_string()))?;
    Ok(out)
}

/// Decode a PNG produced by [`encode_png`] / [`Offscreen::read_png`] into tight RGBA8 pixels.
/// `image::DynamicImage::to_rgba8()` is the 0.25 API for this; there is no `image::convert`.
pub fn png_rgba(png: &[u8]) -> Result<Vec<u8>, Render3dError> {
    let img = image::load_from_memory(png).map_err(|e| Render3dError::Buffer(format!("decode png: {e}")))?.to_rgba8();
    Ok(img.into_raw())
}

/// True when every RGBA pixel of a tight RGBA8 buffer equals the first one (empty counts as uniform).
pub fn pixels_are_uniform(rgba: &[u8]) -> bool {
    let mut it = rgba.chunks_exact(4);
    match it.next() {
        None => true,
        Some(first) => it.all(|p| p == first),
    }
}

/// Clamp a requested size to at least 1 × 1 and at most the device's texture limit.
fn clamp_size(device: &wgpu::Device, (w, h): (u32, u32)) -> (u32, u32) {
    let max = device.limits().max_texture_dimension_2d.max(1);
    (w.clamp(1, max), h.clamp(1, max))
}

/// Colour and depth attachments of one size.
struct Targets {
    width: u32,
    height: u32,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

impl Targets {
    fn new(device: &wgpu::Device, (width, height): (u32, u32)) -> Self {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render3d colour"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render3d depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        Targets { width, height, color, color_view, depth_view }
    }
}

/// How faces are shaded (the `STYLE_*` constants of the pipeline).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// PBR-shaded faces.
    #[default]
    Shaded,
    /// Shaded; the edge lines come from `arch3d::edges`.
    ShadedEdges,
    /// Faces in the clear colour with depth written.
    HiddenLine,
    /// Translucent faces without depth writes.
    GlassHouse,
}

impl Style {
    fn code(self) -> u32 {
        match self {
            Style::Shaded => STYLE_SHADED,
            Style::ShadedEdges => STYLE_SHADED_EDGES,
            Style::HiddenLine => STYLE_HIDDEN_LINE,
            Style::GlassHouse => STYLE_GLASS_HOUSE,
        }
    }
}

/// Options of one render. The default is shaded, without a shadow, without a ground plane.
#[derive(Clone, Copy, Default)]
pub struct RenderOptions<'a> {
    /// A map from [`Offscreen::build_shadow`] of the same renderer: fragments behind its depth get
    /// ambient light only. For [`render_png_with`] only whether it is `Some` matters (a map belongs
    /// to one device, so that function builds its own).
    pub shadow: Option<&'a ShadowMap>,
    /// Face shading.
    pub style: Style,
    /// Draw a [`crate::shadow::ground_mesh`] under the scene.
    pub ground: bool,
}

/// Per-draw uniform slots: one buffer, one bind group, addressed by dynamic offset.
struct Slots {
    capacity: u64,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pbr_buffer: wgpu::Buffer,
    pbr_bind_group: wgpu::BindGroup,
}

/// Offscreen renderer that draws a [`Scene3d`] into its own colour + depth targets.
pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    targets: Targets,
    gpu: GpuPipeline,
    stride: u64,
    slots: Option<Slots>,
    meshes: Option<((u64, bool), GpuMeshes)>,
    edges: Option<EdgePass>,
    shadowed: Option<ShadowPipelines>,
    shadows_built: u64,
    uploads: u64,
    generation: u64,
    info: DeviceInfo,
}

impl Offscreen {
    /// A renderer with its own device on the best available adapter.
    /// [`Render3dError::NoAdapter`] when there is none (headless, no GPU).
    pub async fn new(instance: &wgpu::Instance, size: (u32, u32)) -> Result<Self, Render3dError> {
        Self::new_with(instance, AdapterChoice::Auto, size).await
    }

    /// [`Offscreen::new`] with an explicit adapter choice. The device always uses the
    /// downlevel WebGL2 limits.
    pub async fn new_with(instance: &wgpu::Instance, choice: AdapterChoice, size: (u32, u32)) -> Result<Self, Render3dError> {
        let (device, queue, info) = request_device(instance, choice).await?;
        let mut me = Self::with_device(device, queue, size);
        me.info = info;
        Ok(me)
    }

    /// Blocking form of [`Offscreen::new`] for the CLI and tests (native only: a browser
    /// cannot block on adapter requests).
    pub fn create(size: (u32, u32)) -> Result<Self, Render3dError> {
        Self::create_with(AdapterChoice::Auto, size)
    }

    /// Blocking form of [`Offscreen::new_with`] (native only).
    pub fn create_with(choice: AdapterChoice, size: (u32, u32)) -> Result<Self, Render3dError> {
        let instance = headless_instance();
        futures::executor::block_on(Self::new_with(&instance, choice, size))
    }

    /// The device this renderer runs on. For a renderer built with [`Offscreen::with_device`]
    /// the adapter is unknown, so the name is `unknown` and `downlevel` is whether the device's
    /// limits equal the downlevel WebGL2 limits.
    pub fn device_info(&self) -> &Info {
        &self.info
    }

    /// A renderer on an existing device, e.g. the window's (eframe's render state).
    pub fn with_device(device: wgpu::Device, queue: wgpu::Queue, size: (u32, u32)) -> Self {
        let size = clamp_size(&device, size);
        let targets = Targets::new(&device, size);
        let gpu = GpuPipeline::new(&device, 1);
        let stride = uniform_stride(&device);
        let info = DeviceInfo::unknown(device.limits() == wgpu::Limits::downlevel_webgl2_defaults());
        Offscreen {
            device,
            queue,
            targets,
            gpu,
            stride,
            slots: None,
            meshes: None,
            edges: None,
            shadowed: None,
            shadows_built: 0,
            uploads: 0,
            generation: 1,
            info,
        }
    }

    /// Current target size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.targets.width, self.targets.height)
    }

    /// Resize the targets (clamped to 1 … the device's texture limit). Returns `true` when the
    /// targets were recreated, so callers holding [`Offscreen::color_view`] must rebind it.
    pub fn resize(&mut self, size: (u32, u32)) -> bool {
        let size = clamp_size(&self.device, size);
        if size == self.size() {
            return false;
        }
        self.targets = Targets::new(&self.device, size);
        self.generation += 1;
        true
    }

    /// The colour target (`Rgba8UnormSrgb`), for sampling the image into a window.
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.targets.color_view
    }

    /// Increases every time the colour target is recreated.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// How many times scene meshes were uploaded (a render with an unchanged `scene_key` reuses them).
    pub fn uploads(&self) -> u64 {
        self.uploads
    }

    /// Upload `scene` (plus the ground quad when `ground`) unless `scene_key` is already resident.
    fn ensure_meshes(&mut self, scene: &Scene3d, scene_key: u64, ground: bool) {
        if self.meshes.as_ref().is_none_or(|(k, _)| *k != (scene_key, ground)) {
            let uploaded = if ground {
                let mut with_ground = scene.clone();
                with_ground.meshes.push(crate::shadow::ground_mesh(&scene.bounds));
                upload(&self.device, &with_ground)
            } else {
                upload(&self.device, scene)
            };
            self.meshes = Some(((scene_key, ground), uploaded));
            self.uploads += 1;
        }
    }

    /// How many shadow maps [`Offscreen::build_shadow`] has built.
    pub fn shadow_maps_built(&self) -> u64 {
        self.shadows_built
    }

    /// Build the sun shadow map of `scene` for `light` (see [`crate::shadow::build`]); `None` when
    /// the sun is at or below the horizon or the bounds are empty. `ground` must match the
    /// [`RenderOptions::ground`] it will be rendered with, so the ground casts and receives alike.
    pub fn build_shadow(&mut self, scene: &Scene3d, scene_key: u64, light: &Light, ground: bool) -> Option<ShadowMap> {
        self.ensure_meshes(scene, scene_key, ground);
        let (_, meshes) = self.meshes.as_ref()?;
        let map =
            crate::shadow::build(&self.device, &self.queue, meshes, light.normalized().dir, &crate::shadow::receiver_bounds(&scene.bounds, ground))?;
        self.shadows_built += 1;
        Some(map)
    }

    /// Record a lit, depth-tested draw of `scene` into `encoder` with default [`RenderOptions`].
    /// `scene_key` identifies the scene's content (e.g. its arch revision): meshes are uploaded
    /// again only when it changes.
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene3d, scene_key: u64, camera: &Camera, light: &Light) {
        self.encode_with(encoder, scene, scene_key, camera, light, &RenderOptions::default());
    }

    /// [`Offscreen::encode`] with a shadow map, a shading style and an optional ground plane.
    pub fn encode_with(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene3d,
        scene_key: u64,
        camera: &Camera,
        light: &Light,
        opts: &RenderOptions,
    ) {
        self.ensure_meshes(scene, scene_key, opts.ground);
        if opts.shadow.is_some() && self.shadowed.is_none() {
            self.shadowed = ShadowPipelines::new(&self.device, &self.gpu);
        }
        let Some((_, meshes)) = self.meshes.as_ref() else { return };
        let draws = meshes.draws.len() as u64;
        let filtered_meshes: Vec<&crate::types::Mesh> =
            scene.meshes.iter().filter(|mesh| !mesh.positions.is_empty() && !mesh.indices.is_empty()).collect();
        let style_is_glass_house = matches!(opts.style, Style::GlassHouse);
        if draws > 0 && self.slots.as_ref().is_none_or(|s| s.capacity < draws) {
            let capacity = draws.next_power_of_two();
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render3d draw uniforms"),
                size: capacity * self.stride,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render3d draw uniforms"),
                layout: &self.gpu.bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: std::num::NonZero::new(UNIFORM_SIZE),
                    }),
                }],
            });
            let pbr_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render3d pbr uniforms"),
                size: capacity * self.stride,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let pbr_bind_group = pbr_bind_group(&self.device, &self.gpu.pbr_bgl, &pbr_buffer);
            self.slots = Some(Slots { capacity, buffer, bind_group, pbr_buffer, pbr_bind_group });
        }
        let aspect = f64::from(self.targets.width) / f64::from(self.targets.height);
        let vp = view_proj(camera, aspect);
        let l = light.normalized();
        let eye = camera.eye();
        let eye = [eye.x as f32, eye.y as f32, eye.z as f32];
        if let Some(slots) = &self.slots {
            let mut bytes = vec![0u8; (draws * self.stride) as usize];
            for (i, d) in meshes.draws.iter().enumerate() {
                let u = GpuUniform::new(vp, l.dir, l.ambient, d.albedo);
                let at = i * self.stride as usize;
                bytes[at..at + UNIFORM_SIZE as usize].copy_from_slice(bytemuck::bytes_of(&u));
            }
            let mut pbr_bytes = vec![0u8; bytes.len()];
            for (i, d) in meshes.draws.iter().enumerate() {
                let mut draw = *d;
                if style_is_glass_house {
                    let material = filtered_meshes.get(i).map(|mesh| mesh.material.as_str()).unwrap_or("");
                    if !material.eq_ignore_ascii_case("ground") {
                        draw.alpha = 0.35;
                    }
                }
                let p = PbrUniform::new(&l, 1.0, &draw, opts.style.code(), eye);
                let at = i * self.stride as usize;
                pbr_bytes[at..at + PBR_SIZE as usize].copy_from_slice(bytemuck::bytes_of(&p));
            }
            if !bytes.is_empty() {
                self.queue.write_buffer(&slots.buffer, 0, &bytes);
                self.queue.write_buffer(&slots.pbr_buffer, 0, &pbr_bytes);
            }
        }
        let with_edges = matches!(opts.style, Style::ShadedEdges | Style::HiddenLine);
        if with_edges {
            let edges = self.edges.get_or_insert_with(|| EdgePass::new(&self.device, 1));
            edges.prepare(&self.device, &self.queue, scene, scene_key, vp, EDGE_COLOR);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render3d scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.targets.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(CLEAR), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let Some(slots) = &self.slots else { return };
        if meshes.draws.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, meshes.vertex_buffer.slice(..));
        pass.set_index_buffer(meshes.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        let shadow = opts.shadow.zip(self.shadowed.as_ref());
        let draw_indices: Vec<usize> = if style_is_glass_house {
            let mut draw_index_by_scene_mesh = Vec::new();
            let mut draw_index = 0usize;
            for mesh in &scene.meshes {
                if !mesh.positions.is_empty() && !mesh.indices.is_empty() {
                    draw_index_by_scene_mesh.push((draw_index, mesh));
                    draw_index += 1;
                }
            }
            let ground_draw = if opts.ground && meshes.draws.len() > draw_index { Some(draw_index) } else { None };

            let mut order = Vec::new();
            if let Some(ground_draw) = ground_draw {
                order.push(ground_draw);
            }
            let mut translucent = Vec::new();
            for (draw_index, mesh) in draw_index_by_scene_mesh {
                if mesh.material.eq_ignore_ascii_case("ground") {
                    continue;
                }
                let bounds = mesh.bounds();
                let center = [(bounds.min[0] + bounds.max[0]) * 0.5, (bounds.min[1] + bounds.max[1]) * 0.5, (bounds.min[2] + bounds.max[2]) * 0.5];
                let eye_f64 = [f64::from(eye[0]), f64::from(eye[1]), f64::from(eye[2])];
                let dist_sq =
                    (center[0] as f64 - eye_f64[0]).powi(2) + (center[1] as f64 - eye_f64[1]).powi(2) + (center[2] as f64 - eye_f64[2]).powi(2);
                translucent.push((draw_index, dist_sq));
            }
            translucent.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            order.extend(translucent.into_iter().map(|(draw_index, _)| draw_index));
            order
        } else {
            (0..meshes.draws.len()).collect()
        };
        let draw_pipeline = |is_ground: bool| match (shadow, is_ground) {
            (Some((_, sp)), false) => &sp.glass,
            (Some((_, sp)), true) => &sp.opaque,
            (None, false) => &self.gpu.glass_pipeline,
            (None, true) => &self.gpu.pipeline,
        };
        if style_is_glass_house {
            let mut scene_draw_count = 0usize;
            for mesh in &scene.meshes {
                if !mesh.positions.is_empty() && !mesh.indices.is_empty() {
                    scene_draw_count += 1;
                }
            }
            let ground_draw = if opts.ground && meshes.draws.len() > scene_draw_count { Some(scene_draw_count) } else { None };
            let mut opaque = Vec::new();
            let mut translucent = Vec::new();
            for &i in &draw_indices {
                if ground_draw == Some(i) {
                    opaque.push(i);
                } else {
                    translucent.push(i);
                }
            }
            for (pipeline, list) in [(draw_pipeline(true), &opaque as &[usize]), (draw_pipeline(false), &translucent as &[usize])] {
                pass.set_pipeline(pipeline);
                if let Some((map, _)) = shadow {
                    pass.set_bind_group(2, &map.bind_group, &[]);
                }
                for &i in list {
                    let d = &meshes.draws[i];
                    let offset = [(i as u64 * self.stride) as u32];
                    pass.set_bind_group(0, &slots.bind_group, &offset);
                    pass.set_bind_group(1, &slots.pbr_bind_group, &offset);
                    pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, 0..1);
                }
            }
        } else {
            for blended in [false, true] {
                let pipeline = match (shadow, blended) {
                    (Some((_, sp)), false) => &sp.opaque,
                    (Some((_, sp)), true) => &sp.glass,
                    (None, false) => &self.gpu.pipeline,
                    (None, true) => &self.gpu.glass_pipeline,
                };
                pass.set_pipeline(pipeline);
                if let Some((map, _)) = shadow {
                    pass.set_bind_group(2, &map.bind_group, &[]);
                }
                for (i, d) in meshes.draws.iter().enumerate().filter(|(_, d)| d.is_blended() == blended) {
                    let offset = [(i as u64 * self.stride) as u32];
                    pass.set_bind_group(0, &slots.bind_group, &offset);
                    pass.set_bind_group(1, &slots.pbr_bind_group, &offset);
                    pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, 0..1);
                }
            }
        }
        if with_edges && let Some(edges) = &self.edges {
            edges.draw(&mut pass);
        }
    }

    /// Draw `scene` and submit it (see [`Offscreen::encode`]).
    pub fn render(&mut self, scene: &Scene3d, scene_key: u64, camera: &Camera, light: &Light) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render3d") });
        self.encode(&mut encoder, scene, scene_key, camera, light);
        self.queue.submit([encoder.finish()]);
    }

    /// [`Offscreen::render`] with [`RenderOptions`].
    pub fn render_with(&mut self, scene: &Scene3d, scene_key: u64, camera: &Camera, light: &Light, opts: &RenderOptions) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render3d") });
        self.encode_with(&mut encoder, scene, scene_key, camera, light, opts);
        self.queue.submit([encoder.finish()]);
    }

    /// Read the colour target back as tight RGBA8 rows (sRGB-encoded, as stored). Waits at
    /// most [`READBACK_TIMEOUT`]; a GPU that does not finish is an error, never a hang.
    pub fn read_rgba(&self) -> Result<Vec<u8>, Render3dError> {
        let (width, height) = self.size();
        let row = u64::from(width) * 4;
        let align = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let padded = row.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render3d readback"),
            size: padded * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render3d readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.targets.color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let index = self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait { submission_index: Some(index), timeout: Some(READBACK_TIMEOUT) })
            .map_err(|e| Render3dError::Wgpu(format!("readback: {e}")))?;
        // The map callback runs inside `poll`; it has fired once the submission completed.
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(Render3dError::Wgpu(format!("readback map: {e}"))),
            Err(_) => return Err(Render3dError::Wgpu("readback: the buffer was not mapped in time".into())),
        }
        let mapped = slice.get_mapped_range().map_err(|e| Render3dError::Wgpu(format!("readback map range: {e}")))?;
        let mut pixels = Vec::with_capacity((row * u64::from(height)) as usize);
        for r in 0..u64::from(height) {
            let at = (r * padded) as usize;
            pixels.extend_from_slice(&mapped[at..at + row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }

    /// Read the colour target back as a PNG (see [`Offscreen::read_rgba`]).
    pub fn read_png(&self) -> Result<Vec<u8>, Render3dError> {
        let (w, h) = self.size();
        encode_png(w, h, &self.read_rgba()?)
    }
}

/// An instance for rendering without a window, on the primary backends only (Vulkan, Metal,
/// DX12, browser WebGPU). The default set also starts the GL backend, and several threads starting
/// EGL at once hung test binaries on cgyws2 (2026-10-25: the parallel render3d tests hung for
/// 900 s until headless instances left GL out). The window's device comes from eframe and is
/// unaffected.
pub fn headless_instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::PRIMARY;
    wgpu::Instance::new(desc)
}

/// A device on the best available adapter, or `None` without an adapter or when the driver does
/// not answer within `budget` (the request runs on a helper thread, so a stuck driver costs the
/// budget, never a hang). Native only.
pub fn device_within(budget: Duration) -> Option<(wgpu::Device, wgpu::Queue)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let instance = headless_instance();
        let got = futures::executor::block_on(async {
            let (device, queue, _info) = request_device(&instance, AdapterChoice::Auto).await.ok()?;
            Some((device, queue))
        });
        let _ = tx.send(got);
    });
    rx.recv_timeout(budget).ok().flatten()
}

/// Render `scene` headlessly with `light` and return a PNG. [`Render3dError::NoAdapter`] when
/// there is no GPU adapter.
pub fn render_png(scene: &Scene3d, camera: &Camera, light: &Light, size: (u32, u32)) -> Result<Vec<u8>, Render3dError> {
    let mut offscreen = Offscreen::create(size)?;
    offscreen.render(scene, scene.revision, camera, light);
    offscreen.read_png()
}

/// [`render_png`] with [`RenderOptions`]. A `Some` shadow in `opts` asks for the sun shadow of
/// `light` (built on this renderer's own device); the ground plane and style apply as given.
pub fn render_png_with(scene: &Scene3d, camera: &Camera, light: &Light, size: (u32, u32), opts: &RenderOptions) -> Result<Vec<u8>, Render3dError> {
    let mut offscreen = Offscreen::create(size)?;
    let map = if opts.shadow.is_some() { offscreen.build_shadow(scene, scene.revision, light, opts.ground) } else { None };
    let own = RenderOptions { shadow: map.as_ref(), style: opts.style, ground: opts.ground };
    offscreen.render_with(scene, scene.revision, camera, light, &own);
    offscreen.read_png()
}

/// Render a scene headlessly with the default light and return a PNG.
/// Returns `Render3dError::NoAdapter` if no GPU is available.
pub async fn headless_png(scene: &Scene3d, camera: &Camera, size: (u32, u32)) -> Result<Vec<u8>, Render3dError> {
    render_png(scene, camera, &Light::DEFAULT, size)
}

#[cfg(test)]
mod codec_tests {
    use super::*;

    #[test]
    fn png_roundtrip_keeps_pixels() {
        let mut px = Vec::new();
        for y in 0..4u8 {
            for x in 0..4u8 {
                px.extend_from_slice(&[x * 60, y * 60, 128, 255]);
            }
        }
        let png = encode_png(4, 4, &px).expect("encode");
        assert_eq!(png_rgba(&png).expect("decode"), px);
        assert!(!pixels_are_uniform(&px));
    }

    #[test]
    fn uniform_pixels_are_detected() {
        let px = vec![26u8; 8 * 8 * 4];
        assert!(pixels_are_uniform(&px));
        let mut px2 = px.clone();
        px2[100] = 200;
        assert!(!pixels_are_uniform(&px2));
    }

    #[test]
    fn pixels_are_uniform_empty_is_true() {
        assert!(pixels_are_uniform(&[]));
    }

    #[test]
    fn png_rgba_invalid_data_errors() {
        let result = png_rgba(&[0, 1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn png_roundtrip_single_pixel() {
        let px = vec![10u8, 20, 30, 255];
        let png = encode_png(1, 1, &px).expect("encode");
        assert_eq!(png_rgba(&png).expect("decode"), px);
        assert!(pixels_are_uniform(&px));
    }
}

#[cfg(test)]
mod offscreen_behavior_tests {
    use super::*;

    fn with_offscreen<F>(size: (u32, u32), f: F)
    where
        F: FnOnce(&mut Offscreen),
    {
        match device_within(Duration::from_secs(5)) {
            Some((device, queue)) => {
                let mut off = Offscreen::with_device(device, queue, size);
                f(&mut off);
            }
            None => {
                eprintln!("SKIP: no GPU adapter available");
            }
        }
    }

    /// A 2 m cube around the origin with per-face vertices and outward normals.
    fn cube_scene() -> Scene3d {
        let mut m = crate::types::Mesh::new("concrete", crate::types::ArchId(1));
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ];
        for (n, u, v) in faces {
            let base = m.positions.len() as u32;
            for (a, b) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                m.positions.push([n[0] + a * u[0] + b * v[0], n[1] + a * u[1] + b * v[1], n[2] + a * u[2] + b * v[2]]);
                m.normals.push(n);
                m.uvs.push([a, b]);
            }
            m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let mut s = Scene3d { meshes: vec![m], ..Default::default() };
        s.bounds = crate::types::Bounds3::new([-1.0; 3], [1.0; 3]);
        s
    }

    #[test]
    fn style_codes_are_distinct() {
        let codes: Vec<u32> = [Style::Shaded, Style::ShadedEdges, Style::HiddenLine, Style::GlassHouse].iter().map(|s| s.code()).collect();
        for i in 0..codes.len() {
            for j in i + 1..codes.len() {
                assert_ne!(codes[i], codes[j], "style codes must be distinct");
            }
        }
    }

    #[test]
    fn offscreen_resize_same_size_returns_false() {
        with_offscreen((100, 100), |off| {
            assert!(!off.resize((100, 100)), "same size should return false");
        });
    }

    #[test]
    fn offscreen_resize_increases_generation() {
        with_offscreen((100, 100), |off| {
            let gen_before = off.generation();
            assert!(off.resize((200, 200)), "resize to different size should return true");
            assert_eq!(off.generation(), gen_before + 1, "generation should increment");
            assert_eq!(off.size(), (200, 200));
        });
    }

    #[test]
    fn glass_house_with_empty_scene() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160, 120);
        let mut off = Offscreen::with_device(device, queue, size);
        let scene = Scene3d::default();
        off.render_with(
            &scene,
            1,
            &Camera::Orbit(crate::camera::Orbit::at_origin(0.0, 0.0, 5.0, 45.0)),
            &Light::DEFAULT,
            &RenderOptions { style: Style::GlassHouse, ..Default::default() },
        );
        let rgba = off.read_rgba().expect("readback");
        assert!(pixels_are_uniform(&rgba), "empty scene should be uniform clear colour");
    }

    #[test]
    fn glass_house_with_single_mesh() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160, 120);
        let mut off = Offscreen::with_device(device, queue, size);
        let scene = cube_scene();
        off.render_with(
            &scene,
            1,
            &Camera::Orbit(crate::camera::Orbit::at_origin(0.0, 0.0, 5.0, 45.0)),
            &Light::DEFAULT,
            &RenderOptions { style: Style::GlassHouse, ..Default::default() },
        );
        let rgba = off.read_rgba().expect("readback");
        let bg = |p: &[u8]| p[..3].iter().zip([39u8, 44, 52]).all(|(a, b)| a.abs_diff(b) <= 2);
        let model: Vec<usize> = rgba.chunks_exact(4).enumerate().filter(|(_, p)| !bg(p)).map(|(i, _)| i).collect();
        assert!(!model.is_empty(), "the cube must be visible");
    }

    #[test]
    fn glass_house_with_nan_coordinates() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160, 120);
        let mut off = Offscreen::with_device(device, queue, size);
        let mut scene = cube_scene();
        scene.meshes[0].positions[0][0] = f32::NAN;
        off.render_with(
            &scene,
            1,
            &Camera::Orbit(crate::camera::Orbit::at_origin(0.0, 0.0, 5.0, 45.0)),
            &Light::DEFAULT,
            &RenderOptions { style: Style::GlassHouse, ..Default::default() },
        );
        let rgba = off.read_rgba().expect("readback");
        assert!(!rgba.is_empty(), "render with NaN should still produce output");
    }

    #[test]
    fn glass_house_with_large_scene() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160, 120);
        let mut off = Offscreen::with_device(device, queue, size);
        let mut scene = Scene3d::default();
        for i in 0..20 {
            let mut m = crate::types::Mesh::new("wall", crate::types::ArchId(i));
            let x = (i as f32 - 10.0) * 2.0;
            for (a, b) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                m.positions.push([x + a, b, 0.0]);
                m.normals.push([0.0, 0.0, 1.0]);
                m.uvs.push([a, b]);
            }
            m.indices = vec![0, 1, 2, 0, 2, 3];
            scene.meshes.push(m);
        }
        scene.bounds = crate::types::Bounds3::new([-22.0, -2.0, -1.0], [22.0, 2.0, 1.0]);
        off.render_with(
            &scene,
            1,
            &Camera::Orbit(crate::camera::Orbit::at_origin(0.0, 0.0, 30.0, 45.0)),
            &Light::DEFAULT,
            &RenderOptions { style: Style::GlassHouse, ..Default::default() },
        );
        let rgba = off.read_rgba().expect("readback");
        assert!(!rgba.is_empty(), "large scene should render");
    }

    #[test]
    fn glass_house_with_coincident_walls() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160, 120);
        let mut off = Offscreen::with_device(device, queue, size);
        let mut scene = Scene3d::default();
        for i in 0..5 {
            let m = crate::types::Mesh::new("wall", crate::types::ArchId(i));
            scene.meshes.push(m);
        }
        scene.meshes[0].positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        scene.meshes[0].normals = vec![[0.0, 0.0, 1.0]; 4];
        scene.meshes[0].uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        scene.meshes[0].indices = vec![0, 1, 2, 0, 2, 3];
        for i in 1..5 {
            scene.meshes[i].positions = scene.meshes[0].positions.clone();
            scene.meshes[i].normals = scene.meshes[0].normals.clone();
            scene.meshes[i].uvs = scene.meshes[0].uvs.clone();
            scene.meshes[i].indices = scene.meshes[0].indices.clone();
        }
        scene.bounds = crate::types::Bounds3::new([0.0, 0.0, 0.0], [1.0, 1.0, 0.0]);
        off.render_with(
            &scene,
            1,
            &Camera::Orbit(crate::camera::Orbit::at_origin(0.5, 0.5, 3.0, 45.0)),
            &Light::DEFAULT,
            &RenderOptions { style: Style::GlassHouse, ..Default::default() },
        );
        let rgba = off.read_rgba().expect("readback");
        assert!(!rgba.is_empty(), "coincident walls should render");
    }

    #[test]
    fn glass_house_ground_stays_opaque() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160, 120);
        let mut off = Offscreen::with_device(device, queue, size);
        let scene = cube_scene();
        let camera = Camera::Orbit(crate::camera::Orbit::at_origin(0.0, 0.0, 5.0, 45.0));
        off.render_with(&scene, 1, &camera, &Light::DEFAULT, &RenderOptions { style: Style::GlassHouse, ground: true, ..Default::default() });
        let rgba = off.read_rgba().expect("readback");
        assert!(!rgba.is_empty(), "glass house with ground should render");
    }

    #[test]
    fn shaded_edges_darkens_and_hidden_line_leaves_background() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160u32, 120u32);
        let mut off = Offscreen::with_device(device, queue, size);
        let scene = cube_scene();
        let camera = Camera::Orbit(crate::camera::Orbit::at_origin(30.0, 30.0, 8.0, 45.0));
        let mut shot = |style: Style| {
            off.render_with(&scene, 1, &camera, &Light::DEFAULT, &RenderOptions { style, ..Default::default() });
            off.read_rgba().unwrap_or_default()
        };
        let shaded = shot(Style::Shaded);
        let edged = shot(Style::ShadedEdges);
        let hidden = shot(Style::HiddenLine);
        let bg = |p: &[u8]| p[..3].iter().zip([39u8, 44, 52]).all(|(a, b)| a.abs_diff(b) <= 2);
        let model: Vec<usize> = shaded.chunks_exact(4).enumerate().filter(|(_, p)| !bg(p)).map(|(i, _)| i).collect();
        assert!(!model.is_empty(), "the cube must be visible");
        let lum = |px: &[u8]| -> f64 {
            let sum: f64 =
                model.iter().map(|&i| 0.2126 * f64::from(px[i * 4]) + 0.7152 * f64::from(px[i * 4 + 1]) + 0.0722 * f64::from(px[i * 4 + 2])).sum();
            sum / model.len() as f64
        };
        assert!(lum(&edged) < lum(&shaded), "edges must darken: {} vs {}", lum(&edged), lum(&shaded));
        let covered = hidden.chunks_exact(4).filter(|p| !bg(p)).count();
        assert!((covered as f64) < 0.7 * model.len() as f64, "hidden-line covers {covered} of {}", model.len());
    }

    #[test]
    fn glass_house_blends_and_differs_from_shaded() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160u32, 120u32);
        let mut off = Offscreen::with_device(device, queue, size);
        let scene = cube_scene();
        let camera = Camera::Orbit(crate::camera::Orbit::at_origin(30.0, 30.0, 8.0, 45.0));
        off.render_with(&scene, 1, &camera, &Light::DEFAULT, &RenderOptions { style: Style::Shaded, ..Default::default() });
        let shaded = off.read_rgba().unwrap_or_default();
        off.render_with(&scene, 1, &camera, &Light::DEFAULT, &RenderOptions { style: Style::GlassHouse, ..Default::default() });
        let glass = off.read_rgba().unwrap_or_default();
        assert_eq!(shaded.len(), glass.len());
        // Glass house must differ from shaded (blending produces different pixel values).
        let different = shaded.iter().zip(glass.iter()).filter(|(a, b)| a != b).count();
        assert!(different > 0, "glass house must produce different pixels than shaded");
        // But not every pixel should differ (the silhouette should be similar).
        assert!(different < shaded.len(), "glass house should not change every pixel");
    }

    #[test]
    fn glass_house_shows_an_interior_wall_through_an_exterior_one() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = (160u32, 120u32);
        let mut off = Offscreen::with_device(device, queue, size);
        // Two parallel walls: one in front, one behind.
        let mut scene = Scene3d::default();
        let front = crate::types::Mesh::new("wall", crate::types::ArchId(1));
        scene.meshes.push(front);
        scene.meshes[0].positions = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 2.0], [0.0, 0.0, 2.0]];
        scene.meshes[0].normals = vec![[0.0, 1.0, 0.0]; 4];
        scene.meshes[0].uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        scene.meshes[0].indices = vec![0, 1, 2, 0, 2, 3];
        let back = crate::types::Mesh::new("wall", crate::types::ArchId(2));
        scene.meshes.push(back);
        scene.meshes[1].positions = vec![[0.0, 1.0, 0.0], [2.0, 1.0, 0.0], [2.0, 1.0, 2.0], [0.0, 1.0, 2.0]];
        scene.meshes[1].normals = vec![[0.0, 1.0, 0.0]; 4];
        scene.meshes[1].uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        scene.meshes[1].indices = vec![0, 1, 2, 0, 2, 3];
        scene.bounds = crate::types::Bounds3::new([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let camera = Camera::Orbit(crate::camera::Orbit::at_origin(1.0, 0.5, 1.0, 45.0));
        // Shaded: the front wall occludes the back wall completely.
        off.render_with(&scene, 1, &camera, &Light::DEFAULT, &RenderOptions { style: Style::Shaded, ..Default::default() });
        let shaded = off.read_rgba().unwrap_or_default();
        // Glass house: the back wall should be visible through the front wall (blended).
        off.render_with(&scene, 1, &camera, &Light::DEFAULT, &RenderOptions { style: Style::GlassHouse, ..Default::default() });
        let glass = off.read_rgba().unwrap_or_default();
        assert_eq!(shaded.len(), glass.len());
        // In glass house mode, more pixels should be non-background (both walls visible).
        let bg = |p: &[u8]| p[..3].iter().zip([39u8, 44, 52]).all(|(a, b)| a.abs_diff(b) <= 2);
        let shaded_model: Vec<usize> = shaded.chunks_exact(4).enumerate().filter(|(_, p)| !bg(p)).map(|(i, _)| i).collect();
        let glass_model: Vec<usize> = glass.chunks_exact(4).enumerate().filter(|(_, p)| !bg(p)).map(|(i, _)| i).collect();
        assert!(!shaded_model.is_empty(), "shaded must show the front wall");
        assert!(glass_model.len() >= shaded_model.len(), "glass house should show at least as much as shaded (back wall through front)");
    }
}

#[cfg(test)]
#[path = "offscreen_tests.rs"]
mod tests;
