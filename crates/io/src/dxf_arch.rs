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

/// `CADCRAFT` xdata marker identifying a wall proxy polyline, so the reader drops it.
pub(crate) const ARCH_PROXY_MARKER: &str = "ARCHWALL";

/// `CADCRAFT` xdata marker identifying a save-time opening proxy polyline, so the reader
/// drops it (the opening's model data is the one source of truth).
pub(crate) const ARCH_OPENING_MARKER: &str = "ARCHOPENING";

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
    let json = serde_json::to_string(&Payload { version: 2, model: m.clone() }).ok()?;
    Some(crate::dxf_ext::json_chunks(&json))
}

/// Parse an architectural XRECORD payload; `None` when it is not a payload we understand.
///
/// Accepts schema version 1 (no openings: v1 → empty openings) and version 2 (openings
/// present). Both are bounded — the wall count against [`MAX_WALLS`] and the opening count
/// against [`MAX_OPENINGS`] — so hostile payloads can not overflow allocations.
pub(crate) fn parse_arch(text: &str) -> Option<ArchModel> {
    if text.len() > MAX_ARCH_PAYLOAD {
        return None;
    }
    let p: Payload = serde_json::from_str(text).ok()?;
    if p.version != 1 && p.version != 2 {
        return None;
    }
    if p.model.walls.len() > cadcraft_doc::MAX_WALLS {
        return None;
    }
    let model = if p.version == 1 {
        let mut m = p.model;
        m.openings.clear();
        m
    } else {
        p.model
    };
    if model.openings.len() > cadcraft_doc::arch::MAX_OPENINGS {
        return None;
    }
    Some(model)
}

/// True when an entity's `CADCRAFT` xdata carries the [`ARCH_PROXY_MARKER`] or
/// [`ARCH_OPENING_MARKER`] marker, i.e. the record is a save-time proxy (wall or opening)
/// polyline to be dropped on read.
pub(crate) fn is_arch_proxy(tags: &[Tag]) -> bool {
    let markers = [ARCH_PROXY_MARKER, ARCH_OPENING_MARKER];
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && markers.iter().any(|m| t.str().trim() == *m))
}
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

