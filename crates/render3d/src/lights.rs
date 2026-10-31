//! Fixture lights on the GPU (phase 10): which lights are drawn and shadowed, their uniform, and
//! the projection of their shadow maps.
//!
//! **Group 2 of the lit pipelines** (`crate::lit`), all read in the fragment stage:
//! - binding 0: `FrameU` ([`FRAME_UNIFORM_SIZE`] bytes): `sun_vp: mat4x4<f32>` (the sun map's
//!   light matrix), `sun: vec4<f32>` (x = depth bias, y = 1 when a sun map is bound, z = 1 when
//!   the sun is on, w = 0), `params: vec4<f32>` (x = exposure in display units per lux, y = 1 to
//!   tone-map (Finalize), z = PCF radius in texels (0 = one tap), w = shadow-atlas layer size).
//! - binding 1: the sun map, `texture_depth_2d` (a 1×1 map when there is none).
//! - binding 2: `LightsU` ([`LIGHTS_UNIFORM_SIZE`] bytes): `count: vec4<u32>` (x = lights
//!   drawn), `lights: array<GpuLight, 64>`, `spot_vp: array<mat4x4<f32>, 64>` (light i's spot
//!   matrix in slot i).
//! - binding 3: the shadow atlas, `texture_depth_2d_array` (one layer per spot, six per point
//!   light; a 1×1×1 array when nothing is shadowed).
//!
//! A [`GpuLight`] is four `vec4<f32>`: `pos_range` (xyz light point, w = far plane of its map),
//! `dir_cos_outer` (xyz unit spot aim, w = cos of half the falloff, −2 for a point light),
//! `color_cos_inner` (xyz = colour × intensity in candela, w = cos of half the cone), `shadow`
//! (x = first atlas layer or −1 when unshadowed, y = 1 for a cube (point light), z = near
//! plane, w = depth bias in metres).
//!
//! Shadow maps store the perspective depth `far/(far−near)·(1 − near/z)` of the distance `z`
//! along the face's forward axis; `shaders/light_math.wgsl` turns it back into metres with
//! [`linear_depth`] and compares distances. The six cube faces follow [`cube_face`].
//!
//! Filled here: the constants, [`cube_face`], [`cube_face_view_proj`], [`spot_view_proj`],
//! [`linear_depth`]. Stubs filled by `p10-r3d-light-pack`: [`shadow_priority`],
//! [`select_shadowed`], [`select_drawn`], [`pack_lights`].

use cadcraft_arch3d::light::SceneLight;

/// Most lights one frame draws (the uniform's array length).
pub const MAX_GPU_LIGHTS: usize = 64;
/// Lights shadowed in the interactive view unless the caller asks for another budget.
pub const DEFAULT_SHADOW_BUDGET: usize = 8;
/// Shadow-map edge (texels) per atlas layer in the interactive view and in Finalize Render.
pub const INTERACTIVE_SHADOW_SIZE: u32 = 512;
pub const FINAL_SHADOW_SIZE: u32 = 2048;
/// Most atlas layers (the WebGL2 downlevel `max_texture_array_layers`).
pub const MAX_SHADOW_LAYERS: u32 = 256;
/// Near plane of every fixture shadow map (m).
pub const SHADOW_NEAR: f32 = 0.05;
/// Display units per lux: a white diffuse face under 100 lx shows 100 · 0.01 / π ≈ 0.32.
pub const DEFAULT_EXPOSURE: f32 = 0.01;
/// Ambient level with the sun off (night).
pub const NIGHT_AMBIENT: f32 = 0.02;
/// Bytes of `FrameU` (binding 0) and `LightsU` (binding 2).
pub const FRAME_UNIFORM_SIZE: u64 = 96;
pub const LIGHTS_UNIFORM_SIZE: u64 = 16 + 64 * MAX_GPU_LIGHTS as u64 + 64 * MAX_GPU_LIGHTS as u64;

/// A light's shadow-map slot in the atlas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowSlot {
    /// First layer (a cube uses six from here, in [`cube_face`] order).
    pub first_layer: u32,
    /// A point light's cube (six layers) rather than a spot's single map.
    pub cube: bool,
    /// The spot's matrix ([`spot_view_proj`]); unused for a cube.
    pub view_proj: [[f32; 4]; 4],
    pub near: f32,
    pub far: f32,
    /// Depth bias in metres.
    pub bias: f32,
}

/// Indices of `lights` ordered by how much they matter from `eye`: intensity (cd) divided by
/// max(squared distance from `eye` to the light, 1 m²), largest first; ties by lower index.
/// Non-finite scores sort last.
pub fn shadow_priority(lights: &[SceneLight], eye: [f64; 3]) -> Vec<usize> {
    let _ = eye;
    (0..lights.len()).collect()
}

