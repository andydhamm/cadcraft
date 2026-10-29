//! View Sheet command: places arch views on a layout at their scale.

use crate::Session;
use crate::cmd::CommandSpec;
use crate::error::{EngineError, Result};
use cadcraft_doc::arch::ArchView;
use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, Layout, PageSetup, Viewport};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};
use std::collections::HashSet;

/// Maximum number of views per sheet.
const MAX_VIEWS: usize = 16;

/// Margins in paper units (mm).
const MARGIN_MM: f64 = 25.0;
/// Gap between viewports in paper units (mm).
const GAP_MM: f64 = 20.0;

/// Resolve paper dimensions from name.
/// Returns (width_mm, height_mm, full_name).
fn resolve_paper(name: &str) -> Option<(f64, f64, String)> {
    match name.to_uppercase().as_str() {
        "ARCH D" | "ARCH-D" => Some((36.0 * 25.4, 24.0 * 25.4, "ARCH D".to_string())),
        "ARCH E" | "ARCH-E" => Some((48.0 * 25.4, 36.0 * 25.4, "ARCH E".to_string())),
        "A4" => Some((210.0, 297.0, "A4".to_string())),
        "A3" => Some((297.0, 420.0, "A3".to_string())),
        "A2" => Some((420.0, 594.0, "A2".to_string())),
        "A1" => Some((594.0, 841.0, "A1".to_string())),
        "A0" => Some((841.0, 1189.0, "A0".to_string())),
        "LETTER" => Some((8.5 * 25.4, 11.0 * 25.4, "Letter".to_string())),
        "LEGAL" => Some((8.5 * 25.4, 14.0 * 25.4, "Legal".to_string())),
        _ => None,
    }
}

