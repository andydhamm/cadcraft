//! The architectural 3D pane: a dockable panel showing the built arch scene.
//!
//! Owns an orbit camera and renders the scene offscreen into its own colour+depth
//! targets (same `egui_wgpu::Callback` pattern as [`gpu::install`]). The scene is
//! rebuilt only when `Drawing::arch_revision` advances since the last frame.
//! Without a GPU adapter the pane shows a static "3D not available" placeholder
//! rather than panicking or showing a blank window.
//!
//! **Status:** nothing sets [`Pane3d::available`] yet and `render3d` has no draw pass, so
//! the pane always shows the placeholder; it must not blame a missing GPU for that.

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
}

impl Default for Pane3d {
    fn default() -> Self {
        Self::new()
    }
}

impl Pane3d {
    /// New pane with a default orbit and no scene built yet.
    pub fn new() -> Self {
        Self { orbit: Orbit::default(), last_revision: 0, mesh_count: 0, available: false, has_content: false }
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
            // `available` is never set until the renderer exists: saying "no GPU adapter"
            // here sent people with a working GPU after a hardware problem they do not have.
            ui.label("3D not available yet");
            ui.label("The 3D view is not implemented in this build; Build 3D and .glb export work.");
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
}
