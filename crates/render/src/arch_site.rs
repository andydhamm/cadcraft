//! The site in the plan (phase 9): contours with their labels, the terrain boundary, modifier
//! and path outlines, plant symbols and the north arrow. Filled in by `p9-render-site-plan`.
//!
//! Helpers from `crate::arch`: `arch_layer_skipped(d, layer, plotting)` (true when the layer is
//! off, frozen, or no-plot while plotting) and `push_polyline(list, handle, ink, pts)`. Text is
//! drawn as strokes, as room labels are: `cadcraft_fonts::place(&TextFont::Stroke, text,
//! &TextParams::centered(at, height, angle))` and one polyline per stroke.

use cadcraft_arch::terrain::contour::{label_point, label_text, site_contours};
use cadcraft_arch::terrain::mesh::site_mesh;
use cadcraft_arch::terrain::surface::SiteSurface;
use cadcraft_color::Color;
use cadcraft_doc::arch::{ModifierKind, plant_handle, terrain_handle};
use cadcraft_doc::{Drawing, Handle};
use cadcraft_fonts::{TextFont, TextParams, place};
use cadcraft_geom::Vec2;
use cadcraft_geom::tin_ops::ContourLine;

use crate::DisplayList;
use crate::arch::{arch_layer_skipped, push_polyline};

/// Height of a major contour's label, in metres.
const LABEL_HEIGHT: f64 = 0.35;
/// Height of the north arrow's "N", in metres.
const NORTH_HEIGHT: f64 = 0.5;
/// How far along the north direction the "N" sits from the arrow's foot.
const NORTH_LABEL_OFFSET: f64 = 2.4;
/// Points on a hill's circle (one more closes it).
const HILL_CIRCLE_POINTS: usize = 48;

/// Append the site's plan linework to `list`, each prim tagged with its proxy handle.
pub(crate) fn push_site(d: &Drawing, list: &mut DisplayList, plotting: bool) {
    let m = &d.arch;
    if m.site.is_none() && m.terrain.is_none() && m.plants.is_empty() {
        return;
    }
    if let Some(t) = &m.terrain {
        let ground = terrain_handle(None);
        if let Some(ink) = layer_ink(d, "C-TOPO-BNDY", 3, plotting) {
            push_ring(list, ground, &ink, &t.boundary);
        }
        push_contours(d, list, plotting);
        if let Some(ink) = layer_ink(d, "C-TOPO-GRAD", 4, plotting) {
            for md in &t.modifiers {
                let handle = terrain_handle(Some(md.id));
                match &md.kind {
                    ModifierKind::Flat { polygon, margin, .. } => {
                        push_ring(list, handle, &ink, polygon);
                        if margin.is_finite() && *margin > 0.0 && polygon.iter().all(|p| p.is_finite()) {
                            let dist = vec![*margin; polygon.len()];
                            if let Some(ring) = cadcraft_arch::poly::offset_polygon(polygon, &dist, true) {
                                push_ring(list, handle, &ink, &ring);
                            }
                        }
                    }
                    ModifierKind::Hill { center, radius, .. } => {
                        if center.is_finite() && radius.is_finite() && *radius > 0.0 {
                            let circle: Vec<Vec2> = (0..=HILL_CIRCLE_POINTS)
                                .map(|i| *center + Vec2::from_angle(i as f64 * std::f64::consts::TAU / HILL_CIRCLE_POINTS as f64) * *radius)
                                .collect();
                            push_polyline(list, handle, &ink, &circle);
                        }
                    }
                    ModifierKind::Path { .. } => {}
                }
            }
        }
        if let Some(ink) = layer_ink(d, "C-ROAD", 9, plotting) {
            for md in &t.modifiers {
                if let ModifierKind::Path { polygon, .. } = &md.kind {
                    push_ring(list, terrain_handle(Some(md.id)), &ink, polygon);
                }
            }
        }
    }
    if let Some(ink) = layer_ink(d, "L-PLNT", 3, plotting) {
        for pl in &m.plants {
            if !pl.position.is_finite() {
                continue;
            }
            let Some(sp) = cadcraft_arch::plant::find_species(m, &pl.species) else { continue };
            let (_, spread) = cadcraft_arch::plant::plant_size(&sp, pl);
            for sym in cadcraft_arch::plant::plan_symbol(&sp, pl.position, spread) {
                if sym.len() >= 2 && sym.iter().all(|p| p.is_finite()) {
                    push_polyline(list, plant_handle(pl.id), &ink, &sym);
                }
            }
        }
    }
    if let Some(ink) = layer_ink(d, "A-ANNO-NRTH", 7, plotting) {
        if let Some(arrow) = cadcraft_arch::site::north_arrow(m) {
            let ground = terrain_handle(None);
            if arrow.outline.len() >= 2 && arrow.outline.iter().all(|p| p.is_finite()) {
                push_polyline(list, ground, &ink, &arrow.outline);
            }
            let at = arrow.at + arrow.direction * NORTH_LABEL_OFFSET;
            push_text(list, ground, &ink, "N", at, NORTH_HEIGHT, 0.0);
        }
    }
}

