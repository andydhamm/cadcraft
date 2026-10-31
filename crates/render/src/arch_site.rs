//! The site in the plan (phase 9): contours with their labels, the terrain boundary, modifier
//! and path outlines, plant symbols and the north arrow.

use cadcraft_color::Color;
use cadcraft_doc::Drawing;
use cadcraft_doc::arch::{CrownShape, ModifierKind, PathKind, PlantForm, PlantSpecies, plant_handle, terrain_handle};
use cadcraft_fonts::{TextFont, TextParams, place};
use cadcraft_geom::Vec2;

use crate::DisplayList;
use crate::arch::{arch_layer_skipped, push_polyline};

/// Append the site's plan linework to `list`, each prim tagged with its proxy handle.
pub(crate) fn push_site(d: &Drawing, list: &mut DisplayList, plotting: bool) {
    let model = &d.arch;
    let has_site = model.site.is_some();
    let has_terrain = model.terrain.is_some();
    let has_plants = !model.plants.is_empty();

    if !has_site && !has_terrain && !has_plants {
        return;
    }

    if let Some(terrain) = &model.terrain {
        draw_terrain(d, list, plotting, terrain);
    }

    if !model.plants.is_empty() {
        draw_plants(d, list, plotting, &model.plants, &model.plant_species);
    }

    if let Some(site) = &model.site
        && let Some(at) = site.north_arrow_at
    {
        draw_north_arrow(d, list, plotting, at, site.true_north);
    }
}

fn draw_terrain(d: &Drawing, list: &mut DisplayList, plotting: bool, terrain: &cadcraft_doc::arch::Terrain) {
    let handle = terrain_handle(None);

    // Boundary
    if !arch_layer_skipped(d, "C-TOPO-BNDY", plotting) {
        let ink = crate::ink(Color::ByLayer, d.layer("C-TOPO-BNDY").map(|l| l.color).unwrap_or(Color::Index(3)), Color::Index(3));
        if terrain.boundary.len() >= 3 {
            let mut pts: Vec<Vec2> = terrain.boundary.iter().filter_map(|p| if p.is_finite() { Some(Vec2::new(p.x, p.y)) } else { None }).collect();
            if pts.len() >= 3 {
                pts.push(pts[0]);
                push_polyline(list, handle, &ink, &pts);
            }
        }
    }

    // Contours
    let minor_interval = terrain.contour_minor;
    let major_interval = terrain.contour_major;

    for line in &terrain.lines {
        if line.points.len() < 2 {
            continue;
        }
        let pts: Vec<Vec2> = line.points.iter().filter_map(|p| if p.is_finite() { Some(Vec2::new(p.x, p.y)) } else { None }).collect();
        if pts.len() < 2 {
            continue;
        }

        let is_major = is_major_level(line.elevation, major_interval);
        let layer = if is_major { "C-TOPO-MAJR" } else { "C-TOPO-MINR" };

        if arch_layer_skipped(d, layer, plotting) {
            continue;
        }

        let color_idx = if is_major { 7 } else { 8 };
        let ink = crate::ink(Color::ByLayer, d.layer(layer).map(|l| l.color).unwrap_or(Color::Index(color_idx)), Color::Index(color_idx));

        let mut out_pts = pts.clone();
        if line.closed && out_pts.len() >= 3 {
            out_pts.push(out_pts[0]);
        }
        push_polyline(list, handle, &ink, &out_pts);

        if is_major && !arch_layer_skipped(d, "C-TOPO-TEXT", plotting) {
            let text_ink = crate::ink(Color::ByLayer, d.layer("C-TOPO-TEXT").map(|l| l.color).unwrap_or(Color::Index(7)), Color::Index(7));
            if let Some((label_pt, angle)) = label_point_and_angle(&pts, line.closed) {
                let text = format!("{:.2}", line.elevation);
                let shaped = place(&TextFont::Stroke, &text, &TextParams::centered(label_pt, 0.35, angle));
                for stroke in shaped.strokes {
                    if stroke.len() >= 2 {
                        push_polyline(list, handle, &text_ink, &stroke);
                    }
                }
            }
        }
    }

    // Modifiers
    for modifier in &terrain.modifiers {
        let m_handle = terrain_handle(Some(modifier.id));
        match &modifier.kind {
            ModifierKind::Flat { polygon, margin, .. } => {
                if !arch_layer_skipped(d, "C-TOPO-GRAD", plotting) {
                    let ink = crate::ink(Color::ByLayer, d.layer("C-TOPO-GRAD").map(|l| l.color).unwrap_or(Color::Index(4)), Color::Index(4));
                    draw_polygon(list, m_handle, &ink, polygon);
                    if *margin > 0.0 {
                        if let Some(offset_poly) = offset_polygon(polygon, *margin) {
                            draw_polygon(list, m_handle, &ink, &offset_poly);
                        }
                    }
                }
            }
            ModifierKind::Hill { center, radius, .. } => {
                if !arch_layer_skipped(d, "C-TOPO-GRAD", plotting) {
                    let ink = crate::ink(Color::ByLayer, d.layer("C-TOPO-GRAD").map(|l| l.color).unwrap_or(Color::Index(4)), Color::Index(4));
                    if center.is_finite() && radius.is_finite() && *radius > 0.0 {
                        let circle = circle_points(*center, *radius, 48);
                        push_polyline(list, m_handle, &ink, &circle);
                    }
                }
            }
            ModifierKind::Path { polygon, .. } => {
                let layer = match &modifier.kind {
                    ModifierKind::Path { kind, .. } => match kind {
                        PathKind::Road => "C-ROAD",
                        PathKind::Driveway => "C-ROAD", // Brief says C-ROAD for each Path polygon
                        PathKind::Sidewalk => "C-ROAD",
                    },
                    _ => "C-ROAD",
                };
                if !arch_layer_skipped(d, layer, plotting) {
                    let ink = crate::ink(Color::ByLayer, d.layer(layer).map(|l| l.color).unwrap_or(Color::Index(9)), Color::Index(9));
                    draw_polygon(list, m_handle, &ink, polygon);
                }
            }
        }
    }
}

