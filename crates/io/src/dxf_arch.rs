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

/// Layer of save-time room-label proxy polylines.
pub(crate) const ARCH_ROOM_LAYER: &str = "A-ROOM";

/// `CADCRAFT` xdata marker identifying a save-time room-boundary proxy polyline, so the
/// reader drops it while the arch record is present (the room's model data is the source of truth).
pub(crate) const ARCH_ROOM_MARKER: &str = "CADCRAFT-ROOM";

/// Layer of save-time roof-edge proxy polylines.
pub(crate) const ARCH_ROOF_LAYER: &str = "A-ROOF";

/// `CADCRAFT` xdata marker identifying a save-time roof-edge proxy polyline, so the
/// reader drops it while the arch record is present (the roof's model data is the source of truth).
pub(crate) const ARCH_ROOF_MARKER: &str = "CADCRAFT-ROOF";

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
/// Returns `None` when the model has no walls, openings, rooms, roofs, stairs, railings or cameras
/// (nothing to write); otherwise the escaped, chunked payload, exactly like
/// [`dxf_ext::constraint_chunks`](crate::dxf_ext).
pub(crate) fn arch_chunks(m: &ArchModel) -> Option<Vec<String>> {
    if m.walls.is_empty()
        && m.openings.is_empty()
        && m.rooms.is_empty()
        && m.roofs.is_empty()
        && m.stairs.is_empty()
        && m.railings.is_empty()
        && m.cameras.is_empty()
    {
        return None;
    }
    let json = serde_json::to_string(&Payload { version: 6, model: m.clone() }).ok()?;
    Some(crate::dxf_ext::json_chunks(&json))
}

