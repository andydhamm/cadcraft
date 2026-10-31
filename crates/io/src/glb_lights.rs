//! `KHR_lights_punctual` for the GLB (phase 10): one glTF light per scene light (spots with their
//! cone, intensity in candela as glTF wants for punctual lights) and one node per light under the
//! root, named `light-lf{n}-source`. Filled by `p10-io-glb-lights`.

use cadcraft_arch3d::Scene3d;
use cadcraft_doc::arch::LightKind;
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

/// The extension's `lights` array and one node per light (`{"name", "translation",
/// "rotation"?, "extensions": {"KHR_lights_punctual": {"light": i}}}`), both in
/// `scene.lights` order; empty when the scene has no lights.
pub(crate) fn punctual_lights(scene: &Scene3d) -> (Vec<Value>, Vec<Value>) {
    let mut lights = Vec::new();
    let mut nodes = Vec::new();

    for light in &scene.lights {
        if !light_valid(light) {
            continue;
        }

        let name = format!("lf{}", light.id.0);
        let mut def = json!({
            "name": name,
            "type": if light.kind == LightKind::Spot { "spot" } else { "point" },
            "color": [light.color[0], light.color[1], light.color[2]],
            "intensity": light.intensity,
        });

        if light.kind == LightKind::Spot {
            let inner = cone_angle(light.cos_inner);
            let outer = (cone_angle(light.cos_outer)).min(std::f64::consts::PI / 2.0);
            if !inner.is_finite() || !outer.is_finite() || inner > outer {
                continue;
            }
            def["spot"] = json!({
                "innerConeAngle": inner,
                "outerConeAngle": outer,
            });
        }

        let light_index = lights.len();
        lights.push(def);

        let mut node = json!({
            "name": format!("light-{}-source", name),
            "translation": [light.position.x, light.position.y, light.position.z],
            "extensions": {
                "KHR_lights_punctual": {
                    "light": light_index
                }
            }
        });

        if let Some(rotation) = quaternion_from_direction(light.direction) {
            node["rotation"] = json!(rotation);
        }
        nodes.push(node);
    }

    (lights, nodes)
}

fn light_valid(light: &cadcraft_arch3d::light::SceneLight) -> bool {
    if !light.position.is_finite() || !light.direction.is_finite() || !light.color.iter().all(|c| c.is_finite()) {
        return false;
    }
    if !light.intensity.is_finite() || light.intensity < 0.0 {
        return false;
    }
    let direction = light.direction.normalized();
    if !direction.is_finite() || direction.len() < 1e-12 {
        return false;
    }
    match light.kind {
        LightKind::Point => true,
        LightKind::Spot => {
            light.cos_inner.is_finite()
                && light.cos_outer.is_finite()
                && (-1.0..=1.0).contains(&light.cos_inner)
                && (-1.0..=1.0).contains(&light.cos_outer)
                && light.cos_inner >= light.cos_outer
        }
    }
}

fn cone_angle(cosine: f64) -> f64 {
    let normalized = cosine.clamp(-1.0, 1.0);
    if !normalized.is_finite() {
        return f64::NAN;
    }
    normalized.acos()
}

