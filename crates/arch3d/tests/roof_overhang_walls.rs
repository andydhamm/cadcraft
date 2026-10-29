use cadcraft_arch::roof::{build_roof, plane_apex_z};
use cadcraft_arch3d::Scene3d;
use cadcraft_arch3d::hlr::OrthoView;
use cadcraft_arch3d::section::{Plane, section_cut};
use cadcraft_doc::ArchModel;
use cadcraft_doc::arch::{Level, Roof, Wall};
use cadcraft_geom::Vec2;

fn overhanging_gable_house() -> ArchModel {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });

    for (start, end) in [
        (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
        (Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)),
        (Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)),
        (Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)),
    ] {
        let _ = model.push_wall(Wall { start, end, thickness: 0.2, height: 2.4, exterior: true, base_level: 0, ..Default::default() });
    }

    let roof = Roof {
        id: cadcraft_doc::ArchId(1),
        level: 0,
        pitch_rise: 6.0,
        pitch_run: 12.0,
        overhang: 0.5,
        thickness: 0.2,
        fascia: true,
        soffit: true,
        ..Default::default()
    };
    let built = build_roof(&model, 0, &roof).expect("phase-1 gable house roof should build with overhang");
    let mut model = model;
    model.roofs.push(built);
    model
}

fn find_wall_id(model: &ArchModel, start: Vec2, end: Vec2) -> Option<cadcraft_doc::ArchId> {
    model.walls.iter().find(|w| (w.start - start).len() < 1e-3 && (w.end - end).len() < 1e-3).map(|w| w.id)
}

fn ring_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    let n = points.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i][0] * points[j][1] - points[j][0] * points[i][1];
    }
    area.abs() / 2.0
}

#[test]
fn an_eave_wall_under_an_overhanging_roof_keeps_its_height() {
    let model = overhanging_gable_house();
    let id = find_wall_id(&model, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)).expect("south eave wall missing");
    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&id).expect("eave wall mesh should exist");
    let max_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
    assert!((max_z - 2.4).abs() < 1e-3, "eave wall should stay at 2.4, got {max_z}");
}

#[test]
fn a_gable_wall_meets_the_roof_underside_with_an_overhang() {
    let model = overhanging_gable_house();
    let wall = model.walls.iter().find(|w| w.start == Vec2::new(0.0, 8.0) && w.end == Vec2::new(0.0, 0.0)).unwrap();
    let wall_id = wall.id;
    let dx = wall.end.x - wall.start.x;
    let dy = wall.end.y - wall.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    let dir = Vec2::new(dx / len, dy / len);
    let ring_count = model.roofs[0].planes.iter().flat_map(|p| p.ring.iter()).count();
    let roof_center = model.roofs[0].planes.iter().flat_map(|p| p.ring.iter().copied()).fold(Vec2::ZERO, |a, b| a + b) / ring_count as f64;
    let v = roof_center - Vec2::new((wall.start.x + wall.end.x) / 2.0, (wall.start.y + wall.end.y) / 2.0);
    let out_len = (v.x * v.x + v.y * v.y).sqrt();
    let out = Vec2::new(v.x / out_len, v.y / out_len);
    println!("dir={:?} roof_center={:?} out={:?}", dir, roof_center, out);
    for u in [0.0, 2.0, 4.0, 6.0, 8.0] {
        let p = wall.start + dir * u + out * (wall.thickness / 2.0);
        let vals: Vec<_> = model.roofs[0]
            .planes
            .iter()
            .map(|plane| (plane.eave_z, plane.base_edge, cadcraft_arch3d::roof::underside_z(plane, plane.eave_z, p)))
            .collect();
        println!(
            "u={} p={:?} inside? {} vals={:?}",
            u,
            p,
            model.roofs[0].planes.iter().any(|plane| cadcraft_geom::point_in_polygon(&plane.ring, p)),
            vals
        );
    }
    let poly = cadcraft_arch3d::walls::roof_top_polyline(&model, wall, 0.0, 2.4);
    println!("roof_top_polyline={:?}", poly);
    let poly = poly.unwrap();
    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&wall_id).expect("gable wall mesh should exist");
    let max_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
    let expected = 4.2;
    assert!((max_z - expected).abs() < 1e-3, "gable wall should meet roof underside at {expected}, got {max_z}");
}

#[test]
fn the_3d_ridge_matches_plane_apex_z_with_an_overhang() {
    let model = overhanging_gable_house();
    let plane = &model.roofs[0].planes[0];
    let expected = plane_apex_z(plane);
    let ridge_z = plane.ring.iter().map(|p| cadcraft_arch3d::roof::underside_z(plane, plane.eave_z, *p)).fold(f64::NEG_INFINITY, f64::max);
    assert!((ridge_z - expected).abs() < 1e-6, "roof ridge underside should match plane_apex_z {expected}, got {ridge_z}");
}

#[test]
fn a_section_at_y4_of_the_overhanging_gable_house_cuts_wall_rings_of_0_48() {
    let model = overhanging_gable_house();
    let scene = Scene3d::build(&model);
    let plane = Plane::new([0.0, 4.0, 0.0], [0.0, 1.0, 0.0]).expect("section plane should be valid");
    let view = OrthoView::new([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]).expect("section view should be valid");
    let cut = section_cut(&scene, &plane, &view);

    assert_eq!(cut.rings.len(), 2, "an overhanging gable house section at y=4 should cut two wall rings");
    let areas: Vec<f64> = cut.rings.iter().map(|ring| ring_area(&ring.points)).collect();
    assert!(areas.iter().all(|area| (area - 0.48).abs() < 1e-3), "wall ring area should be 0.48, got {areas:?}");
}
