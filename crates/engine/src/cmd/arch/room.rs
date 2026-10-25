//! The `room` command: create an architectural room from a drawn polygon.
//!
//! JSON parameters (create path):
//! - `points`: required, `[[x, y], ...]` with 3 or more distinct points that form a
//!   simple polygon. The ring is closed implicitly, so a trailing point that is
//!   `near` the first within `1e-9` is dropped before the checks.
//! - `name`: optional, the room's name.
//! - `type`: optional, case-insensitive
//!   `living|bedroom|kitchen|bath|closet|utility|garage|porch|deck|openbelow|open-below|open below|other`.
//!   Unknown values are refused (never coerced to `Other`).
//! - `level`: optional, a level's name or its numeric index; absent means index 0.
//!   Unknown names/indices are refused (never a silent default).
//! - `ceiling` / `floor`: optional positive heights, stored as the room's
//!   ceiling height / floor height.
//!
//! The `{id, ...}` editing path is refused; a later segment implements it.
//!
//! Returns `{"id": "r0", "handle": "...", "interiorArea": n, "standardArea": n}`.

use cadcraft_arch::poly::{ensure_ccw, is_simple, region_area};
use cadcraft_doc::arch::{ArchId, MAX_ROOMS, Room, RoomType};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, distinct_points, points_param, size_param, str_param};
use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{EngineError, Result, Session};

/// Command specs for the room module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("room", "Room", run_room)
            .menu(&["Architecture", "Room"])
            .alias(&["rm"])
            .params("{points: [[x,y],...], name?, type?, level?, ceiling?, floor?} | {id, name?, type?, ceiling?, floor?}")
            .interactive(|_| Ok(Box::new(RoomM::default()))),
    ]
}

/// Map a case-insensitive room-type name to a [`RoomType`].
///
/// Recognised names: `living`, `bedroom`, `kitchen`, `bath`, `closet`, `utility`,
/// `garage`, `porch`, `deck`, `openbelow` / `open-below` / `open below`, and `other`.
/// Unknown strings return [`None`]; the caller refuses them rather than coercing to
/// [`RoomType::Other`].
fn parse_room_type(s: &str) -> Option<RoomType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "living" => Some(RoomType::Living),
        "bedroom" => Some(RoomType::Bedroom),
        "kitchen" => Some(RoomType::Kitchen),
        "bath" => Some(RoomType::Bath),
        "closet" => Some(RoomType::Closet),
        "utility" => Some(RoomType::Utility),
        "garage" => Some(RoomType::Garage),
        "porch" => Some(RoomType::Porch),
        "deck" => Some(RoomType::Deck),
        "openbelow" | "open-below" | "open below" => Some(RoomType::OpenBelow),
        "other" => Some(RoomType::Other),
        _ => None,
    }
}

/// Resolve a level reference (a level name or its numeric index) to a position in the
/// arch model's levels. Mirrors `cmd/arch/level.rs`'s `find_level` but reads a plain `&str`.
/// An unknown name/index is an error, never a silent fallback to level 0.
fn find_room_level(arch: &cadcraft_doc::arch::ArchModel, ref_val: &str) -> Result<usize> {
    if let Some(i) = ref_val.trim().parse::<u64>().ok() {
        if i < arch.levels.len() as u64 {
            return Ok(i as usize);
        }
    }
    if let Some((i, l)) = arch.levels.iter().enumerate().find(|(_, l)| l.name == ref_val.trim()) {
        return Ok(i);
    }
    Err(EngineError::Other(format!("room: unknown level `{ref_val}`")))
}

/// Run the `room` command from JSON parameters (create path). Never opens a dialog.
fn run_room(s: &mut Session, p: &Value) -> Result<Value> {
    // Editing by `id` is a later segment.
    if p.get("id").is_some() {
        return Err(bad("room", "`id` (room editing) is not supported yet"));
    }

    let d = s.doc_mut()?;
    let arch = &d.arch;

    // --- Points: required, distinct, >= 3, all finite, simple, CCW-normalised ---
    let pts = points_param(p, "points").ok_or_else(|| bad("room", "`points` [[x,y],...] (3 or more) is required"))?;
    let pts = distinct_points(&pts);
    // The ring is closed implicitly: drop a trailing point coincident with the first.
    let ring: Vec<Vec2> = if pts.len() >= 2 && pts[pts.len() - 1].near(pts[0], 1e-9) { pts[..pts.len() - 1].to_vec() } else { pts };
    if ring.len() < 3 {
        return Err(bad("room", "`points` [[x,y],...] must have 3 or more distinct points"));
    }
    for pt in &ring {
        if !pt.is_finite() {
            return Err(bad("room", "non-finite coordinate in `points`"));
        }
    }
    if !is_simple(&ring) {
        return Err(bad("room", "`points` do not form a simple polygon (self-intersecting, zero area, or fewer than 3 points)"));
    }
    let ring = ensure_ccw(&ring);

    // --- Level: name or index, else 0; unknown is an error, never a silent default ---
    // `level` must be a string (a level name or a numeric index string). A present
    // non‑string (e.g. a bare JSON number) is malformed, not "absent".
    let level = match p.get("level") {
        None => 0,
        Some(Value::Null) => 0,
        Some(Value::String(n)) => find_room_level(arch, n)?,
        Some(other) => return Err(bad("room", format!("`level` must be a string (a level name or index), got `{other}`"))),
    };

    // --- Type: known names only; unknown is refused, absent is Other ---
    let room_type = match str_param(p, "type") {
        None => RoomType::default(),
        Some(t) => match parse_room_type(t) {
            Some(rt) => rt,
            None => return Err(bad("room", format!("unknown room type `{t}`"))),
        },
    };

    // --- Ceiling / floor: optional, positive, finite (refused otherwise) ---
    let ceiling = size_param("room", p, "ceiling", false)?;
    let floor = size_param("room", p, "floor", false)?;

    // --- Name ---
    let name = str_param(p, "name").map(|s| s.to_string()).unwrap_or_default();

    // --- Areas ---
    let interior_area = region_area(&ring, &[]);
    // `crates/arch/src/area.rs` does not yet expose a standard-area helper, so the
    // standard area equals the interior area for now (see `arch::area`).
    let standard_area = interior_area;

    // --- Cap, then build and push (all checks above ran before touching the model) ---
    if d.arch.rooms.len() >= MAX_ROOMS {
        return Err(bad("room", "room limit reached"));
    }
    let room = Room {
        id: ArchId(0),
        name,
        room_type,
        level: level as u32,
        polygon: ring,
        holes: Vec::new(),
        ceiling_height: ceiling,
        floor_height: floor,
        interior_area,
        standard_area,
    };
    let id = d.arch.push_room(room);
    d.touch_arch();

    Ok(json!({
        "id": format!("r{}", id.0),
        "handle": format!("{:016X}", cadcraft_arch::room_handle(id).0),
        "interiorArea": interior_area,
        "standardArea": standard_area,
    }))
}

