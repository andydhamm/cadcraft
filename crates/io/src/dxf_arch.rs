//! CADCraft architectural data on the DXF named-object dictionary.
//!
//! The `CADCRAFT_ARCH` XRECORD carries the whole [`ArchModel`](cadcraft_doc::ArchModel) as
//! version-tagged JSON, chunked exactly like the constraint record
//! ([`dxf_ext`](crate::dxf_ext)) so it fits the 250-byte XRECORD line limit. Proxy plan
//! geometry is written only on save and not read back while this payload is present — one
//! source of truth.
//!
//! Nothing in this module is wired into the reader or the writer yet; that is the next
//! segment.

use cadcraft_doc::ArchModel;
use cadcraft_dxf::Tag;
use cadcraft_geom::Vec2;
use serde::{Deserialize, Serialize};

/// Named-object-dictionary key of the architectural model XRECORD.
pub(crate) const ARCH_KEY: &str = "CADCRAFT_ARCH";

/// Layer of the save-time proxy plan polylines.
pub(crate) const ARCH_PROXY_LAYER: &str = "A-WALL";

/// `CADCRAFT` xdata marker identifying a proxy polyline, so the reader drops it.
pub(crate) const ARCH_PROXY_MARKER: &str = "ARCHWALL";

/// Rejects payloads larger than this before they are even parsed.
const MAX_ARCH_PAYLOAD: usize = 8 << 20;

/// The record serialized into the `CADCRAFT_ARCH` XRECORD.
///
/// `model` carries `#[serde(default)]`, so a payload without `next_id`, `levels` or `types`
/// still deserialises to their defaults.
#[derive(Serialize, Deserialize)]
struct Payload {
    version: u32,
    #[serde(default)]
    model: ArchModel,
}

/// JSON chunks (≤ 250 characters) for the architectural XRECORD.
///
/// Returns `None` when the model has no walls (nothing to write); otherwise the escaped,
/// chunked payload, exactly like [`dxf_ext::constraint_chunks`](crate::dxf_ext).
pub(crate) fn arch_chunks(m: &ArchModel) -> Option<Vec<String>> {
    if m.is_empty() {
        return None;
    }
    let json = serde_json::to_string(&Payload { version: 1, model: m.clone() }).ok()?;
    Some(crate::dxf_ext::json_chunks(&json))
}

/// Parse an architectural XRECORD payload; `None` when it is not a payload we understand.
pub(crate) fn parse_arch(text: &str) -> Option<ArchModel> {
    if text.len() > MAX_ARCH_PAYLOAD {
        return None;
    }
    let p: Payload = serde_json::from_str(text).ok()?;
    if p.version != 1 {
        return None;
    }
    if p.model.walls.len() > cadcraft_doc::MAX_WALLS {
        return None;
    }
    Some(p.model)
}

/// The four corners of a wall's poche ring (the offset centre-line rectangle), in order.
///
/// Returns `None` for a degenerate wall: a non-finite endpoint, `thickness <= 0`, or a
/// centre-line shorter than `1e-12`.
fn wall_ring(w: &cadcraft_doc::Wall) -> Option<Vec<Vec2>> {
    if !w.start.is_finite() || !w.end.is_finite() {
        return None;
    }
    if w.thickness <= 0.0 {
        return None;
    }
    let dir = w.end - w.start;
    let len = dir.len();
    if len < 1e-12 {
        return None;
    }
    let perp = dir.rotate(std::f64::consts::PI / 2.0) * (w.thickness / 2.0 / len);
    Some([w.start + perp, w.end + perp, w.end - perp, w.start - perp].into_iter().collect())
}

/// The DXF groups (after the common entity groups) of one closed LWPOLYLINE per wall poche
/// ring, on layer [`ARCH_PROXY_LAYER`], each tagged with the [`ARCH_PROXY_MARKER`] xdata and
/// the wall's id. Returns `Vec::new()` for an empty model; the output is capped at
/// [`cadcraft_doc::MAX_WALLS`] polylines.
pub(crate) fn proxy_polylines(m: &ArchModel) -> Vec<Vec<Tag>> {
    if m.is_empty() {
        return Vec::new();
    }
    m.walls
        .iter()
        .take(cadcraft_doc::MAX_WALLS)
        .filter_map(|w| wall_ring(w).map(|corners| (w, corners)))
        .map(|(w, corners)| {
            let mut t = Vec::new();
            t.push(Tag::i(90, 4));
            t.push(Tag::i(70, 1));
            for c in &corners {
                t.push(Tag::f(10, c.x));
                t.push(Tag::f(20, c.y));
            }
            t.push(Tag::s(1001, crate::dxf_ext::APP));
            t.push(Tag::s(1000, ARCH_PROXY_MARKER));
            t.push(Tag::s(1000, &w.id.to_string()));
            t
        })
        .collect()
}

