//! Fixture shadow maps (phase 10): one `Depth32Float` 2D-array texture, one layer per spot light
//! and six per point light (a cube, faces in `lights::cube_face` order), each layer
//! `size`² texels. Read with `textureLoad` like the sun map (WebGL2-safe).
//!
//! A light's maps are cached under its fixture id with [`light_key`] and the scene's
//! [`occluder_key`]: [`ShadowAtlas::update`] renders a light's maps again only when one of the
//! two changed (or the light was not shadowed before). Fixture housings never cast shadows
//! (`Draw::casts_shadow`), so moving one fixture re-renders that fixture's maps only, and a
//! camera move renders none.
//!
//! Filled here: [`ShadowAtlas::new`] (a 1×1×1 placeholder), the accessors. Stubs filled by
//! `p10-r3d-shadow-atlas`: [`ShadowAtlas::update`], [`light_key`], [`occluder_key`].

use crate::lights::ShadowSlot;
use crate::pipeline::{DEPTH_FORMAT, GpuMeshes};
use crate::types::Scene3d;
use cadcraft_arch3d::light::SceneLight;

/// The fixture shadow maps of one renderer (the interactive view and Finalize each own one).
pub struct ShadowAtlas {
    size: u32,
    layers: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    built: u64,
}

impl ShadowAtlas {
    /// An empty atlas whose layers will be `size`² texels (clamped to 1..=the device's 2D limit).
    /// It holds a 1×1×1 placeholder until [`ShadowAtlas::update`] needs layers.
    pub fn new(device: &wgpu::Device, size: u32) -> Self {
        let size = size.clamp(1, device.limits().max_texture_dimension_2d.max(1));
        let (texture, view) = Self::create(device, 1, 1);
        ShadowAtlas { size, layers: 0, texture, view, built: 0 }
    }

    /// A `side`² × `layers` depth array and its `D2Array` view.
    pub(crate) fn create(device: &wgpu::Device, side: u32, layers: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render3d fixture shadow atlas"),
            size: wgpu::Extent3d { width: side.max(1), height: side.max(1), depth_or_array_layers: layers.max(1) },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("render3d fixture shadow atlas view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        (texture, view)
    }

    /// The `D2Array` view group 2 binds at binding 3.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Layer edge in texels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Layers in use (0 while nothing is shadowed).
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// Lights whose maps this atlas has rendered so far (a cube counts once).
    pub fn maps_built(&self) -> u64 {
        self.built
    }

    #[cfg(test)]
    pub(crate) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Bring the maps of the `shadowed` lights (indices into `lights`) up to date and return their
    /// slots. `occluders` is [`occluder_key`] of the scene `meshes` were uploaded from; `far` is
    /// the far plane for every map (the caller passes the distance across the scene bounds).
    /// Lights no longer shadowed lose their slot; layers are reused.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &GpuMeshes,
        occluders: u64,
        lights: &[SceneLight],
        shadowed: &[usize],
        far: f32,
    ) -> Vec<(usize, ShadowSlot)> {
        let _ = (device, queue, meshes, occluders, lights, shadowed, far, &self.texture);
        Vec::new()
    }
}

/// A hash of everything about a light its shadow map depends on: kind, position, direction and
/// both cone cosines (bit patterns of the f64s), not its colour or intensity.
pub fn light_key(light: &SceneLight) -> u64 {
    let _ = light;
    0
}

/// A hash of the shadow casters of `scene`: every mesh whose material is not a fixture's
/// (`arch3d::light::is_light_material`), its positions and indices. Equal scenes give equal keys;
/// moving a fixture leaves it unchanged.
pub fn occluder_key(scene: &Scene3d) -> u64 {
    let _ = scene;
    0
}
