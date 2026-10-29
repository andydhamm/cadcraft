//! The architectural 3D pane: a dockable panel showing the built arch scene.
//!
//! Owns an orbit camera and renders the scene offscreen into its own colour+depth
//! targets (same `egui_wgpu::Callback` pattern as [`gpu::install`]). The scene is
//! rebuilt only when `Drawing::arch_revision` advances since the last frame.
//! Without a GPU adapter the pane shows a static "3D not available" placeholder
//! rather than panicking or showing a blank window.
//!
//! Presentation (assisted building block, 2026-10-25, see `docs/arch3d/DESIGN.md`): [`install`]
//! puts a [`cadcraft_render3d::Offscreen`] on the window's device into the renderer's callback
//! resources. Each frame the pane's paint callback draws the scene into it only when the scene,
//! camera or size changed, then samples its colour target into the pane's rectangle.

use std::sync::Arc;

use egui_wgpu::wgpu;

/// Orbit-camera parameters for the 3D pane.
#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    /// Angle around the vertical axis (radians).
    pub azimuth: f64,
    /// Angle from the horizontal plane (radians); positive = looking down.
    pub elevation: f64,
    /// Distance from the target point.
    pub distance: f64,
}

impl Orbit {
    const fn new_default() -> Self {
        Self { azimuth: 0.0, elevation: 0.35, distance: 10.0 }
    }
}

impl Default for Orbit {
    fn default() -> Self {
        Self::new_default()
    }
}

/// How the pane shades the scene (the header selector).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style3d {
    /// PBR-shaded faces.
    #[default]
    Shaded,
    /// Shaded faces with feature edges.
    ShadedEdges,
    /// Faces in the background colour with visible edges only.
    HiddenLine,
    /// Translucent faces: the whole building shows through.
    GlassHouse,
}

impl Style3d {
    /// Every style, in selector order.
    pub const ALL: [Style3d; 4] = [Style3d::Shaded, Style3d::ShadedEdges, Style3d::HiddenLine, Style3d::GlassHouse];

    /// The next style in selector order, wrapping after the last.
    pub fn next(self) -> Self {
        match self {
            Style3d::Shaded => Style3d::ShadedEdges,
            Style3d::ShadedEdges => Style3d::HiddenLine,
            Style3d::HiddenLine => Style3d::GlassHouse,
            Style3d::GlassHouse => Style3d::Shaded,
        }
    }

    /// The English label (translated at draw time).
    pub fn label(self) -> &'static str {
        match self {
            Style3d::Shaded => "Shaded",
            Style3d::ShadedEdges => "Shaded Edges",
            Style3d::HiddenLine => "Hidden Line",
            Style3d::GlassHouse => "Glass House",
        }
    }

    /// The renderer's style for this one.
    pub fn render_style(self) -> cadcraft_render3d::Style {
        match self {
            Style3d::Shaded => cadcraft_render3d::Style::Shaded,
            Style3d::ShadedEdges => cadcraft_render3d::Style::ShadedEdges,
            Style3d::HiddenLine => cadcraft_render3d::Style::HiddenLine,
            Style3d::GlassHouse => cadcraft_render3d::Style::GlassHouse,
        }
    }
}

/// Walk step in metres; Shift multiplies it by [`RUN_FACTOR`].
pub const WALK_STEP: f64 = 1.0;
/// Shift makes a step 4 m.
pub const RUN_FACTOR: f64 = 4.0;
/// Degrees turned per arrow-key press in the full camera.
pub const TURN_STEP_DEG: f64 = 5.0;

/// The proxy handle (as the 2D pane uses) of the arch object a 3D hit struck.
///
/// Walls and openings number separately, so the mesh's part decides which handle range the id
/// belongs to; `grips::arch_id_from_handle` reads it back.
pub fn proxy_handle(id: cadcraft_doc::ArchId, part: &str) -> cadcraft_doc::Handle {
    use cadcraft_doc::arch as a;
    match part {
        "leaf" | "frame" | "casing" | "glass" | "sill" | "header" => a::opening_handle(id),
        "floor" | "ceiling" | "deck" => a::room_handle(id),
        "roof" => a::roof_handle(id),
        "stair" => a::stair_handle(id),
        "railing" => a::railing_handle(id),
        _ => a::arch_handle(id),
    }
}

/// Per-frame state for the 3D pane.
#[derive(Debug)]
pub struct Pane3d {
    /// The current orbit camera.
    pub orbit: Orbit,
    /// The shading style.
    pub style: Style3d,
    /// The walk-through camera; `Some` replaces the orbit until [`Pane3d::use_orbit`].
    pub full: Option<cadcraft_render3d::camera::FullCamera>,
    /// Orbit target set by a loaded saved camera (`None` = the scene's centre).
    orbit_target: Option<cadcraft_geom::Vec3>,
    /// Orbit field of view set by a loaded saved camera.
    orbit_fov: Option<f64>,
    /// The saved camera last loaded from the drawing.
    loaded_camera: Option<cadcraft_doc::arch::Camera3d>,
    /// The proxy handle of the last 3D click that hit something, until the app takes it.
    picked: Option<cadcraft_doc::Handle>,
    /// The `arch_revision` the scene was last built at (0 = never built).
    pub last_revision: u64,
    /// Number of meshes in the last build (0 when no scene).
    pub mesh_count: usize,
    /// Whether a wgpu adapter is available (set on first frame if a render state is present).
    pub available: bool,
    /// Whether the built scene has visible content (walls, floors, etc.).
    pub has_content: bool,
    /// The last built scene (`None` until the first successful build).
    scene: Option<Arc<cadcraft_render3d::Scene3d>>,
    /// The document the scene was built from ([`Pane3d::sync_doc`]).
    doc: Option<u64>,
    /// Whether the orbit distance has been fitted to a scene yet.
    framed: bool,
}

impl Pane3d {
    /// New pane with a default orbit and no scene built yet.
    pub fn new() -> Self {
        Self {
            orbit: Orbit::default(),
            style: Style3d::default(),
            full: None,
            orbit_target: None,
            orbit_fov: None,
            loaded_camera: None,
            picked: None,
            last_revision: 0,
            mesh_count: 0,
            available: false,
            has_content: false,
            scene: None,
            doc: None,
            framed: false,
        }
    }