/// The first `budget` of [`shadow_priority`], returned in ascending index order.
pub fn select_shadowed(lights: &[SceneLight], eye: [f64; 3], budget: usize) -> Vec<usize> {
    let _ = (lights, eye, budget);
    Vec::new()
}

/// The lights a frame draws: all of them when there are at most [`MAX_GPU_LIGHTS`], else the
/// first [`MAX_GPU_LIGHTS`] of [`shadow_priority`]; ascending index order.
pub fn select_drawn(lights: &[SceneLight], eye: [f64; 3]) -> Vec<usize> {
    let _ = (lights, eye);
    Vec::new()
}

/// The `LightsU` bytes ([`LIGHTS_UNIFORM_SIZE`] long, std140, little-endian) for the `drawn`
/// lights (at most [`MAX_GPU_LIGHTS`], in that order) with their shadow slots (`slots` pairs a
/// light index with its slot; a drawn light without one is unshadowed). Colour = `color` ×
/// `intensity`.
pub fn pack_lights(lights: &[SceneLight], drawn: &[usize], slots: &[(usize, ShadowSlot)]) -> Vec<u8> {
    let _ = (lights, drawn, slots);
    vec![0; LIGHTS_UNIFORM_SIZE as usize]
}

/// Forward and up axes of cube face `face`: 0 +X, 1 −X, 2 +Y, 3 −Y (up +Z), 4 +Z, 5 −Z (up +Y).
/// The face's right axis is `forward × up`. Faces past 5 read as 5.
pub fn cube_face(face: usize) -> ([f64; 3], [f64; 3]) {
    match face {
        0 => ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        1 => ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        2 => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        3 => ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
        4 => ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        _ => ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = dot(a, a).sqrt();
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Column-major `clip = M · [p, 1]` of a perspective camera at `eye` looking along unit `f` with
/// up hint `u`, `scale` = cot(fov/2), depth 0 at `near` and 1 at `far` (wgpu range).
fn perspective_look(eye: [f64; 3], f: [f64; 3], u: [f64; 3], scale: f64, near: f64, far: f64) -> Option<[[f32; 4]; 4]> {
    if !(near.is_finite() && far.is_finite() && near > 0.0 && far > near && scale.is_finite() && scale > 0.0) {
        return None;
    }
    let f = unit(f)?;
    let r = unit(cross(f, u))?;
    let u = cross(r, f);
    let a = far / (near - far);
    let b = far * near / (near - far);
    // Rows of the combined matrix: x = s·r·(p−e), y = s·u·(p−e), z = A·(−f·(p−e)) + B, w = f·(p−e).
    let rows = [
        [scale * r[0], scale * r[1], scale * r[2], -scale * dot(r, eye)],
        [scale * u[0], scale * u[1], scale * u[2], -scale * dot(u, eye)],
        [-a * f[0], -a * f[1], -a * f[2], a * dot(f, eye) + b],
        [f[0], f[1], f[2], -dot(f, eye)],
    ];
    let mut m = [[0.0f32; 4]; 4];
    for (row, vals) in rows.iter().enumerate() {
        for (col, v) in vals.iter().enumerate() {
            if !v.is_finite() {
                return None;
            }
            m[col][row] = *v as f32;
        }
    }
    Some(m)
}

/// The matrix of cube face `face` of a point light at `pos` (90° field of view, square).
pub fn cube_face_view_proj(pos: [f64; 3], face: usize, near: f32, far: f32) -> Option<[[f32; 4]; 4]> {
    let (f, u) = cube_face(face);
    perspective_look(pos, f, u, 1.0, f64::from(near), f64::from(far))
}

/// Widest spot map (full field of view, degrees); a spot whose falloff is wider is shadowed with
/// a cube like a point light.
pub const MAX_SPOT_MAP_FOV: f64 = 150.0;

/// A spot's shadow-map matrix: from its light point along its aim, with a field of view of its
/// falloff plus 4° (so the cone's edge is inside the map), up = +Z unless the aim is within 8° of
/// vertical (then +Y). `None` when the field of view would exceed [`MAX_SPOT_MAP_FOV`] or a value
/// is not finite.
pub fn spot_view_proj(light: &SceneLight, near: f32, far: f32) -> Option<[[f32; 4]; 4]> {
    let half_falloff = light.cos_outer.clamp(-1.0, 1.0).acos();
    let fov = half_falloff.to_degrees() * 2.0 + 4.0;
    if !fov.is_finite() || fov > MAX_SPOT_MAP_FOV {
        return None;
    }
    let f = [light.direction.x, light.direction.y, light.direction.z];
    let up = if f[2].abs() > 8f64.to_radians().cos() { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let scale = 1.0 / (fov.to_radians() * 0.5).tan();
    let p = light.position;
    perspective_look([p.x, p.y, p.z], f, up, scale, f64::from(near), f64::from(far))
}

/// Distance along the map's forward axis (m) of a stored depth `d` (0..1) — the inverse of
/// `far/(far−near)·(1 − near/z)`. Mirrors `linear_depth` in `shaders/light_math.wgsl`.
pub fn linear_depth(d: f32, near: f32, far: f32) -> f32 {
    let k = 1.0 - d * (far - near) / far;
    if k <= 1e-7 { f32::MAX } else { near / k }
}

/// WGSL helpers every lit shader includes: `linear_depth`, `cube_face_of`, `cube_uv_depth`.
pub const LIGHT_MATH_WGSL: &str = include_str!("shaders/light_math.wgsl");

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &[[f32; 4]; 4], p: [f64; 3]) -> [f64; 4] {
        let mut out = [0.0; 4];
        for (row, o) in out.iter_mut().enumerate() {
            *o = (0..4).map(|col| f64::from(m[col][row]) * if col < 3 { p[col] } else { 1.0 }).sum();
        }
        out
    }

    /// Rust mirror of `cube_uv_depth` in light_math.wgsl.
    fn cube_uv_depth(v: [f64; 3], near: f64, far: f64) -> (usize, [f64; 2], f64) {
        let (ax, ay, az) = (v[0].abs(), v[1].abs(), v[2].abs());
        let face = if ax >= ay && ax >= az {
            if v[0] >= 0.0 { 0 } else { 1 }
        } else if ay >= az {
            if v[1] >= 0.0 { 2 } else { 3 }
        } else if v[2] >= 0.0 {
            4
        } else {
            5
        };
        let (f, u) = cube_face(face);
        let r = cross(f, u);
        let ma = dot(v, f);
        let ndc = [dot(v, r) / ma, dot(v, u) / ma];
        (face, [ndc[0] * 0.5 + 0.5, 0.5 - ndc[1] * 0.5], far / (far - near) * (1.0 - near / ma))
    }

    #[test]
    fn cube_lookup_matches_the_face_matrices() {
        let pos = [1.0, 2.0, 2.4];
        for p in [[3.0, 2.5, 1.0], [-2.0, 2.2, 2.0], [1.3, 5.0, 0.0], [0.0, -3.0, 2.0], [1.1, 2.1, 7.0], [1.2, 1.9, -3.0]] {
            let v = [p[0] - pos[0], p[1] - pos[1], p[2] - pos[2]];
            let (face, uv, depth) = cube_uv_depth(v, 0.05, 20.0);
            let m = cube_face_view_proj(pos, face, 0.05, 20.0).unwrap_or([[0.0; 4]; 4]);
            let c = apply(&m, p);
            let ndc = [c[0] / c[3], c[1] / c[3], c[2] / c[3]];
            assert!((ndc[0] * 0.5 + 0.5 - uv[0]).abs() < 1e-5 && (0.5 - ndc[1] * 0.5 - uv[1]).abs() < 1e-5, "{p:?} face {face}");
            assert!((ndc[2] - depth).abs() < 1e-5, "{p:?} depth {} vs {depth}", ndc[2]);
            let back = linear_depth(depth as f32, 0.05, 20.0);
            assert!((f64::from(back) - dot(v, cube_face(face).0)).abs() < 1e-3);
            assert!((0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]));
        }
    }

    #[test]
    fn a_spot_straight_down_maps_its_axis_to_the_map_centre() {
        let l = SceneLight {
            kind: cadcraft_arch3d::light::LightKind::Spot,
            position: cadcraft_geom::Vec3::new(1.0, 1.0, 2.4),
            direction: cadcraft_geom::Vec3::new(0.0, 0.0, -1.0),
            intensity: 950.4,
            color: [1.0; 3],
            cos_inner: 30f64.to_radians().cos(),
            cos_outer: 30f64.to_radians().cos(),
            ..Default::default()
        };
        let m = spot_view_proj(&l, 0.05, 10.0).unwrap_or([[0.0; 4]; 4]);
        let c = apply(&m, [1.0, 1.0, 0.0]);
        assert!((c[0] / c[3]).abs() < 1e-6 && (c[1] / c[3]).abs() < 1e-6);
        let z = f64::from(linear_depth((c[2] / c[3]) as f32, 0.05, 10.0));
        assert!((z - 2.4).abs() < 1e-3, "{z}");
        // The cone's edge (30° off the axis) lies inside the map.
        let edge = apply(&m, [1.0 + 2.4 * 30f64.to_radians().tan(), 1.0, 0.0]);
        assert!((edge[0] / edge[3]).abs() < 1.0 && (edge[1] / edge[3]).abs() < 1.0);
        let wide = SceneLight { cos_outer: -0.5, ..l };
        assert!(spot_view_proj(&wide, 0.05, 10.0).is_none());
    }
}
