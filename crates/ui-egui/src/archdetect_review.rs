//! The Detection Review panel: the candidates of an ARCHDETECT dry run (walls, openings, rooms)
//! with a checkbox each, to accept the checked ones. A thin view over the engine's `archdetect`
//! command: it acts through [`CadApp::run`] and keeps the reply in [`crate::UiState`].

use egui::RichText;
use serde_json::{Value, json};

use crate::CadApp;

/// Candidates with at least this confidence start checked.
const CHECK_CONFIDENCE: f64 = 0.5;

/// What a click in the panel asks for, run once the panel is drawn.
enum Act {
    Detect,
    SelectAll,
    Clear,
    Accept,
    Zoom(Value),
}

/// The panel's last drawn rect, for the screenshot test.
pub fn review_rect(app: &CadApp) -> Option<egui::Rect> {
    app.ui.archdetect_rect
}

fn list<'a>(reply: &'a Value, key: &str) -> &'a [Value] {
    reply.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn id_of(c: &Value) -> Option<&str> {
    c.get("id").and_then(Value::as_str)
}

fn num(c: &Value, key: &str) -> f64 {
    c.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0)
}

fn point(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    let (x, y) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
    (x.is_finite() && y.is_finite()).then_some([x, y])
}

/// Every candidate id of a reply, walls then openings then rooms.
fn all_ids(reply: &Value) -> impl Iterator<Item = String> + '_ {
    ["walls", "openings", "rooms"].into_iter().flat_map(|k| list(reply, k)).filter_map(|c| id_of(c).map(str::to_owned))
}

/// A length in metres as the drawing's units show it: millimetres for a millimetre drawing.
fn length_text(reply: &Value, metres: f64) -> String {
    if reply.get("units").and_then(Value::as_str) == Some("mm") { format!("{:.0} mm", metres * 1000.0) } else { format!("{metres:.2} m") }
}

/// The view that frames a candidate, in drawing units: its centre and a height with margin.
fn zoom_params(reply: &Value, c: &Value) -> Option<Value> {
    let scale = reply.get("scale").and_then(Value::as_f64).filter(|s| s.is_finite() && *s > 0.0)?;
    let mut pts: Vec<[f64; 2]> = Vec::new();
    if let Some(poly) = c.get("polygon").and_then(Value::as_array) {
        pts.extend(poly.iter().filter_map(|p| point(Some(p))));
    }
    pts.extend(["start", "end", "center"].into_iter().filter_map(|k| point(c.get(k))));
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &pts {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if pts.is_empty() {
        return None;
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(num(c, "width")).max(num(c, "thickness")).max(1.0) * 1.6;
    Some(json!({"center": [(lo[0] + hi[0]) / 2.0 / scale, (lo[1] + hi[1]) / 2.0 / scale], "height": size / scale}))
}

/// Uncheck the openings whose wall is unchecked.
fn uncheck_orphans(app: &mut CadApp) {
    let Some(reply) = app.ui.archdetect.as_ref() else { return };
    let orphans: Vec<String> = list(reply, "openings")
        .iter()
        .filter(|o| o.get("host").and_then(Value::as_str).is_some_and(|h| !app.ui.archdetect_checked.contains(h)))
        .filter_map(|o| id_of(o).map(str::to_owned))
        .collect();
    for id in orphans {
        app.ui.archdetect_checked.remove(&id);
    }
}

fn detect(app: &mut CadApp) {
    match app.run("archdetect", json!({"dryRun": true})) {
        Ok(reply) => {
            app.ui.archdetect_checked = ["walls", "openings", "rooms"]
                .into_iter()
                .flat_map(|k| list(&reply, k))
                .filter(|c| num(c, "confidence") >= CHECK_CONFIDENCE)
                .filter_map(|c| id_of(c).map(str::to_owned))
                .collect();
            app.ui.archdetect = Some(reply);
            app.ui.archdetect_note.clear();
            uncheck_orphans(app);
        }
        Err(e) => app.ui.archdetect_note = e,
    }
}

fn accept(app: &mut CadApp) {
    let ids: Vec<&String> = app.ui.archdetect_checked.iter().collect();
    match app.run("archdetect", json!({"accept": ids})) {
        Ok(r) => {
            let n = |k: &str| r.get(k).and_then(Value::as_u64).unwrap_or(0);
            app.ui.archdetect_note = format!("{} / {} / {}", n("walls"), n("openings"), n("rooms"));
            app.ui.archdetect = None;
            app.ui.archdetect_checked.clear();
        }
        Err(e) => app.ui.archdetect_note = e,
    }
}

/// One group of rows under a collapsing header.
fn group(app: &mut CadApp, ui: &mut egui::Ui, reply: &Value, key: &str, title: &str, act: &mut Option<Act>) {
    let t = crate::theme::Tokens::get();
    let rows = list(reply, key);
    egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
        for c in rows {
            let Some(id) = id_of(c) else { continue };
            ui.horizontal(|ui| {
                let mut on = app.ui.archdetect_checked.contains(id);
                if ui.checkbox(&mut on, "").changed() {
                    if on {
                        app.ui.archdetect_checked.insert(id.to_owned());
                    } else {
                        app.ui.archdetect_checked.remove(id);
                    }
                }
                let rules = c
                    .pointer("/evidence/rules")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                if ui.add(egui::Label::new(id).sense(egui::Sense::click())).on_hover_text(rules).clicked() {
                    *act = Some(Act::Zoom(c.clone()));
                }
                match key {
                    "walls" => {
                        ui.label(length_text(reply, num(c, "thickness")));
                    }
                    "openings" => {
                        ui.label(c.get("kind").and_then(Value::as_str).unwrap_or(""));
                        ui.label(length_text(reply, num(c, "width")));
                    }
                    _ => {
                        if let Some(name) = c.get("name").and_then(Value::as_str) {
                            ui.label(name);
                        }
                        ui.label(format!("{:.1} m²", num(c, "area")));
                    }
                }
                ui.label(RichText::new(format!("{:.0}%", num(c, "confidence") * 100.0)).color(t.text_dim)).on_hover_text(crate::tl!("Confidence"));
            });
        }
    });
}

