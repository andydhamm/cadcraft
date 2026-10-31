//! Light fixtures in 3D (phase 10): each fixture's housing (meshes with a `light-…` material,
//! probe part `light`) and its light ([`SceneLight`]), plus direct illuminance with shadow rays.
//!
//! Fixture housings never cast shadows, in the CPU [`illuminance`] or on the GPU: moving a
//! fixture changes no other light's shadows. `scene_lights`, `direct_illuminance` and
//! `illuminance` are stubs filled by `p10-arch3d-scene-lights` (signatures final); the housings
//! are in `crate::fixture`.

use crate::Scene3d;
use cadcraft_doc::ArchId;
use cadcraft_doc::arch::ArchModel;
pub use cadcraft_doc::arch::LightKind;
use cadcraft_geom::Vec3;

/// A light in the scene, in world coordinates (metres, +Z up). Build it with
/// `SceneLight { …, ..Default::default() }` outside this file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneLight {
    /// The fixture's id (label `lf{n}`).
    pub id: ArchId,
    pub kind: LightKind,
    /// The light point.
    pub position: Vec3,
    /// Unit aim of a spot (`[0,0,-1]` straight down); a point light carries `[0,0,-1]`, unused.
    pub direction: Vec3,
    /// Luminous intensity in candela inside the cone (`arch::light::intensity_cd`).
    pub intensity: f64,
    /// Linear RGB of the colour temperature, the largest component 1 (`arch::light::kelvin_to_rgb`).
    pub color: [f64; 3],
    /// cos of half the cone (full intensity inside) and of half the falloff (zero outside);
    /// `-1, -1` for a point light. `cos_inner >= cos_outer`.
    pub cos_inner: f64,
    pub cos_outer: f64,
}

impl SceneLight {
    /// Intensity (cd) toward the unit direction `to_point` (from the light to the lit point):
    /// `intensity` inside the cone, 0 outside the falloff, a smoothstep between them; a point
    /// light is `intensity` everywhere. A direction exactly on the cone edge counts as inside.
    pub fn intensity_toward(&self, to_point: Vec3) -> f64 {
        if self.kind == LightKind::Point {
            return self.intensity;
        }
        let c = to_point.dot(self.direction);
        if !c.is_finite() || c < self.cos_outer - 1e-12 {
            return 0.0;
        }
        if c >= self.cos_inner - 1e-12 || self.cos_inner <= self.cos_outer {
            return self.intensity;
        }
        let t = ((c - self.cos_outer) / (self.cos_inner - self.cos_outer)).clamp(0.0, 1.0);
        self.intensity * t * t * (3.0 - 2.0 * t)
    }
}

/// Whether a mesh material belongs to a light fixture housing (`light-trim`, `light-lens`, …).
pub fn is_light_material(material: &str) -> bool {
    material.starts_with("light-")
}

/// One [`SceneLight`] per fixture that is on and has a light point
/// (`arch::light::light_point`), in fixture order.
pub fn scene_lights(model: &ArchModel) -> Vec<SceneLight> {
    let _ = model;
    Vec::new()
}

/// The housings of every fixture (in `crate::fixture`, filled by `p10-arch3d-fixture-meshes`).
pub use crate::fixture::fixture_meshes;

/// Direct illuminance (lux) from one light at `point` on a surface with unit `normal`, without
/// occlusion: `I(θ) · max(0, n·l) / d²` with `l` the unit vector from the point to the light.
/// 0 when the point is at the light (d < 1e-9) or behind the surface.
pub fn direct_illuminance(light: &SceneLight, point: Vec3, normal: Vec3) -> f64 {
    let _ = (light, point, normal);
    0.0
}

/// Direct illuminance at a point from every light of the scene, with shadow rays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Illuminance {
    /// The total, lux.
    pub lux: f64,
    /// Each light's share (fixture id, lux), in light order; a shadowed light gives 0.
    pub per_light: Vec<(ArchId, f64)>,
}

/// [`direct_illuminance`] summed over `scene.lights`, each light counted only when the segment
/// from `point` to the light is not blocked by a scene triangle (fixture housings excluded):
/// a hit at a distance in `(1e-6, d − 1e-6)` blocks it.
pub fn illuminance(scene: &Scene3d, point: Vec3, normal: Vec3) -> Illuminance {
    let _ = (scene, point, normal);
    Illuminance::default()
}
