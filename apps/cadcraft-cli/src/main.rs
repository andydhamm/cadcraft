//! `cadcraft-cli`: headless CADCraft.
//!
//! ```text
//! cadcraft-cli info FILE.dxf                       summary as JSON
//! cadcraft-cli convert IN.dxf OUT.(dxf|svg|png) [--window x0,y0,x1,y1 | --view extents|current]
//! cadcraft-cli run [FILE|--sample|--metric] [--script TEXT|--script-file F.scr] [--cmd 'id {json}']... [--save OUT] [--export OUT.png] [--window ... | --view ...]
//! cadcraft-cli commands [FILTER]                   command catalog (JSON)
//! cadcraft-cli mcp [--connect HOST:PORT [--lazy] [--launch]]  MCP server on stdio (headless or bridged to the app)
//! cadcraft-cli perf [N]                            timing table on a synthetic N-entity drawing
//! cadcraft-cli perf FILE                           canvas display-list build of a drawing, zoomed in step by step
//! cadcraft-cli sample (bracket|floorplan) OUT       write a built-in sample drawing
//! ```
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// `println!` that doesn't panic when stdout is closed (`cadcraft-cli commands | head`) with
/// "failed printing to stdout: Broken pipe (os error 32)". Once stdout has failed, later output is
/// dropped and the work goes on: see `stdout_failed`.
macro_rules! outln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        if !$crate::stdout_gone() {
            if let Err(e) = writeln!(std::io::stdout(), $($arg)*) {
                $crate::stdout_failed(e);
            }
        }
    }};
}

use std::process::ExitCode;

use cadcraft_engine::doc::{Handle, Space};
use cadcraft_engine::geom::{Bounds2, Vec2};
use cadcraft_engine::{Session, View};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once a write to stdout has failed; later output is dropped.
static STDOUT_GONE: AtomicBool = AtomicBool::new(false);
/// Set when stdout failed for a reason other than a closed reader: the exit status becomes 1.
static STDOUT_ERROR: AtomicBool = AtomicBool::new(false);

fn stdout_gone() -> bool {
    STDOUT_GONE.load(Ordering::Relaxed)
}

/// stdout went away. The program doesn't stop there: the rest of `run` (later `--cmd` results,
/// `--save`, `--export`) still happens, so `cadcraft-cli run … --save out.dxf | head` keeps the
/// drawing. A reader that stopped early (a closed pipe) is not an error, as in ripgrep; any other
/// write error is reported and makes the exit status 1.
fn stdout_failed(e: std::io::Error) {
    if STDOUT_GONE.swap(true, Ordering::Relaxed) || e.kind() == std::io::ErrorKind::BrokenPipe {
        return;
    }
    eprintln!("cadcraft-cli: can't write to stdout: {e}");
    STDOUT_ERROR.store(true, Ordering::Relaxed);
}

const USAGE: &str = "usage:
  cadcraft-cli info FILE.dxf
  cadcraft-cli convert IN.dxf OUT.(dxf|dwg|svg|png|pdf|glb) [FRAMING]
  cadcraft-cli run [FILE | --sample | --metric] [--script TEXT] [--script-file F.scr] [--cmd 'id {json}']... [--save OUT.dxf] [--export OUT.(png|svg|pdf|glb)] [FRAMING]
  cadcraft-cli render FILE OUT.png [--camera SPEC] [--size WxH]
  cadcraft-cli commands [FILTER]
  cadcraft-cli mcp [--connect HOST:PORT [--lazy] [--launch]]
  cadcraft-cli perf [N | FILE]
  cadcraft-cli sample (bracket|floorplan) OUT.(dxf|dwg|svg|png)
  cadcraft-cli --version

image framing (png 2400x1600, svg, pdf; always model space):
  default                  convert: fit the drawing extents.
                           run --export: the current view if the script/commands changed it
                           (zoom, pan, view.set...), otherwise fit the drawing extents.
  --window x0,y0,x1,y1     show this rectangle (drawing units, x0<x1, y0<y1), centred and
                           scaled to fit the image without distortion.
  --view extents           always fit the drawing extents.
  --view current           the current view (as last set by zoom/pan/view.set; a freshly
                           opened drawing's view is its extents plus a 10% margin).

--cmd chaining (run):
  A --cmd parameter value of exactly \"{last}\" is replaced with the last object handle an
  earlier --cmd in the same run returned, so a command can act on what the previous one made:
    --cmd 'mview {\"p1\":[20,20],\"p2\":[200,150]}'        prints {\"viewports\":[\"100\"]}
    --cmd 'viewport.set {\"handle\":\"{last}\",\"scale\":1}' changes viewport 100
  It works in any parameter, at any depth (e.g. \"handles\":[\"{last}\"]); every other string is
  passed unchanged. With no earlier returned handle the run fails and says so.
  A failing --cmd prints `error: ID: message` on stderr and the run continues; the exit status
  is that of the last step. --script still stops the run.";

/// What part of model space an image export shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Framing {
    /// `run`: the current view if the commands changed it, else the extents. `convert`: extents.
    Auto,
    Extents,
    Current,
    Window(Bounds2),
}

/// Parse `--window x0,y0,x1,y1` (min corner first). Hostile values are errors, never panics.
fn parse_window(text: &str) -> Result<Bounds2, String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let [x0, y0, x1, y1] = parts.as_slice() else {
        return Err(format!("--window needs four numbers x0,y0,x1,y1, got `{text}`"));
    };
    let num = |name: &str, v: &str| -> Result<f64, String> {
        match v.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(n),
            _ => Err(format!("--window: {name} must be a finite number, got `{v}`")),
        }
    };
    let (x0, y0, x1, y1) = (num("x0", x0)?, num("y0", y0)?, num("x1", x1)?, num("y1", y1)?);
    if !(x0 < x1 && y0 < y1) {
        return Err(format!("--window: need x0 < x1 and y0 < y1 (lower-left corner first), got {x0},{y0},{x1},{y1}"));
    }
    let w = Bounds2 { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) };
    cadcraft_io::check_window(&w).map_err(|e| format!("--window: {e}"))?;
    Ok(w)
}

/// Parse a framing option (`--window`, `--view`) at `a`, taking its value from `it`. `Ok(false)`
/// means `a` is not a framing option.
fn framing_option<'a>(a: &str, it: &mut impl Iterator<Item = &'a String>, framing: &mut Framing) -> Result<bool, String> {
    let f = match a {
        "--window" => Framing::Window(parse_window(it.next().ok_or("--window needs x0,y0,x1,y1")?)?),
        "--view" => match it.next().map(|v| v.to_ascii_lowercase()).as_deref() {
            Some("extents") => Framing::Extents,
            Some("current") => Framing::Current,
            _ => return Err("--view needs `extents` or `current`".into()),
        },
        _ => return Ok(false),
    };
    if *framing != Framing::Auto && std::mem::discriminant(framing) != std::mem::discriminant(&f) {
        return Err("use only one of --window and --view".into());
    }
    *framing = f;
    Ok(true)
}

/// The model-space view of every open drawing, by drawing id (to tell whether commands changed it).
fn model_views(s: &Session) -> Vec<(u64, Option<View>)> {
    s.docs.iter().map(|st| (st.uid, st.views.iter().find(|(sp, _)| *sp == Space::Model).map(|(_, v)| *v))).collect()
}

/// The active drawing's current model-space view as a rectangle with the viewport's aspect ratio
/// (the CLI viewport is 3:2 like the PNG export, so the image shows exactly this view).
fn current_window(s: &Session) -> Option<Bounds2> {
    let st = s.state().ok()?;
    let v = st.views.iter().find(|(sp, _)| *sp == Space::Model).map(|(_, v)| *v)?;
    let (w, h) = s.viewport_px;
    let half = Vec2::new(v.height * w / h.max(1.0), v.height) * 0.5;
    Some(Bounds2::new(v.center - half, v.center + half))
}

