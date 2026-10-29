//! The `arch` crate is the 2D algorithm layer (L2) for the arch3d feature.
//!
//! The architectural data model (`Wall`, `ArchId`, `ArchModel`, `Level`, `WallType`,
//! `JoinKind`, `RefSide`, `WallLayer`) lives in `cadcraft-doc` (L1) so that it is stored on
//! `Drawing` and participates in undo, autosave, audit and DXF round-trip. This crate contains
//! the pure 2D algorithms that operate on that model:
//!
//! - `wall` — poché/offset geometry, wall bounds, wall construction helpers.
//! - `join` — wall join solver (L/T/X/angled/collinear, mitre/butt, neighbour stretch).
//!
//! Model types are re-exported here so callers of `cadcraft-arch` get them from one place.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub use cadcraft_doc::{
    ArchId, ArchModel, JoinKind, Level, RefSide, Room, RoomType, Wall, WallLayer, WallType, is_room_handle, room_handle, room_label,
};

pub mod area;
pub mod graph;
pub mod join;
pub mod label;
pub mod opening;
pub mod poly;
pub mod railing;
pub mod roof;
pub mod roofedge;
pub mod room;
pub mod stair;
pub mod symbol;
pub mod wall;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use cadcraft_geom::{Bounds2, Vec2};

    #[test]
    fn arch_reexports_model_types() {
        let mut m = ArchModel::default();
        m.push_wall(Wall::new(Vec2::ZERO, Vec2::new(1.0, 0.0), 0.2));
        assert_eq!(m.len(), 1);
        assert!(m.wall(m.next_wall_id()).is_none());
    }

    #[test]
    fn arch_wall_bounds_through_reexport() {
        let w = Wall { start: Vec2::new(0.0, 0.0), end: Vec2::new(10.0, 0.0), thickness: 0.2, ..Wall::default() };
        let b = crate::wall::wall_bounds(&w);
        // Bounds must cover the wall's centerline from start to end.
        assert!(b.min.x <= w.start.x + 1e-9);
        assert!(b.min.y <= w.start.y + 1e-9);
        assert!(b.max.x >= w.end.x - 1e-9);
        assert!(b.max.y >= w.end.y - 1e-9);

        let zero = crate::wall::wall_bounds(&Wall::default());
        assert_eq!(zero, Bounds2::EMPTY);
    }

    #[test]
    fn crate_root_denies_unwrap_and_panic() {
        let src = include_str!("lib.rs");
        assert!(src.contains("deny(clippy::unwrap_used"));
        assert!(src.contains("forbid(unsafe_code)"));
    }

    #[test]
    fn reexports_do_not_shadow_local_modules() {
        // The re-exported Wall type must not shadow crate::wall (the module).
        let w = crate::wall::wall_bounds(&Wall::default());
        assert_eq!(w, Bounds2::EMPTY);
    }

    #[test]
    fn wall_bounds_with_nan_coordinates() {
        let w = Wall { start: Vec2::new(f64::NAN, 0.0), end: Vec2::new(1.0, 0.0), thickness: 0.2, ..Wall::default() };
        let b = crate::wall::wall_bounds(&w);
        // Must not panic; bounds should be well-defined (finite or NaN).
        assert!(b.min.x.is_finite() || b.min.x.is_nan());
        assert!(b.max.x.is_finite() || b.max.x.is_nan());
    }

    #[test]
    fn wall_bounds_with_huge_coordinates() {
        let w = Wall { start: Vec2::new(-1e15, 0.0), end: Vec2::new(1e15, 0.0), thickness: 0.2, ..Wall::default() };
        let b = crate::wall::wall_bounds(&w);
        assert!(b.min.x <= -1e15 + 1e-9);
        assert!(b.max.x >= 1e15 - 1e-9);
    }

    #[test]
    fn wall_bounds_zero_length_with_thickness() {
        let w = Wall { start: Vec2::new(5.0, 3.0), end: Vec2::new(5.0, 3.0), thickness: 0.4, ..Wall::default() };
        let b = crate::wall::wall_bounds(&w);
        assert!(b.min.x <= 5.0 + 1e-9);
        assert!(b.max.x >= 5.0 - 1e-9);
        assert!(b.min.y <= 3.0 + 1e-9);
        assert!(b.max.y >= 3.0 - 1e-9);
    }

    #[test]
    fn arch_model_id_increments() {
        let mut m = ArchModel::default();
        let id1 = m.push_wall(Wall::new(Vec2::ZERO, Vec2::new(1.0, 0.0), 0.2));
        let id2 = m.push_wall(Wall::new(Vec2::ZERO, Vec2::new(0.0, 1.0), 0.2));
        assert_ne!(id1, id2);
        assert_eq!(m.len(), 2);
        assert!(m.wall(id1).is_some());
        assert!(m.wall(id2).is_some());
    }
}
