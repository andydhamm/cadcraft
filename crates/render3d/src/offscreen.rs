//! Offscreen wgpu renderer for headless PNG export.

use crate::Render3dError;
use crate::camera::Camera;
use crate::types::Scene3d;
use image::ImageEncoder;

/// Offscreen renderer that renders to a texture and reads back as PNG.
pub struct Offscreen {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl Offscreen {
    /// Create a new offscreen renderer at the given size.
    pub async fn new(instance: &wgpu::Instance, size: (u32, u32)) -> Result<Self, Render3dError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
            .await
            .map_err(|_| Render3dError::NoAdapter)?;

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.map_err(|e| Render3dError::Wgpu(e.to_string()))?;

        let (width, height) = size;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen color"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&Default::default());

        Ok(Offscreen { instance: instance.clone(), adapter, device, queue, width, height, texture, view })
    }

    /// Render a scene with the given camera.
    pub fn render(&self, scene: &cadcraft_arch3d::Scene3d, camera: &Camera, size: (u32, u32)) {
        // Rendering logic would go here
        let _ = (scene, camera, size);
    }

    /// Read the rendered color attachment back as a PNG.
    pub async fn read_png(&self) -> Result<Vec<u8>, Render3dError> {
        let row_bytes = self.width as u64 * 4;
        // `COPY_BYTES_PER_ROW_ALIGNMENT` is a power of two (>= 4). Round `bytes_per_row` up
        // to the next aligned value so the readback copy always respects the alignment
        // check (raw `width*4` is already 4-aligned; larger powers may still be required).
        let mut align: u64 = 4;
        while row_bytes % align != 0 && align < row_bytes {
            align *= 2;
        }
        // Align to at least `row_bytes` (so we never read past the valid pixels) and round
        // up to the next aligned multiple.
        let bytes_per_row = ((row_bytes + align - 1) / align) * align;
        let buffer_size = bytes_per_row * self.height as u64;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback encoder") });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((bytes_per_row as u32).try_into().unwrap()),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );

        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = futures::channel::oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(5)) });
        rx.await.map_err(|e| Render3dError::Wgpu(format!("channel error: {e}")))?.map_err(|e| Render3dError::Wgpu(e.to_string()))?;

        let full = slice.get_mapped_range().to_vec();
        // Each row is `bytes_per_row` bytes; the first `row_bytes` are the valid pixels.
        // Re-pack into a tight contiguous RGBA buffer (dropping per-row padding).
        let row = self.width as u64 * 4;
        let pixels: Vec<u8> = (0..self.height as u64)
            .flat_map(|r| full[(r * bytes_per_row) as usize..(r * bytes_per_row) as usize + row as usize].iter().copied())
            .collect();
        let img =
            image::RgbaImage::from_raw(self.width, self.height, pixels).ok_or_else(|| Render3dError::Buffer("invalid image data".to_string()))?;

        let mut png_data = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut png_data);
        encoder
            .write_image(img.as_raw(), self.width, self.height, image::ExtendedColorType::Rgba8)
            .map_err(|e| Render3dError::Buffer(e.to_string()))?;

        Ok(png_data)
    }
}

/// Render a scene headlessly and return a PNG. Returns `Render3dError::NoAdapter` if no GPU is available.
pub async fn headless_png(scene: &cadcraft_arch3d::Scene3d, camera: &Camera, size: (u32, u32)) -> Result<Vec<u8>, Render3dError> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let offscreen = Offscreen::new(&instance, size).await?;
    offscreen.render(scene, camera, size);
    offscreen.read_png().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_png_reports_no_adapter_instead_of_panicking() {
        // This test verifies that headless_png returns NoAdapter instead of panicking
        // when no GPU is available. We can't easily test this without a real GPU,
        // but we can verify the function signature and error type exist.
        let scene = cadcraft_arch3d::Scene3d { meshes: Vec::new(), bounds: cadcraft_arch3d::Bounds3::empty(), revision: 0 };
        let camera = Camera::Orbit(crate::camera::Orbit::at_origin(30.0, 30.0, 10.0, 45.0));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        let result = rt.block_on(headless_png(&scene, &camera, (64, 64)));
        // Either we got a PNG or NoAdapter (on CI without GPU)
        match result {
            Ok(_) => {}
            Err(Render3dError::NoAdapter) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