    /// Load the drawing's active saved camera when it differs from the one last loaded.
    /// Returns `true` when the pane's camera changed.
    pub fn load_active_camera(&mut self, model: &cadcraft_doc::ArchModel) -> bool {
        use cadcraft_doc::arch::CameraKind;
        use cadcraft_render3d::camera::FullCamera;
        let Some(cam) = model.active_camera.as_deref().and_then(|n| model.camera_by_name(n)) else {
            self.loaded_camera = None;
            return false;
        };
        if self.loaded_camera.as_ref() == Some(cam) {
            return false;
        }
        let fov = cam.fov_deg.filter(|f| f.is_finite() && *f > 0.0 && *f < 180.0);
        let target = cam.target.filter(|t| t.iter().all(|v| v.is_finite())).map(|t| cadcraft_geom::Vec3::new(t[0], t[1], t[2]));
        match cam.kind {
            CameraKind::Full => {
                let eye = cam.eye.filter(|e| e.iter().all(|v| v.is_finite())).unwrap_or([0.0, 0.0]);
                let eye = cadcraft_geom::Vec3::new(eye[0], eye[1], cam.eye_height.filter(|h| h.is_finite()).unwrap_or(1.6));
                let mut target = target.unwrap_or(cadcraft_geom::Vec3::new(0.0, 0.0, 0.0));
                if (target.x - eye.x).hypot(target.y - eye.y) < 1e-6 {
                    // Looking at its own plan position: face north instead.
                    target = cadcraft_geom::Vec3::new(eye.x, eye.y + 1.0, eye.z);
                }
                self.full = Some(FullCamera::new(eye, target, fov.unwrap_or(45.0)));
            }
            CameraKind::Orbit => {
                self.full = None;
                self.orbit_target = target;
                self.orbit_fov = fov;
                if let Some(a) = cam.azimuth_deg.filter(|v| v.is_finite()) {
                    self.orbit.azimuth = a.to_radians();
                }
                if let Some(e) = cam.elevation_deg.filter(|v| v.is_finite()) {
                    self.orbit.elevation = e.to_radians().clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
                }
                if let Some(d) = cam.distance.filter(|v| v.is_finite() && *v > 0.0) {
                    self.orbit.distance = d;
                    // A saved distance is the user's framing; do not refit over it.
                    self.framed = true;
                }
            }
        }
        self.loaded_camera = Some(cam.clone());
        true
    }

    /// Back to the orbit camera from the walk-through camera.
    pub fn use_orbit(&mut self) {
        self.full = None;
    }

    /// The full camera, creating it from the current view when the pane is still orbiting.
    fn ensure_full(&mut self) -> cadcraft_render3d::camera::FullCamera {
        use cadcraft_render3d::camera::FullCamera;
        match self.full {
            Some(f) => f,
            None => {
                let cam = self.camera();
                let f = FullCamera::new(cam.eye(), cam.target(), cam.fov_deg());
                self.full = Some(f);
                f
            }
        }
    }

    /// Walk the full camera `forward`/`strafe` steps of `step` metres in plan, the maths of the
    /// `walkthrough` command: forward along `(sin yaw, cos yaw)`, strafe along `(cos yaw, -sin yaw)`,
    /// yaw 0 = +Y turning toward +X. Eye and target move together, so the heading holds.
    pub fn walk(&mut self, forward: f64, strafe: f64, step: f64) {
        let mut f = self.ensure_full();
        let yaw = (f.target.x - f.eye.x).atan2(f.target.y - f.eye.y);
        let (sy, cy) = yaw.sin_cos();
        let (fwd, right) = (forward * step, strafe * step);
        let (dx, dy) = (fwd * sy + right * cy, fwd * cy - right * sy);
        if !(dx.is_finite() && dy.is_finite()) {
            return;
        }
        f.eye.x += dx;
        f.eye.y += dy;
        f.target.x += dx;
        f.target.y += dy;
        self.full = Some(f);
    }

    /// Turn the full camera `deg` degrees about its eye (positive = toward +X).
    pub fn turn(&mut self, deg: f64) {
        let mut f = self.ensure_full();
        let (dx, dy) = (f.target.x - f.eye.x, f.target.y - f.eye.y);
        let len = dx.hypot(dy);
        if !deg.is_finite() || len < 1e-9 {
            return;
        }
        let yaw = dx.atan2(dy) + deg.to_radians();
        f.target.x = f.eye.x + len * yaw.sin();
        f.target.y = f.eye.y + len * yaw.cos();
        self.full = Some(f);
    }

    /// The proxy handle under `pixel` (y-down, device pixels of a `size` view), if the ray hits.
    pub fn pick_handle_at(&self, size: (u32, u32), pixel: [f32; 2]) -> Option<cadcraft_doc::Handle> {
        let scene = self.scene.as_ref()?;
        let (origin, dir) = cadcraft_render3d::camera::ray_at(&self.camera(), size, pixel)?;
        let hit = scene.pick(origin, dir)?;
        Some(proxy_handle(hit.object, hit.part))
    }

    /// Click at `pixel`: remember the struck object's handle for the app to select. A miss
    /// leaves the earlier pick (and the selection) alone.
    pub fn pick(&mut self, size: (u32, u32), pixel: [f32; 2]) -> Option<cadcraft_doc::Handle> {
        let h = self.pick_handle_at(size, pixel)?;
        self.picked = Some(h);
        Some(h)
    }

    /// Take the handle of the last 3D pick, for the app to add to the selection the 2D pane uses.
    pub fn take_picked(&mut self) -> Option<cadcraft_doc::Handle> {
        self.picked.take()
    }

    /// Whether the scene must be rebuilt because `current_revision` differs from the last
    /// `arch_revision` we rendered against.
    pub fn needs_rebuild(&self, current_revision: u64) -> bool {
        current_revision != self.last_revision
    }

    /// Mark the scene as rebuilt. Call after `arch3d::build` / `render3d` completes
    /// with a successful result.
    pub fn mark_built(&mut self, revision: u64, mesh_count: usize, has_content: bool) {
        self.last_revision = revision;
        self.mesh_count = mesh_count;
        self.has_content = has_content;
    }

