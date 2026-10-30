//! Flatten DXF entities into primitives.
//!
//! Model space is walked once; every curve becomes a [`Prim`] in metres (drawing units times
//! `DetectOptions::scale`), INSERTs are expanded recursively (nested blocks, MINSERT arrays) and
//! TEXT/MTEXT become [`Label`]s. Everything is bounded: [`MAX_PRIMITIVES`], [`MAX_LABELS`] and
//! [`MAX_BLOCK_DEPTH`].

use std::f64::consts::TAU;

use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, Insert, MAX_BLOCK_DEPTH};
use cadcraft_geom::{Mat3, Vec2, bulge_to_arc};

use super::{DetectError, DetectOptions, Item, Label, Linework, MAX_LABELS, MAX_PRIMITIVES, Prim};

/// Segments an arc becomes under a non-uniform or mirrored-and-skewed matrix.
const ARC_SEGMENTS: usize = 16;
/// Upper bound on INSERT expansions (copies of empty blocks cost no primitives, so count them).
const MAX_VISITS: usize = 4 * MAX_PRIMITIVES;

/// Flatten the drawing into linework primitives.
pub fn flatten(d: &Drawing, opts: &DetectOptions) -> Result<Linework, DetectError> {
    let layers: Option<Vec<String>> = opts.layers.as_ref().map(|v| v.iter().map(|s| s.to_lowercase()).collect());
    let mut cx = Cx { d, layers, out: Linework::default(), poly: 0, visits: 0 };
    let root = Mat3::scale(opts.scale, opts.scale);
    for e in d.model.iter() {
        cx.out.entities += 1;
        cx.entity(e, &root, &e.common.layer, e.handle, 0)?;
    }
    Ok(cx.out)
}

struct Cx<'a> {
    d: &'a Drawing,
    layers: Option<Vec<String>>,
    out: Linework,
    poly: u32,
    visits: usize,
}

