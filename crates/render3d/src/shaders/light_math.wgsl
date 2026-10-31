// Fixture shadow-map math shared by the lit shaders (phase 10). Mirrors render3d::lights:
// maps store far/(far-near)*(1 - near/z) of the distance z along the face's forward axis.

// Distance (m) along the map's forward axis of a stored depth d.
fn linear_depth(d: f32, near: f32, far: f32) -> f32 {
    let k = 1.0 - d * (far - near) / far;
    if (k <= 1e-7) {
        return 3.0e38;
    }
    return near / k;
}

// Cube face of the direction v (from the light to the point): 0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z.
fn cube_face_of(v: vec3<f32>) -> u32 {
    let a = abs(v);
    if (a.x >= a.y && a.x >= a.z) {
        return select(1u, 0u, v.x >= 0.0);
    }
    if (a.y >= a.z) {
        return select(3u, 2u, v.y >= 0.0);
    }
    return select(5u, 4u, v.z >= 0.0);
}

// Forward and up axes of a face (render3d::lights::cube_face); right = cross(forward, up).
fn cube_forward(face: u32) -> vec3<f32> {
    switch face {
        case 0u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 1u: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 2u: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 3u: { return vec3<f32>(0.0, -1.0, 0.0); }
        case 4u: { return vec3<f32>(0.0, 0.0, 1.0); }
        default: { return vec3<f32>(0.0, 0.0, -1.0); }
    }
}

fn cube_up(face: u32) -> vec3<f32> {
    if (face >= 4u) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

// Atlas uv (0..1, y down), the face's forward distance z (m) and the face index of the point at
// v = point - light position.
struct CubeHit { uv: vec2<f32>, z: f32, face: u32 };

fn cube_uv_depth(v: vec3<f32>) -> CubeHit {
    let face = cube_face_of(v);
    let f = cube_forward(face);
    let u = cube_up(face);
    let r = cross(f, u);
    let ma = max(dot(v, f), 1e-6);
    let ndc = vec2<f32>(dot(v, r) / ma, dot(v, u) / ma);
    var h: CubeHit;
    h.uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    h.z = ma;
    h.face = face;
    return h;
}
