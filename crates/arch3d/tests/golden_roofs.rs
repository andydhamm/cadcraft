//! Golden roof scenes for the phase-1 house.
//!
//! Verifies that the automatic roof generator (`cadcraft_arch::roof`) produces the
//! locked hip/gable/mixed geometry and that `arch3d`'s incremental build reproduces
//! the full build exactly when only the roof (or only the walls + roof) is dirty.
//! No file I/O and no external crates.

use std::collections::BTreeSet;

use cadcraft_doc::{ArchId, ArchModel, Level, Roof, RoofDirective, RoofEdgeKind, Wall};
use cadcraft_geom::Vec2;

/// The phase-1 house: one level plus the four exterior centre-line walls.
fn house() -> ArchModel {
    let mut model = ArchModel::default();
    model.levels.push(Level { index: 0, elevation: 0.0, ceiling_height: 2.4, ..Default::default() });
    model.push_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(10.0, 0.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    model.push_wall(Wall {
        start: Vec2::new(10.0, 0.0),
        end: Vec2::new(10.0, 8.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    model.push_wall(Wall {
        start: Vec2::new(10.0, 8.0),
        end: Vec2::new(0.0, 8.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    model.push_wall(Wall {
        start: Vec2::new(0.0, 8.0),
        end: Vec2::new(0.0, 0.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        base_level: 0,
        ..Default::default()
    });
    model
}

fn roof(rise: f64, overhang: f64) -> Roof {
    Roof { pitch_rise: rise, pitch_run: 12.0, overhang, thickness: 0.0, ..Roof::default() }
}

/// Set per-wall directives on the phase-1 house and attach the resulting roof.
fn with_roof(model: &mut ArchModel, directives: &[(usize, RoofDirective)], r: &Roof) {
    for (i, d) in directives {
        model.walls[*i].roof_directive = d.clone();
    }
    let built = cadcraft_arch::roof::build_roof(model, 0, r).expect("roof builds");
    model.push_roof(built);
}

/// Highest z of the wall/roof mesh tagged with `id`.
fn mesh_max_z(scene: &cadcraft_arch3d::Scene3d, id: ArchId) -> f32 {
    let m = scene.meshes.iter().find(|m| m.object == id).expect("mesh for id missing");
    m.positions.iter().map(|p| p[2] as f64).max().expect("wall mesh has no vertices") as f32
}

/// Vertices of the mesh tagged with `id` whose z is within 1e-3 of `z`.
fn top_vertices_at(scene: &cadcraft_arch3d::Scene3d, id: ArchId, z: f32) -> Vec<[f32; 3]> {
    let m = scene.meshes.iter().find(|m| m.object == id).expect("mesh for id missing");
    m.positions.iter().filter(|p| (p[2] - z).abs() < 1e-3).cloned().collect()
}

/// Is the 2-D point `p` within `tol` (both axes) of `q`?
fn near(p: &Vec2, q: &Vec2, tol: f64) -> bool {
    (p.x - q.x).abs() <= tol && (p.y - q.y).abs() <= tol
}

/// Does the edge with endpoints `a`/`b` have `a` near `p` and `b` near `q` (order ignored)?
fn edge_endpoints_match(a: &Vec2, b: &Vec2, p: Vec2, q: Vec2, tol: f64) -> bool {
    (near(a, &p, tol) && near(b, &q, tol)) || (near(a, &q, tol) && near(b, &p, tol))
}

/// The hip (4/12), gable (walls 1+3) and mixed (wall 1 only) golden scenes.
#[test]
fn golden_hip_gable_mixed() {
    let all_auto = [(0usize, RoofDirective::Auto), (1, RoofDirective::Auto), (2, RoofDirective::Auto), (3, RoofDirective::Auto)];

    // Case 1: hip roof — all walls auto.
    {
        let mut model = house();
        with_roof(&mut model, &all_auto, &roof(6.0, 0.0));
        let r = &model.roofs[0];
        assert_eq!(r.planes.len(), 4);
        assert!((cadcraft_arch::roof::roof_apex_z(r) - 4.45).abs() <= 1e-6, "hip 6/12 apex");
        assert!((cadcraft_arch::roof::projected_area_sum(r) - 83.64).abs() <= 1e-6, "hip 6/12 area");
        let e = r.edges.iter().find(|e| e.kind == RoofEdgeKind::Ridge).expect("hip roof has a ridge edge");
        assert!((e.za - 4.45).abs() <= 1e-6, "hip ridge za");
        assert!((e.zb - 4.45).abs() <= 1e-6, "hip ridge zb");
        assert!(edge_endpoints_match(&e.a, &e.b, Vec2::new(4.0, 4.0), Vec2::new(6.0, 4.0), 1e-6), "hip ridge endpoints");

        let scene = cadcraft_arch3d::build(&model);
        for i in 0..4 {
            let z = mesh_max_z(&scene, model.walls[i].id);
            assert!((z - 2.4).abs() < 1e-3, "hip wall {} max z", i);
        }
        assert_eq!(scene.roofs, 1);
    }

    // Case 2: gable on walls 1 and 3.
    {
        let mut model = house();
        let gable = [(0usize, RoofDirective::Auto), (1, RoofDirective::Gable), (2, RoofDirective::Auto), (3, RoofDirective::Gable)];
        with_roof(&mut model, &gable, &roof(6.0, 0.0));
        let r = &model.roofs[0];
        assert_eq!(r.planes.len(), 2);
        let e = r.edges.iter().find(|e| e.kind == RoofEdgeKind::Ridge).expect("gable roof has a ridge edge");
        assert!((e.za - 4.45).abs() <= 1e-6, "gable ridge za");
        assert!((e.zb - 4.45).abs() <= 1e-6, "gable ridge zb");
        assert!(edge_endpoints_match(&e.a, &e.b, Vec2::new(-0.1, 4.0), Vec2::new(10.1, 4.0), 1e-6), "gable ridge endpoints");

        let scene = cadcraft_arch3d::build(&model);
        for i in [1, 3] {
            let z = mesh_max_z(&scene, model.walls[i].id);
            assert!((z - 4.45).abs() < 1e-3, "gable wall {} max z", i);
            assert!(top_vertices_at(&scene, model.walls[i].id, 4.45).iter().any(|v| (v[1] - 4.0).abs() < 1e-3), "gable wall {} vertex at y=4", i);
        }
        for i in [0, 2] {
            let z = mesh_max_z(&scene, model.walls[i].id);
            assert!((z - 2.4).abs() < 1e-3, "gable eave wall {} max z", i);
        }
    }

    // Case 3: mixed — gable on wall 1 only.
    {
        let mut model = house();
        let mixed = [(0usize, RoofDirective::Auto), (1, RoofDirective::Gable), (2, RoofDirective::Auto), (3, RoofDirective::Auto)];
        with_roof(&mut model, &mixed, &roof(6.0, 0.0));
        let r = &model.roofs[0];
        assert_eq!(r.planes.len(), 3);
        let e = r.edges.iter().find(|e| e.kind == RoofEdgeKind::Ridge).expect("mixed roof has a ridge edge");
        assert!((e.za - 4.45).abs() <= 1e-6, "mixed ridge za");
        assert!((e.zb - 4.45).abs() <= 1e-6, "mixed ridge zb");
        assert!(edge_endpoints_match(&e.a, &e.b, Vec2::new(4.0, 4.0), Vec2::new(10.1, 4.0), 1e-6), "mixed ridge endpoints");
    }

    // Hip at 4/12 and 12/12, and overhang 0.5.
    {
        let mut model = house();
        with_roof(&mut model, &all_auto, &roof(4.0, 0.0));
        assert!((cadcraft_arch::roof::roof_apex_z(&model.roofs[0]) - 3.766667).abs() <= 1e-6, "hip 4/12 apex");
        let mut model = house();
        with_roof(&mut model, &all_auto, &roof(12.0, 0.0));
        assert!((cadcraft_arch::roof::roof_apex_z(&model.roofs[0]) - 6.5).abs() <= 1e-6, "hip 12/12 apex");
        let mut model = house();
        with_roof(&mut model, &all_auto, &roof(6.0, 0.5));
        let r = &model.roofs[0];
        assert_eq!(r.planes.len(), 4, "hip overhang 0.5 still 4 planes");
        assert!((cadcraft_arch::roof::projected_area_sum(r) - 103.04).abs() <= 1e-3, "hip overhang 0.5 area");
    }
}

#[test]
fn build_matches_build_dirty_with_roofs() {
    let hip: &[(usize, RoofDirective)] = &[(0, RoofDirective::Auto), (1, RoofDirective::Auto), (2, RoofDirective::Auto), (3, RoofDirective::Auto)];
    let gable: &[(usize, RoofDirective)] =
        &[(0, RoofDirective::Auto), (1, RoofDirective::Gable), (2, RoofDirective::Auto), (3, RoofDirective::Gable)];

    for (label, directives) in [("hip", hip), ("gable", gable)] {
        let mut model = house();
        with_roof(&mut model, directives, &roof(6.0, 0.0));
        let full = cadcraft_arch3d::build(&model);

        // All walls + roof dirty.
        let dirty: BTreeSet<ArchId> = model.walls.iter().map(|w| w.id).chain(model.roofs.iter().map(|r| r.id)).collect();
        let partial = cadcraft_arch3d::build_dirty(&model, &dirty, &full);
        assert_eq!(partial.meshes.len(), full.meshes.len(), "{label}: mesh count");
        assert_eq!(partial.roofs, full.roofs, "{label}: roof count");
        for (a, b) in partial.meshes.iter().zip(full.meshes.iter()) {
            assert_eq!(a.object, b.object, "{label}: object");
            assert_eq!(a.material, b.material, "{label}: material");
            assert_eq!(a.positions, b.positions, "{label}: positions");
            assert_eq!(a.normals, b.normals, "{label}: normals");
            assert_eq!(a.uvs, b.uvs, "{label}: uvs");
            assert_eq!(a.indices, b.indices, "{label}: indices");
        }

        // Only the single roof dirty: roof meshes must still match the full build.
        let roof_id = model.roofs[0].id;
        let single_roof: BTreeSet<ArchId> = model.roofs.iter().map(|r| r.id).collect();
        let partial_roof_only = cadcraft_arch3d::build_dirty(&model, &single_roof, &full);
        let pf: Vec<_> = partial_roof_only.meshes.iter().filter(|m| m.object == roof_id).collect();
        let ff: Vec<_> = full.meshes.iter().filter(|m| m.object == roof_id).collect();
        assert_eq!(pf.len(), ff.len(), "{label}: roof mesh count");
        for (a, b) in pf.iter().zip(ff.iter()) {
            assert_eq!(a.object, b.object, "{label}: roof object");
            assert_eq!(a.material, b.material, "{label}: roof material");
            assert_eq!(a.positions, b.positions, "{label}: roof positions");
            assert_eq!(a.normals, b.normals, "{label}: roof normals");
            assert_eq!(a.uvs, b.uvs, "{label}: roof uvs");
            assert_eq!(a.indices, b.indices, "{label}: roof indices");
        }
    }
}
