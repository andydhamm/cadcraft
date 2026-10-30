//! `archdetect`: find walls, openings and rooms in plain linework, as a dry-run report or an accept.
use std::collections::BTreeSet;
use std::time::Instant;

use cadcraft_arch::detect::{CandKind, DetectOptions, Detection, Evidence, detect, units_named, units_scale};
use cadcraft_doc::arch::{MAX_OPENINGS, MAX_ROOMS, MAX_WALLS, Opening, OpeningKind, Room, Wall, opening_label, room_label, wall_label};
use cadcraft_geom::{Vec2, point_in_polygon};
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, Session, bad};
use crate::{EngineError, Result};

/// Command specs for `archdetect`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("archdetect", "Detect from Linework", run_archdetect)
            .menu(&["Architecture", "Detect from Linework"])
            .params("{dryRun?: bool, accept?: \"all\" | [ids], minConfidence?: 0..1, units?: mm|cm|dm|m|in|ft, layers?: [names]}"),
    ]
}

/// Which candidates an accept takes.
enum Accept {
    /// Every candidate at or above `minConfidence`.
    All,
    /// Exactly these candidate ids.
    Ids(Vec<String>),
}

fn kind_name(k: CandKind) -> &'static str {
    match k {
        CandKind::Door => "door",
        CandKind::Window => "window",
        CandKind::Cased => "cased",
    }
}

fn evidence_json(e: &Evidence) -> Value {
    json!({
        "layers": e.layers,
        "handles": e.handles.iter().map(|h| format!("{:x}", h.0)).collect::<Vec<_>>(),
        "rules": e.rules,
    })
}

fn pt(v: Vec2) -> Value {
    json!([v.x, v.y])
}

/// Candidate ids: walls first, then openings, then rooms.
fn wall_id(i: usize) -> String {
    format!("c{i}")
}

fn opening_id(det: &Detection, i: usize) -> String {
    format!("c{}", det.walls.len() + i)
}

fn room_id(det: &Detection, i: usize) -> String {
    format!("c{}", det.walls.len() + det.openings.len() + i)
}

fn dry_run_reply(det: &Detection, units: &str, scale: f64, ms: f64) -> Value {
    let walls: Vec<Value> = det
        .walls
        .iter()
        .enumerate()
        .map(|(i, w)| {
            json!({
                "id": wall_id(i), "start": pt(w.start), "end": pt(w.end), "thickness": w.thickness,
                "confidence": w.confidence, "evidence": evidence_json(&w.evidence),
            })
        })
        .collect();
    let openings: Vec<Value> = det
        .openings
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let mut v = json!({
                "id": opening_id(det, i), "kind": kind_name(o.kind), "host": wall_id(o.host), "center": pt(o.center),
                "offset": o.offset, "width": o.width, "style": o.style,
                "confidence": o.confidence, "evidence": evidence_json(&o.evidence),
            });
            if let (Some(m), Some(h)) = (v.as_object_mut(), o.hinge.and_then(|h| serde_json::to_value(h).ok())) {
                m.insert("hinge".into(), h);
            }
            if let (Some(m), Some(sw)) = (v.as_object_mut(), o.swing.and_then(|x| serde_json::to_value(x).ok())) {
                m.insert("swing".into(), sw);
            }
            v
        })
        .collect();
    let rooms: Vec<Value> = det
        .rooms
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "id": room_id(det, i), "polygon": r.polygon.iter().map(|p| pt(*p)).collect::<Vec<_>>(), "name": r.name,
                "area": r.area, "confidence": r.confidence, "evidence": evidence_json(&r.evidence),
            })
        })
        .collect();
    json!({
        "units": units, "scale": scale, "walls": walls, "openings": openings, "rooms": rooms,
        "stats": {"entities": det.entities, "primitives": det.primitives, "ms": ms},
    })
}

fn parse_accept(p: &Value) -> Result<Option<Accept>> {
    match p.get("accept") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s == "all" => Ok(Some(Accept::All)),
        Some(Value::Array(a)) => {
            let mut ids = Vec::with_capacity(a.len().min(4096));
            for v in a {
                ids.push(v.as_str().ok_or_else(|| bad("archdetect", "`accept` ids must be strings"))?.to_string());
            }
            Ok(Some(Accept::Ids(ids)))
        }
        Some(_) => Err(bad("archdetect", "`accept` must be \"all\" or an array of candidate ids")),
    }
}

