use crate::*;
use cadcraft_geom::{Bounds2, Mat3, PolyVertex, Vec2, Vec3};

fn line(a: (f64, f64), b: (f64, f64)) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })
}

#[test]
fn new_drawing_has_standard_tables() {
    let d = Drawing::new_imperial();
    assert!(d.layer("0").is_some());
    assert!(d.linetype("continuous").is_some());
    assert!(d.text_style("Standard").is_some());
    assert_eq!(d.header.f64("TEXTSIZE", 0.0), 0.2);
    let m = Drawing::new_metric();
    assert!(m.dim_style("ISO-25").is_some());
}

#[test]
fn new_metric_drawing_has_metric_multileader_and_table_styles() {
    let m = Drawing::new_metric();
    let iso = m.dim_style("ISO-25").unwrap();
    let ml = m.mleader_styles.iter().find(|s| s.name == "Standard").unwrap();
    assert_eq!((ml.text_height, ml.arrow_size), (iso.text_height, iso.arrow_size));
    assert_eq!(ml.dogleg, 5.0);
    let ts = m.table_styles.iter().find(|s| s.name == "Standard").unwrap();
    assert_eq!(ts.text_height, iso.text_height);
    assert_eq!(iso.text_height, 2.5);

    let i = Drawing::new_imperial();
    assert_eq!(i.mleader_styles, vec![MLeaderStyle::default()]);
    assert_eq!(i.table_styles, vec![TableStyle::default()]);
    assert_eq!((i.mleader_styles[0].text_height, i.table_styles[0].text_height), (0.18, 0.18));
}

#[test]
fn add_and_extents() {
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), line((0.0, 0.0), (10.0, 5.0))).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::new(20.0, 0.0, 0.0), radius: 2.0 })).unwrap();
    let b = d.extents(&Space::Model);
    assert_eq!(b.min, Vec2::new(0.0, -2.0));
    assert_eq!(b.max, Vec2::new(22.0, 5.0));
}

#[test]
fn hidden_layers_excluded_from_extents() {
    let mut d = Drawing::new_imperial();
    let c = Common { layer: "Hidden".into(), ..Default::default() };
    d.add(&Space::Model, c, line((100.0, 100.0), (200.0, 200.0))).unwrap();
    d.add(&Space::Model, Common::default(), line((0.0, 0.0), (1.0, 1.0))).unwrap();
    d.layer_mut("Hidden").unwrap().on = false;
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(1.0, 1.0));
}

#[test]
fn rays_and_xlines_do_not_stretch_extents() {
    // ZOOM Extents ignores infinite lines; real drawings carry stray rays millions of units away.
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), line((0.0, 0.0), (1.0, 1.0))).unwrap();
    let far = Vec3::new(-1_977_989.0, -1_952_638.0, 0.0);
    d.add(&Space::Model, Common::default(), EntityKind::Ray(RayLine { base: far, dir: Vec3::new(-0.7, -0.7, 0.0) })).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::XLine(RayLine { base: far, dir: Vec3::new(1.0, 0.0, 0.0) })).unwrap();
    let b = d.extents(&Space::Model);
    assert_eq!((b.min, b.max), (Vec2::ZERO, Vec2::new(1.0, 1.0)));
}

#[test]
fn inserts_of_missing_or_empty_blocks_do_not_stretch_extents() {
    // An unloaded xref or a purged definition draws nothing, so its insertion point is not an extent.
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), line((0.0, 0.0), (1.0, 1.0))).unwrap();
    let ins = |block: &str, x: f64, y: f64| {
        EntityKind::Insert(Insert {
            block: block.into(),
            insert: Vec3::new(x, y, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        })
    };
    d.blocks.insert("Empty".into(), std::sync::Arc::new(Block::new("Empty")));
    d.add(&Space::Model, Common::default(), ins("Missing", -125_319.0, -79_990.0)).unwrap();
    d.add(&Space::Model, Common::default(), ins("Empty", 55_652.0, -3_448_576.0)).unwrap();
    let b = d.extents(&Space::Model);
    assert_eq!((b.min, b.max), (Vec2::ZERO, Vec2::new(1.0, 1.0)));
    // A block with geometry still counts.
    let mut full = Block::new("Full");
    full.entities.push(Entity::new(Handle(1), line((0.0, 0.0), (2.0, 2.0))));
    d.blocks.insert("Full".into(), std::sync::Arc::new(full));
    d.add(&Space::Model, Common::default(), ins("Full", 10.0, 10.0)).unwrap();
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(12.0, 12.0));
}

