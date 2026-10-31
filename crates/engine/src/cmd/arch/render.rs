//! `render`: a 3D still of the drawing (phase 10) through the renderer the app installs with
//! [`set_render_hooks`] (the CLI and the desktop app; the engine has no GPU). One renderer lives
//! for the whole session, so its mesh upload and shadow maps are reused from one `render` to the
//! next: a call that only moves the camera builds no shadow map. `sunstudy` uses the same hook.
//!
//! Filled here: the hook types, [`set_render_hooks`], [`render_hooks`], [`stats`]. The command
//! itself is filled by `p10-cmd-render` (spec stub below).

use std::path::PathBuf;
use std::sync::OnceLock;

use cadcraft_arch3d::camera::Camera;
use cadcraft_doc::Drawing;
use serde_json::Value;

use crate::cmd::CommandSpec;

/// What lights the still besides the fixtures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SunChoice {
    /// The default light (`render3d::Light::DEFAULT`), no sun shadow.
    Default,
    /// Night: the sun is off.
    Off,
    /// The sun from this direction (toward the sun, drawing coordinates, +Z up), with its shadow.
    Dir([f64; 3]),
}

/// One `render` (or one `sunstudy` still).
#[derive(Clone, Debug)]
pub struct RenderRequest<'a> {
    pub drawing: &'a Drawing,
    /// `None`: the active saved camera, else the orbit fitted to the scene.
    pub camera: Option<Camera>,
    /// Pixels, each side 1..=8192.
    pub size: (u32, u32),
    /// Where to write the PNG; `None` renders without writing (the reply still reports).
    pub out: Option<PathBuf>,
    /// Finalize Render instead of the interactive view.
    pub final_render: bool,
    pub sun: SunChoice,
    /// `shaded`, `shaded-edges`, `hidden-line` or `glass-house`.
    pub style: String,
    /// Draw the ground plane.
    pub ground: bool,
    /// Lights shadowed at most in the interactive view (Finalize shadows every light).
    pub budget: usize,
    /// World points whose pixel and colour the reply reports (at most 64).
    pub samples: Vec<[f64; 3]>,
}

/// What a session's renderer has done so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub shadow_maps_built: u64,
    pub final_renders: u64,
}

/// The app's renderer: `render` returns the reply JSON of one still (see `p10-cli-render-hook`
/// for its keys) or an error message; `stats` reports the counters.
pub struct RenderHooks {
    pub render: fn(&RenderRequest) -> Result<Value, String>,
    pub stats: fn() -> RenderStats,
}

static HOOKS: OnceLock<RenderHooks> = OnceLock::new();

/// Install the app's renderer (the first call wins).
pub fn set_render_hooks(h: RenderHooks) {
    let _ = HOOKS.set(h);
}

/// The installed renderer, if any.
pub fn render_hooks() -> Option<&'static RenderHooks> {
    HOOKS.get()
}

/// The installed renderer's counters (zeros without one): `build3d` reports them.
pub fn stats() -> RenderStats {
    HOOKS.get().map(|h| (h.stats)()).unwrap_or_default()
}

/// Command specs (filled in by `p10-cmd-render`).
pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
