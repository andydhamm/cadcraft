use super::args::ConvertArgs;
use super::error::Result;
use super::output::{Streams, export_framed, export_window, skipped_message, undrawn_message};
use super::{open, unicode};
use cadcraft_engine::Session;
use serde_json::json;

pub(crate) fn convert(a: &ConvertArgs, io: &mut Streams) -> Result {
    let i = unicode(&a.input)?;
    let framing = a.framing.framing();
    let mut s = Session::empty();
    open(&mut s, &a.input)?;
    if let Some(m) = skipped_message(s.doc()?, i) {
        io.errln(format_args!("{m}"));
    }

    // Detect arch objects if converting to GLB, no flag set, and no arch objects exist.
    if !a.no_detect && a.output.extension().and_then(|e| e.to_str()) == Some("glb") && s.doc()?.arch.is_empty() {
        match s.execute("archdetect", &json!({"accept": "all"})) {
            Ok(reply) => {
                let w = reply.get("walls").and_then(|v| v.as_u64()).unwrap_or(0);
                let d = reply.get("doors").and_then(|v| v.as_u64()).unwrap_or(0);
                let n = reply.get("windows").and_then(|v| v.as_u64()).unwrap_or(0);
                let r = reply.get("rooms").and_then(|v| v.as_u64()).unwrap_or(0);
                let e = reply.get("entities").and_then(|v| v.as_u64()).unwrap_or(0);
                io.errln(format_args!("archdetect: {w} walls, {d} doors, {n} windows, {r} rooms ({e} entities)"));
            }
            Err(e) => {
                io.errln(format_args!("archdetect: {e}"));
            }
        }
    }

    if cadcraft_io::is_image(&a.output)
        && let Some(m) = undrawn_message(s.doc()?, i)
    {
        io.errln(format_args!("{m}"));
    }
    let window = export_window(&s, framing, None)?;
    export_framed(&s, &a.output, window, io)
}
