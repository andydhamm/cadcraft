use std::collections::BTreeSet;

use cadcraft_arch3d::{ArchId, Mesh, Scene3d};
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

/// Odd-even upward ray crossing count: a point is inside when the count is odd.
fn ray_crossings(mesh: &Mesh, x: f64, y: f64, z: f64) -> usize {
    let mut count = 0;
    for i in (0..mesh.indices.len()).step_by(3) {
        let (i0, i1, i2) = (mesh.indices[i] as usize, mesh.indices[i + 1] as usize, mesh.indices[i + 2] as usize);
        let a = mesh.positions[i0];
        let b = mesh.positions[i1];
        let c = mesh.positions[i2];
        let ax = a[0] as f64;
        let ay = a[1] as f64;
        let az = a[2] as f64;
        let bx = b[0] as f64;
        let by = b[1] as f64;
        let bz = b[2] as f64;
        let cx = c[0] as f64;
        let cy = c[1] as f64;
        let cz = c[2] as f64;
        // Cross product in xy-plane; zero => triangle is vertical (parallel to ray)
        let den = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if den.abs() < 1e-12 {
            continue;
        }
        // Ray direction (0,0,1) intersects the triangle plane at t above az,bz,cz
        let rx = x - ax;
        let ry = y - ay;
        let s = (rx * (cy - ay) - ry * (cx - ax)) / den;
        let t = (ry * (bx - ax) - rx * (by - ay)) / den;
        if s < -1e-6 || t < -1e-6 || s + t > 1.0 + 1e-6 {
            continue;
        }
        let hit_z = az + s * (cz - az) + t * (bz - az);
        if hit_z > z + 1e-9 {
            count += 1;
        }
    }
    count
}

#[test]
fn gable_wall_is_a_probeable_solid() {
    let model = gable_house();
    let scene = Scene3d::build(&model);

    // (0, 4, 3) is inside the west gable wall (wall from (0,8) to (0,0))
    // The ridge is at y=4, z=4.45. At y=4 the wall goes up to 4.45.
    let west_id = find_wall_id(&model, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)).expect("west gable wall not found");
    let west_mesh = scene.mesh_for(&west_id).expect("west gable mesh");
    assert!(ray_crossings(&west_mesh, 0.0, 4.0, 3.0) % 2 == 1, "probe (0,4,3) must find wall inside west gable");

    // (10, 4, 3) is inside the east gable wall (wall from (10,0) to (10,8))
    let east_id = find_wall_id(&model, Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)).expect("east gable wall not found");
    let east_mesh = scene.mesh_for(&east_id).expect("east gable mesh");
    assert!(ray_crossings(&east_mesh, 10.0, 4.0, 3.0) % 2 == 1, "probe (10,4,3) must find wall inside east gable");

    // (10, 0.5, 3.0) is above the eave at y=0.5 (wall height there is ~2.45)
    // so it should find nothing in any wall.
    for w in &model.walls {
        if let Some(wmesh) = scene.mesh_for(&w.id) {
            assert!(ray_crossings(&wmesh, 10.0, 0.5, 3.0) % 2 == 0, "probe (10, 0.5, 3.0) must find no wall above the eave");
        }
    }
}

#[test]
fn gable_build_full_matches_incremental() {
    let model = gable_house();
    let west_id = find_wall_id(&model, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)).expect("west gable wall not found");
    let east_id = find_wall_id(&model, Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0)).expect("east gable wall not found");

    let full = Scene3d::build(&model);
    let dirty = BTreeSet::from([west_id, east_id]);
    let incremental = Scene3d::build_dirty(&model, &dirty, &full);

    let full_west = full.mesh_for(&west_id).expect("full west mesh");
    let inc_west = incremental.mesh_for(&west_id).expect("incremental west mesh");
    assert_eq!(full_west.positions, inc_west.positions, "full and incremental west gable positions must be identical");
    assert_eq!(full_west.indices, inc_west.indices, "full and incremental west gable indices must be identical");

    let full_east = full.mesh_for(&east_id).expect("full east mesh");
    let inc_east = incremental.mesh_for(&east_id).expect("incremental east mesh");
    assert_eq!(full_east.positions, inc_east.positions, "full and incremental east gable positions must be identical");
    assert_eq!(full_east.indices, inc_east.indices, "full and incremental east gable indices must be identical");
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

#[test]
fn wall_taller_than_ridge_is_clipped_at_ridge() {
    let mut model = gable_house();
    if let Some(wall) = model.walls.iter_mut().find(|w| (w.start - Vec2::new(0.0, 8.0)).len() < 1e-3 && (w.end - Vec2::new(0.0, 0.0)).len() < 1e-3) {
        wall.height = 5.0;
    }
    let gable_id = find_wall_id(&model, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)).expect("gable wall not found");
    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&gable_id).expect("gable wall mesh must be Some");
    let max_z = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
    assert!((max_z - 4.45_f32).abs() < 1e-3, "tall gable wall must be clipped to ridge height 4.45, got {max_z}");
}

#[test]
fn flat_roof_leaves_wall_height_unchanged() {
    let mut model = gable_house();
    for plane in model.roofs.iter_mut().flat_map(|r| r.planes.iter_mut()) {
        plane.slope = 0.0;
    }
    let gable_id = find_wall_id(&model, Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0)).expect("gable wall not found");
    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&gable_id).expect("gable wall mesh must be Some");
    let max_z = mesh.positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
    assert!((max_z - 2.4_f32).abs() < 1e-3, "with a flat roof the gable wall must stay at 2.4, got {max_z}");
}

#[test]
fn all_four_walls_have_some_mesh() {
    let model = gable_house();
    let scene = Scene3d::build(&model);
    for wall in &model.walls {
        let mesh = scene.mesh_for(&wall.id);
        assert!(mesh.is_some(), "wall {:?} must have a mesh", wall);
    }
}

#[test]
fn mesh_positions_are_finite() {
    let model = gable_house();
    let scene = Scene3d::build(&model);
    for wall in &model.walls {
        if let Some(mesh) = scene.mesh_for(&wall.id) {
            for p in &mesh.positions {
                assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite(), "non-finite position in wall mesh");
            }
        }
    }
}

#[test]
fn incremental_eave_wall_agrees_with_full() {
    let model = gable_house();
    let eave_id = find_wall_id(&model, Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)).expect("eave wall not found");
    let full = Scene3d::build(&model);
    let full_mesh = full.mesh_for(&eave_id).expect("full eave mesh must be Some");
    let dirty = BTreeSet::from([eave_id]);
    let incremental = Scene3d::build_dirty(&model, &dirty, &full);
    let inc_mesh = incremental.mesh_for(&eave_id).expect("incremental eave mesh must be Some");
    assert_eq!(inc_mesh.positions, full_mesh.positions, "build_dirty must reproduce the same eave mesh positions as build");
}