/// Interactive `room` machine: collect points for a room polygon, then finish with the
/// `Close` keyword or an empty Enter. The machine delegates to the JSON `room` command
/// on finish so there is exactly one implementation path and one undo step.
pub struct RoomM {
    pts: Vec<Vec2>,
    base: Option<Vec2>,
}

impl Default for RoomM {
    fn default() -> Self {
        Self { pts: Vec::new(), base: None }
    }
}

impl Interactive for RoomM {
    fn name(&self) -> &'static str {
        "room"
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
            // Fewer than 3 points: re-prompt instead of erroring (never a model change).
            if self.pts.len() < 3 {
                return Ok(Step::Continue);
            }
            let params = self.build_params();
            return s.execute("room", &params).map(|_| Step::Done);
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

impl RoomM {
    /// Build the JSON parameter object the JSON `room` command accepts. Only the fields the
    /// interactive form set (points) are emitted.
    fn build_params(&self) -> Value {
        let points: Vec<Value> = self.pts.iter().map(|p| json!([p.x, p.y])).collect();
        json!({ "points": points })
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

    /// A 3 x 3 m square (area 9.0).
    fn square_points() -> Value {
        json!([[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]])
    }

    #[test]
    fn room_command_creates_a_room() {
        let mut s = new_session();
        let before = s.doc().unwrap().arch.rooms.len();
        let p = json!({ "points": square_points() });
        let r = s.execute("room", &p).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.rooms.len(), before + 1);
        assert_eq!(r["id"].as_str().unwrap(), "r0");
        assert_eq!(r["interiorArea"].as_f64().unwrap(), 9.0);
        assert_eq!(r["standardArea"].as_f64().unwrap(), 9.0);
        assert_eq!(r["handle"], "0000A01000000000");
        assert_eq!(d.arch.room(d.arch.rooms.last().unwrap().id).unwrap().polygon.len(), 4);
        assert_eq!(d.arch.room(d.arch.rooms.last().unwrap().id).unwrap().level, 0);
        assert_eq!(d.arch.room(d.arch.rooms.last().unwrap().id).unwrap().room_type, RoomType::Other);
    }

    #[test]
    fn room_rejects_an_unknown_type() {
        let mut s = new_session();
        let p = json!({
            "points": square_points(),
            "type": "bogus",
        });
        let r = s.execute("room", &p);
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);
    }

    #[test]
    fn room_rejects_an_unclosed_polygon() {
        // A self-intersecting "polygon" is not a valid closed region.
        let mut s = new_session();
        let p = json!({ "points": [[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]] });
        let r = s.execute("room", &p);
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);

        // And a 2-point "polygon" (after implicit closure) is also rejected.
        let p = json!({ "points": [[0.0, 0.0], [3.0, 0.0]] });
        assert!(s.execute("room", &p).is_err());
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);
    }

    #[test]
    fn room_command_is_undoable() {
        let mut s = new_session();
        let before = s.doc().unwrap().arch.rooms.len();
        let p = json!({ "points": square_points() });
        let _ = s.execute("room", &p).unwrap();
        assert_eq!(s.doc().unwrap().arch.rooms.len(), before + 1);
        assert_eq!(s.undo().unwrap(), Some("Room".to_string()));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), before);
    }

    #[test]
    fn room_command_never_panics_on_garbage() {
        let mut s = new_session();
        let garbage = [
            json!({}),
            json!({"points": "nope"}),
            json!({"points": []}),
            json!({"points": [[1.0, 1.0], [1.0, 1.0], [2.0, 2.0]]}),
            json!({"points": [[f64::NAN, 0.0], [1.0, 0.0], [1.0, 1.0]]}),
            json!({"points": [[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]]}),
            json!({"id": "r0"}),
            json!({"points": square_points(), "level": "nope"}),
            json!({"points": square_points(), "level": 7}),
        ];
        for g in garbage {
            let r = s.execute("room", &g);
            assert!(r.is_err(), "garbage `{g}` should be refused");
            assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);
        }
    }
}
