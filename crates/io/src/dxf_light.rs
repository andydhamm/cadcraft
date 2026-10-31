//! Save-time proxies for light fixtures (phase 10): each fixture's plan symbol on `E-LITE`, so a
//! drawing opened elsewhere still shows the lights. Written only on save, dropped on read while
//! the arch record is present (like site proxies). `proxy_light_entities` is filled by
//! `p10-io-light-proxies`.

use cadcraft_doc::arch::ArchModel;
use cadcraft_dxf::Tag;

/// Marker in the `CADCRAFT` xdata of every light proxy.
pub(crate) const LIGHT_PROXY_MARKER: &str = "CADCRAFT-LIGHT";
/// The AIA/NCS electrical lighting layer.
pub(crate) const LIGHT_LAYER: &str = "E-LITE";

/// `(entity kind, layer, tags)` for every light proxy (`LWPOLYLINE`s on [`LIGHT_LAYER`]).
pub(crate) fn proxy_light_entities(m: &ArchModel) -> Vec<(String, String, Vec<Tag>)> {
    let _ = m;
    Vec::new()
}

/// True when an entity's `CADCRAFT` xdata marks it as a light proxy.
pub(crate) fn is_light_proxy(tags: &[Tag]) -> bool {
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str().trim() == LIGHT_PROXY_MARKER)
}
