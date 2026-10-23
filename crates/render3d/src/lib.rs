//! 3D viewport rendering with wgpu — the only crate allowed to use GPU dependencies.
//!
//! Provides:
//! - [`camera`]: orbit camera with view/projection matrices.
//! - [`pipeline`]: GPU mesh upload and render pipeline setup.
//! - [`offscreen`]: offscreen wgpu renderer for headless PNG export.
//!
//! **Status:** the draw pass is not written yet. [`headless_png`] finds an adapter and
//! then reports [`Render3dError::NotAvailable`] rather than returning the uninitialised
//! target as if it were a picture.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod camera;
pub mod offscreen;
pub mod pipeline;
pub mod types;

pub use camera::{Camera, Orbit};
pub use offscreen::{Offscreen, headless_png};
pub use pipeline::{GpuMeshes, upload};
pub use types::{Mesh, Scene3d, Vertex};

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

    /// The requested output cannot be produced by this build (yet), or the request is
    /// outside what it supports (an image size beyond [`offscreen::MAX_SIZE`]).
    #[error("3D rendering not available: {0}")]
    NotAvailable(String),
}
