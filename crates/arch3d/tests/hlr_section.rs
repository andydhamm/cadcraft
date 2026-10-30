//! Phase 7 kernel: feature edges, hidden-line removal and section cuts on the CPU.
//!
//! Synthetic boxes pin the exact behaviour (which edges are features, which parts of an edge a box hides);
//! the phase-1 house pins the brief's numbers that do not need a roof: the front elevation's width, a window
//! on the back wall adding nothing to the front elevation, the door's closed rectangle, and one poché ring of
//! thickness × height per wall cut by a section at y = 4.

use cadcraft_arch3d::edges::{CREASE_DEG, EdgeKind, feature_edges};
use cadcraft_arch3d::hlr::{Line2, OrthoView, merge_collinear, visible_lines};
use cadcraft_arch3d::section::{Plane, section_cut, section_view};
use cadcraft_arch3d::{Mesh, Scene3d, build};
use cadcraft_doc::arch::{ArchKind, Level, Opening, OpeningKind, Room};
use cadcraft_doc::{ArchId, ArchModel, Wall};
use cadcraft_geom::Vec2;

const TOL: f64 = 1e-4;

fn box_mesh(id: u32, lo: [f64; 3], hi: [f64; 3]) -> Mesh {
    let mut m = Mesh::new("test", ArchId(id));
    let ring = [Vec2::new(lo[0], lo[1]), Vec2::new(hi[0], lo[1]), Vec2::new(hi[0], hi[1]), Vec2::new(lo[0], hi[1])];
    m.push_prism(&ring, lo[2], hi[2]);
    m
}

fn scene(meshes: Vec<Mesh>) -> Scene3d {
    Scene3d { meshes, ..Default::default() }
}

/// The phase-1 house: centre lines (0,0) (10,0) (10,8) (0,8), 0.2 m thick, 2.4 m high. Returns the model and the
/// wall ids south, east, north, west.
fn house() -> (ArchModel, [ArchId; 4]) {
    let mut m = ArchModel::default();
    let w = |a: (f64, f64), b: (f64, f64)| Wall {
        start: Vec2::new(a.0, a.1),
        end: Vec2::new(b.0, b.1),
        thickness: 0.2,
        height: 2.4,
        ..Default::default()
    };
    let s = m.push_wall(w((0.0, 0.0), (10.0, 0.0)));
    let e = m.push_wall(w((10.0, 0.0), (10.0, 8.0)));
    let n = m.push_wall(w((10.0, 8.0), (0.0, 8.0)));
    let wst = m.push_wall(w((0.0, 8.0), (0.0, 0.0)));
    (m, [s, e, n, wst])
}

/// Looking +y at the south (front) face: u = x, v = z.
fn front() -> OrthoView {
    OrthoView::elevation([0.0, 1.0]).expect("a horizontal elevation view exists")
}

fn y4() -> Plane {
    Plane::new([0.0, 4.0, 0.0], [0.0, 1.0, 0.0]).expect("a plane with a non-zero normal exists")
}

fn extent(lines: &[Line2]) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::MAX; 2];
    let mut hi = [f64::MIN; 2];
    for l in lines {
        for p in [l.a, l.b] {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
    }
    (lo, hi)
}

/// Whether the union of the lines covers the segment a–b (collinear lines only, within TOL).
fn covers(lines: &[Line2], a: [f64; 2], b: [f64; 2]) -> bool {
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let d = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for l in lines {
        let off = |p: [f64; 2]| (p[0] - a[0]) * -d[1] + (p[1] - a[1]) * d[0];
        if off(l.a).abs() > TOL || off(l.b).abs() > TOL {
            continue;
        }
        let s = |p: [f64; 2]| (p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1];
        let (s0, s1) = (s(l.a).min(s(l.b)), s(l.a).max(s(l.b)));
        spans.push((s0, s1));
    }
    spans.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut reach = 0.0;
    for (s0, s1) in spans {
        if s0 > reach + TOL {
            break;
        }
        reach = f64::max(reach, s1);
    }
    reach >= len - TOL
}

