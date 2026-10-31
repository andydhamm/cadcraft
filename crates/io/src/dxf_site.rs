//! Save-time proxies for the site (phase 9), so a drawing opened elsewhere still shows contours,
//! labels, the boundary, paths, plants and the north arrow. Written only on save, dropped on read
//! while the arch record is present (like view proxies).

use cadcraft_arch::terrain::contour::{label_point, label_text, site_contours};
use cadcraft_arch::terrain::mesh::site_mesh;
use cadcraft_arch::terrain::surface::SiteSurface;
use cadcraft_doc::arch::{ArchModel, DEFAULT_CONTOUR_MAJOR, DEFAULT_CONTOUR_MINOR, ModifierKind};
use cadcraft_dxf::Tag;
use cadcraft_geom::Vec2;
use cadcraft_geom::tin_ops::ContourLine;

/// Marker in the `CADCRAFT` xdata of every site proxy.
pub(crate) const SITE_PROXY_MARKER: &str = "CADCRAFT-SITE";

const LAYER_BOUNDARY: &str = "C-TOPO-BNDY";
const LAYER_MINOR: &str = "C-TOPO-MINR";
const LAYER_MAJOR: &str = "C-TOPO-MAJR";
const LAYER_TEXT: &str = "C-TOPO-TEXT";
const LAYER_GRADE: &str = "C-TOPO-GRAD";
const LAYER_ROAD: &str = "C-ROAD";
const LAYER_PLANT: &str = "L-PLNT";
const LAYER_NORTH: &str = "A-ANNO-NRTH";

/// Height of the contour labels and the north "N" (m).
const LABEL_HEIGHT: f64 = 0.35;
/// Most proxies written for the site, so a pathological terrain can't flood a save.
const MAX_SITE_PROXIES: usize = 100_000;

type Proxy = (String, String, Vec<Tag>);

fn marked(mut tags: Vec<Tag>) -> Vec<Tag> {
    tags.push(Tag::s(1001, crate::dxf_ext::APP));
    tags.push(Tag::s(1000, SITE_PROXY_MARKER));
    tags
}

fn polyline(layer: &str, pts: &[Vec2], closed: bool) -> Option<Proxy> {
    if pts.len() < 2 || pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    let n = i64::try_from(pts.len()).ok()?;
    let mut tags = vec![Tag::i(90, n), Tag::i(70, i64::from(closed))];
    for p in pts {
        tags.push(Tag::f(10, p.x));
        tags.push(Tag::f(20, p.y));
    }
    Some(("LWPOLYLINE".into(), layer.into(), marked(tags)))
}

fn text(layer: &str, at: Vec2, s: &str, angle_deg: f64) -> Option<Proxy> {
    if !at.x.is_finite() || !at.y.is_finite() || !angle_deg.is_finite() {
        return None;
    }
    let mut tags = vec![Tag::f(10, at.x), Tag::f(20, at.y), Tag::f(30, 0.0), Tag::s(1, s), Tag::f(40, LABEL_HEIGHT)];
    if angle_deg != 0.0 {
        tags.push(Tag::f(50, angle_deg));
    }
    tags.push(Tag::s(7, "Standard"));
    Some(("TEXT".into(), layer.into(), marked(tags)))
}

/// The reading angle (degrees, within -90..=90) of the contour segment nearest `at`.
fn label_angle(line: &ContourLine, at: Vec2) -> f64 {
    let pts = &line.points;
    let n = pts.len();
    let segs = if line.closed { n } else { n.saturating_sub(1) };
    let mut best: Option<(f64, Vec2)> = None;
    for i in 0..segs {
        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
        let d = a.dist(at) + b.dist(at) - a.dist(*b);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, *b - *a));
        }
    }
    let Some((_, v)) = best else { return 0.0 };
    let mut deg = v.y.atan2(v.x).to_degrees();
    if !deg.is_finite() {
        return 0.0;
    }
    if deg > 90.0 {
        deg -= 180.0;
    } else if deg <= -90.0 {
        deg += 180.0;
    }
    deg
}

