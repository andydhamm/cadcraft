//! Offscreen wgpu renderer for headless PNG export.

use crate::Render3dError;
use crate::camera::Camera;
use image::ImageEncoder;

/// Largest width or height accepted for an offscreen target: wgpu's default
/// `max_texture_dimension_2d`. Bigger (or zero) sizes fail wgpu validation, which panics
/// by default, so they are refused before any texture is created.
pub const MAX_SIZE: u32 = 8192;

/// Refuse an offscreen size that wgpu would reject (zero or above [`MAX_SIZE`]).
pub fn check_size(size: (u32, u32)) -> Result<(), Render3dError> {
    let (w, h) = size;
    if w == 0 || h == 0 || w > MAX_SIZE || h > MAX_SIZE {
        return Err(Render3dError::NotAvailable(format!("image size {w}x{h} is outside 1..={MAX_SIZE}")));
    }
    Ok(())
}

/// Offscreen renderer that renders to a texture and reads back as PNG.
pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
    texture: wgpu::Texture,
}

impl Offscreen {
    /// Create a new offscreen renderer at the given size.
    pub async fn new(instance: &wgpu::Instance, size: (u32, u32)) -> Result<Self, Render3dError> {
        check_size(size)?;
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

        // The draw pass (not written yet) will create its colour and depth views here.
        Ok(Offscreen { device, queue, width, height, texture })
    }

    /// Render a scene with the given camera.
    ///
    /// Not written yet: there is no pipeline, shader or depth target. It says so instead
    /// of leaving the colour target uninitialised for [`Offscreen::read_png`] to hand back as
    /// a "rendered" image (an all-black picture used to pass the phase-1 orbit check).
    pub fn render(&self, scene: &cadcraft_arch3d::Scene3d, camera: &Camera, size: (u32, u32)) -> Result<(), Render3dError> {
        let _ = (scene, camera, size);
        Err(Render3dError::NotAvailable("the 3D draw pass is not implemented yet".into()))
    }

    /// Read the rendered color attachment back as a PNG.
    pub async fn read_png(&self) -> Result<Vec<u8>, Render3dError> {
        let row_bytes = self.width as u64 * 4;
        // `TexelCopyBufferInfo::bytes_per_row` must be a multiple of
        // `COPY_BYTES_PER_ROW_ALIGNMENT` (a power of two, >= 4). Round the raw pixel
        // row (`width * 4`) up to the next aligned value so the readback copy always
        // respects the alignment check, while never being smaller than the pixels
        // it must hold (so we never read past the valid row).
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64;
        let bytes_per_row = row_bytes.div_ceil(align) * align;
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
                    bytes_per_row: Some(u32::try_from(bytes_per_row).map_err(|_| Render3dError::Buffer("row too wide".into()))?),
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

        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(5)) })
            .map_err(|e| Render3dError::Wgpu(format!("readback did not finish: {e}")))?;
        rx.await.map_err(|e| Render3dError::Wgpu(format!("channel error: {e}")))?.map_err(|e| Render3dError::Wgpu(e.to_string()))?;

        let full = slice.get_mapped_range().to_vec();
        // Each row is `bytes_per_row` bytes; the first `row_bytes` are the valid pixels.
        // Re-pack into a tight contiguous RGBA buffer (dropping per-row padding).
        let row = self.width as u64 * 4;
        let mut pixels: Vec<u8> = Vec::with_capacity((row * u64::from(self.height)) as usize);
        for r in 0..u64::from(self.height) {
            let start = (r * bytes_per_row) as usize;
            let line = full.get(start..start + row as usize).ok_or_else(|| Render3dError::Buffer("short readback".into()))?;
            pixels.extend_from_slice(line);
        }
        let img =
            image::RgbaImage::from_raw(self.width, self.height, pixels).ok_or_else(|| Render3dError::Buffer("invalid image data".to_string()))?;

        let mut png_data = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
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
    offscreen.render(scene, camera, size)?;
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
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
        assert!(rt.is_ok(), "tokio runtime");
        let Ok(rt) = rt else { return };
        let result = rt.block_on(headless_png(&scene, &camera, (64, 64)));
        // Either we got a PNG or NoAdapter (on CI without GPU)
        // No adapter (CI) or an adapter but no draw pass yet: never a fake picture.
        assert!(matches!(result, Err(Render3dError::NoAdapter | Render3dError::NotAvailable(_))), "unexpected result: {result:?}");
    }

    #[test]
    fn sizes_wgpu_would_reject_are_refused_up_front() {
        assert!(check_size((64, 64)).is_ok());
        assert!(check_size((MAX_SIZE, MAX_SIZE)).is_ok());
        for bad in [(0, 64), (64, 0), (MAX_SIZE + 1, 10), (10, u32::MAX)] {
            assert!(matches!(check_size(bad), Err(Render3dError::NotAvailable(_))), "{bad:?}");
        }
        let scene = cadcraft_arch3d::Scene3d { meshes: Vec::new(), bounds: cadcraft_arch3d::Bounds3::empty(), revision: 0 };
        let camera = Camera::Orbit(crate::camera::Orbit::at_origin(30.0, 30.0, 10.0, 45.0));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
        assert!(rt.is_ok());
        if let Ok(rt) = rt {
            let r = rt.block_on(headless_png(&scene, &camera, (0, 0)));
            assert!(matches!(r, Err(Render3dError::NotAvailable(_))), "{r:?}");
        }
    }
}