#[test]
fn a_box_has_twelve_crease_edges_and_no_diagonals() {
    let e = feature_edges(&box_mesh(1, [0.0; 3], [1.0, 2.0, 3.0]), CREASE_DEG);
    assert_eq!(e.len(), 12, "{e:?}");
    assert!(e.iter().all(|x| x.kind == EdgeKind::Crease));
    for x in &e {
        let axes = (0..3).filter(|&i| (x.a[i] - x.b[i]).abs() > 1e-9).count();
        assert_eq!(axes, 1, "a diagonal came out as a feature edge: {x:?}");
    }
}

#[test]
fn an_open_quad_shows_its_four_boundary_edges() {
    let m = Mesh {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 0.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::new("test", ArchId(1))
    };
    let e = feature_edges(&m, CREASE_DEG);
    assert_eq!(e.len(), 4);
    assert!(e.iter().all(|x| x.kind == EdgeKind::Boundary));
}

#[test]
fn faces_folded_less_than_the_crease_angle_hide_their_edge() {
    let fold = |deg: f64| {
        let r = deg.to_radians();
        let tip = [1.0 + r.cos() as f32, 0.5, r.sin() as f32];
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], tip],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::new("test", ArchId(1))
        }
    };
    let creases = |deg| feature_edges(&fold(deg), CREASE_DEG).iter().filter(|x| x.kind == EdgeKind::Crease).count();
    assert_eq!(creases(10.0), 0);
    assert_eq!(creases(30.0), 1);
}

#[test]
fn a_box_seen_head_on_is_one_rectangle() {
    let v = visible_lines(&scene(vec![box_mesh(1, [0.0; 3], [2.0, 1.0, 3.0])]), &front());
    assert_eq!(v.len(), 4, "{v:?}");
    let (lo, hi) = extent(&v);
    assert!((lo[0] - 0.0).abs() < TOL && (hi[0] - 2.0).abs() < TOL && (lo[1]).abs() < TOL && (hi[1] - 3.0).abs() < TOL);
}

#[test]
fn a_box_in_front_hides_exactly_the_covered_part_of_one_behind() {
    // Front box x 0..2, z 0..1 at y 0..1; back box x 1..3, z 0..2 at y 2..3; looking +y.
    let s = scene(vec![box_mesh(1, [0.0, 0.0, 0.0], [2.0, 1.0, 1.0]), box_mesh(2, [1.0, 2.0, 0.0], [3.0, 3.0, 2.0])]);
    let v = visible_lines(&s, &front());
    let back: Vec<Line2> = v.iter().filter(|l| l.object == ArchId(2)).copied().collect();
    // The back box's bottom edge shows only from x 2 to 3, its left edge only from z 1 to 2.
    assert!(covers(&back, [2.0, 0.0], [3.0, 0.0]));
    assert!(
        !covers(&back, [1.0, 0.0], [2.0, 0.0])
            && !back.iter().any(|l| l.a[0] < 2.0 - TOL && l.b[0] < 2.0 - TOL && l.a[1].abs() < TOL && l.b[1].abs() < TOL)
    );
    assert!(covers(&back, [1.0, 1.0], [1.0, 2.0]));
    assert!(!back.iter().any(|l| (l.a[0] - 1.0).abs() < TOL && (l.b[0] - 1.0).abs() < TOL && l.a[1].min(l.b[1]) < 1.0 - TOL));
    // The front box is untouched.
    assert!(covers(&v, [0.0, 1.0], [2.0, 1.0]) && covers(&v, [2.0, 0.0], [2.0, 1.0]));
}

