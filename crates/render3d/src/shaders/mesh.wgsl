struct Uniforms {
    view_proj: mat4x4<f32>,   // offset 0,  64 bytes (column-major)
    light_dir: vec3<f32>,     // offset 64
    ambient:   f32,           // offset 76
    albedo:    vec3<f32>,     // offset 80
    _pad:      f32,           // offset 92  -> total 96 bytes
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VIn  { @location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(2) uv: vec2<f32> };
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) nrm: vec3<f32>, @location(1) uv: vec2<f32> };

@vertex fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.nrm  = v.nrm;
    o.uv   = v.uv;
    return o;
}

@fragment fn fs(v: VOut) -> @location(0) vec4<f32> {
    let n = normalize(v.nrm);
    let ndl = max(dot(n, u.light_dir), 0.0);
    let c = u.albedo * (u.ambient + (1.0 - u.ambient) * ndl);
    return vec4<f32>(c, 1.0);
}
