//! The `stair` command: lay out a stair flight between two levels.
//!
//! JSON parameters:
//!
//! - `start`: required, `[x, y]` — centre of the bottom nosing line.
//! - `direction`: required, degrees counter-clockwise from +x; stored in
//!   radians on the model.
//! - `width`: optional, metres; defaults to 1.0, must be finite in `(0, 5]`.
//! - `fromLevel` / `toLevel`: required, a level name or its numeric index
//!   (index tried first, then name, like the `level` command); unknown levels
//!   are refused.
//! - `rise`: optional, metres; when present must be finite and positive. When
//!   absent it is derived as `to.elevation - from.elevation`, which must be
//!   finite and positive.
//! - `shape`: optional, `straight|l|u` (case-insensitive, default
//!   `straight`); only straight flights are meshed (phase-5 gap note).
//! - `tread` / `maxRiser` / `minTread`: optional positive metres; default to
//!   `MIN_TREAD` / `MAX_RISER` / `MIN_TREAD`.
//!
//! Headroom is measured against the upper level's slab underside
//! (`to.elevation - to.structure`). When the upper floor lacks
//! `MIN_HEADROOM` clear of the flight, the stair's well becomes an `Open
//! Below` room on the upper level ringed by a guard railing that follows the
//! stair.
//!
//! Returns `{"id":"s0","handle":"...","risers":n,"riserHeight":h,"run":r,
//! "well":bool}`.

use std::f64::consts::PI;

use cadcraft_arch::poly::ring_area;
use cadcraft_arch::stair::{GUARD_HEIGHT, MAX_RISER, MIN_HEADROOM, MIN_TREAD, plan_stair, well_guard_path, well_polygon};
use cadcraft_doc::arch::{Railing, Room, RoomType, Stair, StairShape, stair_handle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, f64_or, f64_req, point_req, size_param};
use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{EngineError, Result, Session};

/// Prompt accept: number or free text (a level index or name, or a measure).
const TEXT_OR_NUMBER: Accept = Accept { point: false, number: true, text: true, select: false, enter: true };

/// Command specs for the stair module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("stair", "Stair", run_stair)
            .menu(&["Architecture", "Stair"])
            .alias(&["st"])
            .params("{start:[x,y], direction:deg, width?, fromLevel?|toLevel?|rise?, shape?, tread?, maxRiser?, minTread?}")
            .interactive(|_| Ok(Box::new(StairM::default()))),
    ]
}

/// Find a level by name (string) or index (number) and return its position in
/// the `levels` vec. Mirrors `cmd/arch/level.rs::find_level`: a numeric string
/// is tried as an index first, then as a name; unknown references are an
/// error.
fn find_level(arch: &cadcraft_doc::arch::ArchModel, ref_val: &Value) -> Result<usize> {
    if let Some(n) = ref_val.as_str() {
        // Try as index first.
        if let Some(i) = n.parse::<u64>().ok() {
            if i < arch.levels.len() as u64 {
                return Ok(i as usize);
            }
        }
        // Then as name.
        if let Some((i, _l)) = arch.levels.iter().enumerate().find(|(_i, l)| l.name == n) {
            return Ok(i);
        }
        return Err(EngineError::Other(format!("stair: unknown level '{n}'")));
    }
    if let Some(i) = ref_val.as_u64() {
        if i < arch.levels.len() as u64 {
            return Ok(i as usize);
        }
        return Err(EngineError::Other(format!("stair: unknown level index {i}")));
    }
    Err(EngineError::Other("stair: invalid level reference".into()))
}

/// Map a case-insensitive shape name to a [`StairShape`]; `None` for unknown
/// strings (refused, never coerced).
fn parse_stair_shape(s: &str) -> Option<StairShape> {
    match s.trim().to_ascii_lowercase().as_str() {
        "straight" => Some(StairShape::Straight),
        "l" => Some(StairShape::L),
        "u" => Some(StairShape::U),
        _ => None,
    }
}

