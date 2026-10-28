//! JSON-only opening commands (`door`, `window`, `garagedoor`).
//!
//! These commands place an [`Opening`](cadcraft_doc::arch::Opening) on an existing
//! wall and are reachable through `Session::execute`, the MCP `execute` tool and
//! the CLI.
//!
//! The interactive machines (`arch.opening.move`, grip-drag along the wall) land
//! in follow-up segments; nothing interactive is wired up in this file yet.

use serde_json::Value;
use serde_json::json;
use std::str::FromStr;

use crate::EngineError;
use crate::Session;
use crate::cmd::CommandSpec;

use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use cadcraft_arch::opening::{place_opening, project_to_wall, validate_placement};
use cadcraft_doc::ArchId;
use cadcraft_doc::arch::{Hinge, MAX_OPENINGS, Opening, OpeningKind, Swing, arch_handle, opening_handle, opening_label, wall_label};
use cadcraft_doc::{ARCH_HANDLE_BASE, Handle, is_arch_handle};
use cadcraft_geom::Vec2;

/// Allowed door styles.
const DOOR_STYLES: [&str; 4] = ["hinged", "sliding", "pocket", "bifold"];

/// Allowed window styles.
const WINDOW_STYLES: [&str; 10] = ["single-hung", "double-hung", "slider", "casement", "picture", "bay", "box", "bow", "awning", "fixed"];

/// Allowed garage-door styles.
const GARAGE_STYLES: [&str; 2] = ["one-piece", "sectional"];

/// Which number the machine is currently prompting for.
#[derive(Clone, Copy, PartialEq)]
enum NumTarget {
    Width,
    Height,
    Sill,
}

/// Interactive machine shared by the `door`, `window` and `garagedoor` commands.
///
/// Stage 1: select a host wall (pick an arch handle, or click near a wall's centre line).
/// Stage 2: specify the offset (as a point or number) and optionally set
/// `width`/`height`/`sill`/`hinge`/`swing` via keywords.
///
/// The machine delegates to the JSON command on finish, so the whole interaction is
/// one undo step and there is a single implementation path.
pub struct OpeningM {
    id: &'static str,
    kind: OpeningKind,
    host: Option<ArchId>,
    host_point: Option<Vec2>,
    offset: Option<f64>,
    point: Option<Vec2>,
    width: Option<f64>,
    height: Option<f64>,
    sill: Option<f64>,
    hinge: Option<&'static str>,
    swing: Option<&'static str>,
    num_target: Option<NumTarget>,
}

impl OpeningM {
    /// Create a new machine for the given opening kind.
    pub fn new(kind: OpeningKind) -> Self {
        let id = match kind {
            OpeningKind::Door => "door",
            OpeningKind::Window => "window",
            OpeningKind::GarageDoor => "garagedoor",
            _ => "door",
        };
        Self {
            id,
            kind,
            host: None,
            host_point: None,
            offset: None,
            point: None,
            width: None,
            height: None,
            sill: None,
            hinge: None,
            swing: None,
            num_target: None,
        }
    }

