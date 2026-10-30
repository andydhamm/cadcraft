//! The Detection Review panel: Detect lists the ARCHDETECT candidates, Accept Selected creates
//! the checked ones, and a wall's openings follow its checkbox.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

fn pline(app: &mut CadApp, pts: Value, closed: bool) {
    app.session.execute("pline", &json!({"vertices": pts, "closed": closed})).unwrap();
}

/// A 10 x 5 m plan, 0.2 m exterior walls and a 0.1 m partition at x = 5 with a door gap.
fn two_rooms() -> CadApp {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.session.doc_mut().unwrap().header.set_i64("INSUNITS", 6);
    pline(&mut app, json!([[-0.1, -0.1], [10.1, -0.1], [10.1, 5.1], [-0.1, 5.1]]), true);
    pline(&mut app, json!([[0.1, 0.1], [9.9, 0.1], [9.9, 4.9], [0.1, 4.9]]), true);
    for x in [4.95, 5.05] {
        pline(&mut app, json!([[x, 0.1], [x, 2.0]]), false);
        pline(&mut app, json!([[x, 3.0], [x, 4.9]]), false);
    }
    pline(&mut app, json!([[4.95, 2.0], [5.05, 2.0]]), false);
    pline(&mut app, json!([[4.95, 3.0], [5.05, 3.0]]), false);
    app.run("archdetect.review", Value::Null).unwrap();
    app
}

fn harness(app: CadApp) -> Harness<'static, CadApp> {
    let mut h = Harness::builder().with_size(vec2(1400.0, 900.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn ids(reply: &Value, key: &str) -> Vec<String> {
    reply[key].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str().map(str::to_owned)).collect()
}

#[test]
fn review_panel_lists_the_candidates_of_a_dry_run() {
    let mut h = harness(two_rooms());
    h.get_by_label("Detect").click();
    h.run_steps(4);
    let reply = h.state().ui.archdetect.clone().expect("a dry-run reply");
    let all: Vec<String> = ["walls", "openings", "rooms"].iter().flat_map(|k| ids(&reply, k)).collect();
    assert!(!ids(&reply, "walls").is_empty() && !ids(&reply, "rooms").is_empty(), "{reply}");
    for id in &all {
        h.get_by_label(id);
    }
    h.get_by_label("Walls");
    h.get_by_label("Rooms");
    // Every candidate with confidence >= 0.5 starts checked.
    let confident = |c: &Value| c["confidence"].as_f64().is_some_and(|v| v >= 0.5);
    let sure = ["walls", "rooms"].iter().flat_map(|k| reply[*k].as_array().into_iter().flatten()).filter(|c| confident(c)).count();
    assert!(h.state().ui.archdetect_checked.len() >= sure);
    assert!(cadcraft_ui_egui::archdetect_review::review_rect(h.state()).is_some());
}

#[test]
fn accept_selected_creates_only_the_checked_candidates() {
    let mut h = harness(two_rooms());
    h.get_by_label("Detect").click();
    h.run_steps(4);
    let reply = h.state().ui.archdetect.clone().expect("a dry-run reply");
    let checked = h.state().ui.archdetect_checked.clone();
    let walls: Vec<String> = ids(&reply, "walls").into_iter().filter(|w| checked.contains(w)).collect();
    let skipped = walls.first().cloned().expect("a checked wall");
    h.state_mut().ui.archdetect_checked.remove(&skipped);
    h.run_steps(2);
    // Openings hosted by the unchecked wall are not accepted either.
    let openings = reply["openings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["id"].as_str().is_some_and(|i| checked.contains(i)) && o["host"] != skipped.as_str())
        .count();
    h.get_by_label("Accept Selected").click();
    h.run_steps(4);
    let doc = h.state().session.doc().unwrap();
    assert_eq!(doc.arch.walls.len(), walls.len() - 1, "{}", h.state().ui.archdetect_note);
    assert_eq!(doc.arch.openings.len(), openings);
    assert!(h.state().ui.archdetect.is_none() && h.state().ui.archdetect_checked.is_empty());
}

#[test]
fn unchecking_a_wall_unchecks_its_openings() {
    let mut h = harness(two_rooms());
    h.get_by_label("Detect").click();
    h.run_steps(4);
    let reply = h.state().ui.archdetect.clone().expect("a dry-run reply");
    let opening = reply["openings"].as_array().and_then(|a| a.first()).expect("a door candidate");
    let (host, id) = (opening["host"].as_str().unwrap().to_owned(), opening["id"].as_str().unwrap().to_owned());
    h.state_mut().ui.archdetect_checked.extend([host.clone(), id.clone()]);
    h.run_steps(2);
    assert!(h.state().ui.archdetect_checked.contains(&id));
    h.state_mut().ui.archdetect_checked.remove(&host);
    h.run_steps(3);
    assert!(!h.state().ui.archdetect_checked.contains(&id));
}