/// The window an image export shows, or `None` to fit the extents. `before` is [`model_views`]
/// taken before the commands ran (`None` for `convert`, where nothing ran).
fn export_window(s: &Session, framing: Framing, before: Option<&[(u64, Option<View>)]>) -> Result<Option<Bounds2>, String> {
    match framing {
        Framing::Window(w) => Ok(Some(w)),
        Framing::Extents => Ok(None),
        Framing::Current => current_window(s).map(Some).ok_or_else(|| "--view current: the drawing has no model-space view".into()),
        Framing::Auto => {
            let Some(before) = before else { return Ok(None) };
            let Ok(st) = s.state() else { return Ok(None) };
            let now = model_views(s);
            let was = before.iter().find(|(uid, _)| *uid == st.uid);
            let is = now.iter().find(|(uid, _)| *uid == st.uid);
            // Only a view the commands changed counts; a drawing opened by the commands keeps
            // the extents framing.
            match (was, is) {
                (Some(a), Some(b)) if a.1 != b.1 => Ok(current_window(s)),
                _ => Ok(None),
            }
        }
    }
}

fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    cadcraft_engine::cmd::file::set_write_as(|d, name, o| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: o.version, binary: o.binary }).map_err(|e| e.to_string())
    });
    // AutoLISP's entget, entmake and entmod see entities as DXF groups.
    cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
        groups: cadcraft_io::entity_groups,
        entity: cadcraft_io::entity_from_groups,
    });
}

fn open(s: &mut Session, path: &str) -> Result<(), String> {
    s.execute("open", &json!({ "path": path })).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Clone, Copy)]
struct OptionSpec {
    name: &'static str,
    takes_value: bool,
}

fn validate_options(command: &str, args: &[String], specs: &[OptionSpec]) -> Result<(), String> {
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        if let Some(name) = arg.strip_prefix("--") {
            let Some(spec) = specs.iter().find(|spec| spec.name == arg) else {
                return Err(format!("{command}: unknown option --{name}"));
            };
            if spec.takes_value && args.get(i + 1).is_some_and(|value| !value.starts_with("--")) {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

const NO_OPTIONS: &[OptionSpec] = &[];

/// Option specs for `render`.
const RENDER_OPTIONS: &[OptionSpec] = &[OptionSpec { name: "--camera", takes_value: true }, OptionSpec { name: "--size", takes_value: true }];

fn info(path: &str) -> Result<(), String> {
    let mut s = Session::empty();
    open(&mut s, path)?;
    let v = s.execute("drawing.inspect", &json!({ "entities": false })).map_err(|e| e.to_string())?;
    outln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

/// Write `bytes` to `out` through the engine's atomic writer (an exclusively created sibling file
/// renamed over `out`), so a failed or interrupted write leaves the previous file untouched.
fn write_atomic(out: &str, bytes: &[u8]) -> std::io::Result<()> {
    cadcraft_engine::atomic_write::write_atomic(std::path::Path::new(out), bytes)
}

fn export(s: &Session, out: &str) -> Result<(), String> {
    export_framed(s, out, None)
}

fn export_framed(s: &Session, out: &str, window: Option<Bounds2>) -> Result<(), String> {
    let d = s.doc().map_err(|e| e.to_string())?;
    let bytes = cadcraft_io::write_framed(d, out, window).map_err(|e| e.to_string())?;
    write_atomic(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out} ({} bytes)", bytes.len());
    if cadcraft_engine::cmd::file::drops_unknown(out) {
        let lost = cadcraft_engine::cmd::file::not_saved(d);
        if let Some(m) = cadcraft_engine::cmd::file::not_saved_message(&lost) {
            eprintln!("{m}");
        }
    }
    Ok(())
}

fn sample(args: &[String]) -> Result<(), String> {
    let (Some(which), Some(out)) = (args.first(), args.get(1)) else { return Err(USAGE.into()) };
    let d = match which.as_str() {
        "bracket" => cadcraft_engine::sample::bracket(),
        "floorplan" | "floor" => cadcraft_engine::sample::floor_plan(),
        other => return Err(format!("unknown sample `{other}` (bracket, floorplan)")),
    };
    let bytes = cadcraft_io::write(&d, out).map_err(|e| e.to_string())?;
    write_atomic(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out} ({} bytes)", bytes.len());
    Ok(())
}

/// Whether `name` is a frameable output (2D image or 3D GLB).
fn is_frameable_output(name: &str) -> bool {
    cadcraft_io::is_image(name) || std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()) == Some("glb".to_string())
}

/// Reject an explicit framing option without a frameable output to apply it to.
fn check_framed_output(framing: Framing, out: Option<&str>) -> Result<(), String> {
    match out {
        _ if framing == Framing::Auto => Ok(()),
        Some(o) if !is_frameable_output(o) => Err(format!("--window/--view only apply to image output (png, svg, pdf) or glb, not `{o}`")),
        Some(_) => Ok(()),
        None => Err("--window/--view need a frameable output (--export OUT.png|svg|pdf|glb)".into()),
    }
}

/// Default pixel size for `render`.
const RENDER_DEFAULT_SIZE: (u32, u32) = (2400, 1600);

/// Parse `WxH` into a pixel size (positive whole numbers).
fn parse_size(v: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = v.split(['x', 'X']).map(str::trim).collect();
    let [w, h] = parts.as_slice() else { return Err(format!("--size needs WxH, got `{v}`")) };
    let num = |name: &str, t: &str| -> Result<u32, String> {
        match t.parse::<u32>() {
            Ok(n) if n > 0 => Ok(n),
            _ => Err(format!("--size {name} must be a positive whole number, got `{t}`")),
        }
    };
    Ok((num("width", w)?, num("height", h)?))
}

/// Parse `'orbit[,azimuth=A][,elevation=E][,distance=D][,fov=F]'` into a [`Camera`], seeded
/// from `base` (the result of [`Orbit::fit`]). Field values must be finite; distance and fov
/// must be positive. A malformed spec is an error, so a bad `--camera` never reaches the GPU.
fn parse_camera(spec: &str, base: cadcraft_render3d::camera::Orbit) -> Result<cadcraft_render3d::camera::Camera, String> {
    let tokens: Vec<&str> = spec.split(',').map(str::trim).collect();
    if tokens.is_empty() || tokens[0] != "orbit" {
        return Err("camera spec must begin with `orbit`".into());
    }
    let mut o = base;
    for t in tokens[1..].iter() {
        if t.is_empty() {
            continue;
        }
        let (name, val) =
            t.split_once('=').map(|(k, v)| (k.trim(), v.trim())).ok_or_else(|| format!("camera field must be `name=value`, got `{t}`"))?;
        let v = val.parse::<f64>().map_err(|e| format!("camera {name}: {e}"))?;
        if !v.is_finite() {
            return Err(format!("camera {name} must be finite, got `{val}`"));
        }
        match name {
            "azimuth" => o.azimuth = v,
            "elevation" => o.elevation = v,
            "distance" => {
                if v <= 0.0 {
                    return Err(format!("camera distance must be positive, got `{val}`"));
                }
                o.distance = v;
            }
            "fov" => {
                if v <= 0.0 {
                    return Err(format!("camera fov must be positive, got `{val}`"));
                }
                o.fov_deg = v;
            }
            other => return Err(format!("unknown camera field `{other}`")),
        }
    }
    Ok(cadcraft_render3d::camera::Camera::Orbit(o))
}

/// Format a projected bounds tuple for stdout display (fixed 6-digit precision).
/// Extracted so the output format can be pinned without needing a GPU.
fn format_projected(pb: [f32; 4]) -> String {
    format!("PROJECTED {:.6},{:.6},{:.6},{:.6}", pb[0], pb[1], pb[2], pb[3])
}

/// Exit code the CLI uses when a 3D render fails for lack of a GPU adapter.
const RENDER_NO_ADAPTER_EXIT: i32 = 3;

/// Exit code for a `render` GPU failure: `RENDER_NO_ADAPTER_EXIT` when there is no GPU
/// adapter (the phase-3 acceptance treats this as SKIP) and `1` for any other render error.
fn render_error_exit(err: &cadcraft_render3d::Render3dError) -> i32 {
    match err {
        cadcraft_render3d::Render3dError::NoAdapter => RENDER_NO_ADAPTER_EXIT,
        _ => 1,
    }
}

/// Render the arch 3D scene in `FILE` to the PNG at `OUT`. `--camera` and `--size` are
/// optional. On success the projected scene bounds are printed to stderr. With no GPU
/// adapter the command prints "NO_ADAPTER" to stderr and exits 3.
fn render(args: &[String]) -> Result<(), String> {
    let mut file: Option<String> = None;
    let mut out: Option<String> = None;
    let mut camera_spec: Option<String> = None;
    let mut size: (u32, u32) = RENDER_DEFAULT_SIZE;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--camera" => {
                i += 1;
                let Some(v) = args.get(i) else { return Err("--camera needs a SPEC".into()) };
                camera_spec = Some(v.clone());
            }
            "--size" => {
                i += 1;
                let Some(v) = args.get(i) else { return Err("--size needs WxH".into()) };
                size = parse_size(v)?;
            }
            _ => {
                if file.is_none() {
                    file = Some(args[i].clone());
                } else if out.is_none() {
                    out = Some(args[i].clone());
                } else {
                    return Err("render: expected `FILE OUT.png`".to_string());
                }
            }
        }
        i += 1;
    }
    let (file, out) = match (file, out) {
        (Some(f), Some(o)) => (f, o),
        _ => return Err("render: expected `FILE OUT.png`".to_string()),
    };

    let mut s = Session::empty();
    open(&mut s, &file)?;
    let d = s.doc().map_err(|e| e.to_string())?;
    let scene = cadcraft_arch3d::build(&d.arch);
    let base = cadcraft_render3d::camera::Orbit::fit(&scene.bounds);
    let camera = match camera_spec {
        Some(spec) => parse_camera(&spec, base),
        None => Ok(cadcraft_render3d::camera::Camera::Orbit(base)),
    }?;

    // The mesh vertices, not the bounding box: room slabs leave its lower corners empty.
    let pb = cadcraft_render3d::camera::projected_scene_bounds(&camera, &scene, size);

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("render: tokio runtime: {e}"))?;
    match rt.block_on(cadcraft_render3d::offscreen::headless_png(&scene, &camera, size)) {
        Ok(png) => {
            std::fs::write(&out, &png).map_err(|e| format!("{out}: {e}"))?;
            eprintln!("wrote {out} ({} bytes)", png.len());
            println!("{}", format_projected(pb));
        }
        Err(e) => {
            let code = render_error_exit(&e);
            if code == RENDER_NO_ADAPTER_EXIT {
                eprintln!("NO_ADAPTER");
                std::process::exit(code);
            }
            return Err(format!("render: {e}"));
        }
    }
    Ok(())
}