/// Run the `stair` command from JSON parameters. Programmatic calls never open
/// dialogs.
fn run_stair(s: &mut Session, p: &Value) -> Result<Value> {
    // --- start: required finite point ---
    let start = point_req("stair", p, "start")?;
    if !start.is_finite() {
        return Err(bad("stair", "start must be a finite point"));
    }

    // --- direction: degrees CCW from +x, stored as radians ---
    let direction_deg = f64_req("stair", p, "direction")?;
    let direction = direction_deg * PI / 180.0;
    if !direction.is_finite() {
        return Err(bad("stair", "direction must be a finite number of degrees"));
    }

    // --- width: default 1.0 m, finite in (0, 5] ---
    let width = f64_or(p, "width", 1.0);
    if !(width > 0.0 && width <= 5.0) {
        return Err(bad("stair", format!("width {width} must be a finite number in (0, 5]")));
    }

    // --- shape: straight | l | u, default straight; unknown refused ---
    let shape = match p.get("shape") {
        None | Some(Value::Null) => StairShape::Straight,
        Some(Value::String(t)) => match parse_stair_shape(t) {
            Some(sh) => sh,
            None => return Err(bad("stair", format!("shape `{t}` must be one of: straight, l or u"))),
        },
        Some(_) => return Err(bad("stair", "shape must be a string")),
    };

    // --- levels: both required; unknown -> error ---
    let from_ref = p.get("fromLevel").ok_or_else(|| bad("stair", "`fromLevel` is required"))?;
    let to_ref = p.get("toLevel").ok_or_else(|| bad("stair", "`toLevel` is required"))?;

    let d = s.doc_mut()?;
    let arch = &d.arch;
    let from_i = find_level(arch, from_ref)?;
    let to_i = find_level(arch, to_ref)?;
    let from_level = arch.levels.get(from_i).ok_or_else(|| bad("stair", "unknown fromLevel"))?;
    let to_level = arch.levels.get(to_i).ok_or_else(|| bad("stair", "unknown toLevel"))?;

    // --- rise: param if finite & positive, else derived (finite & positive) ---
    let rise = match p.get("rise") {
        None | Some(Value::Null) => {
            let r = to_level.elevation - from_level.elevation;
            if r.is_finite() && r > 0.0 {
                r
            } else {
                return Err(bad("stair", "derived rise must be finite and positive"));
            }
        }
        Some(v) => match v.as_f64().filter(|x| x.is_finite()) {
            Some(x) if x > 0.0 => x,
            _ => return Err(bad("stair", "rise must be a finite positive number")),
        },
    };

    // --- optional positive sizes; absent => default ---
    let tread = size_param("stair", p, "tread", false)?;
    let max_riser = size_param("stair", p, "maxRiser", false)?;
    let min_tread = size_param("stair", p, "minTread", false)?;

    // --- plan; no write on failure ---
    let plan = plan_stair(rise, width, tread, max_riser.unwrap_or(MAX_RISER), min_tread.unwrap_or(MIN_TREAD));
    let plan = match plan {
        Some(pl) => pl,
        None => return Err(bad("stair", "no valid stair plan for the given rise, width and tread")),
    };

    // --- well + guard, computed before any write; headroom vs the upper slab
    // underside ---
    let mut stair = Stair {
        start,
        direction,
        width,
        from_level: from_i as u32,
        to_level: to_i as u32,
        rise,
        shape,
        risers: plan.risers,
        riser_height: plan.riser_height,
        tread_depth: plan.tread_depth,
        treads: plan.treads,
        run: plan.run,
        landing_depth: plan.landing_depth,
        well: Vec::new(),
        well_room: None,
        layer: "A-STAIR".into(),
        ..Stair::default()
    };
    let slab_underside = to_level.elevation - to_level.structure;
    let well_poly = well_polygon(&stair, &plan, slab_underside, MIN_HEADROOM);
    let has_well = well_poly.is_some();
    let guard = well_poly.as_ref().and_then(|_| well_guard_path(&stair, &plan, slab_underside, MIN_HEADROOM));
    let well = well_poly.unwrap_or_default();

    // --- one mutation + one revision bump; never a partial write ---
    let stair_id = d.arch.push_stair({
        stair.well = well.clone();
        stair
    });

    if has_well {
        // Upper-level room for the open well.
        let mut room =
            Room { name: "Open Below".into(), room_type: RoomType::OpenBelow, level: to_i as u32, polygon: well.clone(), ..Room::default() };
        room.interior_area = ring_area(&well);
        room.standard_area = cadcraft_arch::area::standard_area(&d.arch, &room);
        let room_id = d.arch.push_room(room);

        // Guard railing ringing the well, following the stair.
        if let Some(guard) = guard {
            d.arch.push_railing(Railing {
                path: guard,
                level: to_i as u32,
                height: GUARD_HEIGHT,
                follows: Some(stair_id),
                layer: "A-RAIL".into(),
                ..Railing::default()
            });
        }

        // Link the room to the stored stair.
        if let Some(pos) = d.arch.stair_index(stair_id) {
            d.arch.stairs[pos].well_room = Some(room_id);
        }
    }

    cadcraft_arch::area::refresh_areas(&mut d.arch);
    d.touch_arch();

    Ok(json!({
        "id": format!("s{}", stair_id.0),
        "handle": format!("{:016X}", stair_handle(stair_id).0),
        "risers": plan.risers,
        "riserHeight": plan.riser_height,
        "run": plan.run,
        "well": has_well,
    }))
}