    /// Rebuild the scene for `revision` from `model` (`None` = no document).
    ///
    /// Returns `true` when the scene was rebuilt. Never rebuilds when
    /// `revision == self.last_revision` and a scene already exists.
    pub fn sync(&mut self, revision: u64, model: Option<&cadcraft_doc::ArchModel>) -> bool {
        match model {
            Some(m) => {
                if self.last_revision == revision {
                    return false;
                }
                self.load_active_camera(m);
                let scene = cadcraft_arch3d::build(m);
                let mesh_count = scene.meshes.len();
                let has_content = !scene.meshes.is_empty();
                if has_content && !self.framed {
                    // The first scene sets a distance that shows all of it; later zooms are the user's.
                    self.orbit.distance = cadcraft_render3d::camera::Orbit::fit(&scene.bounds).distance;
                    self.framed = true;
                }
                self.scene = Some(Arc::new(scene));
                self.mark_built(revision, mesh_count, has_content);
                true
            }
            None => {
                self.scene = None;
                self.mesh_count = 0;
                self.has_content = false;
                self.last_revision = revision;
                true
            }
        }
    }

    /// [`Pane3d::sync`] for the document `doc` (the session's `DocState::uid`): switching to
    /// another document rebuilds even when its revision number happens to match.
    pub fn sync_doc(&mut self, doc: u64, revision: u64, model: Option<&cadcraft_doc::ArchModel>) -> bool {
        if self.doc != Some(doc) {
            self.doc = Some(doc);
            self.last_revision = u64::MAX;
            self.loaded_camera = None;
            self.full = None;
            self.orbit_target = None;
            self.orbit_fov = None;
        }
        self.sync(revision, model)
    }

    /// The scene currently held (`None` when nothing has been built).
    pub fn scene(&self) -> Option<&cadcraft_arch3d::Scene3d> {
        self.scene.as_deref()
    }

    /// Identifies the held scene's content for the renderer's mesh cache.
    fn scene_key(&self) -> u64 {
        self.doc.unwrap_or(0).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ self.last_revision
    }

    /// The orbit `Camera` for the currently held scene.
    ///
    /// Frames the held scene (or an empty bounds when there is none) via
    /// [`cadcraft_render3d::camera::Orbit::fit`], then applies the pane's user
    /// orbit: `azimuth`/`elevation` in degrees and `distance` clamped to at
    /// least [`cadcraft_render3d::camera::MIN_DISTANCE`]. `fit`'s `target` and
    /// `fov_deg` are preserved.
    pub fn camera(&self) -> cadcraft_render3d::Camera {
        let mut orbit = match self.scene {
            Some(ref s) => cadcraft_render3d::camera::Orbit::fit(&s.bounds),
            None => cadcraft_render3d::camera::Orbit::fit(&cadcraft_arch3d::Bounds3::empty()),
        };
        orbit.azimuth = self.orbit.azimuth.to_degrees();
        orbit.elevation = self.orbit.elevation.to_degrees();
        orbit.distance = self.orbit.distance.max(cadcraft_render3d::camera::MIN_DISTANCE);
        if let Some(f) = self.full {
            return cadcraft_render3d::Camera::Full(f);
        }
        if let Some(t) = self.orbit_target {
            orbit.target = t;
        }
        if let Some(f) = self.orbit_fov {
            orbit.fov_deg = f;
        }
        cadcraft_render3d::Camera::Orbit(orbit)
    }
}

/// Draw the 3D pane content into `ui`.
///
/// When no GPU adapter is available or no scene has been built, a text placeholder is
/// shown. When a scene exists it is rendered through the paint callback set up by [`install`];
/// dragging orbits and the wheel zooms.
pub fn draw(ui: &mut egui::Ui, pane: &mut Pane3d) {
    let scene = pane.scene.clone().filter(|_| pane.available && pane.has_content);
    let Some(scene) = scene else {
        egui::Frame::default().show(ui, |ui| {
            ui.add_space(6.0);
            if !pane.available {
                ui.label("3D not available");
                ui.label("No GPU adapter detected.");
            } else {
                ui.label("No walls yet.");
            }
        });
        return;
    };
    ui.horizontal(|ui| {
        for style in Style3d::ALL {
            ui.selectable_value(&mut pane.style, style, crate::i18n::t(style.label()));
        }
        if pane.full.is_some() && ui.button(crate::i18n::t("Orbit")).clicked() {
            pane.use_orbit();
        }
    });
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    let ppp = ui.ctx().pixels_per_point();
    let size = ((rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32);
    if response.dragged() {
        let d = response.drag_delta();
        if pane.full.is_some() {
            pane.turn(f64::from(d.x) * 0.3);
        } else {
            orbit_by_drag(pane, -f64::from(d.x), f64::from(d.y), 0.01);
        }
    }
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        let px = [(p.x - rect.min.x) * ppp, (p.y - rect.min.y) * ppp];
        pane.pick(size, px);
    }
    if response.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            zoom_by_wheel(pane, f64::from(wheel) / 50.0);
        }
    }
    if response.has_focus() {
        walk_keys(ui, pane);
    }
    let callback = Pane3dCallback { scene, key: pane.scene_key(), camera: pane.camera(), size, style: pane.style };
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, callback));
}

/// Apply this frame's WASD / arrow key presses to the full camera: W/S and Up/Down walk,
/// A/D strafe, Left/Right turn; Shift runs.
fn walk_keys(ui: &egui::Ui, pane: &mut Pane3d) {
    use egui::Key;
    let (mut fwd, mut strafe, mut turn, mut run) = (0.0, 0.0, 0.0, false);
    ui.input(|i| {
        run = i.modifiers.shift;
        let n = |k: Key| if i.key_pressed(k) { 1.0 } else { 0.0 };
        fwd = n(Key::W) + n(Key::ArrowUp) - n(Key::S) - n(Key::ArrowDown);
        strafe = n(Key::D) - n(Key::A);
        turn = n(Key::ArrowRight) - n(Key::ArrowLeft);
    });
    let step = if run { WALK_STEP * RUN_FACTOR } else { WALK_STEP };
    if fwd != 0.0 || strafe != 0.0 {
        pane.walk(fwd, strafe, step);
    }
    if turn != 0.0 {
        pane.turn(turn * TURN_STEP_DEG);
    }
}

/// Set the pane up to render on the window's device. Call once with the app's render state;
/// `stencil` is the format of the window pass's stencil buffer, if it has one.
pub fn install(pane: &mut Pane3d, rs: &egui_wgpu::RenderState, stencil: Option<wgpu::TextureFormat>) {
    let res = Resources::new(&rs.device, rs.target_format, stencil.filter(|s| s.has_stencil_aspect()));
    rs.renderer.write().callback_resources.insert(res);
    pane.available = true;
}