/// The contours: minor lines that are not on a major level on C-TOPO-MINR, major lines on
/// C-TOPO-MAJR with their labels on C-TOPO-TEXT. Nothing when the surface or its mesh fails.
fn push_contours(d: &Drawing, list: &mut DisplayList, plotting: bool) {
    let Some(t) = &d.arch.terrain else { return };
    let minor_ink = layer_ink(d, "C-TOPO-MINR", 8, plotting);
    let major_ink = layer_ink(d, "C-TOPO-MAJR", 7, plotting);
    let text_ink = layer_ink(d, "C-TOPO-TEXT", 7, plotting);
    if minor_ink.is_none() && major_ink.is_none() && text_ink.is_none() {
        return;
    }
    let Ok(surf) = SiteSurface::new(&d.arch) else { return };
    let Ok(mesh) = site_mesh(&surf) else { return };
    let set = site_contours(&mesh, t.contour_minor, t.contour_major);
    let ground = terrain_handle(None);
    if let Some(ink) = &minor_ink {
        for line in set.minor.iter().filter(|l| !is_major_level(l.elevation, t.contour_major)) {
            push_line(list, ground, ink, line);
        }
    }
    if let Some(ink) = &major_ink {
        for line in &set.major {
            push_line(list, ground, ink, line);
        }
    }
    if let Some(ink) = &text_ink {
        for line in &set.major {
            let Some(at) = label_point(line) else { continue };
            let Some(dir) = direction_at(line, at) else { continue };
            push_text(list, ground, ink, &label_text(line.elevation), at, LABEL_HEIGHT, readable_angle(dir));
        }
    }
}

/// The ink for linework on `layer` (its colour, else ACI `c`), or `None` when the layer is skipped.
fn layer_ink(d: &Drawing, layer: &str, c: u8, plotting: bool) -> Option<crate::Ink> {
    if arch_layer_skipped(d, layer, plotting) {
        return None;
    }
    Some(crate::ink(Color::ByLayer, d.layer(layer).map(|l| l.color).unwrap_or(Color::Index(c)), Color::Index(c)))
}

/// Whether `level` is a whole multiple of `major` (within 1e-9), as the contour set decides it.
fn is_major_level(level: f64, major: f64) -> bool {
    if !level.is_finite() || !major.is_finite() || major <= 0.0 {
        return false;
    }
    let v = level / major;
    v.is_finite() && (v - v.round()).abs() <= 1e-9
}

/// A closed ring (first point not repeated) as one polyline with the closing point appended.
/// Rings with fewer than three points or any non-finite point are skipped.
fn push_ring(list: &mut DisplayList, handle: Handle, ink: &crate::Ink, ring: &[Vec2]) {
    if ring.len() < 3 || !ring.iter().all(|p| p.is_finite()) {
        return;
    }
    let mut pts = ring.to_vec();
    pts.push(ring[0]);
    push_polyline(list, handle, ink, &pts);
}

