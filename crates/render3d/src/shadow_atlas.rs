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

use crate::lights::{MAX_SHADOW_LAYERS, SHADOW_NEAR, ShadowSlot, cube_face_view_proj, spot_view_proj};
use crate::pipeline::{DEPTH_FORMAT, GpuMeshes, vertex_layout};
use crate::types::{ArchId, Scene3d};
use cadcraft_arch3d::light::{LightKind, SceneLight};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

/// WGSL of the depth-only pass (the same as the sun's).
const DEPTH_SHADER: &str = "
struct Light { vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> light: Light;
@vertex fn vs(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return light.vp * vec4<f32>(pos, 1.0);
}
";

/// Depth bias of every fixture map (m).
const SLOT_BIAS: f32 = 0.02;

/// The fixture shadow maps of one renderer (the interactive view and Finalize each own one).
pub struct ShadowAtlas {
    size: u32,
    layers: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    built: u64,
    /// Layers the texture really has (`layers` counts those in use).
    capacity: u32,
    /// Per fixture: its [`light_key`], first layer and whether it is a cube.
    cache: BTreeMap<ArchId, (u64, u32, bool)>,
    /// The [`occluder_key`] the cached maps were rendered against.
    occluders: u64,
}

impl ShadowAtlas {
    /// An empty atlas whose layers will be `size`² texels (clamped to 1..=the device's 2D limit).
    /// It holds a 1×1×1 placeholder until [`ShadowAtlas::update`] needs layers.
    pub fn new(device: &wgpu::Device, size: u32) -> Self {
        let size = size.clamp(1, device.limits().max_texture_dimension_2d.max(1));
        let (texture, view) = Self::create(device, 1, 1);
        ShadowAtlas { size, layers: 0, texture, view, built: 0, capacity: 0, cache: BTreeMap::new(), occluders: 0 }
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
        let cap = MAX_SHADOW_LAYERS.min(device.limits().max_texture_array_layers).max(1);
        // Assign layers in `shadowed` order; stop at the first light that does not fit.
        let mut plan: Vec<(usize, u32, bool, Option<[[f32; 4]; 4]>)> = Vec::new();
        let mut total = 0u32;
        for &i in shadowed {
            let Some(light) = lights.get(i) else { continue };
            let spot = match light.kind {
                LightKind::Spot => spot_view_proj(light, SHADOW_NEAR, far),
                _ => None,
            };
            let need = if spot.is_some() { 1 } else { 6 };
            if total + need > cap {
                break;
            }
            plan.push((i, total, spot.is_none(), spot));
            total += need;
        }

        let mut full = self.occluders != occluders;
        if total > self.capacity {
            let (texture, view) = Self::create(device, self.size, total);
            self.texture = texture;
            self.view = view;
            self.capacity = total;
            full = true;
        }
        self.occluders = occluders;
        self.layers = total;

        let mut next = BTreeMap::new();
        let mut jobs: Vec<(u32, [[f32; 4]; 4])> = Vec::new();
        let mut slots = Vec::with_capacity(plan.len());
        for &(i, first, cube, spot) in &plan {
            let Some(light) = lights.get(i) else { continue };
            let key = light_key(light);
            let fresh = full || self.cache.get(&light.id) != Some(&(key, first, cube));
            next.insert(light.id, (key, first, cube));
            let mut view_proj = [[0.0f32; 4]; 4];
            match spot {
                Some(m) => {
                    view_proj = m;
                    if fresh {
                        jobs.push((first, m));
                    }
                }
                None => {
                    for face in 0..6usize {
                        // A face that cannot be projected stays cleared (nothing shadowed).
                        let m = cube_face_view_proj([light.position.x, light.position.y, light.position.z], face, SHADOW_NEAR, far)
                            .unwrap_or([[0.0; 4]; 4]);
                        if fresh {
                            jobs.push((first + face as u32, m));
                        }
                    }
                }
            }
            if fresh {
                self.built += 1;
            }
            slots.push((i, ShadowSlot { first_layer: first, cube, view_proj, near: SHADOW_NEAR, far, bias: SLOT_BIAS }));
        }
        self.cache = next;
        if !jobs.is_empty() {
            self.render(device, queue, meshes, &jobs);
        }
        slots
    }

