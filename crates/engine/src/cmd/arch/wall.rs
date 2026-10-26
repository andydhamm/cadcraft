//! The `wall` command: create architectural wall segments from JSON parameters.
//!
//! JSON parameters:
//! - `points`: required, `[[x,y], ...]` with 2 or more distinct points.
//! - `thickness`: optional, wall thickness. Defaults to the exterior or interior
//!   default. For `divider: true`, 0 is allowed; otherwise must be positive.
//! - `height`: optional, wall height. Defaults to the level's `floor_to_floor`.
//! - `exterior`: optional bool. With neither `type` nor `exterior` given, the
//!   exterior flag is inferred: `thickness >= DEFAULT_EXT_THICKNESS` → true.
//! - `closed`: optional bool (default `false`). If true and >2 points and last
//!   differs from first, a closing segment is added.
//! - `level`: optional string, level name. Defaults to the first level in the model.
//! - `type`: optional string (`exterior|interior|knee|railing|retaining`).
//!   Overrides the `exterior` flag and sets the wall kind.
//! - `toCeiling`: optional bool (default `false`). Wall extends to the level ceiling.
//! - `divider`: optional bool (default `false`). A partition wall that is interior
//!   and may have zero thickness.
//!
//! Returns JSON: `{"walls": [{"id": "w1", "handle": "a0000001"}, ...], "joins": 0}`

use cadcraft_arch::{Level, Wall};
use cadcraft_doc::Handle;
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, bool_or, distinct_points, points_param, size_param, str_param};
use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{EngineError, Result, Session};

/// Command specs for the wall module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("wall", "Wall", run_wall)
            .menu(&["Architecture", "Wall"])
            .alias(&["wal"])
            .params("{points: [[x,y],...], thickness?, height?, exterior?, closed?, level?, type?, toCeiling?, divider?}")
            .interactive(|_| Ok(Box::new(WallM::default()))),
    ]
}

/// Which dimension the machine is currently prompting for a number entry.
#[derive(Clone, Copy, PartialEq)]
enum NumTarget {
    Thickness,
    Height,
}

/// Interactive `WALL` machine: collects points for wall segments with
/// `Type`/`Thickness`/`Height`/`Close` keyword support.
///
/// The machine delegates to the JSON `wall` command on finish so there is
/// exactly one implementation path and one undo step.
pub struct WallM {
    pts: Vec<Vec2>,
    base: Option<Vec2>,
    thickness: Option<f64>,
    height: Option<f64>,
    exterior: bool,
    closed: bool,
    num_target: Option<NumTarget>,
}

impl Default for WallM {
    fn default() -> Self {
        Self { pts: Vec::new(), base: None, thickness: None, height: None, exterior: true, closed: false, num_target: None }
    }
}

impl Interactive for WallM {
    fn name(&self) -> &'static str {
        "wall"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        if let Some(target) = self.num_target {
            let msg = match target {
                NumTarget::Thickness => "Specify thickness:".to_string(),
                NumTarget::Height => "Specify height:".to_string(),
            };
            return Prompt {
                message: msg,
                keywords: Vec::new(),
                default: None,
                accept: Accept::TEXT,
                base: None,
                corner: false,
                grip: None,
                picker: None,
                distance: false,
                deferred: false,
                raw: false,
                picks_one: false,
            };
        }

