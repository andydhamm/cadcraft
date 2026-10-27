//! 3D viewport rendering with wgpu — the only crate allowed to use GPU dependencies.
//!
//! Provides:
//! - [`camera`]: orbit camera with view/projection matrices.
//! - [`pipeline`]: GPU mesh upload and render pipeline setup.
//! - [`offscreen`]: offscreen wgpu renderer for headless PNG export.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod camera;
pub mod light;
pub mod offscreen;
pub mod pipeline;
pub mod types;

#[cfg(test)]
pub(crate) mod gputest;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_denies_unwrap_and_panic() {
        let src = include_str!("lib.rs");
        assert!(src.contains("deny(clippy::unwrap_used"), "missing deny(clippy::unwrap_used)");
        assert!(src.contains("forbid(unsafe_code)"), "missing forbid(unsafe_code)");
    }
}

pub use camera::{Camera, Orbit, projected_bounds, projected_scene_bounds};
pub use light::Light;
pub use offscreen::{Offscreen, headless_png, render_png};
pub use pipeline::{Draw, GpuMeshes, material_albedo, upload};
pub use types::{ArchId, Bounds3, Mesh, Scene3d};

/// Errors returned by the render3d crate.
#[derive(Debug, thiserror::Error)]
pub enum Render3dError {
    /// No GPU adapter is available (headless / CI).
    #[error("no GPU adapter available")]
    NoAdapter,

    /// A low-level wgpu operation failed.
    #[error("wgpu error: {0}")]
    Wgpu(String),

    /// A buffer or other resource allocation failed.
    #[error("buffer allocation failed: {0}")]
    Buffer(String),
}
