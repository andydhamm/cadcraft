//! Save-time proxies for the site (phase 9), so a drawing opened elsewhere still shows contours,
//! labels, the boundary, paths, plants and the north arrow. Written only on save, dropped on read
//! while the arch record is present (like view proxies). Filled in by `p9-io-site-proxies`.

use cadcraft_doc::arch::ArchModel;

use cadcraft_dxf::Tag;

/// Marker in the `CADCRAFT` xdata of every site proxy.
pub(crate) const SITE_PROXY_MARKER: &str = "CADCRAFT-SITE";

/// `(entity kind, layer, tags)` for every site proxy: `LWPOLYLINE`, `CIRCLE` or `TEXT`.
pub(crate) fn proxy_site_entities(m: &ArchModel) -> Vec<(String, String, Vec<Tag>)> {
    let _ = m;
    Vec::new()
}

/// True when an entity's `CADCRAFT` xdata marks it as a site proxy.
pub(crate) fn is_site_proxy(tags: &[Tag]) -> bool {
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str().trim() == SITE_PROXY_MARKER)
}