/// Samples the offscreen colour target into the pane's rectangle.
const BLIT: &str = r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: V;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}
@fragment fn fs_linear(v: V) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(image, image_sampler, v.uv).rgb, 1.0);
}
fn encode(c: f32) -> f32 {
    if (c <= 0.0031308) { return 12.92 * c; }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}
@fragment fn fs_encode(v: V) -> @location(0) vec4<f32> {
    let c = textureSample(image, image_sampler, v.uv).rgb;
    return vec4<f32>(encode(c.r), encode(c.g), encode(c.b), 1.0);
}
"#;

/// The pane's GPU state, kept in the renderer's callback resources.
struct Resources {
    blit: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    offscreen: Option<cadcraft_render3d::Offscreen>,
    /// The bind group for the offscreen target of this generation.
    bound: Option<(u64, wgpu::BindGroup)>,
    /// What the offscreen target last drew: scene key, size and view-projection.
    drawn: Option<(u64, (u32, u32), [[f32; 4]; 4], Style3d)>,
    /// How many times the scene was drawn into the offscreen target.
    draws: u64,
}

impl Resources {
    fn new(device: &wgpu::Device, target: wgpu::TextureFormat, stencil: Option<wgpu::TextureFormat>) -> Self {
        let module =
            device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("pane3d blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pane3d blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pane3d blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // The window pass may carry a stencil buffer (clipped viewports); a pipeline drawn in it
        // must name the same format even though it never tests it.
        let depth_stencil = stencil.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: None,
            depth_compare: None,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        // The offscreen target is sRGB, so sampling returns linear colour: an sRGB window takes it
        // as is, any other window gets it encoded (as the canvas does with `linear_out`).
        let fs = if target.is_srgb() { "fs_linear" } else { "fs_encode" };
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pane3d blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fs),
                targets: &[Some(wgpu::ColorTargetState { format: target, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pane3d blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Resources { blit, layout, sampler, offscreen: None, bound: None, drawn: None, draws: 0 }
    }
}

/// One frame's request: draw `scene` from `camera` at `size` device pixels.
struct Pane3dCallback {
    scene: Arc<cadcraft_render3d::Scene3d>,
    key: u64,
    camera: cadcraft_render3d::Camera,
    size: (u32, u32),
    style: Style3d,
}

impl egui_wgpu::CallbackTrait for Pane3dCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        let off = res.offscreen.get_or_insert_with(|| cadcraft_render3d::Offscreen::with_device(device.clone(), queue.clone(), self.size));
        off.resize(self.size);
        let (w, h) = off.size();
        let vp = cadcraft_render3d::camera::view_proj(&self.camera, f64::from(w) / f64::from(h));
        // Redraw only when something visible changed; an idle pane costs one textured triangle.
        let state = (self.key, (w, h), vp, self.style);
        if res.drawn != Some(state) {
            let opts = cadcraft_render3d::RenderOptions { style: self.style.render_style(), ..Default::default() };
            off.encode_with(encoder, &self.scene, self.key, &self.camera, &cadcraft_render3d::Light::DEFAULT, &opts);
            res.drawn = Some(state);
            res.draws += 1;
        }
        if res.bound.as_ref().is_none_or(|(g, _)| *g != off.generation()) {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pane3d blit"),
                layout: &res.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(off.color_view()) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&res.sampler) },
                ],
            });
            res.bound = Some((off.generation(), group));
        }
        Vec::new()
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        let Some((_, group)) = &res.bound else { return };
        let v = info.viewport_in_pixels();
        let [sw, sh] = info.screen_size_px;
        let (x0, y0) = (v.left_px.max(0) as f32, v.top_px.max(0) as f32);
        let (x1, y1) = (((v.left_px + v.width_px) as f32).min(sw as f32), ((v.top_px + v.height_px) as f32).min(sh as f32));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        pass.set_viewport(x0, y0, x1 - x0, y1 - y0, 0.0, 1.0);
        pass.set_pipeline(&res.blit);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Apply a 2D drag delta (UI pixels) to the orbit camera.
pub fn orbit_by_drag(pane: &mut Pane3d, dx: f64, dy: f64, sensitivity: f64) {
    pane.orbit.azimuth += dx * sensitivity;
    pane.orbit.elevation = (pane.orbit.elevation + dy * sensitivity).clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
}

/// Apply a wheel-zoom delta to the orbit camera.
/// Positive `delta` zooms in (decreases distance); negative zooms out.
pub fn zoom_by_wheel(pane: &mut Pane3d, delta: f64) {
    pane.orbit.distance = (pane.orbit.distance * (1.0 - 0.1 * delta)).clamp(0.5, 1000.0);
}

