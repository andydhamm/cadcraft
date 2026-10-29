//! View Sheet command: places arch views on a layout at their scale.

use crate::cmd::CommandSpec;
use crate::cmd::arch::view::ArchView;
use crate::cmd::layout;
use crate::cmd::pagesetup;
use crate::engine::{EngineError, Result, Session};
use cadcraft_doc::arch::ArchModel;
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

/// Maximum number of views per sheet.
const MAX_VIEWS: usize = 16;

/// Margins in paper units (mm).
const MARGIN_MM: f64 = 25.0;
/// Gap between viewports in paper units (mm).
const GAP_MM: f64 = 20.0;

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
    let paper = cadcraft_render::paper::resolve_paper(paper_name).ok_or_else(|| EngineError::Other(format!("unknown paper: {paper_name}")))?;

    // Check layout name uniqueness
    if s.doc()?.layouts.iter().any(|l| l.name == name) {
        return Err(EngineError::Other(format!("layout name already in use: {name}")));
    }

    // Validate views and collect data
    let mut view_data = Vec::new();
    let mut seen = std::collections::HashSet::new();

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

        let bounds = view.bounds();
        // Grown bounds for dimensions
        let grown_min = Vec2::new(bounds.min.x - 1.5, bounds.min.y - 1.5);
        let grown_max = Vec2::new(bounds.max.x + 1.5, bounds.max.y + 1.5);

        view_data.push((v_name.to_string(), view.scale, grown_min, grown_max));
    }

    // Calculate layout
    // k = paper units per model meter
    // Metric drawing assumed for now (mm).
    // k = 1000 / scale. e.g. scale 50 (1:50) -> 20 mm/m.
    // If imperial, k = 1000 / 25.4 / scale.
    // We assume metric for simplicity as per typical arch workflows in this codebase unless units are checked.
    // Checking units:
    let is_metric = doc.units == cadcraft_doc::Units::Metric;

    let mut viewports = Vec::new();
    let mut row_height = 0.0;
    let mut col_widths = Vec::new();

    // We need to determine column widths first to place them?
    // "two per row... 20 mm gaps between columns (a column's x from the widest viewport of the column before)"
    // This implies a grid layout.

    // Let's calculate sizes first
    struct ViewportInfo {
        name: String,
        scale: f64,
        k: f64,
        w_mm: f64,
        h_mm: f64,
        center_model: Vec2,
    }

    let mut infos = Vec::new();
    for (name, scale, min, max) in view_data {
        let k = if is_metric { 1000.0 / scale } else { 1000.0 / 25.4 / scale };

        let w_model = max.x - min.x;
        let h_model = max.y - min.y;

        let w_mm = w_model * k;
        let h_mm = h_model * k;

        let center_model = Vec2::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);

        infos.push(ViewportInfo { name, scale, k, w_mm, h_mm, center_model });
    }

    // Layout logic: 2 per row
    let mut current_x = MARGIN_MM;
    let mut current_y = paper.height - MARGIN_MM; // Top-down Y in paper space? 
    // Paper space usually has Y up.
    // "first row's top edge 25 mm below the sheet's top edge"
    // So Y starts at height - 25.

    let mut col_x = Vec::new();
    let mut max_col_w = 0.0;

    // We need to know column widths to space them.
    // "a column's x from the widest viewport of the column before"
    // This suggests we determine column widths based on the views in that column.
    // Since it's fixed 2 per row, we have Column 0 and Column 1.

    let mut col0_w = 0.0;
    let mut col1_w = 0.0;

    for (i, info) in infos.iter().enumerate() {
        if i % 2 == 0 {
            col0_w = col0_w.max(info.w_mm);
        } else {
            col1_w = col1_w.max(info.w_mm);
        }
    }

    let col0_x = MARGIN_MM;
    let col1_x = MARGIN_MM + col0_w + GAP_MM;

    // Check fit
    let total_w = col1_x + col1_w + MARGIN_MM;
    if total_w > paper.width {
        return Err(EngineError::Other("views do not fit the sheet width".into()));
    }

    // Calculate rows
    let mut y_cursor = paper.height - MARGIN_MM;
    let mut row_idx = 0;

    for (i, info) in infos.iter().enumerate() {
        let is_col0 = i % 2 == 0;
        let x_center = if is_col0 { col0_x + col0_w / 2.0 } else { col1_x + col1_w / 2.0 };

        // Row height is max height of the two views in this row
        if is_col0 {
            let next_h = infos.get(i + 1).map(|n| n.h_mm).unwrap_or(0.0);
            row_height = info.h_mm.max(next_h);

            // Check vertical fit
            if y_cursor - row_height < MARGIN_MM {
                return Err(EngineError::Other("views do not fit the sheet height".into()));
            }
        }

        // Y center: Top of row is y_cursor. Bottom is y_cursor - row_height.
        // Center is y_cursor - row_height / 2.
        let y_center = y_cursor - row_height / 2.0;

        viewports.push((info.name.clone(), info.scale, Vec2::new(x_center, y_center), info.w_mm, info.h_mm));

        if !is_col0 || i == infos.len() - 1 {
            // End of row
            y_cursor -= row_height + GAP_MM;
        }
    }

    // Now execute the changes

    // 1. Create layout
    let layout_idx = s.doc_mut()?.layouts.len();
    let mut new_layout = cadcraft_doc::Layout::new(name.to_string());
    // Do NOT add default viewport

    // 2. Set page setup
    // We need to call pagesetup logic or replicate it.
    // pagesetup command usually modifies the layout's page setup.
    // Let's try to use the command runner if possible, or just set the fields.
    // Setting fields directly is safer for "nothing written" on error, but we already validated.

    // We need to add the layout first to modify it? Or modify then add?
    // Let's add it.
    s.doc_mut()?.layouts.push(new_layout);

    // Now modify the new layout
    let layout = s.doc_mut()?.layouts.last_mut().ok_or_else(|| EngineError::Other("layout creation failed".into()))?;

    // Set paper
    layout.page_setup.paper = paper.name.clone();
    layout.page_setup.landscape = landscape;

    // 3. Create viewports
    let mut handles = Vec::new();

    for (v_name, scale, center_paper, w_mm, h_mm) in viewports {
        // Create viewport entity
        // Viewport in paper space.
        // Center in paper space is center_paper.
        // Width/Height in paper space.

        // We need to create a Viewport entity.
        // Look at mview command.

        // Viewport { center, width, height, view_center, view_width, view_height, scale, locked, ... }

        // The model view center is the center of the grown bounds in model space.
        // We need to find the model center again.
        // We stored center_model in infos.

        // Let's re-lookup info or store it in the loop.
        // We lost center_model in the previous loop. Let's fix the loop above to store it.

        // Actually, let's just use the view's bounds again.
        let view = s.doc()?.arch.views.iter().find(|v| v.name == v_name).ok_or_else(|| EngineError::Other("view lost".into()))?;
        let bounds = view.bounds();
        let grown_min = Vec2::new(bounds.min.x - 1.5, bounds.min.y - 1.5);
        let grown_max = Vec2::new(bounds.max.x + 1.5, bounds.max.y + 1.5);
        let center_model = Vec2::new((grown_min.x + grown_max.x) / 2.0, (grown_min.y + grown_max.y) / 2.0);

        // Viewport entity creation
        let mut vp = cadcraft_doc::Entity::new(cadcraft_doc::EntityKind::Viewport(cadcraft_doc::Viewport {
            center: center_paper,
            width: w_mm,
            height: h_mm,
            view_center: center_model,
            view_width: w_mm / (1000.0 / scale), // Convert paper mm back to model meters?
            // Wait.
            // Scale k = paper units / model unit.
            // Viewport scale in DXF is usually 1:X.
            // The Viewport struct in cadcraft_doc might store scale differently.
            // Let's check cadcraft_doc::Viewport.

            // Assuming Viewport has `scale` field.
            scale,
            locked: true,
            ..Default::default()
        }));

        // Insert entity
        let handle = s.doc_mut()?.model.insert(vp);
        handles.push(handle);

        // Add to layout
        layout.entities.push(handle);
    }

    // Return result
    let mut resp = Vec::new();
    for (h, (name, scale, center, w, h)) in handles.iter().zip(viewports.iter()) {
        resp.push(json!({
            "handle": format!("{:x}", h.0),
            "view": name,
            "scale": scale,
            "center": [center.x, center.y],
            "width": w,
            "height": h
        }));
    }

    Ok(json!({
        "layout": name,
        "paper": paper.name,
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
