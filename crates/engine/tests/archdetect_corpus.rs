//! Phase 8 corpus tests for `archdetect`.
//!
//! The synthetic plans are written as plain DXF linework and read back before detection,
//! so these tests exercise the same flatten → walls → junctions → openings pipeline as
//! the acceptance checks. Matching is one-to-one and uses the acceptance tolerances.

use std::collections::BTreeSet;
use std::time::Instant;

use cadcraft_arch::detect::{self, CandKind, DEFAULT_MIN_CONFIDENCE, DetectOptions, Detection, Item, Linework, Prim, WallRun};
use cadcraft_doc::{Drawing, Handle};
use cadcraft_geom::Vec2;

const DEFAULT: DetectOptions = DetectOptions { scale: 1.0, layers: None };

#[derive(Clone, Copy)]
struct OpeningTruth {
    kind: CandKind,
    center: Vec2,
    width: f64,
}

struct Plan {
    name: &'static str,
    lw: Linework,
    walls: Vec<WallRun>,
    openings: Vec<OpeningTruth>,
}

#[derive(Clone, Copy)]
struct PlanSpec {
    name: &'static str,
    offset: (f64, f64),
    quarter_turns: i32,
    layerless: bool,
}

fn v(p: (f64, f64)) -> Vec2 {
    Vec2::new(p.0, p.1)
}

fn push_item(items: &mut Vec<Item>, prim: Prim, layer: &str) {
    let source = Handle(items.len() as u64 + 1);
    items.push(Item::new(prim, layer, source, None));
}

fn seg(items: &mut Vec<Item>, a: (f64, f64), b: (f64, f64), layer: &str) {
    push_item(items, Prim::Seg { a: v(a), b: v(b) }, layer);
}

fn arc(items: &mut Vec<Item>, c: (f64, f64), r: f64, a0: f64, a1: f64, layer: &str) {
    let sweep = (a1 - a0).rem_euclid(360.0);
    push_item(items, Prim::Arc { c: v(c), r, start: a0.to_radians(), sweep: sweep.to_radians() }, layer);
}

fn rect(items: &mut Vec<Item>, x0: f64, y0: f64, x1: f64, y1: f64, layer: &str) {
    seg(items, (x0, y0), (x1, y0), layer);
    seg(items, (x1, y0), (x1, y1), layer);
    seg(items, (x1, y1), (x0, y1), layer);
    seg(items, (x0, y1), (x0, y0), layer);
}

fn map_point(p: Vec2, quarter_turns: i32, offset: (f64, f64)) -> Vec2 {
    let q = match quarter_turns.rem_euclid(4) {
        0 => p,
        1 => Vec2::new(-p.y, p.x),
        2 => Vec2::new(-p.x, -p.y),
        _ => Vec2::new(p.y, -p.x),
    };
    Vec2::new(q.x + offset.0, q.y + offset.1)
}

fn transform_item(item: &mut Item, quarter_turns: i32, offset: (f64, f64)) {
    match &mut item.prim {
        Prim::Seg { a, b } => {
            *a = map_point(*a, quarter_turns, offset);
            *b = map_point(*b, quarter_turns, offset);
        }
        Prim::Arc { c, start, .. } => {
            *c = map_point(*c, quarter_turns, offset);
            *start += quarter_turns as f64 * std::f64::consts::FRAC_PI_2;
        }
    }
}