/// Interactive `stair` machine: collect the start point, then direction,
/// width (optional), from level and to level one prompt at a time, then
/// delegate to the JSON `stair` command on finish so there is exactly one
/// implementation path and one undo step. Mirrors the `room` machine style.
pub struct StairM {
    /// 0 = start point, 1 = direction, 2 = width, 3 = from level,
    /// 4 = to level.
    phase: u8,
    start: Option<Vec2>,
    direction: Option<String>,
    width: Option<String>,
    from_level: Option<String>,
    to_level: Option<String>,
    base: Option<Vec2>,
}

impl Default for StairM {
    fn default() -> Self {
        Self { phase: 0, start: None, direction: None, width: None, from_level: None, to_level: None, base: None }
    }
}

impl StairM {
    /// The text of the input (free text or a matched keyword), or `None` for
    /// points/enter/cancel/other.
    fn text_of(i: &Input) -> Option<String> {
        match i {
            Input::Text(t) => Some(t.to_string()),
            Input::Keyword(k) => Some(k.to_string()),
            _ => None,
        }
    }

    /// The parameters the JSON `stair` command accepts, built from the values
    /// collected so far (only fields set are emitted).
    fn build_params(&self) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(pt) = self.start {
            m.insert("start".into(), json!([pt.x, pt.y]));
        }
        if let Some(d) = self.direction.as_deref() {
            m.insert("direction".into(), json!(d));
        }
        if let Some(w) = self.width.as_deref() {
            m.insert("width".into(), json!(w));
        }
        if let Some(f) = self.from_level.as_deref() {
            m.insert("fromLevel".into(), json!(f));
        }
        if let Some(t) = self.to_level.as_deref() {
            m.insert("toLevel".into(), json!(t));
        }
        Value::Object(m)
    }
}