/// True when the entity's `CADCRAFT` xdata carries the [`ARCH_PROXY_MARKER`] marker, i.e. the
/// record is a save-time proxy polyline to be dropped on read.
pub(crate) fn is_arch_proxy(tags: &[Tag]) -> bool {
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str().trim() == ARCH_PROXY_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_dxf, write_dxf};
    use cadcraft_doc::{ArchModel, Drawing, EntityKind, Wall};
    use cadcraft_geom::Vec2;

    /// A model with exactly one wall.
    fn one_wall_model() -> ArchModel {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m
    }

    #[test]
    fn arch_chunks_roundtrips_a_model() {
        let m = one_wall_model();
        let chunks = match arch_chunks(&m) {
            Some(c) => c,
            None => Vec::new(),
        };
        assert!(!chunks.is_empty(), "a one-wall model must write a non-empty XRECORD");
        let text = chunks.join("");
        assert_eq!(parse_arch(&text), Some(m));
    }

    #[test]
    fn arch_chunks_is_none_for_an_empty_model() {
        assert!(arch_chunks(&ArchModel::default()).is_none());
    }

    #[test]
    fn parse_arch_rejects_unknown_versions() {
        assert!(parse_arch(r#"{"version": 2, "model": { "walls": [] }}"#).is_none());
        assert!(parse_arch(r#"{"version": 0, "model": { "walls": [] }}"#).is_none());
    }

    #[test]
    fn parse_arch_rejects_oversize_payload() {
        let big = "a".repeat((8 << 20) + 1);
        assert!(parse_arch(&big).is_none());
    }

    #[test]
    fn parse_arch_accepts_model_missing_levels_types_next_id() {
        let m = one_wall_model();
        let mut obj: serde_json::Value = match serde_json::to_value(&m) {
            Ok(v) => v,
            Err(_) => serde_json::Value::Null,
        };
        if let Some(map) = obj.as_object_mut() {
            map.remove("next_id");
            map.remove("levels");
            map.remove("types");
        }
        let text: String = match serde_json::to_string(&serde_json::json!({ "version": 1, "model": obj.clone() })) {
            Ok(t) => t,
            Err(_) => String::new(),
        };
        assert!(!text.is_empty());
        assert!(parse_arch(&text).is_some());
    }

    #[test]
    fn hostile_arch_xrecord_never_panics() {
        for h in ["", "not json", "{}", "[]", "null"] {
            assert!(parse_arch(h).is_none(), "hostile input {h:?}");
        }
        // Version 1 without a model field falls back to the default (empty) model.
        if let Some(m) = parse_arch(r#"{"version": 1}"#) {
            assert!(m.is_empty());
        }
        assert!(parse_arch(&"a".repeat((8 << 20) + 1)).is_none());
    }

    #[test]
    fn proxy_graphics_are_written_and_not_double_read() {
        let mut d = Drawing::new_imperial();
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 0.2));
        let text = write_dxf(&d);
        assert!(text.contains("ARCHWALL"), "no ARCHWALL marker in DXF");
        assert!(text.contains("A-WALL"), "no A-WALL layer in DXF");
        let back = read_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.arch.walls.len(), 2, "arch walls lost on read");
        let mut count = 0;
        for e in back.model.iter() {
            if let EntityKind::LwPolyline(_) = &e.kind {
                count += e.common.xdata.iter().filter(|t| t.code == 1000 && t.value.trim() == ARCH_PROXY_MARKER).count();
            }
        }
        assert_eq!(count, 0, "model LWPOLYLINE carries ARCHWALL xdata");
    }

    #[test]
    fn proxy_rings_match_the_wall_poché() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let rings = proxy_polylines(&m);
        assert_eq!(rings.len(), 1, "expected one ring");
        let tags = &rings[0];
        let mut corners: Vec<(f64, f64)> = Vec::new();
        let mut i = 0;
        while i + 1 < tags.len() {
            if tags[i].code == 10 && tags[i + 1].code == 20 {
                corners.push((tags[i].f64(), tags[i + 1].f64()));
                i += 2;
            } else {
                i += 1;
            }
        }
        assert_eq!(corners.len(), 4, "expected four corners");
        let expected = vec![(0.0, 0.1), (10.0, 0.1), (10.0, -0.1), (0.0, -0.1)];
        for (got, want) in corners.iter().zip(expected.iter()) {
            assert!((got.0 - want.0).abs() < 1e-9, "x: {} != {}", got.0, want.0);
            assert!((got.1 - want.1).abs() < 1e-9, "y: {} != {}", got.1, want.1);
        }
    }

    #[test]
    fn degenerate_walls_write_no_proxy() {
        let mut d = Drawing::new_imperial();
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), 0.2));
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.0));
        let text = write_dxf(&d);
        assert!(!text.contains("ARCHWALL"), "degenerate walls write a proxy");
    }

    #[test]
    fn proxy_graphics_survive_a_foreign_reader() {
        let mut d = Drawing::new_imperial();
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let text1 = write_dxf(&d);
        let count1 = text1.matches("ARCHWALL").count();
        assert!(count1 > 0, "no ARCHWALL marker");
        let back = read_dxf(text1.as_bytes()).unwrap();
        let text2 = write_dxf(&back);
        let count2 = text2.matches("ARCHWALL").count();
        assert_eq!(count1, count2, "ARCHWALL count changed: {} -> {}", count1, count2);
    }
}
