struct Uniforms {
    view_proj: mat4x4<f32>,   // offset 0,  64 bytes (column-major)
    light_dir: vec3<f32>,     // offset 64
    ambient:   f32,           // offset 76
    albedo:    vec3<f32>,     // offset 80
    _pad:      f32,           // offset 92  -> total 96 bytes
};
@group(0) @binding(0) var<uniform> u: Uniforms;

// Per-draw material and per-frame sun (group 1, dynamic offset), 96 bytes.
struct PbrUniforms {
    light_dir:     vec3<f32>,   // offset 0, toward the sun
    sun_intensity: f32,         // offset 12
    ambient:       f32,         // offset 16
    metallic:      f32,         // offset 20
    roughness:     f32,         // offset 24
    alpha:         f32,         // offset 28
    style:         u32,         // offset 32: 0 shaded, 1 shaded-edges, 2 hidden-line, 3 glass-house
    _p0:           f32,         // offset 36
    _p1:           f32,         // offset 40
    _p2:           f32,         // offset 44
    camera_pos:    vec3<f32>,   // offset 48
    shadow_min:    f32,         // offset 60
    shadow_max:    f32,         // offset 64
    _p3:           f32,         // offset 68
    _p4:           f32,         // offset 72
    _p5:           f32,         // offset 76
    _p6:           vec4<f32>,   // offset 80 -> total 96 bytes
};
@group(1) @binding(1) var<uniform> p: PbrUniforms;

const PI: f32 = 3.14159265;
// Matches `offscreen::CLEAR`: hidden-line draws faces in the background colour.
const CLEAR_COLOR: vec3<f32> = vec3<f32>(0.02, 0.025, 0.034);
const GLASS_ALPHA: f32 = 0.35;

struct VIn  { @location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(2) uv: vec2<f32> };
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) nrm: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) wpos: vec3<f32> };

@vertex fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.nrm  = v.nrm;
    o.uv   = v.uv;
    o.wpos = v.pos;
    return o;
}

// GGX normal distribution.
fn distribution_ggx(ndh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = ndh * ndh * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-6);
}

// Smith geometry term (Schlick-GGX form, direct lighting).
fn geometry_smith(ndv: f32, ndl: f32, rough: f32) -> f32 {
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let gv = ndv / (ndv * (1.0 - k) + k);
    let gl = ndl / (ndl * (1.0 - k) + k);
    return gv * gl;
}

// Schlick fresnel.
fn fresnel_schlick(vdh: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - vdh, 0.0, 1.0), 5.0);
}

@fragment fn fs(v: VOut) -> @location(0) vec4<f32> {
    // Flat shading from the triangle's own plane: cross(dpdy, dpdx) of the world position is the
    // face normal turned toward the camera. arch3d's boxes share 8 corners with averaged normals,
    // which interpolate to anything across a face (speckled walls, 2026-10-25), and its winding is
    // not consistent, so neither the vertex normal nor front_facing can be trusted here.
    let n = normalize(cross(dpdy(v.wpos), dpdx(v.wpos)));

    // Hidden-line: faces in the clear colour, depth written; the edges come from arch3d::edges.
    if (p.style == 2u) {
        return vec4<f32>(CLEAR_COLOR, 1.0);
    }

    let l = normalize(p.light_dir);
    var vdir = n;
    let to_eye = p.camera_pos - v.wpos;
    // Matte dielectrics keep a per-face constant view direction so a planar face is one colour
    // (the perspective view vector would otherwise shift the faint specular by a level or two).
    if (dot(to_eye, to_eye) > 1e-12 && (p.metallic > 0.01 || p.alpha < 1.0)) {
        vdir = normalize(to_eye);
    }
    let h = normalize(l + vdir);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, vdir), 1e-4);
    let ndh = max(dot(n, h), 0.0);
    let vdh = max(dot(vdir, h), 0.0);

    let metallic = clamp(p.metallic, 0.0, 1.0);
    let rough = clamp(p.roughness, 0.045, 1.0);
    let f0 = mix(vec3<f32>(0.04), u.albedo, metallic);
    let f = fresnel_schlick(vdh, f0);
    let d = distribution_ggx(ndh, rough * rough);
    let g = geometry_smith(ndv, ndl, rough);
    let spec = d * g * f / max(4.0 * ndv * ndl, 1e-4);
    // Lambert diffuse for dielectrics; metals have none.
    let kd = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = kd * u.albedo / PI;

    // The sun carries (1 - ambient) of the light so a white diffuse face at ndl = 1 is fully lit.
    let radiance = p.sun_intensity * (1.0 - p.ambient) * PI;
    let lit = (diffuse + spec) * radiance * ndl;
    let amb = u.albedo * p.ambient * (1.0 - metallic * 0.5);
    let c = amb + lit;

    var alpha = clamp(p.alpha, 0.0, 1.0);
    if (p.style == 3u) {
        alpha = min(alpha, GLASS_ALPHA);
    }
    return vec4<f32>(c, alpha);
}
