/// Directional light + ambient, matching WGSL `Uniforms` (shaders/mesh.wgsl).
///
/// Frame: +X east, +Y north, +Z up (as `camera::Orbit::eye`). `dir` points FROM the
/// surface TOWARD the light (shader does `dot(n, dir)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub dir: [f32; 3],
    pub ambient: f32,
}

impl Light {
    /// Sun south-west, 45 deg up, ambient 0.25.
    pub const DEFAULT: Light = Light { dir: [-0.5, -0.5, 0.70710678], ambient: 0.25 };

    /// Unit dir (zero/non-finite -> [0,0,1]); ambient clamped 0..=1 (NaN -> 0).
    pub fn normalized(&self) -> Light {
        let mut d = self.dir;
        for c in d.iter_mut() {
            if !c.is_finite() {
                *c = 0.0;
            }
        }
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let dir = if len.is_finite() && len > 0.0 { [d[0] / len, d[1] / len, d[2] / len] } else { [0.0, 0.0, 1.0] };
        let ambient = if self.ambient.is_finite() { self.ambient.clamp(0.0, 1.0) } else { 0.0 };
        Light { dir, ambient }
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_default_is_south_west_up_and_normalised() {
        let l = Light::default();
        assert_eq!(l, Light::DEFAULT);
        let n = l.normalized();
        let len = (n.dir[0] * n.dir[0] + n.dir[1] * n.dir[1] + n.dir[2] * n.dir[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
        assert!(n.dir[0] < 0.0);
        assert!(n.dir[1] < 0.0);
        assert!(n.dir[2] > 0.0);
        assert_eq!(n.ambient, 0.25);
        assert!((n.dir[0] - Light::DEFAULT.dir[0]).abs() < 1e-6);
        assert!((n.dir[1] - Light::DEFAULT.dir[1]).abs() < 1e-6);
        assert!((n.dir[2] - Light::DEFAULT.dir[2]).abs() < 1e-6);
    }

    #[test]
    fn light_normalized_is_unit_length_and_zero_or_nan_safe() {
        let zero = Light { dir: [0.0; 3], ambient: 0.5 };
        assert_eq!(zero.normalized().dir, [0.0, 0.0, 1.0]);

        let nan = Light { dir: [f32::NAN, f32::NAN, f32::NAN], ambient: 0.5 };
        assert_eq!(nan.normalized().dir, [0.0, 0.0, 1.0]);

        let inf = Light { dir: [f32::INFINITY, 0.0, 0.0], ambient: 0.5 };
        assert_eq!(inf.normalized().dir, [0.0, 0.0, 1.0]);

        let v = Light { dir: [3.0, 4.0, 0.0], ambient: 0.5 };
        let n = v.normalized();
        let len = (n.dir[0] * n.dir[0] + n.dir[1] * n.dir[1] + n.dir[2] * n.dir[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
    }

    #[test]
    fn light_ambient_is_clamped_to_zero_one() {
        let low = Light { dir: [0.0, 0.0, 1.0], ambient: -0.5 };
        assert_eq!(low.normalized().ambient, 0.0);

        let high = Light { dir: [0.0, 0.0, 1.0], ambient: 1.5 };
        assert_eq!(high.normalized().ambient, 1.0);

        let nan = Light { dir: [0.0, 0.0, 1.0], ambient: f32::NAN };
        assert_eq!(nan.normalized().ambient, 0.0);

        let ok = Light { dir: [0.0, 0.0, 1.0], ambient: 0.7 };
        assert_eq!(ok.normalized().ambient, 0.7);
    }
}
