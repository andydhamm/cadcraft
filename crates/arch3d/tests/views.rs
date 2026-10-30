//! View generation (phase 7): elevations and sections of the phase-1 gable house, with overall dimensions.

use cadcraft_arch::roof::build_roof;
use cadcraft_arch3d::build;
use cadcraft_arch3d::views::{generate, refresh};
use cadcraft_doc::arch::{ArchKind, ArchView, Level, Opening, OpeningKind, Roof, RoofDirective, Room, ViewGeometry, ViewKind, ViewLine, Wall};
use cadcraft_doc::{ArchId, ArchModel};
use cadcraft_geom::Vec2;

const TOL: f64 = 1e-4;

/// The phase-1 house with a room over the interior and a 6/12 gable roof (overhang 0.5, gables south and north).
/// Returns the model and the south and north wall ids.
fn house() -> (ArchModel, ArchId, ArchId) {
    let mut m = ArchModel::default();
    m.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let mut ids = Vec::new();
    for i in 0..4 {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        let mut w = Wall {
            start: Vec2::new(a.0, a.1),
            end: Vec2::new(b.0, b.1),
            thickness: 0.2,
            height: 2.4,
            exterior: true,
            base_level: 0,
            ..Default::default()
        };
        if i % 2 == 0 {
            w.roof_directive = RoofDirective::Gable;
        }
        ids.push(m.push_wall(w));
    }
    m.push_room(Room {
        name: "Hall".into(),
        polygon: vec![Vec2::new(0.1, 0.1), Vec2::new(9.9, 0.1), Vec2::new(9.9, 7.9), Vec2::new(0.1, 7.9)],
        ..Default::default()
    });
    let roof = Roof {
        id: ArchId(1),
        level: 0,
        pitch_rise: 6.0,
        pitch_run: 12.0,
        overhang: 0.5,
        thickness: 0.2,
        fascia: true,
        soffit: true,
        ..Default::default()
    };
    if let Ok(built) = build_roof(&m, 0, &roof) {
        m.roofs.push(built);
    }
    (m, ids[0], ids[2])
}

fn south() -> ArchView {
    ArchView { name: "South".into(), look: [0.0, 1.0], ..Default::default() }
}

fn section() -> ArchView {
    ArchView {
        name: "A".into(),
        kind: ViewKind::Section,
        look: [0.0, 1.0],
        cut: Some([[-1.0, 4.0], [11.0, 4.0]]),
        layer: "A-SECT".into(),
        ..Default::default()
    }
}

fn view_of(m: &ArchModel, v: &ArchView) -> ViewGeometry {
    generate(&build(m), v, 1)
}

fn kind_lines(g: &ViewGeometry, k: ArchKind) -> Vec<&ViewLine> {
    g.lines.iter().filter(|l| l.kind == k).collect()
}

#[test]
fn the_south_elevation_of_the_gable_house_is_11_2_m_wide() {
    let (m, ..) = house();
    let g = view_of(&m, &south());
    assert!(g.error.is_none(), "{:?}", g.error);
    assert!(((g.extent[2] - g.extent[0]) - 11.2).abs() < TOL, "{:?}", g.extent);
}

#[test]
fn the_door_is_a_closed_0_9_by_2_1_rectangle_in_the_south_elevation() {
    let (mut m, s, _) = house();
    m.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let g = view_of(&m, &south());
    let door = kind_lines(&g, ArchKind::Opening);
    assert!(!door.is_empty());
    let jambs: Vec<f64> =
        door.iter().filter(|l| (l.a[0] - l.b[0]).abs() < TOL && ((l.a[1] - l.b[1]).abs() - 2.1).abs() < TOL).map(|l| l.a[0]).collect();
    let x = jambs.iter().copied().find(|&x| jambs.iter().any(|&y| (y - x - 0.9).abs() < TOL));
    let x = x.unwrap_or(f64::NAN);
    assert!(x.is_finite(), "no pair of 2.1 m jambs 0.9 m apart: {door:?}");
    let covered = |a: [f64; 2], b: [f64; 2]| {
        let mut runs: Vec<(f64, f64)> = g
            .lines
            .iter()
            .filter(|l| (l.a[1] - a[1]).abs() < TOL && (l.b[1] - a[1]).abs() < TOL)
            .map(|l| (l.a[0].min(l.b[0]), l.a[0].max(l.b[0])))
            .collect();
        runs.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut at = a[0];
        for (lo, hi) in runs {
            if lo <= at + TOL {
                at = at.max(hi);
            }
        }
        at >= b[0] - TOL
    };
    assert!(covered([x, 2.1], [x + 0.9, 2.1]), "no head");
    assert!(covered([x, 0.0], [x + 0.9, 0.0]), "no sill");
}

