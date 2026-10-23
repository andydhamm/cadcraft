//! The `wall` command: create architectural wall segments from JSON parameters.
//!
//! JSON parameters:
//! - `points`: required, `[[x,y], ...]` with 2 or more distinct points.
//! - `thickness`: optional, wall thickness. Defaults to the exterior or interior
//!   default based on `exterior`.
//! - `height`: optional, wall height. Defaults to the level's `floor_to_floor`.
//! - `exterior`: optional bool (default `true`).
//! - `closed`: optional bool (default `false`). If true and >2 points and last
//!   differs from first, a closing segment is added.
//! - `level`: optional string, level name (an unknown name is an error). Defaults to the
//!   first level in the model.
//! - `type`: optional string (`exterior|interior|knee|railing|retaining`).
//!   Overrides the `exterior` flag and sets the wall kind.
//!
//! Returns JSON: `{"walls": [{"id": "w1", "handle": "a0000001"}, ...], "joins": N}` where `N`
//! counts the joined ends (L/T/X/butt/square) on the new walls.

use cadcraft_arch::{ArchId, JoinKind, Wall};
use cadcraft_doc::{Layer, arch_handle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, bool_or, distinct_points, points_param, size_param, str_param};
use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{Result, Session};

/// Command specs for the wall module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("wall", "Wall", run_wall)
            .menu(&["Architecture", "Wall"])
            .alias(&["wal"])
            .params("{points: [[x,y],...], thickness?, height?, exterior?, closed?, level?, type?}")
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
    if pts.iter().any(|p| !p.is_finite()) {
        return Err(bad("wall", "non-finite coordinate in `points`"));
    }
    for i in 0..pts.len() - 1 {
        let d = pts[i + 1] - pts[i];
        if d.len() < super::MIN_WALL_LENGTH {
            return Err(bad("wall", "a segment is shorter than the minimum length"));
        }
    }

    // --- Parse remaining parameters ---
    // Everything is validated before `doc_mut`, so a refused call leaves no undo step.
    let closed = bool_or(p, "closed", false);
    let level_name = str_param(p, "level").map(|s| s.to_string());
    let type_str = str_param(p, "type").map(|s| s.to_string());

    // `type` overrides `exterior`; knee/railing/retaining are treated as interior
    let exterior = match &type_str {
        None => bool_or(p, "exterior", true),
        Some(t) => match t.as_str() {
            "exterior" => true,
            "interior" | "knee" | "railing" | "retaining" => false,
            _ => return Err(bad("wall", format!("unknown wall type `{t}`"))),
        },
    };
    let wall_type = type_str.clone().unwrap_or_else(|| if exterior { "exterior".to_string() } else { "interior".to_string() });

    // Thickness: explicit or default based on exterior
    let thickness = size_param("wall", p, "thickness", false)?;
    let thickness = match thickness {
        Some(t) => t,
        None => {
            if exterior {
                super::DEFAULT_EXT_THICKNESS
            } else {
                super::DEFAULT_INT_THICKNESS
            }
        }
    };
    if thickness < cadcraft_doc::arch::MIN_WALL_THICKNESS {
        return Err(bad("wall", format!("thickness {thickness} is below the minimum {}", cadcraft_doc::arch::MIN_WALL_THICKNESS)));
    }

    // Level: from param (an unknown name is an error, not a silent fallback), or the first
    // level in the model when absent
    let doc = s.doc()?;
    let level = match level_name.as_deref() {
        Some(name) => Some(doc.arch.levels.iter().find(|l| l.name == name).ok_or_else(|| bad("wall", format!("unknown level `{name}`")))?),
        None => doc.arch.levels.first(),
    };

    // Height: explicit or level floor_to_floor
    let height = size_param("wall", p, "height", false)?;
    let height = match height {
        Some(h) => h,
        None => level.map(|l| l.floor_to_floor).unwrap_or(super::DEFAULT_EXT_THICKNESS * 5.0),
    };
    let base_level_idx = level.and_then(|l| doc.arch.levels.iter().position(|lv| lv.name == l.name)).unwrap_or(0) as u32;

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

    // The model-wide cap: the join graph and the stretch BFS stop at `MAX_WALLS`, so walls
    // past it would silently never join.
    let segments = pts.len().saturating_sub(1);
    if doc.arch.walls.len().saturating_add(segments) > cadcraft_doc::MAX_WALLS {
        return Err(bad("wall", format!("the drawing would have more than {} walls", cadcraft_doc::MAX_WALLS)));
    }

    // --- Build wall objects ---
    let layers = scaled_layers(super::wall_layers(exterior), thickness);
    let d = s.doc_mut()?;
    // Proxy plan graphics go on A-WALL when the drawing is saved; a drawing opened from a
    // file that predates arch3d has no such layer.
    if d.layer(ARCH_WALL_LAYER).is_none() {
        d.layers.push(Layer { name: ARCH_WALL_LAYER.into(), ..Layer::default() });
    }
    let mut new_walls: Vec<Wall> = Vec::with_capacity(segments);
    for i in 0..segments {
        let id = d.arch.next_wall_id();
        d.arch.next_id = d.arch.next_id.saturating_add(1);
        new_walls.push(Wall {
            id,
            start: pts[i],
            end: pts[i + 1],
            thickness,
            height,
            base_level: base_level_idx,
            to_ceiling: true,
            exterior,
            wall_type: wall_type.clone(),
            join_start: JoinKind::default(),
            join_end: JoinKind::default(),
            layers: layers.clone(),
        });
    }

    // --- Build response: newly added walls only ---
    let walls_json: Vec<Value> = new_walls.iter().map(|w| json!({ "id": format!("w{}", w.id.0), "handle": arch_handle(w.id).hex() })).collect();
    let new_ids: std::collections::BTreeSet<ArchId> = new_walls.iter().map(|w| w.id).collect();

    d.arch.walls.extend(new_walls);
    d.touch_arch();

    // How many wall ends the solver joined on the new walls (L, T, X, butt or square).
    let joins: usize = cadcraft_arch::join::solve_joins(&d.arch).iter().filter(|sh| new_ids.contains(&sh.id)).map(|sh| sh.joins.len()).sum();

    Ok(json!({ "walls": walls_json, "joins": joins }))
}

