use cadcraft_doc::Drawing;
use cadcraft_doc::arch::{ArchId, ArchKind, ArchView, DimKind, ViewDim, ViewGeometry, ViewKind, ViewLine, ViewRing};
use cadcraft_io::{read_dxf, write_dxf};

fn view_geometry() -> ViewGeometry {
    let mut g = ViewGeometry::default();
    g.revision = 1;
    g.extent = [0.0, 0.0, 10.0, 8.0];
    g.lines = vec![
        ViewLine { a: [0.0, 0.0], b: [10.0, 0.0], kind: ArchKind::Wall, object: ArchId(0), ..Default::default() },
        ViewLine { a: [0.0, 0.0], b: [0.0, 8.0], kind: ArchKind::Wall, object: ArchId(1), ..Default::default() },
    ];
    g.rings = vec![ViewRing {
        kind: ArchKind::Room,
        object: ArchId(2),
        material: "cut".into(),
        points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]],
        ..Default::default()
    }];
    g.dims = vec![ViewDim {
        kind: DimKind::Width,
        a: [0.0, -1.0],
        b: [10.0, -1.0],
        at: [5.0, -2.0],
        value: 10.0,
        text: "10.0".into(),
        ..Default::default()
    }];
    g.error = None;
    g
}

fn view_drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut front = ArchView {
        name: "Front".into(),
        kind: ViewKind::Elevation,
        look: [0.0, 1.0],
        cut: None,
        depth: Some(12.0),
        scope: Some([[0.0, 0.0], [12.0, 10.0]]),
        scale: 50.0,
        insert: [100.0, 0.0],
        dimensions: true,
        layer: "A-ELEV".into(),
        generated: Some(view_geometry()),
        ..Default::default()
    };
    let mut section = ArchView {
        name: "Section A".into(),
        kind: ViewKind::Section,
        look: [0.0, 1.0],
        cut: Some([[0.0, 0.0], [8.0, 0.0]]),
        depth: Some(15.0),
        scope: Some([[0.0, -2.0], [12.0, 12.0]]),
        scale: 100.0,
        insert: [200.0, 0.0],
        dimensions: true,
        layer: "A-SECT".into(),
        generated: Some(view_geometry()),
        ..Default::default()
    };
    front.id = ArchId(0);
    section.id = ArchId(1);
    d.arch.views.push(front);
    d.arch.views.push(section);
    d.arch.next_view_id = 2;
    d
}

#[test]
fn views_round_trip_through_the_v7_arch_record() {
    let d = view_drawing();
    let text = write_dxf(&d);
    assert!(text.contains("CADCRAFT_ARCH"), "missing arch record");
    assert!(text.contains("\"version\":8") || text.contains("\"version\": 8"), "record should be v8");

    let back = read_dxf(text.as_bytes()).unwrap();
    assert_eq!(back.arch.views.len(), 2, "views lost on read");
    assert_eq!(back.arch.next_view_id, 2, "next_view_id should persist");

    let first = &back.arch.views[0];
    assert_eq!(first.id, ArchId(0));
    assert_eq!(first.name, "Front");
    assert_eq!(first.kind, ViewKind::Elevation);
    assert_eq!(first.look, [0.0, 1.0]);
    assert_eq!(first.cut, None);
    assert_eq!(first.depth, Some(12.0));
    assert_eq!(first.scope, Some([[0.0, 0.0], [12.0, 10.0]]));
    assert_eq!(first.scale, 50.0);
    assert_eq!(first.insert, [100.0, 0.0]);
    assert!(first.dimensions);
    assert_eq!(first.layer, "A-ELEV");
    assert!(first.generated.is_none(), "generated geometry must not round-trip");

    let second = &back.arch.views[1];
    assert_eq!(second.id, ArchId(1));
    assert_eq!(second.name, "Section A");
    assert_eq!(second.kind, ViewKind::Section);
    assert_eq!(second.cut, Some([[0.0, 0.0], [8.0, 0.0]]));
    assert_eq!(second.layer, "A-SECT");
    assert_eq!(second.scale, 100.0);
    assert!(second.generated.is_none(), "generated geometry must not round-trip");
}

#[test]
fn a_v6_record_reads_with_no_views() {
    let d = view_drawing();
    let text = write_dxf(&d);
    let old = text.replace("\"version\": 8", "\"version\": 6").replace("\"version\":8", "\"version\": 6");
    let back = read_dxf(old.as_bytes()).unwrap();
    assert!(back.arch.views.is_empty(), "v6 payload should clear views");
    assert_eq!(back.arch.next_view_id, 0, "v6 payload should reset next_view_id");
}

#[test]
fn view_proxies_are_written_on_their_layers() {
    let d = view_drawing();
    let text = write_dxf(&d);
    for layer in ["A-ELEV", "A-SECT", "A-SECT-MCUT", "A-ANNO-DIMS"] {
        assert!(text.contains(layer), "missing view proxy layer {layer:?} in output");
    }
    assert!(text.matches("CADCRAFT-VIEW").count() > 0, "view proxy marker is missing");
    assert!(text.matches("LINE").count() >= 2, "expected generated view line proxies");
    assert!(text.contains("LWPOLYLINE"), "expected generated section cut proxies");
}

#[test]
fn view_proxies_are_dropped_on_read_and_never_duplicated() {
    let d = view_drawing();
    let first_text = write_dxf(&d);
    let first = read_dxf(first_text.as_bytes()).unwrap();
    let second_text = write_dxf(&first);
    let second = read_dxf(second_text.as_bytes()).unwrap();

    let count_proxy = |drawing: &Drawing| {
        drawing
            .model
            .iter()
            .filter(|e| matches!(&e.kind, cadcraft_doc::EntityKind::Line(_) | cadcraft_doc::EntityKind::LwPolyline(_)))
            .filter(|e| e.common.xdata.iter().any(|t| t.code == 1000 && t.value.trim() == "CADCRAFT-VIEW"))
            .count()
    };

    assert_eq!(count_proxy(&first), 0, "view proxy stayed in the model after read");
    assert_eq!(count_proxy(&second), 0, "view proxy duplicated on re-save");
    assert_eq!(first.arch.views.len(), 2, "views disappeared on first read");
    assert_eq!(second.arch.views.len(), 2, "views disappeared on second read");
}
