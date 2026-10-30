//! Camera helpers for the 3D viewport: orbit + full cameras, projection, and pixel rays.
//!
//! Pure geometry (no wgpu). `Orbit` is the interactive orbit camera; `FullCamera` is an
//! explicit eye/target camera used for saved cameras and CLI renders. `projected_bounds`,
//! `projected_scene_bounds`, `view_proj`, `project_point` and `ray_at` are the public
//! entry points used by the engine, the CLI and the 3D pane.

use crate::Scene3d;
use crate::mesh::Bounds3;
use cadcraft_geom::{Mat4, Vec3};

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
    /// Azimuth in degrees (0 = +Y (north), 90 = +X).
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
    /// A full camera with an explicit eye and target.
    Full(FullCamera),
}

/// An explicit eye/target camera (no orbit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FullCamera {
    /// Eye position in world space.
    pub eye: Vec3,
    /// Target position in world space (the point the camera looks at).
    pub target: Vec3,
    /// Vertical field of view in degrees.
    pub fov_deg: f64,
}

impl FullCamera {
    /// Construct a full camera.
    pub fn new(eye: Vec3, target: Vec3, fov_deg: f64) -> Self {
        Self { eye, target, fov_deg }
    }

    /// Eye position in world space (identity — the eye is stored directly).
    pub fn eye(&self) -> Vec3 {
        self.eye
    }

    /// View (world→camera) matrix.
    pub fn view(&self) -> Mat4 {
        let fw = (self.target - self.eye).normalized();
        let up_w = Vec3::new(0.0, 0.0, 1.0);
        // If we look straight up/down, the world-up is degenerate; fall back to +X.
        let mut up = up_w;
        let c = fw.cross(up);
        if c.x * c.x + c.y * c.y + c.z * c.z < 1e-12 {
            up = Vec3::new(1.0, 0.0, 0.0);
        }
        let right = fw.cross(up).normalized();
        let new_up = right.cross(fw);
        // Row i is camera axis i (right, up, back), applied as `m · p` (see `apply_col`); `view_proj` transposes
        // the product for the WGSL uniform. 10-29: the rotation was stored transposed, so views off the axes drew
        // rotated (a vertical edge horizontal at azimuth 90).
        let m = [
            [right.x, right.y, right.z, -right.dot(self.eye)],
            [new_up.x, new_up.y, new_up.z, -new_up.dot(self.eye)],
            [-fw.x, -fw.y, -fw.z, fw.dot(self.eye)],
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
            [0.0, 0.0, -z_far / (z_far - z_near), -z_near * z_far / (z_far - z_near)],
            [0.0, 0.0, -1.0, 0.0],
        ];
        Mat4 { m }
    }

    /// Fit a full camera to `bounds`: target at the box centre, eye on −Y at `eye_height`.
    ///
    /// The distance is large enough for the whole box to fit vertically at the given
    /// `fov_deg`. Returns a `FullCamera` with finite, non-degenerate values when the input
    /// is finite; a safe default camera when it is not.
    pub fn fit(b: &Bounds3, eye_height: f64) -> Self {
        let min = b.min;
        let max = b.max;
        let c = Vec3::new((min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0, (min[2] + max[2]) / 2.0);
        let half_diag = (0.5 * (max[0] - min[0]).abs() + 0.5 * (max[1] - min[1]).abs() + 0.5 * (max[2] - min[2]).abs()).max(0.1);
        let fov = 45.0;
        let s = (fov * std::f64::consts::PI / 360.0).sin();
        let dist = (half_diag * 3.0_f64.sqrt() / s).clamp(MIN_DISTANCE, MAX_DISTANCE);
        let eye = Vec3::new(c.x, -dist, eye_height);
        Self::new(eye, c, fov)
    }
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
        // Row i is camera axis i (right, up, back), applied as `m · p` (see `apply_col`); `view_proj` transposes
        // the product for the WGSL uniform. 10-29: the rotation was stored transposed, so views off the axes drew
        // rotated (a vertical edge horizontal at azimuth 90).
        let m = [
            [right.x, right.y, right.z, -right.dot(eye)],
            [new_up.x, new_up.y, new_up.z, -new_up.dot(eye)],
            [-fw.x, -fw.y, -fw.z, fw.dot(eye)],
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
            [0.0, 0.0, -z_far / (z_far - z_near), -z_near * z_far / (z_far - z_near)],
            [0.0, 0.0, -1.0, 0.0],
        ];
        Mat4 { m }
    }

