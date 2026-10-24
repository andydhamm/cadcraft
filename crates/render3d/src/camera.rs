//! Orbit camera for the 3D viewport: eye/view/projection matrices, bounds fitting and
//! clamped orbit control.

use cadcraft_arch3d::mesh::Bounds3;
use cadcraft_geom::{Mat4, Vec2, Vec3};

/// Upper bound for the orbit distance (keeps floats sane and the projection bounded).
pub const MAX_DISTANCE: f64 = 1_000_000.0;
/// Lower bound for the orbit distance (prevents the eye collapsing onto the target).
pub const MIN_DISTANCE: f64 = 0.01;
/// Maximum |elevation| in degrees. ±90° degenerates the view basis.
const MAX_ELEV: f64 = 89.0;

/// An orbit camera: the eye orbits `target` in spherical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    pub target: Vec3,
    /// Azimuth in degrees (0 = +X, 90 = +Y).
    pub azimuth: f64,
    /// Elevation in degrees (0 = horizon, +90 = straight up). Clamped to ±89°.
    pub elevation: f64,
    /// Distance from `target` to `eye`. Clamped to `[MIN_DISTANCE, MAX_DISTANCE]`.
    pub distance: f64,
    /// Vertical field of view in degrees.
    pub fov_deg: f64,
}

/// Camera types for the 3D viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Camera {
    /// An orbit camera.
    Orbit(Orbit),
}

impl Orbit {
    /// Construct an orbit; fields are clamped by `orbit`.
    pub fn new(target: Vec3, azimuth: f64, elevation: f64, distance: f64, fov_deg: f64) -> Self {
        Self { target, azimuth, elevation, distance, fov_deg }
    }

    /// An orbit looking at the origin from the default azimuth/elevation.
    pub fn at_origin(azimuth: f64, elevation: f64, distance: f64, fov_deg: f64) -> Self {
        Self::new(Vec3::new(0.0, 0.0, 0.0), azimuth, elevation, distance, fov_deg)
    }

    /// Apply a clamped orbit control: pan/zoom deltas, returning a new orbit.
    ///
    /// `daz`/`del` are deltas in degrees; `dzoom` is a multiplicative factor applied to
    /// `distance` (a value `< 1.0` zooms in). Elevation is clamped to ±89° and distance to
    /// `[MIN_DISTANCE, MAX_DISTANCE]`; `fov_deg` stays unchanged.
    pub fn orbit(&self, daz: f64, del: f64, dzoom: f64) -> Self {
        let el = clamp_deg(self.elevation + del, -MAX_ELEV, MAX_ELEV);
        let dist = clamp_pos(self.distance * dzoom, MIN_DISTANCE, MAX_DISTANCE);
        Orbit::new(self.target, self.azimuth + daz, el, dist, self.fov_deg)
    }

    /// Eye position in world space.
    pub fn eye(&self) -> Vec3 {
        let a = self.azimuth.to_radians();
        let e = self.elevation.to_radians();
        let x = self.distance * e.cos() * a.sin();
        let y = self.distance * e.cos() * a.cos();
        let z = self.distance * e.sin();
        Vec3::new(self.target.x + x, self.target.y + y, self.target.z + z)
    }

    /// View (world→camera) matrix.
    pub fn view(&self) -> Mat4 {
        let eye = self.eye();
        // Forward points from eye toward target.
        let fw = (self.target - eye).normalized();
        let up_w = Vec3::new(0.0, 0.0, 1.0);
        // If we look straight up/down, the world-up is degenerate; fall back to +X.
        let mut up = up_w;
        let c = fw.cross(up);
        if c.x * c.x + c.y * c.y + c.z * c.z < 1e-12 {
            up = Vec3::new(1.0, 0.0, 0.0);
        }
        let right = fw.cross(up).normalized();
        let new_up = right.cross(fw);
        // Column-major 4x4 (matches a WGSL `matrix<float, 4x4>` uniform).
        let m = [
            [right.x, new_up.x, -fw.x, -right.dot(eye)],
            [right.y, new_up.y, -fw.y, -new_up.dot(eye)],
            [right.z, new_up.z, -fw.z, fw.dot(eye)],
            [0.0, 0.0, 0.0, 1.0],
        ];
        Mat4 { m }
    }

