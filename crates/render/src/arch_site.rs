//! The site in the plan (phase 9): contours with their labels, the terrain boundary, modifier
//! and path outlines, plant symbols and the north arrow. Filled in by `p9-render-site-plan`.
//!
//! Helpers from `crate::arch`: `arch_layer_skipped(d, layer, plotting)` (true when the layer is
//! off, frozen, or no-plot while plotting) and `push_polyline(list, handle, ink, pts)`. Text is
//! drawn as strokes, as room labels are: `cadcraft_fonts::place(&TextFont::Stroke, text,
//! &TextParams::centered(at, height, angle))` and one polyline per stroke.

use cadcraft_doc::Drawing;

use crate::DisplayList;

/// Append the site's plan linework to `list`, each prim tagged with its proxy handle.
pub(crate) fn push_site(d: &Drawing, list: &mut DisplayList, plotting: bool) {
    let _ = (d, list, plotting);
}