/// A contour line as one polyline, the closing point appended when it is closed.
fn push_line(list: &mut DisplayList, handle: Handle, ink: &crate::Ink, line: &ContourLine) {
    let pts: Vec<Vec2> = line.points.iter().copied().filter(|p| p.is_finite()).collect();
    if pts.len() < 2 {
        return;
    }
    if line.closed && pts.len() >= 3 {
        let mut ring = pts.clone();
        ring.push(pts[0]);
        push_polyline(list, handle, ink, &ring);
    } else {
        push_polyline(list, handle, ink, &pts);
    }
}

/// The unit direction of the segment of `line` nearest to `at` (the label point lies on it).
fn direction_at(line: &ContourLine, at: Vec2) -> Option<Vec2> {
    let pts = &line.points;
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let segs = if line.closed { n } else { n - 1 };
    let mut best: Option<(f64, Vec2)> = None;
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let ab = b - a;
        let len = ab.len();
        if !len.is_finite() || len <= 1e-12 {
            continue;
        }
        let t = ((at - a).dot(ab) / (len * len)).clamp(0.0, 1.0);
        let dist = at.dist(a + ab * t);
        if dist.is_finite() && best.map_or(true, |(d, _)| dist < d) {
            best = Some((dist, ab * (1.0 / len)));
        }
    }
    best.map(|(_, dir)| dir)
}

/// The text angle along `dir`, turned by π when `dir` points left (or straight down) so the
/// label reads left to right.
fn readable_angle(dir: Vec2) -> f64 {
    let flip = dir.x < -1e-12 || (dir.x.abs() <= 1e-12 && dir.y < 0.0);
    let r = if flip { dir * -1.0 } else { dir };
    r.y.atan2(r.x)
}