/// Parse an architectural XRECORD payload; `None` when it is not a payload we understand.
///
/// Accepts schema versions 1 through 6:
/// - v1: no openings (cleared).
/// - v2: openings present.
/// - v3: rooms/levels present, no roofs (cleared).
/// - v4: roofs present.
/// - v5: stairs and railings present.
/// - v6: cameras present.
///
/// All versions are bounded — the wall count against [`MAX_WALLS`], the opening count
/// against [`MAX_OPENINGS`], the roof count against [`MAX_ROOFS`], the stair count against
/// [`MAX_STAIRS`], the railing count against [`MAX_RAILINGS`], the room count against
/// [`MAX_ROOMS`], and the camera count against [`MAX_CAMERAS`] — so hostile payloads can not
/// overflow allocations.
pub(crate) fn parse_arch(text: &str) -> Option<ArchModel> {
    if text.len() > MAX_ARCH_PAYLOAD {
        return None;
    }
    let p: Payload = serde_json::from_str(text).ok()?;
    if !(1..=6).contains(&p.version) {
        return None;
    }
    if p.model.walls.len() > cadcraft_doc::MAX_WALLS {
        return None;
    }
    let mut model = p.model;
    if p.version <= 2 {
        model.openings.clear();
    }
    if p.version <= 3 {
        model.roofs.clear();
    }
    if p.version <= 4 {
        model.stairs.clear();
        model.railings.clear();
    }
    if p.version <= 5 {
        model.cameras.clear();
        model.active_camera = None;
    }
    if model.openings.len() > cadcraft_doc::arch::MAX_OPENINGS {
        return None;
    }
    if model.roofs.len() > cadcraft_doc::arch::MAX_ROOFS {
        return None;
    }
    if model.stairs.len() > cadcraft_doc::arch::MAX_STAIRS {
        return None;
    }
    if model.railings.len() > cadcraft_doc::arch::MAX_RAILINGS {
        return None;
    }
    if model.rooms.len() > cadcraft_doc::arch::MAX_ROOMS {
        return None;
    }

    // Validate and filter cameras
    model.cameras.retain(|c| {
        c.target.map_or(false, |t| t.iter().all(|v| v.is_finite()))
            && c.azimuth_deg.map_or(false, |v| v.is_finite())
            && c.elevation_deg.map_or(false, |v| v.is_finite())
            && c.distance.map_or(false, |v| v.is_finite())
            && c.eye.map_or(false, |e| e.iter().all(|v| v.is_finite()))
            && c.eye_height.map_or(false, |v| v.is_finite())
            && c.fov_deg.map_or(false, |v| v.is_finite())
    });
    if model.cameras.len() > cadcraft_doc::arch::MAX_CAMERAS {
        return None;
    }

    // If active_camera points to a dropped camera, clear it
    if let Some(ref name) = model.active_camera {
        if !model.cameras.iter().any(|c| &c.name == name) {
            model.active_camera = None;
        }
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
    if !w.thickness.is_finite() || w.thickness <= 0.0 {
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
/// the wall's id. Rings come from [`cadcraft_arch::join::solve_joins`] so joined corners are
/// mitred rather than drawn as overlapping rectangles; a wall with no solved shape (degenerate)
/// falls back to its own centre-line rectangle. Returns `Vec::new()` for an empty model; the
/// output is capped at [`cadcraft_doc::MAX_WALLS`] polylines.
pub(crate) fn proxy_polylines(m: &ArchModel) -> Vec<Vec<Tag>> {
    if m.is_empty() {
        return Vec::new();
    }
    let shapes = cadcraft_arch::join::solve_joins(m);
    let rings: std::collections::HashMap<cadcraft_doc::ArchId, Vec<Vec2>> = shapes.into_iter().map(|s| (s.id, s.polygon)).collect();

    m.walls
        .iter()
        .take(cadcraft_doc::MAX_WALLS)
        .filter_map(|w| {
            let corners = match rings.get(&w.id) {
                Some(r) => r.clone(),
                None => wall_ring(w)?,
            };
            let n = i64::try_from(corners.len()).ok()?;
            let mut t = Vec::new();
            t.push(Tag::i(90, n));
            t.push(Tag::i(70, 1));
            for c in &corners {
                t.push(Tag::f(10, c.x));
                t.push(Tag::f(20, c.y));
            }
            t.push(Tag::s(1001, crate::dxf_ext::APP));
            t.push(Tag::s(1000, ARCH_PROXY_MARKER));
            t.push(Tag::s(1000, &w.id.to_string()));
            Some(t)
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
            let n = i64::try_from(corners.len()).ok()?;
            let mut t = Vec::new();
            t.push(Tag::i(90, n));
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

/// True when an entity's `CADCRAFT` xdata marks it as a save-time room proxy polyline.
pub(crate) fn is_room_proxy(tags: &[Tag]) -> bool {
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str().trim() == ARCH_ROOM_MARKER)
}

/// True when an entity's `CADCRAFT` xdata marks it as a save-time roof proxy polyline.
pub(crate) fn is_roof_proxy(tags: &[Tag]) -> bool {
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str().trim() == ARCH_ROOF_MARKER)
}

/// The DXF groups (after the common entity groups) of one closed LWPOLYLINE per valid room
/// boundary. Rooms with fewer than three vertices or any non-finite point are skipped; output is
/// capped at [`cadcraft_doc::arch::MAX_ROOMS`] polylines.
pub(crate) fn proxy_room_polylines(m: &ArchModel) -> Vec<Vec<Tag>> {
    m.rooms
        .iter()
        .take(cadcraft_doc::arch::MAX_ROOMS)
        .filter_map(|r| {
            if r.polygon.len() < 3 || !r.polygon.iter().all(|p| p.is_finite()) {
                return None;
            }
            let n = i64::try_from(r.polygon.len()).ok()?;
            let mut t = Vec::new();
            t.push(Tag::i(90, n));
            t.push(Tag::i(70, 1));
            for p in &r.polygon {
                t.push(Tag::f(10, p.x));
                t.push(Tag::f(20, p.y));
            }
            t.push(Tag::s(1001, crate::dxf_ext::APP));
            t.push(Tag::s(1000, ARCH_ROOM_MARKER));
            t.push(Tag::s(1000, &cadcraft_doc::arch::room_label(r.id)));
            Some(t)
        })
        .collect()
}

/// The DXF groups (after the common entity groups) of one open LWPOLYLINE per non-eave roof edge
/// (ridge, hip, valley, gable end). Eave edges and edges with non-finite endpoints are skipped.
/// Output is capped at [`cadcraft_doc::arch::MAX_ROOFS`] roofs and [`cadcraft_doc::arch::MAX_ROOF_EDGES`]
/// edges per roof.
pub(crate) fn proxy_roof_polylines(m: &ArchModel) -> Vec<Vec<Tag>> {
    if m.roofs.is_empty() {
        return Vec::new();
    }
    m.roofs
        .iter()
        .take(cadcraft_doc::arch::MAX_ROOFS)
        .flat_map(|roof| {
            roof.edges
                .iter()
                .take(cadcraft_doc::arch::MAX_ROOF_EDGES)
                .filter_map(|e| {
                    if e.kind == cadcraft_doc::arch::RoofEdgeKind::Eave {
                        return None;
                    }
                    if !e.a.is_finite() || !e.b.is_finite() {
                        return None;
                    }
                    let mut t = Vec::new();
                    t.push(Tag::i(90, 2));
                    t.push(Tag::i(70, 0)); // open
                    t.push(Tag::f(10, e.a.x));
                    t.push(Tag::f(20, e.a.y));
                    t.push(Tag::f(10, e.b.x));
                    t.push(Tag::f(20, e.b.y));
                    t.push(Tag::s(1001, crate::dxf_ext::APP));
                    t.push(Tag::s(1000, ARCH_ROOF_MARKER));
                    t.push(Tag::s(1000, &cadcraft_doc::arch::roof_label(roof.id)));
                    Some(t)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_dxf, write_dxf};
    use cadcraft_doc::arch::{Opening, OpeningKind, Railing, Roof, RoofEdge, RoofEdgeKind, RoofPlane, Room, RoomType, Stair};
    use cadcraft_doc::{ArchModel, Drawing, EntityKind, Wall};
    use cadcraft_geom::Vec2;

    /// A drawing with one wall and one roof (ridge + eave edges).
    fn drawing_with_roof() -> Drawing {
        let mut d = Drawing::new_metric();
        d.ensure_layer(ARCH_ROOF_LAYER);
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_roof(Roof { level: 0, pitch_rise: 6.0, pitch_run: 12.0, ..Roof::default() });
        // Add edges manually since push_roof might not populate them fully in this test context
        // or we want specific test edges.
        if let Some(roof) = d.arch.roofs.first_mut() {
            roof.edges = vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(4.0, 4.0), b: Vec2::new(6.0, 4.0), za: 4.45, zb: 4.45, ..Default::default() },
                RoofEdge { kind: RoofEdgeKind::Eave, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), za: 2.4, zb: 2.4, ..Default::default() },
            ];
        }
        d
    }

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
        // Versions 1..=6 are understood; 0 and 7+ are unknown.
        assert!(parse_arch(r#"{"version": 7, "model": { "walls": [] }}"#).is_none());
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
    fn proxy_polylines_use_the_solved_mitred_ring() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 2, "expected two wall proxies");

        // Collect all corners of each ring.
        let corners_of = |tags: &[Tag]| -> Vec<(f64, f64)> {
            let mut out = Vec::new();
            let mut i = 0;
            while i + 1 < tags.len() {
                if tags[i].code == 10 && tags[i + 1].code == 20 {
                    out.push((tags[i].f64(), tags[i + 1].f64()));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            out
        };

        // Both rings contain the shared mitre points (10.1,-0.1) and (9.9,0.1).
        for tags in &polys {
            let corners = corners_of(tags);
            assert!(corners.iter().any(|(x, y)| (x - 10.1).abs() < 1e-9 && (y + 0.1).abs() < 1e-9), "ring missing mitre (10.1,-0.1)");
            assert!(corners.iter().any(|(x, y)| (x - 9.9).abs() < 1e-9 && (y - 0.1).abs() < 1e-9), "ring missing mitre (9.9,0.1)");
        }

        // The first wall (horizontal) keeps none of the bare-rectangle corners (10,-0.1)/(10,0.1).
        let c0 = corners_of(&polys[0]);
        assert!(!c0.iter().any(|(x, y)| (x - 10.0).abs() < 1e-9 && (y + 0.1).abs() < 1e-9), "wall 0 still has rectangle corner (10,-0.1)");
        assert!(!c0.iter().any(|(x, y)| (x - 10.0).abs() < 1e-9 && (y - 0.1).abs() < 1e-9), "wall 0 still has rectangle corner (10,0.1)");

        // The second wall (vertical) keeps none of (9.9,0)/(10.1,0).
        let c1 = corners_of(&polys[1]);
        assert!(!c1.iter().any(|(x, y)| (x - 9.9).abs() < 1e-9 && y.abs() < 1e-9), "wall 1 still has rectangle corner (9.9,0)");
        assert!(!c1.iter().any(|(x, y)| (x - 10.1).abs() < 1e-9 && y.abs() < 1e-9), "wall 1 still has rectangle corner (10.1,0)");
    }

    #[test]
    fn proxy_for_an_unsolved_wall_falls_back_to_the_rectangle() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 1, "expected one wall proxy");

        let tags = &polys[0];
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
        assert_eq!(corners.len(), 4, "a single wall gets a four-vertex rectangle");
        for want in [(0.0, 0.1), (10.0, 0.1), (10.0, -0.1), (0.0, -0.1)] {
            assert!(corners.iter().any(|c| (c.0 - want.0).abs() < 1e-9 && (c.1 - want.1).abs() < 1e-9), "missing rectangle corner {:?}", want);
        }
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
        // The ring now comes from the join solver, so its winding/starting vertex may differ from
        // a bare rectangle; assert the four poché corners are all present.
        for want in [(0.0, 0.1), (10.0, 0.1), (10.0, -0.1), (0.0, -0.1)] {
            assert!(corners.iter().any(|c| (c.0 - want.0).abs() < 1e-9 && (c.1 - want.1).abs() < 1e-9), "missing poché corner {:?}", want);
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
        let mut map: std::collections::HashMap<String, Vec<Vec<Tag>>> = std::collections::HashMap::new();
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

    #[test]
    fn rooms_round_trip_through_dxf() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Kitchen".into(),
            ..Room::default()
        });
        d.ensure_layer("A-ROOM");
        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 1, "room lost on DXF round trip");
        let got = &back.arch.rooms[0];
        assert_eq!(got.id, rid, "room id changed on round trip");
        assert_eq!(got.name, "Kitchen");
        assert_eq!(got.polygon.len(), 4);
        for (p, q) in got.polygon.iter().zip(d.arch.rooms[0].polygon.iter()) {
            assert!((p.x - q.x).abs() < 1e-9, "polygon x mismatch: {} vs {}", p.x, q.x);
            assert!((p.y - q.y).abs() < 1e-9, "polygon y mismatch: {} vs {}", p.y, q.y);
        }
    }

    #[test]
    fn arch_read_is_idempotent_with_rooms() {
        let mut d = Drawing::new_metric();
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Bedroom".into(),
            ..Room::default()
        });
        d.ensure_layer("A-ROOM");

        let first = read_dxf(write_dxf(&d).as_bytes()).unwrap();
        let second = read_dxf(write_dxf(&first).as_bytes()).unwrap();

        assert_eq!(first.arch.rooms.len(), second.arch.rooms.len(), "room count changed between reads");
        for (a, b) in first.arch.rooms.iter().zip(second.arch.rooms.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.name, b.name);
            assert_eq!(a.polygon.len(), b.polygon.len());
        }

        // Room proxy polylines (ARCHROOM xdata) are dropped on read.
        let count_proxy = |d: &Drawing| {
            d.model
                .iter()
                .filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_)))
                .filter(|e| e.common.xdata.iter().any(|t| t.code == 1000 && t.value.trim() == ARCH_ROOM_MARKER))
                .count()
        };
        assert_eq!(count_proxy(&first), 0, "room proxy polyline survived the first read");
        assert_eq!(count_proxy(&second), 0, "room proxy polyline survived the second read");
    }

    #[test]
    fn a_room_layer_is_registered() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living Room".into(),
            ..Room::default()
        });
        let text = write_dxf(&d);
        assert!(text.contains("A-ROOM"), "no A-ROOM layer in the DXF output");
    }

    fn drawing_with_room() -> Drawing {
        let mut d = Drawing::new_metric();
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Kitchen".into(),
            ..Room::default()
        });
        d
    }

    #[test]
    fn room_proxy_skips_degenerate_polygons() {
        let mut m = ArchModel::default();
        // 0 points
        m.push_room(Room { polygon: Vec::new(), name: "empty".into(), ..Room::default() });
        // 1 point
        m.push_room(Room { polygon: vec![Vec2::new(0.0, 0.0)], name: "one".into(), ..Room::default() });
        // 2 points
        m.push_room(Room { polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)], name: "two".into(), ..Room::default() });
        // 3 points but one is NaN
        m.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(f64::NAN, 0.0)],
            name: "nan".into(),
            ..Room::default()
        });
        // 3 points but one is infinity
        m.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(f64::INFINITY, 0.0)],
            name: "inf".into(),
            ..Room::default()
        });
        // Valid triangle
        m.push_room(Room { polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)], name: "valid".into(), ..Room::default() });

        let polys = proxy_room_polylines(&m);
        assert_eq!(polys.len(), 1, "only the valid room should produce a proxy");
        // Check it's the valid one (has the right label)
        let label = cadcraft_doc::arch::room_label(m.rooms[5].id);
        assert!(polys[0].iter().any(|t| t.code == 1000 && t.str() == label), "wrong room proxy produced");
    }

    #[test]
    fn room_proxy_handles_multiple_rooms() {
        let mut m = ArchModel::default();
        m.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living".into(),
            ..Room::default()
        });
        m.push_room(Room {
            polygon: vec![Vec2::new(5.0, 0.0), Vec2::new(9.0, 0.0), Vec2::new(9.0, 3.0), Vec2::new(5.0, 3.0)],
            name: "Kitchen".into(),
            ..Room::default()
        });
        m.push_room(Room {
            polygon: vec![Vec2::new(0.0, 4.0), Vec2::new(4.0, 4.0), Vec2::new(4.0, 8.0), Vec2::new(0.0, 8.0)],
            name: "Bedroom".into(),
            ..Room::default()
        });

        let polys = proxy_room_polylines(&m);
        assert_eq!(polys.len(), 3, "all three rooms should produce proxies");

        // Each proxy should have its own room label (the tag after ARCH_ROOM_MARKER)
        let labels: Vec<String> = polys
            .iter()
            .map(|tags| {
                tags.iter()
                    .position(|t| t.code == 1000 && t.str() == ARCH_ROOM_MARKER)
                    .and_then(|i| tags.get(i + 1))
                    .filter(|t| t.code == 1000)
                    .map(|t| t.str())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(labels.len(), 3);
        assert!(labels[0] != labels[1] && labels[1] != labels[2] && labels[0] != labels[2], "labels should be unique");
    }

    #[test]
    fn room_proxy_handles_huge_coordinates() {
        let mut m = ArchModel::default();
        m.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(1e15, 0.0), Vec2::new(1e15, 1e15), Vec2::new(0.0, 1e15)],
            name: "Huge".into(),
            ..Room::default()
        });

        let polys = proxy_room_polylines(&m);
        assert_eq!(polys.len(), 1, "huge room should produce a proxy");
        let tags = &polys[0];
        // Check that coordinates are written as finite values
        let xs: Vec<f64> = tags.iter().filter(|t| t.code == 10).map(|t| t.f64()).collect();
        let ys: Vec<f64> = tags.iter().filter(|t| t.code == 20).map(|t| t.f64()).collect();
        assert_eq!(xs.len(), 4);
        assert_eq!(ys.len(), 4);
        assert!(xs.iter().all(|v| v.is_finite()));
        assert!(ys.iter().all(|v| v.is_finite()));
        assert!(xs.contains(&1e15));
        assert!(ys.contains(&1e15));
    }

    #[test]
    fn room_proxy_multiple_rooms_round_trip() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Living".into(),
            ..Room::default()
        });
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(5.0, 0.0), Vec2::new(9.0, 0.0), Vec2::new(9.0, 3.0), Vec2::new(5.0, 3.0)],
            name: "Kitchen".into(),
            ..Room::default()
        });
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 4.0), Vec2::new(4.0, 4.0), Vec2::new(4.0, 8.0), Vec2::new(0.0, 8.0)],
            name: "Bedroom".into(),
            ..Room::default()
        });

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 3, "all three rooms should survive round trip");
        let names: Vec<&str> = back.arch.rooms.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Living"));
        assert!(names.contains(&"Kitchen"));
        assert!(names.contains(&"Bedroom"));

        // No room proxy polylines should survive the read
        let count_proxy = |d: &Drawing| {
            d.model
                .iter()
                .filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_)))
                .filter(|e| e.common.xdata.iter().any(|t| t.code == 1000 && t.value.trim() == ARCH_ROOM_MARKER))
                .count()
        };
        assert_eq!(count_proxy(&back), 0, "room proxy polylines should be dropped on read");
    }

    #[test]
    fn room_proxy_polylines_are_written_on_save() {
        let d = drawing_with_room();
        let label = d.arch.rooms.first().map(|r| cadcraft_doc::arch::room_label(r.id)).unwrap_or_default();
        let polys = proxy_room_polylines(&d.arch);
        assert_eq!(polys.len(), 1, "expected one room proxy ring");
        let tags = &polys[0];
        assert!(tags.iter().any(|t| t.code == 90 && t.i64() == 4));
        assert!(tags.iter().any(|t| t.code == 70 && t.i64() == 1));
        assert_eq!(tags.iter().filter(|t| t.code == 10).count(), 4);
        assert_eq!(tags.iter().filter(|t| t.code == 20).count(), 4);
        assert!(tags.iter().any(|t| t.code == 1000 && t.str() == ARCH_ROOM_MARKER));
        assert!(tags.iter().any(|t| t.code == 1000 && t.str() == label.as_str()));

        let text = write_dxf(&d);
        assert!(text.contains(ARCH_ROOM_MARKER), "no room proxy marker in the DXF");
        assert!(text.contains(ARCH_ROOM_LAYER), "no A-ROOM layer in the DXF");
        assert!(text.contains(label.as_str()), "no room label in the DXF");
    }

    #[test]
    fn room_proxy_is_dropped_when_the_arch_record_is_present() {
        let d = drawing_with_room();
        let text = write_dxf(&d);
        assert!(text.contains(ARCH_KEY), "no architectural record in the DXF");
        assert!(text.contains(ARCH_ROOM_MARKER), "no room proxy written");

        let back = read_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.arch.rooms.len(), 1, "room lost on the arch XRECORD round trip");
        let polylines = back.model.iter().filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_))).count();
        assert_eq!(polylines, 0, "room proxy linework survived the read");
    }

    #[test]
    fn rooms_survive_a_save_read_save_cycle_with_a_room_layer() {
        let mut d = drawing_with_room();
        d.ensure_layer(ARCH_ROOM_LAYER);
        let want = d.arch.rooms[0].clone();

        let first_text = write_dxf(&d);
        let first = read_dxf(first_text.as_bytes()).unwrap();
        let second_text = write_dxf(&first);
        let second = read_dxf(second_text.as_bytes()).unwrap();

        assert_eq!(first.arch.rooms.len(), 1, "room lost on the first read");
        assert_eq!(second.arch.rooms.len(), 1, "room lost on the second read");
        let got = &second.arch.rooms[0];
        assert_eq!(got.id, want.id);
        assert_eq!(got.name, want.name);
        assert_eq!(got.polygon, want.polygon);
        assert!(second.layer(ARCH_ROOM_LAYER).is_some(), "A-ROOM layer lost on round trip");
        assert_eq!(first_text.matches(ARCH_ROOM_MARKER).count(), 1);
        assert_eq!(second_text.matches(ARCH_ROOM_MARKER).count(), 1, "room proxies duplicated on re-save");
    }

    /// The previous wire version (v4) is still understood by [`parse_arch`] even though the
    /// writer now emits v5 — a v4 record reads back into its walls.
    #[test]
    fn arch_record_version_is_four() {
        let m = one_wall_model();
        let json = serde_json::json!({ "version": 4, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text);
        assert!(parsed.is_some(), "a version-4 payload must still parse");
        assert_eq!(parsed.unwrap().walls.len(), 1, "version-4 payload lost its wall");
    }

    #[test]
    fn arch_record_version_is_six() {
        let chunks = arch_chunks(&one_wall_model()).unwrap();
        let obj: serde_json::Value = serde_json::from_str(&chunks.join("")).unwrap();
        assert_eq!(obj["version"].as_u64(), Some(6), "written payload version");
    }

    #[test]
    fn cameras_round_trip_through_dxf() {
        use cadcraft_doc::arch::Camera3d;
        let mut d = Drawing::new_metric();
        d.arch.cameras.push(Camera3d {
            name: "Front".into(),
            kind: cadcraft_doc::arch::CameraKind::Orbit,
            target: Some([0.0, 0.0, 1.0]),
            azimuth_deg: Some(0.0),
            elevation_deg: Some(0.0),
            distance: Some(10.0),
            eye: Some([0.0, -10.0]),
            eye_height: Some(1.5),
            fov_deg: Some(45.0),
            ..Default::default()
        });
        d.arch.active_camera = Some("Front".into());

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.cameras.len(), 1, "camera lost on DXF round trip");
        let got = &back.arch.cameras[0];
        assert_eq!(got.name, "Front");
        assert_eq!(got.kind, cadcraft_doc::arch::CameraKind::Orbit);
        assert_eq!(got.target, Some([0.0, 0.0, 1.0]));
        assert_eq!(got.azimuth_deg, Some(0.0));
        assert_eq!(got.elevation_deg, Some(0.0));
        assert_eq!(got.distance, Some(10.0));
        assert_eq!(got.eye, Some([0.0, -10.0]));
        assert_eq!(got.eye_height, Some(1.5));
        assert_eq!(got.fov_deg, Some(45.0));
        assert_eq!(back.arch.active_camera, Some("Front".into()));
    }

    #[test]
    fn older_arch_records_still_open() {
        // A v5 record with no cameras key should parse with empty cameras
        let mut m = one_wall_model();
        // Ensure no cameras in the source model for this test
        m.cameras.clear();
        m.active_camera = None;

        let json = serde_json::json!({ "version": 5, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text).expect("a version-5 payload must still parse");
        assert!(parsed.cameras.is_empty(), "v5 payload should have no cameras");
        assert!(parsed.active_camera.is_none(), "v5 payload should have no active camera");
    }

    #[test]
    fn a_camera_list_over_the_cap_is_dropped_not_panicked() {
        use cadcraft_doc::arch::Camera3d;
        let mut m = one_wall_model();
        for i in 0..(cadcraft_doc::arch::MAX_CAMERAS + 1) {
            m.cameras.push(Camera3d { name: format!("Cam{}", i), ..Default::default() });
        }
        let json = serde_json::json!({ "version": 6, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        assert!(parse_arch(&text).is_none(), "a payload with too many cameras must be rejected");
    }

    #[test]
    fn non_finite_camera_numbers_are_dropped() {
        use cadcraft_doc::arch::Camera3d;
        let mut m = one_wall_model();
        m.cameras.push(Camera3d { name: "Bad".into(), target: Some([f64::NAN, 0.0, 0.0]), ..Default::default() });
        m.cameras.push(Camera3d { name: "Good".into(), target: Some([0.0, 0.0, 0.0]), ..Default::default() });

        let json = serde_json::json!({ "version": 6, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text).expect("a payload with one bad camera should parse");
        assert_eq!(parsed.cameras.len(), 1, "only the good camera should remain");
        assert_eq!(parsed.cameras[0].name, "Good");
    }

    #[test]
    fn v3_payload_still_parses() {
        let mut m = one_wall_model();
        m.push_roof(Roof { level: 0, pitch_rise: 6.0, pitch_run: 12.0, ..Roof::default() });
        let json = serde_json::json!({ "version": 3, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text).expect("a version-3 payload must still parse");
        assert_eq!(parsed.walls.len(), 1);
        assert!(parsed.roofs.is_empty(), "a v3 file predates roofs");
        // v6 is now the current version and should be accepted, not rejected.
        assert!(parse_arch(r#"{"version": 6, "model": {"walls": []}}"#).is_some());
    }

    #[test]
    fn roof_round_trips_exactly() {
        let mut d = Drawing::new_metric();
        let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let rid = d.arch.push_roof(Roof {
            level: 0,
            pitch_rise: 6.0,
            pitch_run: 12.0,
            overhang: 0.5,
            thickness: 0.25,
            fascia: true,
            soffit: true,
            manual: false,
            hand_edited: true,
            planes: vec![RoofPlane {
                ring: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)],
                slope: 0.5,
                projected_area: 40.0,
                eave_wall: Some(wid),
                base_edge: [0.0, 0.0, 10.0, 0.0],
                thickness: 0.25,
                overhang: 0.5,
                eave_z: 2.4,
                ..Default::default()
            }],
            edges: vec![RoofEdge {
                kind: RoofEdgeKind::Ridge,
                a: Vec2::new(4.0, 4.0),
                b: Vec2::new(6.0, 4.0),
                za: 4.45,
                zb: 4.45,
                ..Default::default()
            }],
            ..Roof::default()
        });

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.roofs.len(), 1);
        assert_eq!(back.arch.roofs[0].id, rid);
        assert_eq!(back.arch.roofs, d.arch.roofs);
        assert_eq!(back.arch.next_roof_id, d.arch.next_roof_id);
        assert_eq!(back.arch.walls, d.arch.walls);
    }

    #[test]
    fn roof_proxy_linework_is_written_on_save() {
        let d = drawing_with_roof();
        let label = cadcraft_doc::arch::roof_label(d.arch.roofs[0].id);
        let polys = proxy_roof_polylines(&d.arch);

        // Only the ridge edge (non-eave) should be written
        assert_eq!(polys.len(), 1, "expected one roof proxy ring (ridge only)");

        let tags = &polys[0];
        assert!(tags.iter().any(|t| t.code == 90 && t.i64() == 2), "vertex count should be 2");
        assert!(tags.iter().any(|t| t.code == 70 && t.i64() == 0), "flag should be 0 (open)");
        assert_eq!(tags.iter().filter(|t| t.code == 10).count(), 2, "expected two x coords");
        assert_eq!(tags.iter().filter(|t| t.code == 20).count(), 2, "expected two y coords");
        assert!(tags.iter().any(|t| t.code == 1000 && t.str() == ARCH_ROOF_MARKER), "missing roof marker");
        assert!(tags.iter().any(|t| t.code == 1000 && t.str() == label.as_str()), "missing roof label");

        let text = write_dxf(&d);
        assert!(text.contains(ARCH_ROOF_MARKER), "no roof proxy marker in DXF");
        assert!(text.contains(label.as_str()), "no roof label in DXF");
        assert!(text.contains(ARCH_ROOF_LAYER), "no A-ROOF layer in DXF");
    }

    #[test]
    fn roof_proxies_are_dropped_when_arch_is_present() {
        let d = drawing_with_roof();
        let text = write_dxf(&d);
        assert!(text.contains(ARCH_KEY), "no architectural record in the DXF");
        assert!(text.contains(ARCH_ROOF_MARKER), "no roof proxy written");

        let back = read_dxf(text.as_bytes()).unwrap();

        // The roof survives via the XRECORD
        assert_eq!(back.arch.roofs.len(), 1, "roof lost on the arch XRECORD round trip");

        // No proxy linework should survive
        let count_proxy = |d: &Drawing| {
            d.model
                .iter()
                .filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_)))
                .filter(|e| e.common.xdata.iter().any(|t| t.code == 1000 && t.value.trim() == ARCH_ROOF_MARKER))
                .count()
        };
        assert_eq!(count_proxy(&back), 0, "roof proxy polyline survived the read");

        // Check that no LWPOLYLINE at all survived (wall/opening/room/roof proxies all dropped)
        let polylines = back.model.iter().filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_))).count();
        assert_eq!(polylines, 0, "proxy linework survived the read");
    }

    #[test]
    fn proxy_roof_polylines_empty_model() {
        let m = ArchModel::default();
        let polys = proxy_roof_polylines(&m);
        assert_eq!(polys.len(), 0, "empty model should produce no roof proxies");
    }

    #[test]
    fn roof_with_only_eave_edges_produces_no_proxy() {
        let mut m = ArchModel::default();
        m.push_roof(Roof {
            level: 0,
            pitch_rise: 6.0,
            pitch_run: 12.0,
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Eave, a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0), za: 2.4, zb: 2.4, ..Default::default() },
                RoofEdge { kind: RoofEdgeKind::Eave, a: Vec2::new(0.0, 0.0), b: Vec2::new(0.0, 8.0), za: 2.4, zb: 2.4, ..Default::default() },
            ],
            ..Roof::default()
        });
        let polys = proxy_roof_polylines(&m);
        assert_eq!(polys.len(), 0, "roof with only eave edges should produce no proxies");
    }

    #[test]
    fn roof_edge_with_non_finite_endpoints_is_skipped() {
        let mut m = ArchModel::default();
        m.push_roof(Roof {
            level: 0,
            pitch_rise: 6.0,
            pitch_run: 12.0,
            edges: vec![
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(f64::NAN, 4.0), b: Vec2::new(6.0, 4.0), za: 4.45, zb: 4.45, ..Default::default() },
                RoofEdge {
                    kind: RoofEdgeKind::Ridge,
                    a: Vec2::new(4.0, f64::INFINITY),
                    b: Vec2::new(6.0, 4.0),
                    za: 4.45,
                    zb: 4.45,
                    ..Default::default()
                },
                RoofEdge { kind: RoofEdgeKind::Ridge, a: Vec2::new(4.0, 4.0), b: Vec2::new(6.0, 4.0), za: 4.45, zb: 4.45, ..Default::default() },
            ],
            ..Roof::default()
        });
        let polys = proxy_roof_polylines(&m);
        assert_eq!(polys.len(), 1, "only the finite edge should produce a proxy");
    }

    #[test]
    fn multiple_roofs_produce_proxies_for_all() {
        let mut m = ArchModel::default();
        m.push_roof(Roof {
            level: 0,
            pitch_rise: 6.0,
            pitch_run: 12.0,
            edges: vec![RoofEdge {
                kind: RoofEdgeKind::Ridge,
                a: Vec2::new(4.0, 4.0),
                b: Vec2::new(6.0, 4.0),
                za: 4.45,
                zb: 4.45,
                ..Default::default()
            }],
            ..Roof::default()
        });
        m.push_roof(Roof {
            level: 1,
            pitch_rise: 4.0,
            pitch_run: 12.0,
            edges: vec![RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(0.0, 0.0), b: Vec2::new(5.0, 5.0), za: 2.4, zb: 3.0, ..Default::default() }],
            ..Roof::default()
        });
        let polys = proxy_roof_polylines(&m);
        assert_eq!(polys.len(), 2, "each roof should produce one proxy");
    }

    #[test]
    fn multiple_roofs_round_trip_through_dxf() {
        let mut d = Drawing::new_metric();
        d.ensure_layer(ARCH_ROOF_LAYER);
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_roof(Roof {
            level: 0,
            pitch_rise: 6.0,
            pitch_run: 12.0,
            edges: vec![RoofEdge {
                kind: RoofEdgeKind::Ridge,
                a: Vec2::new(4.0, 4.0),
                b: Vec2::new(6.0, 4.0),
                za: 4.45,
                zb: 4.45,
                ..Default::default()
            }],
            ..Roof::default()
        });
        d.arch.push_roof(Roof {
            level: 1,
            pitch_rise: 4.0,
            pitch_run: 12.0,
            edges: vec![RoofEdge { kind: RoofEdgeKind::Hip, a: Vec2::new(0.0, 0.0), b: Vec2::new(5.0, 5.0), za: 2.4, zb: 3.0, ..Default::default() }],
            ..Roof::default()
        });

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.roofs.len(), 2, "both roofs should survive round trip");
        assert_eq!(back.arch.roofs[0].edges.len(), 1);
        assert_eq!(back.arch.roofs[1].edges.len(), 1);

        // No proxy linework should survive
        let polylines = back.model.iter().filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_))).count();
        assert_eq!(polylines, 0, "proxy linework survived the read");
    }

    #[test]
    fn roof_edge_count_is_capped() {
        let mut m = ArchModel::default();
        let mut edges = Vec::new();
        for i in 0..(cadcraft_doc::arch::MAX_ROOF_EDGES + 10) {
            edges.push(RoofEdge {
                kind: RoofEdgeKind::Ridge,
                a: Vec2::new(i as f64, 0.0),
                b: Vec2::new((i + 1) as f64, 0.0),
                za: 2.4,
                zb: 2.4,
                ..Default::default()
            });
        }
        m.push_roof(Roof { level: 0, pitch_rise: 6.0, pitch_run: 12.0, edges, ..Roof::default() });
        let polys = proxy_roof_polylines(&m);
        assert_eq!(polys.len(), cadcraft_doc::arch::MAX_ROOF_EDGES, "edge count should be capped at MAX_ROOF_EDGES");
    }

    /// A room with a Unicode name (Cyrillic + emoji) survives a DXF round trip.
    #[test]
    fn room_with_unicode_name_round_trips() {
        let mut d = Drawing::new_metric();
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Спальня \u{1F6CF}\u{FE0F}".into(),
            ..Room::default()
        });
        d.ensure_layer("A-ROOM");
        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 1, "room lost on DXF round trip");
        let got = &back.arch.rooms[0];
        assert_eq!(got.id, rid, "room id changed on round trip");
        assert_eq!(got.name, "Спальня \u{1F6CF}\u{FE0F}", "room name changed on round trip");
    }

    /// Mitred join rings have the correct vertex count tag (not hardcoded to 4).
    #[test]
    fn proxy_polylines_use_correct_vertex_count() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 2, "expected two wall proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
            assert!(vertex_count >= 4, "mitred ring should have at least 4 vertices");
        }
    }

    /// Collinear walls each produce their own proxy with correct vertex count.
    #[test]
    fn proxy_polylines_collinear_walls() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 2, "both collinear walls should produce proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
        }
    }

    /// Angled (non-90 degree) wall joins produce correct proxy rings.
    #[test]
    fn proxy_polylines_angled_walls() {
        let mut m = ArchModel::default();
        let angle = std::f64::consts::PI / 3.0;
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0 + 8.0 * angle.cos(), 8.0 * angle.sin()), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 2, "both angled walls should produce proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
        }
    }

    /// T-junction walls produce correct proxy rings.
    #[test]
    fn proxy_polylines_empty_when_no_walls() {
        let mut m = ArchModel::default();
        // Add an opening but no walls - should produce no wall proxies
        let wid = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));
        // Now remove the wall to test no-walls case
        m.walls.clear();

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 0, "no walls should produce no wall proxies");
    }

    #[test]
    fn proxy_polylines_very_thin_wall() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.001));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 1, "very thin wall should still produce a proxy");

        let tags = &polys[0];
        let xs: Vec<f64> = tags.iter().filter(|t| t.code == 10).map(|t| t.f64()).collect();
        let ys: Vec<f64> = tags.iter().filter(|t| t.code == 20).map(|t| t.f64()).collect();
        assert_eq!(xs.len(), 4, "expected 4 x coords for thin wall");
        assert_eq!(ys.len(), 4, "expected 4 y coords for thin wall");
        // All coordinates should be finite
        assert!(xs.iter().all(|v| v.is_finite()), "x coords must be finite");
        assert!(ys.iter().all(|v| v.is_finite()), "y coords must be finite");
    }

    #[test]
    fn proxy_polylines_triangle_ring() {
        // Three walls forming a triangle
        let mut m = ArchModel::default();
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        let c = Vec2::new(5.0, 8.0);
        m.push_wall(Wall::new(a, b, 0.2));
        m.push_wall(Wall::new(b, c, 0.2));
        m.push_wall(Wall::new(c, a, 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 3, "triangle should produce 3 wall proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
            // A triangle corner should have at least 3 vertices
            assert!(vertex_count >= 3, "triangle corner should have at least 3 vertices");
        }
    }

    #[test]
    fn proxy_polylines_t_junction() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 2, "both T-junction walls should produce proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
        }
    }

    /// Walls with NaN endpoints are skipped; valid walls still produce proxies.
    #[test]
    fn proxy_polylines_nan_coordinates() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(f64::NAN, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 1, "NaN wall should be skipped, valid wall should produce proxy");
    }

    /// Walls at huge coordinates produce finite proxy polylines.
    #[test]
    fn proxy_polylines_huge_coordinates() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(1e15, 1e15), Vec2::new(1e15 + 10.0, 1e15), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 1, "wall at huge coordinates should produce a proxy");

        let tags = &polys[0];
        let xs: Vec<f64> = tags.iter().filter(|t| t.code == 10).map(|t| t.f64()).collect();
        let ys: Vec<f64> = tags.iter().filter(|t| t.code == 20).map(|t| t.f64()).collect();
        assert!(xs.iter().all(|v| v.is_finite()), "x coords must be finite");
        assert!(ys.iter().all(|v| v.is_finite()), "y coords must be finite");
    }

    /// Opening proxy polylines have the correct vertex count tag.
    #[test]
    fn proxy_opening_polylines_use_correct_vertex_count() {
        let mut m = ArchModel::default();
        let wid = m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_opening(Opening::new(wid, 1.0, 0.9, 2.1, OpeningKind::Door));

        let polys = proxy_opening_polylines(&m);
        assert_eq!(polys.len(), 1, "expected one opening proxy");

        let (_, tags) = &polys[0];
        let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
        let actual_points = tags.iter().filter(|t| t.code == 10).count();
        assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
    }

    /// A version-2 payload (which predates rooms) parses with an empty rooms vector.
    #[test]
    fn v2_payload_parses_with_empty_rooms() {
        let mut m = one_wall_model();
        let json = serde_json::json!({ "version": 2, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text).expect("a version-2 payload must still parse");
        assert_eq!(parsed.walls.len(), 1);
        assert!(parsed.rooms.is_empty(), "a v2 file predates rooms");
    }

    /// Two walls crossing in the middle (X-junction) produce correct proxy rings.
    #[test]
    fn proxy_polylines_x_junction() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(5.0, 10.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 2, "both X-junction walls should produce proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
        }
    }

    /// Two walls on the same line (coincident) produce correct proxy rings.
    #[test]
    fn proxy_polylines_coincident_walls() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));

        let polys = proxy_polylines(&m);
        // Coincident walls may merge or produce separate rings depending on the solver
        assert!(polys.len() >= 1, "coincident walls should produce at least one proxy");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
        }
    }

    /// A wall with NaN thickness is skipped.
    #[test]
    fn proxy_polylines_nan_thickness() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), f64::NAN));
        m.push_wall(Wall::new(Vec2::new(0.0, 1.0), Vec2::new(10.0, 1.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 1, "NaN-thickness wall should be skipped, valid wall should produce proxy");
    }

    /// A wall with negative thickness is skipped.
    #[test]
    fn proxy_polylines_negative_thickness() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), -0.2));
        m.push_wall(Wall::new(Vec2::new(0.0, 1.0), Vec2::new(10.0, 1.0), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 1, "negative-thickness wall should be skipped, valid wall should produce proxy");
    }

    /// A complex layout with multiple join types (L, T, X, angled) produces correct proxies.
    #[test]
    fn proxy_polylines_complex_layout() {
        let mut m = ArchModel::default();
        // L-shape
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        m.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
        // T-junction
        m.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(5.0, 4.0), 0.2));
        // Angled wall
        let angle = std::f64::consts::PI / 4.0;
        m.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0 * angle.cos(), 5.0 * angle.sin()), 0.2));

        let polys = proxy_polylines(&m);
        assert_eq!(polys.len(), 4, "all four walls should produce proxies");

        for tags in &polys {
            let vertex_count = tags.iter().find(|t| t.code == 90).map(|t| t.i64()).unwrap_or(0);
            let actual_points = tags.iter().filter(|t| t.code == 10).count();
            assert_eq!(vertex_count as usize, actual_points, "vertex count tag must match actual points");
            // All coordinates should be finite
            let xs: Vec<f64> = tags.iter().filter(|t| t.code == 10).map(|t| t.f64()).collect();
            let ys: Vec<f64> = tags.iter().filter(|t| t.code == 20).map(|t| t.f64()).collect();
            assert!(xs.iter().all(|v| v.is_finite()), "x coords must be finite");
            assert!(ys.iter().all(|v| v.is_finite()), "y coords must be finite");
        }
    }

    /// Mitred join rings survive a DXF round trip with the model intact.
    #[test]
    fn proxy_polylines_mitred_round_trip() {
        let mut d = Drawing::new_metric();
        d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        d.arch.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        // The model should be intact after round trip
        assert_eq!(back.arch.walls.len(), 2, "walls lost on round trip");
        assert_eq!(back.arch.walls[0].thickness, 0.2);
        assert_eq!(back.arch.walls[1].thickness, 0.2);

        // No proxy polylines should survive the read
        let polylines = back.model.iter().filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_))).count();
        assert_eq!(polylines, 0, "proxy linework survived the read");
    }

    /// A model with stairs and a railing survives a DXF round trip as a v6 payload.
    #[test]
    fn arch_v5_round_trips_stairs() {
        let mut d = Drawing::new_metric();
        d.arch.push_stair(Stair::default());
        d.arch.push_railing(Railing::default());

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.stairs.len(), 1, "stair lost on DXF round trip");
        assert_eq!(back.arch.railings.len(), 1, "railing lost on DXF round trip");

        // The record should carry version 6.
        let chunks = arch_chunks(&d.arch);
        assert!(chunks.is_some(), "arch_chunks should produce a v6 record with stairs/railings");
        let obj: serde_json::Value = serde_json::from_str(&chunks.unwrap().join("")).unwrap();
        assert_eq!(obj["version"].as_u64(), Some(6), "written payload version should be 6");
    }

    /// A v4 payload that carries stairs/railings clears them on parse.
    #[test]
    fn v4_payload_clears_stairs() {
        let mut m = ArchModel::default();
        m.push_stair(Stair::default());
        m.push_railing(Railing::default());
        let json = serde_json::json!({ "version": 4, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text).expect("a version-4 payload must still parse");
        assert!(parsed.stairs.is_empty(), "a v4 payload must clear stairs");
        assert!(parsed.railings.is_empty(), "a v4 payload must clear railings");
    }

    /// A v5 payload with more stairs than [`MAX_STAIRS`] is rejected.
    #[test]
    fn hostile_stair_payload_is_rejected() {
        let mut m = ArchModel::default();
        for _ in 0..(cadcraft_doc::arch::MAX_STAIRS + 1) {
            m.push_stair(Stair::default());
        }
        let json = serde_json::json!({ "version": 5, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        assert!(parse_arch(&text).is_none(), "a payload with {} stairs must be rejected", cadcraft_doc::arch::MAX_STAIRS + 1,);
    }

    /// A model with only railings (no walls, openings, rooms, roofs, stairs) still writes the record.
    #[test]
    fn arch_chunks_writes_record_for_railings_only() {
        let mut m = ArchModel::default();
        m.push_railing(Railing::default());
        let chunks = arch_chunks(&m);
        assert!(chunks.is_some(), "a model with only a railing must write an XRECORD");
    }

    /// A model with only stairs (no walls, openings, rooms, roofs, railings) still writes the record.
    #[test]
    fn arch_chunks_writes_record_for_stairs_only() {
        let mut m = ArchModel::default();
        m.push_stair(Stair::default());
        let chunks = arch_chunks(&m);
        assert!(chunks.is_some(), "a model with only a stair must write an XRECORD");
    }

    /// A v5 payload with more railings than [`MAX_RAILINGS`] is rejected.
    #[test]
    fn hostile_railing_payload_is_rejected() {
        let mut m = ArchModel::default();
        for _ in 0..(cadcraft_doc::arch::MAX_RAILINGS + 1) {
            m.push_railing(Railing::default());
        }
        let json = serde_json::json!({ "version": 5, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        assert!(parse_arch(&text).is_none(), "a payload with {} railings must be rejected", cadcraft_doc::arch::MAX_RAILINGS + 1,);
    }

    /// A v5 payload missing the stairs and railings fields still parses (they default to empty).
    #[test]
    fn v5_payload_missing_stairs_railings_fields() {
        let mut m = one_wall_model();
        let json = serde_json::json!({ "version": 5, "model": m }).to_string();
        let text = crate::dxf_ext::json_chunks(&json).join("");
        let parsed = parse_arch(&text).expect("a v5 payload without stairs/railings fields must parse");
        assert_eq!(parsed.walls.len(), 1);
        assert!(parsed.stairs.is_empty());
        assert!(parsed.railings.is_empty());
    }

    /// Multiple stairs and railings survive a DXF round trip.
    #[test]
    fn multiple_stairs_railings_round_trip() {
        let mut d = Drawing::new_metric();
        d.arch.push_stair(Stair::default());
        d.arch.push_stair(Stair::default());
        d.arch.push_railing(Railing::default());
        d.arch.push_railing(Railing::default());

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.stairs.len(), 2, "stairs lost on DXF round trip");
        assert_eq!(back.arch.railings.len(), 2, "railings lost on DXF round trip");
    }

    /// A deck room's `room_type`, `deck_height` and `holes` survive a DXF round trip
    /// through the arch XRECORD.
    #[test]
    fn deck_height_round_trips() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Deck".into(),
            room_type: RoomType::Deck,
            deck_height: Some(0.9),
            ..Room::default()
        });
        let want = d.arch.rooms[0].clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 1, "deck room lost on the DXF round trip");
        let got = &back.arch.rooms[0];
        assert_eq!(got.id, rid, "room id changed on round trip");
        assert_eq!(got.name, want.name);
        assert_eq!(got.room_type, want.room_type, "room type lost on round trip");
        assert_eq!(got.deck_height, want.deck_height, "deck height lost on round trip");
        assert_eq!(got.holes, want.holes);
        assert_eq!(got.polygon, want.polygon);
    }

    /// A room's `holes` ring survives a DXF round trip through the arch XRECORD.
    #[test]
    fn room_holes_round_trip() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(6.0, 4.0), Vec2::new(0.0, 4.0)],
            name: "Room with hole".into(),
            holes: vec![vec![Vec2::new(1.0, 1.0), Vec2::new(3.0, 1.0), Vec2::new(3.0, 2.0), Vec2::new(1.0, 2.0)]],
            ..Room::default()
        });
        let want = d.arch.rooms[0].clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 1, "room lost on the DXF round trip");
        let got = &back.arch.rooms[0];
        assert_eq!(got.id, rid, "room id changed on round trip");
        assert_eq!(got.name, want.name);
        assert_eq!(got.holes, want.holes, "room holes lost on round trip");
        assert_eq!(got.polygon, want.polygon);
    }

    /// A room with multiple hole rings survives a DXF round trip.
    #[test]
    fn room_with_multiple_holes_round_trips() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)],
            name: "Room with two holes".into(),
            holes: vec![
                vec![Vec2::new(1.0, 1.0), Vec2::new(3.0, 1.0), Vec2::new(3.0, 3.0), Vec2::new(1.0, 3.0)],
                vec![Vec2::new(6.0, 4.0), Vec2::new(9.0, 4.0), Vec2::new(9.0, 7.0), Vec2::new(6.0, 7.0)],
            ],
            ..Room::default()
        });
        let want = d.arch.rooms[0].clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 1, "room lost on the DXF round trip");
        let got = &back.arch.rooms[0];
        assert_eq!(got.id, rid, "room id changed on round trip");
        assert_eq!(got.name, want.name);
        assert_eq!(got.holes.len(), 2, "hole count changed on round trip");
        assert_eq!(got.holes, want.holes, "room holes lost on round trip");
        assert_eq!(got.polygon, want.polygon);
    }

    /// A Deck room with `deck_height: None` survives a DXF round trip.
    #[test]
    fn deck_height_none_round_trips() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        let rid = d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Deck no height".into(),
            room_type: RoomType::Deck,
            deck_height: None,
            ..Room::default()
        });
        let want = d.arch.rooms[0].clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 1, "deck room lost on the DXF round trip");
        let got = &back.arch.rooms[0];
        assert_eq!(got.id, rid, "room id changed on round trip");
        assert_eq!(got.name, want.name);
        assert_eq!(got.room_type, want.room_type, "room type lost on round trip");
        assert_eq!(got.deck_height, want.deck_height, "deck height (None) lost on round trip");
        assert_eq!(got.polygon, want.polygon);
    }

    /// A room with a NaN deck height survives a DXF round trip without panicking.
    #[test]
    fn deck_height_nan_round_trips() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Room NaN height".into(),
            deck_height: Some(f64::NAN),
            ..Room::default()
        });

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes());
        assert!(back.is_ok(), "NaN deck height must not break DXF round trip");
        let back = back.unwrap();
        assert_eq!(back.arch.rooms.len(), 1, "room lost on the DXF round trip");
    }

    /// Multiple rooms with different combinations of room_type, deck_height and holes
    /// all survive a DXF round trip.
    #[test]
    fn multiple_rooms_mixed_attributes_round_trip() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");

        // Room 1: Deck with height and one hole
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)],
            name: "Deck".into(),
            room_type: RoomType::Deck,
            deck_height: Some(0.9),
            holes: vec![vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(1.0, 2.0)]],
            ..Room::default()
        });

        // Room 2: Bedroom with no height and no holes
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(5.0, 0.0), Vec2::new(9.0, 0.0), Vec2::new(9.0, 3.0), Vec2::new(5.0, 3.0)],
            name: "Bedroom".into(),
            room_type: RoomType::Bedroom,
            deck_height: None,
            ..Room::default()
        });

        // Room 3: Living room with holes but no deck height
        d.arch.push_room(Room {
            polygon: vec![Vec2::new(0.0, 4.0), Vec2::new(6.0, 4.0), Vec2::new(6.0, 8.0), Vec2::new(0.0, 8.0)],
            name: "Living".into(),
            room_type: RoomType::Living,
            deck_height: None,
            holes: vec![vec![Vec2::new(1.0, 5.0), Vec2::new(3.0, 5.0), Vec2::new(3.0, 7.0), Vec2::new(1.0, 7.0)]],
            ..Room::default()
        });

        let want = d.arch.rooms.clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), 3, "rooms lost on the DXF round trip");
        for (i, (got, w)) in back.arch.rooms.iter().zip(want.iter()).enumerate() {
            assert_eq!(got.id, w.id, "room {} id changed on round trip", i);
            assert_eq!(got.name, w.name, "room {} name changed on round trip", i);
            assert_eq!(got.room_type, w.room_type, "room {} type changed on round trip", i);
            assert_eq!(got.deck_height, w.deck_height, "room {} deck_height changed on round trip", i);
            assert_eq!(got.holes, w.holes, "room {} holes changed on round trip", i);
            assert_eq!(got.polygon, w.polygon, "room {} polygon changed on round trip", i);
        }
    }

    /// Every RoomType variant survives a DXF round trip.
    #[test]
    fn all_room_types_round_trip() {
        let types = [
            RoomType::Living,
            RoomType::Bedroom,
            RoomType::Kitchen,
            RoomType::Bath,
            RoomType::Closet,
            RoomType::Utility,
            RoomType::Garage,
            RoomType::Porch,
            RoomType::Deck,
            RoomType::OpenBelow,
            RoomType::Other,
        ];

        let mut d = Drawing::new_metric();
        d.ensure_layer("A-ROOM");
        for (i, t) in types.iter().enumerate() {
            d.arch.push_room(Room {
                polygon: vec![
                    Vec2::new(i as f64 * 5.0, 0.0),
                    Vec2::new((i as f64 + 1.0) * 5.0, 0.0),
                    Vec2::new((i as f64 + 1.0) * 5.0, 4.0),
                    Vec2::new(i as f64 * 5.0, 4.0),
                ],
                name: format!("Room {:?}", t),
                room_type: *t,
                ..Room::default()
            });
        }

        let want = d.arch.rooms.clone();

        let text = write_dxf(&d);
        let back = read_dxf(text.as_bytes()).unwrap();

        assert_eq!(back.arch.rooms.len(), types.len(), "rooms lost on the DXF round trip");
        for (i, (got, w)) in back.arch.rooms.iter().zip(want.iter()).enumerate() {
            assert_eq!(got.id, w.id, "room {} id changed on round trip", i);
            assert_eq!(got.room_type, w.room_type, "room {} type changed on round trip", i);
        }
    }
}
