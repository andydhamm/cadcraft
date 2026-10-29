//! The camera lives in `cadcraft_arch3d::camera` (no wgpu), so the engine can project and pick without render3d
//! (p6-arch3d-camera); it is re-exported here under its old path, and its tests stay here under their old names.
pub use cadcraft_arch3d::camera::*;

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_arch3d::mesh::Bounds3;
    use cadcraft_geom::Vec3;

    #[test]
    fn orbit_camera_fits_the_scene_bounds() {
        let bounds = Bounds3::new([-10.0, -10.0, 0.0], [10.0, 10.0, 4.0]);
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
    fn orbit_azimuth_zero_looks_from_plus_y() {
        let eye = Orbit::at_origin(0.0, 0.0, 10.0, 45.0).eye();
        assert!((eye.x - 0.0).abs() < 1e-9, "azimuth 0 eye.x = {}", eye.x);
        assert!((eye.y - 10.0).abs() < 1e-9, "azimuth 0 eye.y = {}", eye.y);
        assert!((eye.z - 0.0).abs() < 1e-9, "azimuth 0 eye.z = {}", eye.z);
        let eye90 = Orbit::at_origin(90.0, 0.0, 10.0, 45.0).eye();
        assert!((eye90.x - 10.0).abs() < 1e-9, "azimuth 90 eye.x = {}", eye90.x);
        assert!((eye90.y - 0.0).abs() < 1e-9, "azimuth 90 eye.y = {}", eye90.y);
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
        let tiny = Bounds3::new([-0.01, -0.01, -0.01], [0.01, 0.01, 0.01]);
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
        let doubled = Bounds3::new([-0.1, -0.1, -0.1], [0.1, 0.1, 0.1]);
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

    #[test]
    fn view_proj_target_centre_at_various_angles() {
        let angles = [(0.0, 0.0), (45.0, 30.0), (90.0, -45.0), (180.0, 60.0), (-90.0, -89.0)];
        for (az, el) in angles {
            let cam = Camera::Orbit(Orbit::at_origin(az, el, 10.0, 45.0));
            let vp = view_proj(&cam, 1.0);
            let clip = clip(&vp, [0.0, 0.0, 0.0]);
            let x = clip[0] / clip[3];
            let y = clip[1] / clip[3];
            assert!(x.is_finite() && y.is_finite(), "az={} el={} non-finite clip", az, el);
            assert!(x.abs() < 1e-4, "az={} el={} clip.x/clip.w = {}", az, el, x);
            assert!(y.abs() < 1e-4, "az={} el={} clip.y/clip.w = {}", az, el, y);
        }
    }

    #[test]
    fn view_proj_finite_for_degenerate_aspect() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        for aspect in [0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let vp = view_proj(&cam, aspect);
            for col in &vp {
                for &v in col {
                    assert!(v.is_finite(), "aspect={} produced non-finite {}", aspect, v);
                }
            }
        }
    }

    #[test]
    fn view_proj_all_entries_finite_for_extreme_camera() {
        let cam = Camera::Orbit(Orbit::at_origin(359.9, 89.0, 1_000_000.0, 179.0));
        let vp = view_proj(&cam, 3.0);
        for col in &vp {
            for &v in col {
                assert!(v.is_finite(), "non-finite entry {} for extreme camera", v);
            }
        }
        let clip = clip(&vp, [0.0, 0.0, 0.0]);
        assert!((clip[0] / clip[3]).abs() < 1e-3, "centre x off");
        assert!((clip[1] / clip[3]).abs() < 1e-3, "centre y off");
    }

    #[test]
    fn orbit_azimuth_wraps_and_negative() {
        // 360° is equivalent to 0°
        let eye0 = Orbit::at_origin(0.0, 0.0, 10.0, 45.0).eye();
        let eye360 = Orbit::at_origin(360.0, 0.0, 10.0, 45.0).eye();
        assert!((eye0.x - eye360.x).abs() < 1e-9, "360° != 0° x");
        assert!((eye0.y - eye360.y).abs() < 1e-9, "360° != 0° y");
        assert!((eye0.z - eye360.z).abs() < 1e-9, "360° != 0° z");
        // 720° is also 0°
        let eye720 = Orbit::at_origin(720.0, 0.0, 10.0, 45.0).eye();
        assert!((eye0.x - eye720.x).abs() < 1e-9, "720° != 0° x");
        assert!((eye0.y - eye720.y).abs() < 1e-9, "720° != 0° y");
        // Negative azimuth: -90° == 270°
        let eye_neg90 = Orbit::at_origin(-90.0, 0.0, 10.0, 45.0).eye();
        let eye270 = Orbit::at_origin(270.0, 0.0, 10.0, 45.0).eye();
        assert!((eye_neg90.x - eye270.x).abs() < 1e-9, "-90° != 270° x");
        assert!((eye_neg90.y - eye270.y).abs() < 1e-9, "-90° != 270° y");
    }

    #[test]
    fn orbit_fit_degenerate_bounds() {
        // Empty bounds (min == max): target at that point, distance >= MIN
        let empty = Bounds3::new([5.0, 3.0, 1.0], [5.0, 3.0, 1.0]);
        let o = Orbit::fit(&empty);
        assert!((o.target.x - 5.0).abs() < 1e-9, "empty target x");
        assert!((o.target.y - 3.0).abs() < 1e-9, "empty target y");
        assert!((o.target.z - 1.0).abs() < 1e-9, "empty target z");
        assert!(o.distance >= MIN_DISTANCE, "empty bounds distance too small");
        // Flat bounds (zero height)
        let flat = Bounds3::new([0.0, 0.0, 0.0], [10.0, 5.0, 0.0]);
        let o2 = Orbit::fit(&flat);
        assert!(o2.distance >= MIN_DISTANCE, "flat bounds distance too small");
        assert!(o2.distance.is_finite(), "flat bounds distance not finite");
        // Very large bounds
        let huge = Bounds3::new([-1e6, -1e6, -1e6], [1e6, 1e6, 1e6]);
        let o3 = Orbit::fit(&huge);
        assert!(o3.distance <= MAX_DISTANCE, "huge bounds distance exceeds MAX");
        assert!(o3.distance > 0.0, "huge bounds distance is zero");
    }

    #[test]
    fn orbit_view_matrix_is_orthogonal() {
        // The 3x3 rotation part of the view matrix must be orthonormal
        let o = Orbit::new(Vec3::new(1.0, 2.0, 3.0), 45.0, 30.0, 10.0, 45.0);
        let v = o.view();
        // Rows of the rotation part (m[i][0..3] for i=0,1,2)
        let r0 = [v.m[0][0], v.m[1][0], v.m[2][0]];
        let r1 = [v.m[0][1], v.m[1][1], v.m[2][1]];
        let r2 = [v.m[0][2], v.m[1][2], v.m[2][2]];
        // Row lengths must be 1
        for (i, row) in [r0, r1, r2].iter().enumerate() {
            let len = (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-9, "row {} length {}", i, len);
        }
        // Rows must be mutually orthogonal
        let dot01 = r0[0] * r1[0] + r0[1] * r1[1] + r0[2] * r1[2];
        let dot02 = r0[0] * r2[0] + r0[1] * r2[1] + r0[2] * r2[2];
        let dot12 = r1[0] * r2[0] + r1[1] * r2[1] + r1[2] * r2[2];
        assert!(dot01.abs() < 1e-9, "rows 0,1 not orthogonal: {}", dot01);
        assert!(dot02.abs() < 1e-9, "rows 0,2 not orthogonal: {}", dot02);
        assert!(dot12.abs() < 1e-9, "rows 1,2 not orthogonal: {}", dot12);
    }

    #[test]
    fn orbit_extreme_fov() {
        // Very narrow FOV
        let o1 = Orbit::at_origin(0.0, 0.0, 10.0, 1.0);
        let p1 = o1.proj(1.0);
        for row in p1.m.iter() {
            for &val in row {
                assert!(val.is_finite(), "narrow FOV non-finite: {}", val);
            }
        }
        // Very wide FOV (just under 180°)
        let o2 = Orbit::at_origin(0.0, 0.0, 10.0, 179.0);
        let p2 = o2.proj(1.0);
        for row in p2.m.iter() {
            for &val in row {
                assert!(val.is_finite(), "wide FOV non-finite: {}", val);
            }
        }
        // FOV = 0: tan(0) = 0, so 1/tan(0) = inf — acceptable, must not panic
        let o3 = Orbit::at_origin(0.0, 0.0, 10.0, 0.0);
        let p3 = o3.proj(1.0);
        // The diagonal entries should be infinite (1.0 / 0.0)
        assert!(p3.m[0][0].is_infinite(), "FOV=0: m[0][0] should be inf");
        assert!(p3.m[1][1].is_infinite(), "FOV=0: m[1][1] should be inf");
    }

    #[test]
    fn projected_bounds_huge_scene() {
        let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
        let huge = Bounds3::new([-1e9, -1e9, -1e9], [1e9, 1e9, 1e9]);
        let b = projected_bounds(&cam, &huge, (800u32, 600u32));
        for &v in b.iter() {
            assert!(v.is_finite(), "huge bounds produced non-finite: {}", v);
        }
        // Clamped to viewport
        assert!(b[0] >= 0.0 && b[0] <= 800.0, "min x out of range");
        assert!(b[1] >= 0.0 && b[1] <= 600.0, "min y out of range");
        assert!(b[2] >= 0.0 && b[2] <= 800.0, "max x out of range");
        assert!(b[3] >= 0.0 && b[3] <= 600.0, "max y out of range");
    }
}