fn run_archdetect(s: &mut Session, p: &Value) -> Result<Value> {
    // (1) Validate every parameter before anything else.
    if !p.is_object() {
        return Err(bad("archdetect", "params must be an object"));
    }
    let dry = match p.get("dryRun") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(bad("archdetect", "`dryRun` must be a boolean")),
    };
    let accept = parse_accept(p)?;
    if dry && accept.is_some() {
        return Err(bad("archdetect", "`dryRun` and `accept` cannot be combined"));
    }
    let min_conf = match p.get("minConfidence") {
        None | Some(Value::Null) => cadcraft_arch::detect::DEFAULT_MIN_CONFIDENCE,
        Some(v) => v
            .as_f64()
            .filter(|x| x.is_finite() && (0.0..=1.0).contains(x))
            .ok_or_else(|| bad("archdetect", "`minConfidence` must be a number from 0 to 1"))?,
    };
    let units_param = match p.get("units") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let name = v.as_str().ok_or_else(|| bad("archdetect", "`units` must be a string"))?;
            Some(units_named(name).ok_or_else(|| bad("archdetect", format!("unknown units `{name}` (use mm, cm, dm, m, in or ft)")))?)
        }
    };
    let layers = match p.get("layers") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => {
            let mut v = Vec::with_capacity(a.len().min(4096));
            for x in a {
                v.push(x.as_str().ok_or_else(|| bad("archdetect", "`layers` must be an array of layer names"))?.to_string());
            }
            Some(v)
        }
        Some(_) => return Err(bad("archdetect", "`layers` must be an array of layer names")),
    };

    // (2) Scale.
    let (units, scale) = match units_param {
        Some(u) => u,
        None => {
            let code = s.doc()?.header.i64("INSUNITS", 0);
            units_scale(code).ok_or_else(|| bad("archdetect", format!("unknown $INSUNITS code {code}: pass `units` (mm, cm, dm, m, in or ft)")))?
        }
    };

    // (3) Detect on the unmutated drawing.
    let t0 = Instant::now();
    let det = detect(s.doc()?, &DetectOptions { scale, layers }).map_err(|e| bad("archdetect", e.0))?;
    let ms = t0.elapsed().as_secs_f64() * 1000.0;

    // (4) Dry run.
    let Some(accept) = accept else {
        return Ok(dry_run_reply(&det, units, scale, ms));
    };

    // (5) Accept: resolve the id set and refuse before any mutation.
    let total = det.walls.len() + det.openings.len() + det.rooms.len();
    let confidence = |n: usize| -> f64 {
        if let Some(w) = det.walls.get(n) {
            w.confidence
        } else if let Some(o) = det.openings.get(n - det.walls.len()) {
            o.confidence
        } else {
            det.rooms.get(n - det.walls.len() - det.openings.len()).map_or(0.0, |r| r.confidence)
        }
    };
    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    match &accept {
        Accept::All => chosen.extend((0..total).filter(|&n| confidence(n) >= min_conf)),
        Accept::Ids(ids) => {
            for id in ids {
                let n = id
                    .strip_prefix('c')
                    .and_then(|r| r.parse::<usize>().ok())
                    .filter(|&n| n < total && format!("c{n}") == *id)
                    .ok_or_else(|| bad("archdetect", format!("unknown candidate id `{id}`")))?;
                chosen.insert(n);
            }
        }
    }
    for (i, o) in det.openings.iter().enumerate() {
        if chosen.contains(&(det.walls.len() + i)) && !chosen.contains(&o.host) {
            return Err(bad("archdetect", format!("opening {} needs its host wall {}, which is not accepted", opening_id(&det, i), wall_id(o.host))));
        }
    }
    let n_walls = chosen.iter().filter(|&&n| n < det.walls.len()).count();
    let n_open = chosen.iter().filter(|&&n| n >= det.walls.len() && n < det.walls.len() + det.openings.len()).count();
    let n_rooms = chosen.len() - n_walls - n_open;
    {
        let a = &s.doc()?.arch;
        if a.walls.len() + n_walls > MAX_WALLS || a.openings.len() + n_open > MAX_OPENINGS || a.rooms.len() + n_rooms > MAX_ROOMS {
            return Err(bad("archdetect", "accepting these candidates would exceed the model limits"));
        }
    }

    // Exterior = at most one side of the wall (probed 0.6 x thickness off its midpoint) lies in a room.
    let is_exterior = |w: &cadcraft_arch::detect::WallRun| -> bool {
        let d = w.end - w.start;
        let len = d.len();
        if !(len.is_finite() && len > 0.0) {
            return true;
        }
        let mid = (w.start + w.end) * 0.5;
        let nrm = Vec2::new(-d.y / len, d.x / len);
        let off = nrm * (0.6 * w.thickness);
        let sides = [mid + off, mid - off];
        sides.iter().filter(|q| det.rooms.iter().any(|r| point_in_polygon(&r.polygon, **q))).count() <= 1
    };

    // (6) Apply in ONE document step.
    let d = s.doc_mut()?;
    let converted = if scale != 1.0 {
        super::units::convert_drawing_to_metres(d, scale).map_err(|e| EngineError::BadParams { cmd: "archdetect".into(), msg: e })?;
        json!({"from": units, "scale": scale})
    } else {
        Value::Null
    };

    let height =
        d.arch.levels.first().map(|l| l.ceiling_height + l.structure).filter(|h| h.is_finite() && *h > 0.0).unwrap_or(super::DEFAULT_WALL_HEIGHT);
    let mut accepted = serde_json::Map::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut wall_ids = std::collections::HashMap::new();
    let (mut nw, mut no, mut nr) = (0usize, 0usize, 0usize);
    let (mut used_wall, mut used_door, mut used_glaz, mut used_room) = (false, false, false, false);

    for (i, w) in det.walls.iter().enumerate() {
        if !chosen.contains(&i) {
            continue;
        }
        let exterior = is_exterior(w);
        let raw = super::wall_layers(exterior);
        let sum: f64 = raw.iter().map(|(_, t)| t).sum();
        let layers = if sum > 0.0 && sum.is_finite() && w.thickness.is_finite() {
            raw.into_iter().map(|(n, t)| (n, t * w.thickness / sum)).collect()
        } else {
            raw
        };
        let id = d.arch.push_wall(Wall {
            exterior,
            wall_type: if exterior { "exterior" } else { "interior" }.into(),
            layers,
            height,
            ..Wall::new(w.start, w.end, w.thickness)
        });
        wall_ids.insert(i, id);
        accepted.insert(wall_id(i), Value::String(wall_label(id)));
        nw += 1;
        used_wall = true;
    }

    for (i, o) in det.openings.iter().enumerate() {
        let cid = opening_id(&det, i);
        if !chosen.contains(&(det.walls.len() + i)) {
            continue;
        }
        let Some(&host) = wall_ids.get(&o.host) else { continue };
        let (kind, oh, sill) = match o.kind {
            CandKind::Door => (OpeningKind::Door, 2.1, 0.0),
            CandKind::Window => (OpeningKind::Window, 1.2, 0.9),
            CandKind::Cased => (OpeningKind::CasedOpening, 2.1, 0.0),
        };
        let mut op = Opening::new(host, o.offset, o.width, oh, kind);
        op.sill = sill;
        if !o.style.is_empty() {
            op.style = o.style.clone();
        }
        if let Some(h) = o.hinge {
            op.hinge = h;
        }
        if let Some(sw) = o.swing {
            op.swing = sw;
        }
        match cadcraft_arch::opening::validate_placement(&d.arch, host, o.offset, o.width, sill, oh, kind, &op.style, None) {
            Ok(offset) => {
                op.offset = offset;
                let id = d.arch.push_opening(op);
                accepted.insert(cid, Value::String(opening_label(id)));
                no += 1;
                if kind == OpeningKind::Window {
                    used_glaz = true;
                } else {
                    used_door = true;
                }
            }
            Err(e) => skipped.push(json!({"id": cid, "reason": e.to_string()})),
        }
    }

    for (i, r) in det.rooms.iter().enumerate() {
        if !chosen.contains(&(det.walls.len() + det.openings.len() + i)) {
            continue;
        }
        let id = d.arch.push_room(Room {
            name: r.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "Room".into()),
            polygon: r.polygon.clone(),
            level: 0,
            ..Room::default()
        });
        accepted.insert(room_id(&det, i), Value::String(room_label(id)));
        nr += 1;
        used_room = true;
    }

    for (used, name) in [(used_wall, "A-WALL"), (used_door, "A-DOOR"), (used_glaz, "A-GLAZ"), (used_room, "A-ROOM")] {
        if used {
            d.ensure_layer(name);
        }
    }
    cadcraft_arch::area::refresh_areas(&mut d.arch);
    d.touch_arch();

    let mut reply = json!({"accepted": accepted, "walls": nw, "openings": no, "rooms": nr, "converted": converted});
    if let (Some(m), false) = (reply.as_object_mut(), skipped.is_empty()) {
        m.insert("skipped".into(), Value::Array(skipped));
    }
    Ok(reply)
}