#[test]
fn hostile_meshes_and_views_never_panic() {
    let nan = Mesh {
        positions: vec![[f32::NAN, 0.0, 0.0], [1.0, f32::INFINITY, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 0.0]; 4],
        indices: vec![0, 1, 2, 2, 3, 99, 0, 0, 0, 1],
        ..Mesh::new("test", ArchId(1))
    };
    let s = scene(vec![nan, Mesh::new("empty", ArchId(2))]);
    let _ = visible_lines(&s, &front());
    let _ = feature_edges(&s.meshes[0], CREASE_DEG);
    assert!(OrthoView::new([0.0; 3], [0.0, 0.0, 1.0]).is_none());
    assert!(OrthoView::new([0.0, 0.0, 1.0], [0.0, 0.0, 2.0]).is_none());
    assert!(OrthoView::new([f64::NAN, 1.0, 0.0], [0.0, 0.0, 1.0]).is_none());
    assert!(Plane::new([0.0; 3], [0.0; 3]).is_none());
    if let Some(p) = Plane::new([0.0, 0.5, 0.0], [0.0, 1.0, 0.0]) {
        let _ = section_cut(&s, &p, &front());
    }
}

#[test]
fn merge_collinear_joins_one_objects_overlapping_runs_and_keeps_the_rest() {
    let l = |a: [f64; 2], b: [f64; 2], id| Line2::new(a, b, ArchKind::Wall, ArchId(id));
    let m = merge_collinear(vec![
        l([0.0, 0.0], [1.0, 0.0], 1),
        l([2.0, 0.0], [0.5, 0.0], 1),
        l([3.0, 0.0], [4.0, 0.0], 1),
        l([0.0, 1.0], [1.0, 1.0], 1),
        l([0.0, 0.0], [1.0, 0.0], 2),
    ]);
    // Object 1's two overlapping runs become one; its separate run and its parallel line stay; object 2 keeps its own.
    assert_eq!(m.len(), 4, "{m:?}");
    let one: Vec<Line2> = m.iter().filter(|x| x.object == ArchId(1)).copied().collect();
    assert!(covers(&one, [0.0, 0.0], [2.0, 0.0]));
    assert!(!covers(&one, [0.0, 0.0], [3.0, 0.0]));
    let again = merge_collinear(m.clone());
    assert_eq!(again.len(), m.len(), "merging is idempotent");
    for (x, y) in again.iter().zip(&m) {
        assert!(x.object == y.object && (0..2).all(|i| (x.a[i] - y.a[i]).abs() < 1e-9 && (x.b[i] - y.b[i]).abs() < 1e-9), "{x:?} {y:?}");
    }
}

#[test]
fn merge_collinear_never_joins_a_wall_line_with_an_opening_line_of_the_same_number() {
    let wall = Line2::new([0.0, 0.0], [2.0, 0.0], ArchKind::Wall, ArchId(0));
    let opening = Line2::new([1.0, 0.0], [3.0, 0.0], ArchKind::Opening, ArchId(0));
    let m = merge_collinear(vec![wall, opening]);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(m.iter().any(|l| l.kind == ArchKind::Wall && covers(&[*l], [0.0, 0.0], [2.0, 0.0])));
    assert!(m.iter().any(|l| l.kind == ArchKind::Opening && covers(&[*l], [1.0, 0.0], [3.0, 0.0])));
}

#[test]
fn elevation_lines_carry_the_kind_of_their_object() {
    let (mut m, [s, ..]) = house();
    m.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let v = visible_lines(&build(&m), &front());
    assert!(v.iter().any(|l| l.kind == ArchKind::Opening && l.object == ArchId(0)), "{v:?}");
    assert!(v.iter().any(|l| l.kind == ArchKind::Wall && l.object == ArchId(0)), "{v:?}");
}

#[test]
fn elevation_draws_nothing_of_a_door_above_its_rough_top() {
    let (mut m, [s, ..]) = house();
    m.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let v = visible_lines(&build(&m), &front());
    let opening: Vec<&Line2> = v.iter().filter(|l| l.kind == ArchKind::Opening).collect();
    assert!(!opening.is_empty(), "{v:?}");
    for l in opening {
        assert!(l.a[1] <= 2.22 + TOL && l.b[1] <= 2.22 + TOL, "{l:?}");
    }
}

