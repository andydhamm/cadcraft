//! Light fixtures in the plan (phase 10): each fixture's symbol (`arch::light::plan_symbol`) on
//! the AIA/NCS electrical lighting layer `E-LITE`, tagged with the fixture's handle.

use cadcraft_color::Color;
use cadcraft_doc::Drawing;
use cadcraft_doc::arch::light_handle;

use crate::DisplayList;
use crate::arch::{arch_layer_skipped, push_polyline};

/// The layer fixture symbols are drawn on.
pub(crate) const LIGHT_LAYER: &str = "E-LITE";

/// Append every fixture's plan symbol to `list`.
pub(crate) fn push_lights(d: &Drawing, list: &mut DisplayList, plotting: bool) {
    if d.arch.lights.is_empty() || arch_layer_skipped(d, LIGHT_LAYER, plotting) {
        return;
    }
    // Colour 2 (yellow) when the drawing has no E-LITE layer of its own.
    let ink = crate::ink(Color::ByLayer, d.layer(LIGHT_LAYER).map(|l| l.color).unwrap_or(Color::Index(2)), Color::Index(2));
    for f in &d.arch.lights {
        for sym in cadcraft_arch::light::plan_symbol(f) {
            if sym.len() >= 2 && sym.iter().all(|p| p.is_finite()) {
                push_polyline(list, light_handle(f.id), &ink, &sym);
            }
        }
    }
}
