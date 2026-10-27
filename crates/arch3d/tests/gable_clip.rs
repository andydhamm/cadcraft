use std::collections::BTreeSet;

use cadcraft_arch3d::{ArchId, Scene3d};
use cadcraft_doc::ArchModel;
use cadcraft_doc::arch::{Level, Roof, RoofPlane, Wall};
use cadcraft_geom::Vec2;

/// Build the phase-1 gable house inline.
fn gable_house() -> ArchModel {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });

    let _ = model.push_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(10.0, 0.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    let _ = model.push_wall(Wall {
        start: Vec2::new(10.0, 0.0),
        end: Vec2::new(10.0, 8.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    let _ = model.push_wall(Wall {
        start: Vec2::new(10.0, 8.0),
        end: Vec2::new(0.0, 8.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    let _ = model.push_wall(Wall {
        start: Vec2::new(0.0, 8.0),
        end: Vec2::new(0.0, 0.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });

    let mut roof = Roof::default();
    roof.id = ArchId(100);
    roof.level = 0;
    roof.planes.push(RoofPlane {
        base_edge: [-0.1, -0.1, 10.1, -0.1],
        ring: vec![Vec2::new(-0.1, -0.1), Vec2::new(10.1, -0.1), Vec2::new(10.1, 4.0), Vec2::new(-0.1, 4.0)],
        slope: 0.5,
        eave_z: 2.4,
        thickness: 0.25,
        ..Default::default()
    });
    roof.planes.push(RoofPlane {
        base_edge: [-0.1, 8.1, 10.1, 8.1],
        ring: vec![Vec2::new(-0.1, 8.1), Vec2::new(-0.1, 4.0), Vec2::new(10.1, 4.0), Vec2::new(10.1, 8.1)],
        slope: 0.5,
        eave_z: 2.4,
        thickness: 0.25,
        ..Default::default()
    });
    model.roofs.push(roof);
    model
}

/// Find a wall id by matching its start and end points.
fn find_wall_id(model: &ArchModel, start: Vec2, end: Vec2) -> Option<ArchId> {
    model.walls.iter().find(|w| (w.start - start).len() < 1e-3 && (w.end - end).len() < 1e-3).map(|w| w.id)
}

#[test]
fn gable_wall_top_follows_the_roof_underside() {
    let model = gable_house();

    // Gable wall: (0,8) → (0,0) — perpendicular to the ridge.
    let gable_id = find_wall_id(&model, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)).expect("gable wall not found in model");

    let full = Scene3d::build(&model);

    let mesh = full.mesh_for(&gable_id);
    assert!(mesh.is_some(), "gable wall mesh must be Some");
    let mesh = mesh.unwrap();

    // Ridge is at y = 4.0, z = 2.4 + 0.5 * (4.0 - (-0.1)) = 2.4 + 2.05 = 4.45.
    let ridge_z = 4.45_f32;
    let ridge_y = 4.0_f32;

    let has_ridge_vertex = mesh.positions.iter().any(|p| (p[1] - ridge_y).abs() < 1e-3 && (p[2] - ridge_z).abs() < 1e-3);
    assert!(has_ridge_vertex, "gable mesh must have a vertex at the ridge (y≈4.0, z≈4.45)");

    let max_z = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
    assert!((max_z - ridge_z).abs() < 1e-3, "gable mesh max z must be ≈4.45, got {max_z}");

    assert!(mesh.positions.iter().all(|p| p[2] <= ridge_z + 1e-3), "no vertex may exceed the ridge height");

    // The incremental path must agree with the full build.
    let dirty = BTreeSet::from([gable_id]);
    let incremental = Scene3d::build_dirty(&model, &dirty, &full);
    let inc_mesh = incremental.mesh_for(&gable_id).expect("incremental gable mesh must be Some");

    assert_eq!(inc_mesh.positions, mesh.positions, "build_dirty must reproduce the same gable mesh positions as build");
}

#[test]
fn eave_wall_top_stays_at_its_height() {
    let model = gable_house();

    // Eave wall: (0,0) → (10,0) — its outer face is on the south roof plane's base edge.
    let eave_id = find_wall_id(&model, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)).expect("eave wall not found in model");

    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&eave_id).expect("eave wall mesh must be Some");

    let max_z = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
    assert!((max_z - 2.4_f32).abs() < 1e-3, "eave wall max z must be ≈2.4 (unchanged wall height), got {max_z}");
}
