//! Linked views draw from the geometry stored on `ArchView.generated`: linework on the view's
//! layer, filled poché on `A-SECT-MCUT`, dimensions on `A-ANNO-DIMS`, all tagged with the view's
//! proxy handle and following layer state.

use cadcraft_color::Color;
use cadcraft_doc::Drawing;
use cadcraft_doc::arch::{ArchId, ArchKind, ArchView, ViewDim, ViewGeometry, ViewLine, ViewRing, view_handle};
use cadcraft_geom::Vec2;
use cadcraft_render::{DPrim, DisplayList, Kind, Options, build_arch};

/// A drawing with one view inserted at (10, 20): two lines, one 1 m square ring, one width dim.
fn drawing(generated: bool) -> (Drawing, ArchId) {
    let mut d = Drawing::new_metric();
    for (name, idx) in [("A-ELEV", 1), ("A-SECT-MCUT", 3), ("A-ANNO-DIMS", 5)] {
        if let Some(l) = d.layer_mut(name) {
            l.color = Color::Index(idx);
        }
    }
    let line = |a: [f64; 2], b: [f64; 2]| ViewLine { a, b, object: ArchId(1), ..ViewLine::default() };
    let geometry = ViewGeometry {
        extent: [0.0, 0.0, 4.0, 3.0],
        lines: vec![line([0.0, 0.0], [4.0, 0.0]), line([4.0, 0.0], [4.0, 3.0])],
        rings: vec![ViewRing {
            kind: ArchKind::Wall,
            object: ArchId(1),
            points: vec![[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]],
            ..ViewRing::default()
        }],
        dims: vec![ViewDim { a: [0.0, 0.0], b: [4.0, 0.0], at: [0.0, -1.0], value: 4.0, text: "4000".into(), ..ViewDim::default() }],
        ..ViewGeometry::default()
    };
    let view = ArchView { name: "South".into(), insert: [10.0, 20.0], scale: 50.0, generated: generated.then_some(geometry), ..ArchView::default() };
    let id = d.arch.push_view(view).expect("view is valid");
    (d, id)
}

fn color_of(d: &Drawing, layer: &str) -> cadcraft_color::Rgb {
    let l = d.layer(layer).expect("layer exists");
    // A prim's colour is the layer's ByLayer ink; read it back through a one-wall-free probe.
    match l.color {
        Color::Index(i) => cadcraft_color::aci_rgb(i),
        _ => cadcraft_color::aci_rgb(7),
    }
}

fn polyline_pts<'a>(list: &'a DisplayList, p: &DPrim) -> &'a [Vec2] {
    &list.verts[p.start as usize..(p.start + p.len) as usize]
}

fn build(d: &Drawing, plotting: bool) -> DisplayList {
    build_arch(d, &Options::default(), plotting)
}

#[test]
fn a_view_draws_its_lines_at_its_insertion_point() {
    let (d, id) = drawing(true);
    let list = build(&d, false);
    let h = view_handle(id);
    let ink = color_of(&d, "A-ELEV");
    let lines: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h && p.kind == Kind::Polyline && p.color == ink && p.len == 2).collect();
    assert_eq!(lines.len(), 2, "two A-ELEV lines");
    let first = polyline_pts(&list, lines[0]);
    assert_eq!(first, [Vec2::new(10.0, 20.0), Vec2::new(14.0, 20.0)]);
    let second = polyline_pts(&list, lines[1]);
    assert_eq!(second, [Vec2::new(14.0, 20.0), Vec2::new(14.0, 23.0)]);
}

