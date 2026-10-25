use super::*;
use crate::camera::Orbit;
use crate::gputest;
use crate::types::{ArchId, Mesh};

/// An axis-aligned box mesh with outward normals; `material` picks its albedo.
fn cuboid(material: &str, min: [f32; 3], max: [f32; 3]) -> Mesh {
    let mut m = Mesh::new(material, ArchId(1));
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([1.0, 0.0, 0.0], [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]]),
        ([-1.0, 0.0, 0.0], [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]]),
        ([0.0, 1.0, 0.0], [[x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]]),
        ([0.0, -1.0, 0.0], [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]]),
        ([0.0, 0.0, 1.0], [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]),
        ([0.0, 0.0, -1.0], [[x0, y1, z0], [x1, y1, z0], [x1, y0, z0], [x0, y0, z0]]),
    ];
    for (n, quad) in faces {
        let base = m.positions.len() as u32;
        for p in quad {
            m.positions.push(p);
            m.normals.push(n);
            m.uvs.push([p[0], p[2]]);
        }
        m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

fn scene(meshes: Vec<Mesh>) -> Scene3d {
    let mut s = Scene3d { meshes, ..Default::default() };
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in s.meshes.iter().flat_map(|m| m.positions.iter()) {
        for k in 0..3 {
            lo[k] = lo[k].min(f64::from(p[k]));
            hi[k] = hi[k].max(f64::from(p[k]));
        }
    }
    if lo[0].is_finite() {
        s.bounds = crate::types::Bounds3::new(lo, hi);
    }
    s
}

fn renderer(size: (u32, u32)) -> Option<Offscreen> {
    let (device, queue) = gputest::shared_device()?;
    Some(Offscreen::with_device(device, queue, size))
}

fn px(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn clear_srgb() -> [u8; 4] {
    // CLEAR is linear; the sRGB target stores it encoded.
    let enc = |c: f64| {
        let s = if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round() as u8
    };
    [enc(CLEAR.r), enc(CLEAR.g), enc(CLEAR.b), 255]
}

fn close(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    (0..4).all(|k| a[k].abs_diff(b[k]) <= tol)
}

#[test]
fn headless_png_reports_no_adapter_instead_of_panicking() {
    let scene = cadcraft_arch3d::Scene3d::default();
    let camera = Camera::Orbit(Orbit::at_origin(30.0, 30.0, 10.0, 45.0));
    match render_png(&scene, &camera, &Light::DEFAULT, (64, 64)) {
        Ok(png) => assert_eq!(png_rgba(&png).expect("decode").len(), 64 * 64 * 4),
        Err(Render3dError::NoAdapter) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn png_roundtrip_keeps_pixels() {
    let mut px = Vec::new();
    for y in 0..4u8 {
        for x in 0..4u8 {
            px.extend_from_slice(&[x * 60, y * 60, 128, 255]);
        }
    }
    let png = encode_png(4, 4, &px).expect("encode");
    assert_eq!(png_rgba(&png).expect("decode"), px);
    assert!(!pixels_are_uniform(&px));
}

#[test]
fn uniform_pixels_are_detected() {
    let px = vec![26u8; 8 * 8 * 4];
    assert!(pixels_are_uniform(&px));
    let mut px2 = px.clone();
    px2[100] = 200;
    assert!(!pixels_are_uniform(&px2));
}

#[test]
fn an_empty_scene_reads_back_the_clear_colour() {
    let Some(mut r) = renderer((32, 24)) else { return };
    r.render(&Scene3d::default(), 0, &Camera::Orbit(Orbit::at_origin(30.0, 30.0, 10.0, 45.0)), &Light::DEFAULT);
    let rgba = r.read_rgba().expect("readback");
    assert_eq!(rgba.len(), 32 * 24 * 4);
    assert!(pixels_are_uniform(&rgba));
    assert!(close(px(&rgba, 32, 0, 0), clear_srgb(), 1), "{:?} vs {:?}", px(&rgba, 32, 0, 0), clear_srgb());
}

#[test]
fn a_box_is_drawn_in_the_middle_and_the_corners_stay_clear() {
    let Some(mut r) = renderer((64, 64)) else { return };
    let s = scene(vec![cuboid("wall", [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])]);
    r.render(&s, 1, &Camera::Orbit(Orbit::at_origin(30.0, 30.0, 8.0, 45.0)), &Light::DEFAULT);
    let rgba = r.read_rgba().expect("readback");
    assert!(close(px(&rgba, 64, 0, 0), clear_srgb(), 1));
    assert!(close(px(&rgba, 64, 63, 63), clear_srgb(), 1));
    let mid = px(&rgba, 64, 32, 32);
    assert!(!close(mid, clear_srgb(), 8), "the box is missing at the centre: {mid:?}");
}

#[test]
fn faces_turned_differently_to_the_light_get_different_shades() {
    let Some(mut r) = renderer((96, 96)) else { return };
    let s = scene(vec![cuboid("wall", [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])]);
    // Seen from above the south-west corner: the top and two side faces are visible.
    r.render(&s, 1, &Camera::Orbit(Orbit::at_origin(225.0, 35.0, 7.0, 45.0)), &Light { dir: [0.0, 0.0, 1.0], ambient: 0.2 });
    let rgba = r.read_rgba().expect("readback");
    let shades: std::collections::BTreeSet<[u8; 4]> =
        rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).filter(|&p| !close(p, clear_srgb(), 8)).collect();
    // Light straight down: the top is fully lit, the sides only get ambient.
    assert!(shades.len() >= 2, "a lit box shows at least two distinct shades, got {shades:?}");
}

#[test]
fn the_nearer_of_two_boxes_wins_whatever_the_draw_order() {
    let Some(mut r) = renderer((64, 64)) else { return };
    // Camera on +Y looking at the origin (azimuth 0 = north, elevation 0).
    let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 10.0, 45.0));
    let light = Light { dir: [0.0, 1.0, 0.0], ambient: 0.3 };
    let near = cuboid("glass", [-1.0, 1.0, -1.0], [1.0, 2.0, 1.0]);
    let far = cuboid("door", [-2.0, -3.0, -2.0], [2.0, -2.0, 2.0]);
    let mut seen = Vec::new();
    for (key, order) in [(1, vec![near.clone(), far.clone()]), (2, vec![far, near])] {
        r.render(&scene(order), key, &cam, &light);
        let rgba = r.read_rgba().expect("readback");
        seen.push(px(&rgba, 64, 32, 32));
    }
    assert_eq!(seen[0], seen[1], "draw order changed the visible surface: depth test missing");
    // Glass albedo has more blue than red; the door's is the other way round.
    assert!(seen[0][2] > seen[0][0], "the far box shows through: {:?}", seen[0]);
}

#[test]
fn a_face_seen_from_behind_is_lit_from_the_camera_side() {
    let Some(mut r) = renderer((48, 48)) else { return };
    // One horizontal quad with its normal up, seen from below and lit from below.
    let mut m = Mesh::new("ceiling", ArchId(2));
    for p in [[-2.0, -2.0, 0.0], [2.0, -2.0, 0.0], [2.0, 2.0, 0.0], [-2.0, 2.0, 0.0]] {
        m.positions.push(p);
        m.normals.push([0.0, 0.0, 1.0]);
        m.uvs.push([0.0, 0.0]);
    }
    m.indices = vec![0, 1, 2, 0, 2, 3];
    let s = scene(vec![m]);
    let cam = Camera::Orbit(Orbit::at_origin(10.0, -60.0, 6.0, 45.0));
    r.render(&s, 1, &cam, &Light { dir: [0.0, 0.0, -1.0], ambient: 0.1 });
    let lit = px(&r.read_rgba().expect("readback"), 48, 24, 24);
    r.render(&s, 1, &cam, &Light { dir: [0.0, 0.0, 1.0], ambient: 0.1 });
    let unlit = px(&r.read_rgba().expect("readback"), 48, 24, 24);
    assert!(lit[0] > unlit[0] + 40, "back face not flipped toward the camera: lit {lit:?} unlit {unlit:?}");
}

#[test]
fn an_unchanged_scene_key_reuses_the_uploaded_meshes() {
    let Some(mut r) = renderer((16, 16)) else { return };
    let s = scene(vec![cuboid("wall", [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])]);
    let cam = Camera::Orbit(Orbit::at_origin(30.0, 30.0, 8.0, 45.0));
    for az in [0.0, 10.0, 20.0] {
        let cam = Camera::Orbit(Orbit::at_origin(az, 30.0, 8.0, 45.0));
        r.render(&s, 7, &cam, &Light::DEFAULT);
    }
    assert_eq!(r.uploads(), 1);
    r.render(&s, 8, &cam, &Light::DEFAULT);
    assert_eq!(r.uploads(), 2);
}

#[test]
fn resize_changes_the_readback_size_and_the_generation() {
    let Some(mut r) = renderer((16, 16)) else { return };
    let g = r.generation();
    assert!(!r.resize((16, 16)));
    assert!(r.resize((40, 10)));
    assert!(r.generation() > g);
    assert_eq!(r.size(), (40, 10));
    assert!(r.resize((0, 0)));
    assert_eq!(r.size(), (1, 1));
    r.render(&Scene3d::default(), 0, &Camera::Orbit(Orbit::at_origin(0.0, 30.0, 5.0, 45.0)), &Light::DEFAULT);
    assert_eq!(r.read_rgba().expect("readback").len(), 4);
}

#[test]
fn many_meshes_each_get_their_own_uniform_slot() {
    let Some(mut r) = renderer((64, 32)) else { return };
    // 40 boxes in a row with alternating materials: more draws than a single slot holds.
    let meshes: Vec<Mesh> = (0..40)
        .map(|i| {
            let x = i as f32 * 0.5 - 10.0;
            cuboid(if i % 2 == 0 { "glass" } else { "door" }, [x, -0.2, -2.0], [x + 0.4, 0.2, 2.0])
        })
        .collect();
    let s = scene(meshes);
    let cam = Camera::Orbit(Orbit::at_origin(0.0, 0.0, 14.0, 45.0));
    r.render(&s, 3, &cam, &Light { dir: [0.0, 1.0, 0.0], ambient: 0.3 });
    let rgba = r.read_rgba().expect("readback");
    let row: Vec<[u8; 4]> = (0..64).map(|x| px(&rgba, 64, x, 16)).filter(|p| !close(*p, clear_srgb(), 8)).collect();
    let bluish = row.iter().filter(|p| p[2] > p[0]).count();
    let reddish = row.iter().filter(|p| p[0] > p[2]).count();
    assert!(bluish > 0 && reddish > 0, "every draw used one albedo: {row:?}");
}

#[test]
fn readback_returns_in_bounded_time() {
    let Some(mut r) = renderer((256, 256)) else { return };
    let start = std::time::Instant::now();
    for _ in 0..5 {
        r.render(&Scene3d::default(), 0, &Camera::Orbit(Orbit::at_origin(0.0, 30.0, 5.0, 45.0)), &Light::DEFAULT);
        r.read_rgba().expect("readback");
    }
    assert!(start.elapsed() < READBACK_TIMEOUT, "five readbacks took {:?}", start.elapsed());
}