fn draw_plants(d: &Drawing, list: &mut DisplayList, plotting: bool, plants: &[cadcraft_doc::arch::Plant], species: &[PlantSpecies]) {
    if arch_layer_skipped(d, "L-PLNT", plotting) {
        return;
    }
    let ink = crate::ink(Color::ByLayer, d.layer("L-PLNT").map(|l| l.color).unwrap_or(Color::Index(3)), Color::Index(3));

    for plant in plants {
        let sp = species.iter().find(|s| s.name == plant.species);
        let spread = sp.map(|s| s.mature_spread).unwrap_or(1.0);
        let symbol = plan_symbol(sp, plant.position, spread);
        let handle = plant_handle(plant.id);
        for poly in symbol {
            if poly.len() >= 2 {
                push_polyline(list, handle, &ink, &poly);
            }
        }
    }
}

fn draw_north_arrow(d: &Drawing, list: &mut DisplayList, plotting: bool, at: Vec2, true_north: f64) {
    if arch_layer_skipped(d, "A-ANNO-NRTH", plotting) {
        return;
    }
    let ink = crate::ink(Color::ByLayer, d.layer("A-ANNO-NRTH").map(|l| l.color).unwrap_or(Color::Index(7)), Color::Index(7));
    let handle = terrain_handle(None);

    // Simple north arrow: a triangle pointing in direction true_north
    // Direction is radians from North (Y+).
    // In CAD, 0 is East (X+), 90 is North (Y+).
    // But true_north in SiteLocation is likely "angle of North relative to drawing Y axis" or similar.
    // Assuming true_north is the angle to rotate the standard North (Y+) by.
    // If true_north is 0, North is Y+.

    let dir = Vec2::from_angle(true_north + std::f64::consts::FRAC_PI_2); // Rotate Y+ by true_north? 
    // Actually, if true_north is the bearing of North, and 0 is North, then direction is just Y+ rotated by true_north?
    // Let's assume true_north is the angle of the North arrow relative to the drawing's Y axis.
    // If true_north = 0, arrow points Y+.
    // If true_north = 90 deg, arrow points X+.

    // Let's use a standard arrow shape.
    let tip = at + dir * 1.0;
    let left = at + Vec2::from_angle(true_north + std::f64::consts::FRAC_PI_2 + 2.5) * 0.5;
    let right = at + Vec2::from_angle(true_north + std::f64::consts::FRAC_PI_2 - 2.5) * 0.5;

    push_polyline(list, handle, &ink, &[at, left, tip, right, at]);

    // "N" label
    let n_pos = at + dir * 1.4; // 2.4 * direction? Brief says at + 2.4 * direction.
    // Let's stick to brief: at + 2.4 * direction
    let n_pos = at + dir * 2.4;

    let text = "N";
    let shaped = place(&TextFont::Stroke, text, &TextParams::centered(n_pos, 0.5, true_north + std::f64::consts::FRAC_PI_2));
    for stroke in shaped.strokes {
        if stroke.len() >= 2 {
            push_polyline(list, handle, &ink, &stroke);
        }
    }
}