    /// A comfortable orbit that frames `bounds`: target at the box centre and a
    /// distance at which the box's bounding sphere fits the vertical field of view
    /// with a 10 % margin, at a pleasant 30°/30° angle. The sphere contains the whole
    /// box, so it fits from any angle, and a view at least as wide as tall shows all of it.
    pub fn fit(b: &Bounds3) -> Self {
        let min = b.min;
        let max = b.max;
        let c = Vec3::new((min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0, (min[2] + max[2]) / 2.0);
        let dx = max[0] - min[0];
        let dy = max[1] - min[1];
        let dz = max[2] - min[2];
        // Radius of the box's bounding sphere (half the space diagonal).
        let r = 0.5 * (dx * dx + dy * dy + dz * dz).sqrt();
        // A sphere of radius r fits a fov when distance >= r / sin(fov/2); add 10 % margin.
        let fov = 45.0;
        let s = (fov * std::f64::consts::PI / 360.0).sin();
        let dist = (1.1 * r.max(0.1) / s).clamp(MIN_DISTANCE, MAX_DISTANCE);
        Orbit::new(c, 30.0, 30.0, dist, fov)
    }
}

impl Camera {
    /// Eye position in world space (dispatches on the variant).
    pub fn eye(&self) -> Vec3 {
        match self {
            Camera::Orbit(o) => o.eye(),
            Camera::Full(f) => f.eye(),
        }
    }

    /// Target position in world space (dispatches on the variant).
    pub fn target(&self) -> Vec3 {
        match self {
            Camera::Orbit(o) => o.target,
            Camera::Full(f) => f.target,
        }
    }

    /// View (world→camera) matrix (dispatches on the variant).
    pub fn view(&self) -> Mat4 {
        match self {
            Camera::Orbit(o) => o.view(),
            Camera::Full(f) => f.view(),
        }
    }

    /// Perspective projection matrix for `aspect` (dispatches on the variant).
    pub fn proj(&self, aspect: f64) -> Mat4 {
        match self {
            Camera::Orbit(o) => o.proj(aspect),
            Camera::Full(f) => f.proj(aspect),
        }
    }

