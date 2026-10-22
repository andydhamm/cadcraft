#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod graph;
pub mod join;
pub mod wall;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use cadcraft_doc::{ArchModel, Wall};
    use cadcraft_geom::Vec2;

    use crate::wall::wall_polygon;

    #[test]
    fn arch_wall_bounds_through_reexport() {
        let mut model = ArchModel::default();
        let w = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 10.0), 0.2);
        model.add_wall(w);
        let entry = &model.walls[0];
        let poly = wall_polygon(entry).unwrap();
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in &poly {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
        let b = cadcraft_geom::Bounds2 { min: Vec2::new(min_x, min_y), max: Vec2::new(max_x, max_y) };
        assert!((b.min.x - (-0.1)).abs() < 1e-9);
    }
}