    /// Toggle the hinge value between `"left"` and `"right"`.
    fn toggle_hinge(current: Option<&'static str>) -> Option<&'static str> {
        match current {
            None => Some("left"),
            Some("left") => Some("right"),
            _ => Some("left"),
        }
    }

    /// Toggle the swing value between `"in"` and `"out"`.
    fn toggle_swing(current: Option<&'static str>) -> Option<&'static str> {
        match current {
            None => Some("in"),
            Some("in") => Some("out"),
            _ => Some("in"),
        }
    }

    /// Parse `txt` as the number requested by `num_target`.
    /// Accepts only finite values: `> 0.0` for width/height, `>= 0.0` for sill.
    /// Re-prompts (`Step::Continue`) on invalid input, mirroring `WallM::parse_number`.
    fn parse_number(&mut self, txt: &str) -> Result<Step, EngineError> {
        let v = match txt.trim().parse::<f64>() {
            Ok(v) if v.is_finite() => v,
            _ => return Ok(Step::Continue),
        };
        let ok = match self.num_target {
            Some(NumTarget::Sill) => v >= 0.0,
            _ => v > 0.0,
        };
        if !ok {
            return Ok(Step::Continue);
        }
        match self.num_target {
            Some(NumTarget::Width) => self.width = Some(v),
            Some(NumTarget::Height) => self.height = Some(v),
            Some(NumTarget::Sill) => self.sill = Some(v),
            None => {}
        }
        self.num_target = None;
        Ok(Step::Continue)
    }

    /// Build the JSON parameter map and execute the JSON command.
    /// Prefers `offset` over `point` when both are set.
    fn finish(&self, s: &mut Session) -> Result<Step, EngineError> {
        let Some(host) = self.host else {
            return Ok(Step::Continue);
        };
        let mut map = serde_json::Map::new();
        map.insert("wall".to_string(), json!(host.to_string()));
        if let Some(off) = self.offset {
            map.insert("offset".to_string(), json!(off));
        } else if let Some(pt) = self.point {
            map.insert("point".to_string(), json!([pt.x, pt.y]));
        }
        if let Some(w) = self.width {
            map.insert("width".to_string(), json!(w));
        }
        if let Some(h) = self.height {
            map.insert("height".to_string(), json!(h));
        }
        if let Some(s) = self.sill {
            map.insert("sill".to_string(), json!(s));
        }
        if let Some(h) = self.hinge {
            map.insert("hinge".to_string(), json!(h));
        }
        if let Some(s) = self.swing {
            map.insert("swing".to_string(), json!(s));
        }
        s.execute(self.id, &Value::Object(map)).map(|_| Step::Done)
    }
}

impl Interactive for OpeningM {
    fn name(&self) -> &'static str {
        self.id
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        // Stage 1: select a wall.
        if self.host.is_none() {
            return Prompt::new("Select wall:", Accept::SELECT);
        }
        // Stage 2: specify offset or params.
        if let Some(target) = self.num_target {
            let msg = match target {
                NumTarget::Width => "Specify width:".to_string(),
                NumTarget::Height => "Specify height:".to_string(),
                NumTarget::Sill => "Specify sill:".to_string(),
            };
            return Prompt::new(msg, Accept::TEXT);
        }
        Prompt::new("Specify offset or [Width/Height/Sill/Hinge/Swing]:", Accept::POINT)
            .kw(&["Width", "Height", "Sill", "Hinge", "Swing"])
            .base_opt(self.host_point)
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step, EngineError> {
        match i {
            // --- Stage 1: wall selection ---
            Input::Pick(handles) if self.host.is_none() => {
                for h in &handles {
                    if !is_arch_handle(*h) {
                        continue;
                    }
                    let raw = (*h).0 - ARCH_HANDLE_BASE;
                    let id = match raw.try_into() {
                        Ok(v) => ArchId(v),
                        Err(_) => continue,
                    };
                    let Ok(d) = s.doc() else {
                        continue;
                    };
                    if d.arch.wall(id).is_some() {
                        self.host = Some(id);
                        return Ok(Step::Continue);
                    }
                }
                Ok(Step::Continue)
            }
            Input::Point(pt) if self.host.is_none() => {
                if !pt.is_finite() {
                    return Ok(Step::Continue);
                }
                let Ok(d) = s.doc() else {
                    return Ok(Step::Continue);
                };
                let mut best: Option<(f64, ArchId)> = None;
                for w in &d.arch.walls {
                    let dir = w.end - w.start;
                    let dir_len = dir.len();
                    if dir_len < 1e-12 {
                        continue;
                    }
                    let to_pt = pt - w.start;
                    let cross = to_pt.x * dir.y - to_pt.y * dir.x;
                    let perp_dist = cross.abs() / dir_len;
                    if perp_dist <= w.thickness / 2.0 + 1e-6 && project_to_wall(&d.arch, w.id, pt).is_some() {
                        let candidate = (perp_dist, w.id);
                        if best.map_or(true, |b| candidate.0 < b.0) {
                            best = Some(candidate);
                        }
                    }
                }
                match best {
                    Some((_dist, id)) => {
                        self.host = Some(id);
                        self.host_point = Some(pt);
                        self.point = Some(pt);
                        Ok(Step::Continue)
                    }
                    None => Ok(Step::Continue),
                }
            }
            Input::Cancel => Ok(Step::Cancel),
            Input::Enter if self.host.is_none() => Ok(Step::Continue),

            // --- Stage 2: offset and params ---
            Input::Point(pt) if self.host.is_some() && self.num_target.is_none() => {
                if !pt.is_finite() {
                    return Ok(Step::Continue);
                }
                self.point = Some(pt);
                self.host_point = Some(pt);
                self.finish(s)
            }
            Input::Text(txt) if self.host.is_some() => {
                if self.num_target.is_some() {
                    return self.parse_number(&txt);
                }
                if txt.trim().is_empty() {
                    return self.finish(s);
                }
                let lower = txt.to_ascii_lowercase().trim().to_string();
                match lower.as_str() {
                    "width" => {
                        self.num_target = Some(NumTarget::Width);
                        Ok(Step::Continue)
                    }
                    "height" => {
                        self.num_target = Some(NumTarget::Height);
                        Ok(Step::Continue)
                    }
                    "sill" => {
                        self.num_target = Some(NumTarget::Sill);
                        Ok(Step::Continue)
                    }
                    "hinge" => {
                        self.hinge = Self::toggle_hinge(self.hinge);
                        Ok(Step::Continue)
                    }
                    "swing" => {
                        self.swing = Self::toggle_swing(self.swing);
                        Ok(Step::Continue)
                    }
                    _ => {
                        if let Ok(v) = txt.trim().parse::<f64>()
                            && v.is_finite()
                        {
                            self.offset = Some(v);
                            return self.finish(s);
                        }
                        Ok(Step::Continue)
                    }
                }
            }
            Input::Enter if self.host.is_some() => self.finish(s),
            _ => Ok(Step::Continue),
        }
    }

    /// Returns an empty preview (the opening is only committed via the JSON command).
    fn preview(&self, _s: &Session, _cursor: Vec2) -> Vec<cadcraft_doc::EntityKind> {
        Vec::new()
    }
}

/// Parse a user-supplied id string into an [`ArchId`](cadcraft_doc::ArchId).
///
/// For the **wall** space: accepts `w{n}`, `W{n}`, or bare `{n}`.
/// For the **opening** space: accepts `o{n}`, `O{n}` only.
///
/// The caller validates that the returned id exists in the correct collection
/// (wall vs opening); this function only parses the prefix and numeric part.
fn parse_arch_id(s: &str, space: &str) -> Result<ArchId, String> {
    let s = s.trim();
    let digits = if space == "opening" {
        // Accepts o{n}, O{n}, w{n}, W{n}, or bare {n}
        let start = match s.as_bytes().first() {
            Some(b) if *b == b'o' || *b == b'O' || *b == b'w' || *b == b'W' => 1,
            _ => 0,
        };
        &s[start..]
    } else {
        // wall space: accepts w{n}, W{n}, or bare {n}
        let start = match s.as_bytes().first() {
            Some(b) if *b == b'w' || *b == b'W' => 1,
            _ => 0,
        };
        &s[start..]
    };
    if digits.is_empty() {
        return Err(format!("empty id `{s}`"));
    }
    let n: ArchId = digits.parse().map_err(|_| format!("invalid id `{s}`"))?;
    Ok(n)
}

/// Place an opening on a wall from JSON parameters and return a summary.
fn place_from_params(cmd: &'static str, kind: OpeningKind, s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let d = s.doc_mut()?;

    // 1. Host wall.
    let wall_s = crate::cmd::str_param(p, "wall").ok_or_else(|| crate::cmd::bad(cmd, "`wall` (string id) is required"))?;
    let host: ArchId = parse_arch_id(wall_s, "wall").map_err(|e| crate::cmd::bad(cmd, e))?;
    if d.arch.wall(host).is_none() {
        return Err(crate::cmd::bad(cmd, format!("no wall {host}")));
    }

    // 2. Offset: explicit number or projection of a point.
    let offset: f64 = if let Some(off) = p.get("offset").and_then(Value::as_f64) {
        if !off.is_finite() {
            return Err(crate::cmd::bad(cmd, "`offset` must be a finite number"));
        }
        off
    } else if let Some(pt) = crate::cmd::point_param(p, "point") {
        project_to_wall(&d.arch, host, pt).ok_or_else(|| crate::cmd::bad(cmd, "point does not project onto the wall"))?
    } else {
        return Err(crate::cmd::bad(cmd, "`offset` (number) or `point` ([x,y]) is required"));
    };

    // 3. Sizes with per-kind defaults.
    let (def_width, def_height, def_sill) = match kind {
        OpeningKind::Door => (0.9, 2.1, 0.0),
        OpeningKind::Window | OpeningKind::CasedOpening => (1.2, 1.2, 0.9),
        OpeningKind::GarageDoor => (2.4, 2.1, 0.0),
    };
    let width = crate::cmd::size_param(cmd, p, "width", false)?.unwrap_or(def_width);
    let height = crate::cmd::size_param(cmd, p, "height", false)?.unwrap_or(def_height);
    let sill = crate::cmd::size_param(cmd, p, "sill", true)?.unwrap_or(def_sill);
    let recess = crate::cmd::size_param(cmd, p, "recess", true)?.unwrap_or(0.0);
    let casing = crate::cmd::size_param(cmd, p, "casing", true)?.unwrap_or(0.0);

    // 4. Style (validated against the per-kind whitelist). Only an explicit
    // `style` param overrides the kind default (`hinged` / `slider` /
    // `garage` / `cased`) that `Opening::new` stores; an omitted style keeps it.
    let style: Option<String> = if let Some(st) = crate::cmd::str_param(p, "style") {
        let lower = st.to_ascii_lowercase();
        let valid: &[&str] = match kind {
            OpeningKind::Door => DOOR_STYLES.as_slice(),
            OpeningKind::Window | OpeningKind::CasedOpening => WINDOW_STYLES.as_slice(),
            OpeningKind::GarageDoor => GARAGE_STYLES.as_slice(),
        };
        if !valid.contains(&lower.as_str()) {
            return Err(crate::cmd::bad(cmd, format!("unknown style `{lower}` for {kind:?}")));
        }
        Some(lower)
    } else {
        None
    };

    // 5. Hinge and swing (case-insensitive; unknown is a bad-params error).
    let mut hinge = Hinge::Left;
    if let Some(h) = crate::cmd::str_param(p, "hinge") {
        hinge = match h.to_ascii_lowercase().as_str() {
            "left" => Hinge::Left,
            "right" => Hinge::Right,
            _ => return Err(crate::cmd::bad(cmd, format!("unknown hinge `{h}`"))),
        };
    }
    let mut swing = Swing::In;
    if let Some(sv) = crate::cmd::str_param(p, "swing") {
        swing = match sv.to_ascii_lowercase().as_str() {
            "in" => Swing::In,
            "out" => Swing::Out,
            _ => return Err(crate::cmd::bad(cmd, format!("unknown swing `{sv}`"))),
        };
    }

    // 6. Level: accepted and validated against `d.arch.levels`; not yet
    // stored on the opening (follow-up segment).
    if let Some(lv) = crate::cmd::str_param(p, "level") {
        if !d.arch.levels.iter().any(|l| l.name == lv) {
            return Err(crate::cmd::bad(cmd, format!("unknown level `{lv}`")));
        }
    }

    let fit = crate::cmd::bool_or(p, "fit", false);

    // 7. Capacity.
    if d.arch.openings.len() >= MAX_OPENINGS {
        return Err(crate::cmd::bad(cmd, "max number of openings reached"));
    }

    // 8. Construct, place, persist. The kind default style stands unless the
    // `style` param was supplied; `sill_depth` is the sill board's projection,
    // not the sill height, so it keeps its `Opening::new` value (0.03).
    let requested_width = width;
    let mut o = Opening::new(host, offset, width, height, kind);
    if let Some(s) = style {
        o.style = s;
    }
    o.sill = sill;
    o.recess = recess;
    o.casing_width = casing;
    o.hinge = hinge;
    o.swing = swing;

    let placed = place_opening(&d.arch, o, fit).map_err(|e| crate::cmd::bad(cmd, e.to_string()))?;

    let placed_width = placed.width;
    let resized = (placed_width - requested_width).abs() > 1e-9;

    // Ensure the save-time proxy layer for this opening kind is present before
    // inserting; arch layers are not in a fresh drawing and are created on first
    // use (doors/garages/cased on `A-DOOR`, windows on `A-GLAZ`).
    d.ensure_layer(match kind {
        OpeningKind::Door | OpeningKind::CasedOpening | OpeningKind::GarageDoor => "A-DOOR",
        _ => "A-GLAZ",
    });

    let id = d.arch.push_opening(placed);
    d.touch_arch();

    Ok(json!({
        "openings": [
            {
                "id": opening_label(id),
                "handle": format!("{:x}", opening_handle(id).0),
                "host": wall_label(host),
                "offset": offset,
                "width": placed_width,
                "resized": resized,
            }
        ],
        "revision": d.arch_revision,
    }))
}

fn run_door(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    place_from_params("door", OpeningKind::Door, s, p)
}

fn run_window(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    place_from_params("window", OpeningKind::Window, s, p)
}

fn run_garagedoor(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    place_from_params("garagedoor", OpeningKind::GarageDoor, s, p)
}

/// Move an existing opening along its host wall.
///
/// `opening` is the id of the opening to move (accepts `w{n}`, `o{n}` or a bare
/// number); `offset` gives the new distance from the wall start, or `point`
/// ([x,y]) which is projected onto the wall. The opening is re-validated against
/// the host wall (a position that runs past either end, leaving less than the
/// minimum wall segment, is rejected). Undoable by default.
pub(crate) fn run_opening_move(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let cmd = "arch.opening.move";

    // 1. Host opening id.
    let id_s = crate::cmd::str_param(p, "opening").ok_or_else(|| crate::cmd::bad(cmd, "`opening` (string id) is required"))?;
    let id: ArchId = parse_arch_id(id_s, "opening").map_err(|e| crate::cmd::bad(cmd, e))?;

    // 2. Copy the opening's data into locals before touching `d.arch` again:
    // `d.arch.opening(id)` is a borrow that would conflict with the later
    // `d.arch.openings[i].offset = ...` mutation, so nothing is kept alive.
    let d = s.doc_mut()?;
    let (host, width, sill, height, kind, style) = match d.arch.opening(id) {
        Some(o) => (o.host, o.width, o.sill, o.height, o.kind, o.style.clone()),
        None => return Err(crate::cmd::bad(cmd, format!("no opening {id}"))),
    };

    // 3. New offset: explicit number, or a point projected onto the host wall.
    let new_offset = match crate::cmd::f64_req(cmd, p, "offset") {
        Ok(off) => off,
        Err(_) => {
            if let Some(pt) = crate::cmd::point_param(p, "point") {
                cadcraft_arch::opening::project_to_wall(&d.arch, host, pt)
                    .ok_or_else(|| crate::cmd::bad(cmd, "point does not project onto the wall"))?
            } else {
                return Err(crate::cmd::bad(cmd, "`offset` (number) or `point` ([x,y]) is required"));
            }
        }
    };

    // 4. Re-validate on the host wall. `ignore = Some(id)` so the opening is
    // not compared against itself.
    cadcraft_arch::opening::validate_placement(&d.arch, host, new_offset, width, sill, height, kind, &style, Some(id))
        .map_err(|e| crate::cmd::bad(cmd, e.to_string()))?;

    // 5. Write the new offset back through the index (there is no setter).
    if let Some(i) = d.arch.opening_index(id) {
        d.arch.openings[i].offset = new_offset;
    }
    d.touch_arch();

    // 6. Summary.
    Ok(json!({
        "moved": id.to_string(),
        "offset": new_offset,
        "revision": d.arch_revision,
    }))
}

/// Command specs for the opening module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("door", "Door", run_door)
            .menu(&["Architecture", "Door"])
            .alias(&["dr"])
            .params("{wall:\"w0\", offset?|point?, width?, height?, style?, hinge?, swing?, recess?, casing?, fit?, level?}")
            .interactive(|_| Ok(Box::new(OpeningM::new(OpeningKind::Door)))),
        CommandSpec::new("window", "Window", run_window)
            .menu(&["Architecture", "Window"])
            .alias(&["wn"])
            .params("{wall:\"w0\", offset?|point?, width?, height?, style?, recess?, casing?, fit?, level?}")
            .interactive(|_| Ok(Box::new(OpeningM::new(OpeningKind::Window)))),
        CommandSpec::new("garagedoor", "Garage Door", run_garagedoor)
            .menu(&["Architecture", "Garage Door"])
            .alias(&["gd"])
            .params("{wall:\"w0\", offset?|point?, width?, height?, style?, recess?, casing?, fit?, level?}")
            .interactive(|_| Ok(Box::new(OpeningM::new(OpeningKind::GarageDoor)))),
        CommandSpec::new("arch.opening.move", "Move Opening", run_opening_move).params("{opening:\"w0\", offset?|point?}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A session with one 10-unit wall at the origin (host `w0`).
    fn session_with_wall() -> Session {
        let mut s = Session::new();
        let r = s.execute("wall", &json!({"points": [[0.0, 0.0], [10.0, 0.0]]}));
        r.expect("create wall");
        s
    }

    /// A session with the 10 m wall `w0` and one 0.9 m door `o0` at offset 3.0.
    fn session_with_door() -> Session {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("create door");
        s
    }

    #[test]
    fn door_reports_an_o_prefixed_id_and_a000_handle() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("door execute");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert_eq!(one["id"], json!("o0"), "id should be o0");
        let handle = one["handle"].as_str().expect("handle is a string");
        assert!(handle.starts_with("ffffffff"), "handle should start with ffffffff (arch-proxy base), got {handle}");
        assert_eq!(one["host"], json!("w0"), "host should be w0");
    }

    #[test]
    fn door_command_places_an_opening_and_bumps_revision() {
        let mut s = session_with_wall();
        let rev_before = s.doc().expect("doc").arch_revision;
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("door execute");
        let rev_after = s.doc().expect("doc").arch_revision;
        assert_ne!(rev_before, rev_after, "arch revision did not change");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert_eq!(one["host"], json!("w0"));
        assert_eq!(one["offset"], json!(3.0));
        assert_eq!(one["width"], json!(0.9));
        assert_eq!(one["resized"], json!(false));
        assert_eq!(r["revision"], json!(rev_after));
    }

    #[test]
    fn window_stores_the_sill_and_recess() {
        let mut s = session_with_wall();
        s.execute("window", &json!({"wall": "w0", "offset": 2.0, "sill": 0.9, "recess": 0.05})).expect("window execute");
        let d = s.doc().expect("doc");
        let opening = &d.arch.openings[0];
        assert!((opening.sill - 0.9).abs() < 1e-9, "sill should be 0.9, got {}", opening.sill);
        assert!((opening.sill_depth - 0.03).abs() < 1e-9, "sill_depth should stay at the default 0.03, got {}", opening.sill_depth);
        assert!((opening.recess - 0.05).abs() < 1e-9, "recess should be 0.05, got {}", opening.recess);
    }

    /// A door with no `style` param keeps the kind default (`"hinged"`).
    #[test]
    fn door_without_style_keeps_the_hinged_default() {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("door execute");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.openings[0].style, "hinged");
    }