#[test]
fn a_back_window_leaves_the_south_elevation_unchanged_and_a_front_one_adds_lines() {
    let (m, s, n) = house();
    let bare = view_of(&m, &south());
    let mut back = m.clone();
    back.push_opening(Opening::new(n, 4.0, 0.9, 1.2, OpeningKind::Window));
    assert_eq!(view_of(&back, &south()).lines, bare.lines);
    let mut front = m.clone();
    front.push_opening(Opening::new(s, 4.0, 0.9, 1.2, OpeningKind::Window));
    let g = view_of(&front, &south());
    assert!(g.lines.len() > bare.lines.len());
    assert!(!kind_lines(&g, ArchKind::Opening).is_empty());
}

#[test]
fn moving_a_window_1_m_moves_its_lines_1_m() {
    let (m, s, _) = house();
    let at = |offset: f64| {
        let mut d = m.clone();
        d.push_opening(Opening::new(s, offset, 0.9, 1.2, OpeningKind::Window));
        let g = view_of(&d, &south());
        kind_lines(&g, ArchKind::Opening).iter().map(|l| (l.a[0], l.a[1], l.b[0], l.b[1])).collect::<Vec<_>>()
    };
    let (a, b) = (at(3.0), at(4.0));
    assert!(!a.is_empty());
    assert_eq!(a.len(), b.len());
    for (p, q) in a.iter().zip(&b) {
        assert!((q.0 - p.0 - 1.0).abs() < TOL && (q.2 - p.2 - 1.0).abs() < TOL, "{p:?} {q:?}");
        assert!((q.1 - p.1).abs() < TOL && (q.3 - p.3).abs() < TOL);
    }
}

#[test]
fn a_section_at_y4_has_two_wall_rings_of_0_48_and_slab_and_roof_poche() {
    let (mut m, s, _) = house();
    m.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let g = view_of(&m, &section());
    assert!(g.error.is_none(), "{:?}", g.error);
    let walls: Vec<_> = g.rings.iter().filter(|r| r.kind == ArchKind::Wall).collect();
    assert_eq!(walls.len(), 2, "{walls:?}");
    for r in walls {
        assert!((r.area() - 0.48).abs() < 1e-3, "{}", r.area());
    }
    assert!(g.rings.iter().any(|r| r.kind == ArchKind::Room), "no slab");
    assert!(g.rings.iter().any(|r| r.kind == ArchKind::Roof), "no roof");
    assert!(kind_lines(&g, ArchKind::Opening).is_empty(), "the door is on the south wall, behind the viewer");
}

#[test]
fn overall_dimensions_measure_the_extent_in_millimetres() {
    let (m, ..) = house();
    let mut v = south();
    assert!(view_of(&m, &v).dims.is_empty());
    v.dimensions = true;
    let g = view_of(&m, &v);
    assert_eq!(g.dims.len(), 2);
    let [u0, v0, u1, v1] = g.extent;
    let w = &g.dims[0];
    assert_eq!(w.text, "11200");
    assert!((w.value - 11.2).abs() < TOL);
    assert_eq!((w.a, w.b), ([u0, v0], [u1, v0]));
    assert_eq!(w.at, [(u0 + u1) / 2.0, v0 - 0.75]);
    let h = &g.dims[1];
    assert!((h.value - (v1 - v0)).abs() < 1e-12);
    assert_eq!(h.text, format!("{:.0}", (v1 - v0) * 1000.0));
    assert_eq!((h.a, h.b), ([u0, v0], [u0, v1]));
    assert_eq!(h.at, [u0 - 0.75, (v0 + v1) / 2.0]);
}

#[test]
fn generation_is_deterministic() {
    let (mut m, s, _) = house();
    m.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let scene = build(&m);
    for v in [south(), section()] {
        assert_eq!(generate(&scene, &v, 1), generate(&scene, &v, 1));
    }
}

