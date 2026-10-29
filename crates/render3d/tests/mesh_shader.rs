//! Tests for the mesh WGSL shader.
//!
//! The shader source is included via `include_str!` in the pipeline code, so
//! these tests verify the source text directly. The uniform block layout is
//! load-bearing: the Rust side mirrors it byte-for-byte.

use std::fs;

fn shader_source() -> String {
    fs::read_to_string("src/shaders/mesh.wgsl").expect("mesh.wgsl must exist")
}

#[test]
fn mesh_shader_uniform_struct_declared() {
    let src = shader_source();
    assert!(src.contains("struct Uniforms {"), "shader must declare struct Uniforms");
    assert!(src.contains("view_proj: mat4x4<f32>"), "Uniforms must have view_proj: mat4x4<f32>");
    assert!(src.contains("light_dir: vec3<f32>"), "Uniforms must have light_dir: vec3<f32>");
    assert!(src.contains("ambient:   f32"), "Uniforms must have ambient: f32");
    assert!(src.contains("albedo:    vec3<f32>"), "Uniforms must have albedo: vec3<f32>");
    assert!(src.contains("_pad:      f32"), "Uniforms must have _pad: f32 for alignment");
}

#[test]
fn mesh_shader_entry_points_present() {
    let src = shader_source();
    assert!(src.contains("@vertex fn vs("), "shader must have @vertex fn vs entry point");
    assert!(src.contains("@fragment fn fs("), "shader must have @fragment fn fs entry point");
}

#[test]
fn mesh_shader_vertex_io_layout() {
    let src = shader_source();
    // Vertex input
    assert!(src.contains("@location(0) pos: vec3<f32>"), "vertex input pos must be at location 0");
    assert!(src.contains("@location(1) nrm: vec3<f32>"), "vertex input nrm must be at location 1");
    assert!(src.contains("@location(2) uv: vec2<f32>"), "vertex input uv must be at location 2");
    // Vertex output
    assert!(src.contains("@builtin(position) clip: vec4<f32>"), "vertex output must have clip at builtin(position)");
}

#[test]
fn mesh_shader_flat_shading_uses_world_position_derivatives() {
    let src = shader_source();
    // The fragment shader computes the face normal from the screen-space
    // derivatives of the interpolated world position, not from the vertex
    // normal. This is because arch3d's boxes share corners with averaged
    // normals and inconsistent winding.
    assert!(src.contains("dpdx(v.wpos)") && src.contains("dpdy(v.wpos)"), "fragment shader must compute normal from dpdx/dpdy of world position");
    assert!(src.contains("normalize(cross("), "fragment shader must normalize the cross product of the derivatives");
}

#[test]
fn mesh_shader_lighting_formula() {
    let src = shader_source();
    // Metallic-roughness PBR: ambient + (Lambert diffuse + GGX specular) * radiance * N·L
    assert!(src.contains("max(dot(n, l), 0.0)"), "shader must clamp N·L at 0");
    assert!(src.contains("let c = amb + lit;"), "shader must add ambient and direct lighting");
}

#[test]
fn mesh_shader_returns_opaque_color() {
    let src = shader_source();
    // Opaque by default (alpha 1.0); only glass (alpha < 1) blends.
    assert!(src.contains("return vec4<f32>(c, alpha);"), "fragment shader must return the material alpha");
}
