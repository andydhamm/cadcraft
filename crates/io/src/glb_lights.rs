//! `KHR_lights_punctual` for the GLB (phase 10): one glTF light per scene light (spots with their
//! cone, intensity in candela as glTF wants for punctual lights) and one node per light under the
//! root, named `light-lf{n}-source`. Filled by `p10-io-glb-lights`.

use cadcraft_arch3d::Scene3d;
use serde_json::Value;

/// The extension's `lights` array and one node per light (`{"name", "translation",
/// "rotation"?, "extensions": {"KHR_lights_punctual": {"light": i}}}`), both in
/// `scene.lights` order; empty when the scene has no lights.
pub(crate) fn punctual_lights(scene: &Scene3d) -> (Vec<Value>, Vec<Value>) {
    let _ = scene;
    (Vec::new(), Vec::new())
}