/// The hand-checked apartment geometry: six walls, two doors and five windows.
/// Transformed copies exercise translation, rotation and operation without layer names.
fn synthetic_plan(spec: PlanSpec) -> Plan {
    let wall_layer = if spec.layerless { "0" } else { "A-WALL" };
    let door_layer = if spec.layerless { "0" } else { "A-DOOR" };
    let window_layer = if spec.layerless { "0" } else { "A-GLAZ" };
    let mut items = Vec::new();

    rect(&mut items, 0.0, 0.0, 12.0, 8.0, wall_layer);
    rect(&mut items, 0.2, 0.2, 11.8, 7.8, wall_layer);
    rect(&mut items, 7.0, 0.2, 7.1, 1.5, wall_layer);
    rect(&mut items, 7.0, 2.4, 7.1, 7.8, wall_layer);
    rect(&mut items, 7.1, 5.0, 9.0, 5.1, wall_layer);
    rect(&mut items, 9.8, 5.0, 11.8, 5.1, wall_layer);

    for (x0, y0, x1, y1, a, b) in [
        (1.5, 0.0, 3.5, 0.2, (1.5, 0.1), (3.5, 0.1)),
        (4.2, 0.0, 6.2, 0.2, (4.2, 0.1), (6.2, 0.1)),
        (8.6, 0.0, 10.6, 0.2, (8.6, 0.1), (10.6, 0.1)),
        (0.0, 4.3, 0.2, 6.3, (0.1, 4.3), (0.1, 6.3)),
        (11.8, 5.8, 12.0, 7.2, (11.9, 5.8), (11.9, 7.2)),
    ] {
        rect(&mut items, x0, y0, x1, y1, window_layer);
        seg(&mut items, a, b, window_layer);
    }
    seg(&mut items, (7.1, 1.5), (8.0, 1.5), door_layer);
    arc(&mut items, (7.1, 1.5), 0.9, 0.0, 90.0, door_layer);
    seg(&mut items, (9.0, 5.0), (9.0, 4.2), door_layer);
    arc(&mut items, (9.0, 5.0), 0.8, 270.0, 360.0, door_layer);

    let mut walls = vec![
        WallRun::new(v((0.1, 0.1)), v((11.9, 0.1)), 0.2),
        WallRun::new(v((11.9, 0.1)), v((11.9, 7.9)), 0.2),
        WallRun::new(v((11.9, 7.9)), v((0.1, 7.9)), 0.2),
        WallRun::new(v((0.1, 7.9)), v((0.1, 0.1)), 0.2),
        WallRun::new(v((7.05, 0.1)), v((7.05, 7.9)), 0.1),
        WallRun::new(v((7.05, 5.05)), v((11.9, 5.05)), 0.1),
    ];
    let mut openings = vec![
        OpeningTruth { kind: CandKind::Door, center: v((7.05, 1.95)), width: 0.9 },
        OpeningTruth { kind: CandKind::Door, center: v((9.4, 5.05)), width: 0.8 },
        OpeningTruth { kind: CandKind::Window, center: v((2.5, 0.1)), width: 2.0 },
        OpeningTruth { kind: CandKind::Window, center: v((5.2, 0.1)), width: 2.0 },
        OpeningTruth { kind: CandKind::Window, center: v((9.6, 0.1)), width: 2.0 },
        OpeningTruth { kind: CandKind::Window, center: v((0.1, 5.3)), width: 2.0 },
        OpeningTruth { kind: CandKind::Window, center: v((11.9, 6.5)), width: 1.4 },
    ];

    for item in &mut items {
        transform_item(item, spec.quarter_turns, spec.offset);
    }
    for wall in &mut walls {
        wall.start = map_point(wall.start, spec.quarter_turns, spec.offset);
        wall.end = map_point(wall.end, spec.quarter_turns, spec.offset);
    }
    for opening in &mut openings {
        opening.center = map_point(opening.center, spec.quarter_turns, spec.offset);
    }

    Plan { name: spec.name, lw: Linework { items, ..Default::default() }, walls, openings }
}

fn pair(out: &mut String, code: i32, value: &str) {
    out.push_str(&format!("{code}\n{value}\n"));
}

fn number(out: &mut String, code: i32, value: f64) {
    pair(out, code, &value.to_string());
}

/// Write only ordinary DXF entities: there is no CADCRAFT dictionary or xdata to read back.
fn plain_dxf(lw: &Linework) -> String {
    let mut out = String::new();
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "HEADER");
    pair(&mut out, 9, "$INSUNITS");
    pair(&mut out, 70, "6");
    pair(&mut out, 0, "ENDSEC");

    let layers: BTreeSet<&str> = lw.items.iter().map(|it| it.layer.as_str()).collect();
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "TABLES");
    pair(&mut out, 0, "TABLE");
    pair(&mut out, 2, "LAYER");
    pair(&mut out, 70, &layers.len().to_string());
    for layer in layers {
        pair(&mut out, 0, "LAYER");
        pair(&mut out, 2, layer);
        pair(&mut out, 70, "0");
        pair(&mut out, 62, "7");
        pair(&mut out, 6, "CONTINUOUS");
    }
    pair(&mut out, 0, "ENDTAB");
    pair(&mut out, 0, "ENDSEC");

    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "ENTITIES");
    for item in &lw.items {
        match item.prim {
            Prim::Seg { a, b } => {
                pair(&mut out, 0, "LINE");
                pair(&mut out, 8, &item.layer);
                number(&mut out, 10, a.x);
                number(&mut out, 20, a.y);
                number(&mut out, 30, 0.0);
                number(&mut out, 11, b.x);
                number(&mut out, 21, b.y);
                number(&mut out, 31, 0.0);
            }
            Prim::Arc { c, r, start, sweep } => {
                pair(&mut out, 0, "ARC");
                pair(&mut out, 8, &item.layer);
                number(&mut out, 10, c.x);
                number(&mut out, 20, c.y);
                number(&mut out, 30, 0.0);
                number(&mut out, 40, r);
                number(&mut out, 50, start.to_degrees());
                number(&mut out, 51, (start + sweep).to_degrees());
            }
        }
    }
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "EOF");
    out
}