impl Cx<'_> {
    /// Whether geometry on `layer` is read.
    fn wanted(&self, layer: &str) -> bool {
        if let Some(l) = self.d.layer(layer)
            && (!l.on || l.frozen)
        {
            return false;
        }
        match &self.layers {
            Some(list) => list.iter().any(|n| n.eq_ignore_ascii_case(layer)),
            None => true,
        }
    }

    fn entity(&mut self, e: &Entity, m: &Mat3, parent_layer: &str, source: Handle, depth: usize) -> Result<(), DetectError> {
        // Layer "0" inside a block takes the layer of the INSERT carrying it.
        let layer: &str = if depth > 0 && e.common.layer == "0" { parent_layer } else { &e.common.layer };
        // An INSERT is gated on its own layer only for the reference itself; its content is gated per entity.
        let gate = self.d.layer(layer).is_none_or(|l| l.on && !l.frozen);
        if !gate {
            return Ok(());
        }
        match &e.kind {
            EntityKind::Insert(ins) => return self.insert(ins, m, layer, source, depth),
            _ if !self.wanted(layer) => return Ok(()),
            EntityKind::Line(l) => self.place(Prim::Seg { a: l.a.xy(), b: l.b.xy() }, m, layer, source, None)?,
            EntityKind::Arc(a) => {
                let sweep = match (a.end - a.start).rem_euclid(TAU) {
                    w if w < 1e-12 => TAU,
                    w => w,
                };
                self.place(Prim::Arc { c: a.center.xy(), r: a.radius, start: a.start, sweep }, m, layer, source, None)?;
            }
            EntityKind::Circle(c) => self.place(Prim::Arc { c: c.center.xy(), r: c.radius, start: 0.0, sweep: TAU }, m, layer, source, None)?,
            EntityKind::LwPolyline(p) => {
                let pts: Vec<(Vec2, f64)> = p.vertices.iter().map(|v| (v.p, v.bulge)).collect();
                self.polyline(&pts, p.closed, m, layer, source)?;
            }
            EntityKind::Polyline3d(p) => {
                let pts: Vec<(Vec2, f64)> = p.points.iter().map(|v| (v.xy(), 0.0)).collect();
                self.polyline(&pts, p.closed, m, layer, source)?;
            }
            EntityKind::Text(t) => self.label(t.insert.xy(), &t.value, m, source)?,
            EntityKind::MText(t) => self.label(t.insert.xy(), &t.contents, m, source)?,
            _ => {}
        }
        Ok(())
    }

    fn insert(&mut self, ins: &Insert, m: &Mat3, layer: &str, source: Handle, depth: usize) -> Result<(), DetectError> {
        if depth >= MAX_BLOCK_DEPTH {
            return Ok(());
        }
        let Some(blk) = self.d.block(&ins.block) else { return Ok(()) };
        let blk = blk.clone();
        let local = ins.transform(blk.base.xy());
        let (cols, rows) = (u64::from(ins.cols.max(1)), u64::from(ins.rows.max(1)));
        if cols.saturating_mul(rows) > MAX_VISITS as u64 {
            return Err(too_many());
        }
        let (sin, cos) = ins.rotation.sin_cos();
        for r in 0..rows {
            for c in 0..cols {
                self.visits += 1;
                if self.visits > MAX_VISITS {
                    return Err(too_many());
                }
                let (dx, dy) = (c as f64 * ins.col_spacing, r as f64 * ins.row_spacing);
                let off = Vec2::new(dx * cos - dy * sin, dx * sin + dy * cos);
                let copy = m.then_before(Mat3::translate(off).then_before(local));
                // Block content is placed on the top-level INSERT (`source`).
                for child in blk.entities.iter() {
                    self.entity(child, &copy, layer, source, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    fn polyline(&mut self, pts: &[(Vec2, f64)], closed: bool, m: &Mat3, layer: &str, source: Handle) -> Result<(), DetectError> {
        self.poly = self.poly.wrapping_add(1);
        let n = pts.len();
        let poly = Some(self.poly);
        if pts.iter().all(|(_, b)| *b == 0.0) {
            let world: Vec<Vec2> = pts.iter().map(|(p, _)| m.apply(*p)).collect();
            if world.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
                return Ok(());
            }
            for prim in super::arcs::polyline_prims(&world, closed) {
                self.push(prim, layer, source, poly)?;
            }
            return Ok(());
        }
        let count = if closed { n } else { n.saturating_sub(1) };
        for i in 0..count {
            let (Some((a, bulge)), Some((b, _))) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
            let prim = match bulge_to_arc(*a, *b, *bulge) {
                Some((arc, _)) => Prim::Arc { c: arc.center, r: arc.radius, start: arc.start, sweep: arc.sweep() },
                None => Prim::Seg { a: *a, b: *b },
            };
            self.place(prim, m, layer, source, poly)?;
        }
        Ok(())
    }

    fn label(&mut self, at: Vec2, raw: &str, m: &Mat3, source: Handle) -> Result<(), DetectError> {
        let text = label_text(raw);
        let at = m.apply(at);
        if text.is_empty() || !at.x.is_finite() || !at.y.is_finite() {
            return Ok(());
        }
        if self.out.labels.len() >= MAX_LABELS {
            return Err(DetectError(format!("the drawing has more than {MAX_LABELS} labels")));
        }
        self.out.labels.push(Label::new(at, text, source));
        Ok(())
    }

    /// Transform a block-space primitive by `m` and add it.
    fn place(&mut self, prim: Prim, m: &Mat3, layer: &str, source: Handle, poly: Option<u32>) -> Result<(), DetectError> {
        match prim {
            Prim::Seg { a, b } => {
                let (a, b) = (m.apply(a), m.apply(b));
                self.push(Prim::Seg { a, b }, layer, source, poly)
            }
            Prim::Arc { c, r, start, sweep } => {
                let uniform = (m.a * m.a + m.b * m.b - m.c * m.c - m.d * m.d).abs() <= 1e-9 * (m.a * m.a + m.b * m.b).max(1e-300)
                    && (m.a * m.c + m.b * m.d).abs() <= 1e-9 * (m.a * m.a + m.b * m.b).max(1e-300);
                if uniform {
                    let k = m.det().abs().sqrt();
                    let dir = m.apply_vec(Vec2::new(start.cos(), start.sin()));
                    let (c, r) = (m.apply(c), r.abs() * k);
                    let (start, sweep) = (dir.y.atan2(dir.x), if m.det() < 0.0 { -sweep } else { sweep });
                    return self.push(Prim::Arc { c, r, start, sweep }, layer, source, poly);
                }
                if !r.is_finite() || !sweep.is_finite() {
                    return Ok(());
                }
                let mut prev = m.apply(Vec2::new(c.x + r * start.cos(), c.y + r * start.sin()));
                for i in 1..=ARC_SEGMENTS {
                    let t = start + sweep * i as f64 / ARC_SEGMENTS as f64;
                    let next = m.apply(Vec2::new(c.x + r * t.cos(), c.y + r * t.sin()));
                    self.push(Prim::Seg { a: prev, b: next }, layer, source, poly)?;
                    prev = next;
                }
                Ok(())
            }
        }
    }

    /// Add a primitive in metres; a non-finite one is dropped.
    fn push(&mut self, prim: Prim, layer: &str, source: Handle, poly: Option<u32>) -> Result<(), DetectError> {
        let finite = match prim {
            Prim::Seg { a, b } => [a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()),
            Prim::Arc { c, r, start, sweep } => [c.x, c.y, r, start, sweep].iter().all(|v| v.is_finite()) && r > 0.0,
        };
        if !finite {
            return Ok(());
        }
        if self.out.items.len() >= MAX_PRIMITIVES {
            return Err(too_many());
        }
        self.out.items.push(Item::new(prim, layer, source, poly));
        Ok(())
    }
}

fn too_many() -> DetectError {
    DetectError(format!("the drawing has more than {MAX_PRIMITIVES} primitives"))
}

/// Extract text from a raw label string: the first paragraph, formatting removed.
pub fn label_text(raw: &str) -> String {
    let mut out = String::new();
    let mut it = raw.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\n' | '\r' => break,
            '{' | '}' => {}
            '\\' => match it.next() {
                Some('P') | None => break,
                Some('H' | 'W' | 'Q' | 'T' | 'A' | 'C' | 'F' | 'f' | 'p') => {
                    for c in it.by_ref() {
                        if c == ';' {
                            break;
                        }
                    }
                }
                Some('L' | 'l' | 'O' | 'o' | 'K' | 'k') => {}
                Some('~') => out.push(' '),
                Some(c) => out.push(c),
            },
            '%' if it.peek() == Some(&'%') => {
                let mut ahead = it.clone();
                ahead.next();
                match ahead.next() {
                    Some('c' | 'C') => out.push('Ø'),
                    Some('d' | 'D') => out.push('°'),
                    Some('p' | 'P') => out.push('±'),
                    _ => {
                        out.push('%');
                        continue;
                    }
                }
                it = ahead;
            }
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Block, Circle, Common, Line, LwPolyline, MText, Space};
    use cadcraft_geom::{PolyVertex, Vec3};

    fn common(layer: &str) -> Common {
        Common { layer: layer.into(), ..Common::default() }
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> EntityKind {
        EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })
    }

    fn insert(name: &str, at: (f64, f64), rotation: f64) -> Insert {
        Insert {
            block: name.into(),
            insert: Vec3::new(at.0, at.1, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }
    }

    fn define(d: &mut Drawing, name: &str, content: &[(&str, EntityKind)]) {
        let mut b = Block::new(name);
        for (i, (layer, kind)) in content.iter().enumerate() {
            b.entities.push(Entity { handle: Handle(0x9000 + i as u64), common: common(layer), kind: kind.clone() });
        }
        d.blocks.insert(name.into(), std::sync::Arc::new(b));
    }

    fn opts(scale: f64) -> DetectOptions {
        DetectOptions { scale, ..Default::default() }
    }

    fn seg(p: &Prim) -> (Vec2, Vec2) {
        match p {
            Prim::Seg { a, b } => (*a, *b),
            other => panic!("not a segment: {other:?}"),
        }
    }

    #[test]
    fn millimetre_lines_are_read_in_metres() {
        let mut d = Drawing::new_metric();
        d.add(&Space::Model, common("A-WALL"), line((0.0, 0.0), (3000.0, 0.0))).unwrap();
        let lw = flatten(&d, &opts(0.001)).unwrap();
        assert_eq!(lw.entities, 1);
        let (a, b) = seg(&lw.items[0].prim);
        assert!(a.dist(Vec2::new(0.0, 0.0)) < 1e-9 && b.dist(Vec2::new(3.0, 0.0)) < 1e-9);
        assert_eq!(lw.items[0].layer, "A-WALL");
    }

    #[test]
    fn a_bulged_polyline_edge_is_an_arc() {
        let mut d = Drawing::new_metric();
        let vertices =
            vec![PolyVertex::with_bulge(Vec2::new(0.0, 0.0), 1.0), PolyVertex::new(Vec2::new(2.0, 0.0)), PolyVertex::new(Vec2::new(2.0, 2.0))];
        let pl = LwPolyline { vertices, closed: false, const_width: 0.0, elevation: 0.0, plinegen: false };
        d.add(&Space::Model, common("0"), EntityKind::LwPolyline(pl)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 2);
        match lw.items[0].prim {
            Prim::Arc { c, r, sweep, .. } => {
                assert!(c.dist(Vec2::new(1.0, 0.0)) < 1e-9 && (r - 1.0).abs() < 1e-9);
                assert!((sweep - std::f64::consts::PI).abs() < 1e-9);
            }
            other => panic!("expected an arc, got {other:?}"),
        }
        assert!(matches!(lw.items[1].prim, Prim::Seg { .. }));
        assert_eq!(lw.items[0].poly, lw.items[1].poly);
        assert!(lw.items[0].poly.is_some());
    }

    fn door_block(d: &mut Drawing) {
        let arc = EntityKind::Arc(cadcraft_doc::Arc { center: Vec3::ZERO, radius: 0.9, start: 0.0, end: std::f64::consts::FRAC_PI_2 });
        define(d, "DOOR", &[("0", arc)]);
    }

    #[test]
    fn inserts_place_block_content_in_the_world() {
        let mut d = Drawing::new_metric();
        door_block(&mut d);
        let h1 = d.add(&Space::Model, common("A-DOOR"), EntityKind::Insert(insert("DOOR", (1.0, 1.0), 0.0))).unwrap();
        d.add(&Space::Model, common("A-DOOR"), EntityKind::Insert(insert("DOOR", (5.0, 2.0), std::f64::consts::FRAC_PI_2))).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 2);
        assert_eq!(lw.items[0].source, h1);
        let Prim::Arc { c, r, start, sweep } = lw.items[0].prim else { panic!("arc expected") };
        assert!(c.dist(Vec2::new(1.0, 1.0)) < 1e-9 && (r - 0.9).abs() < 1e-9 && start.abs() < 1e-9 && sweep > 0.0);
        let Prim::Arc { c, start, .. } = lw.items[1].prim else { panic!("arc expected") };
        assert!(c.dist(Vec2::new(5.0, 2.0)) < 1e-9);
        assert!((start - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn nested_blocks_compose_their_transforms() {
        let mut d = Drawing::new_metric();
        define(&mut d, "L0", &[("0", line((0.0, 0.0), (1.0, 0.0)))]);
        define(&mut d, "L1", &[("0", EntityKind::Insert(insert("L0", (1.0, 0.0), 0.0)))]);
        define(&mut d, "L2", &[("0", EntityKind::Insert(insert("L1", (0.0, 1.0), 0.0)))]);
        let top = d.add(&Space::Model, common("A-WALL"), EntityKind::Insert(insert("L2", (10.0, 10.0), 0.0))).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 1);
        let (a, b) = seg(&lw.items[0].prim);
        assert!(a.dist(Vec2::new(11.0, 11.0)) < 1e-9 && b.dist(Vec2::new(12.0, 11.0)) < 1e-9);
        assert_eq!(lw.items[0].source, top);
    }

    #[test]
    fn a_self_inserting_block_stops_without_error() {
        let mut d = Drawing::new_metric();
        define(&mut d, "LOOP", &[("0", line((0.0, 0.0), (1.0, 0.0))), ("0", EntityKind::Insert(insert("LOOP", (1.0, 0.0), 0.0)))]);
        d.add(&Space::Model, common("A-WALL"), EntityKind::Insert(insert("LOOP", (0.0, 0.0), 0.0))).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), MAX_BLOCK_DEPTH);
    }

    #[test]
    fn minsert_arrays_give_every_copy() {
        let mut d = Drawing::new_metric();
        define(&mut d, "B", &[("0", line((0.0, 0.0), (1.0, 0.0)))]);
        let mut ins = insert("B", (0.0, 0.0), 0.0);
        (ins.cols, ins.rows, ins.col_spacing, ins.row_spacing) = (2, 3, 4.0, 5.0);
        d.add(&Space::Model, common("A-WALL"), EntityKind::Insert(ins)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 6);
        let (a, _) = seg(&lw.items[5].prim);
        assert!(a.dist(Vec2::new(4.0, 10.0)) < 1e-9);
    }

    #[test]
    fn layer_zero_block_content_takes_the_insert_layer() {
        let mut d = Drawing::new_metric();
        define(&mut d, "B", &[("0", line((0.0, 0.0), (1.0, 0.0))), ("A-GLAZ", line((0.0, 1.0), (1.0, 1.0)))]);
        d.add(&Space::Model, common("A-WALL"), EntityKind::Insert(insert("B", (0.0, 0.0), 0.0))).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items[0].layer, "A-WALL");
        assert_eq!(lw.items[1].layer, "A-GLAZ");
    }

    #[test]
    fn off_and_frozen_layers_are_skipped() {
        let mut d = Drawing::new_metric();
        for l in ["OFF", "ICE", "ON"] {
            d.add(&Space::Model, common(l), line((0.0, 0.0), (1.0, 0.0))).unwrap();
        }
        d.add(&Space::Model, common("ON"), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 })).unwrap();
        if let Some(l) = d.layer_mut("OFF") {
            l.on = false;
        }
        if let Some(l) = d.layer_mut("ICE") {
            l.frozen = true;
        }
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 2);
        assert!(lw.items.iter().all(|i| i.layer == "ON"));
        let only = DetectOptions { scale: 1.0, layers: Some(vec!["on".into()]), ..Default::default() };
        assert_eq!(flatten(&d, &only).unwrap().items.len(), 2);
        let none = DetectOptions { scale: 1.0, layers: Some(vec!["other".into()]), ..Default::default() };
        assert!(flatten(&d, &none).unwrap().items.is_empty());
    }

    #[test]
    fn mtext_label_text_is_its_first_line_without_codes() {
        assert_eq!(label_text("LIVING / DINING\\P{\\H0.6x;34.6 m²}"), "LIVING / DINING");
        assert_eq!(label_text("{\\fArial|b1;KITCHEN}  \\Lroom\\~one"), "KITCHEN room one");
        assert_eq!(label_text("A\nB"), "A");
        assert_eq!(label_text("%%c20 %%d %%p1"), "Ø20 ° ±1");
        let mut d = Drawing::new_metric();
        let t = EntityKind::MText(MText { contents: "BED 1\\PXX".into(), ..sample_mtext() });
        d.add(&Space::Model, common("A-ANNO"), t).unwrap();
        let lw = flatten(&d, &opts(0.001)).unwrap();
        assert_eq!(lw.labels.len(), 1);
        assert_eq!(lw.labels[0].text, "BED 1");
    }

    fn sample_mtext() -> MText {
        MText {
            insert: Vec3::new(1000.0, 0.0, 0.0),
            height: 2.5,
            width: 0.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: String::new(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
        }
    }

    #[test]
    fn an_empty_drawing_produces_empty_linework() {
        let d = Drawing::new_metric();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.entities, 0);
        assert!(lw.items.is_empty());
        assert!(lw.labels.is_empty());
    }

    #[test]
    fn a_zero_length_line_is_kept() {
        let mut d = Drawing::new_metric();
        d.add(&Space::Model, common("0"), line((1.0, 2.0), (1.0, 2.0))).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 1);
        let (a, b) = seg(&lw.items[0].prim);
        assert!(a.dist(b) < 1e-12);
    }

    #[test]
    fn nan_coordinates_are_dropped() {
        let mut d = Drawing::new_metric();
        let nan_line = EntityKind::Line(Line { a: Vec3::new(f64::NAN, 0.0, 0.0), b: Vec3::new(1.0, 1.0, 0.0) });
        d.add(&Space::Model, common("0"), nan_line).unwrap();
        let good_line = EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(1.0, 1.0, 0.0) });
        d.add(&Space::Model, common("0"), good_line).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 1);
    }

    #[test]
    fn a_uniform_scaled_arc_transforms_correctly() {
        let mut d = Drawing::new_metric();
        define(&mut d, "B", &[("0", EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 }))]);
        let mut ins = insert("B", (0.0, 0.0), 0.0);
        ins.scale = Vec3::new(2.0, 2.0, 1.0);
        d.add(&Space::Model, common("0"), EntityKind::Insert(ins)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 1);
        match lw.items[0].prim {
            Prim::Arc { c, r, sweep, .. } => {
                assert!(c.dist(Vec2::ZERO) < 1e-9);
                assert!((r - 2.0).abs() < 1e-9);
                assert!((sweep - TAU).abs() < 1e-9);
            }
            other => panic!("expected arc, got {other:?}"),
        }
    }

    #[test]
    fn a_mirrored_matrix_reverses_arc_sweep() {
        let mut d = Drawing::new_metric();
        define(&mut d, "B", &[("0", EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 }))]);
        let mut ins = insert("B", (0.0, 0.0), 0.0);
        ins.scale = Vec3::new(-1.0, 1.0, 1.0);
        d.add(&Space::Model, common("0"), EntityKind::Insert(ins)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 1);
        match lw.items[0].prim {
            Prim::Arc { r, sweep, .. } => {
                assert!((r - 1.0).abs() < 1e-9);
                assert!((sweep + TAU).abs() < 1e-9);
            }
            other => panic!("expected arc, got {other:?}"),
        }
    }

    #[test]
    fn an_arc_under_nonuniform_scale_tessellates() {
        let mut d = Drawing::new_metric();
        define(&mut d, "B", &[("0", EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 }))]);
        let mut ins = insert("B", (0.0, 0.0), 0.0);
        ins.scale = Vec3::new(2.0, 1.0, 1.0);
        d.add(&Space::Model, common("0"), EntityKind::Insert(ins)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), ARC_SEGMENTS);
        for item in &lw.items {
            assert!(matches!(item.prim, Prim::Seg { .. }));
        }
    }

    #[test]
    fn entities_count_only_model_space() {
        let mut d = Drawing::new_metric();
        define(&mut d, "B", &[("0", line((0.0, 0.0), (1.0, 0.0))), ("0", line((0.0, 1.0), (1.0, 1.0))), ("0", line((0.0, 2.0), (1.0, 2.0)))]);
        d.add(&Space::Model, common("0"), EntityKind::Insert(insert("B", (0.0, 0.0), 0.0))).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.entities, 1);
        assert_eq!(lw.items.len(), 3);
    }

    #[test]
    fn a_closed_polyline_without_bulges_uses_polyline_prims() {
        let mut d = Drawing::new_metric();
        let vertices = vec![
            PolyVertex::new(Vec2::new(0.0, 0.0)),
            PolyVertex::new(Vec2::new(1.0, 0.0)),
            PolyVertex::new(Vec2::new(1.0, 1.0)),
            PolyVertex::new(Vec2::new(0.0, 1.0)),
        ];
        let pl = LwPolyline { vertices, closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
        let h = d.add(&Space::Model, common("0"), EntityKind::LwPolyline(pl)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert_eq!(lw.items.len(), 4);
        for item in &lw.items {
            assert_eq!(item.source, h);
            assert!(item.poly.is_some());
        }
    }

    #[test]
    fn a_polyline_with_one_vertex_produces_nothing() {
        let mut d = Drawing::new_metric();
        let vertices = vec![PolyVertex::new(Vec2::new(0.0, 0.0))];
        let pl = LwPolyline { vertices, closed: false, const_width: 0.0, elevation: 0.0, plinegen: false };
        d.add(&Space::Model, common("0"), EntityKind::LwPolyline(pl)).unwrap();
        let lw = flatten(&d, &opts(1.0)).unwrap();
        assert!(lw.items.is_empty());
    }

    #[test]
    fn label_text_handles_edge_cases() {
        assert_eq!(label_text(""), "");
        assert_eq!(label_text("   "), "");
        assert_eq!(label_text("  HELLO  WORLD  "), "HELLO WORLD");
        assert_eq!(label_text("A\\PB"), "A");
        assert_eq!(label_text("A\\pB"), "A");
        assert_eq!(label_text("A\\PB"), "A");
        assert_eq!(label_text("{nested {inner}}"), "nested inner");
        assert_eq!(label_text("A\\~B"), "A B");
    }
}