impl Interactive for StairM {
    fn name(&self) -> &'static str {
        "stair"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        let (message, keywords, accept) = match self.phase {
            0 => ("Specify start point:".to_string(), Vec::new(), Accept::POINT),
            1 => ("Specify direction (degrees) or [Cancel]:".to_string(), vec!["Cancel".to_string()], TEXT_OR_NUMBER),
            2 => ("Specify width in metres or [Enter for 1]:".to_string(), Vec::new(), TEXT_OR_NUMBER),
            3 => ("Specify from level or [Cancel]:".to_string(), vec!["Cancel".to_string()], TEXT_OR_NUMBER),
            4 => ("Specify to level:".to_string(), vec![], TEXT_OR_NUMBER),
            _ => ("Specify to level:".to_string(), Vec::new(), TEXT_OR_NUMBER),
        };
        Prompt {
            message,
            keywords,
            default: None,
            accept,
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
        // Cancel (the input, or a "cancel" keyword/text) exits the command.
        match &i {
            Input::Cancel => return Ok(Step::Cancel),
            Input::Keyword(k) => {
                if k.to_ascii_lowercase() == "cancel" {
                    return Ok(Step::Cancel);
                }
            }
            Input::Text(t) => {
                if t.to_ascii_lowercase() == "cancel" {
                    return Ok(Step::Cancel);
                }
            }
            _ => {}
        }

        // Phase 0: start point (points only; anything else re-prompts).
        if self.phase == 0 {
            if let Input::Point(pt) = i {
                if pt.is_finite() {
                    self.start = Some(pt);
                    self.base = Some(pt);
                    self.phase = 1;
                }
            }
            return Ok(Step::Continue);
        }

        let Some(txt) = StairM::text_of(&i) else {
            // Enter etc.
            // Width is the only phase where "no answer" means "use the
            // default"; every required phase re-prompts.
            if self.phase == 2 {
                self.phase = 3;
            }
            return Ok(Step::Continue);
        };

        let txt = txt.trim();
        match self.phase {
            // Direction: required.
            1 => {
                if !txt.is_empty() {
                    self.direction = Some(txt.to_string());
                    self.phase = 2;
                }
                return Ok(Step::Continue);
            }
            // Width: optional; empty = default.
            2 => {
                if !txt.is_empty() {
                    self.width = Some(txt.to_string());
                }
                self.phase = 3;
                return Ok(Step::Continue);
            }
            // From level: required.
            3 => {
                if !txt.is_empty() {
                    self.from_level = Some(txt.to_string());
                    self.phase = 4;
                }
                return Ok(Step::Continue);
            }
            // To level: required; a non-empty answer finishes the command.
            4 | _ => {
                if !txt.is_empty() {
                    self.to_level = Some(txt.to_string());
                    return s.execute("stair", &self.build_params()).map(|_| Step::Done);
                }
                // Empty text / enter: to level is required, re-prompt.
                Ok(Step::Continue)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use cadcraft_doc::arch::RoomType;
    use serde_json::json;

    fn new_session() -> Session {
        Session::new()
    }

    /// Add a second level (Level 2 at 2.7 m) above the default Level 1.
    fn two_level_session() -> Session {
        let mut s = new_session();
        let _ = s.execute("level", &json!({"above": 0}));
        s
    }

    #[test]
    fn stair_command_16_risers_for_two_levels() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 })).unwrap();
        assert_eq!(r["id"].as_str().unwrap(), "s0");
        assert_eq!(r["risers"].as_u64(), Some(16));
        let riser_height = r["riserHeight"].as_f64().unwrap();
        let run = r["run"].as_f64().unwrap();
        assert!((riser_height - 0.16875).abs() < 1e-9);
        assert!((run - 3.81).abs() < 1e-6);
    }

    #[test]
    fn stair_command_refuses_unknown_level() {
        let mut s = new_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 99 }));
        assert!(r.is_err());
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }

    #[test]
    fn stair_command_refuses_spiral_shape() {
        let mut s = two_level_session();
        let r = s.execute(
            "stair",
            &json!({
                "start": [0.0, 0.0],
                "direction": 90.0,
                "fromLevel": 0,
                "toLevel": 1,
                "shape": "spiral",
            }),
        );
        assert!(r.is_err());
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }

    #[test]
    fn stair_command_creates_well_room_and_guard() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 })).unwrap();
        let d = s.doc().unwrap();
        assert!(r["well"].as_bool().unwrap());
        assert_eq!(d.arch.stairs.len(), 1);
        assert!(!d.arch.stairs[0].well.is_empty());
        assert_eq!(d.arch.stairs[0].well_room, Some(d.arch.rooms.last().unwrap().id));
        let room = d.arch.rooms.last().unwrap();
        assert_eq!(room.room_type, RoomType::OpenBelow);
        assert!(!room.polygon.is_empty());
        assert_eq!(d.arch.railings.len(), 1);
        assert_eq!(d.arch.railings[0].follows, Some(d.arch.stairs[0].id));
    }

    #[test]
    fn stair_command_writes_one_undo_step() {
        let mut s = two_level_session();
        let _ = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 }));
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 1);
        assert_eq!(s.undo().unwrap(), Some("Stair".to_string()));
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }
}
