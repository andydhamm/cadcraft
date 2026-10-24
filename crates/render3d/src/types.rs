//! Shared 3D scene types used by the renderer.
//!
//! The scene types are owned by `cadcraft_arch3d`; this module re-exports them so the
//! renderer and its callers use one source of truth.

pub use cadcraft_arch3d::{ArchId, Bounds3, Mesh, Scene3d};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch3d_archid_is_re_exported() {
        let id = ArchId(7);
        assert_eq!(id.0, 7);
    }
}