    /// Perspective projection matrix for `aspect` (width/height).
    pub fn proj(&self, aspect: f64) -> Mat4 {
        let half = self.fov_deg.to_radians() / 2.0;
        let tan_half = half.tan();
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        let z_near = 0.1;
        let z_far = 10_000.0;
        let m = [
            [1.0 / (aspect * tan_half), 0.0, 0.0, 0.0],
            [0.0, 1.0 / tan_half, 0.0, 0.0],
            [0.0, 0.0, (z_far + z_near) / (z_near - z_far), -1.0],
            [0.0, 0.0, (2.0 * z_near * z_far) / (z_near - z_far), 0.0],
        ];
        Mat4 { m }
    }

    /// A comfortable orbit that frames `bounds`: target at the box centre and a
    /// distance large enough for the whole box to fit, at a pleasant 30°/30° angle.
    pub fn fit(b: &Bounds3) -> Self {
        let min = b.min;
        let max = b.max;
        let c = Vec3::new((min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0, (min[2] + max[2]) / 2.0);
        // Diagonal length gives a distance that fits the box from any angle.
        let half_diag = (0.5 * (max[0] - min[0]).abs() + 0.5 * (max[1] - min[1]).abs() + 0.5 * (max[2] - min[2]).abs()).max(0.1);
        // Distance = diagonal / (2 * sin(fov/2)) so the far diagonal just fits vertically.
        let fov = 45.0;
        let s = (fov * std::f64::consts::PI / 360.0).sin();
        let dist = (half_diag * 3.0_f64.sqrt() / s).max(MIN_DISTANCE);
        Orbit::new(c, 30.0, 30.0, dist, fov)
    }
}

fn clamp_deg(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

fn clamp_pos(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// Project scene bounds into viewport pixel coordinates.
///
/// Returns `[min_x, min_y, max_x, max_y]` in pixel coordinates with y-down,
/// clamped to `[0, w] x [0, h]`. Never returns NaN or panics on degenerate
/// input (empty bounds, non-finite corners, zero-size viewport).
pub fn projected_bounds(camera: &Camera, bounds: &Bounds3, size: (u32, u32)) -> [f32; 4] {
    let (w, h) = size;
    let aspect = w as f64 / h.max(1) as f64;
    let w_f = w as f64;
    let h_f = h.max(1) as f64;
    let orbit = match camera {
        Camera::Orbit(o) => o,
    };
    // Empty bounds (min == max): return the viewport centre pixel.
    if bounds.min == bounds.max {
        let cx = (w_f / 2.0).clamp(0.0, w_f) as f32;
        let cy = (h_f / 2.0).clamp(0.0, h_f) as f32;
        return [cx, cy, cx, cy];
    }
    let view = orbit.view();
    let proj = orbit.proj(aspect);
    let corners = [
        [bounds.min[0], bounds.min[1], bounds.min[2]],
        [bounds.max[0], bounds.min[1], bounds.min[2]],
        [bounds.max[0], bounds.max[1], bounds.min[2]],
        [bounds.min[0], bounds.max[1], bounds.min[2]],
        [bounds.min[0], bounds.min[1], bounds.max[2]],
        [bounds.max[0], bounds.min[1], bounds.max[2]],
        [bounds.max[0], bounds.max[1], bounds.max[2]],
        [bounds.min[0], bounds.max[1], bounds.max[2]],
    ];
    let mut px_min = f64::MAX;
    let mut py_min = f64::MAX;
    let mut px_max = f64::NEG_INFINITY;
    let mut py_max = f64::NEG_INFINITY;
    let mut any_finite = false;
    for c in &corners {
        let vs = apply_col(&view, [c[0], c[1], c[2]]);
        let clip = apply_col(&proj, [vs[0], vs[1], vs[2]]);
        let w_clip = clip[3];
        let w_div = if w_clip.abs() < 1e-9 { 1.0 } else { w_clip };
        let nx = clip[0] / w_div;
        let ny = clip[1] / w_div;
        if !nx.is_finite() || !ny.is_finite() {
            continue;
        }
        any_finite = true;
        let px = (nx * 0.5 + 0.5) * w_f;
        let py = (1.0 - (ny * 0.5 + 0.5)) * h_f;
        px_min = px.min(px_min);
        py_min = py.min(py_min);
        px_max = px.max(px_max);
        py_max = py.max(py_max);
    }
    if !any_finite {
        let cx = (w_f / 2.0).clamp(0.0, w_f) as f32;
        let cy = (h_f / 2.0).clamp(0.0, h_f) as f32;
        return [cx, cy, cx, cy];
    }
    [px_min.clamp(0.0, w_f) as f32, py_min.clamp(0.0, h_f) as f32, px_max.clamp(0.0, w_f) as f32, py_max.clamp(0.0, h_f) as f32]
}

/// Multiply a `Mat4` by a 3D point (implicitly homogenized to w=1).
///
/// `out[i] = m[i][0]*p[0] + m[i][1]*p[1] + m[i][2]*p[2] + m[i][3]`
fn apply_col(m: &Mat4, p: [f64; 3]) -> [f64; 4] {
    let mut out = [0.0f64; 4];
    for i in 0..4 {
        out[i] = m.m[i][0] * p[0] + m.m[i][1] * p[1] + m.m[i][2] * p[2] + m.m[i][3];
    }
    out
}

/// Combined view·projection in WGSL `mat4x4<f32>` layout.
///
/// `out[c][r]` is column `c`, row `r`. `Orbit::view`/`Orbit::proj` store matrices in
/// the *transposed* (apply-column) convention — see `apply_col` in this file — so the
/// product `proj.mul(&view)` must be transposed before it is uploaded as a WGSL
/// `mat4x4<f32>` uniform (`u.view_proj * vec4(pos, 1)` in `shaders/mesh.wgsl`).
///
/// With `Orbit::proj`'s z mapping, NDC z is monotonically increasing with distance
/// from the eye and lies in [0, 1] for distances ≳ 0.2 m, which is why the pipeline
/// uses `CompareFunction::Less` (nearer = smaller depth wins). Never returns NaN: the
/// inputs are already finite (`Orbit::view` guards the degenerate up-vector,
/// `Orbit::proj` guards the aspect).
pub fn view_proj(camera: &Camera, aspect: f64) -> [[f32; 4]; 4] {
    let Camera::Orbit(o) = camera;
    let m = o.proj(aspect).mul(&o.view());
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = m.m[r][c] as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_camera_fits_the_scene_bounds() {
        let bounds = Bounds3 { min: [-10.0, -10.0, 0.0], max: [10.0, 10.0, 4.0] };
        let o = Orbit::fit(&bounds);
        assert!((o.target.x - 0.0).abs() < 1e-9);
        assert!((o.target.y - 0.0).abs() < 1e-9);
        assert!((o.target.z - 2.0).abs() < 1e-9);
        let eye = o.eye();
        assert!((eye.x - o.target.x).abs() > 1e-9);
        assert!((eye.y - o.target.y).abs() > 1e-9);
        assert!((eye.z - o.target.z).abs() > 1e-9);
        let target_len = (o.target.x * o.target.x + o.target.y * o.target.y + o.target.z * o.target.z).sqrt();
        assert!(o.distance > target_len - 0.1);
    }

    #[test]
    fn orbit_clamps_elevation_and_distance() {
        let o = Orbit::new(Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0, 10.0, 45.0);
        let r = o.orbit(0.0, 200.0, 1.0);
        assert!((r.elevation - 89.0).abs() < 1e-9);
        let r = o.orbit(0.0, -200.0, 1.0);
        assert!((r.elevation + 89.0).abs() < 1e-9);
        let r = o.orbit(0.0, 0.0, 0.0001);
        assert!((r.distance - MIN_DISTANCE).abs() < 1e-9);
        let r = o.orbit(0.0, 0.0, 1e12);
        assert!((r.distance - MAX_DISTANCE).abs() < 1e-6);
        assert!(r.fov_deg == o.fov_deg);
    }

    #[test]
    fn view_and_proj_are_finite_for_all_angles() {
        let mut bad = false;
        for az in -180..=180 {
            for el in -89..=89 {
                let o = Orbit::new(Vec3::new(0.0, 0.0, 0.0), az as f64, el as f64, 10.0, 45.0);
                let v = o.view();
                let p = o.proj(1.5);
                for x in v.m.iter() {
                    if !x.iter().all(|v| v.is_finite()) {
                        bad = true;
                    }
                }
                for x in p.m.iter() {
                    if !x.iter().all(|v| v.is_finite()) {
                        bad = true;
                    }
                }
            }
        }
        assert!(!bad, "non-finite matrix entry");
    }

    #[test]
    fn projected_bounds_matches_the_camera() {
        let cam = Camera::Orbit(Orbit::new(Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0, 10.0, 45.0));
        let size = (64u32, 64u32);
        let tiny = Bounds3 { min: [-0.01, -0.01, -0.01], max: [0.01, 0.01, 0.01] };
        let b = projected_bounds(&cam, &tiny, size);
        // Centre of the projected box is at the pixel centre.
        let cx = (b[0] + b[2]) / 2.0;
        let cy = (b[1] + b[3]) / 2.0;
        assert!((cx - 32.0).abs() < 1e-3, "centre x {} expected 32", cx);
        assert!((cy - 32.0).abs() < 1e-3, "centre y {} expected 32", cy);
        // Extents are inside the viewport.
        assert!(b[0] >= 0.0 && b[2] <= 64.0);
        assert!(b[1] >= 0.0 && b[3] <= 64.0);
        // Doubling the box strictly grows the projected area.
        let doubled = Bounds3 { min: [-0.1, -0.1, -0.1], max: [0.1, 0.1, 0.1] };
        let b2 = projected_bounds(&cam, &doubled, size);
        let tiny_area = (b[2] - b[0]).abs() * (b[3] - b[1]).abs();
        let double_area = (b2[2] - b2[0]).abs() * (b2[3] - b2[1]).abs();
        assert!(double_area > tiny_area, "doubled box (area {}) should be larger than tiny box (area {})", double_area, tiny_area);
    }

    #[test]
    fn projected_bounds_is_finite_for_degenerate_input() {
        let cam_min = Camera::Orbit(Orbit::new(Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0, MIN_DISTANCE, 45.0));
        let b1 = projected_bounds(&cam_min, &Bounds3::empty(), (64u32, 64u32));
        for &v in b1.iter() {
            assert!(v.is_finite(), "MIN_DISTANCE empty bounds: {}", v);
        }
        let cam = Camera::Orbit(Orbit::new(Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0, 10.0, 45.0));
        let b2 = projected_bounds(&cam, &Bounds3::empty(), (64u32, 64u32));
        for &v in b2.iter() {
            assert!(v.is_finite(), "normal distance empty bounds: {}", v);
        }
    }

    #[test]
    fn view_proj_maps_the_target_to_the_screen_centre() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        let vp = view_proj(&cam, 1.0);
        let clip = clip(&vp, [0.0, 0.0, 0.0]);
        assert!((clip[0] / clip[3]).abs() < 1e-5, "clip.x/clip.w = {}", clip[0] / clip[3]);
        assert!((clip[1] / clip[3]).abs() < 1e-5, "clip.y/clip.w = {}", clip[1] / clip[3]);
        let z = clip[2] / clip[3];
        assert!((z > 0.5 && z < 1.0), "clip.z/clip.w = {} not in (0.5, 1.0)", z);
    }

    #[test]
    fn view_proj_depth_grows_with_distance() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        let vp = view_proj(&cam, 1.0);
        let near = clip(&vp, [0.0, 5.0, 0.0]);
        let far = clip(&vp, [0.0, -3.0, 0.0]);
        let near_z = near[2] / near[3];
        let far_z = far[2] / far[3];
        assert!(far_z > near_z, "far z ({}) should be > near z ({})", far_z, near_z);
        assert!(near_z >= 0.0 && near_z <= 1.0, "near z ({}) not in [0, 1]", near_z);
        assert!(far_z >= 0.0 && far_z <= 1.0, "far z ({}) not in [0, 1]", far_z);
    }

    /// Multiply a WGSL `mat4x4<f32>` view·projection by a 3D point.
    ///
    /// `out[i] = vp[0][i]*p0 + vp[1][i]*p1 + vp[2][i]*p2 + vp[3][i]` — the
    /// WGSL `M * vec4(p, 1)` convention.
    fn clip(vp: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 4] {
        let mut out = [0.0f32; 4];
        for i in 0..4 {
            out[i] = vp[0][i] * p[0] + vp[1][i] * p[1] + vp[2][i] * p[2] + vp[3][i];
        }
        out
    }
}
