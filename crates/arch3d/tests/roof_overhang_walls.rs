use cadcraft_arch::roof::{build_roof, plane_apex_z};
use cadcraft_arch3d::Scene3d;
use cadcraft_arch3d::hlr::OrthoView;
use cadcraft_arch3d::section::{Plane, section_cut};
use cadcraft_doc::ArchModel;
use cadcraft_doc::arch::{Level, Roof, RoofDirective, Wall};
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

    // Gable ends on the south and north walls leave the east and west walls
    // as eave walls, whose tops must remain at 2.4 m.
    for wall in &mut model.walls {
        if (wall.start.y - wall.end.y).abs() < 1e-9 {
            wall.roof_directive = RoofDirective::Gable;
        }
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
    let id = find_wall_id(&model, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)).expect("west eave wall missing");
    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&id).expect("eave wall mesh should exist");
    let max_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
    assert!((max_z - 2.4).abs() < 1e-3, "eave wall should stay at 2.4, got {max_z}");
}

#[test]
fn a_gable_wall_meets_the_roof_underside_with_an_overhang() {
    let model = overhanging_gable_house();
    let wall = model.walls.iter().find(|w| w.start == Vec2::new(0.0, 0.0) && w.end == Vec2::new(10.0, 0.0)).expect("south gable wall missing");
    let wall_id = wall.id;

    // The ridge is 5.1 m in from the eave wall line: 2.4 + 5.1 * 0.5.
    let expected = 4.95;
    let poly = cadcraft_arch3d::walls::roof_top_polyline(&model, wall, 0.0, 2.4).expect("gable wall should have a roof-clipped top");
    let poly_top = poly.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    assert!((poly_top - expected).abs() < 1e-3, "profile should meet the underside at {expected}, got {poly_top}");

    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&wall_id).expect("gable wall mesh should exist");
    let max_z = mesh.positions.iter().map(|p| p[2] as f64).fold(f64::NEG_INFINITY, f64::max);
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

    let areas: Vec<f64> = cut.rings.iter().map(|ring| ring_area(&ring.points)).collect();
    let wall_areas: Vec<f64> = areas.iter().copied().filter(|area| (*area - 0.48).abs() < 1e-3).collect();
    assert_eq!(wall_areas.len(), 2, "section should contain two 0.48 m² wall rings; all ring areas: {areas:?}");
}
