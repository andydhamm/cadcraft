//! Tests for the mesh shader uniform layout.

#[repr(C)]
#[allow(dead_code)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 3],
    ambient: f32,
    albedo: [f32; 3],
    _pad: f32,
}

#[test]
fn mesh_shader_uniform_block_is_96_bytes() {
    let src = include_str!("../src/shaders/mesh.wgsl");

    assert_eq!(std::mem::offset_of!(Uniforms, view_proj), 0);
    assert_eq!(std::mem::offset_of!(Uniforms, light_dir), 64);
    assert_eq!(std::mem::offset_of!(Uniforms, ambient), 76);
    assert_eq!(std::mem::offset_of!(Uniforms, albedo), 80);
    assert_eq!(std::mem::offset_of!(Uniforms, _pad), 92);
    assert_eq!(std::mem::size_of::<Uniforms>(), 96);

    assert!(src.contains("struct Uniforms {"));
    assert!(src.contains("view_proj: mat4x4<f32>"));
    assert!(src.contains("light_dir: vec3<f32>"));
    assert!(src.contains("ambient:   f32"));
    assert!(src.contains("albedo:    vec3<f32>"));
    assert!(src.contains("_pad:      f32"));
    assert!(src.contains("@group(0) @binding(0) var<uniform> u: Uniforms;"));
}
