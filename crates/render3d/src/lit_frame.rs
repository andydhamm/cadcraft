//! A lit frame of the interactive view (phase 10): pick the lights to draw and shadow, bring
//! their shadow maps up to date, pack the uniforms and bind group 2.
//!
//! The choice of shadowed lights (nearest and brightest, `lights::select_shadowed`) is made when
//! the scene or the lights change and kept while only the camera moves, so a camera move never
//! builds a shadow map. Stub filled by `p10-r3d-lit-frame`: [`Offscreen::lit_frame`].

use crate::light::Light;
use crate::lit::LitFrame;
use crate::offscreen::Offscreen;
use crate::types::Scene3d;

/// What the interactive view keeps between frames (filled by `p10-r3d-lit-frame`: the fixture
/// atlas, the shadowed selection and the scene/lights key it was made for).
#[derive(Default)]
pub struct LitState {
    pub(crate) atlas: Option<crate::shadow_atlas::ShadowAtlas>,
}

/// What a lit frame lights with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// The sun (`None` = night: the sun is off).
    pub sun: Option<Light>,
    /// Build and use the sun's shadow map (`--sun` / a site sun; the default light casts none).
    pub sun_shadow: bool,
    /// Lights shadowed at most (`lights::DEFAULT_SHADOW_BUDGET`); the rest light unshadowed.
    pub budget: usize,
    /// Display units per lux (`lights::DEFAULT_EXPOSURE`).
    pub exposure: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting {
            sun: Some(Light::DEFAULT),
            sun_shadow: false,
            budget: crate::lights::DEFAULT_SHADOW_BUDGET,
            exposure: crate::lights::DEFAULT_EXPOSURE,
        }
    }
}

impl Offscreen {
    /// Group 2 for drawing `scene` (uploaded under `scene_key`, with the ground quad when
    /// `ground`) seen from `eye`, lit by `lighting` and the scene's fixture lights. `None` when the
    /// lit pipelines are unavailable; the caller then draws without fixture lights.
    pub fn lit_frame(&mut self, scene: &Scene3d, scene_key: u64, eye: [f64; 3], lighting: &Lighting, ground: bool) -> Option<LitFrame> {
        let _ = (scene, scene_key, eye, lighting, ground);
        None
    }
}