#[test]
fn a_window_header_is_hidden_in_elevation() {
    let (mut m, [s, ..]) = house();
    let mut o = Opening::new(s, 4.0, 0.9, 1.2, OpeningKind::Window);
    o.sill = 0.9;
    m.push_opening(o);
    let v = visible_lines(&build(&m), &front());
    let opening: Vec<&Line2> = v.iter().filter(|l| l.kind == ArchKind::Opening).collect();
    assert!(!opening.is_empty(), "{v:?}");
    for l in opening {
        assert!(l.a[1] <= 2.22 + TOL && l.b[1] <= 2.22 + TOL, "window header shows above rough top: {l:?}");
    }
}

#[test]
fn a_garage_door_header_is_hidden_in_elevation() {
    let (mut m, [s, ..]) = house();
    m.push_opening(Opening::new(s, 5.0, 3.0, 2.2, OpeningKind::GarageDoor));
    let v = visible_lines(&build(&m), &front());
    let opening: Vec<&Line2> = v.iter().filter(|l| l.kind == ArchKind::Opening).collect();
    assert!(!opening.is_empty(), "{v:?}");
    for l in opening {
        assert!(l.a[1] <= 2.32 + TOL && l.b[1] <= 2.32 + TOL, "garage door header shows above rough top: {l:?}");
    }
}

#[test]
fn section_rings_carry_the_kind_of_their_object() {
    let (mut m, _) = house();
    m.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    m.rooms.push(Room {
        id: ArchId(0),
        level: 0,
        polygon: vec![Vec2::new(0.1, 0.1), Vec2::new(9.9, 0.1), Vec2::new(9.9, 7.9), Vec2::new(0.1, 7.9)],
        ..Default::default()
    });
    let cut = section_cut(&build(&m), &y4(), &front());
    let walls = cut.rings.iter().filter(|r| r.kind == ArchKind::Wall).count();
    let rooms = cut.rings.iter().filter(|r| r.kind == ArchKind::Room).count();
    assert_eq!(walls, 2, "{:?}", cut.rings);
    assert!(rooms >= 1, "{:?}", cut.rings);
}

#[test]
fn the_front_elevation_of_the_house_is_its_outer_width() {
    let (m, _) = house();
    let v = visible_lines(&build(&m), &front());
    let (lo, hi) = extent(&v);
    assert!((hi[0] - lo[0] - 10.2).abs() < TOL, "{lo:?} {hi:?}");
    assert!((hi[1] - lo[1] - 2.4).abs() < TOL, "{lo:?} {hi:?}");
    // No triangulation diagonal of the front face shows: every line is horizontal or vertical.
    assert!(v.iter().all(|l| (l.a[0] - l.b[0]).abs() < TOL || (l.a[1] - l.b[1]).abs() < TOL), "{v:?}");
}

#[test]
fn a_window_on_the_back_wall_adds_nothing_to_the_front_elevation_and_one_on_the_front_does() {
    let (m, [s, _, n, _]) = house();
    let bare = visible_lines(&build(&m), &front());
    let mut back = m.clone();
    back.push_opening(Opening::new(n, 4.0, 0.9, 1.2, OpeningKind::Window));
    assert_eq!(visible_lines(&build(&back), &front()), bare);
    let mut fr = m.clone();
    fr.push_opening(Opening::new(s, 4.0, 0.9, 1.2, OpeningKind::Window));
    assert!(visible_lines(&build(&fr), &front()).len() > bare.len());
}

