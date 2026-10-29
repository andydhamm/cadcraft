//! Arch linework shows through paper-space viewports and layout plots, at the viewport's scale,
//! clipped to it, following its layer state.

use cadcraft_doc::arch::Wall;
use cadcraft_doc::{Common, Drawing, EntityKind, Space, Viewport};
use cadcraft_geom::{Bounds2, Vec2};
use cadcraft_render::{DisplayList, Options, VIEWPORT_CONTENT, arch_handle, build, build_plot};

const SCALE: f64 = 20.0;

/// A 10 x 8 m house (centre lines, 0.2 m walls: a 10.2 m outline) and a layout with one viewport
/// centred on it at 20 paper units per model unit.
fn house(vp_on: bool, frozen: &[&str]) -> Drawing {
    let mut d = Drawing::new_imperial();
    let pts = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)];
    for i in 0..4 {
        let w = Wall { start: pts[i], end: pts[(i + 1) % 4], thickness: 0.2, height: 2.7, ..Wall::default() };
        d.arch.push_wall(w);
    }
    let vp = Viewport {
        center: Vec2::new(150.0, 120.0).to3(0.0),
        width: 300.0,
        height: 250.0,
        view_center: Vec2::new(5.0, 4.0),
        view_height: 250.0 / SCALE,
        id: 2,
        locked: false,
        frozen_layers: frozen.iter().map(|s| s.to_string()).collect(),
        layer_colors: Vec::new(),
        clip: None,
        on: vp_on,
    };
    d.add(&paper(), Common::default(), EntityKind::Viewport(vp)).unwrap();
    d
}

fn paper() -> Space {
    Space::Paper("Layout1".into())
}

fn is_wall(p: &cadcraft_render::DPrim, d: &Drawing) -> bool {
    d.arch.walls.iter().any(|w| arch_handle(w.id) == p.handle)
}

fn wall_bounds(l: &DisplayList, d: &Drawing) -> Option<Bounds2> {
    // A plot drops the proxy handles (viewport content carries its layer index instead); the
    // drawing has no model entities, so all viewport content is arch linework.
    let mut b = Bounds2::EMPTY;
    let mut any = false;
    for p in l.prims.iter().filter(|p| is_wall(p, d) || p.handle == VIEWPORT_CONTENT) {
        for q in l.points(p) {
            b.add(*q);
            any = true;
        }
    }
    any.then_some(b)
}

fn assert_scaled_inside(l: &DisplayList, d: &Drawing) {
    let b = wall_bounds(l, d).expect("wall prims in the viewport");
    let rect = Bounds2::new(Vec2::new(0.0, -5.0), Vec2::new(300.0, 245.0)).expand(1e-6);
    assert!(rect.contains(b.min) && rect.contains(b.max), "{b:?} outside the viewport");
    assert!((b.width() - 10.2 * SCALE).abs() < 1e-6, "width {}", b.width());
    assert!((b.height() - 8.2 * SCALE).abs() < 1e-6, "height {}", b.height());
    // Centred on the viewport centre: model (5,4) -> paper (150,120).
    assert!(b.center().dist(Vec2::new(150.0, 120.0)) < 1e-6);
}

#[test]
fn arch_walls_show_through_a_viewport_at_its_scale() {
    let d = house(true, &[]);
    let l = build(&d, &paper(), &Options::default());
    assert_scaled_inside(&l, &d);
}

#[test]
fn a_layer_frozen_in_the_viewport_hides_arch_linework() {
    let d = house(true, &["A-WALL"]);
    let l = build(&d, &paper(), &Options::default());
    assert!(wall_bounds(&l, &d).is_none(), "frozen A-WALL still drawn");
    // Model-space linework is unaffected by the viewport's freeze: the drawing itself is not.
    assert!(d.layer("A-WALL").is_none_or(|x| !x.frozen));
}

#[test]
fn plotting_a_layout_includes_arch_linework() {
    let d = house(true, &[]);
    let l = build_plot(&d, &paper(), &Options::default());
    assert_scaled_inside(&l, &d);
}

#[test]
fn a_viewport_that_is_off_shows_no_arch_linework() {
    let d = house(false, &[]);
    let l = build(&d, &paper(), &Options::default());
    assert!(wall_bounds(&l, &d).is_none());
    let l = build_plot(&d, &paper(), &Options::default());
    assert!(wall_bounds(&l, &d).is_none());
}