fn quaternion_from_direction(direction: Vec3) -> Option<[f64; 4]> {
    let dir = direction.normalized();
    if !dir.is_finite() || dir.len() < 1e-12 {
        return None;
    }

    let from = Vec3::new(0.0, 0.0, -1.0);
    if (from - dir).len() < 1e-12 {
        return None;
    }
    if (from + dir).len() < 1e-12 {
        return Some([1.0, 0.0, 0.0, 0.0]);
    }

    let axis = from.cross(dir);
    let axis_len = axis.len();
    if !axis_len.is_finite() || axis_len < 1e-12 {
        return None;
    }

    let axis = axis * (1.0 / axis_len);
    let dot = (from.dot(dir)).clamp(-1.0, 1.0);
    let angle = dot.acos();
    let half = angle / 2.0;
    let s = half.sin();
    let w = half.cos();
    Some([axis.x * s, axis.y * s, axis.z * s, w])
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_arch3d::{Mesh, Scene3d};
    use cadcraft_doc::arch::{ArchId, LightKind};
    use cadcraft_geom::Vec3;
    use serde_json::Value;

    fn scene_light(
        id: u32,
        kind: LightKind,
        position: Vec3,
        direction: Vec3,
        intensity: f64,
        color: [f64; 3],
        cos_inner: f64,
        cos_outer: f64,
    ) -> cadcraft_arch3d::light::SceneLight {
        cadcraft_arch3d::light::SceneLight { id: ArchId(id), kind, position, direction, intensity, color, cos_inner, cos_outer, ..Default::default() }
    }

    fn glb_json_chunk(bytes: &[u8]) -> Value {
        let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        let start = 20;
        serde_json::from_slice(&bytes[start..start + json_len]).unwrap_or_default()
    }

    fn rotate_quat(v: Vec3, q: [f64; 4]) -> Vec3 {
        let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
        let norm = (x * x + y * y + z * z + w * w).sqrt();
        let (x, y, z, w) = if norm > 0.0 { (x / norm, y / norm, z / norm, w / norm) } else { (0.0, 0.0, 0.0, 1.0) };
        let qv = Vec3::new(x, y, z);
        let uv = qv.cross(v);
        let uuv = qv.cross(uv);
        let t = uv * (2.0 * w) + uuv * 2.0;
        v + t
    }

    #[test]
    fn punctual_spot_lights_carry_candela_and_cone() {
        let half = 30.0_f64.to_radians();
        let scene = Scene3d {
            lights: vec![
                scene_light(0, LightKind::Spot, Vec3::new(1.0, 1.0, 2.4), Vec3::new(0.0, 0.0, -1.0), 950.36, [1.0, 1.0, 1.0], half.cos(), half.cos()),
                scene_light(1, LightKind::Spot, Vec3::new(3.0, 1.0, 2.4), Vec3::new(0.0, 0.0, -1.0), 950.36, [1.0, 0.8, 0.7], half.cos(), half.cos()),
            ],
            ..Default::default()
        };

        let (lights, _) = punctual_lights(&scene);
        assert_eq!(lights.len(), 2);
        for light in &lights {
            assert_eq!(light["type"], "spot");
            assert!((light["intensity"].as_f64().unwrap_or(f64::NAN) - 950.36).abs() < 0.01);
            let spot = &light["spot"];
            assert!((spot["outerConeAngle"].as_f64().unwrap_or(f64::NAN) - std::f64::consts::PI / 6.0).abs() < 1e-9, "{spot}");
            assert!((spot["innerConeAngle"].as_f64().unwrap_or(f64::NAN) - std::f64::consts::PI / 6.0).abs() < 1e-9, "{spot}");
        }
    }

    #[test]
    fn punctual_light_nodes_face_their_aim() {
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let scene = Scene3d {
            lights: vec![scene_light(0, LightKind::Spot, Vec3::new(0.0, 0.0, 0.0), dir, 100.0, [1.0, 1.0, 1.0], 0.5, 0.5)],
            ..Default::default()
        };

        let (_, nodes) = punctual_lights(&scene);
        let node = &nodes[0];
        let rotation = node["rotation"].as_array().unwrap();
        let q = [
            rotation[0].as_f64().unwrap_or_default(),
            rotation[1].as_f64().unwrap_or_default(),
            rotation[2].as_f64().unwrap_or_default(),
            rotation[3].as_f64().unwrap_or_default(),
        ];
        let rotated = rotate_quat(Vec3::new(0.0, 0.0, -1.0), q);
        assert!((rotated.x - 1.0).abs() < 1e-9, "rotated={rotated:?}");
        assert!((rotated.y.abs()) < 1e-9);
        assert!((rotated.z.abs()) < 1e-9);
    }

    #[test]
    fn glb_declares_the_lights_extension() {
        let mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            uvs: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]],
            indices: vec![0, 1, 2],
            ..Mesh::new("wall", ArchId(1))
        };
        let half = 30.0_f64.to_radians();
        let scene = Scene3d {
            meshes: vec![mesh],
            lights: vec![scene_light(
                0,
                LightKind::Spot,
                Vec3::new(1.0, 1.0, 2.4),
                Vec3::new(0.0, 0.0, -1.0),
                950.36,
                [1.0, 1.0, 1.0],
                half.cos(),
                half.cos(),
            )],
            ..Default::default()
        };

        let bytes = crate::glb::write(&scene);
        let blocks = glb_json_chunk(&bytes);
        let extensions_used = blocks["extensionsUsed"].as_array().unwrap();
        assert!(extensions_used.iter().any(|v| v == "KHR_lights_punctual"));
        let root = blocks["nodes"][0].as_object().unwrap();
        let children = root["children"].as_array().unwrap();
        assert!(children.iter().any(|n| {
            let idx = n.as_u64().unwrap() as usize;
            blocks["nodes"][idx]["name"] == "light-lf0-source"
        }));
    }
}