/// Name of the layer the save-time wall proxy polylines are written on.
const ARCH_WALL_LAYER: &str = "A-WALL";

/// The default band build-up scaled so the bands sum to `thickness` exactly (the natural
/// build-ups sum to 0.138 / 0.116, so unscaled bands left part of a 0.2 m wall unbanded and
/// overflowed a 0.1 m one).
fn scaled_layers(layers: Vec<(String, f64)>, thickness: f64) -> Vec<(String, f64)> {
    let sum: f64 = layers.iter().map(|(_, t)| t).sum();
    if sum.is_nan() || sum <= 0.0 || !thickness.is_finite() {
        return layers;
    }
    layers.into_iter().map(|(n, t)| (n, t * thickness / sum)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineError, Session};
    use cadcraft_arch::Level;
    use serde_json::json;

    fn new_session_with_level() -> Session {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        d.arch.levels.push(Level { name: "ground".to_string(), floor_to_floor: 3.0, ..Default::default() });
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
    fn wall_command_rejects_an_unknown_level_without_an_undo_step() {
        let mut s = new_session_with_level();
        let undo = s.state().unwrap().undo.len();
        let r = s.execute("wall", &json!({ "points": [[0.0, 0.0], [2.0, 0.0]], "level": "attic" }));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "{r:?}");
        assert!(s.doc().unwrap().arch.walls.is_empty());
        // A refused call (bad type) does not leave an empty undo step either.
        let r = s.execute("wall", &json!({ "points": [[0.0, 0.0], [2.0, 0.0]], "type": "bogus" }));
        assert!(r.is_err());
        assert_eq!(s.state().unwrap().undo.len(), undo);
    }

    #[test]
    fn wall_command_rejects_a_thickness_below_the_model_minimum() {
        let mut s = new_session_with_level();
        let r = s.execute("wall", &json!({ "points": [[0.0, 0.0], [2.0, 0.0]], "thickness": 1e-12 }));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "{r:?}");
        assert!(s.doc().unwrap().arch.validate().is_empty());
    }

    #[test]
    fn wall_command_caps_the_model_wall_count() {
        let mut s = new_session_with_level();
        let pts: Vec<Value> = (0..=4000).map(|i| json!([i as f64, 0.0])).collect();
        s.execute("wall", &json!({ "points": pts })).unwrap();
        let pts: Vec<Value> = (0..=200).map(|i| json!([i as f64, 5.0])).collect();
        let r = s.execute("wall", &json!({ "points": pts }));
        assert!(matches!(r, Err(EngineError::BadParams { .. })), "{r:?}");
        assert_eq!(s.doc().unwrap().arch.walls.len(), 4000);
    }

    #[test]
    fn wall_command_reports_the_joins_it_made() {
        let mut s = new_session_with_level();
        let r = s.execute("wall", &json!({ "points": [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]] })).unwrap();
        // One L corner: both walls join at (4, 0).
        assert_eq!(r["joins"], json!(2));
    }

    #[test]
    fn wall_layer_bands_sum_to_the_wall_thickness() {
        let mut s = new_session_with_level();
        s.execute("wall", &json!({ "points": [[0.0, 0.0], [4.0, 0.0]], "thickness": 0.3 })).unwrap();
        s.execute("wall", &json!({ "points": [[0.0, 2.0], [4.0, 2.0]], "type": "interior" })).unwrap();
        for w in &s.doc().unwrap().arch.walls {
            let sum: f64 = w.layers.iter().map(|(_, t)| t).sum();
            assert!((sum - w.thickness).abs() < 1e-12, "bands sum {sum} != thickness {}", w.thickness);
        }
    }

    #[test]
    fn wall_command_adds_the_a_wall_layer_when_missing() {
        let mut s = new_session_with_level();
        s.doc_mut().unwrap().layers.retain(|l| l.name != "A-WALL");
        s.execute("wall", &json!({ "points": [[0.0, 0.0], [4.0, 0.0]] })).unwrap();
        assert!(s.doc().unwrap().layer("A-WALL").is_some());
    }

    #[test]
    fn wall_command_never_opens_a_dialog() {
        let mut s = new_session_with_level();
        let p = json!({ "points": [[0.0, 0.0], [1.0, 0.0]] });
        let r = s.execute("wall", &p);
        assert!(r.is_ok());
    }

    #[test]
    fn wall_defaults_use_the_level_floor_to_floor() {
        let mut s = new_session_with_level();
        let p = json!({
            "points": [[0.0, 0.0], [2.0, 0.0]],
            "level": "ground",
        });
        let _ = s.execute("wall", &p).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls[0].height, 3.0);
    }

    #[test]
    fn wall_machine_collects_points_and_creates_walls() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 3.0))).unwrap();
        s.input(Input::Text("".to_string())).unwrap();
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
        s.input(Input::Text("Thickness".to_string())).unwrap();
        // Submit zero, which must be rejected (re-prompt, not finish)
        s.input(Input::Text("0".to_string())).unwrap();
        // The wall has not been created yet because we never finished.
        assert_eq!(s.doc().unwrap().arch.walls.len(), 0);
        // Now submit a valid value and finish
        s.input(Input::Text("4.0".to_string())).unwrap();
        s.input(Input::Text("".to_string())).unwrap();
        assert_eq!(s.doc().unwrap().arch.walls.len(), 1);
    }

    #[test]
    fn wall_machine_type_keyword_switches_to_interior() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(3.0, 0.0))).unwrap();
        // Default exterior = true; toggle to interior
        s.input(Input::Text("type".to_string())).unwrap();
        s.input(Input::Text("".to_string())).unwrap();
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
        s.input(Input::Text("".to_string())).unwrap();
        assert_eq!(s.doc().unwrap().arch.walls.len(), 0);
        // Add a second point and now finish.
        s.input(Input::Point(Vec2::new(4.0, 0.0))).unwrap();
        s.input(Input::Text("".to_string())).unwrap();
        assert_eq!(s.doc().unwrap().arch.walls.len(), 1);
    }

    #[test]
    fn wall_machine_close_keyword_adds_the_closing_segment() {
        let mut s = new_session_with_level();
        s.start("wall").unwrap();
        s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(4.0, 0.0))).unwrap();
        s.input(Input::Point(Vec2::new(4.0, 3.0))).unwrap();
        s.input(Input::Text("close".to_string())).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), 3);
        assert_eq!(d.arch.walls[2].start, Vec2::new(4.0, 3.0));
        assert_eq!(d.arch.walls[2].end, Vec2::new(0.0, 0.0));
    }
}
