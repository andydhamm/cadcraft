//! Offscreen wgpu renderer: colour + depth targets, a lit depth-tested draw, and a bounded
//! synchronous readback for headless PNG export. The 3D pane uses the same renderer on the
//! window's device ([`Offscreen::with_device`]) and records into the frame's encoder
//! ([`Offscreen::encode`]).
//!
//! Building block drafted outside the swarm (assisted, 2026-10-25; see `docs/arch3d/DESIGN.md`).

use std::sync::mpsc;
use std::time::Duration;

use crate::Render3dError;
use crate::camera::{Camera, view_proj};
use crate::light::Light;
use crate::pipeline::{COLOR_FORMAT, DEPTH_FORMAT, GpuMeshes, GpuPipeline, GpuUniform, UNIFORM_SIZE, uniform_stride, upload};
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

/// Per-draw uniform slots: one buffer, one bind group, addressed by dynamic offset.
struct Slots {
    capacity: u64,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Offscreen renderer that draws a [`Scene3d`] into its own colour + depth targets.
pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    targets: Targets,
    gpu: GpuPipeline,
    stride: u64,
    slots: Option<Slots>,
    meshes: Option<(u64, GpuMeshes)>,
    uploads: u64,
    generation: u64,
}

impl Offscreen {
    /// A renderer with its own device on the best available adapter.
    /// [`Render3dError::NoAdapter`] when there is none (headless, no GPU).
    pub async fn new(instance: &wgpu::Instance, size: (u32, u32)) -> Result<Self, Render3dError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
            .await
            .map_err(|_| Render3dError::NoAdapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor { label: Some("render3d"), ..Default::default() })
            .await
            .map_err(|e| Render3dError::Wgpu(e.to_string()))?;
        Ok(Self::with_device(device, queue, size))
    }

    /// Blocking form of [`Offscreen::new`] for the CLI and tests (native only: a browser
    /// cannot block on adapter requests).
    pub fn create(size: (u32, u32)) -> Result<Self, Render3dError> {
        let instance = headless_instance();
        futures::executor::block_on(Self::new(&instance, size))
    }

    /// A renderer on an existing device, e.g. the window's (eframe's render state).
    pub fn with_device(device: wgpu::Device, queue: wgpu::Queue, size: (u32, u32)) -> Self {
        let size = clamp_size(&device, size);
        let targets = Targets::new(&device, size);
        let gpu = GpuPipeline::new(&device, 1);
        let stride = uniform_stride(&device);
        Offscreen { device, queue, targets, gpu, stride, slots: None, meshes: None, uploads: 0, generation: 1 }
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

    /// Record a lit, depth-tested draw of `scene` into `encoder`. `scene_key` identifies the
    /// scene's content (e.g. its arch revision): meshes are uploaded again only when it changes.
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene3d, scene_key: u64, camera: &Camera, light: &Light) {
        if self.meshes.as_ref().is_none_or(|(k, _)| *k != scene_key) {
            self.meshes = Some((scene_key, upload(&self.device, scene)));
            self.uploads += 1;
        }
        let Some((_, meshes)) = self.meshes.as_ref() else { return };
        let draws = meshes.draws.len() as u64;
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
            self.slots = Some(Slots { capacity, buffer, bind_group });
        }
        let aspect = f64::from(self.targets.width) / f64::from(self.targets.height);
        let vp = view_proj(camera, aspect);
        let l = light.normalized();
        if let Some(slots) = &self.slots {
            let mut bytes = vec![0u8; (draws * self.stride) as usize];
            for (i, d) in meshes.draws.iter().enumerate() {
                let u = GpuUniform::new(vp, l.dir, l.ambient, d.albedo);
                let at = i * self.stride as usize;
                bytes[at..at + UNIFORM_SIZE as usize].copy_from_slice(bytemuck::bytes_of(&u));
            }
            if !bytes.is_empty() {
                self.queue.write_buffer(&slots.buffer, 0, &bytes);
            }
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
        pass.set_pipeline(&self.gpu.pipeline);
        pass.set_vertex_buffer(0, meshes.vertex_buffer.slice(..));
        pass.set_index_buffer(meshes.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (i, d) in meshes.draws.iter().enumerate() {
            pass.set_bind_group(0, &slots.bind_group, &[(i as u64 * self.stride) as u32]);
            pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, 0..1);
        }
    }

    /// Draw `scene` and submit it (see [`Offscreen::encode`]).
    pub fn render(&mut self, scene: &Scene3d, scene_key: u64, camera: &Camera, light: &Light) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render3d") });
        self.encode(&mut encoder, scene, scene_key, camera, light);
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
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
                .await
                .ok()?;
            adapter.request_device(&wgpu::DeviceDescriptor { label: Some("render3d"), ..Default::default() }).await.ok()
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

/// Render a scene headlessly with the default light and return a PNG.
/// Returns `Render3dError::NoAdapter` if no GPU is available.
pub async fn headless_png(scene: &Scene3d, camera: &Camera, size: (u32, u32)) -> Result<Vec<u8>, Render3dError> {
    render_png(scene, camera, &Light::DEFAULT, size)
}

#[cfg(test)]
#[path = "offscreen_tests.rs"]
mod tests;