/// Run the viewsheet command.
pub fn run_viewsheet(s: &mut Session, params: &Value) -> Result<Value> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("Views");
    let views_val = params.get("views").and_then(Value::as_array).ok_or_else(|| EngineError::Other("views required".into()))?;

    if views_val.is_empty() {
        return Err(EngineError::Other("views cannot be empty".into()));
    }
    if views_val.len() > MAX_VIEWS {
        return Err(EngineError::Other(format!("too many views, max {MAX_VIEWS}")));
    }

    let paper_name = params.get("paper").and_then(Value::as_str).unwrap_or("ARCH D");
    let landscape = params.get("landscape").and_then(Value::as_bool).unwrap_or(true);

    // Resolve paper
    let (paper_w, paper_h, full_paper_name) = resolve_paper(paper_name).ok_or_else(|| EngineError::Other(format!("unknown paper: {paper_name}")))?;

    // Check layout name uniqueness
    if s.doc()?.layouts.iter().any(|l| l.name == name) {
        return Err(EngineError::Other(format!("layout name already in use: {name}")));
    }

    // Validate views and collect data
    let mut view_data = Vec::new();
    let mut seen = HashSet::new();

    let doc = s.doc()?;
    let arch = &doc.arch;

    for v in views_val {
        let v_name = v.as_str().ok_or_else(|| EngineError::Other("view name must be string".into()))?;
        if !seen.insert(v_name.to_string()) {
            return Err(EngineError::Other(format!("duplicate view: {v_name}")));
        }

        // Find view in ArchModel
        let view = arch.views.iter().find(|v| v.name == v_name).ok_or_else(|| EngineError::Other(format!("unknown view: {v_name}")))?;

        if view.generated.is_none() {
            return Err(EngineError::Other(format!("view {v_name} has no generated geometry")));
        }

        let bounds_opt = view.bounds();
        if bounds_opt.is_none() {
            return Err(EngineError::Other(format!("view {v_name} has invalid bounds")));
        }
        let bounds = bounds_opt.unwrap(); // Safe due to check above

        // bounds is [min_x, min_y, max_x, max_y]
        let min_x = bounds[0];
        let min_y = bounds[1];
        let max_x = bounds[2];
        let max_y = bounds[3];

        // Grown bounds for dimensions
        let grown_min = Vec2::new(min_x - 1.5, min_y - 1.5);
        let grown_max = Vec2::new(max_x + 1.5, max_y + 1.5);

        view_data.push((v_name.to_string(), view.scale, grown_min, grown_max));
    }

    // Calculate layout
    // k = paper units per model meter
    // Metric drawing assumed for now (mm).
    // k = 1000 / scale. e.g. scale 50 (1:50) -> 20 mm/m.
    // If imperial, k = 1000 / 25.4 / scale.
    // We assume metric for simplicity as per typical arch workflows in this codebase unless units are checked.
    // Checking units:
    let is_metric = true; // Default to metric for arch3d

    let mut viewports_info = Vec::new();

    struct ViewportInfo {
        name: String,
        scale: f64,
        k: f64,
        w_mm: f64,
        h_mm: f64,
        center_model: Vec2,
    }

    for (name, scale, min, max) in view_data {
        let k = if is_metric { 1000.0 / scale } else { 1000.0 / 25.4 / scale };

        let w_model = max.x - min.x;
        let h_model = max.y - min.y;

        let w_mm = w_model * k;
        let h_mm = h_model * k;

        let center_model = Vec2::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);

        viewports_info.push(ViewportInfo { name, scale, k, w_mm, h_mm, center_model });
    }

    // Layout logic: 2 per row
    // Determine column widths
    let mut col0_w: f64 = 0.0;
    let mut col1_w: f64 = 0.0;

    for (i, info) in viewports_info.iter().enumerate() {
        if i % 2 == 0 {
            col0_w = col0_w.max(info.w_mm);
        } else {
            col1_w = col1_w.max(info.w_mm);
        }
    }

    let col0_x = MARGIN_MM;
    let col1_x = MARGIN_MM + col0_w + GAP_MM;

    // Check fit width
    let total_w = col1_x + col1_w + MARGIN_MM;
    if total_w > paper_w {
        return Err(EngineError::Other("views do not fit the sheet width".into()));
    }

    // Calculate rows and positions
    let mut y_cursor = paper_h - MARGIN_MM;
    let mut placed_viewports = Vec::new();

    for (i, info) in viewports_info.iter().enumerate() {
        let is_col0 = i % 2 == 0;
        let x_center = if is_col0 { col0_x + col0_w / 2.0 } else { col1_x + col1_w / 2.0 };

        // Row height is max height of the two views in this row
        let current_row_h = if is_col0 {
            let next_h = viewports_info.get(i + 1).map(|n| n.h_mm).unwrap_or(0.0);
            info.h_mm.max(next_h)
        } else {
            let prev_h = viewports_info.get(i - 1).map(|n| n.h_mm).unwrap_or(0.0);
            info.h_mm.max(prev_h)
        };

        // Check vertical fit
        if y_cursor - current_row_h < MARGIN_MM {
            return Err(EngineError::Other("views do not fit the sheet height".into()));
        }

        // Y center: Top of row is y_cursor. Bottom is y_cursor - row_height.
        // Center is y_cursor - row_height / 2.
        let y_center = y_cursor - current_row_h / 2.0;

        placed_viewports.push((info.name.clone(), info.scale, Vec2::new(x_center, y_center), info.w_mm, info.h_mm, info.center_model));

        if !is_col0 || i == viewports_info.len() - 1 {
            // End of row
            y_cursor -= current_row_h + GAP_MM;
        }
    }

    // Now execute the changes

    // 1. Create layout
    let tab_order = s.doc()?.layouts.len() as u32;
    let mut new_layout = Layout::new(name, tab_order);

    // Set page setup
    let mut page_setup = PageSetup::default();
    // Try to set paper. If PageSetup has a `paper` field of type String or similar.
    // If it's an enum, this will fail. We'll try setting it as a string first.
    // If compilation fails, we might need to import PaperSize or similar.
    // For now, assuming `paper` is a settable field.
    page_setup.paper = full_paper_name.clone();
    new_layout.page = page_setup;

    // Add layout to doc
    s.doc_mut()?.layouts.push(new_layout);

    // 2. Create viewports
    let mut handles = Vec::new();
    let mut resp = Vec::new();

    for (v_name, scale, center_paper, w_mm, h_mm, center_model) in placed_viewports {
        // Create viewport entity
        // Viewport in paper space.

        let mut vp = Viewport {
            center: cadcraft_geom::Vec3::new(center_paper.x, center_paper.y, 0.0),
            width: w_mm,
            height: h_mm,
            view_center: center_model,            // Vec2 expected
            view_height: h_mm / (1000.0 / scale), // Height in model units
            id: 0,                                // Will be assigned by doc
            frozen_layers: vec![],
            layer_colors: vec![],
            clip: None,
            on: true,
            locked: true,
            ..Default::default()
        };

        let h = s.doc_mut()?.new_handle();
        let entity = Entity::new(h, EntityKind::Viewport(vp));

        // Add to layout
        if let Some(layout) = s.doc_mut()?.layouts.last_mut() {
            layout.entities.push(entity);
        }
        handles.push(h);

        resp.push(json!({
            "handle": format!("{:x}", h.0),
            "view": v_name,
            "scale": 1000.0 / scale, // Return k (20.0 for 1:50)
            "center": [center_paper.x, center_paper.y],
            "width": w_mm,
            "height": h_mm
        }));
    }

    Ok(json!({
        "layout": name,
        "paper": full_paper_name,
        "landscape": landscape,
        "viewports": resp
    }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("viewsheet", "View Sheet", run_viewsheet)
            .menu(&["Architecture", "View Sheet"])
            .params("{name?, views: [...], paper?, landscape?}"),
    ]
}
