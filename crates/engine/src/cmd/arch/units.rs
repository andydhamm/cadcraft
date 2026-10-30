//! Converting a whole drawing to metres, the unit the arch model lives in.
use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_geom::{Mat3, Vec2};

/// Convert `d` to metres in place: `scale` is the factor from its current unit to metres (0.001
/// for a millimetre drawing).
///
/// Every model-space entity is scaled about the origin (an INSERT's position and scale factors
/// follow, so its block content does too; block definitions are left alone). The header's
/// DIMSCALE, LTSCALE and TEXTSIZE and every dimension style's scale are multiplied by `scale`
/// when positive. Each viewport keeps framing the same content: its model view centre and height
/// are scaled; paper space itself is not. `$INSUNITS` becomes 6 and `$MEASUREMENT` 1.
///
/// Returns the number of model-space entities converted. A non-finite or non-positive `scale` is
/// refused before anything changes. Only `d` is touched, so the caller's undo step covers it.
pub(crate) fn convert_drawing_to_metres(d: &mut Drawing, scale: f64) -> Result<usize, String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(format!("the unit scale must be a positive number, not {scale}"));
    }
    let m = Mat3::scale_about(Vec2::ZERO, scale);
    let mut n = 0;
    for h in d.model.handles() {
        if d.modify_entity(h, |e| e.kind.transform(&m)).is_ok() {
            n += 1;
        }
    }

    for name in ["DIMSCALE", "LTSCALE", "TEXTSIZE"] {
        let v = d.header.f64(name, 0.0);
        if v > 0.0 {
            d.header.set_f64(name, v * scale);
        }
    }
    for s in &mut d.dim_styles {
        if s.scale > 0.0 {
            s.scale *= scale;
        }
    }

    for layout in &mut d.layouts {
        let handles = layout.entities.handles();
        for h in handles {
            layout.entities.modify(h, |e| {
                if let EntityKind::Viewport(v) = &mut e.kind {
                    v.view_center = Vec2::new(v.view_center.x * scale, v.view_center.y * scale);
                    v.view_height *= scale;
                }
            });
        }
    }

    d.header.set_i64("INSUNITS", 6);
    d.header.set_i64("MEASUREMENT", 1);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Arc, Block, Common, DimKind, Dimension, Entity, Handle, Insert, Layout, Line, MText, Space, Viewport};
    use cadcraft_geom::Vec3;

    fn mm_drawing() -> Drawing {
        let mut d = Drawing::new_metric();
        let mut b = Block::new("CHAIR");
        b.entities.push(Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(400.0, 0.0, 0.0) })));
        d.blocks.insert("CHAIR".into(), std::sync::Arc::new(b));
        d
    }

    fn add(d: &mut Drawing, k: EntityKind) -> Handle {
        d.add(&Space::Model, Common::default(), k).expect("add")
    }

    fn viewport(center: Vec2, height: f64) -> EntityKind {
        EntityKind::Viewport(Viewport { view_center: center, view_height: height, ..Viewport::default() })
    }

    #[test]
    fn model_space_and_inserts_scale_to_metres() {
        let mut d = mm_drawing();
        d.header.set_f64("TEXTSIZE", 250.0);
        d.header.set_f64("DIMSCALE", 100.0);
        let line = add(&mut d, EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(12000.0, 8000.0, 0.0) }));
        let arc = add(&mut d, EntityKind::Arc(Arc { center: Vec3::new(1000.0, 2000.0, 0.0), radius: 500.0, start: 0.0, end: 1.5 }));
        let text = add(
            &mut d,
            EntityKind::MText(MText {
                insert: Vec3::new(3000.0, 4000.0, 0.0),
                height: 250.0,
                width: 0.0,
                attach: 1,
                rotation: 0.0,
                style: "Standard".into(),
                contents: "Kitchen".into(),
                line_spacing: 1.0,
                line_spacing_exact: false,
                columns: None,
            }),
        );
        let dim = add(
            &mut d,
            EntityKind::Dimension(Dimension {
                kind: DimKind::Aligned,
                defpt: Vec3::new(0.0, 500.0, 0.0),
                text_mid: Vec3::new(6000.0, 500.0, 0.0),
                p13: Vec3::ZERO,
                p14: Vec3::new(12000.0, 0.0, 0.0),
                p15: Vec3::ZERO,
                p16: Vec3::ZERO,
                text: String::new(),
                style: "Standard".into(),
                measurement: 12000.0,
                text_rotation: 0.0,
                user_text_pos: false,
                block: None,
                overrides: Default::default(),
                assoc: Vec::new(),
                marks: Default::default(),
            }),
        );
        let ins = add(
            &mut d,
            EntityKind::Insert(Insert {
                block: "CHAIR".into(),
                insert: Vec3::new(2000.0, 1000.0, 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        );
        let before = d.blocks.get("CHAIR").cloned();

        let n = convert_drawing_to_metres(&mut d, 0.001).expect("convert");
        assert_eq!(n, 5);

        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let kind = |h: Handle| d.entity(h).map(|e| e.kind.clone());
        match kind(line) {
            Some(EntityKind::Line(l)) => assert!(near(l.b.x, 12.0) && near(l.b.y, 8.0)),
            other => panic!("line: {other:?}"),
        }
        match kind(arc) {
            Some(EntityKind::Arc(a)) => assert!(near(a.radius, 0.5) && near(a.center.x, 1.0) && near(a.center.y, 2.0)),
            other => panic!("arc: {other:?}"),
        }
        match kind(text) {
            Some(EntityKind::MText(t)) => assert!(near(t.height, 0.25) && near(t.insert.x, 3.0)),
            other => panic!("mtext: {other:?}"),
        }
        match kind(dim) {
            Some(EntityKind::Dimension(x)) => assert!(near(x.p14.x, 12.0) && near(x.text_mid.x, 6.0)),
            other => panic!("dimension: {other:?}"),
        }
        match kind(ins) {
            Some(EntityKind::Insert(i)) => {
                assert!(near(i.insert.x, 2.0) && near(i.insert.y, 1.0));
                assert!(near(i.scale.x, 0.001) && near(i.scale.y, 0.001));
            }
            other => panic!("insert: {other:?}"),
        }
        assert_eq!(d.blocks.get("CHAIR").cloned(), before, "block definitions are untouched");
        assert!(near(d.header.f64("TEXTSIZE", 0.0), 0.25));
        assert!(near(d.header.f64("DIMSCALE", 0.0), 0.1));
        assert_eq!(d.header.i64("INSUNITS", 0), 6);
        assert_eq!(d.header.i64("MEASUREMENT", 0), 1);
    }

    #[test]
    fn viewports_keep_framing_the_same_content() {
        let mut d = mm_drawing();
        let mut layout = Layout::new("Sheet", 1);
        let line = Entity::new(Handle(0x200), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(297.0, 0.0, 0.0) }));
        layout.entities.push(line);
        layout.entities.push(Entity::new(Handle(0x201), viewport(Vec2::new(6000.0, 4000.0), 9000.0)));
        d.layouts.push(layout);

        convert_drawing_to_metres(&mut d, 0.001).expect("convert");

        let layout = d.layout("Sheet").expect("layout");
        match layout.entities.get(Handle(0x201)).map(|e| e.kind.clone()) {
            Some(EntityKind::Viewport(v)) => {
                assert!((v.view_center.x - 6.0).abs() < 1e-9 && (v.view_center.y - 4.0).abs() < 1e-9);
                assert!((v.view_height - 9.0).abs() < 1e-9);
            }
            other => panic!("viewport: {other:?}"),
        }
        match layout.entities.get(Handle(0x200)).map(|e| e.kind.clone()) {
            Some(EntityKind::Line(l)) => assert_eq!(l.b.x, 297.0, "paper space is not scaled"),
            other => panic!("paper line: {other:?}"),
        }
    }

    #[test]
    fn a_bad_scale_changes_nothing() {
        let mut d = mm_drawing();
        add(&mut d, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(5000.0, 0.0, 0.0) }));
        let before = d.clone();
        for s in [0.0, -0.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(convert_drawing_to_metres(&mut d, s).is_err(), "scale {s}");
            assert_eq!(d, before);
        }
    }
}