/// The four corners of an opening's rough rectangle along the host wall, in the same corner
/// order as [`wall_ring`], offset by half the wall thickness.
///
/// The rough span is centred on `o.offset` (measured from the wall start in metres) and uses
/// `o.rough_width`, mirroring `arch::symbol::opening_symbol`'s rough geometry. Returns `None`
/// for a degenerate wall (as [`wall_ring`] does) or when any input is non-finite.
fn opening_ring(w: &cadcraft_doc::Wall, o: &cadcraft_doc::arch::Opening) -> Option<Vec<Vec2>> {
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
    let offset = if o.offset.is_finite() { o.offset } else { 0.0 };
    let width = if o.width.is_finite() && o.width > 0.0 { o.width } else { 0.9 };
    let rough_width = if o.rough_width.is_finite() && o.rough_width > 0.0 { o.rough_width } else { width + 0.12 };
    if !offset.is_finite() || !width.is_finite() || !rough_width.is_finite() {
        return None;
    }
    let rough_start = offset - (rough_width - width) / 2.0;
    let rough_end = rough_start + rough_width;
    if !rough_start.is_finite() || !rough_end.is_finite() {
        return None;
    }
    let along = dir.normalized();
    let half = along.perp() * (w.thickness / 2.0);
    Some(
        [
            w.start + along * rough_start + half,
            w.start + along * rough_end + half,
            w.start + along * rough_end - half,
            w.start + along * rough_start - half,
        ]
        .into_iter()
        .collect(),
    )
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

/// The DXF groups of one closed LWPOLYLINE per opening proxy ring, on the opening's own
/// layer (`A-DOOR` for doors/garage doors, `A-GLAZ` for windows), each tagged with the
/// [`ARCH_OPENING_MARKER`] xdata and the opening's label (`o{n}`). Returns `Vec::new()`
/// when there are no openings; the output is capped at
/// [`cadcraft_arch::MAX_OPENINGS`] polylines.
pub(crate) fn proxy_opening_polylines(m: &ArchModel) -> Vec<(String, Vec<Tag>)> {
    m.openings
        .iter()
        .take(cadcraft_doc::arch::MAX_OPENINGS)
        .filter_map(|o| {
            let symbol = cadcraft_arch::symbol::opening_symbol(m, o);
            let mut corners: Option<Vec<Vec2>> = symbol.as_ref().and_then(|s| s.polylines.first().cloned());
            if corners.as_ref().map_or(true, |c| c.is_empty()) {
                corners = m.wall(o.host).and_then(|w| opening_ring(w, o));
            }
            let Some(corners) = corners else {
                return None;
            };
            if corners.is_empty() {
                return None;
            }
            let mut t = Vec::new();
            t.push(Tag::i(90, 4));
            t.push(Tag::i(70, 1));
            for c in &corners {
                t.push(Tag::f(10, c.x));
                t.push(Tag::f(20, c.y));
            }
            t.push(Tag::s(1001, crate::dxf_ext::APP));
            t.push(Tag::s(1000, ARCH_OPENING_MARKER));
            t.push(Tag::s(1000, &format!("o{}", o.id)));
            Some((o.layer.clone(), t))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_dxf, write_dxf};
    use cadcraft_doc::arch::{Opening, OpeningKind};
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
        // Only schema versions 1 and 2 are understood; 3 and later are unknown.
        assert!(parse_arch(r#"{"version": 3, "model": { "walls": [] }}"#).is_none());
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

    #[test]
    fn openings_roundtrip_through_the_arch_xrecord() {
        let mut d = Drawing::new_metric();
        let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let oid = d.arch.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        let want = d.arch.opening(oid).unwrap().clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.openings.len(), 1, "opening lost on the arch XRECORD round trip");
        let got = &back.arch.openings[0];
        assert_eq!(got.id, oid, "opening id changed on round trip");
        assert_eq!(got.host, wid, "host wall id changed on round trip");
        assert_eq!(got.width, want.width);
        assert_eq!(got.height, want.height);
        assert_eq!(got.sill, want.sill);
        assert_eq!(got.kind, OpeningKind::Door);
        assert_eq!(got.layer, want.layer);
    }

    #[test]
    fn version_1_payload_still_parses() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        // A version-1 file predates the openings concept; the JSON may carry openings but
        // `parse_arch` drops them, keeping the wall.
        let json = serde_json::json!({ "version": 1, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text);
        assert!(parsed.is_some(), "a version-1 payload must still parse");
        if let Some(m) = parsed {
            assert!(m.openings.is_empty(), "version 1 must yield no openings");
            assert_eq!(m.walls.len(), 1, "version 1 lost its wall");
        }
    }

    /// The per-opening `(layer, tags)` from `proxy_opening_polylines`, grouped by layer.
    fn poly_by_layer(polys: &[(String, Vec<Tag>)]) -> std::collections::HashMap<String, Vec<Vec<Tag>>> {
        let mut map: std::collections::HashMap<String, Vec<Tag>> = std::collections::HashMap::new();
        for (l, t) in polys {
            map.entry(l.clone()).or_default().push(t.clone());
        }
        map
    }

    #[test]
    fn opening_proxy_polylines_are_written_on_a_door() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-DOOR");
        let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));

        let text = write_dxf(&d);
        assert!(text.contains("ARCHOPENING"), "no ARCHOPENING marker in the DXF");
        assert!(text.contains("A-DOOR"), "no A-DOOR layer in the DXF");
    }

    #[test]
    fn opening_proxy_polylines_use_the_opening_layer() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-DOOR");
        d.ensure_layer("A-GLAZ");
        let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        d.arch.push_opening(Opening::new(wid, 4.0, 0.9, 1.5, OpeningKind::Window));

        let polys = proxy_opening_polylines(&d.arch);
        assert_eq!(polys.len(), 2, "expected two opening proxy rings");

        // Each ring is on the opening's own layer.
        let (dl, dts) = &polys[0];
        let (wl, wts) = &polys[1];
        assert_eq!(dl.as_str(), d.arch.openings[0].layer.as_str(), "door ring not on its layer");
        assert_eq!(wl.as_str(), d.arch.openings[1].layer.as_str(), "window ring not on its layer");

        // One ring per expected layer.
        let grouped = poly_by_layer(&polys);
        assert!(!grouped.get("A-DOOR").map(|v| v.is_empty()).unwrap_or(true), "no proxy ring on A-DOOR");
        assert!(!grouped.get("A-GLAZ").map(|v| v.is_empty()).unwrap_or(true), "no proxy ring on A-GLAZ");

        // XDATA carries ARCHOPENING marker and the o{n} label.
        assert!(dts.iter().any(|t| t.code == 1000 && t.str().trim() == ARCH_OPENING_MARKER));
        assert!(dts.iter().any(|t| t.code == 1000 && t.str().trim().starts_with("o")));
        assert!(wts.iter().any(|t| t.code == 1000 && t.str().trim().starts_with("o")));
    }

    #[test]
    fn resaving_does_not_duplicate_openings() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-DOOR");
        d.ensure_layer("A-GLAZ");
        let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        d.arch.push_opening(Opening::new(wid, 4.0, 0.9, 1.5, OpeningKind::Window));

        let want = d.arch.openings.len();
        let first = read_dxf(write_dxf(&d).as_bytes()).unwrap();
        let second = read_dxf(write_dxf(&first).as_bytes()).unwrap();

        assert_eq!(first.arch.openings.len(), want, "openings changed on first read");
        assert_eq!(second.arch.openings.len(), want, "openings changed on second read");

        // No ARCHOPENING proxy polyline survives the read (model is the source of truth).
        let count_proxy = |d: &Drawing| {
            d.model
                .iter()
                .filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_)))
                .filter(|e| e.common.xdata.iter().any(|t| t.code == 1000 && t.value.trim() == ARCH_OPENING_MARKER))
                .count()
        };
        assert_eq!(count_proxy(&first), 0, "opening proxy polyline survived the first read");
        assert_eq!(count_proxy(&second), 0, "opening proxy polyline survived the second read");
    }

    #[test]
    fn openings_round_trip_through_dxf() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-DOOR");
        d.ensure_layer("A-GLAZ");
        let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(12.0, 0.0), 0.2));
        let door = d.arch.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        let window = d.arch.push_opening(Opening::new(wid, 4.0, 1.2, 1.5, OpeningKind::Window));
        let garage = d.arch.push_opening(Opening::new(wid, 6.5, 2.4, 2.1, OpeningKind::GarageDoor));

        let snap = d.arch.clone();
        for _ in 0..2 {
            let text = write_dxf(&d);
            let back = read_dxf(text.as_bytes()).unwrap();
            d = back;
        }
        assert_eq!(d.arch.openings.len(), snap.openings.len(), "opening count changed after round trip");
        let door_r = d.arch.opening(door).unwrap().clone();
        let window_r = d.arch.opening(window).unwrap().clone();
        let garage_r = d.arch.opening(garage).unwrap().clone();

        assert_eq!(door_r.kind, snap.openings[0].kind);
        assert_eq!(window_r.kind, snap.openings[1].kind);
        assert_eq!(garage_r.kind, snap.openings[2].kind);
        assert_eq!(window_r.sill, snap.openings[1].sill);
        assert_eq!(door_r.width, snap.openings[0].width);
        assert_eq!(garage_r.width, snap.openings[2].width);

        // Layers persist in the saved drawing.
        let text = write_dxf(&d);
        assert!(text.contains("A-DOOR"), "A-DOOR missing from a saved drawing with a door");
        assert!(text.contains("A-GLAZ"), "A-GLAZ missing from a saved drawing with a window");
    }

    #[test]
    fn proxy_openings_are_dropped_when_the_xrecord_is_present() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_opening(Opening::new(d.arch.walls[0].id, 1.0, 0.9, 2.1, OpeningKind::Door));

        let text = write_dxf(&d);
        assert!(text.contains("ARCHOPENING"), "no opening proxy written");
        let back = read_dxf(text.as_bytes()).unwrap();

        // The opening survives via the XRECORD...
        assert_eq!(back.arch.openings.len(), 1, "opening lost via the XRECORD");

        // ...but the save-time proxy polyline (ARCHOPENING xdata) is dropped on read.
        let mut count = 0;
        for e in back.model.iter() {
            if let EntityKind::LwPolyline(_) = &e.kind {
                count += e.common.xdata.iter().filter(|t| t.code == 1000 && t.value.trim() == ARCH_OPENING_MARKER).count();
            }
        }
        assert_eq!(count, 0, "model LWPOLYLINE carries ARCHOPENING xdata");
    }

    #[test]
    fn hostile_opening_payload_never_panics() {
        for h in ["", "not json", "{}", "[]", "null"] {
            assert!(parse_arch(h).is_none(), "hostile input {h:?}");
        }
        // A version-2 payload whose openings field has the wrong type is rejected.
        assert!(parse_arch(r#"{"version": 2, "model": {"openings": "boom"}}"#).is_none());
        // An oversized payload is rejected before it is parsed.
        assert!(parse_arch(&"a".repeat((8 << 20) + 1)).is_none());
        // arch_chunks is None for an empty model and must not panic for a populated one.
        assert!(arch_chunks(&ArchModel::default()).is_none());
    }
}