fn convert(args: &[String]) -> Result<(), String> {
    let mut framing = Framing::Auto;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if !framing_option(a, &mut it, &mut framing)? {
            files.push(a);
        }
    }
    let [i, o] = files.as_slice() else { return Err(USAGE.into()) };
    check_framed_output(framing, Some(o))?;
    let mut s = Session::empty();
    open(&mut s, i)?;
    if let Some(m) = skipped_message(s.doc().map_err(|e| e.to_string())?, i) {
        eprintln!("{m}");
    }
    if cadcraft_io::is_image(o)
        && let Some(m) = undrawn_message(s.doc().map_err(|e| e.to_string())?, i)
    {
        eprintln!("{m}");
    }
    let window = export_window(&s, framing, None)?;
    export_framed(&s, o, window)
}

/// The warning for the model-space entities of `path` an image can't show, by type (those kept
/// as unknown objects, `Proxy` in `info`), or `None` when there are none.
fn undrawn_message(d: &cadcraft_engine::doc::Drawing, path: &str) -> Option<String> {
    let mut types: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for e in d.model.iter() {
        if let cadcraft_engine::doc::EntityKind::Unknown(u) = &e.kind {
            let n = types.entry(u.dxf_type.as_str()).or_insert(0);
            *n = n.saturating_add(1);
        }
    }
    let n = types.values().fold(0usize, |a, n| a.saturating_add(*n));
    if n == 0 {
        return None;
    }
    let list: Vec<String> = types.iter().map(|(t, n)| format!("{t}: {n}")).collect();
    let entities = if n == 1 { "entity" } else { "entities" };
    Some(format!("{path}: {n} {entities} not drawn ({})", list.join(", ")))
}

/// The warning for the records of `path` the reader could not place, or `None` when there are none.
fn skipped_message(d: &cadcraft_engine::doc::Drawing, path: &str) -> Option<String> {
    let n = d.skipped_records.values().fold(0usize, |a, n| a.saturating_add(*n));
    if n == 0 {
        return None;
    }
    let list: Vec<String> = d.skipped_records.iter().map(|(section, n)| format!("{section}: {n}")).collect();
    let records = if n == 1 { "record" } else { "records" };
    Some(format!("{path}: {n} {records} could not be read and are left out ({})", list.join(", ")))
}

/// A `--cmd` parameter value of exactly `{last}` is filled in with the last object handle an
/// earlier `--cmd` in the same run returned: `mview` answers `{"viewports":["100"]}`, so
/// `viewport.set {"handle":"{last}","scale":1}` changes viewport 100. Any other string is passed
/// through unchanged, and `--script` text is never rewritten.
const LAST_HANDLE: &str = "{last}";

/// How deep [`result_handles`] walks a command result (results are the engine's own JSON).
const RESULT_DEPTH: usize = 16;

/// Result keys whose values are object references: the engine's result convention for handles.
/// A handle can be a string (`handle`, `viewport`, `insert`…) or a list of them (`handles`,
/// `viewports`, `members`…). Every other field may hold a user-defined name, a message or a
/// count, and is never read as a handle — a group named `100` is a name, not object `100`.
fn handle_field(key: &str) -> bool {
    matches!(
        key,
        "handle"
            | "handles"
            | "viewport"
            | "viewports"
            | "boundary"
            | "clip"
            | "insert"
            | "members"
            | "source"
            | "objects"
            | "entities"
            | "selection"
            | "parts"
            | "refs"
            | "matches"
            | "line"
            | "arc"
            | "blocks"
    )
}

