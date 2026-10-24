//! The architectural 3D pane: a dockable panel showing the built arch scene.
//!
//! Owns an orbit camera and renders the scene offscreen into its own colour+depth
//! targets (same `egui_wgpu::Callback` pattern as [`gpu::install`]). The scene is
//! rebuilt only when `Drawing::arch_revision` advances since the last frame.
//! Without a GPU adapter the pane shows a static "3D not available" placeholder
//! rather than panicking or showing a blank window.

use std::cell::RefCell;

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

/// Per-frame state for the 3D pane.
#[derive(Debug)]
pub struct Pane3d {
    /// The current orbit camera.
    pub orbit: Orbit,
    /// The `arch_revision` the scene was last built at (0 = never built).
    pub last_revision: u64,
    /// Number of meshes in the last build (0 when no scene).
    pub mesh_count: usize,
    /// Whether a wgpu adapter is available (set on first frame if a render state is present).
    pub available: bool,
    /// Whether the built scene has visible content (walls, floors, etc.).
    pub has_content: bool,
    /// The last built scene (`None` until the first successful build).
    scene: Option<cadcraft_render3d::Scene3d>,
}

impl Pane3d {
    /// New pane with a default orbit and no scene built yet.
    pub fn new() -> Self {
        Self { orbit: Orbit::default(), last_revision: 0, mesh_count: 0, available: false, has_content: false, scene: None }
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
                let scene = cadcraft_arch3d::build(m);
                let mesh_count = scene.meshes.len();
                let has_content = !scene.meshes.is_empty();
                self.scene = Some(scene);
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

    /// The scene currently held (`None` when nothing has been built).
    pub fn scene(&self) -> Option<&cadcraft_arch3d::Scene3d> {
        self.scene.as_ref()
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
        cadcraft_render3d::Camera::Orbit(orbit)
    }
}

/// Draw the 3D pane content into `ui`.
///
/// When no GPU adapter is available or no scene has been built, a text placeholder is
/// shown. When a scene exists, the offscreen texture would be presented as an image via
/// the `egui_wgpu::Callback` pattern (same as `canvas`/`gpu`).
pub fn draw(ui: &mut egui::Ui, pane: &mut Pane3d) {
    egui::Frame::default().show(ui, |ui| {
        ui.add_space(6.0);
        if !pane.available {
            ui.label("3D not available");
            ui.label("No GPU adapter detected.");
        } else if pane.last_revision == 0 || !pane.has_content {
            ui.label("No walls yet.");
        } else {
            ui.label(format!("Scene: {} mesh(es), revision {}", pane.mesh_count, pane.last_revision));
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let cadcraft_render3d::Camera::Orbit(o) = cam;

        // `fit` centres the camera on the scene; for a 0..10 m wall that is x = 5.
        assert!((o.target.x - 5.0).abs() < 0.25);
        assert!(o.distance >= cadcraft_render3d::camera::MIN_DISTANCE);
        assert!((o.distance - f64::max(10.0, cadcraft_render3d::camera::MIN_DISTANCE)).abs() < 1e-9);
        // The render3d Orbit stores angles in degrees; the pane default is 0.0 / 0.35 rad.
        assert!((o.azimuth - 0.0).abs() < 1e-9);
        assert!((o.elevation - 0.35 * (180.0 / std::f64::consts::PI)).abs() < 1e-6);
    }
}
