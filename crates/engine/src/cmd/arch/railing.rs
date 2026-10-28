//! The `railing` command: create a railing along a polyline path.
//!
//! JSON parameters:
//! - `path`: required, `[[x, y], ...]` — 2 to 512 finite points.
//! - `level`: optional, a level's name or its numeric index; absent means index 0.
//!   Unknown names/indices are refused (never a silent default).
//! - `height`: optional, metres; must pass the guard-band check
//!   (`cadcraft_arch::railing::validate_height(h, false)`), i.e. finite and in
//!   0.914–1.2 m. Absent means `GUARD_HEIGHT` (0.914 m). A 0.5 m guard is an
//!   error — never clamped.
//! - `style`: optional, `panel` or `balusters` (case-insensitive); absent means
//!   `panel`. Unknown values are refused.
//! - `thickness`: optional, metres; default 0.05. Must be positive and finite.
//!
//! Returns `{"id":"ra0","handle":"...","height":h,"style":s,"balusterGap":g}`.

use cadcraft_arch::railing::{GUARD_HEIGHT, baluster_spacing, path_length};
use cadcraft_doc::arch::{ARCH_HANDLE_BASE, MAX_RAILINGS, RAILING_HANDLE_OFFSET, Railing, RailingStyle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, points_param, size_param};
use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{EngineError, Result, Session};

/// Maximum number of path points accepted for a railing.
const MAX_POINTS: usize = 512;

/// Default railing thickness (metres).
const DEFAULT_THICKNESS: f64 = 0.05;

/// Command specs for the railing module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("railing", "Railing", run_railing)
            .menu(&["Architecture", "Railing"])
            .alias(&["rl"])
            .params("{path:[[x,y],...], level?, height?, style?: panel|balusters, thickness?}")
            .interactive(|_| Ok(Box::new(RailingM::default()))),
    ]
}

/// Resolve a level reference (a level name or its numeric index) to a position in
/// the arch model's levels. An unknown name/index is an error, never a silent
/// fallback to level 0.
fn find_level(arch: &cadcraft_doc::arch::ArchModel, ref_val: &str) -> Result<usize> {
    if let Some(i) = ref_val.trim().parse::<u64>().ok() {
        if i < arch.levels.len() as u64 {
            return Ok(i as usize);
        }
    }
    if let Some((i, l)) = arch.levels.iter().enumerate().find(|(_, l)| l.name == ref_val.trim()) {
        return Ok(i);
    }
    Err(EngineError::Other(format!("railing: unknown level `{ref_val}`")))
}

/// Map a case-insensitive style name to a [`RailingStyle`]; `None` for unknown
/// strings (refused, never coerced).
fn parse_railing_style(s: &str) -> Option<RailingStyle> {
    match s.trim().to_ascii_lowercase().as_str() {
        "panel" => Some(RailingStyle::Panel),
        "balusters" => Some(RailingStyle::Balusters),
        _ => None,
    }
}

/// Run the `railing` command from JSON parameters. Never opens a dialog.
fn run_railing(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let arch = &d.arch;

    // --- Path: required, 2..=512 finite points ---
    let pts = points_param(p, "path").ok_or_else(|| bad("railing", "`path` [[x,y],...] (2 to 512 points) is required"))?;
    if pts.len() < 2 || pts.len() > MAX_POINTS {
        return Err(bad("railing", format!("`path` must have 2 to {} points, got {}", MAX_POINTS, pts.len())));
    }
    for pt in &pts {
        if !pt.is_finite() {
            return Err(bad("railing", "non-finite coordinate in `path`"));
        }
    }

    // --- Level: name or index, else 0; unknown is an error ---
    let level = match p.get("level") {
        None => 0,
        Some(Value::Null) => 0,
        Some(Value::String(n)) => find_level(arch, n)?,
        Some(other) => return Err(bad("railing", format!("`level` must be a string (a level name or index), got `{other}`"))),
    };

    // --- Height: optional, must pass the guard-band check (never clamped) ---
    let height = match p.get("height") {
        None | Some(Value::Null) => GUARD_HEIGHT,
        Some(v) => {
            let h = match v.as_f64() {
                Some(h) => h,
                _ => return Err(bad("railing", "`height` must be a number")),
            };
            match cadcraft_arch::railing::validate_height(h, false) {
                Some(h) => h,
                None => return Err(bad("railing", format!("`height` {} is outside the guard band ({}–{} m)", h, GUARD_HEIGHT, 1.2))),
            }
        }
    };

    // --- Style: panel or balusters, default panel; unknown refused ---
    let style = match p.get("style") {
        None | Some(Value::Null) => RailingStyle::default(),
        Some(Value::String(t)) => match parse_railing_style(t) {
            Some(s) => s,
            None => return Err(bad("railing", format!("`style` `{t}` must be `panel` or `balusters`"))),
        },
        Some(_) => return Err(bad("railing", "`style` must be a string")),
    };

    // --- Thickness: optional, default 0.05 m ---
    let thickness = size_param("railing", p, "thickness", false)?.unwrap_or(DEFAULT_THICKNESS);

    // --- Baluster gap from path length and thickness; 0.0 when degenerate ---
    let gap = baluster_spacing(path_length(&pts), thickness).map(|(g, _)| g).unwrap_or(0.0);

    // --- Cap, then build and push (one mutation + one revision bump) ---
    if d.arch.railings.len() >= MAX_RAILINGS {
        return Err(bad("railing", "railing limit reached"));
    }
    let mut railing = Railing { path: pts, level: level as u32, height, style, thickness, baluster_gap: gap, ..Railing::default() };
    let id = d.arch.push_railing(railing);
    d.touch_arch();

    Ok(json!({
        "id": format!("ra{}", id.0),
        "handle": format!("{:016X}", ARCH_HANDLE_BASE + RAILING_HANDLE_OFFSET + id.0 as u64),
        "height": height,
        "style": match style {
            RailingStyle::Panel => "panel",
            RailingStyle::Balusters => "balusters",
        },
        "balusterGap": gap,
    }))
}

