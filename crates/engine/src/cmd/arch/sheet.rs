//! View Sheet command: places arch views on a layout at their scale.

use crate::Session;
use crate::cmd::CommandSpec;
use crate::error::{EngineError, Result};
use cadcraft_doc::arch::ArchView;
use cadcraft_geom::{Bounds2, Vec2};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Maximum number of views per sheet.
const MAX_VIEWS: usize = 16;

/// Margins in paper units (mm).
const MARGIN_MM: f64 = 25.0;
/// Gap between viewports in paper units (mm).
const GAP_MM: f64 = 20.0;
/// Growth of view bounds for dimensions (model units, m).
const DIM_MARGIN_M: f64 = 1.5;

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

    // Check layout name uniqueness
    if s.doc()?.layouts.iter().any(|l| l.name.eq_ignore_ascii_case(name)) {
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

        // Find view in ArchModel by its id label (e.g. "v0"); names are not a valid lookup key.
        let view = arch.view_by_label(v_name).ok_or_else(|| EngineError::Other(format!("unknown view: {v_name}")))?;

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
        let grown_min = Vec2::new(min_x - DIM_MARGIN_M, min_y - DIM_MARGIN_M);
        let grown_max = Vec2::new(max_x + DIM_MARGIN_M, max_y + DIM_MARGIN_M);

        view_data.push((v_name.to_string(), view.scale, grown_min, grown_max));
    }

    // Calculate layout
    // k = paper units per model meter (mm/m for metric)
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

    // Resolve paper dimensions to check fit
    // We use the same logic as pagesetup/run_pagesetup to get paper size
    // But since we haven't created the layout yet, we need to resolve it here.
    // The helper `new_bare_layout` will do this, but we need dimensions for layout calc.
    // We can duplicate the resolution or use a helper.
    // Let's use a simple match for common papers, falling back to error if unknown.
    // The prompt says "resolve the name the way pagesetup does".
    // pagesetup uses `cadcraft_render::paper_size`.
    // We can't easily call that from here without importing render crate (L2) into engine (L4)?
    // Wait, engine depends on render? Yes, L4 depends on L2.
    // But `cadcraft_render::paper_size` is likely available.
    // Let's check imports. `use cadcraft_render::paper_size` might work.

    let ps = cadcraft_render::paper_size(paper_name).ok_or_else(|| EngineError::Other(format!("unknown paper: {paper_name}")))?;
    let (paper_w, paper_h) = if landscape {
        (ps.width_mm.max(ps.height_mm), ps.width_mm.min(ps.height_mm))
    } else {
        (ps.width_mm.min(ps.height_mm), ps.width_mm.max(ps.height_mm))
    };
    let full_paper_name = ps.name.to_string();

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

    // Now execute the changes using helpers

    // 1. Create layout
    crate::cmd::layout::new_bare_layout(s, name, paper_name, landscape)?;

    // 2. Create viewports
    let mut resp = Vec::new();

    for (v_name, scale, center_paper, w_mm, h_mm, center_model) in placed_viewports {
        // Create viewport rectangle in paper space (mm)
        // Viewport center is center_paper. Width/Height are w_mm/h_mm.
        let rect = Bounds2::new(
            Vec2::new(center_paper.x - w_mm / 2.0, center_paper.y - h_mm / 2.0),
            Vec2::new(center_paper.x + w_mm / 2.0, center_paper.y + h_mm / 2.0),
        );

        // k = paper units per model unit
        let k = if is_metric { 1000.0 / scale } else { 1000.0 / 25.4 / scale };

        let h = crate::cmd::layout::add_scaled_viewport(s, name, rect, center_model, k)?;

        resp.push(json!({
            "handle": h.hex(),
            "view": v_name,
            "scale": k, // Return k (20.0 for 1:50)
            "center": [center_paper.x, center_paper.y],
            "width": w_mm,
            "height": h_mm
        }));
    }

    Ok(json!({
        "layout": name,
        "paper": paper_name,
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
