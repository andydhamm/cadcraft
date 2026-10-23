//! The `arch.wall.stretch` command: stretch one end of a wall and transitively
//! move every neighbour end that shared that point (one undo step).

use cadcraft_arch::join;
use cadcraft_doc::{ARCH_HANDLE_BASE, ArchId, Handle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, point_param, str_param};
use crate::{Result, Session};

/// Command specs for the arch wall stretch module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("arch.wall.stretch", "Arch Wall Stretch", run_stretch)
            .menu(&["Architecture", "Wall"])
            .params("{wall: \"w{n}\", end: \"start\"|\"end\", to: [x, y]}"),
    ]
}

/// Run the `arch.wall.stretch` command from JSON parameters.
fn run_stretch(s: &mut Session, p: &Value) -> Result<Value> {
    let wall_str = str_param(p, "wall").ok_or_else(|| bad("arch.wall.stretch", "`wall` (e.g. \"w1\") is required"))?;
    let end_str = str_param(p, "end").ok_or_else(|| bad("arch.wall.stretch", "`end` (\"start\" or \"end\") is required"))?;
    let to = point_param(p, "to").ok_or_else(|| bad("arch.wall.stretch", "`to` ([x, y]) is required"))?;

    if !to.is_finite() {
        return Err(bad("arch.wall.stretch", "`to` must be a finite point"));
    }

    let arch_id = parse_arch_id(wall_str).ok_or_else(|| bad("arch.wall.stretch", format!("invalid wall id `{wall_str}`")))?;

    let end = match end_str.as_str() {
        "start" => join::End::Start,
        "end" => join::End::End,
        _ => return Err(bad("arch.wall.stretch", "`end` must be \"start\" or \"end\"")),
    };

    let d = s.doc_mut()?;
    let changed = join::stretch_wall(&mut d.arch, arch_id, end, to).map_err(|e| bad("arch.wall.stretch", e.to_string()))?;

    let walls_json: Vec<Value> = changed
        .iter()
        .map(|id| {
            json!({
                "id": format!("w{}", id.0),
                "handle": Handle(ARCH_HANDLE_BASE + id.0 as u64).hex(),
            })
        })
        .collect();

    Ok(json!({
        "walls": walls_json,
        "count": changed.len(),
    }))
}

/// Parse an arch id string like `w123` or `123` into an [`ArchId`].
fn parse_arch_id(s: &str) -> Option<ArchId> {
    let s = s.trim();
    let digits = if s.starts_with('w') || s.starts_with('W') { &s[1..] } else { s };
    u32::from_str_radix(digits, 10).ok().map(ArchId)
}