#[test]
fn scope_leaves_out_objects_outside_it() {
    let (mut m, s, _) = house();
    m.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let all = view_of(&m, &south());
    assert!(!kind_lines(&all, ArchKind::Opening).is_empty());
    let mut v = south();
    v.scope = Some([[-1.0, 5.0], [11.0, 9.0]]);
    let g = view_of(&m, &v);
    assert!(kind_lines(&g, ArchKind::Opening).is_empty());
    assert!(!g.lines.is_empty());
}

#[test]
fn refresh_regenerates_only_stale_views() {
    let (mut m, ..) = house();
    m.push_view(south()).expect("a valid view");
    m.push_view(section()).expect("a valid view");
    assert_eq!(refresh(&mut m, 5), 2);
    assert!(m.views.iter().all(|v| v.generated.as_ref().is_some_and(|g| g.revision == 5)));
    assert_eq!(refresh(&mut m, 5), 0);
    m.views[0].generated = None;
    assert_eq!(refresh(&mut m, 5), 1);
    assert_eq!(refresh(&mut m, 6), 2);
}

#[test]
fn an_empty_model_generates_an_empty_view() {
    let m = ArchModel::default();
    for mut v in [south(), section()] {
        v.dimensions = true;
        let g = view_of(&m, &v);
        assert!(g.lines.is_empty() && g.rings.is_empty() && g.dims.is_empty());
        assert_eq!(g.extent, [0.0; 4]);
        assert!(g.error.is_none());
    }
}

#[test]
fn section_depth_limits_the_beyond_lines() {
    let (m, ..) = house();
    let full = view_of(&m, &section());
    let mut v = section();
    v.depth = Some(0.0);
    let shallow = view_of(&m, &v);
    assert!(shallow.error.is_none(), "{:?}", shallow.error);
    // With zero depth no mesh beyond the plane survives, so the beyond lines vanish
    // while the cut rings remain.
    assert!(shallow.lines.len() <= full.lines.len());
    assert!(!shallow.rings.is_empty(), "the cut rings must survive a zero depth");
}

#[test]
fn section_fails_when_viewer_looks_along_the_cut() {
    let (m, ..) = house();
    let v = ArchView { name: "A".into(), kind: ViewKind::Section, look: [1.0, 0.0], cut: Some([[-1.0, 4.0], [11.0, 4.0]]), ..Default::default() };
    let g = view_of(&m, &v);
    assert!(g.error.is_some(), "viewer looking along the cut line must fail");
    assert!(g.lines.is_empty() && g.rings.is_empty());
}

#[test]
fn section_fails_on_a_zero_length_cut() {
    let (m, ..) = house();
    let v = ArchView { name: "A".into(), kind: ViewKind::Section, look: [0.0, 1.0], cut: Some([[4.0, 4.0], [4.0, 4.0]]), ..Default::default() };
    let g = view_of(&m, &v);
    assert!(g.error.is_some(), "a zero-length cut must fail");
}

#[test]
fn diagonal_elevation_works() {
    let (m, ..) = house();
    let v = ArchView { name: "SE".into(), look: [1.0, 1.0], ..Default::default() };
    let g = view_of(&m, &v);
    assert!(g.error.is_none(), "{:?}", g.error);
    assert!(!g.lines.is_empty(), "a diagonal elevation must produce lines");
}

#[test]
fn elevation_with_huge_coordinates() {
    let mut m = ArchModel::default();
    m.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    m.push_wall(Wall {
        start: Vec2::new(1e15, 0.0),
        end: Vec2::new(1e15 + 10.0, 0.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    let g = view_of(&m, &south());
    assert!(g.error.is_none(), "{:?}", g.error);
    assert!(!g.lines.is_empty(), "the distant wall must be visible");
    assert!(g.extent[0] > 1e14, "the extent must be near the wall position");
}

#[test]
fn scope_far_away_excludes_everything() {
    let (m, ..) = house();
    let mut v = south();
    v.scope = Some([[100.0, 100.0], [110.0, 110.0]]);
    let g = view_of(&m, &v);
    assert!(g.lines.is_empty(), "a scope far from the house must exclude all meshes");
    assert!(g.rings.is_empty());
}