/// The panel body.
pub fn section(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get();
    app.ui.archdetect_rect = Some(ui.max_rect());
    let mut act = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui.button(crate::tl!("Detect")).clicked() {
            act = Some(Act::Detect);
        }
        let has = app.ui.archdetect.is_some();
        if ui.add_enabled(has, egui::Button::new(crate::tl!("Select All"))).clicked() {
            act = Some(Act::SelectAll);
        }
        if ui.add_enabled(has, egui::Button::new(crate::tl!("Clear"))).clicked() {
            act = Some(Act::Clear);
        }
        let any = has && !app.ui.archdetect_checked.is_empty();
        if ui.add_enabled(any, egui::Button::new(crate::tl!("Accept Selected"))).clicked() {
            act = Some(Act::Accept);
        }
    });
    if !app.ui.archdetect_note.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(app.ui.archdetect_note.as_str()).color(t.text_dim));
        });
    }
    ui.add_space(6.0);
    if let Some(reply) = app.ui.archdetect.clone() {
        uncheck_orphans(app);
        group(app, ui, &reply, "walls", crate::tl!("Walls"), &mut act);
        // A wall unchecked just now unchecks its openings before they are drawn.
        uncheck_orphans(app);
        group(app, ui, &reply, "openings", crate::tl!("Openings"), &mut act);
        group(app, ui, &reply, "rooms", crate::tl!("Rooms"), &mut act);
        if let Some(Act::Zoom(c)) = &act {
            if let Some(p) = zoom_params(&reply, c) {
                if let Err(e) = app.run("view.set", p) {
                    app.ui.archdetect_note = e;
                }
            }
            act = None;
        }
    }
    match act {
        Some(Act::Detect) => detect(app),
        Some(Act::SelectAll) => {
            if let Some(reply) = app.ui.archdetect.as_ref() {
                app.ui.archdetect_checked = all_ids(reply).collect();
            }
        }
        Some(Act::Clear) => app.ui.archdetect_checked.clear(),
        Some(Act::Accept) => accept(app),
        _ => {}
    }
}