#[test]
fn the_front_door_shows_as_a_closed_rectangle_and_moves_with_its_offset() {
    let (m, [s, ..]) = house();
    let rect = |offset: f64| {
        let mut d = m.clone();
        d.push_opening(Opening::new(s, offset, 0.9, 2.1, OpeningKind::Door));
        let v = visible_lines(&build(&d), &front());
        // The two jambs: vertical lines 2.1 m tall, 0.9 m apart, joined by the head at 2.1 and the sill at 0.
        let jambs: Vec<f64> =
            v.iter().filter(|l| (l.a[0] - l.b[0]).abs() < TOL && ((l.a[1] - l.b[1]).abs() - 2.1).abs() < TOL).map(|l| l.a[0]).collect();
        let left = jambs.iter().copied().find(|&x| jambs.iter().any(|&y| (y - x - 0.9).abs() < TOL));
        let x = left.unwrap_or(f64::NAN);
        assert!(x.is_finite(), "no pair of 2.1 m jambs 0.9 m apart: {v:?}");
        assert!(covers(&v, [x, 2.1], [x + 0.9, 2.1]), "no head");
        assert!(covers(&v, [x, 0.0], [x + 0.9, 0.0]), "no sill");
        x
    };
    assert!((rect(4.0) - rect(3.0) - 1.0).abs() < TOL);
}

#[test]
fn a_section_at_y4_gives_one_ring_per_cut_wall_of_thickness_times_height() {
    let (m, [_, e, _, w]) = house();
    let plane = y4();
    let cut = section_cut(&build(&m), &plane, &front());
    assert_eq!(cut.rings.len(), 2, "{:?}", cut.rings);
    let mut objs: Vec<ArchId> = cut.rings.iter().map(|r| r.object).collect();
    objs.sort();
    let mut want = vec![e, w];
    want.sort();
    assert_eq!(objs, want);
    for r in &cut.rings {
        assert!((r.signed_area() - 0.2 * 2.4).abs() < 1e-5, "{r:?}");
    }
}

#[test]
fn a_section_view_shows_what_is_beyond_the_cut_and_not_what_is_behind_it() {
    let (m, [s, ..]) = house();
    let mut d = m.clone();
    d.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let sv = section_view(&build(&d), &y4(), [0.0, 0.0, 1.0]).expect("a vertical section view with up = +z exists");
    // The door is in the south wall, behind the cut: none of its 2.1 m jambs shows.
    assert!(!sv.beyond.iter().any(|l| (l.a[0] - l.b[0]).abs() < TOL && ((l.a[1] - l.b[1]).abs() - 2.1).abs() < TOL), "{:?}", sv.beyond);
    // The north wall's inner face is beyond it, between the two cut walls.
    assert!(covers(&sv.beyond, [0.1, 0.0], [9.9, 0.0]) && covers(&sv.beyond, [0.1, 2.4], [9.9, 2.4]), "{:?}", sv.beyond);
    // Nothing beyond shows inside the poché: no line strictly inside a cut wall's 0.2 m band.
    for r in &sv.cut.rings {
        let (lo, hi) = r
            .points
            .iter()
            .fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
        let inside = |p: [f64; 2]| p[0] > lo[0] + TOL && p[0] < hi[0] - TOL && p[1] > lo[1] + TOL && p[1] < hi[1] - TOL;
        assert!(!sv.beyond.iter().any(|l| inside([(l.a[0] + l.b[0]) / 2.0, (l.a[1] + l.b[1]) / 2.0])), "a line inside the poché of {:?}", r.object);
    }
}

#[test]
fn a_smooth_column_shows_its_silhouette_and_no_facet_edges() {
    let ring: Vec<Vec2> = (0..36)
        .map(|i| {
            let a = f64::from(i) * 10f64.to_radians();
            Vec2::new(a.cos(), a.sin())
        })
        .collect();
    let mut m = Mesh::new("test", ArchId(1));
    m.push_prism(&ring, 0.0, 2.0);
    let v = visible_lines(&scene(vec![m]), &front());
    // The facets meet at 10°, under the crease angle: only the rims and the two silhouettes at x = ±1 show.
    assert!(covers(&v, [-1.0, 0.0], [-1.0, 2.0]) && covers(&v, [1.0, 0.0], [1.0, 2.0]), "{v:?}");
    assert!(!v.iter().any(|l| (l.a[0] - l.b[0]).abs() < TOL && l.a[0].abs() < 0.99), "a facet edge shows: {v:?}");
}