fn read_drawing(text: &str) -> Drawing {
    match cadcraft_io::read(text) {
        Ok(drawing) => drawing,
        Err(error) => panic!("generated DXF did not read: {error}"),
    }
}

fn detect_drawing(drawing: &Drawing) -> Detection {
    match detect::detect(drawing, &DEFAULT) {
        Ok(detection) => detection,
        Err(error) => panic!("detection failed: {error}"),
    }
}

fn detect_linework(lw: &Linework) -> Detection {
    let drawing = read_drawing(&plain_dxf(lw));
    detect_drawing(&drawing)
}

#[derive(Clone, Copy, Default)]
struct Match {
    hits: usize,
    found: usize,
    expected: usize,
}

impl Match {
    fn add(&mut self, other: Match) {
        self.hits += other.hits;
        self.found += other.found;
        self.expected += other.expected;
    }
}

#[derive(Default)]
struct Pool {
    walls: Match,
    doors: Match,
    windows: Match,
    notes: String,
}

fn wall_matches(candidate: &WallRun, truth: &WallRun) -> bool {
    if (candidate.thickness - truth.thickness).abs() > 0.2 * truth.thickness {
        return false;
    }
    let tolerance = (0.5 * truth.thickness).max(0.05);
    let direct = candidate.start.dist(truth.start) <= tolerance && candidate.end.dist(truth.end) <= tolerance;
    let reversed = candidate.start.dist(truth.end) <= tolerance && candidate.end.dist(truth.start) <= tolerance;
    direct || reversed
}

fn score_walls(plan: &str, detection: &Detection, truth: &[WallRun], notes: &mut String) -> Match {
    let found: Vec<&WallRun> = detection.walls.iter().filter(|w| w.confidence >= DEFAULT_MIN_CONFIDENCE).collect();
    let mut used = vec![false; found.len()];
    let mut hits = 0;
    for expected in truth {
        if let Some((index, _)) = found.iter().enumerate().find(|(index, candidate)| !used[*index] && wall_matches(candidate, expected)) {
            used[index] = true;
            hits += 1;
        } else {
            notes.push_str(&format!("{plan}: missed wall {expected:?}\n"));
        }
    }
    for (index, candidate) in found.iter().enumerate() {
        if !used[index] {
            notes.push_str(&format!("{plan}: extra wall {candidate:?}\n"));
        }
    }
    Match { hits, found: found.len(), expected: truth.len() }
}

fn score_openings(plan: &str, detection: &Detection, truth: &[OpeningTruth], kind: CandKind, notes: &mut String) -> Match {
    let expected: Vec<&OpeningTruth> = truth.iter().filter(|o| o.kind == kind).collect();
    let found: Vec<_> = detection.openings.iter().filter(|o| o.kind == kind && o.confidence >= DEFAULT_MIN_CONFIDENCE).collect();
    let mut used = vec![false; found.len()];
    let mut hits = 0;
    for truth in expected.iter() {
        let matching = found.iter().enumerate().find(|(index, candidate)| {
            !used[*index] && candidate.center.dist(truth.center) <= 0.15 && (candidate.width - truth.width).abs() <= 0.1 * truth.width
        });
        if let Some((index, _)) = matching {
            used[index] = true;
            hits += 1;
        } else {
            notes.push_str(&format!("{plan}: missed {kind:?} at {:?}, width {}\n", truth.center, truth.width));
        }
    }
    for (index, candidate) in found.iter().enumerate() {
        if !used[index] {
            notes.push_str(&format!("{plan}: extra {kind:?} {candidate:?}\n"));
        }
    }
    Match { hits, found: found.len(), expected: expected.len() }
}

fn add_plan(pool: &mut Pool, plan: &Plan) {
    let detection = detect_linework(&plan.lw);
    pool.walls.add(score_walls(plan.name, &detection, &plan.walls, &mut pool.notes));
    pool.doors.add(score_openings(plan.name, &detection, &plan.openings, CandKind::Door, &mut pool.notes));
    pool.windows.add(score_openings(plan.name, &detection, &plan.openings, CandKind::Window, &mut pool.notes));
}

