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
        // Dry run is read-only (never mutates the document) and reports
        // `stats.entities` plus an `openings` array. Doors/windows are counted
        // here by `kind`; the raw entity count comes from `stats`.
        let (doors, windows, entities) = match s.execute("archdetect", &json!({"dryRun": true})) {
            Ok(reply) => {
                let mut doors = 0usize;
                let mut windows = 0usize;
                if let Some(openings) = reply.get("openings").and_then(|v| v.as_array()) {
                    for o in openings {
                        match o.get("kind").and_then(|k| k.as_str()) {
                            Some("door") => doors += 1,
                            Some("window") => windows += 1,
                            _ => {}
                        }
                    }
                }
                let entities = reply.get("stats").and_then(|v| v.get("entities")).and_then(|v| v.as_u64()).unwrap_or(0);
                (doors, windows, entities)
            }
            Err(e) => {
                io.errln(format_args!("archdetect: {e}"));
                (0, 0, 0)
            }
        };

        // Accept applies the detection (one undo step) and reports the number
        // of walls and rooms now in the model.
        match s.execute("archdetect", &json!({"accept": "all"})) {
            Ok(reply) => {
                let w = reply.get("walls").and_then(|v| v.as_u64()).unwrap_or(0);
                let r = reply.get("rooms").and_then(|v| v.as_u64()).unwrap_or(0);
                io.errln(format_args!("archdetect: {w} walls, {doors} doors, {windows} windows, {r} rooms ({entities} entities)"));
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