        let (message, keywords) = if self.pts.is_empty() {
            ("Specify start point:".to_string(), Vec::new())
        } else {
            (
                "Specify next point or [Type/Thickness/Height/Close]:".to_string(),
                vec!["Type".to_string(), "Thickness".to_string(), "Height".to_string(), "Close".to_string()],
            )
        };

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
        match i {
            Input::Point(pt) => {
                if self.num_target.is_some() {
                    // A point is not a number; re-prompt for the value.
                    return Ok(Step::Continue);
                }
                if !pt.is_finite() {
                    return Ok(Step::Continue);
                }
                if self.pts.len() < super::MAX_WALLS {
                    self.pts.push(pt);
                }
                self.base = Some(pt);
                Ok(Step::Continue)
            }
            Input::Text(txt) => {
                if self.num_target.is_some() {
                    return self.parse_number(&txt);
                }
                if txt.trim().is_empty() {
                    if self.pts.len() >= 2 {
                        let params = self.build_params();
                        return s.execute("wall", &params).map(|_| Step::Done);
                    }
                    return Ok(Step::Continue);
                }
                let lower = txt.to_ascii_lowercase().trim().to_string();
                match lower.as_str() {
                    "type" => {
                        self.exterior = !self.exterior;
                        self.thickness = None;
                        Ok(Step::Continue)
                    }
                    "thickness" => {
                        self.num_target = Some(NumTarget::Thickness);
                        Ok(Step::Continue)
                    }
                    "height" => {
                        self.num_target = Some(NumTarget::Height);
                        Ok(Step::Continue)
                    }
                    "close" => {
                        self.closed = true;
                        if self.pts.len() >= 2 {
                            let params = self.build_params();
                            s.execute("wall", &params).map(|_| Step::Done)
                        } else {
                            Ok(Step::Continue)
                        }
                    }
                    _ => Ok(Step::Continue),
                }
            }
            Input::Cancel => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
}

impl WallM {
    /// Build the JSON parameter object that the JSON `wall` command accepts.
    /// `thickness` and `height` are included only when the user set them.
    fn build_params(&self) -> Value {
        let points: Vec<Value> = self.pts.iter().map(|p| json!([p.x, p.y])).collect();
        let mut map = serde_json::Map::new();
        map.insert("points".to_string(), json!(points));
        map.insert("exterior".to_string(), json!(self.exterior));
        map.insert("closed".to_string(), json!(self.closed));
        if let Some(t) = self.thickness {
            map.insert("thickness".to_string(), json!(t));
        }
        if let Some(h) = self.height {
            map.insert("height".to_string(), json!(h));
        }
        Value::Object(map)
    }

    /// Parse `txt` as the number requested by `num_target`.
    /// Rejects non-positive, non-finite, and unparseable values by re-prompting.
    fn parse_number(&mut self, txt: &str) -> Result<Step> {
        match txt.trim().parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => {
                match self.num_target {
                    Some(NumTarget::Thickness) => self.thickness = Some(v),
                    Some(NumTarget::Height) => self.height = Some(v),
                    None => {}
                }
                self.num_target = None;
                Ok(Step::Continue)
            }
            _ => Ok(Step::Continue),
        }
    }
}

