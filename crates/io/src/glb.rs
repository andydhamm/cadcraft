//! Own glTF 2.0 GLB writer for CADCraft arch3d scenes.
//!
//! Pure Rust, no new dependency: a single binary buffer, one bufferView and one
//! accessor per attribute (`componentType` 5126/5125, `type` VEC3/VEC2/SCALAR,
//! `min`/`max` on POSITION), one mesh + node per [`Mesh`] named `wall-<ArchId>`,
//! one material per distinct `Mesh::material` (flat pbrMetallicRoughness base
//! colour via [`cadcraft_color`]), and one scene. The GLB header magic is
//! `0x46546C67`, little-endian, 4-byte aligned.

use std::collections::HashMap;

use cadcraft_arch3d::{Mesh, Scene3d};
use cadcraft_color::{Color, Rgb};
use cadcraft_doc::Drawing;
use serde_json::{Value, json};

/// Build a GLB from the 3D scene extracted from the drawing's arch model.
pub fn write_drawing(drawing: &Drawing) -> Vec<u8> {
    write(&Scene3d::build(&drawing.arch))
}

/// Write a full GLB file (header + JSON chunk + BIN chunk) for `scene`.
///
/// The file is the 12-byte header, a space-padded JSON chunk and a
/// zero-padded binary chunk, all little-endian and 4-byte aligned.
pub fn write(scene: &Scene3d) -> Vec<u8> {
    let (json_text, bin) = build_bin(scene);

    let json_pad = (4 - json_text.len() % 4) % 4;
    let bin_pad = (4 - bin.len() % 4) % 4;
    let total = 12 + (8 + json_text.len() + json_pad) + (8 + bin.len() + bin_pad);

    let mut out = Vec::with_capacity(total);
    // Magic "glTF" = 0x46546C67 (little-endian).
    out.extend_from_slice(&[0x67, 0x6c, 0x54, 0x46]);
    // Version 2.
    out.extend_from_slice(&[0x02, 0x00, 0x00, 0x00]);
    push_u32(&mut out, total as u32);

    // JSON chunk (0x4E4F534A), padded to 4-byte alignment.
    out.extend_from_slice(&[0x4a, 0x53, 0x4f, 0x4e]);
    push_u32(&mut out, json_text.len() as u32);
    out.extend_from_slice(json_text.as_bytes());
    out.resize(out.len() + json_pad, 0);

    // Binary chunk (0x004E4942), padded to 4-byte alignment.
    out.extend_from_slice(&[0x42, 0x49, 0x4e, 0x00]);
    push_u32(&mut out, bin.len() as u32);
    out.extend_from_slice(&bin);
    out.resize(out.len() + bin_pad, 0);

    out
}

/// Append a 32-bit value in little-endian byte order.
fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.push((v & 0xFF) as u8);
    buf.push(((v >> 8) & 0xFF) as u8);
    buf.push(((v >> 16) & 0xFF) as u8);
    buf.push(((v >> 24) & 0xFF) as u8);
}

/// Append a float in little-endian byte order (platform independent).
fn push_f32(buf: &mut Vec<u8>, v: f32) {
    push_u32(buf, v.to_bits());
}

/// Ensure `buf` length is a multiple of 4.
fn align4(buf: &mut Vec<u8>) {
    let r = buf.len() % 4;
    if r != 0 {
        buf.resize(buf.len() + (4 - r), 0);
    }
}