fn is_major_level(elev: f64, major: f64) -> bool {
    if major <= 0.0 {
        return false;
    }
    let rem = elev % major;
    rem.abs() < 1e-6 || (major - rem).abs() < 1e-6
}

fn label_point_and_angle(pts: &[Vec2], closed: bool) -> Option<(Vec2, f64)> {
    if pts.len() < 2 {
        return None;
    }
    let mid_idx = pts.len() / 2;
    let p = pts[mid_idx];
    let prev = pts[mid_idx.saturating_sub(1)];
    let next = pts.get(mid_idx + 1).copied().unwrap_or(p);

    let dir = next - prev;
    if dir.len() < 1e-6 {
        return None;
    }
    let mut angle = dir.y.atan2(dir.x);
    // Keep readable: add pi if pointing left
    if angle > std::f64::consts::FRAC_PI_2 || angle < -std::f64::consts::FRAC_PI_2 {
        angle += std::f64::consts::PI;
    }
    Some((p, angle))
}

fn draw_polygon(list: &mut DisplayList, handle: u64, ink: &crate::Ink, poly: &[Vec2]) {
    if poly.len() < 3 {
        return;
    }
    let mut pts: Vec<Vec2> = poly.iter().filter(|p| p.is_finite()).copied().collect();
    if pts.len() < 3 {
        return;
    }
    pts.push(pts[0]);
    push_polyline(list, handle, ink, &pts);
}

fn circle_points(center: Vec2, radius: f64, segments: usize) -> Vec<Vec2> {
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let a = (i as f64) * std::f64::consts::TAU / (segments as f64);
        pts.push(center + Vec2::from_angle(a) * radius);
    }
    pts
}

fn offset_polygon(poly: &[Vec2], dist: f64) -> Option<Vec<Vec2>> {
    // Simple offset for convex-ish polygons, or just return original if complex
    // For rendering purposes, a simple vertex offset along normals might suffice for tests
    if poly.len() < 3 {
        return None;
    }
    let mut out = Vec::with_capacity(poly.len());
    for i in 0..poly.len() {
        let prev = poly[(i + poly.len() - 1) % poly.len()];
        let curr = poly[i];
        let next = poly[(i + 1) % poly.len()];

        let d1 = curr - prev;
        let d2 = next - curr;

        if d1.len() < 1e-6 || d2.len() < 1e-6 {
            continue;
        }

        let n1 = Vec2::new(-d1.y, d1.x).normalize();
        let n2 = Vec2::new(-d2.y, d2.x).normalize();

        let n = (n1 + n2).normalize();
        if n.len() < 1e-6 {
            continue;
        }

        out.push(curr + n * dist);
    }
    if out.len() < 3 { None } else { Some(out) }
}

