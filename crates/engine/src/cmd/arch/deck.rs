//! The `deck` command: create a deck room from a drawn polygon.
//!
//! JSON parameters:
//! - `points`: required, `[[x, y], ...]` — a closed ring (the last point repeats
//!   the first within 1e-6). After dropping the trailing duplicate, 4 or more
//!   distinct points must form a simple polygon with finite area at least
//!   `MIN_ROOM_AREA`.
//! - `height`: optional, metres above grade; must be finite with `0 < h < 10`.
//!   Absent means the default deck height (0.9 m).
//! - `name`: optional, the deck's name.
//! - `level`: optional, a level's name or its numeric index; absent means index 0.
//!   Unknown names/indices are refused (never a silent default).
//!
//! Returns `{"id":"r0","handle":"...","deckHeight":h,"interiorArea":a}`.

use cadcraft_arch::poly::{ensure_ccw, is_simple, region_area, ring_area};
use cadcraft_doc::arch::{MAX_ROOMS, MIN_ROOM_AREA, Room, RoomType};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, distinct_points, points_param};
use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{EngineError, Result, Session};

/// Default deck height above grade (metres).
const DEFAULT_DECK_HEIGHT: f64 = 0.9;

/// Maximum accepted deck height (metres).
const MAX_DECK_HEIGHT: f64 = 10.0;

/// Command specs for the deck module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("deck", "Deck", run_deck)
            .menu(&["Architecture", "Deck"])
            .alias(&["dk"])
            .params("{points: closed ring [[x,y],...], height?, name?, level?}")
            .interactive(|_| Ok(Box::new(DeckM::default()))),
    ]
}

/// Resolve a level reference (a level name or its numeric index) to a position in
/// the arch model's levels. Mirrors `cmd/arch/room.rs`'s `find_room_level` but
/// reads a plain `&str`. An unknown name/index is an error, never a silent
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
    Err(EngineError::Other(format!("deck: unknown level `{ref_val}`")))
}

/// Run the `deck` command from JSON parameters. Never opens a dialog.
fn run_deck(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let arch = &d.arch;

    // --- Points: required, distinct, explicitly closed, >= 4, finite, simple,
    // CCW-normalised, finite area above MIN_ROOM_AREA ---
    let pts = points_param(p, "points").ok_or_else(|| bad("deck", "`points` [[x,y],...] (closed ring, 4 or more) is required"))?;
    let pts = distinct_points(&pts);
    // The ring must be explicitly closed: the last point repeats the first.
    if pts.len() < 2 || !pts[pts.len() - 1].near(pts[0], 1e-6) {
        return Err(bad("deck", "points must be a closed ring: repeat the first point at the end"));
    }
    // Drop the trailing duplicate.
    let ring: Vec<Vec2> = pts[..pts.len() - 1].to_vec();
    if ring.len() < 4 {
        return Err(bad("deck", "`points` [[x,y],...] must have 4 or more distinct points"));
    }
    for pt in &ring {
        if !pt.is_finite() {
            return Err(bad("deck", "non-finite coordinate in `points`"));
        }
    }
    if !is_simple(&ring) {
        return Err(bad("deck", "`points` do not form a simple polygon (self-intersecting or zero area)"));
    }
    let ring = ensure_ccw(&ring);
    // Reject non-finite or too-small areas (e.g. 1e308 corners).
    let area = ring_area(&ring);
    if !area.is_finite() || area < MIN_ROOM_AREA {
        return Err(bad("deck", "`points` do not enclose a valid deck area"));
    }

    // --- Height: optional, finite, 0 < h < 10 (default 0.9 m) ---
    let height = match p.get("height") {
        None | Some(Value::Null) => DEFAULT_DECK_HEIGHT,
        Some(v) => {
            let h = match v.as_f64() {
                Some(h) => h,
                _ => return Err(bad("deck", "`height` must be a number")),
            };
            if !h.is_finite() || !(h > 0.0 && h < MAX_DECK_HEIGHT) {
                return Err(bad("deck", format!("`height` {} must be finite and in (0, {})", h, MAX_DECK_HEIGHT)));
            }
            h
        }
    };

    // --- Level: name or index, else 0; unknown is an error, never a silent default ---
    let level = match p.get("level") {
        None => 0,
        Some(Value::Null) => 0,
        Some(Value::String(n)) => find_level(arch, n)?,
        Some(other) => return Err(bad("deck", format!("`level` must be a string (a level name or index), got `{other}`"))),
    };

    // --- Name ---
    let name = p.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_default();

    // --- Areas ---
    let interior_area = region_area(&ring, &[]);

    // --- Cap, then build and push (all checks above ran before touching the model) ---
    if d.arch.rooms.len() >= MAX_ROOMS {
        return Err(bad("deck", "room limit reached"));
    }
    // The standard area is measured against the surrounding walls, so the room must
    // first sit in the model. `standard_area` only reads `arch`; that immutable
    // borrow ends before `push_room` mutates `d`.
    let mut room = Room { name, room_type: RoomType::Deck, level: level as u32, polygon: ring, deck_height: Some(height), ..Room::default() };
    let standard_area = cadcraft_arch::area::standard_area(arch, &room);
    room.standard_area = standard_area;
    let id = d.arch.push_room(room);
    cadcraft_arch::area::refresh_areas(&mut d.arch);
    d.touch_arch();

    Ok(json!({
        "id": format!("r{}", id.0),
        "handle": format!("{:016X}", cadcraft_arch::room_handle(id).0),
        "deckHeight": height,
        "interiorArea": interior_area,
    }))
}