/// Build the JSON text plus the single binary buffer (with all views/accessors) for `scene`.
fn build_bin(scene: &Scene3d) -> (String, Vec<u8>) {
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut meshes: Vec<Value> = Vec::new();
    let mut nodes: Vec<Value> = Vec::new();

    let mut materials: Vec<Value> = Vec::new();
    let mut material_index: HashMap<String, u32> = HashMap::new();

    for (i, m) in scene.meshes.iter().enumerate() {
        let mat_idx = if let Some(idx) = material_index.get(&m.material) {
            *idx
        } else {
            let rgb = base_color_for(&m.material);
            materials.push(json!({
                "name": m.material,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [
                        rgb.0 as f64 / 255.0,
                        rgb.1 as f64 / 255.0,
                        rgb.2 as f64 / 255.0,
                        1.0
                    ],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0,
                },
            }));
            let idx = (materials.len() - 1) as u32;
            material_index.insert(m.material.clone(), idx);
            idx
        };

        let name = format!("wall-{}", m.object);

        // POSITION (VEC3, f32).
        let pos_start = bin.len();
        for p in &m.positions {
            push_f32(&mut bin, p[0]);
            push_f32(&mut bin, p[1]);
            push_f32(&mut bin, p[2]);
        }
        align4(&mut bin);
        let pos_len = (m.positions.len() * 12) as u32;
        buffer_views.push(json!({
            "buffer": 0,
            "offset": pos_start,
            "length": pos_len,
            "viewType": 6,
        }));
        let (pmin, pmax) = extents3(&m.positions);
        accessors.push(json!({
            "buffer": 0,
            "offset": pos_start,
            "count": m.positions.len() as u32,
            "type": "VEC3",
            "componentType": 5126,
            "min": [pmin[0] as f64, pmin[1] as f64, pmin[2] as f64],
            "max": [pmax[0] as f64, pmax[1] as f64, pmax[2] as f64],
        }));
        let acc_pos = accessors.len() - 1;

        // NORMAL (VEC3, f32).
        let n_start = bin.len();
        for n in &m.normals {
            push_f32(&mut bin, n[0]);
            push_f32(&mut bin, n[1]);
            push_f32(&mut bin, n[2]);
        }
        align4(&mut bin);
        let n_len = (m.normals.len() * 12) as u32;
        buffer_views.push(json!({
            "buffer": 0,
            "offset": n_start,
            "length": n_len,
            "viewType": 6,
        }));
        accessors.push(json!({
            "buffer": 0,
            "offset": n_start,
            "count": m.normals.len() as u32,
            "type": "VEC3",
            "componentType": 5126,
        }));
        let acc_norm = accessors.len() - 1;

        // TEXTURE_COORDS_0 (VEC2, f32).
        let uv_start = bin.len();
        for uv in &m.uvs {
            push_f32(&mut bin, uv[0]);
            push_f32(&mut bin, uv[1]);
        }
        align4(&mut bin);
        let uv_len = (m.uvs.len() * 8) as u32;
        buffer_views.push(json!({
            "buffer": 0,
            "offset": uv_start,
            "length": uv_len,
            "viewType": 5,
        }));
        accessors.push(json!({
            "buffer": 0,
            "offset": uv_start,
            "count": m.uvs.len() as u32,
            "type": "VEC2",
            "componentType": 5126,
        }));
        let acc_uv = accessors.len() - 1;

        // INDICES (SCALAR, uint32).
        let idx_start = bin.len();
        for idx in &m.indices {
            push_u32(&mut bin, *idx);
        }
        align4(&mut bin);
        let idx_len = (m.indices.len() * 4) as u32;
        buffer_views.push(json!({
            "buffer": 0,
            "offset": idx_start,
            "length": idx_len,
            "viewType": 4,
        }));
        accessors.push(json!({
            "buffer": 0,
            "offset": idx_start,
            "count": m.indices.len() as u32,
            "type": "SCALAR",
            "componentType": 5125,
        }));
        let acc_idx = accessors.len() - 1;

        meshes.push(json!({
            "name": name,
            "primitives": [
                {
                    "attributes": {
                        "POSITION": acc_pos,
                        "NORMAL": acc_norm,
                        "TEXTURE_COORDS_0": acc_uv
                    },
                    "indices": acc_idx,
                    "material": mat_idx
                }
            ]
        }));

        nodes.push(json!({
            "name": name,
            "mesh": i as u32,
            "translation": [0.0, 0.0, 0.0]
        }));
    }

    let buffers: Vec<Value> = if bin.is_empty() { Vec::new() } else { vec![json!({ "offset": 0, "length": bin.len() as u32 })] };
    let node_refs: Vec<u32> = (0..scene.meshes.len() as u32).collect();
    let value = json!({
        "version": 2,
        "asset": { "comment": "CADCraft arch3d GLB" },
        "scene": 0,
        "scenes": [ { "id": 0, "nodes": node_refs } ],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "buffers": buffers,
        "bufferViews": buffer_views,
        "accessors": accessors,
    });

    (json_string(value), bin)
}

/// Resolve a material name to a flat base colour via [`cadcraft_color`].
fn base_color_for(name: &str) -> Rgb {
    let c = Color::parse(name).unwrap_or(Color::ByLayer);
    c.resolve(Color::ByLayer, Color::ByLayer)
}

/// Axis-aligned extents (min/max) of a set of `VEC3` vertices.
fn extents3(v: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    if v.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for p in v {
        for i in 0..3 {
            if p[i] < min[i] {
                min[i] = p[i];
            }
            if p[i] > max[i] {
                max[i] = p[i];
            }
        }
    }
    (min, max)
}

