//! Finalize Render (phase 10): one still from the current camera with every light shadowed at
//! high resolution (`lights::FINAL_SHADOW_SIZE` layers in its own atlas), soft shadow edges
//! (PCF radius 2), 2×2 supersampling and tone mapping. It runs in steps so the pane can show
//! progress and cancel it: one step per shadowed light, then one per band of image rows, then
//! the downsample.
//!
//! Stubs filled by `p10-r3d-finalize`: [`FinalRender::start`], [`FinalRender::step`],
//! [`render_final`].

use crate::Render3dError;
use crate::camera::Camera;
use crate::lit_frame::Lighting;
use crate::offscreen::Offscreen;
use crate::types::Scene3d;

/// Where a Finalize Render is.
#[derive(Clone, Debug, PartialEq)]
pub enum FinalStatus {
    /// Still working; `progress` is 0..1.
    Running { progress: f32 },
    /// Finished: the RGBA8 pixels (`width·height·4`, rows top to bottom) of the still.
    Done { width: u32, height: u32, rgba: Vec<u8> },
    /// Cancelled before it finished.
    Cancelled,
    /// It could not run (no lit pipelines, a GPU error).
    Failed(String),
}

/// A Finalize Render in progress.
pub struct FinalRender {
    pub(crate) size: (u32, u32),
    pub(crate) cancelled: bool,
    pub(crate) status: FinalStatus,
}

impl FinalRender {
    /// Start a still of `scene` (uploaded under `scene_key`) from `camera` at `size` pixels.
    pub fn start(off: &mut Offscreen, scene: &Scene3d, scene_key: u64, camera: &Camera, lighting: &Lighting, ground: bool, size: (u32, u32)) -> Self {
        let _ = (off, scene, scene_key, camera, lighting, ground);
        FinalRender { size, cancelled: false, status: FinalStatus::Failed("Finalize Render is not available yet".into()) }
    }

    /// Do one bounded piece of work and report where it is. After `Done`, `Cancelled` or
    /// `Failed` it keeps returning that status.
    pub fn step(&mut self, off: &mut Offscreen) -> FinalStatus {
        let _ = off;
        if self.cancelled {
            self.status = FinalStatus::Cancelled;
        }
        self.status.clone()
    }

    /// Stop at the next step.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// The pixel size of the still.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

/// Run a Finalize Render to the end and return the PNG (headless `render --final` and the
/// `render {"final":true}` command). Counts one final render on `off`.
pub fn render_final(
    off: &mut Offscreen,
    scene: &Scene3d,
    scene_key: u64,
    camera: &Camera,
    lighting: &Lighting,
    ground: bool,
    size: (u32, u32),
) -> Result<Vec<u8>, Render3dError> {
    let _ = (off, scene, scene_key, camera, lighting, ground, size);
    Err(Render3dError::Wgpu("Finalize Render is not available yet".into()))
}
