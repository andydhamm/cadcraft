//! Own glTF 2.0 GLB writer for CADCraft arch3d scenes.
//!
//! Pure Rust, no new dependency: a single binary buffer, one bufferView and one
//! accessor per attribute (`componentType` 5126/5125, `type` VEC3/VEC2/SCALAR,
//! `min`/`max` on POSITION), one mesh + node per [`Mesh`] named by material
//! (`wall-w{n}`, `<material>-o{n}`, or roof: `roof-r{n}p{m}`,
//! `roof-r{n}-fascia`, `roof-r{n}-soffit`),
//! one material per distinct `Mesh::material` (flat pbrMetallicRoughness base
//! colour via [`cadcraft_color`]), and one scene. The GLB header magic is
//! `0x46546C67`, little-endian, 4-byte aligned.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

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

    // JSON chunk (0x4E4F534A), padded to 4-byte alignment with 0x20.
    // chunkLength is the padded length per glTF 2.0 §4.4.
    push_u32(&mut out, (json_text.len() + json_pad) as u32);
    out.extend_from_slice(&[0x4a, 0x53, 0x4f, 0x4e]);
    out.extend_from_slice(json_text.as_bytes());
    out.resize(out.len() + json_pad, 0x20);

    // Binary chunk (0x004E4942), padded to 4-byte alignment with 0x00.
    // chunkLength is the padded length per glTF 2.0 §4.4.
    push_u32(&mut out, (bin.len() + bin_pad) as u32);
    out.extend_from_slice(&[0x42, 0x49, 0x4e, 0x00]);
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
    /// Plane index seen so far per roof id, so roof planes are named in
    /// scene order: `roof-r{n}p0`, `roof-r{n}p1`, ...
    let mut roof_planes: HashMap<u32, u32> = HashMap::new();

    let written_meshes: Vec<&Mesh> = scene.meshes.iter().filter(|m| !m.positions.is_empty()).collect();
    for (i, m) in written_meshes.iter().enumerate() {
        let mat_idx = if let Some(idx) = material_index.get(&m.material) {
            *idx
        } else {
            let is_roof = m.material == "roof";
            let rgb = base_color_for(&m.material);
            let is_glass = m.material.contains("glass");
            let mut mat = json!({
                "name": m.material,
                "pbrMetallicRoughness": {
                    "baseColorFactor": if is_roof {
                        [ROOF_ALBEDO[0], ROOF_ALBEDO[1], ROOF_ALBEDO[2], 1.0]
                    } else {
                        [
                            rgb.0 as f64 / 255.0,
                            rgb.1 as f64 / 255.0,
                            rgb.2 as f64 / 255.0,
                            if is_glass { 0.35 } else { 1.0 }
                        ]
                    },
                    "metallicFactor": 0.0,
                    "roughnessFactor": if is_roof { 0.9 } else if is_glass { 0.05 } else { 1.0 },
                },
            });
            if is_glass {
                mat["alphaMode"] = json!("BLEND");
            }
            materials.push(mat);
            let idx = (materials.len() - 1) as u32;
            material_index.insert(m.material.clone(), idx);
            idx
        };

        let name = match m.material.as_str() {
            "roof" => {
                let plane = match roof_planes.entry(m.object.0) {
                    Entry::Occupied(mut e) => {
                        let i = *e.get();
                        *e.get_mut() += 1;
                        i
                    }
                    Entry::Vacant(_) => {
                        roof_planes.insert(m.object.0, 1);
                        0
                    }
                };
                format!("roof-r{}p{}", m.object.0, plane)
            }
            "fascia" => format!("roof-r{}-fascia", m.object.0),
            "soffit" => format!("roof-r{}-soffit", m.object.0),
            "wall" => format!("wall-{}", m.object),
            "stair" => format!("stair-s{}", m.object.0),
            "railing" => format!("railing-ra{}", m.object.0),
            "deck" => format!("deck-r{}", m.object.0),
            _ => format!("{}-o{}", m.material, m.object.0),
        };

        // POSITION (VEC3, f32).
        let pos_start = bin.len();
        for p in &m.positions {
            push_f32(&mut bin, p[0]);
            push_f32(&mut bin, p[1]);
            push_f32(&mut bin, p[2]);
        }
        align4(&mut bin);
        let pos_len = (m.positions.len() * 12) as u32;
        let bv_pos = buffer_views.len();
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": pos_start,
            "byteLength": pos_len,
            "target": 34962,
        }));
        let (pmin, pmax) = extents3(&m.positions);
        accessors.push(json!({
            "bufferView": bv_pos,
            "byteOffset": 0,
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
        let bv_norm = buffer_views.len();
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": n_start,
            "byteLength": n_len,
            "target": 34962,
        }));
        accessors.push(json!({
            "bufferView": bv_norm,
            "byteOffset": 0,
            "count": m.normals.len() as u32,
            "type": "VEC3",
            "componentType": 5126,
        }));
        let acc_norm = accessors.len() - 1;

        // TEXCOORD_0 (VEC2, f32). glTF 2.0 names the first UV set TEXCOORD_0;
        // application-specific semantics must instead start with an underscore.
        let uv_start = bin.len();
        for uv in &m.uvs {
            push_f32(&mut bin, uv[0]);
            push_f32(&mut bin, uv[1]);
        }
        align4(&mut bin);
        let uv_len = (m.uvs.len() * 8) as u32;
        let bv_uv = buffer_views.len();
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": uv_start,
            "byteLength": uv_len,
            "target": 34962,
        }));
        accessors.push(json!({
            "bufferView": bv_uv,
            "byteOffset": 0,
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
        let bv_idx = buffer_views.len();
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": idx_start,
            "byteLength": idx_len,
            "target": 34963,
        }));
        accessors.push(json!({
            "bufferView": bv_idx,
            "byteOffset": 0,
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
                        "TEXCOORD_0": acc_uv
                    },
                    "indices": acc_idx,
                    "material": mat_idx
                }
            ]
        }));

        // Per-mesh node. The mesh is placed under a single root node that
        // rotates the Z-up arch3d scene into glTF's Y-up convention
        // (-90 degrees about X: (x, y, z) -> (x, z, -y)).
        nodes.push(json!({
            "name": name,
            "mesh": i as u32
        }));
    }

    let buffers: Vec<Value> = if bin.is_empty() { Vec::new() } else { vec![json!({ "byteLength": bin.len() as u32 })] };
    // Root node carries the Z-up -> Y-up rotation; its children are the per-mesh
    // nodes. An empty scene keeps an empty node list.
    let root = if written_meshes.is_empty() {
        Vec::new()
    } else {
        // Per-mesh nodes are the children; the root is node 0, so mesh nodes are
        // renumbered 1..=n to sit after it in the node list.
        let children: Vec<u32> = (1..=written_meshes.len() as u32).collect();
        vec![json!({
            "name": "arch3d-root",
            "rotation": [ROOT_ROTATION[0], ROOT_ROTATION[1], ROOT_ROTATION[2], ROOT_ROTATION[3]],
            "children": children,
        })]
    };
    let all_nodes: Vec<Value> = root.into_iter().chain(nodes).collect();
    let scene_nodes: Vec<Value> = if written_meshes.is_empty() { Vec::new() } else { vec![json!(0)] };
    let value = json!({
        "asset": { "version": "2.0", "generator": "CADCraft arch3d" },
        "scene": 0,
        "scenes": [ { "name": "arch3d", "nodes": scene_nodes } ],
        "nodes": all_nodes,
        "meshes": meshes,
        "materials": materials,
        "buffers": buffers,
        "bufferViews": buffer_views,
        "accessors": accessors,
    });

    (json_string(value), bin)
}