/// Interactive `deck` machine: collect points for a deck polygon, then finish with
/// the `Close` keyword or an empty Enter. The machine delegates to the JSON `deck`
/// command on finish so there is exactly one implementation path and one undo step.
pub struct DeckM {
    pts: Vec<Vec2>,
    base: Option<Vec2>,
}

impl Default for DeckM {
    fn default() -> Self {
        Self { pts: Vec::new(), base: None }
    }
}

impl Interactive for DeckM {
    fn name(&self) -> &'static str {
        "deck"
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
            // Fewer than 3 points: re-prompt instead of erroring (never a model change).
            if self.pts.len() < 3 {
                return Ok(Step::Continue);
            }
            let params = self.build_params();
            return s.execute("deck", &params).map(|_| Step::Done);
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

impl DeckM {
    /// Build the JSON parameter object the JSON `deck` command accepts. Only the
    /// fields the interactive form set (points) are emitted.
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

    /// A 3 x 3 m closed square (area 9.0).
    fn square_points() -> Value {
        json!([[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0], [0.0, 0.0]])
    }

    #[test]
    fn deck_is_a_deck_room_with_its_own_height() {
        let mut s = new_session();
        let r = s
            .execute(
                "deck",
                &json!({
                    "points": square_points(),
                    "height": 0.9,
                    "name": "Back Deck",
                }),
            )
            .unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.rooms.len(), 1);
        assert_eq!(d.arch.rooms[0].room_type, RoomType::Deck);
        assert_eq!(d.arch.rooms[0].deck_height, Some(0.9));
        assert_eq!(d.arch.rooms[0].name, "Back Deck");
        assert_eq!(r["id"].as_str().unwrap(), "r0");
        assert_eq!(r["deckHeight"].as_f64().unwrap(), 0.9);
        assert_eq!(r["interiorArea"].as_f64().unwrap(), 9.0);
    }

    #[test]
    fn deck_refuses_bad_height_and_open_ring() {
        let mut s = new_session();

        // Bad height: 0 (must be > 0).
        let r = s.execute("deck", &json!({ "points": square_points(), "height": 0.0 }));
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);

        // Bad height: negative.
        let r = s.execute("deck", &json!({ "points": square_points(), "height": -1.0 }));
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);

        // Bad height: too large (>= 10).
        let r = s.execute("deck", &json!({ "points": square_points(), "height": 11.0 }));
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);

        // Open ring (last point does not repeat the first).
        let r = s.execute(
            "deck",
            &json!({
                "points": [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]],
            }),
        );
        assert!(matches!(r, Err(EngineError::BadParams { cmd: _, msg: _ })));
        assert_eq!(s.doc().unwrap().arch.rooms.len(), 0);
    }
}