    /// A garage door with no `style` param keeps the kind default (`"garage"`).
    #[test]
    fn garagedoor_without_style_keeps_the_garage_style() {
        let mut s = session_with_wall();
        s.execute("garagedoor", &json!({"wall": "w0", "offset": 4.0, "width": 1.8, "height": 2.1})).expect("garage door execute");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.openings[0].style, "garage");
    }

    /// A window's `sill` height is stored separately from `sill_depth`; the
    /// latter is the sill board's projection and keeps the `Opening::new`
    /// default (0.03).
    #[test]
    fn window_sill_does_not_overwrite_sill_depth() {
        let mut s = session_with_wall();
        s.execute("window", &json!({"wall": "w0", "offset": 2.0, "width": 1.2, "height": 1.2, "sill": 0.9, "recess": 0.05})).expect("window execute");
        let d = s.doc().expect("doc");
        let opening = &d.arch.openings[0];
        assert!((opening.sill - 0.9).abs() < 1e-9, "sill should be 0.9, got {}", opening.sill);
        assert!((opening.sill_depth - 0.03).abs() < 1e-9, "sill_depth should stay at the default 0.03, got {}", opening.sill_depth);
    }

    #[test]
    fn garagedoor_command_enforces_the_minimum_width() {
        let mut s = session_with_wall();
        let r = s.execute("garagedoor", &json!({"wall": "w0", "offset": 2.0, "width": 1.0}));
        let err = r.expect_err("garage door with width 1.0 < 1.8 m min should fail");
        assert!(err.to_string().contains("1.8"), "expected min-width error, got: {err}");
    }

    #[test]
    fn door_command_accepts_a_point_instead_of_an_offset() {
        let mut s = session_with_wall();
        // Wall w0 runs (0,0) -> (10,0); (3,0) projects to offset 3.0.
        let r = s.execute("door", &json!({"wall": "w0", "point": [3.0, 0.0], "width": 0.9, "height": 2.1})).expect("door with point execute");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert_eq!(one["offset"], json!(3.0));
    }

    #[test]
    fn unknown_style_is_a_bad_params_error() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 1.0, "width": 0.9, "height": 2.1, "style": "spinning"}));
        let err = r.expect_err("unknown door style should be rejected");
        assert!(err.to_string().contains("unknown style"), "expected unknown-style error, got: {err}");
    }

    #[test]
    fn missing_offset_is_a_bad_params_error() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "width": 0.9, "height": 2.1}));
        let err = r.expect_err("missing offset/point should be rejected");
        assert!(err.to_string().contains("`offset` (number) or `point`"), "expected missing-offset error, got: {err}");
    }

    #[test]
    fn door_command_is_undoable() {
        let mut s = session_with_wall();
        let before = s.doc().expect("doc").arch.openings.len();
        s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1})).expect("door execute");
        let mid = s.doc().expect("doc").arch.openings.len();
        assert_eq!(mid, before + 1, "opening should be added");
        s.undo().expect("undo after door");
        let after = s.doc().expect("doc").arch.openings.len();
        assert_eq!(after, before, "opening should be removed by undo");
    }

    #[test]
    fn each_opening_command_is_one_undo_step() {
        for id in ["door", "window", "garagedoor"] {
            let mut s = session_with_wall();
            let before = s.doc().expect("doc").arch.openings.len();
            s.start(id).expect("start");
            s.input(Input::Pick(vec![cadcraft_doc::arch_handle(ArchId(0))])).expect("pick");
            s.input(Input::Text("3.0".into())).expect("offset");
            s.input(Input::Enter).expect("finish");
            let after = s.doc().expect("doc").arch.openings.len();
            assert_eq!(after, before + 1, "should have added one opening for {id}");
            s.undo().expect("undo");
            let after_undo = s.doc().expect("doc").arch.openings.len();
            assert_eq!(after_undo, before, "undo should remove the opening for {id}");
        }
    }

    #[test]
    fn opening_machine_selects_a_wall_then_an_offset() {
        let mut s = session_with_wall();
        let before = s.doc().expect("doc").arch.openings.len();
        s.start("door").expect("start");
        s.input(Input::Pick(vec![cadcraft_doc::arch_handle(ArchId(0))])).expect("pick");
        s.input(Input::Text("3.0".into())).expect("offset");
        s.input(Input::Enter).expect("finish");
        let after = s.doc().expect("doc").arch.openings.len();
        assert_eq!(after, before + 1, "should have added one opening");
        let opening = &s.doc().expect("doc").arch.openings[after - 1];
        assert_eq!(opening.offset, 3.0, "offset should be 3.0");
    }

    #[test]
    fn opening_machine_cancel_leaves_the_model_unchanged() {
        let mut s = session_with_wall();
        let before = s.doc().expect("doc").arch.openings.len();
        s.start("door").expect("start");
        s.input(Input::Pick(vec![cadcraft_doc::arch_handle(ArchId(0))])).expect("pick");
        s.input(Input::Cancel).expect("cancel");
        let after = s.doc().expect("doc").arch.openings.len();
        assert_eq!(after, before, "cancel should not add any opening");
    }

    #[test]
    fn opening_machine_keyword_changes_the_width() {
        let mut s = session_with_wall();
        let before = s.doc().expect("doc").arch.openings.len();
        s.start("door").expect("start");
        s.input(Input::Pick(vec![cadcraft_doc::arch_handle(ArchId(0))])).expect("pick");
        s.input(Input::Text("Width".into())).expect("width keyword");
        s.input(Input::Text("1.5".into())).expect("width value");
        s.input(Input::Text("3.0".into())).expect("offset");
        s.input(Input::Enter).expect("finish");
        let after = s.doc().expect("doc").arch.openings.len();
        assert_eq!(after, before + 1, "should have added one opening");
        let opening = &s.doc().expect("doc").arch.openings[after - 1];
        assert_eq!(opening.width, 1.5, "width should be 1.5 (user-set), not the default 0.9");
    }

    #[test]
    fn opening_move_updates_the_offset() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 6.5})).expect("move opening");
        assert_eq!(r["offset"], json!(6.5), "moved offset should be echoed");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(6.5), "stored offset should be 6.5");
    }

    #[test]
    fn opening_move_rejects_an_offset_past_the_wall_end() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 9.9}));
        let err = r.expect_err("offset 9.9 on a 10 m wall must fail (rough span exceeds wall)");
        assert!(!err.to_string().is_empty(), "expected a placement error, got empty");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(3.0), "stored offset must be unchanged");
    }

    #[test]
    fn opening_move_rejects_an_unknown_id() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o99"}));
        let err = r.expect_err("moving a non-existent opening should be rejected");
        assert!(err.to_string().contains("no opening"), "expected 'no opening' error, got: {err}");
    }

    #[test]
    fn opening_move_accepts_a_point_instead_of_an_offset() {
        let mut s = session_with_door();
        // Wall w0 runs (0,0) -> (10,0); (6,0) projects to offset 6.0.
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "point": [6.0, 0.0]})).expect("move opening via point");
        assert_eq!(r["offset"], json!(6.0), "projected offset should be 6.0");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(6.0), "stored offset should be 6.0");
    }

    #[test]
    fn opening_move_is_undoable() {
        let mut s = session_with_door();
        let before = s.doc().expect("doc").arch.opening(ArchId(0)).map(|o| o.offset);
        s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 5.0})).expect("move opening");
        let mid = s.doc().expect("doc").arch.opening(ArchId(0)).map(|o| o.offset);
        assert_eq!(mid, Some(5.0), "offset should be 5.0 after move");
        s.undo().expect("undo after move");
        let after = s.doc().expect("doc").arch.opening(ArchId(0)).map(|o| o.offset);
        assert_eq!(after, before, "offset should be restored after undo");
    }

    #[test]
    fn window_command_defaults_the_sill() {
        let mut s = session_with_wall();
        let r = s.execute("window", &json!({"wall": "w0", "offset": 2.0})).expect("window with default sill");
        let d = s.doc().expect("doc");
        let opening = &d.arch.openings[0];
        assert!((opening.sill - 0.9).abs() < 1e-9, "sill should default to 0.9, got {}", opening.sill);
    }

    #[test]
    fn opening_move_rejects_a_position_past_the_wall() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "point": [15.0, 0.0]}));
        let err = r.expect_err("point past the wall end must be rejected");
        assert!(!err.to_string().is_empty(), "expected a placement error, got empty");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(3.0), "stored offset must be unchanged");
    }

    /// A wall, then a door and a window: the save-time proxy layers are created on
    /// first use — `A-DOOR` for the door, `A-GLAZ` for the window — each exactly once.
    #[test]
    fn door_and_window_create_their_layers() {
        let mut s = session_with_wall();
        s.execute("door", &json!({ "wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1 })).unwrap();
        s.execute("window", &json!({ "wall": "w0", "offset": 6.0, "width": 1.2, "height": 1.2 })).unwrap();
        let d = s.doc().expect("doc");
        assert!(d.layers.iter().any(|l| l.name == "A-DOOR"), "A-DOOR not created by the door command");
        assert!(d.layers.iter().any(|l| l.name == "A-GLAZ"), "A-GLAZ not created by the window command");
        assert_eq!(d.layers.iter().filter(|l| l.name == "A-DOOR").count(), 1, "A-DOOR created more than once");
        assert_eq!(d.layers.iter().filter(|l| l.name == "A-GLAZ").count(), 1, "A-GLAZ created more than once");
    }

    /// A wall, door, window and roof together create four distinct arch layers,
    /// each exactly once, with no duplication across commands.
    #[test]
    fn multiple_arch_commands_create_distinct_layers() {
        let mut s = Session::new();
        s.execute("wall", &json!({ "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0], [0.0, 0.0]], "exterior": true })).unwrap();
        s.execute("door", &json!({ "wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1 })).unwrap();
        s.execute("window", &json!({ "wall": "w1", "offset": 3.0, "width": 1.2, "height": 1.2 })).unwrap();
        let d = s.doc_mut().expect("doc");
        d.arch.levels.push(cadcraft_doc::Level { index: 1, name: "Level 1".into(), ..cadcraft_doc::Level::default() });
        d.touch_arch();
        s.execute("roofauto", &json!({ "pitchRise": 6.0, "pitchRun": 12.0 })).unwrap();

        let d = s.doc().expect("doc");
        for name in ["A-WALL", "A-DOOR", "A-GLAZ", "A-ROOF"] {
            assert!(d.layers.iter().any(|l| l.name == name), "{name} not created");
            assert_eq!(d.layers.iter().filter(|l| l.name == name).count(), 1, "{name} created more than once");
        }
    }

    /// A non-numeric offset is rejected as a bad-params error (not silently accepted).
    #[test]
    fn door_rejects_nan_offset() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": "not-a-number", "width": 0.9, "height": 2.1}));
        let err = r.expect_err("non-numeric offset should be rejected");
        assert!(err.to_string().contains("offset"), "expected offset error, got: {err}");
    }

    /// A zero width is rejected (width must be > 0).
    #[test]
    fn door_rejects_zero_width() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.0, "height": 2.1}));
        let err = r.expect_err("zero width should be rejected");
        assert!(!err.to_string().is_empty(), "expected an error, got empty");
    }

    /// Moving an opening to its current offset succeeds and is a no-op.
    #[test]
    fn opening_move_to_same_position_succeeds() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 3.0})).expect("move to same position");
        assert_eq!(r["offset"], json!(3.0), "offset should be echoed");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(3.0), "stored offset should be unchanged");
    }

    /// Moving one opening does not affect other openings on the same wall.
    #[test]
    fn opening_move_with_two_openings_only_moves_one() {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 2.0, "width": 0.9, "height": 2.1})).expect("door 1");
        s.execute("door", &json!({"wall": "w0", "offset": 6.0, "width": 0.9, "height": 2.1})).expect("door 2");
        s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 4.0})).expect("move o0");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(4.0), "o0 should be at 4.0");
        assert_eq!(d.arch.opening(ArchId(1)).map(|o| o.offset), Some(6.0), "o1 should be unchanged at 6.0");
    }

    /// With fit=true, an oversized opening is resized to fit within the wall.
    #[test]
    fn door_with_fit_true_resizes_to_fit() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 10.0, "height": 2.1, "fit": true})).expect("door with fit");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert!(one["width"].as_f64().unwrap() < 10.0, "width should be resized to fit");
        assert_eq!(one["resized"], json!(true), "resized should be true");
    }

    /// Style parsing is case-insensitive: "HINGED", "Hinged", "hinged" all store "hinged".
    #[test]
    fn door_style_is_case_insensitive() {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 1.0, "width": 0.9, "height": 2.1, "style": "HINGED"})).expect("HINGED");
        s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1, "style": "Hinged"})).expect("Hinged");
        s.execute("door", &json!({"wall": "w0", "offset": 5.0, "width": 0.9, "height": 2.1, "style": "hinged"})).expect("hinged");
        let d = s.doc().expect("doc");
        for (i, o) in d.arch.openings.iter().enumerate() {
            assert_eq!(o.style, "hinged", "opening {i} style should be lowercase hinged");
        }
    }

    /// An explicit window style overrides the kind default.
    #[test]
    fn window_with_explicit_style_stores_it() {
        let mut s = session_with_wall();
        s.execute("window", &json!({"wall": "w0", "offset": 2.0, "width": 1.2, "height": 1.2, "style": "casement"})).expect("window casement");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.openings[0].style, "casement", "explicit window style should be stored");
    }

    /// An explicit garage door style overrides the kind default.
    #[test]
    fn garagedoor_with_explicit_style_stores_it() {
        let mut s = session_with_wall();
        s.execute("garagedoor", &json!({"wall": "w0", "offset": 2.0, "width": 1.8, "height": 2.1, "style": "sectional"})).expect("garage sectional");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.openings[0].style, "sectional", "explicit garage door style should be stored");
    }

    /// Hinge and swing parsing is case-insensitive.
    #[test]
    fn door_hinge_and_swing_are_case_insensitive() {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 1.0, "width": 0.9, "height": 2.1, "hinge": "LEFT", "swing": "OUT"}))
            .expect("door LEFT OUT");
        let d = s.doc().expect("doc");
        let o = &d.arch.openings[0];
        assert_eq!(o.hinge, Hinge::Left, "LEFT should parse as Hinge::Left");
        assert_eq!(o.swing, Swing::Out, "OUT should parse as Swing::Out");
    }

    /// A sill of exactly 0.0 is accepted (the validation is >= 0.0).
    #[test]
    fn zero_sill_is_accepted() {
        let mut s = session_with_wall();
        s.execute("window", &json!({"wall": "w0", "offset": 2.0, "width": 1.2, "height": 1.2, "sill": 0.0})).expect("window sill 0");
        let d = s.doc().expect("doc");
        assert!((d.arch.openings[0].sill - 0.0).abs() < 1e-9, "sill should be 0.0");
    }

    /// Multiple openings on the same wall each keep their own style defaults.
    #[test]
    fn multiple_openings_keep_their_own_defaults() {
        let mut s = session_with_wall();
        s.execute("door", &json!({"wall": "w0", "offset": 1.0, "width": 0.9, "height": 2.1})).expect("door 1");
        s.execute("window", &json!({"wall": "w0", "offset": 3.0, "width": 1.2, "height": 1.2})).expect("window 1");
        s.execute("door", &json!({"wall": "w0", "offset": 5.0, "width": 0.9, "height": 2.1})).expect("door 2");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.openings.len(), 3, "should have 3 openings");
        assert_eq!(d.arch.openings[0].style, "hinged", "first door should be hinged");
        assert_eq!(d.arch.openings[1].style, "slider", "window should be slider");
        assert_eq!(d.arch.openings[2].style, "hinged", "second door should be hinged");
    }

    /// Moving an opening requires either an offset or a point.
    #[test]
    fn opening_move_rejects_missing_offset_and_point() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0"}));
        let err = r.expect_err("missing both offset and point should be rejected");
        assert!(err.to_string().contains("`offset` (number) or `point`"), "expected missing-offset error, got: {err}");
    }

    /// When both offset and point are provided, offset takes precedence.
    #[test]
    fn opening_move_offset_takes_precedence_over_point() {
        let mut s = session_with_door();
        // offset 6.5, point projects to 7.0 - offset should win
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 6.5, "point": [7.0, 0.0]})).expect("move with both");
        assert_eq!(r["offset"], json!(6.5), "offset should take precedence");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(6.5), "stored offset should be 6.5");
    }

    /// Moving an opening bumps the arch revision.
    #[test]
    fn opening_move_bumps_revision() {
        let mut s = session_with_door();
        let rev_before = s.doc().expect("doc").arch_revision;
        s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 5.0})).expect("move opening");
        let rev_after = s.doc().expect("doc").arch_revision;
        assert_ne!(rev_before, rev_after, "revision should change after move");
    }

    /// A null offset is rejected (not silently treated as missing).
    #[test]
    fn opening_move_rejects_null_offset() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "offset": null}));
        let err = r.expect_err("null offset should be rejected");
        assert!(!err.to_string().is_empty(), "expected an error, got empty");
    }

    /// The opening id can be a bare number (no prefix).
    #[test]
    fn opening_move_accepts_bare_number_id() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "0", "offset": 5.0})).expect("move with bare number");
        assert!(r["moved"].is_string(), "moved should be a string");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(5.0), "stored offset should be 5.0");
    }

    /// The opening id can use the wall prefix format (w0).
    #[test]
    fn opening_move_accepts_wall_id_format() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "w0", "offset": 5.0})).expect("move with w-prefixed id");
        assert!(r["moved"].is_string(), "moved should be a string");
        let d = s.doc().expect("doc");
        assert_eq!(d.arch.opening(ArchId(0)).map(|o| o.offset), Some(5.0), "stored offset should be 5.0");
    }

    /// The response includes the current arch revision matching the doc.
    #[test]
    fn opening_move_returns_revision() {
        let mut s = session_with_door();
        let r = s.execute("arch.opening.move", &json!({"opening": "o0", "offset": 5.0})).expect("move opening");
        let rev = r["revision"].as_u64().expect("revision should be a number");
        let d = s.doc().expect("doc");
        assert_eq!(rev, d.arch_revision, "revision in response should match doc revision");
    }

    /// A negative offset is rejected (the opening would be before the wall start).
    #[test]
    fn door_rejects_negative_offset() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": -1.0, "width": 0.9, "height": 2.1}));
        let err = r.expect_err("negative offset should be rejected");
        assert!(!err.to_string().is_empty(), "expected an error, got empty");
    }

    /// An offset past the wall end is rejected (the opening would be after the wall end).
    #[test]
    fn door_rejects_offset_past_wall_end() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 11.0, "width": 0.9, "height": 2.1}));
        let err = r.expect_err("offset past wall end should be rejected");
        assert!(!err.to_string().is_empty(), "expected an error, got empty");
    }

    /// When both offset and point are provided, offset takes precedence.
    #[test]
    fn door_uses_offset_when_both_offset_and_point_given() {
        let mut s = session_with_wall();
        // offset 3.0, point projects to 5.0 - offset should win
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "point": [5.0, 0.0], "width": 0.9, "height": 2.1})).expect("door with both");
        let one = &r["openings"].as_array().expect("openings")[0];
        assert_eq!(one["offset"], json!(3.0), "offset should take precedence");
    }

    /// A point past the wall end is rejected (the projected offset is too far along the wall).
    #[test]
    fn door_rejects_point_past_wall_end() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "point": [15.0, 0.0], "width": 0.9, "height": 2.1}));
        let err = r.expect_err("point past wall end should be rejected");
        assert!(!err.to_string().is_empty(), "expected an error, got empty");
    }

    /// An unknown level is rejected.
    #[test]
    fn door_rejects_unknown_level() {
        let mut s = session_with_wall();
        let r = s.execute("door", &json!({"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1, "level": "Level 99"}));
        let err = r.expect_err("unknown level should be rejected");
        assert!(err.to_string().contains("unknown level"), "expected unknown level error, got: {err}");
    }
}