/// Collect the handles of `v`, a value under a handle field: a handle string, an array of them,
/// or objects that carry handle fields of their own (`entities`, `parts`).
fn collect_result_handles(s: &Session, v: &Value, depth: usize, out: &mut Vec<Handle>) {
    if depth > RESULT_DEPTH {
        return;
    }
    match v {
        Value::String(t) => {
            if let Some(h) = Handle::parse_hex(t)
                && s.doc().is_ok_and(|d| d.entity(h).is_some())
            {
                out.push(h);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_result_handles(s, x, depth + 1, out)),
        Value::Object(m) => {
            for (k, x) in m {
                if handle_field(k) {
                    collect_result_handles(s, x, depth + 1, out);
                } else {
                    find_result_handles(s, x, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

/// Find handle fields nested in containers that are not handles themselves (`groups` in
/// `groups.list`, whose members are handles): strings here are names, never references.
fn find_result_handles(s: &Session, v: &Value, depth: usize, out: &mut Vec<Handle>) {
    if depth > RESULT_DEPTH {
        return;
    }
    match v {
        Value::Array(a) => a.iter().for_each(|x| find_result_handles(s, x, depth + 1, out)),
        Value::Object(m) => {
            for (k, x) in m {
                if handle_field(k) {
                    collect_result_handles(s, x, depth + 1, out);
                } else {
                    find_result_handles(s, x, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

/// The object handles a command result mentions, in the order it lists them. A handle-only result
/// (a handle string or an array of them) counts as it is; otherwise only handle fields do.
fn result_handles(s: &Session, v: &Value, depth: usize, out: &mut Vec<Handle>) {
    match v {
        Value::String(_) => collect_result_handles(s, v, depth, out),
        Value::Array(a) if a.iter().all(Value::is_string) => collect_result_handles(s, v, depth, out),
        Value::Array(_) | Value::Object(_) => find_result_handles(s, v, depth, out),
        _ => {}
    }
}

/// The last handle `r` returned: what a later `{last}` stands for.
fn returned_handle(s: &Session, r: &Value) -> Option<Handle> {
    let mut hs = Vec::new();
    result_handles(s, r, 0, &mut hs);
    hs.last().copied()
}

/// The active drawing's identity ([`cadcraft_engine::DocState::uid`]): a handle is only meaningful
/// in the drawing it came from, and `new`, `open`, `document.switch` and `close` can make another
/// drawing current.
fn active_drawing(s: &Session) -> Option<u64> {
    s.docs.get(s.active).map(|st| st.uid)
}

/// Every `{last}` in `p`, at any depth, replaced by `h`. With no handle to fill in it is an
/// error, never a token reaching the command as a (bad) handle.
fn fill_last(p: &Value, h: Option<Handle>) -> Result<Value, String> {
    match p {
        Value::String(t) if t.as_str() == LAST_HANDLE => match h {
            Some(h) => Ok(Value::String(h.hex())),
            None => Err(format!("`{LAST_HANDLE}` needs an earlier --cmd in this run to have returned a handle")),
        },
        Value::Array(a) => a.iter().map(|x| fill_last(x, h)).collect::<Result<Vec<_>, _>>().map(Value::Array),
        Value::Object(m) => {
            m.iter().map(|(k, x)| Ok((k.clone(), fill_last(x, h)?))).collect::<Result<serde_json::Map<String, Value>, _>>().map(Value::Object)
        }
        other => Ok(other.clone()),
    }
}

/// One stderr line for a failed `--cmd` step.
fn cmd_step_error_line(id: &str, msg: &str) -> String {
    format!("error: {id}: {msg}")
}
/// `run` returns this when its last step was a failed `--cmd`: the line is already on stderr, so
/// `cli` sets the exit status without printing an empty line.
const RUN_LAST_STEP_FAILED: &str = "";

/// One `--cmd` step. `Err` is the formatted `error: {id}: {msg}` line.
fn cmd_step(s: &mut Session, body: &str, drawing: Option<u64>, last_handle: &mut Option<Handle>) -> Result<(), String> {
    let (id, json) = body.split_once(' ').unwrap_or((body, "{}"));
    let params: Value = serde_json::from_str(json).map_err(|e| cmd_step_error_line(id, &e.to_string()))?;
    let params = fill_last(&params, *last_handle).map_err(|e| cmd_step_error_line(id, &e))?;
    let r = s.execute(id, &params).map_err(|e| cmd_step_error_line(id, &e.to_string()))?;
    if active_drawing(s) != drawing {
        *last_handle = None;
    }
    if let Some(h) = returned_handle(s, &r) {
        *last_handle = Some(h);
    }
    if !r.is_null() {
        outln!("{r}");
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let mut s = Session::empty();
    let mut it = args.iter();
    let mut save = None;
    let mut exp: Option<String> = None;
    let mut framing = Framing::Auto;
    let mut steps: Vec<(String, String)> = Vec::new();
    while let Some(a) = it.next() {
        if framing_option(a, &mut it, &mut framing)? {
            continue;
        }
        match a.as_str() {
            "--sample" => {
                s.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
            }
            "--metric" => {
                s.new_drawing(true);
            }
            "--script" => steps.push(("script".into(), it.next().ok_or("--script needs text")?.replace("\\n", "\n"))),
            "--script-file" => {
                let p = it.next().ok_or("--script-file needs a path")?;
                steps.push(("script".into(), std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?));
            }
            "--cmd" => steps.push(("cmd".into(), it.next().ok_or("--cmd needs `id {json}`")?.clone())),
            "--save" => save = Some(it.next().ok_or("--save needs a path")?.clone()),
            "--export" => exp = Some(it.next().ok_or("--export needs a path")?.clone()),
            f if !f.starts_with("--") => open(&mut s, f)?,
            other => return Err(format!("unknown option {other}")),
        }
    }
    if s.docs.is_empty() {
        s.new_drawing(false);
    }
    check_framed_output(framing, exp.as_deref())?;
    let before = model_views(&s);
    let mut last_handle: Option<Handle> = None;
    let mut last_step_error: Option<String> = None;
    for (kind, body) in steps {
        // A handle belongs to the drawing that returned it: if this step makes another drawing
        // current (`new`, `open`, `document.switch`, `close`, or a script running one of them),
        // the remembered handle is stale. A handle the step itself returns is taken afterwards,
        // so it is a handle of the drawing it made current.
        let drawing = active_drawing(&s);
        match kind.as_str() {
            "script" => {
                s.script(&body).map_err(|e| e.to_string())?;
                last_step_error = None;
                if active_drawing(&s) != drawing {
                    last_handle = None;
                }
            }
            _ => match cmd_step(&mut s, &body, drawing, &mut last_handle) {
                Ok(()) => last_step_error = None,
                Err(line) => {
                    eprintln!("{line}");
                    last_step_error = Some(line);
                }
            },
        }
    }
    for l in &s.log {
        eprintln!("{l}");
    }
    if let Some(p) = save {
        export(&s, &p)?;
    }
    if let Some(p) = exp {
        let window = if cadcraft_io::is_image(&p) { export_window(&s, framing, Some(&before))? } else { None };
        export_framed(&s, &p, window)?;
    }
    match last_step_error {
        Some(_) => Err(RUN_LAST_STEP_FAILED.to_string()),
        None => Ok(()),
    }
}

/// Small deterministic generator (xorshift64*) so perf runs are repeatable without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// A synthetic drawing with `n` mixed entities (lines, circles, arcs, polylines, text) spread
/// over a square whose side grows with sqrt(n), so density stays constant.
fn synthetic(n: usize, side: f64) -> Result<cadcraft_engine::doc::Drawing, String> {
    use cadcraft_engine::doc::{self as d, Common, EntityKind, Space};
    use cadcraft_engine::geom::{PolyVertex, Vec2, Vec3};
    let mut dr = d::Drawing::new_imperial();
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let v3 = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    for i in 0..n {
        let (x, y) = (r.range(0.0, side), r.range(0.0, side));
        let s = r.range(0.5, 8.0);
        let kind = match i % 5 {
            0 => EntityKind::Line(d::Line { a: v3(x, y), b: v3(x + r.range(-s, s), y + r.range(-s, s)) }),
            1 => EntityKind::Circle(d::Circle { center: v3(x, y), radius: s * 0.5 }),
            2 => EntityKind::Arc(d::Arc { center: v3(x, y), radius: s * 0.5, start: r.range(0.0, 3.0), end: r.range(3.2, 6.2) }),
            3 => EntityKind::LwPolyline(d::LwPolyline {
                vertices: (0..6)
                    .map(|k| PolyVertex {
                        p: Vec2::new(x + f64::from(k) * s * 0.3, y + r.range(-s, s) * 0.3),
                        bulge: if k == 2 { 0.4 } else { 0.0 },
                        start_width: 0.0,
                        end_width: 0.0,
                    })
                    .collect(),
                closed: i % 2 == 0,
                const_width: 0.0,
                elevation: 0.0,
                plinegen: false,
            }),
            _ => EntityKind::Text(d::Text {
                insert: v3(x, y),
                align_pt: None,
                height: s * 0.2,
                value: format!("T{i}"),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: d::HAlign::Left,
                valign: d::VAlign::Baseline,
            }),
        };
        dr.add(&Space::Model, Common::default(), kind).map_err(|e| e.to_string())?;
    }
    Ok(dr)
}

/// `perf FILE`: the display list built as the canvas builds it (a 1600 x 1000 px view at the
/// drawing's centre: tolerance, dash level of detail and view window from the zoom band), at
/// zoom extents and zoomed in 4x at a time, next to the full-detail build plots use.
fn perf_file(path: &str) -> Result<(), String> {
    use cadcraft_engine::geom::{Bounds2, Vec2};
    use cadcraft_engine::render::{Options, build};
    use std::time::Instant;
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    let mut s = Session::empty();
    let t = Instant::now();
    open(&mut s, path)?;
    let open_ms = ms(t);
    let st = s.state().map_err(|e| e.to_string())?;
    let (d, space) = (&st.doc, &st.space);
    let ext = d.extents(space);
    let size = ext.width().max(ext.height());
    if ext.is_empty() || !(size.is_finite() && size > 0.0) {
        return Err("perf: the drawing is empty".into());
    }
    outln!("open {open_ms:.0} ms, {} entities, extents {:.1} x {:.1}", d.entity_count(), ext.width(), ext.height());
    outln!("{:>8} {:>10} {:>10}", "zoom", "build ms", "prims");
    let mut finest = Options::default();
    for k in 0..5 {
        let zoom = 4f64.powi(k);
        let h = size / zoom;
        let pixel = 2f64.powi((h / 1000.0).max(1e-300).log2().floor() as i32);
        let half = Vec2::new(h * 0.8, h * 0.5);
        let visible = Bounds2::new(ext.center() - half, ext.center() + half);
        let screen = Options { tolerance: pixel * 0.5, min_dash: pixel * 2.0, view: Some(visible.expand(h * 0.8)), ..Default::default() };
        let t = Instant::now();
        let n = build(d, space, &screen).prims.len();
        outln!("{:>7}x {:>10.1} {n:>10}", zoom, ms(t));
        finest = screen;
    }
    // Plots: every pattern line, everywhere (last, so its large list doesn't weigh on the above).
    let t = Instant::now();
    let n = build(d, space, &Options { min_dash: 0.0, view: None, ..finest }).prims.len();
    outln!("{:>8} {:>10.1} {n:>10}", "full", ms(t));
    Ok(())
}

fn perf(args: &[String]) -> Result<(), String> {
    use cadcraft_engine::doc::Space;
    use cadcraft_engine::geom::{Bounds2, Vec2};
    use cadcraft_engine::{select, snap};
    use std::time::Instant;
    if let Some(path) = args.first().filter(|a| a.parse::<usize>().is_err()) {
        return perf_file(path);
    }
    let n: usize = match args.first() {
        Some(a) => a.parse().map_err(|_| format!("perf: bad entity count {a}"))?,
        None => 200_000,
    };
    let n = n.clamp(1, 20_000_000);
    let side = (n as f64).sqrt() * 10.0;
    let mut rows: Vec<(String, f64, String)> = Vec::new();
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let d = synthetic(n, side)?;
    rows.push(("build synthetic drawing".into(), ms(t), format!("{n} entities")));

    // Display list at a 1600 px wide "zoom extents" view (tolerance = half a pixel).
    let px = side / 1600.0;
    let opts = cadcraft_engine::render::Options { tolerance: px * 0.5, min_dash: px * 2.0, ..Default::default() };
    let t = Instant::now();
    let list = cadcraft_engine::render::build(&d, &Space::Model, &opts);
    rows.push(("display list build".into(), ms(t), format!("{} prims, {} segments", list.prims.len(), list.segment_count())));

    // Hit testing on a zoomed-in view (aperture = 5 px at 0.02 units/px).
    let ap = 0.1;
    let mut r = Rng(42);
    let pts: Vec<Vec2> = (0..1000).map(|_| Vec2::new(r.range(0.0, side), r.range(0.0, side))).collect();
    let t = Instant::now();
    let first = select::pick(&d, &Space::Model, pts.first().copied().unwrap_or(Vec2::ZERO), ap);
    rows.push(("first pick (incl. index build)".into(), ms(t), format!("{first:?}")));
    let t = Instant::now();
    let hits = pts.iter().filter(|p| select::pick(&d, &Space::Model, **p, ap).is_some()).count();
    let total = ms(t);
    rows.push(("pick x1000".into(), total, format!("{hits} hits, {:.3} ms/pick", total / 1000.0)));

    // One edit, then a pick: only the touched chunk is re-indexed.
    let mut edited = d.clone();
    if let Some(h) = edited.model.last().map(|e| e.handle) {
        edited.model.modify(h, |e| e.common.layer = "0".into());
    }
    let t = Instant::now();
    let _ = select::pick(&edited, &Space::Model, pts.first().copied().unwrap_or(Vec2::ZERO), ap);
    rows.push(("pick after one edit (incremental re-index)".into(), ms(t), String::new()));

    let mut sel = 0usize;
    let t = Instant::now();
    for k in 0..20 {
        let c = Vec2::new(r.range(0.0, side), r.range(0.0, side));
        let half = side * 0.05;
        sel += select::select_window(&d, &Space::Model, Bounds2::new(c - Vec2::new(half, half), c + Vec2::new(half, half)), k % 2 == 1).len();
    }
    rows.push(("window/crossing select x20 (10% side)".into(), ms(t), format!("{sel} selected")));

    let t = Instant::now();
    let fence: Vec<Vec2> = (0..4).map(|_| Vec2::new(r.range(0.0, side), r.range(0.0, side))).collect();
    let fsel = select::select_fence(&d, &Space::Model, &fence).len();
    rows.push(("fence select (3 legs)".into(), ms(t), format!("{fsel} selected")));

    let osmode = snap::mode::END | snap::mode::MID | snap::mode::CEN | snap::mode::QUA | snap::mode::INT;
    let t = Instant::now();
    let snaps = pts.iter().filter(|p| snap::osnap(&d, &Space::Model, **p, ap * 2.0, osmode, None, false).is_some()).count();
    let total = ms(t);
    rows.push(("osnap x1000".into(), total, format!("{snaps} hits, {:.3} ms/query", total / 1000.0)));

    let t = Instant::now();
    let bytes = cadcraft_io::write(&d, "perf.dxf").map_err(|e| e.to_string())?;
    rows.push(("DXF write".into(), ms(t), format!("{:.1} MB", bytes.len() as f64 / 1e6)));
    let t = Instant::now();
    let back = cadcraft_io::read(&bytes, "perf.dxf").map_err(|e| e.to_string())?;
    rows.push(("DXF read".into(), ms(t), format!("{} entities", back.entity_count())));

    outln!("{:<40} {:>12}  notes", "operation", "ms");
    outln!("{}", "-".repeat(80));
    for (name, t, note) in rows {
        outln!("{name:<40} {t:>12.2}  {note}");
    }
    Ok(())
}

/// The backend `cadcraft-cli mcp` serves from.
#[derive(Debug, PartialEq, Eq)]
enum McpTarget {
    /// An in-process drawing session (no `--connect`).
    Headless,
    /// The running app's control channel at `HOST:PORT`; `lazy`: reached at the first call;
    /// `launch`: opened then if it isn't running.
    Connect { addr: String, lazy: bool, launch: bool },
}

/// Parses the arguments after `mcp`. `--connect` must be followed by a `HOST:PORT`; a missing,
/// empty or flag-like value is an error, never a silent fall back to a headless session (#103).
fn parse_mcp_args(args: &[String]) -> Result<McpTarget, String> {
    let mut addr: Option<String> = None;
    let (mut lazy, mut launch) = (false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--connect" => {
                let a = it
                    .next()
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty() && !v.starts_with('-'))
                    .ok_or("--connect needs HOST:PORT (the app's --control port, e.g. 127.0.0.1:7979)")?;
                if addr.is_some() {
                    return Err("--connect given more than once".into());
                }
                addr = Some(a.to_string());
            }
            "--lazy" => lazy = true,
            "--launch" => launch = true,
            other => return Err(format!("mcp: unknown argument {other}\n{USAGE}")),
        }
    }
    match addr {
        // `--launch` connects at the first call too (the app opens then).
        Some(addr) => Ok(McpTarget::Connect { addr, lazy: lazy || launch, launch }),
        None if lazy || launch => Err("--lazy and --launch need --connect HOST:PORT".into()),
        None => Ok(McpTarget::Headless),
    }
}

fn mcp(args: &[String]) -> Result<(), String> {
    let backend: Box<dyn cadcraft_mcp::Backend> = match parse_mcp_args(args)? {
        McpTarget::Connect { addr, lazy: true, launch } => Box::new(cadcraft_mcp::Remote::lazy(&addr, launch)),
        McpTarget::Connect { addr, .. } => match cadcraft_mcp::Remote::connect(&addr) {
            Ok(r) => Box::new(r),
            Err(e) => return Err(format!("cadcraft-cli: cannot connect to {addr}: {e}")),
        },
        McpTarget::Headless => Box::new(cadcraft_mcp::Headless::default()),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    cadcraft_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod mcp_args_tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mcp_without_connect_is_headless() {
        assert_eq!(parse_mcp_args(&args(&[])), Ok(McpTarget::Headless));
    }

    #[test]
    fn mcp_connect_with_address_selects_remote() {
        let to = |addr: &str, lazy, launch| Ok(McpTarget::Connect { addr: addr.into(), lazy, launch });
        assert_eq!(parse_mcp_args(&args(&["--connect", "127.0.0.1:7979"])), to("127.0.0.1:7979", false, false));
        assert_eq!(parse_mcp_args(&args(&["--connect", "localhost:7979"])), to("localhost:7979", false, false));
        assert_eq!(parse_mcp_args(&args(&["--lazy", "--connect", "localhost:7979"])), to("localhost:7979", true, false));
        assert_eq!(parse_mcp_args(&args(&["--connect", "localhost:7979", "--launch"])), to("localhost:7979", true, true));
        // Without an app to reach they mean nothing: never a headless session.
        assert!(parse_mcp_args(&args(&["--lazy"])).is_err());
        assert!(parse_mcp_args(&args(&["--launch"])).is_err());
    }

    #[test]
    fn mcp_connect_without_address_is_an_error_not_headless() {
        for bad in [&["--connect"][..], &["--connect", ""], &["--connect", "  "], &["--connect", "--foo"], &["--connect", "-v"]] {
            let r = parse_mcp_args(&args(bad));
            let Err(e) = r else { panic!("{bad:?} should be rejected, got {r:?}") };
            assert!(e.contains("--connect needs HOST:PORT"), "{bad:?}: {e}");
        }
    }

    #[test]
    fn mcp_rejects_unknown_and_repeated_arguments() {
        assert!(parse_mcp_args(&args(&["--conect", "127.0.0.1:7979"])).is_err());
        assert!(parse_mcp_args(&args(&["127.0.0.1:7979"])).is_err());
        assert!(parse_mcp_args(&args(&["--connect", "127.0.0.1:1", "--connect", "127.0.0.1:2"])).is_err());
    }
}

fn main() -> ExitCode {
    let code = cli();
    if STDOUT_ERROR.load(Ordering::Relaxed) { ExitCode::FAILURE } else { code }
}

fn cli() -> ExitCode {
    install_io();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    let r = match args.first().map(String::as_str) {
        Some("info") => validate_options("info", &rest, NO_OPTIONS).and_then(|_| rest.first().ok_or_else(|| USAGE.to_string())).and_then(|p| info(p)),
        Some("convert") => convert(&rest),
        Some("run") => run(&rest),
        Some("render") => validate_options("render", &rest, RENDER_OPTIONS).and_then(|_| render(&rest)),
        Some("perf") => validate_options("perf", &rest, NO_OPTIONS).and_then(|_| perf(&rest)),
        Some("sample") => validate_options("sample", &rest, NO_OPTIONS).and_then(|_| sample(&rest)),
        Some("commands") => validate_options("commands", &rest, NO_OPTIONS).map(|_| {
            let s = Session::new();
            let f = rest.first().map(|x| x.to_ascii_lowercase());
            let v: Vec<Value> = cadcraft_engine::command_specs()
                .iter()
                .filter(|c| f.as_ref().is_none_or(|f| c.id.contains(f.as_str()) || c.label.to_ascii_lowercase().contains(f.as_str())))
                .map(|c| serde_json::to_value(c.info(&s)).unwrap_or_default())
                .collect();
            outln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        }),
        Some("mcp") => mcp(&rest),
        Some("--version" | "-V") => {
            outln!("cadcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h" | "help") => {
            outln!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}");
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cadcraft-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn write_atomic_replaces_file_and_leaves_no_staging_files() {
        let dir = scratch_dir("replace");
        let out = dir.join("a.dxf");
        let out = out.to_str().expect("utf-8 path");
        write_atomic(out, b"old").expect("create");
        write_atomic(out, b"new").expect("overwrite");
        assert_eq!(std::fs::read(out).expect("read"), b"new");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_failure_keeps_destination_and_cleans_up() {
        let dir = scratch_dir("fail");
        let out = dir.join("target");
        std::fs::create_dir(&out).expect("dir as destination");
        std::fs::write(out.join("keep"), b"x").expect("seed");
        assert!(write_atomic(out.to_str().expect("utf-8 path"), b"new").is_err());
        assert_eq!(std::fs::read(out.join("keep")).expect("kept"), b"x");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn b(x0: f64, y0: f64, x1: f64, y1: f64) -> Bounds2 {
        Bounds2::new(Vec2::new(x0, y0), Vec2::new(x1, y1))
    }

    fn sample_session() -> Session {
        let mut s = Session::empty();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
        s
    }

    fn view_box(svg: &str) -> String {
        let i = svg.find("viewBox=\"").map(|i| i + 9).unwrap_or(0);
        svg[i..].split('"').next().unwrap_or("").to_string()
    }

    fn assert_near(a: Bounds2, e: Bounds2) {
        let d = (a.min - e.min).len().max((a.max - e.max).len());
        assert!(d < 1e-9, "{a:?} != {e:?}");
    }

    #[test]
    fn window_parses() {
        assert_eq!(parse_window("0,0,30,20"), Ok(b(0.0, 0.0, 30.0, 20.0)));
        assert_eq!(parse_window(" -5.5, -1e3 ,2e3,4 "), Ok(b(-5.5, -1000.0, 2000.0, 4.0)));
    }

    #[test]
    fn hostile_window_is_an_error() {
        for w in [
            "",
            "1,2,3",
            "1,2,3,4,5",
            "a,0,1,1",
            "NaN,0,1,1",
            "0,0,inf,1",
            "0,0,1e400,1",
            "0,0,0,1",
            "0,0,1,0",
            "5,0,1,1",
            "0,5,1,1",
            "-1e300,-1e300,1e300,1e300",
            "0,0,1e-12,1e-12",
        ] {
            assert!(parse_window(w).is_err(), "`{w}` should be rejected");
        }
    }

    #[test]
    fn framing_options() {
        let parse = |args: &[&str]| -> Result<Framing, String> {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            let mut f = Framing::Auto;
            let mut it = args.iter();
            while let Some(a) = it.next() {
                assert!(framing_option(a, &mut it, &mut f)?, "{a} is a framing option");
            }
            Ok(f)
        };
        assert_eq!(parse(&["--view", "extents"]), Ok(Framing::Extents));
        assert_eq!(parse(&["--view", "CURRENT"]), Ok(Framing::Current));
        assert_eq!(parse(&["--window", "0,0,2,1"]), Ok(Framing::Window(b(0.0, 0.0, 2.0, 1.0))));
        assert!(parse(&["--view"]).is_err());
        assert!(parse(&["--view", "selection"]).is_err());
        assert!(parse(&["--window"]).is_err());
        assert!(parse(&["--window", "0,0,2,1", "--view", "extents"]).is_err());
        assert!(check_framed_output(Framing::Extents, Some("a.dxf")).is_err());
        assert!(check_framed_output(Framing::Extents, None).is_err());
        assert!(check_framed_output(Framing::Extents, Some("a.png")).is_ok());
        assert!(check_framed_output(Framing::Auto, Some("a.dxf")).is_ok());
    }

    #[test]
    fn last_placeholder_takes_the_previous_handle() {
        let mut s = sample_session();
        let r = s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        let h = returned_handle(&s, &r).expect("LINE returns its handle");
        assert_eq!(fill_last(&json!({"handle": "{last}"}), Some(h)), Ok(json!({"handle": h.hex()})));
        // Nested, as in a selection list.
        assert_eq!(fill_last(&json!({"handles": ["{last}", "1"]}), Some(h)), Ok(json!({"handles": [h.hex(), "1"]})));
        // Only the exact token: a name or a sentence that merely contains it stays as written.
        for p in [json!("last"), json!("{last} "), json!({"name": "last"})] {
            assert_eq!(fill_last(&p, Some(h)), Ok(p.clone()));
        }
        // Nothing returned yet: an error, not a token passed to the command as a bad handle.
        assert!(fill_last(&json!({"handle": "{last}"}), None).unwrap_err().contains("{last}"));
    }

    /// Handles are read from a result by looking the strings up in the drawing: counts and names
    /// are not mistaken for handles, and `mview`'s `viewports` works like `handle`/`handles`.
    #[test]
    fn returned_handle_reads_only_real_handles() {
        let mut s = Session::empty();
        s.new_drawing(false);
        s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
        let r = s.execute("mview", &json!({"p1": [20, 20], "p2": [200, 150]})).unwrap();
        let h = returned_handle(&s, &r).expect("mview returns its viewport handle");
        assert!(r["viewports"].as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(h.hex().as_str()))), "{r}");
        assert_eq!(returned_handle(&s, &json!({"changed": 3, "name": "A1"})), None);
        // The most recent of several.
        let a = s.execute("line", &json!({"points": [[0, 0], [1, 0]]})).unwrap();
        let b = s.execute("line", &json!({"points": [[0, 0], [2, 0]]})).unwrap();
        let (ha, hb) = (returned_handle(&s, &a).unwrap(), returned_handle(&s, &b).unwrap());
        assert_eq!(returned_handle(&s, &json!({"handles": [ha.hex(), hb.hex()]})), Some(hb));
    }

    #[test]
    fn default_framing_is_extents() {
        // convert: nothing ran, always the extents.
        let s = sample_session();
        assert_eq!(export_window(&s, Framing::Auto, None), Ok(None));
        // run: commands that leave the view alone keep the extents framing.
        let mut s = sample_session();
        let before = model_views(&s);
        s.script("LINE 0,0 10,10\n\n").unwrap();
        assert_eq!(export_window(&s, Framing::Auto, Some(&before)), Ok(None));
        // A fresh drawing too.
        let mut s = Session::empty();
        s.new_drawing(false);
        let before = model_views(&s);
        s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
        assert_eq!(export_window(&s, Framing::Auto, Some(&before)), Ok(None));
    }

    #[test]
    fn zoom_window_frames_export() {
        let mut s = sample_session();
        let before = model_views(&s);
        s.execute("zoom", &json!({"mode": "window", "p1": [0, 0], "p2": [30, 20]})).unwrap();
        let w = export_window(&s, Framing::Auto, Some(&before)).unwrap().unwrap();
        // The CLI viewport is 3:2, so a 3:2 window is shown exactly.
        assert_near(w, b(0.0, 0.0, 30.0, 20.0));
        assert_eq!(export_window(&s, Framing::Current, Some(&before)), Ok(Some(w)));
        // --view extents and --window override the zoom.
        assert_eq!(export_window(&s, Framing::Extents, Some(&before)), Ok(None));
        let explicit = b(1.0, 1.0, 2.0, 2.0);
        assert_eq!(export_window(&s, Framing::Window(explicit), Some(&before)), Ok(Some(explicit)));
        // A taller window keeps its height; the 3:2 image adds room left and right.
        s.execute("zoom", &json!({"mode": "center", "center": [5, 5], "height": 4})).unwrap();
        assert_near(export_window(&s, Framing::Auto, Some(&before)).unwrap().unwrap(), b(2.0, 3.0, 8.0, 7.0));
    }

    #[test]
    fn zoom_object_frames_the_selection() {
        let mut s = sample_session();
        let before = model_views(&s);
        // The smallest entity with an area, selected then framed with ZOOM Object.
        let d = s.doc().unwrap();
        let bounds = |e: &cadcraft_engine::doc::Entity| cadcraft_engine::doc::entity_bounds(d, e, 0);
        let e = d
            .model
            .iter()
            .filter(|e| bounds(e).width() > 0.0 && bounds(e).height() > 0.0)
            .min_by(|a, b| bounds(a).width().total_cmp(&bounds(b).width()))
            .unwrap();
        let (h, eb, ext) = (e.handle, bounds(e), d.extents(&Space::Model));
        s.set_selection(vec![h]);
        s.execute("zoom", &json!({"mode": "object"})).unwrap();
        let w = export_window(&s, Framing::Auto, Some(&before)).unwrap().unwrap();
        assert!(w.contains(eb.min) && w.contains(eb.max));
        assert!(w.width() < ext.width() && w.height() < ext.height(), "{w:?} vs extents {ext:?}");
    }

    #[test]
    fn run_export_honours_zoom() {
        let dir = std::env::temp_dir().join(format!("cadcraft-cli-165-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = |n: &str| dir.join(n).to_string_lossy().to_string();
        let run_svg = |extra: &[&str], out: &str| -> String {
            let mut args: Vec<String> = ["--sample"].iter().chain(extra).map(|a| a.to_string()).collect();
            args.extend(["--export".to_string(), path(out)]);
            run(&args).unwrap();
            std::fs::read_to_string(path(out)).unwrap()
        };
        let base = run_svg(&[], "base.svg");
        let zoomed = run_svg(&["--cmd", r#"zoom {"mode":"window","p1":[0,0],"p2":[30,20]}"#], "zoom.svg");
        let window = run_svg(&["--window", "0,0,30,20"], "window.svg");
        let forced = run_svg(&["--cmd", r#"zoom {"mode":"window","p1":[0,0],"p2":[30,20]}"#, "--view", "extents"], "forced.svg");
        assert_ne!(view_box(&zoomed), view_box(&base));
        assert_eq!(view_box(&zoomed), "0 0 30.000000 20.000000");
        assert_eq!(zoomed, window);
        assert_eq!(forced, base);
        // Hostile --window: a clear error, nothing written.
        let bad: Vec<String> = ["--sample", "--window", "0,0,NaN,1", "--export", &path("bad.png")].iter().map(|a| a.to_string()).collect();
        assert!(run(&bad).unwrap_err().contains("--window"));
        assert!(!dir.join("bad.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn validators_accept_only_the_documented_options() {
        assert!(validate_options("info", &args(&["file.dxf"]), NO_OPTIONS).is_ok());
        assert!(validate_options("perf", &args(&["100"]), NO_OPTIONS).is_ok());
        assert!(validate_options("sample", &args(&["bracket", "out.dxf"]), NO_OPTIONS).is_ok());
        assert!(validate_options("commands", &args(&["line"]), NO_OPTIONS).is_ok());
        for (command, specs) in [("info", NO_OPTIONS), ("perf", NO_OPTIONS), ("sample", NO_OPTIONS), ("commands", NO_OPTIONS)] {
            assert!(validate_options(command, &args(&["--typo"]), specs).is_err());
        }
    }

    #[test]
    fn unknown_options_are_rejected_before_side_effects() {
        assert_eq!(validate_options("info", &args(&["missing.dxf", "--typo"]), NO_OPTIONS), Err("info: unknown option --typo".into()));
        assert_eq!(validate_options("perf", &args(&["1", "--typo"]), NO_OPTIONS), Err("perf: unknown option --typo".into()));
        assert_eq!(validate_options("sample", &args(&["bracket", "out.dxf", "--typo"]), NO_OPTIONS), Err("sample: unknown option --typo".into()));
    }

    #[test]
    fn format_projected_is_fixed_width() {
        assert_eq!(format_projected([1.0, 2.0, 3.0, 4.0]), "PROJECTED 1.000000,2.000000,3.000000,4.000000");
    }

    #[test]
    fn no_adapter_exits_three() {
        assert_eq!(render_error_exit(&cadcraft_render3d::Render3dError::NoAdapter), 3);
        assert_eq!(render_error_exit(&cadcraft_render3d::Render3dError::Wgpu("x".into())), 1);
        assert_eq!(render_error_exit(&cadcraft_render3d::Render3dError::Buffer("y".into())), 1);
    }

    #[test]
    fn cmd_step_error_line_names_the_command() {
        assert_eq!(cmd_step_error_line("door", "too wide"), "error: door: too wide");
    }

    #[test]
    fn cmd_error_does_not_abort_the_run() {
        let dir = scratch_dir("cmd_err");
        let out = dir.join("a.dxf").to_string_lossy().to_string();
        let args: Vec<String> = ["--metric", "--cmd", "no.such.command {}", "--cmd", r#"line {"points":[[0,0],[10,0]]}"#, "--save", &out]
            .iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(run(&args), Ok(()));
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("LINE"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cmd_json_parse_error_does_not_abort_the_run() {
        let dir = scratch_dir("cmd_json");
        let out = dir.join("a.dxf").to_string_lossy().to_string();
        let args: Vec<String> = ["--metric", "--cmd", r#"line {"points": [["#, "--cmd", r#"line {"points":[[0,0],[10,0]]}"#, "--save", &out]
            .iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(run(&args), Ok(()));
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("LINE"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_good_last_step_clears_the_failure_status() {
        // Bad --cmd then good --cmd -> Ok
        let good_args: Vec<String> =
            ["--metric", "--cmd", "no.such.command {}", "--cmd", "drawing.inspect {}"].iter().map(|a| a.to_string()).collect();
        assert_eq!(run(&good_args), Ok(()));
        // Good first, bad last -> Err with RUN_LAST_STEP_FAILED
        let bad_args: Vec<String> =
            ["--metric", "--cmd", r#"line {"points":[[0,0],[10,0]]}"#, "--cmd", "no.such.command {}"].iter().map(|a| a.to_string()).collect();
        assert_eq!(run(&bad_args), Err(RUN_LAST_STEP_FAILED.to_string()));
    }

    #[test]
    fn cmd_error_leaves_last_handle_untouched() {
        let mut s = sample_session();
        let r = s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        let h = returned_handle(&s, &r).expect("line returns a handle");
        let mut last_handle = Some(h);
        let drawing = active_drawing(&s);
        let result = cmd_step(&mut s, "no.such.command {}", drawing, &mut last_handle);
        assert!(result.is_err());
        assert_eq!(last_handle, Some(h), "failing cmd_step must not clear last_handle");
    }

    #[test]
    fn script_step_is_still_fatal() {
        let args: Vec<String> = ["--metric", "--script", "NO.SUCH.COMMAND\n", "--cmd", "drawing.inspect {}"].iter().map(|a| a.to_string()).collect();
        assert!(run(&args).is_err());
    }

    #[test]
    fn script_step_clears_cmd_error() {
        let args: Vec<String> = ["--metric", "--cmd", "no.such.command {}", "--script", "LINE 0,0 10,10\n\n"].iter().map(|a| a.to_string()).collect();
        assert_eq!(run(&args), Ok(()));
    }

    #[test]
    fn multiple_cmd_errors_all_reported() {
        let args: Vec<String> = ["--metric", "--cmd", "no.such.command {}", "--cmd", "another.missing {}", "--cmd", "drawing.inspect {}"]
            .iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(run(&args), Ok(()));
    }

    #[test]
    fn cmd_with_no_json_object() {
        let args: Vec<String> = ["--metric", "--cmd", "drawing.inspect", "--cmd", "drawing.inspect {}"].iter().map(|a| a.to_string()).collect();
        assert_eq!(run(&args), Ok(()));
    }

    #[test]
    fn render_no_adapter_exit_constant() {
        assert_eq!(RENDER_NO_ADAPTER_EXIT, 3);
    }

    #[test]
    fn format_projected_edge_cases() {
        // All zeros
        assert_eq!(format_projected([0.0; 4]), "PROJECTED 0.000000,0.000000,0.000000,0.000000");
        // Negative values
        assert_eq!(format_projected([-1.5, -2.5, -3.5, -4.5]), "PROJECTED -1.500000,-2.500000,-3.500000,-4.500000");
        // Large values
        assert_eq!(format_projected([1e6, 2e6, 3e6, 4e6]), "PROJECTED 1000000.000000,2000000.000000,3000000.000000,4000000.000000");
        // Values that round up (use 8 in 7th decimal so f64 imprecision can't flip it)
        assert_eq!(format_projected([1.0000008, 2.0000008, 3.0000008, 4.0000008]), "PROJECTED 1.000001,2.000001,3.000001,4.000001");
        // Values that round down
        assert_eq!(format_projected([1.0000003, 2.0000003, 3.0000003, 4.0000003]), "PROJECTED 1.000000,2.000000,3.000000,4.000000");
    }

    #[test]
    fn parse_size_valid() {
        assert_eq!(parse_size("2400x1600"), Ok((2400, 1600)));
        assert_eq!(parse_size("100x100"), Ok((100, 100)));
        assert_eq!(parse_size("1x1"), Ok((1, 1)));
        assert_eq!(parse_size("1920x1080"), Ok((1920, 1080)));
        // With spaces and uppercase X
        assert_eq!(parse_size(" 100 x 200 "), Ok((100, 200)));
        assert_eq!(parse_size("100X200"), Ok((100, 200)));
        // u32 max
        assert_eq!(parse_size("4294967295x4294967295"), Ok((4294967295, 4294967295)));
    }

    #[test]
    fn parse_size_invalid() {
        assert!(parse_size("").is_err());
        assert!(parse_size("100").is_err());
        assert!(parse_size("100x").is_err());
        assert!(parse_size("x100").is_err());
        assert!(parse_size("0x100").is_err());
        assert!(parse_size("100x0").is_err());
        assert!(parse_size("abcxdef").is_err());
        assert!(parse_size("100x200x300").is_err());
        assert!(parse_size("100 x 200 x 300").is_err());
        // u32 max + 1
        assert!(parse_size("4294967296x100").is_err());
        // Negative
        assert!(parse_size("-1x100").is_err());
        // Decimal
        assert!(parse_size("100.5x100").is_err());
    }

    #[test]
    fn parse_camera_valid() {
        let bounds = cadcraft_arch3d::mesh::Bounds3::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let base = cadcraft_render3d::camera::Orbit::fit(&bounds);
        // Just "orbit"
        assert!(parse_camera("orbit", base).is_ok());
        // All fields
        let cam = parse_camera("orbit,azimuth=1.5,elevation=0.5,distance=10,fov=45", base).unwrap();
        // Empty tokens after commas are skipped
        assert!(parse_camera("orbit,,azimuth=1.5,,", base).is_ok());
        // Whitespace around fields
        assert!(parse_camera("orbit , azimuth = 1.5 , elevation = 0.5", base).is_ok());
    }

    #[test]
    fn format_projected_rounds_half_up_at_seventh_decimal() {
        // 0.5 in the 7th decimal should round up
        assert_eq!(format_projected([1.0000005, 2.0000005, 3.0000005, 4.0000005]), "PROJECTED 1.000001,2.000001,3.000001,4.000001");
    }

    #[test]
    fn format_projected_very_small_values_round_to_zero() {
        assert_eq!(format_projected([1e-10, 2e-10, 3e-10, 4e-10]), "PROJECTED 0.000000,0.000000,0.000000,0.000000");
    }

    #[test]
    fn format_projected_f32_max_is_finite() {
        let max = f32::MAX as f64;
        let s = format_projected([max, max, max, max]);
        assert!(s.starts_with("PROJECTED "));
        let rest = s.strip_prefix("PROJECTED ").unwrap();
        let parts: Vec<&str> = rest.split(',').collect();
        assert_eq!(parts.len(), 4);
        for p in parts {
            let v: f64 = p.parse().unwrap();
            assert!(v.is_finite());
        }
    }

    #[test]
    fn projected_output_round_trips() {
        let pb = [1.234567, -2.345678, 3.456789, -4.567890];
        let s = format_projected(pb);
        let rest = s.strip_prefix("PROJECTED ").unwrap();
        let parts: Vec<&str> = rest.split(',').collect();
        assert_eq!(parts.len(), 4);
        for (i, p) in parts.iter().enumerate() {
            let v: f64 = p.parse().unwrap();
            assert!((v - pb[i]).abs() < 1e-6, "round-trip failed for index {i}: {v} vs {}", pb[i]);
        }
    }

    #[test]
    fn parse_camera_invalid() {
        let bounds = cadcraft_arch3d::mesh::Bounds3::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let base = cadcraft_render3d::camera::Orbit::fit(&bounds);
        assert!(parse_camera("", base).is_err());
        assert!(parse_camera("perspective", base).is_err());
        assert!(parse_camera("orbit,azimuth", base).is_err());
        assert!(parse_camera("orbit,azimuth=abc", base).is_err());
        assert!(parse_camera("orbit,azimuth=NaN", base).is_err());
        assert!(parse_camera("orbit,distance=0", base).is_err());
        assert!(parse_camera("orbit,distance=-1", base).is_err());
        assert!(parse_camera("orbit,fov=0", base).is_err());
        assert!(parse_camera("orbit,fov=-1", base).is_err());
        assert!(parse_camera("orbit,unknown=1", base).is_err());
    }
}
