//! The Detection Review panel and the 3D pane on the apartment sample, rendered with wgpu.
//!
//! Without `CADCRAFT_ARCHDETECT_SHOTS` the test is headless: Detect, count the rows, Accept
//! Selected. With it set, the test also renders `review.png`, `model3d.png` and `shots.json`
//! into that directory (acceptance check 10).
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, archdetect_review};
use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::{Value, json};

const SIZE: (f32, f32) = (1600.0, 1000.0);

fn apartment() -> CadApp {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/apartment.dxf");
    let bytes = std::fs::read(path).expect("read examples/apartment.dxf");
    let drawing = cadcraft_io::read(&bytes, "apartment.dxf").expect("parse apartment.dxf");
    let mut app = CadApp::new(Session::new(), Services::default());
    app.canvas.zoom_pending = false;
    app.session.open_drawing(drawing, "apartment.dxf", None);
    app
}

/// A software-or-hardware wgpu render state on the primary backends (Vulkan/Metal/DX12).
fn render_state() -> egui_wgpu::RenderState {
    let mut setup = egui_kittest::wgpu::default_wgpu_setup();
    if let egui_wgpu::WgpuSetup::CreateNew(create) = &mut setup {
        create.instance_descriptor.backends = egui_wgpu::wgpu::Backends::PRIMARY;
    }
    egui_kittest::wgpu::create_render_state(setup, Default::default())
}

fn rows(app: &CadApp) -> usize {
    let Some(reply) = app.ui.archdetect.as_ref() else { return 0 };
    ["walls", "openings", "rooms"].into_iter().filter_map(|k| reply.get(k).and_then(Value::as_array)).map(Vec::len).sum()
}

fn walls(app: &CadApp) -> usize {
    app.session.doc().map(|d| d.arch.walls.len()).unwrap_or(0)
}

fn rect_px(r: egui::Rect, ppp: f32) -> [i64; 4] {
    [r.min.x, r.min.y, r.max.x, r.max.y].map(|v| (v * ppp).round() as i64)
}

#[test]
fn review_panel_and_3d_pane_render_the_apartment() {
    let shots = std::env::var_os("CADCRAFT_ARCHDETECT_SHOTS").map(std::path::PathBuf::from);
    let mut app = apartment();
    let rs = shots.as_ref().map(|_| render_state());
    if let Some(rs) = &rs {
        app.set_wgpu(rs, None);
    }
    let mut builder = Harness::builder().with_size(vec2(SIZE.0, SIZE.1));
    if let Some(rs) = &rs {
        builder = builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs.clone()));
    }
    let mut h = builder.build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);

    h.state_mut().run("archdetect.review", json!({})).expect("open the review panel");
    h.run_steps(3);
    h.get_by_label("Detect").click();
    h.run_steps(3);
    let n = rows(h.state());
    assert!(n >= 10, "expected at least 10 candidate rows, got {n}");

    let mut doc = None;
    if let Some(dir) = &shots {
        std::fs::create_dir_all(dir).expect("create the shots dir");
        let img = h.render().expect("render the review panel");
        img.save(dir.join("review.png")).expect("save review.png");
        let ppp = img.width() as f32 / SIZE.0;
        let rect = archdetect_review::review_rect(h.state()).expect("the review panel was drawn");
        doc = Some((rect_px(rect, ppp), ppp));
    }

    h.get_by_label("Accept Selected").click();
    h.run_steps(3);
    assert_eq!(walls(h.state()), 6, "accepting the candidates should leave 6 walls");

    let (Some(dir), Some((review, ppp))) = (shots, doc) else { return };
    h.state_mut().run("ui.show3d", json!({"on": true})).expect("show the 3D pane");
    let mut drawn = false;
    for _ in 0..30 {
        h.run_steps(1);
        let p = &h.state().pane3d;
        if p.available && p.has_content && p.last_rect().is_some() {
            drawn = true;
            break;
        }
    }
    assert!(drawn, "the 3D pane never drew (no adapter, or no scene content)");
    h.run_steps(2);
    let img = h.render().expect("render the 3D pane");
    img.save(dir.join("model3d.png")).expect("save model3d.png");
    let pane = h.state().pane3d.last_rect().expect("the 3D pane rect");
    let shots_json = json!({
        "review": {"rect": review, "rows": n},
        "pane3d": {"rect": rect_px(pane, ppp)},
    });
    std::fs::write(dir.join("shots.json"), serde_json::to_vec_pretty(&shots_json).expect("serialize")).expect("write shots.json");
}
