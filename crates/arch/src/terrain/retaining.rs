//! Retaining walls: their top and bottom from the ground on each side. Filled in by
//! `p9-arch-retaining`.

use cadcraft_doc::arch::{ArchModel, Wall};

/// `(base, top)` of retaining wall `w`, or `None` without a terrain (the wall keeps its level span).
pub fn retaining_span(model: &ArchModel, w: &Wall) -> Option<(f64, f64)> {
    let _ = (model, w);
    None
}
