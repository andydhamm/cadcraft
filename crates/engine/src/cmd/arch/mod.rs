//! Architecture command group.
use crate::cmd::CommandSpec;

/// Maximum number of walls that can be created in a single `wall` invocation.
pub const MAX_WALLS: usize = 4096;

/// Minimum length (drawing units) of a wall segment. Shorter segments are rejected
/// rather than producing degenerate geometry.
pub const MIN_WALL_LENGTH: f64 = 1e-6;

/// Default exterior wall thickness (m).
pub const DEFAULT_EXT_THICKNESS: f64 = 0.2;

/// Default interior wall thickness (m).
pub const DEFAULT_INT_THICKNESS: f64 = 0.1;

/// Default wall height when the model has no level (the derived floor-to-floor of the default level).
pub const DEFAULT_WALL_HEIGHT: f64 = 2.7;

pub mod build3d;
pub mod inspect;
pub mod level;
pub mod opening;
pub mod roofauto;
pub mod room;
pub mod roomdetect;
pub mod sample;
pub mod stretch;
pub mod view;
pub mod wall;

/// Default layered wall band thicknesses for the given wall side.
///
/// The values are the *natural* band thicknesses in drawing units:
/// exterior = siding 0.02, sheathing 0.015, stud 0.09, drywall 0.013 (sum 0.138)
/// interior = drywall 0.013, stud 0.09, drywall 0.013 (sum 0.116)
///
/// The caller must scale these proportionally so the bands sum exactly to the
/// requested wall thickness (see `crates/arch/src/wall.rs::wall_layer_polygons`).
pub(crate) fn wall_layers(exterior: bool) -> Vec<(String, f64)> {
    if exterior {
        vec![("siding".to_string(), 0.02), ("sheathing".to_string(), 0.015), ("stud".to_string(), 0.09), ("drywall".to_string(), 0.013)]
    } else {
        vec![("drywall".to_string(), 0.013), ("stud".to_string(), 0.09), ("drywall".to_string(), 0.013)]
    }
}

/// Command specs for the architecture module.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = Vec::new();
    v.extend(wall::specs());
    v.extend(opening::specs());
    v.extend(room::specs());
    v.extend(roomdetect::specs());
    v.extend(inspect::specs());
    v.extend(level::specs());
    v.extend(build3d::specs());
    v.extend(sample::specs());
    v.extend(stretch::specs());
    v.extend(view::specs());
    v.extend(roofauto::specs());
    v
}