    /// Render one depth map per `(layer, matrix)` job.
    fn render(&self, device: &wgpu::Device, queue: &wgpu::Queue, meshes: &GpuMeshes, jobs: &[(u32, [[f32; 4]; 4])]) {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render3d fixture shadow pass layout"),
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
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render3d fixture shadow depth"),
            source: wgpu::ShaderSource::Wgsl(DEPTH_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render3d fixture shadow pipeline layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render3d fixture shadow pipeline"),
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
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render3d fixture shadows") });
        for &(layer, matrix) in jobs {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render3d fixture shadow uniform"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&matrix));
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render3d fixture shadow bind group"),
                layout: &bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
            });
            let target = self.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("render3d fixture shadow layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render3d fixture shadow depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !meshes.draws.is_empty() {
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.set_vertex_buffer(0, meshes.vertex_buffer.slice(..));
                pass.set_index_buffer(meshes.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                for d in meshes.draws.iter().filter(|d| d.casts_shadow) {
                    pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, 0..1);
                }
            }
        }
        queue.submit([encoder.finish()]);
    }
}

/// A hash of everything about a light its shadow map depends on: kind, position, direction and
/// both cone cosines (bit patterns of the f64s), not its colour or intensity.
pub fn light_key(light: &SceneLight) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(&light.kind).hash(&mut h);
    for v in [
        light.position.x,
        light.position.y,
        light.position.z,
        light.direction.x,
        light.direction.y,
        light.direction.z,
        light.cos_inner,
        light.cos_outer,
    ] {
        v.to_bits().hash(&mut h);
    }
    h.finish()
}