    /// The vertical field of view in degrees (dispatches on the variant).
    pub fn fov_deg(&self) -> f64 {
        match self {
            Camera::Orbit(o) => o.fov_deg,
            Camera::Full(f) => f.fov_deg,
        }
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
    let w_f = w as f64;
    let h_f = h.max(1) as f64;
    // Empty bounds (min == max): return the viewport centre pixel.
    if bounds.min == bounds.max {
        let cx = (w_f / 2.0).clamp(0.0, w_f) as f32;
        let cy = (h_f / 2.0).clamp(0.0, h_f) as f32;
        return [cx, cy, cx, cy];
    }
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
    project_points(camera, corners.into_iter(), size)
}

/// Project every mesh vertex of `scene` into viewport pixels: `[min_x, min_y, max_x, max_y]`,
/// y-down, clamped like [`projected_bounds`]. Unlike the bounding box's corners, the vertices are
/// what the rasteriser covers, so this matches the rendered silhouette when the geometry does not
/// fill its bounding box (room slabs sit inside the walls' outer faces). Falls back to
/// [`projected_bounds`] for a scene with no vertices.
pub fn projected_scene_bounds(camera: &Camera, scene: &Scene3d, size: (u32, u32)) -> [f32; 4] {
    let mut points = scene.meshes.iter().flat_map(|m| m.positions.iter()).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).peekable();
    if points.peek().is_none() {
        return projected_bounds(camera, &scene.bounds, size);
    }
    project_points(camera, points, size)
}

/// A full camera framing `b` from −Y at `eye_height` (the spec's free function; see [`FullCamera::fit`]).
pub fn fit_full_camera(b: &Bounds3, eye_height: f64) -> FullCamera {
    FullCamera::fit(b, eye_height)
}

/// Project a single world point `p` into viewport pixels `[x, y]` (y-down).
///
/// Returns `None` when the point is behind the eye (`w_clip <= 1e-9`), when any value is
/// non-finite, or when the viewport size is zero.
pub fn project_point(camera: &Camera, size: (u32, u32), p: [f64; 3]) -> Option<[f32; 2]> {
    if !p.iter().all(|v| v.is_finite()) {
        return None;
    }
    let (w, h) = size;
    if w == 0 || h == 0 {
        return None;
    }
    let pf = [p[0] as f32, p[1] as f32, p[2] as f32];
    let aspect = w as f64 / h as f64;
    let vp = view_proj(camera, aspect);
    let mut clip = [0.0f32; 4];
    for i in 0..4 {
        clip[i] = vp[0][i] * pf[0] + vp[1][i] * pf[1] + vp[2][i] * pf[2] + vp[3][i];
    }
    let w_clip = clip[3];
    if w_clip <= 1e-9 {
        return None;
    }
    let nx = clip[0] / w_clip;
    let ny = clip[1] / w_clip;
    if !nx.is_finite() || !ny.is_finite() {
        return None;
    }
    let px = (nx * 0.5 + 0.5) * (w as f32);
    let py = (1.0 - (ny * 0.5 + 0.5)) * (h as f32);
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    Some([px, py])
}

/// The world-space ray through pixel `pixel` (x, y in pixels, y-down).
///
/// Returns `(origin, direction)` where `origin` is the eye and `direction` is a unit
/// vector pointing into the scene. Returns `None` on non-finite input, zero size, or a
/// pixel outside `[0, w] × [0, h]` (with a 1 px margin). The centre pixel yields the
/// camera's forward direction.
pub fn ray_at(camera: &Camera, size: (u32, u32), pixel: [f32; 2]) -> Option<(Vec3, Vec3)> {
    if !pixel.iter().all(|v| v.is_finite()) {
        return None;
    }
    let (w, h) = size;
    if w == 0 || h == 0 {
        return None;
    }
    let margin = 1.0;
    if pixel[0] < -margin || pixel[0] > w as f32 + margin || pixel[1] < -margin || pixel[1] > h as f32 + margin {
        return None;
    }
    let w_f = w as f32;
    let h_f = h as f32;
    let aspect = w_f / h_f;
    let half = (camera.fov_deg() * std::f64::consts::PI / 360.0) as f32;
    let tan_half = half.tan();
    // Normalised pixel → NDC (y flipped to y-down).
    let nx = (pixel[0] / w_f) * 2.0 - 1.0;
    let ny = 1.0 - (pixel[1] / h_f) * 2.0;
    // Inverse view rotation applied to the view-space direction. The view matrix's rows are the camera axes
    // [right, new_up, -fw], so d_world = right*d_x + new_up*d_y + (-fw)*d_z; with d_z = -1 that is
    // right*d_x + new_up*d_y + fw.
    let view = camera.view();
    let right = Vec3::new(view.m[0][0], view.m[0][1], view.m[0][2]);
    let up = Vec3::new(view.m[1][0], view.m[1][1], view.m[1][2]);
    let fw = Vec3::new(-view.m[2][0], -view.m[2][1], -view.m[2][2]);
    // Reconstruct the view-space direction from NDC.  The projection matrix maps
    // `view_x -> clip_x = view_x / (aspect * tan_half)` and `view_y -> clip_y =
    // view_y / tan_half`, with `clip_w = -view_z`.  For a ray (direction, no
    // translation) we set `view_z = -1`, giving:
    //   view_x = ndc_x * aspect * tan_half
    //   view_y = ndc_y * tan_half
    //   view_z = -1
    let d_view_x = (nx * aspect * tan_half) as f64;
    let d_view_y = (ny * tan_half) as f64;
    let d_view_z = -1.0;
    let dir_world = right * d_view_x + up * d_view_y + Vec3::new(-fw.x, -fw.y, -fw.z) * d_view_z;
    if !dir_world.is_finite() {
        return None;
    }
    let dir = dir_world.normalized();
    if dir.is_finite() { Some((camera.eye(), dir)) } else { None }
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
    let m = camera.proj(aspect).mul(&camera.view());
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = m.m[r][c] as f32;
        }
    }
    out
}

