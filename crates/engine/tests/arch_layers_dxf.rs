use cadcraft_doc::Drawing;
use cadcraft_engine::Session;
use serde_json::json;

/// Write a drawing built through the commands (a wall `w0`, then a door on it)
/// to DXF; verify its LAYER table lists the arch proxy layers created on first
/// use: `A-WALL` (from the wall) and `A-DOOR` (from the door).
#[test]
fn a_saved_model_lists_its_arch_layers() {
    let mut s = Session::new();
    s.execute("wall", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
    s.execute("door", &json!({ "wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1 })).unwrap();

    let d = s.doc().unwrap();
    let bytes = cadcraft_io::write(d, "model.dxf").unwrap();
    let round: Drawing = cadcraft_io::read(&bytes, "model.dxf").unwrap();
    let names: Vec<String> = round.layers.iter().map(|l| l.name.clone()).collect();
    assert!(names.iter().any(|n| n == "A-WALL"), "A-WALL missing from saved model layers: {names:?}");
    assert!(names.iter().any(|n| n == "A-DOOR"), "A-DOOR missing from saved model layers: {names:?}");
}