/// The window's rough opening is a hole in the wall profile, so its jambs show through the section.
#[test]
fn a_window_beyond_a_section_shows_through_its_wall() {
    let (m, [_, _, n, _]) = house();
    let mut d = m.clone();
    let mut o = Opening::new(n, 6.0, 0.9, 1.2, OpeningKind::Window);
    o.sill = 0.9;
    d.push_opening(o);
    let sv = section_view(&build(&d), &y4(), [0.0, 0.0, 1.0]).expect("a vertical section view with up = +z exists");
    assert!(sv.beyond.iter().any(|l| (l.a[0] - l.b[0]).abs() < TOL && ((l.a[1] - l.b[1]).abs() - 1.2).abs() < TOL), "{:?}", sv.beyond);
}

#[test]
fn merge_collinear_three_kinds_same_object_all_overlap() {
    let wall = Line2::new([0.0, 0.0], [3.0, 0.0], ArchKind::Wall, ArchId(0));
    let opening = Line2::new([0.5, 0.0], [2.5, 0.0], ArchKind::Opening, ArchId(0));
    let room = Line2::new([1.0, 0.0], [2.0, 0.0], ArchKind::Room, ArchId(0));
    let m = merge_collinear(vec![wall, opening, room]);
    assert_eq!(m.len(), 3, "{m:?}");
    let kinds: Vec<ArchKind> = m.iter().map(|l| l.kind).collect();
    assert!(kinds.contains(&ArchKind::Wall));
    assert!(kinds.contains(&ArchKind::Opening));
    assert!(kinds.contains(&ArchKind::Room));
}

#[test]
fn section_view_beyond_lines_carry_kinds() {
    let (m, [s, ..]) = house();
    let mut d = m.clone();
    d.push_opening(Opening::new(s, 3.0, 0.9, 2.1, OpeningKind::Door));
    let sv = section_view(&build(&d), &y4(), [0.0, 0.0, 1.0]).expect("a vertical section view with up = +z exists");
    let wall_lines: Vec<&Line2> = sv.beyond.iter().filter(|l| l.kind == ArchKind::Wall).collect();
    assert!(!wall_lines.is_empty(), "no wall lines in beyond: {:?}", sv.beyond);
}

#[test]
fn merge_collinear_empty_and_single() {
    assert!(merge_collinear(vec![]).is_empty());
    let l = Line2::new([0.0, 0.0], [1.0, 0.0], ArchKind::Wall, ArchId(0));
    let m = merge_collinear(vec![l]);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].kind, ArchKind::Wall);
    assert_eq!(m[0].object, ArchId(0));
}

#[test]
fn section_rings_distinguish_wall_and_room_same_id() {
    let (mut m, [_, e, _, w]) = house();
    m.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    m.rooms.push(Room {
        id: ArchId(0),
        level: 0,
        polygon: vec![Vec2::new(0.1, 0.1), Vec2::new(9.9, 0.1), Vec2::new(9.9, 7.9), Vec2::new(0.1, 7.9)],
        ..Default::default()
    });
    let cut = section_cut(&build(&m), &y4(), &front());
    // The section at y=4 cuts the east (ArchId(1)) and west (ArchId(3)) walls, not the south (ArchId(0)).
    let wall_e = cut.rings.iter().filter(|r| r.kind == ArchKind::Wall && r.object == e).count();
    let wall_w = cut.rings.iter().filter(|r| r.kind == ArchKind::Wall && r.object == w).count();
    let room0 = cut.rings.iter().filter(|r| r.kind == ArchKind::Room && r.object == ArchId(0)).count();
    assert!(wall_e >= 1, "no wall ring for east wall: {:?}", cut.rings);
    assert!(wall_w >= 1, "no wall ring for west wall: {:?}", cut.rings);
    assert!(room0 >= 1, "no room ring for ArchId(0): {:?}", cut.rings);
}