/// A hash of the shadow casters of `scene`: every mesh whose material is not a fixture's
/// (`arch3d::light::is_light_material`), its positions and indices. Equal scenes give equal keys;
/// moving a fixture leaves it unchanged.
pub fn occluder_key(scene: &Scene3d) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for m in scene.meshes.iter().filter(|m| !cadcraft_arch3d::light::is_light_material(&m.material)) {
        m.positions.len().hash(&mut h);
        for p in &m.positions {
            for c in p {
                c.to_bits().hash(&mut h);
            }
        }
        m.indices.hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::{Vec2, Vec3};

    fn square(half: f64) -> Vec<Vec2> {
        vec![Vec2::new(-half, -half), Vec2::new(half, -half), Vec2::new(half, half), Vec2::new(-half, half)]
    }

    fn prism(material: &str, id: u32, half: f64, z0: f64, z1: f64) -> crate::types::Mesh {
        let mut m = crate::types::Mesh::new(material, ArchId(id));
        m.push_prism(&square(half), z0, z1);
        m
    }

    fn floor_scene() -> Scene3d {
        let m = prism("floor", 1, 5.0, -0.1, 0.0);
        Scene3d { bounds: m.bounds(), meshes: vec![m], ..Default::default() }
    }

    fn spot(id: u32, x: f64) -> SceneLight {
        SceneLight {
            id: ArchId(id),
            kind: LightKind::Spot,
            position: Vec3::new(x, 0.0, 3.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
            intensity: 900.0,
            color: [1.0; 3],
            cos_inner: 30f64.to_radians().cos(),
            cos_outer: 35f64.to_radians().cos(),
            ..Default::default()
        }
    }

    fn point(id: u32) -> SceneLight {
        SceneLight {
            id: ArchId(id),
            kind: LightKind::Point,
            position: Vec3::new(0.0, 0.0, 2.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
            intensity: 100.0,
            color: [1.0; 3],
            cos_inner: -1.0,
            cos_outer: -1.0,
            ..Default::default()
        }
    }

    #[test]
    fn light_key_ignores_colour_and_intensity() {
        let a = spot(1, 0.0);
        let b = SceneLight { color: [0.5, 0.6, 0.7], intensity: 5.0, ..spot(1, 0.0) };
        assert_eq!(light_key(&a), light_key(&b));
        assert_ne!(light_key(&a), light_key(&spot(1, 0.5)));
        assert_ne!(light_key(&a), light_key(&SceneLight { cos_outer: 0.5, ..spot(1, 0.0) }));
        assert_ne!(light_key(&a), light_key(&SceneLight { kind: LightKind::Point, ..spot(1, 0.0) }));
    }

    #[test]
    fn occluder_key_ignores_fixture_meshes() {
        let base = floor_scene();
        let mut with = floor_scene();
        with.meshes.push(prism("light-trim", 2, 0.1, 2.9, 3.0));
        assert_eq!(occluder_key(&base), occluder_key(&with));
        let mut moved = floor_scene();
        moved.meshes.push(prism("light-trim", 2, 0.1, 1.9, 2.0));
        assert_eq!(occluder_key(&with), occluder_key(&moved));
        let mut wall = floor_scene();
        wall.meshes.push(prism("wall", 3, 0.5, 0.0, 2.0));
        assert_ne!(occluder_key(&base), occluder_key(&wall));
    }

    #[test]
    fn update_builds_each_light_once_then_reuses() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let scene = floor_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        let key = occluder_key(&scene);
        let lights = [spot(1, 0.0), spot(2, 1.0)];
        let mut atlas = ShadowAtlas::new(&device, 64);
        let slots = atlas.update(&device, &queue, &meshes, key, &lights, &[0, 1], 20.0);
        assert_eq!(slots.len(), 2);
        assert_eq!(atlas.maps_built(), 2);
        let again = atlas.update(&device, &queue, &meshes, key, &lights, &[0, 1], 20.0);
        assert_eq!(again, slots);
        assert_eq!(atlas.maps_built(), 2);
        // Changed occluders rebuild everything.
        atlas.update(&device, &queue, &meshes, key ^ 1, &lights, &[0, 1], 20.0);
        assert_eq!(atlas.maps_built(), 4);
    }

    #[test]
    fn moving_one_light_rebuilds_only_its_map() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let scene = floor_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        let key = occluder_key(&scene);
        let mut lights = [spot(1, 0.0), spot(2, 1.0)];
        let mut atlas = ShadowAtlas::new(&device, 64);
        atlas.update(&device, &queue, &meshes, key, &lights, &[0, 1], 20.0);
        lights[1] = spot(2, 2.0);
        atlas.update(&device, &queue, &meshes, key, &lights, &[0, 1], 20.0);
        assert_eq!(atlas.maps_built(), 3);
    }

    #[test]
    fn a_point_light_takes_six_layers() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let scene = floor_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        let lights = [point(1), spot(2, 1.0)];
        let mut atlas = ShadowAtlas::new(&device, 32);
        let slots = atlas.update(&device, &queue, &meshes, occluder_key(&scene), &lights, &[0, 1], 20.0);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots.first().map(|s| (s.1.first_layer, s.1.cube)), Some((0, true)));
        assert_eq!(slots.get(1).map(|s| (s.1.first_layer, s.1.cube)), Some((6, false)));
        assert_eq!(atlas.layers(), 7);
        assert_eq!(atlas.maps_built(), 2);
    }

    #[test]
    fn a_floor_under_a_spot_is_at_its_distance_in_the_map() {
        let Some((device, queue)) = crate::gputest::shared_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = 64u32;
        let scene = floor_scene();
        let meshes = crate::pipeline::upload(&device, &scene);
        let lights = [spot(1, 0.0)];
        let mut atlas = ShadowAtlas::new(&device, size);
        let slots = atlas.update(&device, &queue, &meshes, occluder_key(&scene), &lights, &[0], 20.0);
        let first_layer = slots.first().map(|s| s.1.first_layer).unwrap_or(0);
        assert_eq!(slots.len(), 1);
        let row = size * 4;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atlas readback"),
            size: u64::from(row) * u64::from(size),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: atlas.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: first_layer },
                aspect: wgpu::TextureAspect::DepthOnly,
            },
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
        let depths = slice.get_mapped_range().map(|r| bytemuck::cast_slice::<u8, f32>(&r).to_vec()).unwrap_or_default();
        buffer.unmap();
        let centre = depths.get((size as usize / 2) * size as usize + size as usize / 2).copied().unwrap_or(1.0);
        let d = crate::lights::linear_depth(centre, SHADOW_NEAR, 20.0);
        assert!((d - 3.0).abs() < 0.05, "floor at {d} m, expected 3");
    }
}
