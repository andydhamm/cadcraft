use std::cmp::Ordering;

use cadcraft_arch::area::refresh_areas;
use cadcraft_arch::room::detect_rooms;
use cadcraft_doc::{ArchModel, Room, Wall};
use cadcraft_geom::Vec2;

/// Build a wall from its centre-line endpoints and thickness.
fn wall(a: Vec2, b: Vec2, thickness: f64, exterior: bool) -> Wall {
    Wall { start: a, end: b, thickness, exterior, ..Default::default() }
}

/// The phase-1 10 x 8 house: exterior centre lines at 0.2 m and two
/// interior walls at 0.1 m.
fn house() -> ArchModel {
    let mut model = ArchModel::default();
    // Exterior centre lines: (0,0) (10,0) (10,8) (0,8), 0.2 m.
    model.push_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2, true));
    model.push_wall(wall(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2, true));
    model.push_wall(wall(Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0), 0.2, true));
    model.push_wall(wall(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2, true));
    // Interior walls, 0.1 m.
    model.push_wall(wall(Vec2::new(0.0, 3.0), Vec2::new(10.0, 3.0), 0.1, false));
    model.push_wall(wall(Vec2::new(6.0, 3.0), Vec2::new(6.0, 8.0), 0.1, false));
    model
}

/// The house plus a closed 0.1 m loop with centre lines 2.5..3.5 x 1.0..2.0.
fn house_with_loop() -> ArchModel {
    let mut model = house();
    model.push_wall(wall(Vec2::new(2.5, 1.0), Vec2::new(3.5, 1.0), 0.1, false));
    model.push_wall(wall(Vec2::new(3.5, 1.0), Vec2::new(3.5, 2.0), 0.1, false));
    model.push_wall(wall(Vec2::new(3.5, 2.0), Vec2::new(2.5, 2.0), 0.1, false));
    model.push_wall(wall(Vec2::new(2.5, 2.0), Vec2::new(2.5, 1.0), 0.1, false));
    model
}

/// Run detection into `model.rooms`, then compute interior/standard areas.
fn detect_and_areas(model: &mut ArchModel) {
    let rooms = detect_rooms(model);
    model.rooms = rooms;
    refresh_areas(model);
}

fn approx(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn empty_model_detects_no_rooms() {
    let mut model = ArchModel::default();
    detect_and_areas(&mut model);
    assert_eq!(model.rooms.len(), 0, "expected 0 rooms for empty model");
}

#[test]
fn single_closed_rectangle_detects_one_room() {
    let mut model = ArchModel::default();
    // 4 x 3 rectangle with 0.1 m exterior walls
    model.push_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), 0.1, true));
    model.push_wall(wall(Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), 0.1, true));
    model.push_wall(wall(Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0), 0.1, true));
    model.push_wall(wall(Vec2::new(0.0, 3.0), Vec2::new(0.0, 0.0), 0.1, true));
    detect_and_areas(&mut model);
    assert_eq!(model.rooms.len(), 1, "expected 1 room for single rectangle");
    // Interior area: (4-0.1) * (3-0.1) = 3.9 * 2.9 = 11.31
    assert!(approx(model.rooms[0].interior_area, 11.31, 1e-3), "interior area {} != expected 11.31", model.rooms[0].interior_area);
}

#[test]
fn degenerate_zero_length_wall_handled() {
    let mut model = ArchModel::default();
    // A zero-length wall (start == end) should not crash detection
    model.push_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), 0.1, false));
    // Plus a normal closed rectangle
    model.push_wall(wall(Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), 0.1, true));
    model.push_wall(wall(Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), 0.1, true));
    model.push_wall(wall(Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0), 0.1, true));
    model.push_wall(wall(Vec2::new(0.0, 3.0), Vec2::new(0.0, 0.0), 0.1, true));
    detect_and_areas(&mut model);
    // Should still detect the one room from the rectangle
    assert_eq!(model.rooms.len(), 1, "expected 1 room despite zero-length wall");
}

#[test]
fn golden_interior_and_standard_areas() {
    // --- Base house: exactly 3 rooms ---
    let mut model = house();
    detect_and_areas(&mut model);
    assert_eq!(model.rooms.len(), 3, "expected 3 rooms, got {}", model.rooms.len());

    let mut rooms: Vec<&Room> = model.rooms.iter().collect();
    rooms.sort_by(|a, b| a.interior_area.partial_cmp(&b.interior_area).unwrap_or(Ordering::Equal));

    let expected_interior = [18.6725, 27.93, 28.3725];
    let expected_standard = [20.91, 31.62, 31.11];
    for (i, r) in rooms.iter().enumerate() {
        assert!(
            approx(r.interior_area, expected_interior[i], 1e-3),
            "room #{} interior area {} != expected {}",
            i,
            r.interior_area,
            expected_interior[i]
        );
        assert!(
            approx(r.standard_area, expected_standard[i], 1e-3),
            "room #{} standard area {} != expected {}",
            i,
            r.standard_area,
            expected_standard[i]
        );
    }
    let std_sum: f64 = model.rooms.iter().map(|r| r.standard_area).sum();
    assert!(approx(std_sum, 83.64, 1e-3), "standard sum {} != expected 83.64", std_sum);

    // --- Loop variant: the south room gains a hole, an inner room appears ---
    let mut model2 = house_with_loop();
    detect_and_areas(&mut model2);
    assert_eq!(model2.rooms.len(), 4, "expected 4 rooms, got {}", model2.rooms.len());

    // South room: the one containing (1, 1).
    let south = model2.rooms.iter().find(|r| cadcraft_geom::point_in_polygon(&r.polygon, Vec2::new(1.0, 1.0))).expect("south room containing (1, 1)");
    assert!(approx(south.interior_area, 26.72, 1e-3), "south interior area {} != expected 26.72", south.interior_area);
    assert_eq!(south.holes.len(), 1, "south room holes: {}, expected 1", south.holes.len());

    // New inner room with interior area 0.81.
    let inner = model2.rooms.iter().find(|r| approx(r.interior_area, 0.81, 1e-3)).expect("inner room with interior area 0.81");
    assert_eq!(inner.holes.len(), 0, "inner room has {} holes, expected 0", inner.holes.len());
}
