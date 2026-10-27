//! Tests that DXF entity handles in the arch-proxy reserved range are replaced
//! with fresh ordinary handles so they can never be confused with synthetic
//! arch proxies.

use cadcraft_doc::{ARCH_HANDLE_BASE, Handle, is_arch_handle};

/// A minimal DXF with a single LINE whose handle falls in the reserved range
/// [ARCH_HANDLE_BASE, u64::MAX].  The reader must assign a fresh handle below
/// ARCH_HANDLE_BASE instead.
const DXF_RESERVED_HANDLE: &str = "\
0\nSECTION\n2\nENTITIES\n\
0\nLINE\n5\nFFFFFFFF00000005\n8\n0\n\
10\n0.0\n20\n0.0\n30\n0.0\n\
11\n10.0\n21\n0.0\n31\n0.0\n\
0\nENDSEC\n0\nEOF\n";

#[test]
fn a_file_handle_in_the_reserved_range_is_reassigned() {
    let d = cadcraft_io::read_dxf(DXF_RESERVED_HANDLE.as_bytes()).unwrap();

    // The original handle FFFFFFFF00000005 must not appear in the drawing.
    let original = Handle(0xFFFF_FFFF_0000_0005);
    assert!(d.entity(original).is_none(), "entity with the original reserved handle must not exist");

    // Exactly one entity must have been placed (the LINE).
    let entities: Vec<_> = d.model.iter().collect();
    assert_eq!(entities.len(), 1, "expected one entity after reading");

    let reassigned = entities[0].handle;

    // The reassigned handle must be below ARCH_HANDLE_BASE.
    assert!(reassigned.0 < ARCH_HANDLE_BASE, "reassigned handle {:#x} must be below ARCH_HANDLE_BASE {:#x}", reassigned.0, ARCH_HANDLE_BASE,);

    // It must not be classified as an arch handle.
    assert!(!is_arch_handle(reassigned), "reassigned handle must not be an arch handle");
}