/// Interactive `railing` machine: collect path points, then finish with the
/// `Close` keyword or an empty Enter. The machine delegates to the JSON `railing`
/// command on finish so there is exactly one implementation path and one undo step.
pub struct RailingM {
    pts: Vec<Vec2>,
    base: Option<Vec2>,
}

impl Default for RailingM {
    fn default() -> Self {
        Self { pts: Vec::new(), base: None }
    }
}

impl Interactive for RailingM {
    fn name(&self) -> &'static str {
        "railing"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        let message = if self.pts.is_empty() { "Specify start point:".to_string() } else { "Specify next point or [Close]:".to_string() };
        let keywords = if self.pts.is_empty() { Vec::new() } else { vec!["Close".to_string()] };
        Prompt {
            message,
            keywords,
            default: None,
            accept: Accept::POINT,
            base: self.base,
            corner: false,
            grip: None,
            picker: None,
            distance: false,
            deferred: false,
            raw: false,
            picks_one: false,
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let closes = match i {
            Input::Enter => true,
            Input::Keyword(_) => true,
            Input::Text(ref txt) => {
                let l = txt.trim().to_ascii_lowercase();
                l.is_empty() || l == "close"
            }
            _ => false,
        };
        if closes {
            // Fewer than 2 points: re-prompt instead of erroring (never a model change).
            if self.pts.len() < 2 {
                return Ok(Step::Continue);
            }
            let params = self.build_params();
            return s.execute("railing", &params).map(|_| Step::Done);
        }
        match i {
            Input::Point(pt) => {
                if !pt.is_finite() {
                    return Ok(Step::Continue);
                }
                self.pts.push(pt);
                self.base = Some(pt);
                Ok(Step::Continue)
            }
            Input::Cancel => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
}

impl RailingM {
    /// Build the JSON parameter object the JSON `railing` command accepts. Only the
    /// fields the interactive form set (path) are emitted.
    fn build_params(&self) -> Value {
        let points: Vec<Value> = self.pts.iter().map(|p| json!([p.x, p.y])).collect();
        json!({ "path": points })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use cadcraft_doc::arch::RailingStyle;
    use serde_json::json;

    fn new_session() -> Session {
        Session::new()
    }

    /// A simple 2-point path (3 m long).
    fn simple_path() -> Value {
        json!([[0.0, 0.0], [3.0, 0.0]])
    }

    #[test]
    fn railing_defaults_to_a_914_guard() {
        let mut s = new_session();
        let r = s.execute("railing", &json!({ "path": simple_path() })).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.railings.len(), 1);
        assert_eq!(d.arch.railings[0].height, 0.914);
        assert_eq!(d.arch.railings[0].style, RailingStyle::Panel);
        assert_eq!(r["id"].as_str().unwrap(), "ra0");
        assert_eq!(r["height"].as_f64().unwrap(), 0.914);
        assert_eq!(r["style"].as_str().unwrap(), "panel");
    }

    #[test]
    fn railing_refuses_a_500_guard() {
        let mut s = new_session();
        let r = s.execute(
            "railing",
            &json!({
                "path": simple_path(),
                "height": 0.5,
            }),
        );
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.railings.len(), 0);
    }

    #[test]
    fn railing_refuses_unknown_style() {
        let mut s = new_session();
        let r = s.execute(
            "railing",
            &json!({
                "path": simple_path(),
                "style": "bogus",
            }),
        );
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.railings.len(), 0);
    }
}