#[test]
fn visible_attributes_of_a_missing_block_still_count() {
    // The renderer draws the attributes of a reference whose definition is missing, so they are
    // extents and selection targets, as with an empty definition. The insertion point still isn't.
    let attrib = |invisible: bool| Attrib {
        tag: "TAG".into(),
        text: Text {
            insert: Vec3::new(1000.0, 1000.0, 0.0),
            align_pt: None,
            height: 10.0,
            value: "hello".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Left,
            valign: VAlign::Baseline,
        },
        invisible,
        constant: false,
        prompt: String::new(),
        props: Default::default(),
        multiline: None,
        modes: Default::default(),
    };
    let ins = |block: &str, attribs: Vec<Attrib>| {
        Entity::new(
            Handle(1),
            EntityKind::Insert(Insert {
                block: block.into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs,
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        )
    };
    let mut d = Drawing::new_imperial();
    d.blocks.insert("Empty".into(), std::sync::Arc::new(Block::new("Empty")));
    let missing = entity_bounds(&d, &ins("Missing", vec![attrib(false)]), 0);
    let empty = entity_bounds(&d, &ins("Empty", vec![attrib(false)]), 0);
    assert!(!missing.is_empty());
    assert_eq!((missing.min, missing.max), (empty.min, empty.max));
    assert_eq!(missing.min, Vec2::new(1000.0, 1000.0));
    // Without a visible attribute there is nothing to measure.
    assert!(entity_bounds(&d, &ins("Missing", vec![]), 0).is_empty());
    assert!(entity_bounds(&d, &ins("Missing", vec![attrib(true)]), 0).is_empty());
}

#[test]
fn transform_arc_mirror_keeps_shape() {
    let mut k = EntityKind::Arc(Arc { center: Vec3::ZERO, radius: 1.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 });
    k.transform(&Mat3::mirror(Vec2::ZERO, Vec2::Y));
    if let EntityKind::Arc(a) = k {
        let g = cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
        assert!(g.mid_point().near(Vec2::from_angle(3.0 * std::f64::consts::FRAC_PI_4), 1e-9));
    } else {
        panic!()
    }
}

#[test]
fn insert_bounds_with_cyclic_block_terminate() {
    let mut d = Drawing::new_imperial();
    let mut b = Block::new("A");
    b.entities.push(Entity::new(
        Handle(1),
        EntityKind::Insert(Insert {
            block: "A".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    ));
    b.entities.push(Entity::new(Handle(2), line((0.0, 0.0), (1.0, 1.0))));
    d.blocks.insert("A".into(), std::sync::Arc::new(b));
    let h = d
        .add(
            &Space::Model,
            Common::default(),
            EntityKind::Insert(Insert {
                block: "A".into(),
                insert: Vec3::new(5.0, 5.0, 0.0),
                scale: Vec3::new(2.0, 2.0, 1.0),
                rotation: 0.0,
                attribs: vec![],
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        )
        .unwrap();
    let e = d.entity(h).unwrap().clone();
    let bb = entity_bounds(&d, &e, 0);
    assert_eq!(bb.max, Vec2::new(7.0, 7.0));
}

#[test]
fn entity_serde_roundtrip() {
    let e = Entity::new(Handle(7), line((1.0, 2.0), (3.0, 4.0)));
    let s = serde_json::to_string(&e).unwrap();
    let back: Entity = serde_json::from_str(&s).unwrap();
    assert_eq!(e, back);
}

#[test]
fn paper_space_entities() {
    let mut d = Drawing::new_imperial();
    let h = d.add(&Space::Paper("Layout1".into()), Common::default(), line((0.0, 0.0), (1.0, 0.0))).unwrap();
    assert_eq!(d.space_of(h), Some(Space::Paper("Layout1".into())));
    assert!(d.remove_entity(h).is_some());
    assert_eq!(d.entity_count(), 0);
}

fn wide_pline(verts: &[(f64, f64, f64, f64, f64)], closed: bool, const_width: f64) -> Bounds2 {
    // (x, y, start width, end width, bulge) per vertex.
    let mut d = Drawing::new_metric();
    let vertices = verts.iter().map(|&(x, y, s, e, b)| PolyVertex { p: Vec2::new(x, y), bulge: b, start_width: s, end_width: e }).collect();
    let kind = EntityKind::LwPolyline(LwPolyline { vertices, closed, const_width, elevation: 0.0, plinegen: false });
    d.add(&Space::Model, Common::default(), kind).unwrap();
    d.extents(&Space::Model)
}

fn near_bounds(b: Bounds2, min: (f64, f64), max: (f64, f64)) -> bool {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    close(b.min.x, min.0) && close(b.min.y, min.1) && close(b.max.x, max.0) && close(b.max.y, max.1)
}

/// #90: a polyline's extents enclose its drawn band, whether the width is constant or per segment.
#[test]
fn wide_polyline_extents_follow_the_band() {
    // The issue's cases: constant width, the same band from per-segment widths, a taper,
    // two segments of different widths, and a width on the open end vertex (which draws nothing).
    let b = wide_pline(&[(0.0, 0.0, 0.0, 0.0, 0.0), (10.0, 0.0, 0.0, 0.0, 0.0)], false, 2.0);
    assert!(near_bounds(b, (0.0, -1.0), (10.0, 1.0)), "{b:?}");
    let b = wide_pline(&[(0.0, 0.0, 2.0, 2.0, 0.0), (10.0, 0.0, 2.0, 2.0, 0.0)], false, 0.0);
    assert!(near_bounds(b, (0.0, -1.0), (10.0, 1.0)), "{b:?}");
    let b = wide_pline(&[(0.0, 0.0, 0.0, 4.0, 0.0), (10.0, 0.0, 4.0, 0.0, 0.0)], false, 0.0);
    assert!(near_bounds(b, (0.0, -2.0), (10.0, 2.0)), "{b:?}");
    let b = wide_pline(&[(0.0, 0.0, 2.0, 2.0, 0.0), (10.0, 0.0, 6.0, 6.0, 0.0), (20.0, 0.0, 0.0, 0.0, 0.0)], false, 0.0);
    assert!(near_bounds(b, (0.0, -3.0), (20.0, 3.0)), "{b:?}");
    let b = wide_pline(&[(0.0, 0.0, 0.0, 0.0, 0.0), (10.0, 0.0, 6.0, 6.0, 0.0)], false, 0.0);
    assert!(near_bounds(b, (0.0, 0.0), (10.0, 0.0)), "{b:?}");
    // A diagonal band: its exact corners, not the centre line padded by the half-width.
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let b = wide_pline(&[(0.0, 0.0, 0.0, 0.0, 0.0), (10.0, 10.0, 0.0, 0.0, 0.0)], false, 2.0);
    assert!(near_bounds(b, (-h, -h), (10.0 + h, 10.0 + h)), "{b:?}");
    // A closed polyline's closing segment counts (here the only wide one).
    let sq = [(0.0, 0.0, 0.0, 0.0, 0.0), (10.0, 0.0, 0.0, 0.0, 0.0), (10.0, 10.0, 0.0, 0.0, 0.0), (0.0, 10.0, 2.0, 2.0, 0.0)];
    assert!(near_bounds(wide_pline(&sq, true, 0.0), (-1.0, 0.0), (10.0, 10.0)));
    assert!(near_bounds(wide_pline(&sq, false, 0.0), (0.0, 0.0), (10.0, 10.0)), "open: the end vertex starts no segment");
    // Non-finite or negative widths count as zero.
    let b = wide_pline(&[(0.0, 0.0, f64::NAN, f64::INFINITY, 0.0), (10.0, 0.0, 0.0, 0.0, 0.0)], false, 0.0);
    assert!(near_bounds(b, (0.0, 0.0), (10.0, 0.0)), "{b:?}");
    let b = wide_pline(&[(0.0, 0.0, -4.0, -4.0, 0.0), (10.0, 0.0, 0.0, 0.0, 0.0)], false, 0.0);
    assert!(near_bounds(b, (0.0, 0.0), (10.0, 0.0)), "{b:?}");
}

/// A wide arc segment reaches its outer edge (radius plus half the width), never less, and only
/// by the sampling tolerance more.
#[test]
fn wide_arc_segment_extents_reach_the_outer_edge() {
    // A half circle below the axis (bulge 1, counter-clockwise from (0,0) to (10,0)), radius 5.
    let exact_min = (-1.0, -6.0);
    let exact_max = (11.0, 0.0);
    for b in [
        wide_pline(&[(0.0, 0.0, 0.0, 0.0, 1.0), (10.0, 0.0, 0.0, 0.0, 0.0)], false, 2.0),
        wide_pline(&[(0.0, 0.0, 2.0, 2.0, 1.0), (10.0, 0.0, 0.0, 0.0, 0.0)], false, 0.0),
    ] {
        assert!(b.min.x <= exact_min.0 && b.min.y <= exact_min.1 && b.max.x >= exact_max.0 && b.max.y >= exact_max.1, "never smaller: {b:?}");
        assert!(
            b.min.x > exact_min.0 - 1e-3 && b.min.y > exact_min.1 - 1e-3 && b.max.x < exact_max.0 + 1e-3 && b.max.y < exact_max.1 + 1e-3,
            "tight: {b:?}"
        );
    }
    // A taper along the arc (0 to 4 wide): compared with a brute-force sampling of both band
    // edges, since the extremes of a tapering curve lie between the obvious points.
    let b = wide_pline(&[(0.0, 0.0, 0.0, 4.0, 1.0), (10.0, 0.0, 0.0, 0.0, 0.0)], false, 0.0);
    let mut exact = Bounds2::EMPTY;
    for k in 0..=100_000 {
        let t = f64::from(k) / 100_000.0;
        // Counter-clockwise from angle pi (the start, (0,0)) to 2 pi (the end, (10,0)).
        let a = std::f64::consts::PI * (1.0 + t);
        let dir = Vec2::new(a.cos(), a.sin());
        let h = 2.0 * t;
        exact.add(Vec2::new(5.0, 0.0) + dir * (5.0 + h));
        exact.add(Vec2::new(5.0, 0.0) + dir * (5.0 - h));
    }
    for (got, want, outward) in [(b.min.x, exact.min.x, -1.0), (b.min.y, exact.min.y, -1.0), (b.max.x, exact.max.x, 1.0), (b.max.y, exact.max.y, 1.0)]
    {
        let over = (got - want) * outward;
        assert!(over > -1e-6 && over < 1e-3, "got {b:?}, exact {exact:?}");
    }
}

fn plain_insert(block: &str) -> EntityKind {
    EntityKind::Insert(Insert {
        block: block.into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    })
}

fn on_layer(layer: &str, kind: EntityKind, h: u64) -> Entity {
    Entity { handle: Handle(h), common: Common { layer: layer.into(), ..Default::default() }, kind }
}

/// Model space with one insert of a block holding a near line (0,0)-(1,1) plus `extra`.
fn drawing_with_block(extra: Entity) -> Drawing {
    let mut d = Drawing::new_imperial();
    let mut blk = Block::new("B");
    blk.entities.push(Entity::new(Handle(1), line((0.0, 0.0), (1.0, 1.0))));
    blk.entities.push(extra);
    d.blocks.insert("B".into(), std::sync::Arc::new(blk));
    d.add(&Space::Model, Common::default(), plain_insert("B")).unwrap();
    d
}

fn far_line(layer: &str) -> Entity {
    on_layer(layer, line((5000.0, 5000.0), (6000.0, 6000.0)), 2)
}

fn extents_pair(d: &Drawing) -> (Vec2, Vec2) {
    let b = d.extents(&Space::Model);
    (b.min, b.max)
}

/// #706: block A reaches B once through a chain of 14 blocks (B at depth 15, its line's block E at
/// 16, past the limit) and once directly (B at depth 1). `children` picks which of the two A holds.
fn deep_and_shallow(children: &[&str]) -> Drawing {
    let mut d = Drawing::new_imperial();
    let mut add = |name: &str, kids: Vec<EntityKind>| {
        let mut blk = Block::new(name);
        for (i, k) in kids.into_iter().enumerate() {
            blk.entities.push(Entity::new(Handle(i as u64 + 1), k));
        }
        d.blocks.insert(name.into(), std::sync::Arc::new(blk));
    };
    add("E", vec![line((1000.0, 1000.0), (2000.0, 1000.0))]);
    add("B", vec![plain_insert("E")]);
    for i in 1..=14 {
        let next = if i == 14 { "B".to_string() } else { format!("C{}", i + 1) };
        add(&format!("C{i}"), vec![plain_insert(&next)]);
    }
    add("A", children.iter().map(|c| plain_insert(c)).collect());
    d.add(&Space::Model, Common::default(), plain_insert("A")).unwrap();
    d
}

#[test]
fn deep_block_visit_does_not_hide_a_shallow_reference() {
    let line = (Vec2::new(1000.0, 1000.0), Vec2::new(2000.0, 1000.0));
    // Control: past the limit, the deep path alone contributes nothing.
    assert!(deep_and_shallow(&["C1"]).extents(&Space::Model).is_empty());
    assert_eq!(extents_pair(&deep_and_shallow(&["B"])), line);
    assert_eq!(extents_pair(&deep_and_shallow(&["B", "C1"])), line);
    assert_eq!(extents_pair(&deep_and_shallow(&["C1", "B"])), line);
}

#[test]
fn block_entity_with_invisible_flag_is_ignored_in_insert_extents() {
    let mut far = far_line("0");
    far.common.visible = false;
    let d = drawing_with_block(far);
    assert_eq!(extents_pair(&d), (Vec2::ZERO, Vec2::new(1.0, 1.0)));
    // Control: the same entity visible does count.
    let d = drawing_with_block(far_line("0"));
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
}

#[test]
fn block_entity_on_off_or_frozen_layer_is_ignored() {
    for frozen in [false, true] {
        let mut d = drawing_with_block(far_line("Hidden"));
        assert!(d.layer("Hidden").is_none(), "block contents do not create layers");
        let mut layer = Layer::new("Hidden");
        if frozen {
            layer.frozen = true;
        } else {
            layer.on = false;
        }
        d.layers.push(layer);
        assert_eq!(extents_pair(&d), (Vec2::ZERO, Vec2::new(1.0, 1.0)), "frozen = {frozen}");
        // Turned back on, it counts again.
        let l = d.layer_mut("Hidden").unwrap();
        l.on = true;
        l.frozen = false;
        assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
    }
}

#[test]
fn block_entity_on_layer_zero_counts_even_if_layer_zero_is_off_or_missing() {
    // Layer "0" takes the insert's layer, which the caller has already checked; so it counts here.
    // The insert itself sits on layer "Vis".
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common { layer: "Vis".into(), ..Default::default() }, plain_insert("B")).unwrap();
    let mut blk = Block::new("B");
    blk.entities.push(far_line("0"));
    d.blocks.insert("B".into(), std::sync::Arc::new(blk));
    // Layer 0 switched off.
    d.layer_mut("0").unwrap().on = false;
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
    // Layer 0 frozen.
    let l = d.layer_mut("0").unwrap();
    l.on = true;
    l.frozen = true;
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
    // Layer 0 not defined at all.
    d.layers.retain(|l| l.name != "0");
    assert!(d.layer("0").is_none());
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
}

#[test]
fn block_entity_on_undefined_layer_counts() {
    // An unknown layer name is not hidden (is_none_or).
    let d = drawing_with_block(far_line("Nowhere"));
    assert!(d.layer("Nowhere").is_none());
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
}

#[test]
fn attdef_in_block_is_ignored_in_insert_extents() {
    let attdef = Entity::new(
        Handle(2),
        EntityKind::AttDef(Attrib {
            tag: "TAG".into(),
            text: Text {
                insert: Vec3::new(5000.0, 5000.0, 0.0),
                align_pt: None,
                height: 1.0,
                value: "x".into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: HAlign::Left,
                valign: VAlign::Baseline,
            },
            invisible: false,
            constant: false,
            prompt: String::new(),
            props: Default::default(),
            multiline: None,
            modes: Default::default(),
        }),
    );
    let d = drawing_with_block(attdef);
    assert_eq!(extents_pair(&d), (Vec2::ZERO, Vec2::new(1.0, 1.0)));
}

fn text_kind(value: &str) -> EntityKind {
    EntityKind::Text(Text {
        insert: Vec3::new(10.0, 10.0, 0.0),
        align_pt: None,
        height: 2.0,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
    })
}

fn mtext_kind(contents: &str) -> EntityKind {
    EntityKind::MText(MText {
        insert: Vec3::new(10.0, 10.0, 0.0),
        height: 2.0,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: contents.into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
    })
}

#[test]
fn blank_text_and_mtext_have_no_bounds() {
    let d = Drawing::new_imperial();
    for s in ["", " ", "   ", "\t \n", "\\~", " \\~ \\~", "\u{a0}"] {
        for kind in [text_kind(s), mtext_kind(s)] {
            let b = entity_bounds(&d, &Entity::new(Handle(1), kind), 0);
            assert!(b.is_empty(), "{s:?} should have empty bounds");
        }
    }
    // Blank texts do not stretch the drawing extents either.
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), line((0.0, 0.0), (1.0, 1.0))).unwrap();
    d.add(&Space::Model, Common::default(), text_kind(" \\~")).unwrap();
    d.add(&Space::Model, Common::default(), mtext_kind("")).unwrap();
    assert_eq!(extents_pair(&d), (Vec2::ZERO, Vec2::new(1.0, 1.0)));
}

#[test]
fn non_blank_text_and_mtext_have_bounds() {
    let d = Drawing::new_imperial();
    for kind in [text_kind("Hi"), text_kind(" a "), mtext_kind("Hi"), mtext_kind("a\\Pb"), mtext_kind("\\~x")] {
        let b = entity_bounds(&d, &Entity::new(Handle(1), kind.clone()), 0);
        assert!(!b.is_empty(), "{kind:?} should have bounds");
    }
}

#[test]
fn invisible_dimblock_entity_is_ignored_in_dimension_extents() {
    // Dynamic blocks keep dimensional constraints on layer *ADSK_CONSTRAINTS (group 60 = 1).
    let dim = |d: &mut Drawing, far: Entity| {
        let mut blk = Block::new("*D1");
        blk.entities.push(Entity::new(Handle(1), line((0.0, 0.0), (1.0, 0.0))));
        blk.entities.push(far);
        d.blocks.insert("*D1".into(), std::sync::Arc::new(blk));
        d.add(
            &Space::Model,
            Common::default(),
            EntityKind::Dimension(Dimension {
                kind: DimKind::Linear { rotation: 0.0 },
                defpt: Vec3::new(1.0, 1.0, 0.0),
                text_mid: Vec3::new(0.5, 1.0, 0.0),
                p13: Vec3::ZERO,
                p14: Vec3::new(1.0, 0.0, 0.0),
                p15: Vec3::ZERO,
                p16: Vec3::ZERO,
                text: String::new(),
                style: "Standard".into(),
                measurement: 1.0,
                text_rotation: 0.0,
                user_text_pos: false,
                block: Some("*D1".into()),
                overrides: Default::default(),
                assoc: vec![],
                marks: Default::default(),
            }),
        )
        .unwrap();
    };
    // Invisible flag.
    let mut d = Drawing::new_imperial();
    let mut hidden = on_layer("*ADSK_CONSTRAINTS", line((5000.0, 5000.0), (6000.0, 6000.0)), 2);
    hidden.common.visible = false;
    dim(&mut d, hidden);
    assert!(d.extents(&Space::Model).max.x < 100.0);
    // Hidden by its layer state.
    let mut d = Drawing::new_imperial();
    let mut l = Layer::new("*ADSK_CONSTRAINTS");
    l.on = false;
    d.layers.push(l);
    dim(&mut d, on_layer("*ADSK_CONSTRAINTS", line((5000.0, 5000.0), (6000.0, 6000.0)), 2));
    assert!(d.extents(&Space::Model).max.x < 100.0);
    // Control: a visible far entity does count.
    let mut d = Drawing::new_imperial();
    dim(&mut d, far_line("0"));
    assert_eq!(d.extents(&Space::Model).max, Vec2::new(6000.0, 6000.0));
}

#[test]
fn space_names_its_tab() {
    assert!(!Space::Model.is_paper());
    assert_eq!(Space::Model.layout(), None);
    assert_eq!(Space::Model.tab_name(), "Model");
    let p = Space::Paper("Sheet A".into());
    assert!(p.is_paper());
    assert_eq!(p.layout(), Some("Sheet A"));
    assert_eq!(p.tab_name(), "Sheet A");
}

/// A new drawing provides the `A-WALL` architectural layer for proxy graphics.
#[test]
fn a_wall_layer_exists_in_a_new_drawing() {
    let d = Drawing::new_metric();
    assert!(d.layers.iter().any(|l| l.name == "A-WALL"));
}

/// The five AIA/NCS arch layers are present in a new drawing in the standard order,
/// after `0` and `Defpoints`.
#[test]
fn arch_layers_are_in_a_new_drawing_in_order() {
    let d = Drawing::new_imperial();
    let arch = ["A-WALL", "A-DOOR", "A-GLAZ", "A-ROOF", "A-STAIR"];
    for (i, name) in arch.iter().enumerate() {
        let idx = 2 + i;
        assert_eq!(d.layers[idx].name, *name, "layer at index {idx}");
    }
}

/// `RoofEdge` and `RoofEdgeKind` are re-exported at the crate root so downstream
/// callers can `use cadcraft_doc::RoofEdge` (phase-4 roof-edge export).
#[test]
fn roof_edge_types_are_reexported() {
    let e = crate::RoofEdge {
        kind: crate::RoofEdgeKind::Ridge,
        a: cadcraft_geom::Vec2::ZERO,
        b: cadcraft_geom::Vec2::new(1.0, 0.0),
        za: 4.45,
        zb: 4.45,
        ..Default::default()
    };
    assert_eq!(e.kind, crate::RoofEdgeKind::Ridge);

    // Serde round-trip: the re-exported type must serialize/deserialize through `serde_json`.
    let s = serde_json::to_string(&e).unwrap();
    let back: crate::RoofEdge = serde_json::from_str(&s).unwrap();
    assert_eq!(back.kind, crate::RoofEdgeKind::Ridge);
    assert_eq!(back.a, cadcraft_geom::Vec2::ZERO);
    assert_eq!(back.b, cadcraft_geom::Vec2::new(1.0, 0.0));
    assert_eq!(back.za, 4.45);
    assert_eq!(back.zb, 4.45);
}

/// All five RoofEdgeKind variants survive a serde round-trip through the re-exported type.
#[test]
fn roof_edge_all_kinds_roundtrip() {
    let kinds =
        [crate::RoofEdgeKind::Eave, crate::RoofEdgeKind::Ridge, crate::RoofEdgeKind::Hip, crate::RoofEdgeKind::Valley, crate::RoofEdgeKind::GableEnd];
    for kind in kinds {
        let e = crate::RoofEdge {
            kind,
            a: cadcraft_geom::Vec2::new(0.0, 0.0),
            b: cadcraft_geom::Vec2::new(2.0, 3.0),
            za: 1.5,
            zb: 2.5,
            ..Default::default()
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: crate::RoofEdge = serde_json::from_str(&s).unwrap();
        assert_eq!(back.kind, kind, "kind round-trip failed");
        assert_eq!(back.a, e.a);
        assert_eq!(back.b, e.b);
        assert_eq!(back.za, e.za);
        assert_eq!(back.zb, e.zb);
    }
}

/// A degenerate zero-length roof edge (a == b) serializes and round-trips correctly.
#[test]
fn roof_edge_zero_length_roundtrips() {
    let pt = cadcraft_geom::Vec2::new(5.0, 5.0);
    let e = crate::RoofEdge { kind: crate::RoofEdgeKind::Ridge, a: pt, b: pt, za: 3.0, zb: 3.0, ..Default::default() };
    let s = serde_json::to_string(&e).unwrap();
    let back: crate::RoofEdge = serde_json::from_str(&s).unwrap();
    assert_eq!(back.a, pt);
    assert_eq!(back.b, pt);
    assert_eq!(back.za, 3.0);
    assert_eq!(back.zb, 3.0);
}

/// Roof edges with huge coordinate values and NaN heights survive a round-trip.
#[test]
fn roof_edge_huge_and_nan_values_roundtrip() {
    let huge = crate::RoofEdge {
        kind: crate::RoofEdgeKind::Eave,
        a: cadcraft_geom::Vec2::new(1e15, -1e15),
        b: cadcraft_geom::Vec2::new(-1e15, 1e15),
        za: 1e12,
        zb: -1e12,
        ..Default::default()
    };
    let s = serde_json::to_string(&huge).unwrap();
    let back: crate::RoofEdge = serde_json::from_str(&s).unwrap();
    assert_eq!(back.a, huge.a);
    assert_eq!(back.b, huge.b);
    assert_eq!(back.za, huge.za);
    assert_eq!(back.zb, huge.zb);

    let nan = crate::RoofEdge {
        kind: crate::RoofEdgeKind::Hip,
        a: cadcraft_geom::Vec2::new(f64::NAN, 1.0),
        b: cadcraft_geom::Vec2::new(2.0, f64::NAN),
        za: f64::NAN,
        zb: f64::NAN,
        ..Default::default()
    };
    // serde_json serializes f64::NAN as null; verify the JSON contains nulls for the NaN fields
    // and finite values for the others. A full round-trip is not possible without NaN-aware
    // deserialization, so we check the serialized form directly.
    let s = serde_json::to_string(&nan).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["a"]["x"].is_null(), "a.x should be null (NaN)");
    assert_eq!(v["a"]["y"].as_f64(), Some(1.0));
    assert_eq!(v["b"]["x"].as_f64(), Some(2.0));
    assert!(v["b"]["y"].is_null(), "b.y should be null (NaN)");
    assert!(v["za"].is_null(), "za should be null (NaN)");
    assert!(v["zb"].is_null(), "zb should be null (NaN)");
}

/// A sloped roof edge (za != zb) preserves its height difference through a round-trip.
#[test]
fn roof_edge_sloped_heights_roundtrip() {
    let e = crate::RoofEdge {
        kind: crate::RoofEdgeKind::Valley,
        a: cadcraft_geom::Vec2::new(0.0, 0.0),
        b: cadcraft_geom::Vec2::new(10.0, 0.0),
        za: 2.4,
        zb: 4.45,
        ..Default::default()
    };
    let s = serde_json::to_string(&e).unwrap();
    let back: crate::RoofEdge = serde_json::from_str(&s).unwrap();
    assert_eq!(back.za, 2.4);
    assert_eq!(back.zb, 4.45);
    // The height difference is preserved exactly.
    assert_eq!(back.zb - back.za, e.zb - e.za);
}