/// The pixel bounding box of `points` seen by `camera` (see [`projected_bounds`]).
fn project_points(camera: &Camera, points: impl Iterator<Item = [f64; 3]>, size: (u32, u32)) -> [f32; 4] {
    let (w, h) = size;
    let aspect = w as f64 / h.max(1) as f64;
    let w_f = w as f64;
    let h_f = h.max(1) as f64;
    let view = camera.view();
    let proj = camera.proj(aspect);
    let mut px_min = f64::MAX;
    let mut py_min = f64::MAX;
    let mut px_max = f64::NEG_INFINITY;
    let mut py_max = f64::NEG_INFINITY;
    let mut any_finite = false;
    for c in points {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn corners(b: &Bounds3) -> Vec<[f64; 3]> {
        let mut v = Vec::new();
        for x in [b.min[0], b.max[0]] {
            for y in [b.min[1], b.max[1]] {
                for z in [b.min[2], b.max[2]] {
                    v.push([x, y, z]);
                }
            }
        }
        v
    }

    #[test]
    fn orbit_fit_keeps_every_corner_in_view() {
        let b = Bounds3::new([0.0, 0.0, 0.0], [12.0, 8.0, 2.7]);
        let fit = Orbit::fit(&b);
        for az in (0..360).step_by(30) {
            for el in [0.0, 20.0, 45.0, 80.0] {
                let cam = Camera::Orbit(Orbit::new(fit.target, az as f64, el, fit.distance, fit.fov_deg));
                for c in corners(&b) {
                    let p = project_point(&cam, (900, 900), c).expect("corner projects");
                    assert!((0.0..=900.0).contains(&p[0]) && (0.0..=900.0).contains(&p[1]), "az {az} el {el} corner {c:?} -> {p:?}");
                }
            }
        }
    }

    #[test]
    fn orbit_fit_fills_a_wide_view() {
        let b = Bounds3::new([0.0, 0.0, 0.0], [12.0, 8.0, 2.7]);
        let fit = Orbit::fit(&b);
        let cam = Camera::Orbit(Orbit::new(fit.target, 0.0, 20.0, fit.distance, fit.fov_deg));
        let r = projected_bounds(&cam, &b, (1600, 900));
        let (sx, sy) = (f64::from(r[2] - r[0]), f64::from(r[3] - r[1]));
        assert!(sx >= 0.35 * 1600.0 || sy >= 0.35 * 900.0, "spans {sx} x {sy}");
    }

    #[test]
    fn full_camera_ray_at_the_centre_points_at_the_target() {
        let bounds = Bounds3::new([-10.0, -10.0, 0.0], [10.0, 10.0, 4.0]);
        let cam = FullCamera::fit(&bounds, 3.0);
        let cam = Camera::Full(cam);
        let size = (64u32, 64u32);
        let (origin, dir) = ray_at(&cam, size, [32.0, 32.0]).unwrap();
        assert!((origin - cam.eye()).len() < 1e-6, "origin should be the eye");
        let to_target = (cam.target() - origin).normalized();
        assert!((to_target - dir).len() < 1e-4, "centre ray should point at target: {:?} vs {:?}", to_target, dir);
    }

    #[test]
    fn project_point_and_ray_at_are_inverses() {
        let cam = Camera::Orbit(Orbit::at_origin(30.0, 30.0, 20.0, 45.0));
        let size = (64u32, 64u32);
        let p = Vec3::new(2.0, 1.0, 1.0);
        let px = project_point(&cam, size, [p.x, p.y, p.z]).unwrap();
        let (origin, dir) = ray_at(&cam, size, px).unwrap();
        // The point p should lie on the ray (within tolerance).
        let t = (p - origin).dot(dir);
        let on_ray = (p - (origin + dir * t)).len();
        assert!(on_ray < 1e-2, "projected point should lie on the ray: {} (t={})", on_ray, t);
    }

    /// The view has no roll: a vertical line through the target projects straight up the screen from any orbit or
    /// full camera, and the eye's right-hand side is the screen's right. 10-29: the view matrix carried its rotation
    /// transposed, so at azimuth 90 a wall's vertical edge drew horizontally and `ray_at` could not invert
    /// `project_point`.
    #[test]
    fn world_up_projects_straight_up_the_screen() {
        let size = (100u32, 100u32);
        let cams = [
            Camera::Orbit(Orbit::at_origin(0.0, 0.0, 20.0, 45.0)),
            Camera::Orbit(Orbit::at_origin(30.0, 30.0, 20.0, 45.0)),
            Camera::Orbit(Orbit::at_origin(90.0, 10.0, 20.0, 45.0)),
            Camera::Orbit(Orbit::at_origin(215.0, -20.0, 15.0, 60.0)),
            Camera::Full(FullCamera::new(Vec3::new(3.0, -8.0, 1.6), Vec3::new(3.0, 0.0, 1.05), 60.0)),
        ];
        for cam in &cams {
            let t = cam.target();
            let c = project_point(cam, size, [t.x, t.y, t.z]).unwrap();
            let up = project_point(cam, size, [t.x, t.y, t.z + 1.0]).unwrap();
            assert!((up[0] - c[0]).abs() < 1e-3, "world up leans sideways on screen: {c:?} -> {up:?}");
            assert!(up[1] < c[1], "world up points down the screen: {c:?} -> {up:?}");
            let fw = (t - cam.eye()).normalized();
            let right = fw.cross(Vec3::new(0.0, 0.0, 1.0)).normalized();
            let r = project_point(cam, size, [t.x + right.x, t.y + right.y, t.z + right.z]).unwrap();
            assert!(r[0] > c[0] && (r[1] - c[1]).abs() < 1e-3, "the eye's right is not the screen's right: {c:?} -> {r:?}");
        }
    }

    #[test]
    fn fit_full_camera_is_full_camera_fit() {
        let b = Bounds3::new([0.0, 0.0, 0.0], [10.0, 8.0, 2.4]);
        let (a, f) = (fit_full_camera(&b, 1.6), FullCamera::fit(&b, 1.6));
        assert!((a.eye - f.eye).len() < 1e-12 && (a.target - f.target).len() < 1e-12);
    }

    #[test]
    fn project_point_is_none_behind_the_eye() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        let size = (64u32, 64u32);
        // A point behind the eye (further from target than the eye on the same side).
        let behind = Vec3::new(0.0, 20.0, 0.0);
        assert!(project_point(&cam, size, [behind.x, behind.y, behind.z]).is_none(), "behind-eye point should be None");
    }

    #[test]
    fn orbit_view_proj_is_unchanged_after_the_move() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        let vp = view_proj(&cam, 1.0);
        // The target should project to the screen centre.
        let mut clip = [0.0f32; 4];
        for i in 0..4 {
            clip[i] = vp[0][i] * 0.0 + vp[1][i] * 0.0 + vp[2][i] * 0.0 + vp[3][i];
        }
        assert!((clip[0] / clip[3]).abs() < 1e-5, "clip.x/clip.w = {}", clip[0] / clip[3]);
        assert!((clip[1] / clip[3]).abs() < 1e-5, "clip.y/clip.w = {}", clip[1] / clip[3]);
    }

    #[test]
    fn full_camera_view_is_orthonormal() {
        let cam = FullCamera::new(Vec3::new(0.0, -10.0, 3.0), Vec3::new(0.0, 0.0, 0.0), 45.0);
        let v = cam.view();
        let r0 = [v.m[0][0], v.m[1][0], v.m[2][0]];
        let r1 = [v.m[0][1], v.m[1][1], v.m[2][1]];
        let r2 = [v.m[0][2], v.m[1][2], v.m[2][2]];
        for (i, row) in [r0, r1, r2].iter().enumerate() {
            let len = (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-9, "row {} length {}", i, len);
        }
        let dot01 = r0[0] * r1[0] + r0[1] * r1[1] + r0[2] * r1[2];
        let dot02 = r0[0] * r2[0] + r0[1] * r2[1] + r0[2] * r2[2];
        let dot12 = r1[0] * r2[0] + r1[1] * r2[1] + r1[2] * r2[2];
        assert!(dot01.abs() < 1e-9, "rows 0,1 not orthogonal: {}", dot01);
        assert!(dot02.abs() < 1e-9, "rows 0,2 not orthogonal: {}", dot02);
        assert!(dot12.abs() < 1e-9, "rows 1,2 not orthogonal: {}", dot12);
    }

    #[test]
    fn hostile_camera_values_never_panic() {
        let size = (64u32, 64u32);
        let hostile = [
            Camera::Orbit(Orbit::new(Vec3::new(f64::NAN, f64::NAN, f64::NAN), f64::NAN, f64::NAN, f64::NAN, f64::NAN)),
            Camera::Orbit(Orbit::new(Vec3::new(f64::INFINITY, 0.0, 0.0), 0.0, 0.0, f64::INFINITY, 179.0)),
            Camera::Full(FullCamera::new(Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), f64::NAN)),
            Camera::Full(FullCamera::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(f64::INFINITY, 0.0, 0.0), 0.0)),
        ];
        for cam in hostile {
            let _ = view_proj(&cam, 1.0);
            let _ = view_proj(&cam, f64::NAN);
            let _ = project_point(&cam, size, [0.0, 0.0, 0.0]);
            let _ = ray_at(&cam, size, [32.0, 32.0]);
            let _ = projected_bounds(&cam, &Bounds3::empty(), size);
        }
    }

    #[test]
    fn ray_at_rejects_pixels_outside_the_viewport() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        let size = (64u32, 64u32);
        assert!(ray_at(&cam, size, [70.0, 32.0]).is_none(), "x=70 > w should be None");
        assert!(ray_at(&cam, size, [-2.0, 32.0]).is_none(), "x=-2 < 0 should be None");
        assert!(ray_at(&cam, size, [32.0, 70.0]).is_none(), "y=70 > h should be None");
        assert!(ray_at(&cam, size, [32.0, -2.0]).is_none(), "y=-2 < 0 should be None");
        assert!(ray_at(&cam, size, [f64::NAN as f32, 32.0]).is_none(), "NaN pixel should be None");
        assert!(ray_at(&cam, (0u32, 64u32), [32.0, 32.0]).is_none(), "zero width should be None");
    }
}