#[test]
fn section_poche_is_filled_on_a_sect_mcut() {
    let (d, id) = drawing(true);
    let list = build(&d, false);
    let h = view_handle(id);
    let ink = color_of(&d, "A-SECT-MCUT");
    let fills: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h && p.kind == Kind::Tris).collect();
    assert_eq!(fills.len(), 1, "one filled ring");
    assert_eq!(fills[0].color, ink);
    assert_eq!(fills[0].len, 6, "a square is two triangles");
    let tris = &list.tris[fills[0].start as usize..(fills[0].start + fills[0].len) as usize];
    assert!(tris.iter().all(|p| p.x >= 11.0 - 1e-9 && p.x <= 12.0 + 1e-9 && p.y >= 21.0 - 1e-9 && p.y <= 22.0 + 1e-9));
    let outline = list.prims.iter().any(|p| p.handle == h && p.kind == Kind::Polyline && p.color == ink && p.len == 5);
    assert!(outline, "the ring also gets a closed outline");
}

#[test]
fn view_dimensions_draw_ticks_lines_and_text() {
    let (d, id) = drawing(true);
    let list = build(&d, false);
    let h = view_handle(id);
    let ink = color_of(&d, "A-ANNO-DIMS");
    let dims: Vec<&DPrim> = list.prims.iter().filter(|p| p.handle == h && p.kind == Kind::Polyline && p.color == ink).collect();
    let k = 50.0 / 1000.0;
    // Two extension lines (a -> dimension line + overshoot), the dimension line and two ticks.
    let two_pt: Vec<&&DPrim> = dims.iter().filter(|p| p.len == 2).collect();
    assert!(two_pt.len() >= 5, "extension lines, dimension line and ticks: {}", two_pt.len());
    // Dimension line at y = 20 - 1 between x = 10 and 14.
    assert!(two_pt.iter().any(|p| {
        let v = polyline_pts(&list, p);
        (v[0].y + 1.0 - 20.0).abs() < 1e-9 && (v[1].y + 1.0 - 20.0).abs() < 1e-9 && (v[0].dist(v[1]) - 4.0).abs() < 1e-9
    }));
    // Extension line from (10, 20) overshooting the dimension line (y = 19) by 1.5 mm at scale.
    assert!(two_pt.iter().any(|p| {
        let v = polyline_pts(&list, p);
        v[0].dist(Vec2::new(10.0, 20.0)) < 1e-9 && (v[1].y - (19.0 - 1.5 * k)).abs() < 1e-9
    }));
    // A 45-degree tick of length 3 mm at scale.
    assert!(two_pt.iter().any(|p| {
        let v = polyline_pts(&list, p);
        let d = v[1] - v[0];
        (d.len() - 3.0 * k).abs() < 1e-9 && (d.x.abs() - d.y.abs()).abs() < 1e-9 && d.x.abs() > 1e-9
    }));
    // Text sits above the dimension line midpoint (12, 19).
    let text: Vec<&&DPrim> = dims.iter().filter(|p| p.len > 2 || !two_pt.iter().any(|q| std::ptr::eq(**q, **p))).collect();
    assert!(!text.is_empty(), "the dimension text draws strokes");
    assert!(text.iter().all(|p| polyline_pts(&list, p).iter().all(|q| q.y > 19.0 && (q.x - 12.0).abs() < 1.0)));
}

#[test]
fn a_frozen_view_layer_hides_the_view() {
    let (mut d, id) = drawing(true);
    if let Some(l) = d.layer_mut("A-ELEV") {
        l.frozen = true;
    }
    let list = build(&d, false);
    let h = view_handle(id);
    let ink = color_of(&d, "A-ELEV");
    assert!(!list.prims.iter().any(|p| p.handle == h && p.color == ink), "no A-ELEV prims");
    // Other view layers still draw.
    assert!(list.prims.iter().any(|p| p.handle == h && p.kind == Kind::Tris));
}

#[test]
fn an_ungenerated_view_draws_nothing() {
    let (d, _) = drawing(false);
    let list = build(&d, false);
    assert!(list.prims.is_empty());
}

#[test]
fn the_template_has_the_phase_7_and_railing_layers() {
    for d in [Drawing::new_imperial(), Drawing::new_metric()] {
        for name in ["A-RAIL", "A-ELEV", "A-SECT", "A-SECT-MCUT", "A-ANNO-DIMS"] {
            assert!(d.layer(name).is_some(), "{name} must be in the template");
        }
    }
}