/// Serialize a `Value` to a `String`. `serde_json::to_string` never fails for a
/// `Value`, so the fallback is defensive only.
fn json_string(v: Value) -> String {
    match serde_json::to_string(&v) {
        Ok(s) => s,
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cadcraft_arch3d::{Bounds3, Mesh, Scene3d};
    use cadcraft_doc::ArchId;
    use serde_json::Value;

    /// One triangle, one triangle's indices, and a known material/id.
    fn wall_mesh() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            uvs: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]],
            indices: vec![0, 1, 2],
            material: "wall".into(),
            object: ArchId(1),
        }
    }

    fn scene_with(walls: usize) -> Scene3d {
        Scene3d {
            meshes: (0..walls)
                .map(|i| {
                    let mut m = wall_mesh();
                    m.object = ArchId((i + 1) as u32);
                    m
                })
                .collect(),
            bounds: Bounds3::empty(),
            revision: 0,
        }
    }

    /// Parse the JSON chunk of a GLB byte slice.
    fn glb_json(bytes: &[u8], offset: usize, len: usize) -> Value {
        serde_json::from_slice(&bytes[offset..offset + len]).unwrap_or_default()
    }

    /// A little-endian `u32` stored at byte offset `off` (4 bytes, LE).
    fn le32(bytes: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    }

    #[test]
    fn glb_header_and_chunk_layout_are_valid() {
        let bytes = write(&scene_with(1));
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(&bytes[4..8], &[0x02, 0x00, 0x00, 0x00]);
        let total = le32(&bytes, 8);
        assert_eq!(total as usize, bytes.len());

        let json_start = 12;
        assert_eq!(&bytes[json_start..json_start + 4], b"JSON");
        let json_len = le32(&bytes, json_start + 4) as usize;
        let json_pad = (4 - json_len % 4) % 4;
        let bin_start = json_start + 8 + json_len + json_pad;
        assert_eq!(bin_start % 4, 0);
        assert_eq!(&bytes[bin_start..bin_start + 4], b"BIN\0");
        let bin_len = le32(&bytes, bin_start + 4) as usize;
        let bin_pad = (4 - bin_len % 4) % 4;
        assert_eq!(bytes.len(), bin_start + 8 + bin_len + bin_pad);
    }

    #[test]
    fn glb_json_chunk_parses_and_has_one_mesh_per_wall() {
        let bytes = write(&scene_with(3));
        let json_start = 12;
        let json_len = le32(&bytes, json_start + 4) as usize;
        let json = glb_json(&bytes, json_start + 8, json_len);
        match &json["meshes"] {
            Value::Array(m) => {
                assert_eq!(m.len(), 3);
                for i in 0..3 {
                    assert_eq!(m[i]["name"], format!("wall-w{}", i + 1));
                }
            }
            _ => assert!(false, "json['meshes'] is not an array"),
        }
    }

    #[test]
    fn glb_accessor_min_max_match_the_geometry() {
        let bytes = write(&scene_with(1));
        let json_start = 12;
        let json_len = le32(&bytes, json_start + 4) as usize;
        let json = glb_json(&bytes, json_start + 8, json_start + 8 + json_len);
        let pos = &json["accessors"][0];
        match &pos["min"] {
            Value::Array(a) => {
                assert_eq!(a.len(), 3);
                assert_eq!(a[0].as_f64().unwrap_or(f64::NAN), 0.0);
                assert_eq!(a[1].as_f64().unwrap_or(f64::NAN), 0.0);
                assert_eq!(a[2].as_f64().unwrap_or(f64::NAN), 0.0);
            }
            _ => assert!(false, "min is not an array"),
        }
        match &pos["max"] {
            Value::Array(a) => {
                assert_eq!(a.len(), 3);
                assert_eq!(a[0].as_f64().unwrap_or(f64::NAN), 1.0);
                assert_eq!(a[1].as_f64().unwrap_or(f64::NAN), 0.0);
                assert_eq!(a[2].as_f64().unwrap_or(f64::NAN), 1.0);
            }
            _ => assert!(false, "max is not an array"),
        }
    }

    #[test]
    fn glb_indices_count_is_a_multiple_of_three() {
        let bytes = write(&scene_with(2));
        let json_start = 12;
        let json_len = le32(&bytes, json_start + 4) as usize;
        let json = glb_json(&bytes, json_start + 8, json_start + 8 + json_len);
        match &json["accessors"] {
            Value::Array(accs) => {
                for acc in accs {
                    if let Some(t) = acc["type"].as_str() {
                        if t == "SCALAR" {
                            match acc["count"].as_u64() {
                                Some(count) => assert!(count % 3 == 0, "indices count {count} is not a multiple of 3"),
                                _ => assert!(false, "no count on SCALAR accessor"),
                            }
                        }
                    }
                }
            }
            _ => assert!(false, "accessors is not an array"),
        }
    }

    #[test]
    fn glb_is_byte_deterministic() {
        let a = write(&scene_with(2));
        let b = write(&scene_with(2));
        assert_eq!(a, b);
    }

    #[test]
    fn empty_scene_writes_a_valid_empty_glb() {
        let scene = Scene3d { meshes: Vec::new(), bounds: Bounds3::empty(), revision: 0 };
        let bytes = write(&scene);
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(&bytes[4..8], &[0x02, 0x00, 0x00, 0x00]);
        let total = le32(&bytes, 8);
        assert_eq!(total as usize, bytes.len());
        let json_start = 12;
        let json_len = le32(&bytes, json_start + 4) as usize;
        let json = glb_json(&bytes, json_start + 8, json_start + 8 + json_len);
        assert_eq!(json["meshes"].as_array().and_then(|a| a.len()), Some(0));
        assert_eq!(json["scenes"][0]["nodes"].as_array().and_then(|a| a.len()), Some(0));
        let json_pad = (4 - json_len % 4) % 4;
        let bin_start = json_start + 8 + json_len + json_pad;
        assert_eq!(&bytes[bin_start..bin_start + 4], b"BIN\0");
        assert_eq!(bytes.len(), bin_start + 8);
    }
}
