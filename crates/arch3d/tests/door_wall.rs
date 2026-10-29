use cadcraft_arch3d::Scene3d;
use cadcraft_doc::arch::{Opening, OpeningKind, wall_label};
use cadcraft_doc::{ArchId, ArchModel, Wall};
use cadcraft_geom::{Vec2, Vec3};

fn wall_with_door() -> (ArchModel, ArchId) {
    let mut model = ArchModel::default();
    let wall_id = model.push_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(10.0, 0.0),
        thickness: 0.2,
        height: 2.4,
        exterior: true,
        ..Default::default()
    });
    model.push_opening(Opening::new(wall_id, 3.0, 0.9, 2.1, OpeningKind::Door));
    (model, wall_id)
}

#[test]
fn a_wall_with_a_door_is_solid_beside_and_above_the_door() {
    let (model, wall_id) = wall_with_door();
    let scene = Scene3d::build(&model);

    for p in [Vec3::new(1.0, 0.0, 1.0), Vec3::new(6.0, 0.0, 1.0), Vec3::new(3.45, 0.0, 2.3)] {
        let hit = scene.probe(p).expect("probe should hit the wall beside or above the door");
        assert_eq!(hit.part, "wall", "probe at {:?} hit {} instead of wall", p, hit.part);
        assert_eq!(hit.id, wall_label(wall_id), "probe at {:?} hit wrong wall id", p);
    }
}

#[test]
fn the_door_opening_itself_is_open() {
    let (model, _) = wall_with_door();
    let scene = Scene3d::build(&model);
    let hit = scene.probe(Vec3::new(3.45, 0.0, 1.0));
    assert!(hit.is_none() || hit.as_ref().is_some_and(|h| h.part != "wall"), "door opening should not hit wall material");
}

#[test]
fn a_wall_with_a_door_keeps_its_outer_and_inner_faces() {
    let (model, wall_id) = wall_with_door();
    let scene = Scene3d::build(&model);
    let mesh = scene.mesh_for(&wall_id).expect("wall mesh is missing");

    let mut found_outer = false;
    let mut found_inner = false;

    for tri in mesh.indices.chunks_exact(3) {
        let a = mesh.positions[tri[0] as usize];
        let b = mesh.positions[tri[1] as usize];
        let c = mesh.positions[tri[2] as usize];
        let v1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let nx = v1[1] * v2[2] - v1[2] * v2[1];
        let ny = v1[2] * v2[0] - v1[0] * v2[2];
        let nz = v1[0] * v2[1] - v1[1] * v2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let normal = if len > 1e-12 { [nx / len, ny / len, nz / len] } else { [0.0, 0.0, 1.0] };

        let center_y = (a[1] + b[1] + c[1]) / 3.0;
        if (center_y + 0.1).abs() < 1e-4 {
            found_outer = found_outer || normal[1] < 0.0;
        }
        if (center_y - 0.1).abs() < 1e-4 {
            found_inner = found_inner || normal[1] > 0.0;
        }
    }

    assert!(found_outer, "outer wall face beside the door should face -y");
    assert!(found_inner, "inner wall face beside the door should face +y");
}