/// Run the `wall` command from JSON parameters. Never opens a dialog.
fn run_wall(s: &mut Session, p: &Value) -> Result<Value> {
    // --- Parse and validate points ---
    let pts = points_param(p, "points").ok_or_else(|| bad("wall", "`points` [[x,y],...] (2 or more) is required"))?;
    let pts = distinct_points(&pts);
    if pts.len() < 2 {
        return Err(bad("wall", "`points` [[x,y],...] (2 or more) is required"));
    }
    if pts.len() > super::MAX_WALLS {
        return Err(bad("wall", format!("too many points ({} > {})", pts.len(), super::MAX_WALLS)));
    }
    for i in 0..pts.len() {
        if !pts[i].is_finite() {
            return Err(bad("wall", "non-finite coordinate in `points`"));
        }
    }
    for i in 0..pts.len() - 1 {
        let d = pts[i + 1] - pts[i];
        if d.len() < super::MIN_WALL_LENGTH {
            return Err(bad("wall", "a segment is shorter than the minimum length"));
        }
    }

    // --- Parse remaining parameters ---
    let closed = bool_or(p, "closed", false);
    let level_name = str_param(p, "level").map(|s| s.to_string());
    let type_str = str_param(p, "type").map(|s| s.to_string());
    let divider = bool_or(p, "divider", false);
    let to_ceiling = bool_or(p, "toCeiling", false);
    let has_exterior_param = p.as_object().is_some_and(|o| o.contains_key("exterior"));

    let d = s.doc_mut()?;

    // --- Exterior: `type` decides; otherwise explicit `exterior` param,
    // or thickness-based inference (below). `divider` always forces interior.
    let mut exterior = match &type_str {
        None => {
            if has_exterior_param {
                bool_or(p, "exterior", true)
            } else {
                true
            }
        }
        Some(t) => match t.as_str() {
            "exterior" => true,
            "interior" | "knee" | "railing" | "retaining" => false,
            _ => return Err(bad("wall", format!("unknown wall type `{t}`"))),
        },
    };
    if divider {
        exterior = false;
    }

    // --- Thickness: explicit or default (divider allows 0, else size_param
    // or the exterior/interior default from the preliminary exterior above) ---
    let thickness = if divider {
        match p.get("thickness").and_then(Value::as_f64) {
            Some(t) if t.is_finite() && t >= 0.0 => t,
            Some(_) => return Err(bad("wall", "divider `thickness` must be finite and non-negative")),
            None => 0.0,
        }
    } else {
        let has_thickness = p.as_object().is_some_and(|o| o.contains_key("thickness"));
        let explicit = size_param("wall", p, "thickness", false)?;
        match explicit {
            Some(t) => t,
            None if has_thickness => return Err(bad("wall", "`thickness` must be a positive finite number")),
            None => {
                if exterior {
                    super::DEFAULT_EXT_THICKNESS
                } else {
                    super::DEFAULT_INT_THICKNESS
                }
            }
        }
    };

    // --- Thickness-based exterior inference (applies only when neither `type`
    // nor an explicit `exterior` param was given) ---
    if type_str.is_none() && !has_exterior_param {
        exterior = thickness >= super::DEFAULT_EXT_THICKNESS;
    }
    if divider {
        exterior = false;
    }

    let wall_type = type_str.clone().unwrap_or_else(|| if exterior { "exterior".to_string() } else { "interior".to_string() });

    // Level: from param, or the first level in the model when absent
    let level = level_name.as_deref().and_then(|name| d.arch.levels.iter().find(|l| l.name == name)).or_else(|| d.arch.levels.first());

    // Height: explicit or the level's derived floor-to-floor (ceiling + structure)
    let has_height = p.as_object().is_some_and(|o| o.contains_key("height"));
    let height = if has_height {
        size_param("wall", p, "height", false)?.unwrap_or(f64::NAN)
    } else {
        level.map(|l| l.ceiling_height + l.structure).unwrap_or(super::DEFAULT_WALL_HEIGHT)
    };
    if !height.is_finite() || height <= 0.0 {
        return Err(bad("wall", "`height` must be a positive finite number"));
    }

    // Apply closed flag
    let pts = if closed && pts.len() > 2 {
        match pts.last() {
            Some(last) if !last.near(pts[0], 1e-6) => {
                let first = pts[0];
                let mut v = pts;
                v.push(first);
                v
            }
            _ => pts,
        }
    } else {
        pts
    };

    // --- Build wall objects ---
    let base_level_idx = level.and_then(|l| d.arch.levels.iter().position(|lv| lv.name == l.name)).unwrap_or(0) as u32;
    let layers = super::wall_layers(exterior);
    let mut new_walls: Vec<Wall> = Vec::with_capacity(pts.len().saturating_sub(1));
    for i in 0..pts.len().saturating_sub(1) {
        let id = d.arch.next_wall_id();
        d.arch.next_id = d.arch.next_id.saturating_add(1);
        new_walls.push(Wall {
            id,
            start: pts[i],
            end: pts[i + 1],
            thickness,
            height,
            base_level: base_level_idx,
            to_ceiling,
            divider,
            exterior,
            wall_type: wall_type.clone(),
            layers: layers.clone(),
            ..Default::default()
        });
    }

    // --- Build response: newly added walls only ---
    let walls_json: Vec<Value> = new_walls
        .iter()
        .map(|w| {
            let handle = Handle(0xA000_0000u64 + w.id.0 as u64);
            json!({
                "id": format!("w{}", w.id.0),
                "handle": handle.hex(),
            })
        })
        .collect();

    d.arch.walls.extend(new_walls);
    d.touch_arch();

    // --- Join solver (pending p1-join-solver merge) ---
    let joins = 0usize;
    // TODO(p1-join-solver): let joins = cadcraft_arch::join::solve_joins(&mut d.arch);

    Ok(json!({ "walls": walls_json, "joins": joins }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;

    fn new_session_with_level() -> Session {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "ground".to_string(), ceiling_height: 2.4, structure: 0.3, ..Default::default() });
        d.touch_arch();
        s
    }

    #[test]
    fn wall_command_creates_segments_and_bumps_revision() {
        let mut s = new_session_with_level();
        let before = s.doc().unwrap().arch_revision;
        let p = json!({
            "points": [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0]],
        });
        let r = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch_revision > before);
        let walls = r["walls"].as_array().unwrap();
        assert_eq!(walls.len(), 2);
        assert_eq!(d.arch.walls.len(), 2);
        assert_eq!(d.arch.walls[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(d.arch.walls[0].end, Vec2::new(3.0, 0.0));
        assert_eq!(d.arch.walls[1].start, Vec2::new(3.0, 0.0));
        assert_eq!(d.arch.walls[1].end, Vec2::new(3.0, 3.0));
    }

    #[test]
    fn wall_command_rejects_bad_thickness_and_short_segments() {
        let mut s = new_session_with_level();
        // thickness: 0 (size_param with zero_ok=false rejects zero)
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": 0.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));

        // thickness: -1
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": -1.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));

        // short segment (below MIN_WALL_LENGTH)
        let p = json!({
            "points": [[0.0, 0.0], [0.0000001, 0.0]],
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));

        // height: very negative (non-positive, rejected by size_param)
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "height": -1.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_closed_flag_adds_the_closing_segment() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]],
            "closed": true,
        });
        let r = s.execute("wall", &p).unwrap();
        let walls = r["walls"].as_array().unwrap();
        assert_eq!(walls.len(), 4);
    }

    #[test]
    fn wall_command_is_undoable() {
        let mut s = new_session_with_level();
        let before = s.doc().unwrap().arch.walls.len();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
        });
        let _ = s.execute("wall", &p).unwrap();
        let after_add = s.doc().unwrap().arch.walls.len();
        assert_eq!(after_add, before + 1);

        s.undo().unwrap();
        let after_undo = s.doc().unwrap().arch.walls.len();
        assert_eq!(after_undo, before);
    }

    #[test]
    fn hostile_wall_params_never_panic() {
        let mut s = new_session_with_level();

        // duplicate consecutive points (reduced by distinct_points)
        let p = json!({ "points": [[1.0, 1.0], [1.0, 1.0], [2.0, 2.0]] });
        let _ = s.execute("wall", &p);

        // 2-point list where points are equal
        let p = json!({ "points": [[5.0, 5.0], [5.0, 5.0]] });
        let _ = s.execute("wall", &p);

        // thickness: 0
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": 0.0,
        });
        let _ = s.execute("wall", &p);

        // thickness: -1
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "thickness": -1.0,
        });
        let _ = s.execute("wall", &p);

        // height: very negative
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "height": -1.0,
        });
        let _ = s.execute("wall", &p);

        // points: "nope" (not an array, points_param returns None)
        let p = json!({ "points": "nope" });
        let _ = s.execute("wall", &p);

        // points: [] (empty array)
        let p = json!({ "points": [] });
        let _ = s.execute("wall", &p);

        // closed: true with 2 points
        let p = json!({
            "points": [[0.0, 0.0], [1.0, 0.0]],
            "closed": true,
        });
        let _ = s.execute("wall", &p);

        // 5000 points (refused, not allocated beyond the cap)
        let pts: Vec<Value> = (0..=5000).map(|i| json!([i as f64, 0.0])).collect();
        let p = json!({ "points": pts });
        let _ = s.execute("wall", &p);
    }

    #[test]
    fn wall_command_never_opens_a_dialog() {
        let mut s = new_session_with_level();
        let p = json!({ "points": [[0.0, 0.0], [1.0, 0.0]] });
        let r = s.execute("wall", &p);
        assert!(r.is_ok());
    }

    #[test]
    fn wall_height_default_no_level_param() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // ground: ceiling 2.4 + structure 0.3 = 2.7
        assert!((d.arch.walls[0].height - 2.7).abs() < 1e-9);
    }

    #[test]
    fn wall_height_defaults_to_derived_floor_to_floor() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // Default level 0 (ceiling 2.4 + structure 0.3) gives derived floor-to-floor 2.7
        assert!((d.arch.walls[0].height - 2.7).abs() < 1e-9);
    }

    #[test]
    fn wall_height_defaults_to_the_derived_floor_to_floor() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "ground",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // ground: ceiling 2.4 + structure 0.3 = 2.7
        assert!((d.arch.walls[0].height - 2.7).abs() < 1e-9);
    }

    #[test]
    fn wall_defaults_use_the_level_floor_to_floor() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "floor2".to_string(), ceiling_height: 2.1, structure: 0.4, ..Default::default() });
        d.touch_arch();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "floor2",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // ceiling 2.1 + structure 0.4 = 2.5
        assert!((d.arch.walls[0].height - 2.5).abs() < 1e-9);
    }

    #[test]
    fn wall_infers_interior_from_a_thin_thickness() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": 0.1,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "interior".to_string());
    }

    #[test]
    fn wall_explicit_exterior_param_overrides_the_inference() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": 0.1,
            "exterior": true,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "exterior".to_string());
    }

    #[test]
    fn wall_type_param_overrides_the_inference() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": 0.1,
            "type": "exterior",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "exterior".to_string());
    }

    #[test]
    fn wall_to_ceiling_param_is_honoured() {
        let mut s = new_session_with_level();
        // Default: to_ceiling is false
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
        });
        let _ = s.execute("wall", &p).unwrap();
        assert!(!s.doc().unwrap().arch.walls[0].to_ceiling);

        // Explicit: to_ceiling is true
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "toCeiling": true,
        });
        let _ = s.execute("wall", &p).unwrap();
        assert!(s.doc().unwrap().arch.walls[1].to_ceiling);
    }

    #[test]
    fn wall_divider_allows_zero_thickness() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "divider": true,
            "thickness": 0.0,
        });
        let r = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch.walls[0].divider);
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].thickness, 0.0);
    }

    #[test]
    fn wall_height_uses_level_ceiling_plus_structure() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "l2".to_string(), ceiling_height: 3.0, structure: 0.4, ..Default::default() });
        d.touch_arch();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "l2",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // ceiling 3.0 + structure 0.4 = 3.4
        assert!((d.arch.walls[0].height - 3.4).abs() < 1e-9);
    }

    #[test]
    fn wall_explicit_height_still_wins() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "height": 1.5,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // explicit 1.5 beats derived floor-to-floor 2.7
        assert!((d.arch.walls[0].height - 1.5).abs() < 1e-9);
    }

    #[test]
    fn wall_rejects_zero_thickness_without_divider() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": 0.0,
        });
        assert!(s.execute("wall", &p).is_err());
    }

    #[test]
    fn wall_machine_collects_points_and_creates_walls() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 3.0))).unwrap();
        s.input(Input::Text("".to_string()).into()).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), 2);
    }

    #[test]
    fn wall_machine_thickness_keyword_rejects_zero() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 0.0))).unwrap();
        // Trigger the thickness prompt
        s.input(Input::Text("Thickness".to_string()).into()).unwrap();
        // Submit zero, which must be rejected (re-prompt, not finish)
        s.input(Input::Text("0".to_string()).into()).unwrap();
        // The wall has not been created yet because we never finished.
        assert_eq!(s.doc().unwrap().arch.walls.len(), 0);
        // Now submit a valid value and finish
        s.input(Input::Text("4.0".to_string()).into()).unwrap();
        s.input(Input::Text("".to_string()).into()).unwrap();
        assert_eq!(s.doc().unwrap().arch.walls.len(), 1);
    }

    #[test]
    fn wall_machine_type_keyword_switches_to_interior() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 0.0))).unwrap();
        // Default exterior = true; toggle to interior
        s.input(Input::Text("type".to_string()).into()).unwrap();
        s.input(Input::Text("".to_string()).into()).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), 1);
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "interior".to_string());
    }

    #[test]
    fn wall_machine_cancel_touches_nothing() {
        let mut s = new_session_with_level();
        let before = s.doc().unwrap().arch.walls.len();
        let before_rev = s.doc().unwrap().arch_revision;
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 0.0))).unwrap();
        s.input(Input::Cancel).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), before);
        assert_eq!(d.arch_revision, before_rev);
    }

    #[test]
    fn wall_machine_needs_two_points_to_finish() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        // Enter with only one point: must not finish.
        s.input(Input::Text("".to_string()).into()).unwrap();
        assert_eq!(s.doc().unwrap().arch.walls.len(), 0);
        // Add a second point and now finish.
        s.input(Input::Point(Vec2::new(4.0, 0.0))).unwrap();
        s.input(Input::Text("".to_string()).into()).unwrap();
        assert_eq!(s.doc().unwrap().arch.walls.len(), 1);
    }

    #[test]
    fn wall_machine_close_keyword_adds_the_closing_segment() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(4.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(4.0, 3.0))).unwrap();
        s.input(Input::Text("close".to_string()).into()).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), 3);
        assert_eq!(d.arch.walls[2].start, Vec2::new(4.0, 3.0));
        assert_eq!(d.arch.walls[2].end, Vec2::new(0.0, 0.0));
    }

    #[test]
    fn wall_height_nan_rejected() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "height": serde_json::Value::Null,
        });
        // Null height is not NaN but tests the None path; test explicit NaN via a large non-finite
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "height": f64::NAN,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_height_zero_rejected() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "height": 0.0,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_height_huge_value_works() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "height": 1e12,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!((d.arch.walls[0].height - 1e12).abs() < 1e-3);
    }

    #[test]
    fn wall_level_zero_structure() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "flat".to_string(), ceiling_height: 3.0, structure: 0.0, ..Default::default() });
        d.touch_arch();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "flat",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // ceiling 3.0 + structure 0.0 = 3.0
        assert!((d.arch.walls[0].height - 3.0).abs() < 1e-9);
    }

    #[test]
    fn wall_level_zero_ceiling() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "slab".to_string(), ceiling_height: 0.0, structure: 0.5, ..Default::default() });
        d.touch_arch();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "slab",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // ceiling 0.0 + structure 0.5 = 0.5
        assert!((d.arch.walls[0].height - 0.5).abs() < 1e-9);
    }

    #[test]
    fn wall_no_level_uses_default_wall_height() {
        let mut s = Session::new();
        // No level added; wall should fall back to DEFAULT_WALL_HEIGHT (2.7)
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!((d.arch.walls[0].height - super::super::DEFAULT_WALL_HEIGHT).abs() < 1e-9);
    }

    #[test]
    fn wall_height_infinity_rejected() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "height": f64::INFINITY,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_unknown_level_uses_first_level() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "only".to_string(), ceiling_height: 2.8, structure: 0.3, ..Default::default() });
        d.touch_arch();
        // Level name "missing" does not exist; falls back to the first level
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "missing",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // First level is the default (ceiling 2.4 + structure 0.3 = 2.7), not "only"
        assert!((d.arch.walls[0].height - 2.7).abs() < 1e-9);
    }

    #[test]
    fn wall_multiple_levels_selects_correct_one() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "ground".to_string(), ceiling_height: 2.4, structure: 0.3, ..Default::default() });
        d.arch.levels.push(Level { name: "upper".to_string(), ceiling_height: 2.2, structure: 0.4, ..Default::default() });
        d.touch_arch();
        // Select the second level explicitly
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "upper",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        // upper: ceiling 2.2 + structure 0.4 = 2.6
        assert!((d.arch.walls[0].height - 2.6).abs() < 1e-9);
    }

    #[test]
    fn wall_thickness_at_exterior_default_is_exterior() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": super::super::DEFAULT_EXT_THICKNESS,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "exterior".to_string());
    }

    #[test]
    fn wall_thickness_just_below_exterior_default_is_interior() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": super::super::DEFAULT_EXT_THICKNESS - 0.001,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "interior".to_string());
    }

    #[test]
    fn wall_divider_forces_interior_even_with_explicit_exterior() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "divider": true,
            "exterior": true,
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch.walls[0].divider);
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "interior".to_string());
    }

    #[test]
    fn wall_level_zero_ceiling_and_structure_rejected() {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "zero".to_string(), ceiling_height: 0.0, structure: 0.0, ..Default::default() });
        d.touch_arch();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "zero",
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_thickness_nan_rejected() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "thickness": f64::NAN,
        });
        assert!(matches!(s.execute("wall", &p), Err(EngineError::BadParams { cmd: _, msg: _ })));
    }

    #[test]
    fn wall_divider_with_explicit_exterior_type() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "divider": true,
            "type": "exterior",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(d.arch.walls[0].divider);
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "exterior".to_string());
    }

    #[test]
    fn wall_divider_rejects_negative_thickness() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "divider": true,
            "thickness": -1.0,
        });
        assert!(s.execute("wall", &p).is_err());
    }

    #[test]
    fn wall_divider_rejects_nan_thickness() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "divider": true,
            "thickness": f64::NAN,
        });
        assert!(s.execute("wall", &p).is_err());
    }

    #[test]
    fn wall_divider_rejects_infinity_thickness() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "divider": true,
            "thickness": f64::INFINITY,
        });
        assert!(s.execute("wall", &p).is_err());
    }

    #[test]
    fn wall_with_type_knee_is_interior() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "type": "knee",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert!(!d.arch.walls[0].exterior);
        assert_eq!(d.arch.walls[0].wall_type, "knee".to_string());
    }

    #[test]
    fn wall_with_unknown_type_fails() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "type": "unknown",
        });
        assert!(s.execute("wall", &p).is_err());
    }
}