fn assert_targets(kind: &str, result: Match, min_precision: f64, min_recall: f64, notes: &str) {
    let precision = if result.found == 0 { 0.0 } else { result.hits as f64 / result.found as f64 };
    let recall = if result.expected == 0 { 0.0 } else { result.hits as f64 / result.expected as f64 };
    assert!(
        precision >= min_precision && recall >= min_recall,
        "{kind}: precision {precision:.3} ({}/{}), recall {recall:.3} ({}/{})\n{notes}",
        result.hits,
        result.found,
        result.hits,
        result.expected,
    );
}

#[test]
fn synthetic_corpus_meets_the_targets() {
    let specs = [
        PlanSpec { name: "house", offset: (0.0, 0.0), quarter_turns: 0, layerless: false },
        PlanSpec { name: "ell", offset: (30.0, 0.0), quarter_turns: 0, layerless: false },
        PlanSpec { name: "turned", offset: (0.0, 30.0), quarter_turns: 1, layerless: false },
        PlanSpec { name: "cabin", offset: (40.0, 40.0), quarter_turns: 0, layerless: false },
        PlanSpec { name: "bay", offset: (-30.0, 20.0), quarter_turns: 2, layerless: false },
        PlanSpec { name: "house-layer-0", offset: (80.0, 80.0), quarter_turns: 0, layerless: true },
    ];
    let mut pool = Pool::default();
    for spec in specs {
        add_plan(&mut pool, &synthetic_plan(spec));
    }
    assert_targets("synthetic walls", pool.walls, 0.95, 0.95, &pool.notes);
    assert_targets("synthetic doors", pool.doors, 0.90, 0.90, &pool.notes);
    assert_targets("synthetic windows", pool.windows, 0.90, 0.90, &pool.notes);
}

#[test]
fn apartment_walls_and_rooms_meet_the_targets() {
    let drawing = read_drawing(include_str!("../../../examples/apartment.dxf"));
    let detection = detect_drawing(&drawing);
    let truth = synthetic_plan(PlanSpec { name: "apartment-truth", offset: (0.0, 0.0), quarter_turns: 0, layerless: false });
    let mut notes = String::new();
    let walls = score_walls("apartment", &detection, &truth.walls, &mut notes);
    assert_targets("apartment walls", walls, 0.80, 0.80, &notes);

    let names: Vec<String> = detection.rooms.iter().filter_map(|room| room.name.as_ref()).map(|name| name.to_uppercase()).collect();
    assert!(
        names.iter().any(|name| name.contains("LIVING") && name.contains("DINING")),
        "no LIVING / DINING room in {names:?}; detection: {detection:#?}"
    );
    assert!(names.iter().any(|name| name.contains("BEDROOM")), "no BEDROOM in {names:?}; detection: {detection:#?}");
    assert!(names.iter().any(|name| name.contains("BATH")), "no BATH in {names:?}; detection: {detection:#?}");
}

#[test]
fn ten_thousand_entities_find_six_walls_per_tile_quickly() {
    let base = synthetic_plan(PlanSpec { name: "tile", offset: (0.0, 0.0), quarter_turns: 0, layerless: false });
    let mut items = Vec::new();
    let mut tiles = Vec::new();
    for y in 0..17 {
        for x in 0..17 {
            let offset = (x as f64 * 20.0, y as f64 * 16.0);
            tiles.push(offset);
            for source in &base.lw.items {
                let mut item = source.clone();
                transform_item(&mut item, 0, offset);
                items.push(item);
            }
        }
    }
    assert!(items.len() >= 10_000, "the timing corpus is too small: {}", items.len());
    let drawing = read_drawing(&plain_dxf(&Linework { items, ..Default::default() }));
    assert!(drawing.model.len() >= 10_000, "the DXF round trip lost entities");

    let started = Instant::now();
    let detection = detect_drawing(&drawing);
    let elapsed = started.elapsed();

    let mut counts = vec![0_usize; tiles.len()];
    for wall in detection.walls.iter().filter(|wall| wall.confidence >= DEFAULT_MIN_CONFIDENCE) {
        let middle = (wall.start + wall.end) * 0.5;
        for (index, &(dx, dy)) in tiles.iter().enumerate() {
            if middle.x >= dx - 0.2 && middle.x <= dx + 12.2 && middle.y >= dy - 0.2 && middle.y <= dy + 8.2 {
                counts[index] += 1;
                break;
            }
        }
    }
    let wrong: Vec<String> = counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count != 6)
        .map(|(index, count)| format!("tile {index} ({}): {count} walls", tiles[index]))
        .collect();
    assert!(wrong.is_empty(), "per-tile wall counts differ: {}", wrong.join(", "));

    if cfg!(debug_assertions) {
        return;
    }
    assert!(elapsed.as_secs_f64() < 2.0, "detection took {elapsed:?} for {} entities", drawing.model.len());
}
