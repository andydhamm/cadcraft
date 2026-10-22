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
        let fw = normalize(Vec3::new(self.target.x - eye.x, self.target.y - eye.y, self.target.z - eye.z));
        let up_w = Vec3::new(0.0, 0.0, 1.0);
        // If we look straight up/down, the world-up is degenerate; fall back to +X.
        let mut up = up_w;
        if cross(fw, up).len_sq() < 1e-12 {
            up = Vec3::new(1.0, 0.0, 0.0);
        }
        let right = normalize(cross(fw, up));
        let new_up = cross(right, fw);
        // Column-major 4x4 (matches a WGSL `matrix<float, 4x4>` uniform).
        Mat4::from_columns([
            Vec4::new(right.x, right.y, right.z, 0.0),
            Vec4::new(new_up.x, new_up.y, new_up.z, 0.0),
            Vec4::new(-fw.x, -fw.y, -fw.z, 0.0),
            Vec4::new(-dot(right, eye), -dot(new_up, eye), dot(fw, eye), 1.0),
        ])
    }

    /// Perspective projection matrix for `aspect` (width/height).
    pub fn proj(&self, aspect: f64) -> Mat4 {
        let half = self.fov_deg.to_radians() / 2.0;
        let tan_half = half.tan();
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        let z_near = 0.1;
        let z_far = 10_000.0;
        Mat4::from_columns([
            Vec4::new(1.0 / (aspect * tan_half), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0 / tan_half, 0.0, 0.0),
            Vec4::new((z_far + z_near) / (z_near - z_far), (2.0 * z_near * z_far) / (z_near - z_far), -1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
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
        let dist = (half_diag * std::sqrt(3.0) / s).max(MIN_DISTANCE);
        Orbit::new(c, 30.0, 30.0, dist, fov)
    }

    /// 4-column column-major matrix built from 4 column vectors.
    #[inline]
    fn from_columns(cols: [Vec4; 4]) -> Mat4 {
        let m = [0.0_f64; 16];
        for c in 0..4 {
            for r in 0..4 {
                m[c * 4 + r] = cols[c].y[r];
            }
        }
        Mat4 { m }
    }
}

/// 2D column vector.
pub struct Vec2 {
    /// x
    pub x: f64,
    /// y
    pub y: f64,
}

impl Vec2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// 3D column vector.
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// 4D column vector.
pub struct Vec4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Vec4 {
    pub fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }
    fn y(&self) -> Vec4 {
        Self::new(self.x, self.y, self.z, self.w)
    }
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn normalize(v: Vec3) -> Vec3 {
    let l2 = v.x * v.x + v.y * v.y + v.z * v.z;
    if l2 <= 1e-12 {
        return Vec3::new(0.0, 0.0, 0.0);
    }
    let inv = 1.0 / l2.sqrt();
    Vec3::new(v.x * inv, v.y * inv, v.z * inv)
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
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

// NOTE: Vec2/Vec3/Vec4/Mat4 above are stand-ins so this file compiles; the real
// `cadcraft_geom::{Vec2, Vec3, Mat4}` from the workspace is used at the L5 boundary.

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
        assert!(o.distance > o.target.length() - 0.1);
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
                for x in v.iter().flatten() {
                    if !x.is_finite() {
                        bad = true;
                    }
                }
                for x in p.iter().flatten() {
                    if !x.is_finite() {
                        bad = true;
                    }
                }
            }
        }
        assert!(!bad, "non-finite matrix entry");
    }
}
