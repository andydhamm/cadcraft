//! Floor slabs and ceiling surfaces generated from room polygons.
//!
//! Both functions produce a single `Mesh` per room via `push_prism_with_holes`,
//! so the resulting geometry is always manifold and carries the room's
//! `ArchId` as its `object` field.

use cadcraft_doc::ArchId;
use cadcraft_doc::arch::{Level, Room, RoomType};
use cadcraft_geom::Vec2;

use crate::mesh::Mesh;

/// Build a floor-slab mesh for `room` at `level`.
///
/// The slab's top face sits exactly at `level.elevation` and it is extruded
/// *down* by `level.structure` (the floor-structure thickness).
///
/// Returns `None` when:
/// - the room type has no floor (`RoomType::has_floor` is false, e.g. `OpenBelow`),
/// - `level.structure` is non-finite or <= 0,
/// - `level.elevation` is non-finite,
/// - the resulting mesh has no vertices (e.g. empty polygon).
pub fn floor_slab(room: &Room, level: &Level, mat: &str) -> Option<Mesh> {
    if !room.room_type.has_floor() {
        return None;
    }
    if !level.elevation.is_finite() {
        return None;
    }

    // Deck slab logic
    if room.room_type == RoomType::Deck {
        if let Some(h) = room.deck_height {
            if h.is_finite() && h > 0.0 {
                let top = level.elevation + h;
                let mut m = Mesh::new("deck", room.id);
                m.level_elevation = level.elevation;
                // 0.05 thick
                m.push_prism_with_holes(&room.polygon, &room.holes, top - 0.05, top);
                if m.positions.is_empty() { None } else { Some(m) }
            } else {
                // Invalid deck height, fall through to standard slab or None?
                // Prompt says "Otherwise unchanged". If deck_height is Some but invalid,
                // we probably shouldn't emit a deck slab.
                // If it's None, we emit standard slab.
                // If it's Some(invalid), we treat as no deck slab?
                // "deck_height is Some(h) finite and > 0 ... Otherwise unchanged."
                // So if not (finite and > 0), we do the standard logic.
                if !level.structure.is_finite() || level.structure <= 0.0 {
                    return None;
                }
                let top = level.elevation;
                let mut m = Mesh::new(mat, room.id);
                m.level_elevation = level.elevation;
                m.push_prism_with_holes(&room.polygon, &room.holes, top - level.structure, top);
                if m.positions.is_empty() { None } else { Some(m) }
            }
        } else {
            // No deck height, standard slab
            if !level.structure.is_finite() || level.structure <= 0.0 {
                return None;
            }
            let top = level.elevation;
            let mut m = Mesh::new(mat, room.id);
            m.level_elevation = level.elevation;
            m.push_prism_with_holes(&room.polygon, &room.holes, top - level.structure, top);
            if m.positions.is_empty() { None } else { Some(m) }
        }
    } else {
        // Standard room
        if !level.structure.is_finite() || level.structure <= 0.0 {
            return None;
        }
        let top = level.elevation;
        let mut m = Mesh::new(mat, room.id);
        m.level_elevation = level.elevation;
        m.push_prism_with_holes(&room.polygon, &room.holes, top - level.structure, top);
        if m.positions.is_empty() { None } else { Some(m) }
    }
}

