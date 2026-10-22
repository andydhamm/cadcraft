//! 3D viewport rendering with wgpu — the only crate allowed to use GPU dependencies.
//!
//! Provides:
//! - [`camera`]: orbit camera with view/projection matrices.
//! - [`pipeline`]: GPU mesh upload and render pipeline setup.
//! - [`offscreen`]: offscreen wgpu renderer for headless PNG export.

pub mod camera;
pub mod offscreen;
pub mod pipeline;

pub use camera::{Camera, Orbit};
pub use offscreen::{Offscreen, Render3dError};
pub use pipeline::{GpuMeshes, upload};

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