/// `text` as stroke polylines centred on `at`.
fn push_text(list: &mut DisplayList, handle: Handle, ink: &crate::Ink, text: &str, at: Vec2, height: f64, angle: f64) {
    if !at.is_finite() || !angle.is_finite() {
        return;
    }
    let (shape, _) = place(&TextFont::Stroke, text, &TextParams::centered(at, height, angle));
    for stroke in &shape.strokes {
        if stroke.len() >= 2 && stroke.iter().all(|p| p.is_finite()) {
            push_polyline(list, handle, ink, stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DPrim, Options};
    use cadcraft_color::Rgb;
    use cadcraft_doc::arch::{ArchId, PathKind, Plant, SiteLocation, Terrain, TerrainModifier};
    use cadcraft_geom::Vec3;

    /// The site layers, each given its own colour so a prim's colour says which layer drew it.
    const LAYERS: [(&str, u8); 8] = [
        ("C-TOPO-BNDY", 1),
        ("C-TOPO-MINR", 2),
        ("C-TOPO-MAJR", 3),
        ("C-TOPO-TEXT", 4),
        ("C-TOPO-GRAD", 5),
        ("C-ROAD", 6),
        ("L-PLNT", 30),
        ("A-ANNO-NRTH", 40),
    ];

    fn rgb(layer: &str) -> Rgb {
        let c = LAYERS.iter().find(|(l, _)| *l == layer).map_or(7, |(_, c)| *c);
        cadcraft_color::aci_rgb(c)
    }

    /// A 40 × 30 m site rising 2 m from west to east (z = x / 20): minor contours every 0.25 m
    /// are the vertical lines x = 5, 10, …, 35; the 1.00 major is x = 20.
    fn sloped() -> Terrain {
        Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)],
            points: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)],
            pad: false,
            ..Default::default()
        }
    }

    fn drawing(t: Option<Terrain>) -> Drawing {
        let mut d = Drawing::new_metric();
        for (name, c) in LAYERS {
            d.ensure_layer(name);
            if let Some(l) = d.layer_mut(name) {
                l.color = Color::Index(c);
            }
        }
        d.arch.terrain = t;
        d.touch_arch();
        d
    }

    fn build(d: &Drawing, plotting: bool) -> DisplayList {
        crate::arch::build_arch(d, &Options::default(), plotting)
    }

    fn on<'a>(list: &'a DisplayList, layer: &str) -> Vec<&'a DPrim> {
        list.prims.iter().filter(|p| p.color == rgb(layer)).collect()
    }

    /// The (min, max) corners of the prims' points.
    fn bbox(list: &DisplayList, prims: &[&DPrim]) -> (Vec2, Vec2) {
        let mut lo = Vec2::new(f64::INFINITY, f64::INFINITY);
        let mut hi = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in prims {
            for q in list.points(p) {
                lo = Vec2::new(lo.x.min(q.x), lo.y.min(q.y));
                hi = Vec2::new(hi.x.max(q.x), hi.y.max(q.y));
            }
        }
        (lo, hi)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn site_plan_draws_contours_on_their_layers() {
        let list = build(&drawing(Some(sloped())), false);

        let bndy = on(&list, "C-TOPO-BNDY");
        assert_eq!(bndy.len(), 1, "one boundary");
        let pts = list.points(bndy[0]);
        assert_eq!(pts.len(), 5, "the boundary is closed: {pts:?}");
        assert_eq!(pts.first(), pts.last());
        assert_eq!(bndy[0].handle, terrain_handle(None));

        // Each level is one line across the site at x = 20·level, from y = 0 to y = 30.
        let lines = |layer: &str| -> Vec<f64> {
            let prims = on(&list, layer);
            assert!(prims.iter().all(|p| p.handle == terrain_handle(None)), "{layer} lines carry the terrain handle");
            let mut xs: Vec<f64> = prims
                .iter()
                .map(|p| {
                    let pts = list.points(p);
                    let x = pts[0].x;
                    assert!(pts.iter().all(|q| close(q.x, x)), "{layer}: a level is a straight north-south line: {pts:?}");
                    let (lo, hi) = bbox(&list, &[*p]);
                    assert!(close(lo.y, 0.0) && close(hi.y, 30.0), "{layer}: the line crosses the site: {pts:?}");
                    (x * 1e6).round() / 1e6
                })
                .collect();
            xs.sort_by(f64::total_cmp);
            xs
        };
        assert_eq!(lines("C-TOPO-MINR"), vec![5.0, 10.0, 15.0, 25.0, 30.0, 35.0], "minor levels, the major 1.00 left out");
        assert_eq!(lines("C-TOPO-MAJR"), vec![20.0], "the 1.00 major");

        // Modifiers: the flat region and its margin ring and the hill's circle on C-TOPO-GRAD, the
        // driveway on C-ROAD, each with its modifier's handle.
        let mut t = sloped();
        t.modifiers.push(TerrainModifier {
            id: 0,
            kind: ModifierKind::Flat {
                polygon: vec![Vec2::new(30.0, 10.0), Vec2::new(36.0, 10.0), Vec2::new(36.0, 20.0), Vec2::new(30.0, 20.0)],
                elevation: 1.5,
                margin: 1.0,
            },
        });
        t.modifiers.push(TerrainModifier { id: 1, kind: ModifierKind::Hill { center: Vec2::new(8.0, 22.0), height: 1.0, radius: 3.0 } });
        t.modifiers.push(TerrainModifier {
            id: 2,
            kind: ModifierKind::Path {
                kind: PathKind::Driveway,
                polygon: vec![Vec2::new(2.0, 2.0), Vec2::new(12.0, 2.0), Vec2::new(12.0, 5.0), Vec2::new(2.0, 5.0)],
                thickness: 0.1,
                material: "concrete".into(),
            },
        });
        let list = build(&drawing(Some(t)), false);
        let grad = on(&list, "C-TOPO-GRAD");
        let flat: Vec<&DPrim> = grad.iter().copied().filter(|p| p.handle == terrain_handle(Some(0))).collect();
        assert_eq!(flat.len(), 2, "the flat polygon and its margin ring");
        let (lo, hi) = bbox(&list, &flat[..1]);
        assert!(close(lo.x, 30.0) && close(hi.x, 36.0) && close(lo.y, 10.0) && close(hi.y, 20.0), "the flat polygon: {lo:?} {hi:?}");
        let (lo, hi) = bbox(&list, &flat[1..]);
        assert!(close(lo.x, 29.0) && close(hi.x, 37.0) && close(lo.y, 9.0) && close(hi.y, 21.0), "margin ring 1 m out: {lo:?} {hi:?}");
        let hill: Vec<&DPrim> = grad.iter().copied().filter(|p| p.handle == terrain_handle(Some(1))).collect();
        assert_eq!(hill.len(), 1);
        let hp = list.points(hill[0]);
        assert_eq!(hp.len(), HILL_CIRCLE_POINTS + 1);
        assert!(hp.iter().all(|q| close(q.dist(Vec2::new(8.0, 22.0)), 3.0)), "the hill's circle at its radius");
        assert_eq!(grad.len(), 3, "the path is not drawn on C-TOPO-GRAD");
        let road = on(&list, "C-ROAD");
        assert_eq!(road.len(), 1);
        assert_eq!(road[0].handle, terrain_handle(Some(2)));
        assert_eq!(list.points(road[0]).len(), 5, "the path polygon is closed");
    }

    #[test]
    fn site_plan_labels_major_contours() {
        let d = drawing(Some(sloped()));
        let list = build(&d, false);
        let text = on(&list, "C-TOPO-TEXT");
        assert!(!text.is_empty(), "the 1.00 major is labelled");
        assert!(text.iter().all(|p| p.handle == terrain_handle(None)));
        let (lo, hi) = bbox(&list, &text);
        let mid = (lo + hi) * 0.5;
        assert!(mid.dist(Vec2::new(20.0, 15.0)) < 0.3, "the label sits at the line's halfway point: {lo:?} {hi:?}");
        // The line runs north-south, so the text runs along y; 0.35 m high across it.
        assert!(hi.y - lo.y > hi.x - lo.x, "the label follows the line: {lo:?} {hi:?}");
        assert!(hi.x - lo.x <= LABEL_HEIGHT * 1.5, "label height 0.35: {lo:?} {hi:?}");

        // Exactly the strokes of "1.00" at that place and angle.
        let (shape, _) =
            place(&TextFont::Stroke, "1.00", &TextParams::centered(Vec2::new(20.0, 15.0), LABEL_HEIGHT, readable_angle(Vec2::new(0.0, -1.0))));
        assert_eq!(text.len(), shape.strokes.iter().filter(|s| s.len() >= 2).count());

        // Readable: a line pointing left or down reads left to right / bottom to top.
        assert!(close(readable_angle(Vec2::new(-1.0, 0.0)), 0.0));
        assert!(close(readable_angle(Vec2::new(0.0, -1.0)), std::f64::consts::FRAC_PI_2));
        assert!(close(readable_angle(Vec2::new(1.0, 1.0).normalized()), std::f64::consts::FRAC_PI_4));
        assert!(close(readable_angle(Vec2::new(-1.0, -1.0).normalized()), std::f64::consts::FRAC_PI_4));
    }

    #[test]
    fn site_plan_draws_plants_and_north_arrow() {
        let mut d = drawing(Some(sloped()));
        d.arch.site = Some(SiteLocation { true_north: 30.0, ..SiteLocation::default() });
        d.arch.plants.push(Plant { id: ArchId(4), species: "Sugar Maple".into(), position: Vec2::new(8.0, 22.0), age: 10.0, ..Default::default() });
        d.arch.plants.push(Plant { id: ArchId(5), species: "No Such Tree".into(), position: Vec2::new(1.0, 1.0), ..Default::default() });
        d.touch_arch();
        let list = build(&d, false);

        let plants = on(&list, "L-PLNT");
        assert!(plants.iter().all(|p| p.handle == plant_handle(ArchId(4))), "an unknown species draws nothing");
        let sp = cadcraft_arch::plant::find_species(&d.arch, "Sugar Maple").expect("catalogue species");
        let (_, spread) = cadcraft_arch::plant::plant_size(&sp, &d.arch.plants[0]);
        let want = cadcraft_arch::plant::plan_symbol(&sp, Vec2::new(8.0, 22.0), spread);
        assert_eq!(plants.len(), want.len(), "every polyline of the plan symbol");
        assert_eq!(list.points(plants[0]), want[0].as_slice(), "the first is the circle at the spread");

        let north = on(&list, "A-ANNO-NRTH");
        let arrow = cadcraft_arch::site::north_arrow(&d.arch).expect("a site has a north arrow");
        assert!(north.iter().all(|p| p.handle == terrain_handle(None)));
        assert_eq!(list.points(north[0]), arrow.outline.as_slice(), "the arrow's outline");
        let (lo, hi) = bbox(&list, &north[1..]);
        let n_at = arrow.at + arrow.direction * NORTH_LABEL_OFFSET;
        assert!(((lo + hi) * 0.5).dist(n_at) < 0.3, "the N at 2.4 m along north: {lo:?} {hi:?} vs {n_at:?}");
        assert!(hi.y - lo.y <= NORTH_HEIGHT * 1.2 && hi.y - lo.y >= NORTH_HEIGHT * 0.5, "N is 0.5 high: {lo:?} {hi:?}");
    }

    #[test]
    fn site_plan_respects_layer_state() {
        let mut d = drawing(Some(sloped()));
        d.arch.plants.push(Plant { id: ArchId(0), species: "Sugar Maple".into(), position: Vec2::new(8.0, 22.0), age: 10.0, ..Default::default() });
        d.touch_arch();
        if let Some(l) = d.layer_mut("C-TOPO-MINR") {
            l.on = false;
        }
        if let Some(l) = d.layer_mut("L-PLNT") {
            l.frozen = true;
        }
        if let Some(l) = d.layer_mut("A-ANNO-NRTH") {
            l.plot = false;
        }
        let screen = build(&d, false);
        assert!(on(&screen, "C-TOPO-MINR").is_empty(), "an off layer draws nothing");
        assert!(on(&screen, "L-PLNT").is_empty(), "a frozen layer draws nothing");
        assert!(!on(&screen, "C-TOPO-MAJR").is_empty() && !on(&screen, "C-TOPO-TEXT").is_empty(), "the other contour layers still draw");
        assert!(!on(&screen, "A-ANNO-NRTH").is_empty(), "a no-plot layer draws on screen");
        let plot = build(&d, true);
        assert!(on(&plot, "A-ANNO-NRTH").is_empty(), "and not when plotting");
        assert!(!on(&plot, "C-TOPO-BNDY").is_empty());

        // A missing layer draws in its default colour (the boundary's is 3).
        let mut bare = Drawing::new_metric();
        bare.arch.terrain = Some(sloped());
        bare.touch_arch();
        let list = build(&bare, false);
        let bndy: Vec<&DPrim> = list.prims.iter().filter(|p| list.points(p).len() == 5 && p.handle == terrain_handle(None)).collect();
        assert!(bndy.iter().any(|p| p.color == cadcraft_color::aci_rgb(3)), "the boundary in colour 3 with no layer");
    }

    #[test]
    fn site_plan_draws_nothing_without_a_site() {
        let d = drawing(None);
        let list = build(&d, false);
        assert!(list.prims.is_empty() && list.bounds.is_empty());

        // Hostile data never panics: non-finite points and plants are skipped.
        let mut t = sloped();
        t.boundary.push(Vec2::new(f64::NAN, 3.0));
        t.points.push(Vec3::new(f64::INFINITY, 1.0, 1.0));
        t.contour_minor = f64::NAN;
        t.modifiers.push(TerrainModifier { id: 0, kind: ModifierKind::Hill { center: Vec2::new(f64::NAN, 0.0), height: 1.0, radius: 3.0 } });
        t.modifiers.push(TerrainModifier {
            id: 1,
            kind: ModifierKind::Flat { polygon: vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0)], elevation: 0.0, margin: f64::INFINITY },
        });
        let mut d = drawing(Some(t));
        d.arch.plants.push(Plant {
            id: ArchId(0),
            species: "Sugar Maple".into(),
            position: Vec2::new(f64::NAN, 1.0),
            age: f64::NAN,
            ..Default::default()
        });
        d.touch_arch();
        let list = build(&d, false);
        assert!(on(&list, "C-TOPO-BNDY").is_empty(), "a boundary with a non-finite point is skipped");
        assert!(on(&list, "L-PLNT").is_empty(), "a plant at a non-finite position is skipped");
        assert!(on(&list, "C-TOPO-GRAD").is_empty());
        assert!(list.verts.iter().all(|p| p.is_finite()));
    }
}
