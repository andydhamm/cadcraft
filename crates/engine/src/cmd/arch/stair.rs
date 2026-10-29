//! The `stair` command: lay out a stair flight between two levels.
//!
//! JSON parameters:
//!
//! - `start`: required, `[x, y]` — centre of the bottom nosing line.
//! - `direction`: required, degrees counter-clockwise from +x; stored in
//!   radians on the model.
//! - `width`: optional, metres; defaults to 1.0, must be finite in `(0, 5]`.
//! - `fromLevel` / `toLevel`: optional, a level name or its numeric index
//!   (index tried first, then name, like the `level` command). Absent (or
//!   null) means the first level / the last level; a present but unknown
//!   level is refused.
//! - `rise`: optional, metres; when present must be finite and positive. When
//!   absent it is derived as `to.elevation - from.elevation`, which must be
//!   finite and positive.
//! - `shape`: optional, `straight|l|u` (case-insensitive, default
//!   `straight`); only straight flights are meshed (phase-5 gap note).
//! - `tread` / `maxRiser` / `minTread`: optional positive metres; default to
//!   `MIN_TREAD` / `MAX_RISER` / `MIN_TREAD`.
//!
//! A well only exists when the flight climbs (`toLevel` above `fromLevel`); a
//! deck stair to grade makes none. Headroom is measured against the upper
//! level's slab underside (`to.elevation - to.structure`). When the upper
//! floor lacks `MIN_HEADROOM` clear of the flight, the stair's well becomes an
//! `Open Below` room on the upper level ringed by a guard railing that follows
//! the stair, and is cut as a hole into the upper room that contains it.
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

    // --- levels: optional (first / last); a present unknown level -> error ---
    let from_ref = p.get("fromLevel").filter(|v| !v.is_null());
    let to_ref = p.get("toLevel").filter(|v| !v.is_null());

    let d = s.doc_mut()?;
    let arch = &d.arch;
    let from_i = match from_ref {
        Some(v) => find_level(arch, v)?,
        None => 0,
    };
    let to_i = match to_ref {
        Some(v) => find_level(arch, v)?,
        None => arch.levels.len().saturating_sub(1),
    };
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
    let well_poly = if to_i > from_i { well_polygon(&stair, &plan, slab_underside, MIN_HEADROOM) } else { None };
    let has_well = well_poly.is_some();
    let guard = well_poly.as_ref().and_then(|_| well_guard_path(&stair, &plan, slab_underside, MIN_HEADROOM));
    let well = well_poly.unwrap_or_default();

    // --- one mutation + one revision bump; never a partial write ---
    let stair_id = d.arch.push_stair({
        stair.well = well.clone();
        stair
    });

    if has_well {
        // Cut the well out of the upper room that contains it (never invent
        // a room).
        if let Some(c) = cadcraft_geom::polygon_centroid(&well) {
            let target = d.arch.rooms.iter().position(|r| {
                r.level == to_i as u32
                    && cadcraft_geom::point_in_polygon(&r.polygon, c)
                    && !r.holes.iter().any(|h| cadcraft_geom::point_in_polygon(h, c))
            });
            if let Some(room) = target.and_then(|i| d.arch.rooms.get_mut(i)) {
                room.holes.push(well.clone());
            }
        }
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

    /// Push a 20 m square room on `level` centred on the origin.
    fn add_big_room(s: &mut Session, level: u32) {
        let d = s.doc_mut().unwrap();
        let polygon = vec![Vec2::new(-10.0, -10.0), Vec2::new(10.0, -10.0), Vec2::new(10.0, 10.0), Vec2::new(-10.0, 10.0)];
        d.arch.push_room(Room { name: "Hall".into(), level, polygon, ..Room::default() });
    }

    #[test]
    fn stair_without_levels_uses_first_and_last_level() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0 })).unwrap();
        assert_eq!(r["risers"].as_u64(), Some(16));
        let st = &s.doc().unwrap().arch.stairs[0];
        assert_eq!((st.from_level, st.to_level), (0, 1));
        // One level and no rise: nothing to derive, refused.
        let mut one = new_session();
        assert!(one.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0 })).is_err());
        assert_eq!(one.doc().unwrap().arch.stairs.len(), 0);
        // A present unknown level is still refused.
        assert!(s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "toLevel": 99 })).is_err());
    }

    #[test]
    fn deck_stair_to_grade_makes_no_well() {
        let mut s = two_level_session();
        add_big_room(&mut s, 0);
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 1, "toLevel": 0, "rise": 2.7 })).unwrap();
        assert_eq!(r["well"].as_bool(), Some(false));
        let d = s.doc().unwrap();
        assert!(d.arch.stairs[0].well.is_empty());
        assert!(d.arch.railings.is_empty());
        assert!(d.arch.rooms.iter().all(|r| r.room_type != RoomType::OpenBelow && r.holes.is_empty()));
    }

    #[test]
    fn stair_well_becomes_a_hole_of_the_upper_room() {
        let mut s = two_level_session();
        add_big_room(&mut s, 0);
        add_big_room(&mut s, 1);
        s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 })).unwrap();
        let d = s.doc().unwrap();
        let well = d.arch.stairs[0].well.clone();
        assert!(!well.is_empty());
        let upper = d.arch.rooms.iter().find(|r| r.level == 1 && r.name == "Hall").unwrap();
        assert_eq!(upper.holes, vec![well]);
        let lower = d.arch.rooms.iter().find(|r| r.level == 0).unwrap();
        assert!(lower.holes.is_empty());
    }

    #[test]
    fn stair_well_room_area_matches_the_flight() {
        let mut s = two_level_session();
        add_big_room(&mut s, 1);
        s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 })).unwrap();
        let d = s.doc().unwrap();
        let well_area = ring_area(&d.arch.stairs[0].well);
        assert!(well_area > 0.0);
        let upper = d.arch.rooms.iter().find(|r| r.name == "Hall").unwrap();
        assert!((upper.interior_area - (400.0 - well_area)).abs() < 1e-6, "{}", upper.interior_area);
        let open = d.arch.rooms.iter().find(|r| r.room_type == RoomType::OpenBelow).unwrap();
        assert!((open.interior_area - well_area).abs() < 1e-6);
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

    #[test]
    fn stair_command_refuses_same_level() {
        let mut s = new_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 0 }));
        assert!(r.is_err());
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }

    #[test]
    fn stair_command_explicit_rise() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1, "rise": 1.0 })).unwrap();
        // 1.0 m rise: n=6 gives 0.1667 m risers (closest to 0.16875 without exceeding 0.196)
        assert_eq!(r["risers"].as_u64(), Some(6));
        let riser_height = r["riserHeight"].as_f64().unwrap();
        assert!((riser_height - 1.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn stair_command_level_by_name() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": "Level 1", "toLevel": "Level 2" })).unwrap();
        assert_eq!(r["risers"].as_u64(), Some(16));
    }

    #[test]
    fn stair_command_width_zero() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "width": 0.0, "fromLevel": 0, "toLevel": 1 }));
        assert!(r.is_err());
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }

    #[test]
    fn stair_command_width_too_large() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "width": 5.1, "fromLevel": 0, "toLevel": 1 }));
        assert!(r.is_err());
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }

    #[test]
    fn stair_command_refuses_nan_start() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [f64::NAN, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 }));
        assert!(r.is_err());
        assert_eq!(s.doc().unwrap().arch.stairs.len(), 0);
    }

    #[test]
    fn stair_command_null_levels_treated_as_absent() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": null, "toLevel": null })).unwrap();
        assert_eq!(r["risers"].as_u64(), Some(16));
        let st = &s.doc().unwrap().arch.stairs[0];
        assert_eq!((st.from_level, st.to_level), (0, 1));
    }

    #[test]
    fn stair_command_three_levels_stair_between_first_and_last() {
        let mut s = two_level_session();
        let _ = s.execute("level", &json!({"above": 1}));
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0 })).unwrap();
        let st = &s.doc().unwrap().arch.stairs[0];
        assert_eq!((st.from_level, st.to_level), (0, 2));
        // Rise is 5.4 m (2 levels × 2.7 m); n minimises |5.4/n - 0.16875|
        // n=32 gives 0.16875 exactly
        assert_eq!(r["risers"].as_u64(), Some(32));
    }

    #[test]
    fn stair_command_well_with_no_containing_room() {
        let mut s = two_level_session();
        // No rooms on upper level — well is created but no hole is added
        s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "fromLevel": 0, "toLevel": 1 })).unwrap();
        let d = s.doc().unwrap();
        assert!(!d.arch.stairs[0].well.is_empty());
        // Open Below room exists but no other room has a hole
        let other_rooms: Vec<_> = d.arch.rooms.iter().filter(|r| r.room_type != RoomType::OpenBelow).collect();
        assert!(other_rooms.iter().all(|r| r.holes.is_empty()));
    }

    #[test]
    fn stair_command_width_boundary_five() {
        let mut s = two_level_session();
        let r = s.execute("stair", &json!({ "start": [0.0, 0.0], "direction": 90.0, "width": 5.0, "fromLevel": 0, "toLevel": 1 })).unwrap();
        assert_eq!(s.doc().unwrap().arch.stairs[0].width, 5.0);
    }
}