/// `(entity kind, layer, tags)` for every site proxy: `LWPOLYLINE`, `CIRCLE` or `TEXT`.
pub(crate) fn proxy_site_entities(m: &ArchModel) -> Vec<(String, String, Vec<Tag>)> {
    let mut out: Vec<Proxy> = Vec::new();
    if let Some(t) = &m.terrain {
        if t.boundary.len() >= 3 {
            out.extend(polyline(LAYER_BOUNDARY, &t.boundary, true));
        }
        let (minor, major) = if t.contour_minor.is_finite() && t.contour_major.is_finite() {
            (t.contour_minor, t.contour_major)
        } else {
            (DEFAULT_CONTOUR_MINOR, DEFAULT_CONTOUR_MAJOR)
        };
        if let Ok(surf) = SiteSurface::new(m)
            && let Ok(mesh) = site_mesh(&surf)
        {
            let c = site_contours(&mesh, minor, major);
            let major_bits: Vec<u64> = c.major.iter().map(|l| l.elevation.to_bits()).collect();
            for l in c.minor.iter().filter(|l| !major_bits.contains(&l.elevation.to_bits())) {
                if out.len() >= MAX_SITE_PROXIES {
                    break;
                }
                out.extend(polyline(LAYER_MINOR, &l.points, l.closed));
            }
            for l in &c.major {
                if out.len() >= MAX_SITE_PROXIES {
                    break;
                }
                out.extend(polyline(LAYER_MAJOR, &l.points, l.closed));
                if let Some(at) = label_point(l) {
                    out.extend(text(LAYER_TEXT, at, &label_text(l.elevation), label_angle(l, at)));
                }
            }
        }
        for md in &t.modifiers {
            match &md.kind {
                ModifierKind::Flat { polygon, .. } => out.extend(polyline(LAYER_GRADE, polygon, true)),
                ModifierKind::Path { polygon, .. } => out.extend(polyline(LAYER_ROAD, polygon, true)),
                ModifierKind::Hill { .. } => {}
            }
        }
    }
    for p in &m.plants {
        if out.len() >= MAX_SITE_PROXIES {
            break;
        }
        let Some(sp) = cadcraft_arch::plant::find_species(m, &p.species) else { continue };
        let (_, spread) = cadcraft_arch::plant::plant_size(&sp, p);
        if !p.position.x.is_finite() || !p.position.y.is_finite() || !spread.is_finite() || spread <= 0.0 {
            continue;
        }
        let tags = vec![Tag::f(10, p.position.x), Tag::f(20, p.position.y), Tag::f(30, 0.0), Tag::f(40, spread / 2.0)];
        out.push(("CIRCLE".into(), LAYER_PLANT.into(), marked(tags)));
    }
    if let Some(a) = cadcraft_arch::site::north_arrow(m) {
        out.extend(polyline(LAYER_NORTH, &a.outline, false));
        let at = Vec2::new(a.at.x + a.direction.x * 2.3, a.at.y + a.direction.y * 2.3);
        out.extend(text(LAYER_NORTH, at, "N", 0.0));
    }
    out
}

/// True when an entity's `CADCRAFT` xdata marks it as a site proxy.
pub(crate) fn is_site_proxy(tags: &[Tag]) -> bool {
    crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str().trim() == SITE_PROXY_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::{Plant, Terrain};
    use cadcraft_geom::Vec3;

    fn sloped() -> ArchModel {
        let mut m = ArchModel::default();
        m.terrain = Some(Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)],
            points: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 2.0), Vec3::new(0.0, 10.0, 2.0)],
            ..Terrain::default()
        });
        m.plants.push(Plant { species: "Red Oak".into(), position: Vec2::new(5.0, 5.0), age: 50.0, ..Plant::default() });
        m
    }

    #[test]
    fn site_proxies_cover_contours_labels_plants_and_arrow() {
        let m = sloped();
        let p = proxy_site_entities(&m);
        let count = |layer: &str, kind: &str| p.iter().filter(|(k, l, _)| l == layer && k == kind).count();
        assert_eq!(count("C-TOPO-BNDY", "LWPOLYLINE"), 1);
        assert!(count("C-TOPO-MINR", "LWPOLYLINE") > 0, "minor contours");
        assert!(count("C-TOPO-MAJR", "LWPOLYLINE") > 0, "major contours");
        assert_eq!(count("C-TOPO-TEXT", "TEXT"), count("C-TOPO-MAJR", "LWPOLYLINE"));
        assert_eq!(count("L-PLNT", "CIRCLE"), 1);
        assert_eq!(count("A-ANNO-NRTH", "LWPOLYLINE"), 1);
        assert_eq!(count("A-ANNO-NRTH", "TEXT"), 1);
        assert!(p.iter().all(|(_, _, tags)| is_site_proxy(tags)));
    }

    #[test]
    fn site_proxies_are_dropped_on_read() {
        let m = sloped();
        let mut d = cadcraft_doc::Drawing::default();
        d.arch = m;
        let n = proxy_site_entities(&d.arch).len();
        assert!(n > 0);
        let text = crate::write_dxf(&d);
        let back = crate::read_dxf(text.as_bytes()).expect("reopen");
        assert_eq!(proxy_site_entities(&back.arch).len(), n, "same proxies, none duplicated");
        let again = crate::write_dxf(&back);
        assert_eq!(again.matches("CADCRAFT-SITE").count(), text.matches("CADCRAFT-SITE").count());
    }
}