/// Build a ceiling-surface mesh for `room` at `level`.
///
/// The surface is a thin 0.02-unit slab whose top face sits at
/// `level.elevation + ch` where `ch` is `room.ceiling_height` (if finite and
/// positive) or `level.ceiling_height` as fallback.
///
/// Returns `None` when:
/// - the room type has no ceiling (`RoomType::has_ceiling` is false, e.g.
///   `Deck`, `Porch`),
/// - the effective ceiling height is non-finite or <= 0.02,
/// - the resulting mesh has no vertices (e.g. empty polygon).
pub fn ceiling_surface(room: &Room, level: &Level, mat: &str) -> Option<Mesh> {
    if !room.room_type.has_ceiling() {
        return None;
    }
    let ch = room.ceiling_height.filter(|h| h.is_finite() && *h > 0.0).unwrap_or(level.ceiling_height);
    if !ch.is_finite() || ch <= 0.02 {
        return None;
    }
    let top = level.elevation + ch;
    let mut m = Mesh::new(mat, room.id);
    m.level_elevation = level.elevation;
    m.push_prism_with_holes(&room.polygon, &room.holes, top - 0.02, top);
    if m.positions.is_empty() { None } else { Some(m) }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::f64::INFINITY;
    use std::f64::NEG_INFINITY;

    /// 4x4 square polygon.
    fn sq() -> Vec<Vec2> {
        vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)]
    }

    /// Sum the areas of triangles whose three vertices all sit at `top_z`.
    fn top_cap_area(m: &Mesh, top_z: f64) -> f64 {
        let tri_count = m.indices.len() / 3;
        let mut area = 0.0;
        for i in 0..tri_count {
            let i0 = m.indices[i * 3] as usize;
            let i1 = m.indices[i * 3 + 1] as usize;
            let i2 = m.indices[i * 3 + 2] as usize;
            let v0 = m.positions[i0];
            let v1 = m.positions[i1];
            let v2 = m.positions[i2];
            let z0 = v0[2] as f64;
            let z1 = v1[2] as f64;
            let z2 = v2[2] as f64;
            if (z0 - top_z).abs() < 1e-4 && (z1 - top_z).abs() < 1e-4 && (z2 - top_z).abs() < 1e-4 {
                let dx1 = v1[0] as f64 - v0[0] as f64;
                let dy1 = v1[1] as f64 - v0[1] as f64;
                let dz1 = v1[2] as f64 - v0[2] as f64;
                let dx2 = v2[0] as f64 - v0[0] as f64;
                let dy2 = v2[1] as f64 - v0[1] as f64;
                let dz2 = v2[2] as f64 - v0[2] as f64;
                let cx = dy1 * dz2 - dz1 * dy2;
                let cy = dz1 * dx2 - dx1 * dz2;
                let cz = dx1 * dy2 - dy1 * dx2;
                area += 0.5 * (cx * cx + cy * cy + cz * cz).sqrt();
            }
        }
        area
    }

    fn default_level(elevation: f64, structure: f64) -> Level {
        Level { elevation, structure, ..Default::default() }
    }

    #[test]
    fn floor_slab_top_is_the_level_elevation() {
        let level = default_level(2.7, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("floor slab");
        assert_eq!(slab.material, "floor");
        assert_eq!(slab.object, room.id);
        let z_min = slab.positions.iter().map(|p| p[2] as f64).fold(INFINITY, f64::min);
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - 2.7).abs() < 1e-6, "z_max = {z_max}, expected 2.7");
        assert!((z_min - 2.4).abs() < 1e-6, "z_min = {z_min}, expected 2.4");
    }

    #[test]
    fn ceiling_surface_is_at_the_ceiling_height() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        let ceil = ceiling_surface(&room, &level, "ceiling").expect("ceiling surface");
        let z_min = ceil.positions.iter().map(|p| p[2] as f64).fold(INFINITY, f64::min);
        let z_max = ceil.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - 2.4).abs() < 1e-6, "z_max = {z_max}, expected 2.4");
        assert!((z_min - 2.38).abs() < 1e-6, "z_min = {z_min}, expected 2.38");

        // Room-specific ceiling height overrides the level default.
        let room2 = Room { ceiling_height: Some(3.0), ..room };
        let ceil2 = ceiling_surface(&room2, &level, "ceiling").expect("ceiling surface");
        let z_max2 = ceil2.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max2 - 3.0).abs() < 1e-6, "z_max = {z_max2}, expected 3.0");
    }

    #[test]
    fn open_below_room_gets_no_floor() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::OpenBelow, polygon: sq(), ..Default::default() };
        assert!(floor_slab(&room, &level, "floor").is_none());
        assert!(ceiling_surface(&room, &level, "ceiling").is_some());
    }

    #[test]
    fn deck_and_porch_get_no_ceiling() {
        let level = default_level(0.0, 0.3);
        for rt in [RoomType::Deck, RoomType::Porch] {
            let room = Room { id: ArchId(1), room_type: rt, polygon: sq(), ..Default::default() };
            assert!(ceiling_surface(&room, &level, "ceiling").is_none());
            assert!(floor_slab(&room, &level, "floor").is_some());
        }
    }

    #[test]
    fn slab_hole_follows_the_room_hole() {
        let level = default_level(0.0, 0.3);
        let room = Room {
            id: ArchId(1),
            room_type: RoomType::Living,
            polygon: sq(),
            holes: vec![vec![Vec2::new(1.5, 1.5), Vec2::new(2.5, 1.5), Vec2::new(2.5, 2.5), Vec2::new(1.5, 2.5)]],
            ..Default::default()
        };
        let slab = floor_slab(&room, &level, "floor").expect("floor slab");
        assert!(slab.is_manifold());
        // Top-cap area: 4x4 - 1x1 = 15.
        let area = top_cap_area(&slab, 0.0);
        assert!((area - 15.0).abs() < 1e-6, "top-cap area = {area}, expected 15.0");
    }

    #[test]
    fn floor_slab_nan_elevation_returns_none() {
        let level = Level { elevation: f64::NAN, structure: 0.3, ..Default::default() };
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        assert!(floor_slab(&room, &level, "floor").is_none());
    }

    #[test]
    fn ceiling_surface_nan_height_returns_none() {
        let level = Level { ceiling_height: f64::NAN, ..Default::default() };
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        assert!(ceiling_surface(&room, &level, "ceiling").is_none());
    }

    #[test]
    fn zero_structure_gives_no_slab() {
        let level = default_level(0.0, 0.0);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        assert!(floor_slab(&room, &level, "floor").is_none());

        // Empty polygon also yields no mesh.
        let room_empty = Room { id: ArchId(1), room_type: RoomType::Living, polygon: Vec::new(), ..Default::default() };
        let level2 = default_level(0.0, 0.3);
        assert!(floor_slab(&room_empty, &level2, "floor").is_none());
        assert!(ceiling_surface(&room_empty, &level2, "ceiling").is_none());
    }

    #[test]
    fn floor_slab_nan_structure_returns_none() {
        let level = Level { structure: f64::NAN, ..Default::default() };
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        assert!(floor_slab(&room, &level, "floor").is_none());
    }

    #[test]
    fn deck_slab_top_is_at_its_own_height() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Deck, polygon: sq(), deck_height: Some(0.9), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("deck slab");
        assert_eq!(slab.material, "deck");
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - 0.9).abs() < 1e-6, "z_max = {z_max}, expected 0.9");
    }

    #[test]
    fn deck_without_height_keeps_the_grade_slab() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Deck, polygon: sq(), deck_height: None, ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("grade slab");
        assert_eq!(slab.material, "floor");
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - 0.0).abs() < 1e-6, "z_max = {z_max}, expected 0.0");
    }

    #[test]
    fn floor_slab_negative_structure_returns_none() {
        let level = default_level(0.0, -0.1);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        assert!(floor_slab(&room, &level, "floor").is_none());
    }

    #[test]
    fn ceiling_surface_ignores_zero_room_height() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ceiling_height: Some(0.0), ..Default::default() };
        let ceil = ceiling_surface(&room, &level, "ceiling").expect("should fall back to level height");
        let z_max = ceil.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - 2.4).abs() < 1e-6, "z_max = {z_max}, expected 2.4 (level default)");
    }

    #[test]
    fn ceiling_surface_ignores_negative_room_height() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ceiling_height: Some(-1.0), ..Default::default() };
        let ceil = ceiling_surface(&room, &level, "ceiling").expect("should fall back to level height");
        let z_max = ceil.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - 2.4).abs() < 1e-6, "z_max = {z_max}, expected 2.4 (level default)");
    }

    #[test]
    fn ceiling_surface_too_thin_returns_none() {
        let level = Level { ceiling_height: 0.01, ..Default::default() };
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        assert!(ceiling_surface(&room, &level, "ceiling").is_none());
    }

    #[test]
    fn floor_slab_level_elevation_field_is_set() {
        let level = default_level(3.5, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("floor slab");
        assert!((slab.level_elevation - 3.5).abs() < 1e-6, "level_elevation = {}", slab.level_elevation);
    }

    #[test]
    fn ceiling_surface_level_elevation_field_is_set() {
        let level = default_level(3.5, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        let ceil = ceiling_surface(&room, &level, "ceiling").expect("ceiling surface");
        assert!((ceil.level_elevation - 3.5).abs() < 1e-6, "level_elevation = {}", ceil.level_elevation);
    }

    #[test]
    fn floor_slab_negative_elevation_works() {
        let level = default_level(-2.7, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: sq(), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("floor slab");
        let z_min = slab.positions.iter().map(|p| p[2] as f64).fold(INFINITY, f64::min);
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(NEG_INFINITY, f64::max);
        assert!((z_max - (-2.7)).abs() < 1e-6, "z_max = {z_max}, expected -2.7");
        assert!((z_min - (-3.0)).abs() < 1e-6, "z_min = {z_min}, expected -3.0");
    }

    #[test]
    fn floor_slab_huge_polygon_coordinates() {
        let level = default_level(0.0, 0.3);
        let huge = vec![Vec2::new(0.0, 0.0), Vec2::new(1e6, 0.0), Vec2::new(1e6, 1e6), Vec2::new(0.0, 1e6)];
        let room = Room { id: ArchId(1), room_type: RoomType::Living, polygon: huge, ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("floor slab");
        assert!(!slab.positions.is_empty());
        assert!(slab.is_manifold());
    }

    #[test]
    fn deck_slab_with_holes() {
        let level = default_level(0.0, 0.3);
        let room = Room {
            id: ArchId(1),
            room_type: RoomType::Deck,
            polygon: sq(),
            holes: vec![vec![Vec2::new(1.5, 1.5), Vec2::new(2.5, 1.5), Vec2::new(2.5, 2.5), Vec2::new(1.5, 2.5)]],
            deck_height: Some(0.9),
            ..Default::default()
        };
        let slab = floor_slab(&room, &level, "floor").expect("deck slab");
        assert!(slab.is_manifold());
        // Top-cap area: 4x4 - 1x1 = 15.
        let area = top_cap_area(&slab, 0.9);
        assert!((area - 15.0).abs() < 1e-6, "top-cap area = {area}, expected 15.0");
    }

    #[test]
    fn deck_slab_with_nan_height_falls_back_to_grade() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Deck, polygon: sq(), deck_height: Some(f64::NAN), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("grade slab");
        assert_eq!(slab.material, "floor");
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
        assert!((z_max - 0.0).abs() < 1e-6, "z_max = {z_max}, expected 0.0");
    }

    #[test]
    fn deck_slab_with_zero_height_falls_back_to_grade() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Deck, polygon: sq(), deck_height: Some(0.0), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("grade slab");
        assert_eq!(slab.material, "floor");
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
        assert!((z_max - 0.0).abs() < 1e-6, "z_max = {z_max}, expected 0.0");
    }

    #[test]
    fn deck_slab_with_negative_height_falls_back_to_grade() {
        let level = default_level(0.0, 0.3);
        let room = Room { id: ArchId(1), room_type: RoomType::Deck, polygon: sq(), deck_height: Some(-0.5), ..Default::default() };
        let slab = floor_slab(&room, &level, "floor").expect("grade slab");
        assert_eq!(slab.material, "floor");
        let z_max = slab.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
        assert!((z_max - 0.0).abs() < 1e-6, "z_max = {z_max}, expected 0.0");
    }
}
