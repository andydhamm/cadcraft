//! Tests that entity handles that fall in the old 0xA000_0000 range (below the
//! new ARCH_HANDLE_BASE = 0xFFFF_FFFF_0000_0000) are treated as ordinary entity
//! handles and are fully usable via grips.

use cadcraft_doc::{EntityKind, Handle, is_arch_handle};
use cadcraft_engine::{Session, grips::GripMode};
use cadcraft_geom::Vec2;

/// A minimal DXF with a single LINE at handle A0000001 — the start of the *old*
/// 0xA000_0000 reserved base, which is now well below the current ARCH_HANDLE_BASE
/// (0xFFFF_FFFF_0000_0000) and must be accepted as an ordinary entity handle.
const DXF_LINE_AT_A0000001: &str = "\
0\nSECTION\n2\nENTITIES\n\
0\nLINE\n5\nA0000001\n8\n0\n\
10\n0.0\n20\n0.0\n30\n0.0\n\
11\n10.0\n21\n0.0\n31\n0.0\n\
0\nENDSEC\n0\nEOF\n";

#[test]
fn a_file_line_at_a0000001_is_an_ordinary_entity() {
    let d = cadcraft_io::read_dxf(DXF_LINE_AT_A0000001.as_bytes()).unwrap();
    let handle = Handle::parse_hex("A0000001").unwrap();

    // The entity must exist with the original handle A0000001.
    assert!(d.entity(handle).is_some(), "entity with handle A0000001 must exist");
    assert!(matches!(d.entity(handle).unwrap().kind, EntityKind::Line(_)), "entity at A0000001 must be a LINE");

    // A0000001 is below the new ARCH_HANDLE_BASE, so it must not be an arch handle.
    assert!(!is_arch_handle(handle), "handle A0000001 must not be classified as an arch proxy");

    // Open the drawing in a session and verify the entity is accessible.
    let mut s = Session::empty();
    s.open_drawing(d, "test", None);
    assert!(s.doc().unwrap().entity(handle).is_some(), "entity must be accessible via session");

    // Grip-stretch: move the end point (grip index 2) from (10, 0) to (20, 0).
    s.grip_edit(handle, 2, Vec2::new(20.0, 0.0), GripMode::Stretch).unwrap();

    // Verify the line's end point moved.
    let doc = s.doc().unwrap();
    let entity = doc.entity(handle).unwrap();
    match &entity.kind {
        EntityKind::Line(l) => {
            assert!((l.b.x - 20.0).abs() < 1e-9, "line end x should be 20 after stretch, got {}", l.b.x);
            assert!(l.b.y.abs() < 1e-9, "line end y should remain 0, got {}", l.b.y);
        }
        _ => panic!("entity at A0000001 should still be a LINE after stretch"),
    }
}