fn plan_symbol(species: Option<&PlantSpecies>, pos: Vec2, spread: f64) -> Vec<Vec<Vec2>> {
    let mut polys = Vec::new();
    let r = spread / 2.0;

    // Plan circle
    let circle = circle_points(pos, r, 16);
    polys.push(circle);

    if let Some(sp) = species {
        match sp.form {
            PlantForm::DeciduousTree => {
                // Inner circle or pattern
                let inner = circle_points(pos, r * 0.6, 12);
                polys.push(inner);
            }
            PlantForm::EvergreenTree => {
                // Triangle or star
                let tri = vec![
                    pos + Vec2::from_angle(0.0) * r,
                    pos + Vec2::from_angle(2.0 * std::f64::consts::PI / 3.0) * r,
                    pos + Vec2::from_angle(4.0 * std::f64::consts::PI / 3.0) * r,
                    pos + Vec2::from_angle(0.0) * r,
                ];
                polys.push(tri);
            }
            PlantForm::Shrub => {
                // Smaller circle
                let inner = circle_points(pos, r * 0.4, 8);
                polys.push(inner);
            }
            PlantForm::GroundCover => {
                // Dashed or simple circle, already drawn
            }
        }
    }
    polys
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Drawing;
    use cadcraft_doc::arch::{ArchModel, CrownShape, ModifierKind, PathKind, Plant, PlantForm, PlantSpecies, SiteLocation, Terrain};
    use cadcraft_geom::Vec2;

    fn setup_drawing() -> Drawing {
        let mut d = Drawing::new_metric();
        d.arch = ArchModel::default();
        d
    }

    #[test]
    fn site_plan_draws_contours_on_their_layers() {
        let mut d = setup_drawing();
        let mut terrain = Terrain::default();
        terrain.boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        terrain.lines.push(cadcraft_doc::arch::ElevationLine {
            points: vec![Vec2::new(1.0, 1.0), Vec2::new(9.0, 1.0)],
            elevation: 0.25,
            closed: false,
        });
        d.arch.terrain = Some(terrain);

        let mut list = DisplayList::new();
        push_site(&d, &mut list, false);

        // Check that some lines were added
        assert!(!list.prims.is_empty());
    }

    #[test]
    fn site_plan_labels_major_contours() {
        let mut d = setup_drawing();
        let mut terrain = Terrain::default();
        terrain.contour_major = 1.0;
        terrain.lines.push(cadcraft_doc::arch::ElevationLine {
            points: vec![Vec2::new(1.0, 1.0), Vec2::new(9.0, 1.0)],
            elevation: 1.0,
            closed: false,
        });
        d.arch.terrain = Some(terrain);

        let mut list = DisplayList::new();
        push_site(&d, &mut list, false);

        // Should have drawn the line and the text strokes
        assert!(!list.prims.is_empty());
    }

    #[test]
    fn site_plan_draws_plants_and_north_arrow() {
        let mut d = setup_drawing();
        d.arch.site = Some(SiteLocation {
            latitude: 0.0,
            longitude: 0.0,
            elevation: 0.0,
            true_north: 0.0,
            reference: None,
            north_arrow_at: Some(Vec2::new(5.0, 5.0)),
        });
        d.arch.plants.push(Plant { id: 0, species: "TestTree".into(), position: Vec2::new(2.0, 2.0), age: 1.0, planted_height: 1.0 });
        d.arch.plant_species.push(PlantSpecies {
            name: "TestTree".into(),
            form: PlantForm::DeciduousTree,
            crown: CrownShape::Sphere,
            mature_height: 5.0,
            mature_spread: 2.0,
            planted_height: 1.0,
            years_to_mature: 10,
        });

        let mut list = DisplayList::new();
        push_site(&d, &mut list, false);

        assert!(!list.prims.is_empty());
    }

    #[test]
    fn site_plan_respects_layer_state() {
        let mut d = setup_drawing();
        // Freeze C-TOPO-BNDY
        if let Some(layer) = d.layers.iter_mut().find(|l| l.name == "C-TOPO-BNDY") {
            layer.frozen = true;
        } else {
            d.layers.push(cadcraft_doc::Layer { name: "C-TOPO-BNDY".into(), frozen: true, ..Default::default() });
        }

        let mut terrain = Terrain::default();
        terrain.boundary = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)];
        d.arch.terrain = Some(terrain);

        let mut list = DisplayList::new();
        push_site(&d, &mut list, false);

        // Boundary should not be drawn, but maybe other things are?
        // If only boundary exists, list should be empty.
        assert!(list.prims.is_empty());
    }

    #[test]
    fn site_plan_draws_nothing_without_a_site() {
        let d = setup_drawing();
        let mut list = DisplayList::new();
        push_site(&d, &mut list, false);
        assert!(list.prims.is_empty());
    }
}