/// Apply the last 3D pick as the drawing's selection, so a click in the 3D pane selects the
/// same proxy handle as a click in the 2D pane (phase-6 carry-over). Takes the pending handle
/// and replaces the selection with it, returning it so the caller can repaint. A miss (or no
/// pending pick) leaves the selection alone.
pub fn apply_pick(pane: &mut Pane3d, session: &mut cadcraft_engine::Session) -> Option<cadcraft_doc::Handle> {
    let h = pane.take_picked()?;
    session.set_selection(vec![h]);
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    use cadcraft_engine::Session;
    use serde_json::json;

    /// The phase-1 house (four exterior walls) with the phase-2 front door on `w0` at offset
    /// 3.0 — the same two commands as `tests/acceptance/phase-7/p7lib.inc::HOUSE`.
    fn house_session() -> Session {
        let mut session = Session::new();
        let _walls = session
            .execute(
                "wall",
                &json!({
                    "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]],
                    "thickness": 0.2,
                    "height": 2.4,
                    "exterior": true
                }),
            )
            .expect("the phase-1 house walls");
        let _door = session
            .execute(
                "door",
                &json!({
                    "wall": "w0",
                    "offset": 3.0,
                    "width": 0.9,
                    "height": 2.1,
                    "style": "hinged"
                }),
            )
            .expect("the front door on w0");
        session
    }

    /// The selection's handles as hex strings, for assertions independent of `Handle`'s derives.
    fn sel_handles(s: &Session) -> Vec<String> {
        s.selection().iter().map(|h| h.hex()).collect()
    }

    /// The phase-6 front camera: eye [3,-8,1.6] -> target [3,0,1.05], fov 60, view 480x320.
    /// The 0.9 m door at offset 3.0 on the south wall is dead-centre; [8,8] is sky.
    fn front_camera() -> (cadcraft_render3d::camera::FullCamera, (u32, u32)) {
        let size = (480u32, 320u32);
        (cadcraft_render3d::camera::FullCamera::new(cadcraft_geom::Vec3::new(3.0, -8.0, 1.6), cadcraft_geom::Vec3::new(3.0, 0.0, 1.05), 60.0), size)
    }

    /// A `Pane3d` with the phase-1 house scene synced and the front camera installed.
    fn house_pane() -> (Pane3d, Session) {
        let mut session = house_session();
        let mut pane = Pane3d::new();
        {
            let st = session.state().expect("session state");
            assert!(pane.sync_doc(st.uid, st.doc.arch_revision, Some(&st.doc.arch)), "the phase-1 house scene must build");
        }
        let (cam, _) = front_camera();
        pane.full = Some(cam);
        (pane, session)
    }

    #[test]
    fn pane3d_builds_without_an_adapter() {
        let mut pane = Pane3d::new();
        assert!(!pane.available);
        assert!(!pane.has_content);
        assert_eq!(pane.last_revision, 0);
        assert_eq!(pane.mesh_count, 0);

        let ctx = egui::Context::default();
        let raw = egui::RawInput::default();
        let mut out = ctx.run_ui(raw, |ui| draw(ui, &mut pane));
        out.textures_delta.clear();

        assert!(!pane.available);
        assert!(!pane.has_content);
    }

    #[test]
    fn pane3d_rebuilds_only_when_the_revision_changes() {
        let mut pane = Pane3d::new();

        assert!(!pane.needs_rebuild(0));

        pane.mark_built(1, 5, true);
        assert_eq!(pane.last_revision, 1);
        assert_eq!(pane.mesh_count, 5);
        assert!(pane.has_content);
        assert!(!pane.needs_rebuild(1));
        assert!(!pane.needs_rebuild(1));

        assert!(pane.needs_rebuild(2));

        pane.mark_built(2, 7, true);
        assert_eq!(pane.last_revision, 2);
        assert_eq!(pane.mesh_count, 7);
        assert!(!pane.needs_rebuild(2));
        assert!(!pane.needs_rebuild(2));

        assert!(pane.needs_rebuild(3));
    }

    #[test]
    fn pane3d_renders_after_a_revision_bump() {
        let mut arch = cadcraft_doc::ArchModel::default();
        arch.push_wall(cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(0.0, 0.0),
            end: cadcraft_geom::Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            ..Default::default()
        });

        let mut pane = Pane3d::new();
        assert!(pane.scene().is_none());
        assert!(!pane.has_content);

        // First successful build at revision 1.
        assert!(pane.sync(1, Some(&arch)));
        assert!(pane.scene().is_some());
        assert!(pane.has_content);
        assert_eq!(pane.last_revision, 1);

        // "No document" clears the scene.
        assert!(pane.sync(2, None));
        assert!(pane.scene().is_none());
        assert!(!pane.has_content);
        assert_eq!(pane.last_revision, 2);

        // A revision bump rebuilds.
        assert!(pane.sync(3, Some(&arch)));
        assert!(pane.scene().is_some());
        assert!(pane.has_content);
        assert_eq!(pane.last_revision, 3);
    }

    #[test]
    fn pane3d_sync_skips_an_unchanged_revision() {
        let mut arch = cadcraft_doc::ArchModel::default();
        arch.push_wall(cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(0.0, 0.0),
            end: cadcraft_geom::Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            ..Default::default()
        });

        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        let first_mesh_count = pane.mesh_count;

        // Same revision, same model: no rebuild.
        assert!(!pane.sync(1, Some(&arch)));
        assert!(pane.scene().is_some());
        assert_eq!(pane.mesh_count, first_mesh_count);
    }

    #[test]
    fn pane3d_camera_frames_the_scene_bounds() {
        let mut arch = cadcraft_doc::ArchModel::default();
        arch.push_wall(cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(0.0, 0.0),
            end: cadcraft_geom::Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            ..Default::default()
        });

        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        assert!(pane.scene().is_some());

        let cam = pane.camera();
        let o = match cam {
            cadcraft_render3d::Camera::Orbit(o) => o,
            _ => cadcraft_render3d::camera::Orbit::at_origin(0.0, 0.0, 1.0, 45.0),
        };

        // `fit` centres the camera on the scene; for a 0..10 m wall that is x = 5.
        assert!((o.target.x - 5.0).abs() < 0.25);
        assert!(o.distance >= cadcraft_render3d::camera::MIN_DISTANCE);
        // The first scene with content sets the distance that frames it.
        let fit = cadcraft_render3d::camera::Orbit::fit(&pane.scene().expect("scene").bounds).distance;
        assert!((o.distance - fit).abs() < 1e-9);
        // The render3d Orbit stores angles in degrees; the pane default is 0.0 / 0.35 rad.
        assert!((o.azimuth - 0.0).abs() < 1e-9);
        assert!((o.elevation - 0.35 * (180.0 / std::f64::consts::PI)).abs() < 1e-6);
    }

    fn house() -> cadcraft_doc::ArchModel {
        let mut arch = cadcraft_doc::ArchModel::default();
        let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0), (0.0, 0.0)];
        for w in pts.windows(2) {
            arch.push_wall(cadcraft_doc::Wall {
                start: cadcraft_geom::Vec2::new(w[0].0, w[0].1),
                end: cadcraft_geom::Vec2::new(w[1].0, w[1].1),
                thickness: 0.2,
                height: 2.7,
                ..Default::default()
            });
        }
        arch
    }

    #[test]
    fn switching_documents_rebuilds_even_at_the_same_revision() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync_doc(1, 5, Some(&arch)));
        assert!(!pane.sync_doc(1, 5, Some(&arch)));
        assert!(pane.sync_doc(2, 5, Some(&arch)));
        assert!(pane.sync_doc(1, 5, Some(&arch)));
    }

    #[test]
    fn only_the_first_scene_sets_the_orbit_distance() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        let fitted = pane.orbit.distance;
        zoom_by_wheel(&mut pane, 2.0);
        let zoomed = pane.orbit.distance;
        assert!(zoomed < fitted);
        assert!(pane.sync(2, Some(&arch)));
        assert!((pane.orbit.distance - zoomed).abs() < 1e-12, "a rebuild undid the user's zoom");
    }

    /// A device for GPU tests, or `None` (the test skips) without an adapter or when the driver
    /// does not answer within 30 s.
    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        cadcraft_render3d::offscreen::device_within(std::time::Duration::from_secs(30))
    }

    fn read_target(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, (w, h): (u32, u32)) -> Vec<u8> {
        let padded = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(padded * h),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let index = queue.submit([enc.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::Wait { submission_index: Some(index), timeout: Some(std::time::Duration::from_secs(20)) }).expect("readback");
        let mapped = buffer.slice(..).get_mapped_range().expect("mapped range");
        (0..h as usize).flat_map(|r| mapped[r * padded as usize..r * padded as usize + w as usize * 4].to_vec()).collect()
    }

    #[test]
    fn pane3d_house_scene_has_multiple_mesh_types() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        let scene = pane.scene().expect("scene");
        assert!(!scene.meshes.is_empty(), "house scene should have meshes");

        let mut materials = std::collections::HashSet::new();
        for mesh in &scene.meshes {
            materials.insert(mesh.material.clone());
        }
        assert!(materials.contains("wall"), "house scene should have wall meshes");
    }

    #[test]
    fn pane3d_house_scene_bounds_include_full_height() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        let scene = pane.scene().expect("scene");
        assert!(scene.bounds.max[2] >= 2.0, "scene bounds should include wall height");
        assert!(scene.bounds.min[2] <= 0.1, "scene bounds should include ground level");
    }

    #[test]
    fn pane3d_camera_frames_house_scene() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));

        let cam = pane.camera();
        let o = match cam {
            cadcraft_render3d::Camera::Orbit(o) => o,
            _ => cadcraft_render3d::camera::Orbit::at_origin(0.0, 0.0, 1.0, 45.0),
        };

        assert!(o.distance >= cadcraft_render3d::camera::MIN_DISTANCE);
        // House is 10x8m; camera should target near its center (5, 4)
        assert!((o.target.x - 5.0).abs() < 1.0, "camera should target near house center x");
        assert!((o.target.y - 4.0).abs() < 1.0, "camera should target near house center y");
    }

    #[test]
    fn pane3d_orbit_and_zoom_with_house_scene() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));

        let initial_azimuth = pane.orbit.azimuth;
        let initial_distance = pane.orbit.distance;

        orbit_by_drag(&mut pane, 100.0, 0.0, 0.01);
        assert_ne!(pane.orbit.azimuth, initial_azimuth, "orbit should change azimuth");

        zoom_by_wheel(&mut pane, 1.0);
        assert_ne!(pane.orbit.distance, initial_distance, "zoom should change distance");
        assert!(pane.orbit.distance < initial_distance, "positive zoom delta should zoom in");
    }

    #[test]
    fn pane3d_empty_model_has_no_content() {
        let arch = cadcraft_doc::ArchModel::default();
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        assert!(!pane.has_content, "empty model should have no content");
        assert!(pane.scene().expect("scene").meshes.is_empty(), "empty model should have no meshes");
    }

    #[test]
    fn pane3d_single_wall_scene_has_content() {
        let mut arch = cadcraft_doc::ArchModel::default();
        arch.push_wall(cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(0.0, 0.0),
            end: cadcraft_geom::Vec2::new(10.0, 0.0),
            thickness: 0.2,
            height: 2.7,
            ..Default::default()
        });
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        assert!(pane.has_content, "single wall should have content");
        assert!(!pane.scene().expect("scene").meshes.is_empty(), "single wall should have meshes");
    }

    #[test]
    fn pane3d_camera_with_no_scene_uses_empty_bounds() {
        let mut pane = Pane3d::new();
        // No scene built yet; camera should still be valid
        let cam = pane.camera();
        let o = match cam {
            cadcraft_render3d::Camera::Orbit(o) => o,
            _ => cadcraft_render3d::camera::Orbit::at_origin(0.0, 0.0, 1.0, 45.0),
        };
        assert!(o.distance >= cadcraft_render3d::camera::MIN_DISTANCE);
        assert!(o.target.is_finite(), "camera target must be finite with no scene");
    }

    #[test]
    fn pane3d_orbit_elevation_clamps_to_vertical() {
        let mut pane = Pane3d::new();
        // Drag up a lot (positive dy increases elevation)
        orbit_by_drag(&mut pane, 0.0, 1000.0, 0.01);
        assert!(pane.orbit.elevation <= std::f64::consts::FRAC_PI_2 + 1e-9, "elevation should clamp at +90 degrees");
        // Drag down a lot
        orbit_by_drag(&mut pane, 0.0, -1000.0, 0.01);
        assert!(pane.orbit.elevation >= -std::f64::consts::FRAC_PI_2 - 1e-9, "elevation should clamp at -90 degrees");
    }

    #[test]
    fn pane3d_zoom_clamps_distance() {
        let mut pane = Pane3d::new();
        // Zoom in many times
        for _ in 0..100 {
            zoom_by_wheel(&mut pane, 1.0);
        }
        assert!(pane.orbit.distance >= 0.5 - 1e-9, "distance should clamp at 0.5");
        // Zoom out many times
        for _ in 0..100 {
            zoom_by_wheel(&mut pane, -1.0);
        }
        assert!(pane.orbit.distance <= 1000.0 + 1e-9, "distance should clamp at 1000");
    }

    #[test]
    fn pane3d_zero_length_wall_builds_scene() {
        let mut arch = cadcraft_doc::ArchModel::default();
        arch.push_wall(cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(5.0, 5.0),
            end: cadcraft_geom::Vec2::new(5.0, 5.0),
            thickness: 0.2,
            height: 2.7,
            ..Default::default()
        });
        let mut pane = Pane3d::new();
        // A zero-length wall should build without panicking
        assert!(pane.sync(1, Some(&arch)));
        assert_eq!(pane.last_revision, 1);
    }

    #[test]
    fn pane3d_wall_with_nan_coordinates_does_not_panic() {
        let mut arch = cadcraft_doc::ArchModel::default();
        arch.push_wall(cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(f64::NAN, 0.0),
            end: cadcraft_geom::Vec2::new(10.0, f64::NAN),
            thickness: 0.2,
            height: 2.7,
            ..Default::default()
        });
        let mut pane = Pane3d::new();
        // Should not panic on NaN coordinates
        pane.sync(1, Some(&arch));
        assert_eq!(pane.last_revision, 1);
    }

    #[test]
    fn pane3d_sync_doc_clears_with_none_model() {
        let arch = house();
        let mut pane = Pane3d::new();
        assert!(pane.sync_doc(1, 1, Some(&arch)));
        assert!(pane.has_content);
        // Clear with None model
        assert!(pane.sync_doc(1, 2, None));
        assert!(!pane.has_content, "scene should be cleared");
        assert!(pane.scene().is_none(), "scene should be None");
        assert_eq!(pane.last_revision, 2);
    }

    #[test]
    fn the_paint_callback_draws_the_scene_into_the_window_and_redraws_only_on_change() {
        let Some((device, queue)) = test_device() else { return };
        let (w, h) = (64u32, 48u32);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(Resources::new(&device, format, None));
        let mut pane = Pane3d::new();
        pane.available = true;
        assert!(pane.sync(1, Some(&house())));
        let callback = |pane: &Pane3d| Pane3dCallback {
            scene: pane.scene.clone().expect("scene"),
            key: pane.scene_key(),
            camera: pane.camera(),
            size: (w, h),
            style: pane.style,
        };
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point: 1.0 };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let frame = |cb: &Pane3dCallback, resources: &mut egui_wgpu::CallbackResources| {
            let mut enc = device.create_command_encoder(&Default::default());
            let extra = egui_wgpu::CallbackTrait::prepare(cb, &device, &queue, &screen, &mut enc, resources);
            {
                let mut pass = enc
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    })
                    .forget_lifetime();
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32));
                let info = egui::PaintCallbackInfo { viewport: rect, clip_rect: rect, pixels_per_point: 1.0, screen_size_px: [w, h] };
                egui_wgpu::CallbackTrait::paint(cb, info, &mut pass, resources);
            }
            queue.submit(extra.into_iter().chain([enc.finish()]));
        };
        frame(&callback(&pane), &mut resources);
        let px = read_target(&device, &queue, &target, (w, h));
        assert!(px.chunks_exact(4).all(|p| p != [255, 255, 255, 255]), "the blit left part of the pane unpainted");
        let first = &px[..4];
        assert!(px.chunks_exact(4).any(|p| p != first), "the pane shows only the background: the scene was not drawn");
        let draws = |r: &egui_wgpu::CallbackResources| r.get::<Resources>().expect("resources").draws;
        assert_eq!(draws(&resources), 1);
        // An unchanged camera and scene: the next frames only blit.
        frame(&callback(&pane), &mut resources);
        frame(&callback(&pane), &mut resources);
        assert_eq!(draws(&resources), 1);
        // Orbiting draws again.
        orbit_by_drag(&mut pane, 30.0, 0.0, 0.01);
        frame(&callback(&pane), &mut resources);
        assert_eq!(draws(&resources), 2);
    }

    #[test]
    fn pane3d_style_selector_cycles_all_four() {
        let mut s = Style3d::default();
        assert_eq!(s, Style3d::Shaded);
        let mut seen = vec![s];
        for _ in 0..3 {
            s = s.next();
            assert!(!seen.contains(&s));
            seen.push(s);
        }
        assert_eq!(seen, Style3d::ALL.to_vec());
        assert_eq!(s.next(), Style3d::Shaded);
        let styles: std::collections::HashSet<_> = Style3d::ALL.iter().map(|s| format!("{:?}", s.render_style())).collect();
        assert_eq!(styles.len(), 4);
    }

    fn full_of(pane: &Pane3d) -> cadcraft_render3d::camera::FullCamera {
        match pane.camera() {
            cadcraft_render3d::Camera::Full(f) => f,
            other => panic!("expected a full camera, got {other:?}"),
        }
    }

    #[test]
    fn pane3d_walkthrough_moves_the_eye_in_plan() {
        let mut pane = Pane3d::new();
        let eye = cadcraft_geom::Vec3::new(1.0, 2.0, 1.6);
        pane.full = Some(cadcraft_render3d::camera::FullCamera::new(eye, cadcraft_geom::Vec3::new(1.0, 12.0, 1.6), 60.0));
        // Yaw 0 faces +Y: forward 2 steps adds (0, 2); strafe 3 adds (3, 0).
        pane.walk(2.0, 0.0, WALK_STEP);
        let f = full_of(&pane);
        assert!((f.eye.x - 1.0).abs() < 1e-9 && (f.eye.y - 4.0).abs() < 1e-9 && (f.eye.z - 1.6).abs() < 1e-9);
        pane.walk(0.0, 3.0, WALK_STEP);
        let f = full_of(&pane);
        assert!((f.eye.x - 4.0).abs() < 1e-9 && (f.eye.y - 4.0).abs() < 1e-9);
        // Shift runs 4 m; turning to yaw 90 (facing +X) then walking forward adds (4, 0).
        pane.turn(90.0);
        pane.walk(1.0, 0.0, WALK_STEP * RUN_FACTOR);
        let f = full_of(&pane);
        assert!((f.eye.x - 8.0).abs() < 1e-9 && (f.eye.y - 4.0).abs() < 1e-9, "{:?}", f.eye);
        // From the orbit camera the walk starts at its eye and the pane switches to the full camera.
        let mut orbiting = Pane3d::new();
        assert!(orbiting.full.is_none());
        orbiting.walk(1.0, 0.0, WALK_STEP);
        assert!(orbiting.full.is_some());
        orbiting.use_orbit();
        assert!(matches!(orbiting.camera(), cadcraft_render3d::Camera::Orbit(_)));
    }

    #[test]
    fn pane3d_pick_selects_the_same_proxy_handle_as_2d() {
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&house())));
        let size = (400u32, 300u32);
        let cam = pane.camera();
        // The open-topped house may let the centre ray through: take the first pixel that strikes it.
        let (px, hit) = (0..30)
            .flat_map(|y| (0..40).map(move |x| [x as f32 * 10.0 + 5.0, y as f32 * 10.0 + 5.0]))
            .find_map(|px| {
                let (o, d) = cadcraft_render3d::camera::ray_at(&cam, size, px)?;
                Some((px, pane.scene()?.pick(o, d)?))
            })
            .expect("some pixel of the view hits the house");
        let handle = pane.pick(size, px).expect("pick");
        assert_eq!(handle, proxy_handle(hit.object, hit.part));
        assert_eq!(handle, cadcraft_doc::arch::arch_handle(hit.object), "a wall hit is the wall's proxy handle");
        assert_eq!(cadcraft_engine::grips::arch_id_from_handle(handle), Some(cadcraft_engine::grips::ArchTarget::Wall(hit.object)));
        assert_eq!(pane.take_picked(), Some(handle));
        // A miss reports nothing and clears nothing.
        pane.pick(size, px);
        assert_eq!(pane.pick(size, [f32::NAN, 0.0]), None);
        assert_eq!(pane.take_picked(), Some(handle));
        // Other object kinds route to their own handle ranges.
        let id = cadcraft_doc::ArchId(3);
        assert_eq!(proxy_handle(id, "leaf"), cadcraft_doc::arch::opening_handle(id));
        assert_eq!(proxy_handle(id, "floor"), cadcraft_doc::arch::room_handle(id));
        assert_eq!(proxy_handle(id, "roof"), cadcraft_doc::arch::roof_handle(id));
    }

    #[test]
    fn pane3d_camera_move_does_not_reupload_meshes() {
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&house())));
        let key = pane.scene_key();
        orbit_by_drag(&mut pane, 40.0, 10.0, 0.01);
        zoom_by_wheel(&mut pane, 1.0);
        pane.walk(1.0, 1.0, WALK_STEP);
        pane.turn(10.0);
        assert_eq!(pane.scene_key(), key, "camera moves must not change the mesh cache key");
        assert!(!pane.sync(1, Some(&house())), "and must not rebuild the scene");
        let Some((device, queue)) = test_device() else { return };
        let mut off = cadcraft_render3d::Offscreen::with_device(device, queue, (32, 24));
        let scene = pane.scene.clone().expect("scene");
        off.render(&scene, key, &pane.camera(), &cadcraft_render3d::Light::DEFAULT);
        pane.walk(2.0, 0.0, WALK_STEP);
        off.render(&scene, key, &pane.camera(), &cadcraft_render3d::Light::DEFAULT);
        pane.use_orbit();
        off.render(&scene, key, &pane.camera(), &cadcraft_render3d::Light::DEFAULT);
        assert_eq!(off.uploads(), 1);
    }

    #[test]
    fn pane3d_loads_the_active_saved_camera() {
        use cadcraft_doc::arch::{Camera3d, CameraKind};
        let mut arch = house();
        arch.set_camera(Camera3d {
            name: "walk".into(),
            kind: CameraKind::Full,
            eye: Some([5.0, -3.0]),
            eye_height: Some(1.7),
            target: Some([5.0, 4.0, 1.7]),
            fov_deg: Some(70.0),
            ..Camera3d::default()
        })
        .expect("camera");
        arch.set_camera(Camera3d {
            name: "high".into(),
            kind: CameraKind::Orbit,
            azimuth_deg: Some(45.0),
            elevation_deg: Some(30.0),
            distance: Some(25.0),
            target: Some([1.0, 2.0, 3.0]),
            ..Camera3d::default()
        })
        .expect("camera");
        arch.active_camera = Some("walk".into());
        let mut pane = Pane3d::new();
        assert!(pane.sync(1, Some(&arch)));
        let f = full_of(&pane);
        assert!((f.eye.x - 5.0).abs() < 1e-9 && (f.eye.y + 3.0).abs() < 1e-9 && (f.eye.z - 1.7).abs() < 1e-9);
        assert!((f.fov_deg - 70.0).abs() < 1e-9);
        // Unchanged: nothing reloads, so the user's walking is kept.
        pane.walk(1.0, 0.0, WALK_STEP);
        assert!(!pane.load_active_camera(&arch));
        assert!((full_of(&pane).eye.y + 2.0).abs() < 1e-9);
        // Switching the active camera loads the orbit one.
        arch.active_camera = Some("high".into());
        assert!(pane.sync(2, Some(&arch)));
        match pane.camera() {
            cadcraft_render3d::Camera::Orbit(o) => {
                assert!((o.azimuth - 45.0).abs() < 1e-9 && (o.elevation - 30.0).abs() < 1e-9 && (o.distance - 25.0).abs() < 1e-9);
                assert!((o.target.x - 1.0).abs() < 1e-9 && (o.target.z - 3.0).abs() < 1e-9);
            }
            other => panic!("expected an orbit camera, got {other:?}"),
        }
    }

    #[test]
    fn a_3d_pick_becomes_the_2d_selection() {
        let (mut pane, mut session) = house_pane();
        let (_, size) = front_camera();
        let center = [240.0, 160.0];

        // The centre pixel strikes the door; its proxy handle is the opening's (0x8000 range),
        // not the wall's.
        assert!(pane.pick(size, center).is_some(), "the centre pixel strikes the door");
        assert!(apply_pick(&mut pane, &mut session).is_some(), "the door pick must be applied to the selection");
        assert_eq!(
            sel_handles(&session),
            vec![cadcraft_doc::arch::opening_handle(cadcraft_doc::ArchId(0)).hex()],
            "a 3D door pick selects the opening handle"
        );
    }

    #[test]
    fn a_3d_miss_leaves_the_selection_alone() {
        let (mut pane, mut session) = house_pane();
        let (_, size) = front_camera();
        let center = [240.0, 160.0];
        let sky = [8.0, 8.0];

        // Establish a baseline selection via a real 3D pick.
        assert!(pane.pick(size, center).is_some(), "the centre pixel strikes the door");
        assert!(apply_pick(&mut pane, &mut session).is_some());
        let before = sel_handles(&session);
        assert_eq!(
            before,
            vec![cadcraft_doc::arch::opening_handle(cadcraft_doc::ArchId(0)).hex()],
            "the baseline 3D pick selects the opening handle"
        );

        // A sky pick strikes nothing and must leave that selection exactly as it was.
        assert!(pane.pick(size, sky).is_none(), "a sky pixel must strike nothing");
        assert!(apply_pick(&mut pane, &mut session).is_none(), "a miss has no handle to apply");
        assert_eq!(sel_handles(&session), before, "a sky pick leaves the selection alone");
    }
}
