use cadcraft_doc::*;
use cadcraft_geom::Vec2;

#[test]
fn touching_a_non_empty_arch_model_makes_the_drawing_metres() {
    let mut d = Drawing::new_metric();
    assert_eq!(d.header.i64("INSUNITS", 0), 4);
    d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 0.2));
    d.touch_arch();
    assert_eq!(d.header.i64("INSUNITS", 0), 6);
    assert_eq!(d.header.i64("MEASUREMENT", 0), 1);
}

#[test]
fn an_empty_arch_model_leaves_insunits_alone() {
    let mut d = Drawing::new_metric();
    d.touch_arch();
    assert_eq!(d.header.i64("INSUNITS", 0), 4);
}