/// Root node rotation: -90 degrees about X, the quaternion that turns the Z-up
/// arch3d scene into glTF's Y-up convention, mapping (x, y, z) to (x, z, -y).
/// Stored as (x, y, z, w); the angle is -π/2, so the axis (1,0,0) scaled by
/// sin(-π/4) ≈ -0.7071068 with w = cos(-π/4) ≈ 0.7071068.
const ROOT_ROTATION: [f64; 4] = [-0.7071068, 0.0, 0.0, 0.7071068];

/// Matte reddish roof albedo, as a 0..=1 `f64` triple.
const ROOF_ALBEDO: [f64; 3] = [151.0 / 255.0, 84.0 / 255.0, 74.0 / 255.0];

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

    use cadcraft_arch3d::{Mesh, Scene3d};
    use cadcraft_doc::ArchId;
    use serde_json::Value;

    /// One triangle, one triangle's indices, and a known material/id.
    fn wall_mesh() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            uvs: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]],
            indices: vec![0, 1, 2],
            ..Mesh::new("wall", ArchId(1))
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
            ..Default::default()
        }
    }

    /// A roof scene: two roof planes, a fascia and a soffit, all on roof 0.
    fn scene_with_roof() -> Scene3d {
        let mut scene = scene_with(0);
        scene.meshes.extend(["roof", "roof", "fascia", "soffit"].iter().map(|material| {
            let mut m = wall_mesh();
            m.material = material.to_string();
            m.object = ArchId(0);
            m
        }));
        scene
    }

    /// Parse the JSON chunk of a GLB byte slice.
    fn glb_json(bytes: &[u8], offset: usize, len: usize) -> Value {
        serde_json::from_slice(&bytes[offset..offset + len]).unwrap_or_default()
    }

    /// The root node of the scene, if any (present only when there is a mesh).
    fn root_node(json: &Value) -> Option<&Value> {
        json["nodes"].as_array()?.first()
    }

    /// Read the JSON chunk of a GLB byte slice and parse it.
    fn glb_json_chunk(bytes: &[u8]) -> Value {
        let json_len = le32(&bytes, 12) as usize;
        glb_json(bytes, 20, json_len)
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

        let json_len = le32(&bytes, 12) as usize;
        assert_eq!(le32(&bytes, 16), 0x4E4F534A);
        let json_pad = (4 - json_len % 4) % 4;
        let bin_start = 12 + 8 + json_len + json_pad;
        assert_eq!(bin_start % 4, 0);
        let bin_len = le32(&bytes, bin_start) as usize;
        assert_eq!(le32(&bytes, bin_start + 4), 0x004E4942);
        let bin_pad = (4 - bin_len % 4) % 4;
        assert_eq!(bytes.len(), bin_start + 8 + bin_len + bin_pad);
    }

    #[test]
    fn glb_json_chunk_parses_and_has_one_mesh_per_wall() {
        let bytes = write(&scene_with(3));
        let json = glb_json_chunk(&bytes);
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
        let json = glb_json_chunk(&bytes);
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
        let json = glb_json_chunk(&bytes);
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
    fn glb_chunk_header_is_length_then_type() {
        let bytes = write(&scene_with(1));
        let json_len = le32(&bytes, 12);
        assert_eq!(le32(&bytes, 16), 0x4E4F534A);
        let json_pad = (4 - json_len as usize % 4) % 4;
        let bin_start = 12 + 8 + json_len as usize + json_pad;
        let bin_len = le32(&bytes, bin_start);
        assert_eq!(le32(&bytes, bin_start + 4), 0x004E4942);
    }

    #[test]
    fn glb_buffers_use_byte_length() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        assert!(json["buffers"][0].get("byteLength").is_some());
        assert!(json["buffers"][0].get("offset").is_none());
        assert!(json["buffers"][0].get("length").is_none());
    }

    #[test]
    fn glb_buffer_views_use_byte_offset_and_length() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        for bv in json["bufferViews"].as_array().unwrap() {
            assert!(bv.get("byteOffset").is_some());
            assert!(bv.get("byteLength").is_some());
            assert!(bv.get("target").is_some());
            assert!(bv.get("offset").is_none());
            assert!(bv.get("length").is_none());
            assert!(bv.get("viewType").is_none());
        }
    }

    #[test]
    fn glb_accessors_reference_buffer_views() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        for acc in json["accessors"].as_array().unwrap() {
            assert!(acc.get("bufferView").is_some());
            assert!(acc.get("byteOffset").is_some());
            assert!(acc.get("buffer").is_none());
            assert!(acc.get("offset").is_none());
        }
    }

    #[test]
    fn glb_asset_declares_version_2_0() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        assert_eq!(json["asset"]["version"], "2.0");
        assert_eq!(json["asset"]["generator"], "CADCraft arch3d");
    }

    #[test]
    fn glb_json_chunk_is_space_padded() {
        let bytes = write(&scene_with(1));
        let json_len = le32(&bytes, 12) as usize;
        let json_pad = (4 - json_len % 4) % 4;
        for i in 0..json_pad {
            assert_eq!(bytes[20 + json_len + i], 0x20);
        }
    }

    #[test]
    fn empty_scene_writes_a_valid_empty_glb() {
        let scene = Scene3d::default();
        let bytes = write(&scene);
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(&bytes[4..8], &[0x02, 0x00, 0x00, 0x00]);
        let total = le32(&bytes, 8);
        assert_eq!(total as usize, bytes.len());
        let json = glb_json_chunk(&bytes);
        assert_eq!(json["meshes"].as_array().and_then(|a| Some(a.len())), Some(0));
        assert_eq!(json["scenes"][0]["nodes"].as_array().and_then(|a| Some(a.len())), Some(0));
        // No root node when the scene is empty.
        assert_eq!(json["nodes"].as_array().and_then(|a| Some(a.len())), Some(0));
        let json_len = le32(&bytes, 12) as usize;
        let json_pad = (4 - json_len % 4) % 4;
        let bin_start = 12 + 8 + json_len + json_pad;
        assert_eq!(le32(&bytes, bin_start + 4), 0x004E4942);
        assert_eq!(bytes.len(), bin_start + 8);
    }

    #[test]
    fn glb_wall_names_are_unchanged() {
        let scene = scene_with(3);
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let meshes = json["meshes"].as_array().unwrap();
        assert_eq!(meshes[0]["name"], "wall-w1");
        assert_eq!(meshes[1]["name"], "wall-w2");
        assert_eq!(meshes[2]["name"], "wall-w3");
    }

    #[test]
    fn glb_names_opening_meshes_by_component() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "glass".into();
            m.object = ArchId(2);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "frame".into();
            m.object = ArchId(0);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "leaf".into();
            m.object = ArchId(1);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "casing".into();
            m.object = ArchId(2);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "sill".into();
            m.object = ArchId(2);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "header".into();
            m.object = ArchId(0);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let meshes = json["meshes"].as_array().unwrap();
        assert_eq!(meshes[0]["name"], "glass-o2");
        assert_eq!(meshes[1]["name"], "frame-o0");
        assert_eq!(meshes[2]["name"], "leaf-o1");
        assert_eq!(meshes[3]["name"], "casing-o2");
        assert_eq!(meshes[4]["name"], "sill-o2");
        assert_eq!(meshes[5]["name"], "header-o0");
    }

    #[test]
    fn glb_glass_material_blends() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "glass".into();
            m.object = ArchId(2);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mat = &json["materials"][0];
        let pbr = &mat["pbrMetallicRoughness"];
        let base = &pbr["baseColorFactor"];
        assert_eq!(base[3].as_f64().unwrap(), 0.35);
        assert_eq!(pbr["metallicFactor"].as_f64().unwrap(), 0.0);
        assert_eq!(pbr["roughnessFactor"].as_f64().unwrap(), 0.05);
        assert_eq!(mat["alphaMode"], "BLEND");
    }

    #[test]
    fn glb_is_byte_deterministic_with_openings() {
        let mut scene = scene_with(2);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "glass".into();
            m.object = ArchId(5);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "frame".into();
            m.object = ArchId(3);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "leaf".into();
            m.object = ArchId(4);
            m
        });
        let a = write(&scene);
        let b = write(&scene);
        assert_eq!(a, b);
    }

    #[test]
    fn glb_nodes_include_door_window_and_glass() {
        let mut scene = scene_with(0);
        // Door frame mesh (o0).
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "door-frame".into();
            m.object = ArchId(0);
            m
        });
        // Window glass mesh (o1) — verifies {material}-o{number} naming.
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "window-glass".into();
            m.object = ArchId(1);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let nodes = json["nodes"].as_array().unwrap();
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap().to_string()).collect();
        assert!(mesh_names.contains(&"door-frame-o0".to_string()), "missing door-frame-o0: {:?}", mesh_names);
        assert!(mesh_names.contains(&"window-glass-o1".to_string()), "missing window-glass-o1: {:?}", mesh_names);
        // Nodes mirror mesh names (the first node is the root, the rest are meshes).
        let node_names: Vec<String> = nodes.iter().skip(1).map(|n| n["name"].as_str().unwrap().to_string()).collect();
        assert!(node_names.contains(&"door-frame-o0".to_string()), "node missing door-frame-o0: {:?}", node_names);
        assert!(node_names.contains(&"window-glass-o1".to_string()), "node missing window-glass-o1: {:?}", node_names);
        // Glass material: alphaMode BLEND and baseColorFactor[3] < 0.6.
        let materials = json["materials"].as_array().unwrap();
        assert!(
            materials.iter().any(|m| m["name"] == "window-glass"
                && m["alphaMode"] == "BLEND"
                && m["pbrMetallicRoughness"]["baseColorFactor"][3].as_f64().unwrap() < 0.6),
            "window-glass material missing BLEND/alpha"
        );
    }

    #[test]
    fn glb_names_roof_nodes() {
        let bytes = write(&scene_with_roof());
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(mesh_names, vec!["roof-r0p0", "roof-r0p1", "roof-r0-fascia", "roof-r0-soffit"]);
        // The first node is the root; the rest are the per-mesh nodes.
        let node_names: Vec<String> = json["nodes"].as_array().unwrap().iter().skip(1).filter_map(|n| n["name"].as_str()).map(String::from).collect();
        assert_eq!(node_names, vec!["roof-r0p0", "roof-r0p1", "roof-r0-fascia", "roof-r0-soffit"]);
    }

    #[test]
    fn glb_roof_material_is_distinct() {
        let scene = Scene3d {
            meshes: vec![wall_mesh(), {
                let mut m = wall_mesh();
                m.material = "roof".into();
                m.object = ArchId(0);
                m
            }],
            ..Default::default()
        };
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mats = json["materials"].as_array().unwrap();
        assert_eq!(mats.len(), 2);
        let roof = mats.iter().find(|m| m["name"] == "roof").unwrap();
        let pbr = &roof["pbrMetallicRoughness"];
        let base = &pbr["baseColorFactor"];
        assert!(base[0].as_f64().unwrap() > 0.5 && base[1].as_f64().unwrap() < 0.5 && base[2].as_f64().unwrap() < 0.5, "roof albedo not reddish");
        assert_eq!(base[3].as_f64().unwrap(), 1.0);
        assert_eq!(pbr["metallicFactor"].as_f64().unwrap(), 0.0);
        assert_eq!(pbr["roughnessFactor"].as_f64().unwrap(), 0.9);
        // Distinct from the wall entry.
        let wall = mats.iter().find(|m| m["name"] == "wall").unwrap();
        assert_eq!(wall["pbrMetallicRoughness"]["roughnessFactor"].as_f64().unwrap(), 1.0);
        assert_ne!(wall["pbrMetallicRoughness"]["baseColorFactor"].as_array().unwrap(), base.as_array().unwrap());
    }

    #[test]
    fn glb_is_deterministic_with_a_roof() {
        let scene = scene_with_roof();
        let a = write(&scene);
        let b = write(&scene);
        assert_eq!(a, b);
    }

    #[test]
    fn glb_names_multiple_roofs_with_interleaved_planes() {
        let mut scene = scene_with(0);
        // Roof 0, plane 0
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m
        });
        // Roof 1, plane 0
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(1);
            m
        });
        // Roof 0, plane 1
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m
        });
        // Roof 1, plane 1
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(1);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(mesh_names, vec!["roof-r0p0", "roof-r1p0", "roof-r0p1", "roof-r1p1"]);
    }

    #[test]
    fn glb_roof_with_nan_positions_is_valid() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m.positions[0][0] = f32::NAN;
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        assert_eq!(json["meshes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn glb_roof_with_huge_positions_is_valid() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m.positions[0][0] = 1e10;
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        assert_eq!(json["meshes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn glb_roof_with_zero_planes_creates_no_meshes() {
        let scene = scene_with(0);
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert!(mesh_names.is_empty());
    }

    #[test]
    fn glb_roof_material_only_created_when_roof_mesh_exists() {
        let scene = scene_with(0);
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let materials: Vec<String> = json["materials"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert!(!materials.contains(&"roof".to_string()));
    }

    /// Verify that both chunk length fields carry the PADDED length (a multiple
    /// of 4) and that the BIN chunk header sits exactly at 12 + 8 + jsonChunkLen.
    /// Four successive model sizes are tested so every alignment residue (0‥3)
    /// is exercised, guaranteeing that the padding code path is hit.
    #[test]
    fn glb_chunk_lengths_are_padded_multiples_of_four() {
        for n in 0..=3 {
            let bytes = write(&scene_with(n));
            let json_chunk_len = le32(&bytes, 12) as usize;
            assert_eq!(json_chunk_len % 4, 0, "n={n}: JSON chunkLength {json_chunk_len} is not 4-byte aligned");
            // BIN chunk must start at 12 (header) + 8 (JSON chunk header) + json_chunk_len.
            let bin_offset = 12 + 8 + json_chunk_len;
            assert!(bin_offset + 8 <= bytes.len(), "n={n}: bin_offset {bin_offset} is out of bounds (file len {})", bytes.len());
            assert_eq!(le32(&bytes, bin_offset + 4), 0x004E4942, "n={n}: chunk at byte {bin_offset} is not a BIN chunk");
            let bin_chunk_len = le32(&bytes, bin_offset) as usize;
            assert_eq!(bin_chunk_len % 4, 0, "n={n}: BIN chunkLength {bin_chunk_len} is not 4-byte aligned");
        }
    }

    /// The 12-byte GLB header's `length` field must equal the total byte count.
    #[test]
    fn glb_header_length_is_the_file_length() {
        for n in 0..=3 {
            let bytes = write(&scene_with(n));
            let header_len = le32(&bytes, 8) as usize;
            assert_eq!(header_len, bytes.len(), "n={n}: header length {header_len} != actual file length {}", bytes.len());
        }
    }

    /// Walk the GLB chunk-by-chunk from the 12-byte header to EOF, verifying
    /// each chunk length is a multiple of 4 and that exactly 2 chunks
    /// (JSON + BIN) are found.
    #[test]
    fn glb_chunks_can_be_walked_from_header() {
        let bytes = write(&scene_with(2));
        let total = le32(&bytes, 8) as usize;
        assert_eq!(total, bytes.len());

        let mut offset = 12;
        let mut chunk_count = 0;
        while offset < bytes.len() {
            let chunk_len = le32(&bytes, offset) as usize;
            let chunk_type = le32(&bytes, offset + 4);
            assert_eq!(chunk_len % 4, 0, "chunk {chunk_count} length {chunk_len} not multiple of 4");
            offset += 8 + chunk_len;
            chunk_count += 1;
        }
        assert_eq!(chunk_count, 2, "expected exactly 2 chunks (JSON + BIN)");
        assert_eq!(offset, bytes.len(), "chunk walking did not reach end of file");
    }

    /// Extract the JSON chunk using only the declared chunk length from the
    /// header and verify it parses as valid JSON.
    #[test]
    fn glb_json_chunk_content_is_valid_when_extracted_by_declared_length() {
        let bytes = write(&scene_with(3));
        let json_len = le32(&bytes, 12) as usize;
        let json_data = &bytes[20..20 + json_len];
        let json_str = std::str::from_utf8(json_data).expect("JSON chunk is not valid UTF-8");
        let value: Value = serde_json::from_str(json_str).expect("JSON chunk is not valid JSON");
        assert_eq!(value["asset"]["version"], "2.0");
        assert!(value.get("meshes").is_some());
    }

    /// Verify chunk structure holds for a larger scene (100 meshes).
    #[test]
    fn glb_large_scene_chunk_structure_is_valid() {
        let bytes = write(&scene_with(100));
        let total = le32(&bytes, 8) as usize;
        assert_eq!(total, bytes.len());

        let json_len = le32(&bytes, 12) as usize;
        assert_eq!(json_len % 4, 0);

        let bin_start = 12 + 8 + json_len;
        let bin_len = le32(&bytes, bin_start) as usize;
        assert_eq!(bin_len % 4, 0);
        assert_eq!(bin_start + 8 + bin_len, bytes.len());
    }

    /// Verify the declared BIN chunk length matches the actual binary data
    /// written (computed from mesh attribute sizes).
    #[test]
    fn glb_bin_chunk_length_matches_actual_bin_data() {
        let n = 5;
        let bytes = write(&scene_with(n));
        let json_len = le32(&bytes, 12) as usize;
        let bin_start = 12 + 8 + json_len;
        let bin_len = le32(&bytes, bin_start) as usize;

        // Each wall mesh contributes: 3*12 (pos) + 3*12 (norm) + 3*8 (uv) + 3*4 (idx) = 108 bytes
        let expected_bin = n * 108;
        assert_eq!(bin_len, expected_bin, "BIN chunk length {bin_len} != expected {expected_bin}");
    }

    /// Every primitive attribute key is a valid glTF 2.0 semantic: one of the
    /// built-in names or an application-specific name starting with "_".
    #[test]
    fn glb_attribute_names_are_gltf_semantics() {
        let bytes = write(&scene_with(2));
        let json = glb_json_chunk(&bytes);
        let attrs = json["meshes"].as_array().unwrap();
        for prim in attrs {
            let attributes = prim["primitives"][0]["attributes"].as_object().unwrap();
            for key in attributes.keys() {
                let builtin =
                    matches!(key.as_str(), "POSITION" | "NORMAL" | "TANGENT" | "TEXCOORD_0" | "TEXCOORD_1" | "COLOR_0" | "JOINTS_0" | "WEIGHTS_0");
                assert!(builtin || key.starts_with('_'), "non-gltf attribute {key}");
            }
        }
    }

    /// The scene is Y-up: a single root node carries the -90°-about-X rotation
    /// and the per-mesh nodes are its children.
    #[test]
    fn glb_scene_is_y_up() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        let root = root_node(&json).expect("root node");
        assert_eq!(root["name"], "arch3d-root");
        let rot = root["rotation"].as_array().unwrap();
        assert_eq!(rot[0].as_f64().unwrap(), -0.7071068);
        assert_eq!(rot[1].as_f64().unwrap(), 0.0);
        assert_eq!(rot[2].as_f64().unwrap(), 0.0);
        assert_eq!(rot[3].as_f64().unwrap(), 0.7071068);
        let children = root["children"].as_array().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].as_u64().unwrap(), 1);
        // scenes[0].nodes references only the root.
        assert_eq!(json["scenes"][0]["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(json["scenes"][0]["nodes"][0].as_u64().unwrap(), 0);
    }

    /// The top-level JSON carries no stray "version" key; the version lives in
    /// asset.version per glTF 2.0.
    #[test]
    fn glb_json_has_no_stray_top_level_version() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        assert!(json.get("version").is_none(), "stray top-level version");
        assert_eq!(json["asset"]["version"], "2.0");
    }

    #[test]
    fn glb_roof_material_shared_across_roofs() {
        let mut scene = scene_with(0);
        // Roof 0
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m
        });
        // Roof 1
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(1);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let materials: Vec<String> = json["materials"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(materials, vec!["roof"]);
    }

    /// An empty scene has no root node and no scene node references.
    #[test]
    fn glb_empty_scene_has_empty_node_list() {
        let scene = Scene3d::default();
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 0, "empty scene should have no nodes");
        let scene_nodes = json["scenes"][0]["nodes"].as_array().unwrap();
        assert_eq!(scene_nodes.len(), 0, "empty scene should have no scene node references");
    }

    /// The root node carries only name, rotation and children — no mesh or
    /// translation fields (identity translations were dropped per spec).
    #[test]
    fn glb_root_node_has_no_mesh_or_translation() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        let root = root_node(&json).expect("root node");
        assert!(root.get("mesh").is_none(), "root node should not have a mesh field");
        assert!(root.get("translation").is_none(), "root node should not have a translation field");
        assert!(root.get("name").is_some(), "root node should have a name");
        assert!(root.get("rotation").is_some(), "root node should have a rotation");
        assert!(root.get("children").is_some(), "root node should have children");
    }

    /// No node in the scene carries an identity translation field; the spec
    /// requires all identity translations to be dropped.
    #[test]
    fn glb_all_nodes_lack_identity_translation() {
        let bytes = write(&scene_with(3));
        let json = glb_json_chunk(&bytes);
        let nodes = json["nodes"].as_array().unwrap();
        for node in nodes {
            let name = node["name"].as_str().unwrap_or("<unnamed>");
            assert!(node.get("translation").is_none(), "node {name} should not have a translation field");
        }
    }

    /// Every primitive of every mesh type (wall, roof, glass, frame, etc.) uses
    /// only valid glTF 2.0 attribute semantics.
    #[test]
    fn glb_attribute_names_valid_across_mesh_types() {
        let mut scene = scene_with(1);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "glass".into();
            m.object = ArchId(5);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "frame".into();
            m.object = ArchId(6);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let meshes = json["meshes"].as_array().unwrap();
        for mesh in meshes {
            let mesh_name = mesh["name"].as_str().unwrap_or("<unnamed>");
            let prims = mesh["primitives"].as_array().unwrap();
            for prim in prims {
                let attrs = prim["attributes"].as_object().unwrap();
                for key in attrs.keys() {
                    let builtin = matches!(
                        key.as_str(),
                        "POSITION" | "NORMAL" | "TANGENT" | "TEXCOORD_0" | "TEXCOORD_1" | "COLOR_0" | "JOINTS_0" | "WEIGHTS_0"
                    );
                    assert!(builtin || key.starts_with('_'), "non-gltf attribute {key} on mesh {mesh_name}");
                }
            }
        }
    }

    /// For any non-empty scene, scenes[0].nodes references only the root node
    /// at index 0; per-mesh nodes are children of the root, not scene-level.
    #[test]
    fn glb_scenes_nodes_always_references_root_only() {
        let bytes = write(&scene_with(5));
        let json = glb_json_chunk(&bytes);
        let scene_nodes = json["scenes"][0]["nodes"].as_array().unwrap();
        assert_eq!(scene_nodes.len(), 1, "scene should reference only the root node");
        assert_eq!(scene_nodes[0].as_u64().unwrap(), 0, "scene should reference root node at index 0");
    }

    /// Buffer views use ARRAY_BUFFER (34962) for attributes and
    /// ELEMENT_ARRAY_BUFFER (34963) for indices.
    #[test]
    fn glb_buffer_view_targets_are_correct() {
        let bytes = write(&scene_with(2));
        let json = glb_json_chunk(&bytes);
        let views = json["bufferViews"].as_array().unwrap();
        assert_eq!(views.len(), 8);
        for i in 0..8 {
            let target = views[i]["target"].as_u64().unwrap();
            if i % 4 == 3 {
                assert_eq!(target, 34963, "indices buffer view target wrong");
            } else {
                assert_eq!(target, 34962, "attribute buffer view target wrong");
            }
        }
    }

    /// The material list contains one entry per distinct material name.
    #[test]
    fn glb_material_count_matches_distinct_materials() {
        let mut scene = scene_with(2);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "glass".into();
            m.object = ArchId(5);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "glass".into();
            m.object = ArchId(6);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "frame".into();
            m.object = ArchId(7);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let materials = json["materials"].as_array().unwrap();
        assert_eq!(materials.len(), 3);
    }

    /// A non-empty scene has one root node plus one node per mesh.
    #[test]
    fn glb_node_count_is_one_plus_mesh_count() {
        let bytes = write(&scene_with(4));
        let json = glb_json_chunk(&bytes);
        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 5);
    }

    /// Glass detection works for material names containing "glass" at any position.
    #[test]
    fn glb_glass_detection_for_various_names() {
        let mut scene = scene_with(0);
        for name in ["glass", "window-glass", "glass-panel", "tinted-glass"] {
            scene.meshes.push({
                let mut m = wall_mesh();
                m.material = name.into();
                m.object = ArchId(0);
                m
            });
        }
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let materials = json["materials"].as_array().unwrap();
        for mat in materials {
            assert_eq!(mat["alphaMode"], "BLEND", "material {} should be BLEND", mat["name"]);
            let alpha = mat["pbrMetallicRoughness"]["baseColorFactor"][3].as_f64().unwrap();
            assert_eq!(alpha, 0.35, "material {} alpha should be 0.35", mat["name"]);
        }
    }

    /// Non-glass materials remain opaque (no alphaMode, alpha 1.0).
    #[test]
    fn glb_non_glass_materials_are_opaque() {
        let mut scene = scene_with(0);
        for name in ["wall", "frame", "leaf", "casing", "sill", "header", "roof"] {
            scene.meshes.push({
                let mut m = wall_mesh();
                m.material = name.into();
                m.object = ArchId(0);
                m
            });
        }
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let materials = json["materials"].as_array().unwrap();
        for mat in materials {
            assert!(mat.get("alphaMode").is_none(), "material {} should not have alphaMode", mat["name"]);
            let alpha = mat["pbrMetallicRoughness"]["baseColorFactor"][3].as_f64().unwrap();
            assert_eq!(alpha, 1.0, "material {} alpha should be 1.0", mat["name"]);
        }
    }

    /// Every buffer view's byteOffset is a multiple of 4 (4-byte alignment).
    #[test]
    fn glb_buffer_views_are_4_byte_aligned() {
        let bytes = write(&scene_with(3));
        let json = glb_json_chunk(&bytes);
        for bv in json["bufferViews"].as_array().unwrap() {
            let offset = bv["byteOffset"].as_u64().unwrap();
            assert_eq!(offset % 4, 0, "buffer view byteOffset {} not 4-byte aligned", offset);
        }
    }

    /// Accessor type and componentType match the glTF 2.0 spec for each attribute.
    #[test]
    fn glb_accessor_types_and_component_types() {
        let bytes = write(&scene_with(1));
        let json = glb_json_chunk(&bytes);
        let accessors = json["accessors"].as_array().unwrap();
        // 4 accessors per mesh: POSITION(VEC3/5126), NORMAL(VEC3/5126), TEXCOORD_0(VEC2/5126), INDICES(SCALAR/5125)
        assert_eq!(accessors.len(), 4);
        assert_eq!(accessors[0]["type"], "VEC3");
        assert_eq!(accessors[0]["componentType"], 5126);
        assert_eq!(accessors[1]["type"], "VEC3");
        assert_eq!(accessors[1]["componentType"], 5126);
        assert_eq!(accessors[2]["type"], "VEC2");
        assert_eq!(accessors[2]["componentType"], 5126);
        assert_eq!(accessors[3]["type"], "SCALAR");
        assert_eq!(accessors[3]["componentType"], 5125);
    }

    /// The buffer's byteLength matches the BIN chunk's declared length.
    #[test]
    fn glb_buffer_byte_length_matches_bin_chunk() {
        let bytes = write(&scene_with(2));
        let json = glb_json_chunk(&bytes);
        let buffer_len = json["buffers"][0]["byteLength"].as_u64().unwrap();
        let json_len = le32(&bytes, 12) as usize;
        let bin_start = 12 + 8 + json_len;
        let bin_len = le32(&bytes, bin_start) as u64;
        assert_eq!(buffer_len, bin_len, "buffer byteLength {} != BIN chunk length {}", buffer_len, bin_len);
    }

    /// Every node's mesh index is within the range of the meshes array.
    #[test]
    fn glb_node_mesh_indices_are_valid() {
        let bytes = write(&scene_with(3));
        let json = glb_json_chunk(&bytes);
        let mesh_count = json["meshes"].as_array().unwrap().len();
        for node in json["nodes"].as_array().unwrap().iter().skip(1) {
            let mesh_idx = node["mesh"].as_u64().unwrap();
            assert!(mesh_idx < mesh_count as u64, "node mesh index {} >= mesh count {}", mesh_idx, mesh_count);
        }
    }

    /// Every buffer view references buffer 0.
    #[test]
    fn glb_all_buffer_views_reference_buffer_zero() {
        let bytes = write(&scene_with(2));
        let json = glb_json_chunk(&bytes);
        for bv in json["bufferViews"].as_array().unwrap() {
            assert_eq!(bv["buffer"].as_u64().unwrap(), 0, "buffer view does not reference buffer 0");
        }
    }

    #[test]
    fn glb_names_stair_nodes() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(42);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert!(mesh_names.contains(&"stair-s42".to_string()), "missing stair-s42: {:?}", mesh_names);

        let node_names: Vec<String> = json["nodes"].as_array().unwrap().iter().skip(1).filter_map(|n| n["name"].as_str()).map(String::from).collect();
        assert!(node_names.contains(&"stair-s42".to_string()), "node missing stair-s42: {:?}", node_names);
    }

    #[test]
    fn glb_names_railing_and_deck_nodes() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(7);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(9);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert!(mesh_names.contains(&"railing-ra7".to_string()), "missing railing-ra7: {:?}", mesh_names);
        assert!(mesh_names.contains(&"deck-r9".to_string()), "missing deck-r9: {:?}", mesh_names);

        let node_names: Vec<String> = json["nodes"].as_array().unwrap().iter().skip(1).filter_map(|n| n["name"].as_str()).map(String::from).collect();
        assert!(node_names.contains(&"railing-ra7".to_string()), "node missing railing-ra7: {:?}", node_names);
        assert!(node_names.contains(&"deck-r9".to_string()), "node missing deck-r9: {:?}", node_names);
    }

    #[test]
    fn glb_multiple_stairs_railings_decks_interleaved() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(1);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(1);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(1);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(2);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(2);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(2);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(mesh_names, vec!["stair-s1", "railing-ra1", "deck-r1", "stair-s2", "railing-ra2", "deck-r2"]);

        let node_names: Vec<String> = json["nodes"].as_array().unwrap().iter().skip(1).filter_map(|n| n["name"].as_str()).map(String::from).collect();
        assert_eq!(node_names, vec!["stair-s1", "railing-ra1", "deck-r1", "stair-s2", "railing-ra2", "deck-r2"]);
    }

    #[test]
    fn glb_stair_railing_deck_zero_id() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(0);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(0);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(0);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(mesh_names, vec!["stair-s0", "railing-ra0", "deck-r0"]);

        let node_names: Vec<String> = json["nodes"].as_array().unwrap().iter().skip(1).filter_map(|n| n["name"].as_str()).map(String::from).collect();
        assert_eq!(node_names, vec!["stair-s0", "railing-ra0", "deck-r0"]);
    }

    #[test]
    fn glb_stair_railing_deck_large_id() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(65535);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(65536);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(1048576);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let mesh_names: Vec<String> = json["meshes"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(mesh_names, vec!["stair-s65535", "railing-ra65536", "deck-r1048576"]);

        let node_names: Vec<String> = json["nodes"].as_array().unwrap().iter().skip(1).filter_map(|n| n["name"].as_str()).map(String::from).collect();
        assert_eq!(node_names, vec!["stair-s65535", "railing-ra65536", "deck-r1048576"]);
    }

    #[test]
    fn glb_mixed_scene_all_types_node_count() {
        let mut scene = scene_with(2);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(5);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(3);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(2);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "roof".into();
            m.object = ArchId(0);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let meshes = json["meshes"].as_array().unwrap();
        assert_eq!(meshes.len(), 6);
        let mesh_names: Vec<String> = meshes.iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert_eq!(mesh_names, vec!["wall-w1", "wall-w2", "stair-s5", "railing-ra3", "deck-r2", "roof-r0p0"]);

        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 7);
    }

    #[test]
    fn glb_stair_railing_deck_have_pbr_materials() {
        let mut scene = scene_with(0);
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "stair".into();
            m.object = ArchId(1);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "railing".into();
            m.object = ArchId(1);
            m
        });
        scene.meshes.push({
            let mut m = wall_mesh();
            m.material = "deck".into();
            m.object = ArchId(1);
            m
        });
        let bytes = write(&scene);
        let json = glb_json_chunk(&bytes);
        let materials = json["materials"].as_array().unwrap();
        assert_eq!(materials.len(), 3);
        for mat in materials {
            let name = mat["name"].as_str().unwrap();
            let pbr = &mat["pbrMetallicRoughness"];
            let base = &pbr["baseColorFactor"];
            assert_eq!(base.as_array().unwrap().len(), 4, "material {} baseColorFactor must have 4 components", name);
            assert!(pbr.get("metallicFactor").is_some(), "material {} must have metallicFactor", name);
            assert!(pbr.get("roughnessFactor").is_some(), "material {} must have roughnessFactor", name);
            assert_eq!(base[3].as_f64().unwrap(), 1.0, "material {} should be opaque", name);
            assert!(mat.get("alphaMode").is_none(), "material {} should not have alphaMode", name);
        }
        let mat_names: Vec<String> = materials.iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        assert!(mat_names.contains(&"stair".to_string()));
        assert!(mat_names.contains(&"railing".to_string()));
        assert!(mat_names.contains(&"deck".to_string()));
    }
}
