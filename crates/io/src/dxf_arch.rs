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
use serde::{Deserialize, Serialize};

/// Named-object-dictionary key of the architectural model XRECORD.
pub(crate) const ARCH_KEY: &str = "CADCRAFT_ARCH";

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
    if p.model.walls.len() > cadcraft_doc::arch::MAX_WALLS {
        return None;
    }
    Some(p.model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{ArchModel, Wall};
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
        let Some(chunks) = arch_chunks(&m) else {
            assert!(false, "a one-wall model must write an XRECORD");
        };
        assert!(!chunks.is_empty(), "a one-wall model must write an XRECORD");
        let text: String = chunks.iter().map(str::as_str).collect();
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
            Some(v) => v,
            None => assert!(false, "own model must serialise"),
        };
        if let Some(obj) = obj.as_object_mut() {
            obj.remove("next_id");
            obj.remove("levels");
            obj.remove("types");
        }
        let text: String = match serde_json::to_string(&serde_json::json!({ "version": 1, "model": obj })) {
            Some(t) => t,
            None => assert!(false, "own model must serialise"),
        };
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
}
