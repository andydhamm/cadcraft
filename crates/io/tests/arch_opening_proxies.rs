use cadcraft_doc::arch::{Opening, OpeningKind};
use cadcraft_doc::{Drawing, EntityKind, Wall};
use cadcraft_geom::Vec2;
use cadcraft_io::{read_dxf, write_dxf};

/// Counts LWPOLYLINE entities in the parsed drawing that carry `marker` in their
/// CADCRAFT xdata.
fn lwpoly_count_marker(d: &Drawing, marker: &str) -> usize {
    d.model
        .iter()
        .filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_)))
        .filter(|e| e.common.xdata.iter().any(|t| t.code == 1000 && t.value.trim() == marker))
        .count()
}

/// Scans raw DXF text for LWPOLYLINE sections and returns a list of (layer, vertex_count,
/// has_marker) for each that carries the ARCHOPENING marker. Group code 8 is the layer;
/// group code 10 is an x coordinate; `1000 ARCHOPENING` is the xdata marker.
fn lwpoly_archopenings_in_text(text: &str) -> Vec<(String, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i].trim() == "0" && lines[i + 1].trim() == "LWPOLYLINE" {
            let mut j = i + 2;
            let mut layer = String::from("");
            let mut vertex_count = 0;
            let mut has_archopening = false;
            // DXF tags are code/value pairs. Advancing one line at a time would
            // mistake a coordinate value of 0 for the next entity's group code.
            while j + 1 < lines.len() {
                if lines[j].trim() == "0" {
                    break; // next entity
                }
                match (lines[j].trim(), lines[j + 1].trim()) {
                    ("8", value) => layer = value.to_string(),
                    ("10", _) => vertex_count += 1,
                    ("1000", "ARCHOPENING") => has_archopening = true,
                    _ => {}
                }
                j += 2;
            }
            if has_archopening {
                out.push((layer, vertex_count));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// A 10 m wall with a hinged door and a slider window produces at least 4 A-DOOR and
/// at least 5 A-GLAZ LWPOLYLINEs, all carrying the ARCHOPENING marker.
#[test]
fn opening_proxies_carry_the_whole_symbol() {
    let mut d = Drawing::new_metric();
    d.ensure_layer("A-DOOR");
    d.ensure_layer("A-GLAZ");

    let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    d.arch.push_opening(Opening::new(wid, 2.5, 0.9, 2.1, OpeningKind::Door));
    d.arch.push_opening(Opening::new(wid, 6.5, 1.5, 1.2, OpeningKind::Window));

    let text = write_dxf(&d);
    let arch = lwpoly_archopenings_in_text(&text);

    let door = arch.iter().filter(|(l, _)| l.as_str() == "A-DOOR").count();
    let glaz = arch.iter().filter(|(l, _)| l.as_str() == "A-GLAZ").count();
    assert!(door >= 4, "expected >= 4 A-DOOR LWPOLYLINEs with ARCHOPENING, got {door}",);
    assert!(glaz >= 5, "expected >= 5 A-GLAZ LWPOLYLINEs with ARCHOPENING, got {glaz}",);
}

/// With the CADCRAFT_ARCH record present, proxies are dropped on read and the re-save
/// writes the same number of proxies (none duplicated, none kept as plain linework).
#[test]
fn opening_proxies_are_dropped_on_read_with_the_record() {
    let mut d = Drawing::new_metric();
    d.ensure_layer("A-DOOR");
    d.ensure_layer("A-GLAZ");

    let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    d.arch.push_opening(Opening::new(wid, 2.5, 0.9, 2.1, OpeningKind::Door));
    d.arch.push_opening(Opening::new(wid, 6.5, 1.5, 1.2, OpeningKind::Window));

    let first_text = write_dxf(&d);
    let first = read_dxf(first_text.as_bytes()).unwrap();

    // No ARCHOPENING-tagged LWPOLYLINEs survive the read while the record is present.
    assert_eq!(lwpoly_count_marker(&first, "ARCHOPENING"), 0, "ARCHOPENING-tagged LWPOLYLINEs survived the first read",);

    let second_text = write_dxf(&first);
    let second = read_dxf(second_text.as_bytes()).unwrap();

    assert_eq!(lwpoly_count_marker(&second, "ARCHOPENING"), 0, "ARCHOPENING-tagged LWPOLYLINEs survived the second read",);
    assert_eq!(second.arch.openings.len(), 2, "openings changed on round trip");

    // Same total proxy count on first and second save (none duplicated, none kept).
    let first_total = lwpoly_archopenings_in_text(&first_text).len();
    let second_total = lwpoly_archopenings_in_text(&second_text).len();
    assert_eq!(first_total, second_total, "proxy count changed: {first_total} -> {second_total}",);
}

/// Renaming the dictionary key to STRIPPED_ARCH makes the reader keep every proxy as
/// plain LWPOLYLINEs; the door's swing arc is among them with >= 8 vertices.
#[test]
fn a_stripped_drawing_keeps_the_door_swing_as_linework() {
    let mut d = Drawing::new_metric();
    d.ensure_layer("A-DOOR");
    d.ensure_layer("A-GLAZ");

    let wid = d.arch.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    d.arch.push_opening(Opening::new(wid, 2.5, 0.9, 2.1, OpeningKind::Door));
    d.arch.push_opening(Opening::new(wid, 6.5, 1.5, 1.2, OpeningKind::Window));

    // Rename the CADCRAFT_ARCH key so the reader does not find an arch record.
    let text = write_dxf(&d).replace("CADCRAFT_ARCH", "STRIPPED_ARCH");
    let stripped = read_dxf(text.as_bytes()).unwrap();

    // Without the record, no openings in the model.
    assert_eq!(stripped.arch.openings.len(), 0, "openings should be empty when the arch record is stripped",);

    // Every proxy LWPOLYLINE (door + window) survived as plain linework. Count
    // the entities themselves rather than requiring proxy xdata as metadata.
    let arch = lwpoly_archopenings_in_text(&text);
    let surviving = stripped.model.iter().filter(|e| matches!(&e.kind, EntityKind::LwPolyline(_))).count();
    assert!(surviving >= arch.len(), "expected at least {} opening LWPOLYLINEs to survive stripping, got {surviving}", arch.len());

    // The door's swing arc is among the A-DOOR LWPOLYLINEs and has at least 8 vertices.
    let max_vertices: usize = arch.iter().filter(|(layer, _)| layer.as_str() == "A-DOOR").map(|(_, n)| *n).max().unwrap_or(0);
    assert!(max_vertices >= 8, "door swing arc (>= 8 vertices) missing: max A-DOOR LWPOLYLINE vertices = {max_vertices}",);
}
