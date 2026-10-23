//! Grip editing: dragging an object's grips (the points from [`EntityKind::grips`]) the way
//! AutoCAD does, plus the multi-functional grip modes (move, rotate, scale, mirror about a grip).
//!
//! The UI calls [`Session::grip_edit`]; every edit runs through a `grip.*` command so it is
//! undoable and reachable from scripts, the control channel and MCP.

use cadcraft_doc::arch::Opening;
use cadcraft_doc::{ARCH_HANDLE_BASE, ArchId, DimKind, EntityKind, Handle, LwPolyline, OPENING_HANDLE_OFFSET, is_arch_handle, is_opening_handle};
use cadcraft_geom::{Arc, EPS, Mat3, Polyline, Vec2, Vec3, norm_angle};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{EngineError, Result, Session};

/// What dragging a hot grip does (AutoCAD's grip modes, cycled with Space/Enter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GripMode {
    /// Move the grip itself (the object's shape changes).
    #[default]
    Stretch,
    /// Move the whole object by the grip displacement.
    Move,
    /// Rotate the object about the grip; the angle is from the grip to the new point.
    Rotate,
    /// Scale the object about the grip; the factor is the distance from the grip to the new point.
    Scale,
    /// Mirror the object about the line from the grip to the new point.
    Mirror,
}

fn set_xy(p: &mut Vec3, q: Vec2) {
    p.x = q.x;
    p.y = q.y;
}

fn translated(k: &EntityKind, d: Vec2) -> EntityKind {
    let mut k = k.clone();
    k.transform(&Mat3::translate(d));
    k
}

/// What a polyline grip index refers to (matches the order of `EntityKind::grips`).
enum PolyGrip {
    Vertex(usize),
    /// Midpoint of the segment starting at this vertex.
    Mid(usize),
}

fn poly_grip(p: &LwPolyline, index: usize) -> Option<PolyGrip> {
    let n = p.vertices.len();
    // Start vertices of the non-degenerate segments, in the same order as Polyline::segments().
    let count = if p.closed { n } else { n.saturating_sub(1) };
    let mut seg_starts = Vec::with_capacity(count);
    for i in 0..count {
        let (Some(v), Some(w)) = (p.vertices.get(i), p.vertices.get((i + 1) % n.max(1))) else { continue };
        if v.p.near(w.p, EPS) && v.bulge.abs() < 1e-12 {
            continue;
        }
        seg_starts.push(i);
    }
    let mut k = 0usize;
    for i in 0..n {
        if k == index {
            return Some(PolyGrip::Vertex(i));
        }
        k += 1;
        if let Some(start) = seg_starts.get(i) {
            if k == index {
                return Some(PolyGrip::Mid(*start));
            }
            k += 1;
        }
    }
    None
}

/// The shape after dragging grip `index` to `to` (STRETCH mode). `None` when the grip does not
/// exist or the result would be degenerate.
pub fn stretch_grip(kind: &EntityKind, index: usize, to: Vec2) -> Option<EntityKind> {
    if !to.is_finite() {
        return None;
    }
    let grips = kind.grips();
    let from = *grips.get(index)?;
    let delta = to - from;
    let mut k = kind.clone();
    // The further parts of a broken table move on their own.
    if index > 0
        && let EntityKind::Table(t) = &mut k
        && let Some(b) = &mut t.breaks
    {
        let i = index - 1;
        if b.offsets.len() <= i {
            b.offsets.resize(i + 1, Vec2::ZERO);
        }
        if let Some(o) = b.offsets.get_mut(i) {
            *o += delta;
        }
        return Some(k);
    }
    match &mut k {
        EntityKind::Line(l) => match index {
            0 => set_xy(&mut l.a, to),
            1 => return Some(translated(kind, delta)),
            _ => set_xy(&mut l.b, to),
        },
        EntityKind::Point(_) | EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_) | EntityKind::Image(_) | EntityKind::Table(_) => {
            return Some(translated(kind, delta));
        }
        EntityKind::Hatch(_) | EntityKind::Tolerance(_) => return Some(translated(kind, delta)),
        EntityKind::Circle(c) => {
            if index == 0 {
                return Some(translated(kind, delta));
            }
            let r = c.center.xy().dist(to);
            if r <= EPS {
                return None;
            }
            c.radius = r;
        }
        EntityKind::Arc(a) => {
            let g = Arc::new(a.center.xy(), a.radius, a.start, a.end);
            let (mut s, mut m, mut e) = (g.start_point(), g.mid_point(), g.end_point());
            match index {
                0 => s = to,
                1 => m = to,
                2 => e = to,
                _ => return Some(translated(kind, delta)),
            }
            let na = Arc::from_3_points(s, m, e)?;
            return Some(crate::cmd::helpers::arc(&na));
        }
        EntityKind::Ellipse(e) => {
            let c = e.center.xy();
            let mj = e.major.xy();
            let a = mj.len();
            if a <= EPS {
                return None;
            }
            let b = a * e.ratio;
            match index {
                0 => return Some(translated(kind, delta)),
                1 | 3 => {
                    let v = if index == 1 { to - c } else { c - to };
                    let l = v.len();
                    if l <= EPS {
                        return None;
                    }
                    if l >= b {
                        e.major = v.to3(0.0);
                        e.ratio = (b / l).clamp(1e-9, 1.0);
                    } else {
                        e.major = (v.normalized().perp() * b).to3(0.0);
                        e.ratio = (l / b).clamp(1e-9, 1.0);
                        shift_params(e, -std::f64::consts::FRAC_PI_2);
                    }
                }
                _ => {
                    let u = mj / a;
                    let l = (to - c).dot(u.perp()).abs();
                    if l <= EPS {
                        return None;
                    }
                    if l <= a {
                        e.ratio = (l / a).clamp(1e-9, 1.0);
                    } else {
                        e.major = (u.perp() * l).to3(0.0);
                        e.ratio = (a / l).clamp(1e-9, 1.0);
                        shift_params(e, -std::f64::consts::FRAC_PI_2);
                    }
                }
            }
        }
        EntityKind::LwPolyline(p) => {
            let n = p.vertices.len();
            match poly_grip(p, index)? {
                PolyGrip::Vertex(i) => p.vertices.get_mut(i)?.p = to,
                PolyGrip::Mid(i) => {
                    let j = (i + 1) % n.max(1);
                    let (a, b) = (p.vertices.get(i)?.p, p.vertices.get(j)?.p);
                    if p.vertices.get(i)?.bulge.abs() > 1e-12 {
                        // Arc segment: reshape through the dragged midpoint.
                        p.vertices.get_mut(i)?.bulge = crate::cmd::curves::bulge_through(a, to, b);
                    } else {
                        p.vertices.get_mut(i)?.p = a + delta;
                        p.vertices.get_mut(j)?.p = b + delta;
                    }
                }
            }
        }
        EntityKind::Polyline3d(p) => set_xy(p.points.get_mut(index)?, to),
        EntityKind::Spline(sp) => {
            if sp.fit.is_empty() {
                *sp.control.get_mut(index)? = to;
            } else {
                let mut fit = sp.fit.clone();
                *fit.get_mut(index)? = to;
                *sp = sp.refit(&fit, sp.closed);
            }
        }
        // Centre, base radius (and start angle), top radius.
        EntityKind::Helix(h) => match index {
            0 => return Some(translated(kind, delta)),
            1 => {
                let r = h.center.xy().dist(to);
                if r <= EPS {
                    return None;
                }
                h.base_radius = r;
                h.start_angle = h.center.xy().angle_to(to);
            }
            _ => h.top_radius = h.center.xy().dist(to),
        },
        EntityKind::Ray(r) | EntityKind::XLine(r) => {
            if index == 0 {
                return Some(translated(kind, delta));
            }
            let d = (to - r.base.xy()).normalized();
            if d == Vec2::ZERO {
                return None;
            }
            r.dir = d.to3(0.0);
        }
        EntityKind::Insert(i) => {
            if index == 0 {
                return Some(translated(kind, delta));
            }
            let a = i.attribs.get_mut(index - 1)?;
            let ip = a.text.insert.xy() + delta;
            set_xy(&mut a.text.insert, ip);
            if let Some(al) = &mut a.text.align_pt {
                let q = al.xy() + delta;
                set_xy(al, q);
            }
        }
        EntityKind::Dimension(d) => {
            match index {
                0 => set_xy(&mut d.p13, to),
                1 => set_xy(&mut d.p14, to),
                2 => {
                    set_xy(&mut d.defpt, to);
                    if matches!(d.kind, DimKind::Radius | DimKind::Diameter) {
                        d.user_text_pos = false;
                    }
                }
                _ => {
                    set_xy(&mut d.text_mid, to);
                    d.user_text_pos = true;
                }
            }
            d.block = None;
        }
        EntityKind::Leader(l) => set_xy(l.vertices.get_mut(index)?, to),
        EntityKind::MLine(m) => set_xy(&mut m.vertices.get_mut(index)?.p, to),
        EntityKind::MLeader(m) => {
            let mut idx = 0usize;
            let mut done = false;
            'outer: for l in &mut m.leaders {
                for v in l.iter_mut() {
                    if idx == index {
                        set_xy(v, to);
                        done = true;
                        break 'outer;
                    }
                    idx += 1;
                }
            }
            if !done {
                // The landing grip moves the landing and its content.
                let ld = m.landing.xy() + delta;
                set_xy(&mut m.landing, ld);
                if let Some(t) = &mut m.text {
                    let q = t.insert.xy() + delta;
                    set_xy(&mut t.insert, q);
                }
                if let Some(b) = &mut m.block {
                    let q = b.insert.xy() + delta;
                    set_xy(&mut b.insert, q);
                }
            }
        }
        EntityKind::Solid(s) | EntityKind::Trace(s) => set_xy(s.corners.get_mut(index)?, to),
        EntityKind::Face3d(f) => set_xy(f.corners.get_mut(index)?, to),
        EntityKind::Region(_) => return Some(translated(kind, delta)),
        EntityKind::Viewport(v) => {
            let opposite = *grips.get((index + 2) % 4)?;
            let w = (to.x - opposite.x).abs();
            let h = (to.y - opposite.y).abs();
            if w <= EPS || h <= EPS {
                return None;
            }
            let c = to.mid(opposite);
            set_xy(&mut v.center, c);
            v.width = w;
            v.height = h;
        }
        EntityKind::Wipeout(w) => *w.boundary.get_mut(index)? = to,
        EntityKind::Unknown(_) => return None,
    }
    Some(k)
}

fn shift_params(e: &mut cadcraft_doc::Ellipse, by: f64) {
    let full = (cadcraft_geom::ccw_sweep(e.start, e.end) - cadcraft_geom::TAU).abs() < 1e-9;
    if !full {
        e.start = norm_angle(e.start + by);
        e.end = norm_angle(e.end + by);
    }
}

/// A multifunctional grip option besides Stretch (the menu a polyline, line or arc grip shows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GripAction {
    /// Polyline: insert a vertex after this vertex, or on this segment.
    AddVertex,
    /// Polyline: remove this vertex.
    RemoveVertex,
    /// Polyline: make this straight segment an arc through a picked point.
    ToArc,
    /// Polyline: make this arc segment straight.
    ToLine,
    /// Line or arc end: lengthen or shorten the object along itself.
    Lengthen,
    /// Arc midpoint: change the radius, keeping the center and angles.
    Radius,
}

impl GripAction {
    /// The command-line keyword at the hot-grip prompt.
    pub fn keyword(self) -> &'static str {
        match self {
            GripAction::AddVertex => "Add vertex",
            GripAction::RemoveVertex => "Remove vertex",
            GripAction::ToArc => "conVert to arc",
            GripAction::ToLine => "conVert to line",
            GripAction::Lengthen => "Lengthen",
            GripAction::Radius => "Radius",
        }
    }
    /// The grip menu label.
    pub fn label(self) -> &'static str {
        match self {
            GripAction::AddVertex => "Add Vertex",
            GripAction::RemoveVertex => "Remove Vertex",
            GripAction::ToArc => "Convert to Arc",
            GripAction::ToLine => "Convert to Line",
            GripAction::Lengthen => "Lengthen",
            GripAction::Radius => "Radius",
        }
    }
    /// The mode name on the command line ("** ADD VERTEX **").
    pub fn title(self) -> &'static str {
        match self {
            GripAction::AddVertex => "** ADD VERTEX **",
            GripAction::RemoveVertex => "** REMOVE VERTEX **",
            GripAction::ToArc => "** CONVERT TO ARC **",
            GripAction::ToLine => "** CONVERT TO LINE **",
            GripAction::Lengthen => "** LENGTHEN **",
            GripAction::Radius => "** RADIUS **",
        }
    }
    /// Whether the option needs a point (or value); the others act at once.
    pub fn needs_point(self) -> bool {
        !matches!(self, GripAction::RemoveVertex | GripAction::ToLine)
    }
}

/// The multifunctional options of grip `index` of `kind` (empty when it has only Stretch):
/// polyline vertices Add/Remove Vertex, polyline segment midpoints Add Vertex and Convert to
/// Arc/Line, line and arc ends Lengthen, the arc midpoint Radius.
pub fn grip_actions(kind: &EntityKind, index: usize) -> Vec<GripAction> {
    match kind {
        EntityKind::LwPolyline(p) => match poly_grip(p, index) {
            Some(PolyGrip::Vertex(_)) if p.vertices.len() > 2 => vec![GripAction::AddVertex, GripAction::RemoveVertex],
            Some(PolyGrip::Vertex(_)) => vec![GripAction::AddVertex],
            Some(PolyGrip::Mid(i)) => {
                let arc = p.vertices.get(i).is_some_and(|v| v.bulge.abs() > 1e-12);
                vec![GripAction::AddVertex, if arc { GripAction::ToLine } else { GripAction::ToArc }]
            }
            None => Vec::new(),
        },
        EntityKind::Line(_) if index == 0 || index == 2 => vec![GripAction::Lengthen],
        EntityKind::Arc(_) if index == 0 || index == 2 => vec![GripAction::Lengthen],
        EntityKind::Arc(_) if index == 1 => vec![GripAction::Radius],
        _ => Vec::new(),
    }
}

/// The object after the multifunctional `action` on grip `index`, with the picked point `to`
/// (Add Vertex: the new vertex; Convert to Arc: a point on the arc; Lengthen: the new end,
/// projected onto the object; Radius: a point on the new circle) or the typed `value` (Lengthen:
/// the change in length; Radius: the new radius). `None` when the option doesn't apply or the
/// result would be degenerate.
pub fn grip_action(kind: &EntityKind, index: usize, action: GripAction, to: Option<Vec2>, value: Option<f64>) -> Option<EntityKind> {
    if !grip_actions(kind, index).contains(&action) || to.is_some_and(|p| !p.is_finite()) || value.is_some_and(|v| !v.is_finite()) {
        return None;
    }
    let mut k = kind.clone();
    match (&mut k, action) {
        (EntityKind::LwPolyline(p), GripAction::AddVertex) => {
            let to = to?;
            let at = match poly_grip(p, index)? {
                PolyGrip::Vertex(i) | PolyGrip::Mid(i) => i,
            };
            if at >= p.vertices.len() {
                return None;
            }
            let v = p.vertices.get_mut(at)?;
            // The segment is split in two straight ones.
            v.bulge = 0.0;
            let nv = cadcraft_geom::PolyVertex { p: to, bulge: 0.0, ..*v };
            p.vertices.insert(at + 1, nv);
        }
        (EntityKind::LwPolyline(p), GripAction::RemoveVertex) => {
            let PolyGrip::Vertex(i) = poly_grip(p, index)? else { return None };
            if p.vertices.len() <= 2 || i >= p.vertices.len() {
                return None;
            }
            p.vertices.remove(i);
            // The segment into the removed vertex now runs to the next one: straight.
            let n = p.vertices.len();
            let prev = if i == 0 { if p.closed { n - 1 } else { return Some(k) } } else { i - 1 };
            p.vertices.get_mut(prev)?.bulge = 0.0;
        }
        (EntityKind::LwPolyline(p), GripAction::ToArc) => {
            let PolyGrip::Mid(i) = poly_grip(p, index)? else { return None };
            let j = (i + 1) % p.vertices.len().max(1);
            let (a, b) = (p.vertices.get(i)?.p, p.vertices.get(j)?.p);
            let bulge = crate::cmd::curves::bulge_through(a, to?, b);
            if bulge.abs() < 1e-12 || !bulge.is_finite() {
                return None;
            }
            p.vertices.get_mut(i)?.bulge = bulge;
        }
        (EntityKind::LwPolyline(p), GripAction::ToLine) => {
            let PolyGrip::Mid(i) = poly_grip(p, index)? else { return None };
            p.vertices.get_mut(i)?.bulge = 0.0;
        }
        (EntityKind::Line(l), GripAction::Lengthen) => {
            let (fixed, end) = if index == 0 { (l.b.xy(), l.a.xy()) } else { (l.a.xy(), l.b.xy()) };
            let len = fixed.dist(end);
            let dir = (end - fixed).normalized();
            if dir == Vec2::ZERO {
                return None;
            }
            let new_len = match (value, to) {
                (Some(d), _) => len + d,
                (None, Some(p)) => (p - fixed).dot(dir),
                _ => return None,
            };
            if !(new_len > EPS && new_len.is_finite()) {
                return None;
            }
            let q = fixed + dir * new_len;
            set_xy(if index == 0 { &mut l.a } else { &mut l.b }, q);
        }
        (EntityKind::Arc(a), GripAction::Lengthen) => {
            let c = a.center.xy();
            let ang = match (value, to) {
                (Some(d), _) if a.radius > EPS => {
                    let da = d / a.radius;
                    if index == 0 { a.start - da } else { a.end + da }
                }
                (None, Some(p)) if !p.near(c, EPS) => c.angle_to(p),
                _ => return None,
            };
            let ang = norm_angle(ang);
            if index == 0 {
                a.start = ang;
            } else {
                a.end = ang;
            }
            let sweep = cadcraft_geom::ccw_sweep(a.start, a.end);
            if !(sweep > 1e-9 && sweep < cadcraft_geom::TAU - 1e-9) {
                return None;
            }
        }
        (EntityKind::Arc(a), GripAction::Radius) => {
            let r = match (value, to) {
                (Some(r), _) => r,
                (None, Some(p)) => a.center.xy().dist(p),
                _ => return None,
            };
            if !(r > EPS && r.is_finite()) {
                return None;
            }
            a.radius = r;
        }
        _ => return None,
    }
    Some(k)
}

/// The transform for a multi-functional grip mode about `base` with the dragged point `to`.
pub fn mode_matrix(mode: GripMode, base: Vec2, to: Vec2) -> Option<Mat3> {
    match mode {
        GripMode::Stretch => None,
        GripMode::Move => Some(Mat3::translate(to - base)),
        GripMode::Rotate => Some(Mat3::rotate_about(base, base.angle_to(to))),
        GripMode::Scale => {
            let f = base.dist(to);
            (f > EPS && f.is_finite()).then(|| Mat3::scale_about(base, f))
        }
        GripMode::Mirror => (!base.near(to, EPS)).then(|| Mat3::mirror(base, to)),
    }
}

impl Session {
    /// Drag grip `grip_index` of `handle` to `new_point` in `mode` (one undo step).
    /// The base point of the rotate/scale/mirror modes is the grip itself. As in AutoCAD the edit
    /// covers the selection: a stretch drags the grips of other selected objects that sit on the
    /// same point, and the other modes transform every selected object.
    pub fn grip_edit(&mut self, handle: Handle, grip_index: usize, new_point: Vec2, mode: GripMode) -> Result<()> {
        if is_opening_handle(handle) {
            let id = ArchId((handle.0 - ARCH_HANDLE_BASE - OPENING_HANDLE_OFFSET) as u32);
            let d = self.doc()?;
            if let Some(o) = d.arch.opening(id) {
                if mode != GripMode::Stretch {
                    return Err(EngineError::Other("arch openings support only the stretch grip".into()));
                }
                let w = d.arch.wall(o.host).ok_or_else(|| EngineError::Other("no such wall".into()))?;
                let off = cadcraft_arch::opening::project_to_wall(&d.arch, o.host, new_point)
                    .ok_or_else(|| EngineError::Other("could not project point to wall".into()))?;
                return self
                    .execute(
                        "arch.opening.move",
                        &json!({
                            "opening": format!("o{}", id.0),
                            "offset": off,
                        }),
                    )
                    .map(|_| ());
            }
            return Err(EngineError::Other("no such opening".into()));
        }
        if is_arch_handle(handle) {
            let id = ArchId((handle.0 - ARCH_HANDLE_BASE) as u32);
            let d = self.doc()?;
            if mode != GripMode::Stretch {
                return Err(EngineError::Other("arch walls support only the stretch grip".into()));
            }
            let end = if grip_index == 0 { "start" } else { "end" };
            return self
                .execute(
                    "arch.wall.stretch",
                    &json!({
                        "wall": format!("w{}", id.0),
                        "end": end,
                        "to": [new_point.x, new_point.y],
                    }),
                )
                .map(|_| ());
        }
        let to = [new_point.x, new_point.y];
        let hs: Vec<String> = std::iter::once(handle).chain(self.selection().into_iter().filter(|h| *h != handle)).map(|h| h.hex()).collect();
        let r = match mode {
            GripMode::Stretch => self.execute("grip.move", &json!({ "handle": handle.hex(), "handles": hs, "index": grip_index, "to": to })),
            GripMode::Move => {
                self.execute("grip.move", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to, "mode": "move" }))
            }
            GripMode::Rotate => self.execute("grip.rotate", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to })),
            GripMode::Scale => self.execute("grip.scale", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to })),
            GripMode::Mirror => self.execute("grip.mirror", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to })),
        };
        r.map(|_| ())
    }

    /// Grip points of an object (the indices `grip_edit` takes).
    pub fn grips_of(&self, handle: Handle) -> Result<Vec<Vec2>> {
        if is_opening_handle(handle) {
            let id = ArchId((handle.0 - ARCH_HANDLE_BASE - OPENING_HANDLE_OFFSET) as u32);
            let d = self.doc()?;
            let Some(o) = d.arch.opening(id) else {
                return Err(EngineError::Other("no such object".into()));
            };
            let w = d.arch.wall(o.host).ok_or_else(|| EngineError::Other("no such wall".into()))?;
            let dir = (w.end - w.start).normalized();
            let perp = dir.perp();
            let centre = w.start + dir * o.offset;
            let half_w = o.width / 2.0;
            let half_t = w.thickness / 2.0;
            let hinge = centre + perp * half_t - dir * half_w;
            let far = centre + perp * half_t + dir * half_w;
            return Ok(vec![centre, hinge, far]);
        }
        if is_arch_handle(handle) {
            let id = ArchId((handle.0 - ARCH_HANDLE_BASE) as u32);
            let d = self.doc()?;
            return match d.arch.wall(id) {
                Some(w) => Ok(vec![w.start, w.end]),
                None => Err(EngineError::Other("no such object".into())),
            };
        }
        self.doc()?.entity(handle).map(|e| e.kind.grips()).ok_or_else(|| EngineError::Other("no such object".into()))
    }
}

/// Polyline helper kept public for the UI: segment midpoints are grips too.
pub fn polyline_grip_count(p: &LwPolyline) -> usize {
    p.vertices.len() + Polyline { vertices: p.vertices.clone(), closed: p.closed }.segments().len().min(p.vertices.len())
}

#[cfg(test)]
#[path = "grips_tests.rs"]
mod tests;

#[cfg(test)]
mod arch_grips_tests {
    use super::*;
    use crate::Session;
    use cadcraft_doc::{ArchId, Handle, arch_handle, opening_handle};
    use cadcraft_geom::Vec2;
    use serde_json::json;

    /// Build a session with the standard L-shaped wall
    /// (w0: (0,0)-(10,0), w1: (10,0)-(10,10)).
    fn setup() -> Session {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0, 0], [10, 0], [10, 10]]})).unwrap();
        s
    }

    #[test]
    fn arch_wall_grips_are_its_endpoints() {
        let s = setup();
        let grips = s.grips_of(arch_handle(ArchId(0))).unwrap();
        assert_eq!(grips, vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]);
        assert!(s.grips_of(arch_handle(ArchId(99))).is_err());
    }

    #[test]
    fn grip_drag_of_arch_wall_stretches_neighbours() {
        let mut s = setup();
        s.grip_edit(arch_handle(ArchId(0)), 1, Vec2::new(12.0, 0.0), GripMode::Stretch).unwrap();
        let d = s.doc().unwrap();
        let w1 = d.arch.wall(ArchId(1)).unwrap();
        assert_eq!(w1.start, Vec2::new(12.0, 0.0));
    }

    #[test]
    fn arch_wall_grip_drag_is_undoable() {
        let mut s = setup();
        s.grip_edit(arch_handle(ArchId(0)), 1, Vec2::new(12.0, 0.0), GripMode::Stretch).unwrap();
        s.undo().unwrap();
        let d = s.doc().unwrap();
        let w0 = d.arch.wall(ArchId(0)).unwrap();
        let w1 = d.arch.wall(ArchId(1)).unwrap();
        assert_eq!(w0.end, Vec2::new(10.0, 0.0));
        assert_eq!(w1.start, Vec2::new(10.0, 0.0));
    }

    #[test]
    fn normal_entity_grips_still_work() {
        let mut s = setup();
        s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        let d = s.doc().unwrap();
        let h = d.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(_))).map(|e| e.handle).unwrap();
        let grips = s.grips_of(h).unwrap();
        assert!(grips.len() >= 2);
        s.grip_edit(h, 0, Vec2::new(1.0, 1.0), GripMode::Stretch).unwrap();
        assert!(s.grip_edit(arch_handle(ArchId(0)), 0, Vec2::new(1.0, 1.0), GripMode::Move).is_err());
    }

    /// Build a session with one wall and one door on it.
    fn setup_opening() -> Session {
        let mut s = Session::new();
        s.execute("wall", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        s.execute("door", &json!({"wall": "w0", "offset": 5.0, "width": 0.9})).unwrap();
        s
    }

    #[test]
    fn opening_grips_are_on_the_host_wall() {
        let s = setup_opening();
        let d = s.doc().unwrap();
        let o = d.arch.opening(ArchId(0)).unwrap();
        let grips = s.grips_of(opening_handle(ArchId(0))).unwrap();
        assert_eq!(grips.len(), 3);
        // Centre grip sits on the wall centre line at the opening offset.
        assert_eq!(grips[0], Vec2::new(5.0, 0.0));
        // Hinge and far grips are offset by half the wall thickness.
        let w = d.arch.wall(o.host).unwrap();
        let half_t = w.thickness / 2.0;
        assert_eq!(grips[1].y, half_t);
        assert_eq!(grips[2].y, half_t);
    }

    #[test]
    fn grip_drag_moves_an_opening_along_its_wall() {
        let mut s = setup_opening();
        s.grip_edit(opening_handle(ArchId(0)), 0, Vec2::new(7.0, 0.0), GripMode::Stretch).unwrap();
        let d = s.doc().unwrap();
        let o = d.arch.opening(ArchId(0)).unwrap();
        assert_eq!(o.offset, 7.0);
    }

    #[test]
    fn opening_grip_drag_is_undoable() {
        let mut s = setup_opening();
        s.grip_edit(opening_handle(ArchId(0)), 0, Vec2::new(7.0, 0.0), GripMode::Stretch).unwrap();
        s.undo().unwrap();
        let d = s.doc().unwrap();
        let o = d.arch.opening(ArchId(0)).unwrap();
        assert_eq!(o.offset, 5.0);
    }

    #[test]
    fn opening_grip_rejects_the_move_mode() {
        let s = setup_opening();
        let mut s = s;
        assert!(s.grip_edit(opening_handle(ArchId(0)), 0, Vec2::new(7.0, 0.0), GripMode::Move).is_err());
    }

    #[test]
    fn wall_grips_still_work() {
        let mut s = setup_opening();
        let grips = s.grips_of(arch_handle(ArchId(0))).unwrap();
        assert_eq!(grips, vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]);
        s.grip_edit(arch_handle(ArchId(0)), 1, Vec2::new(12.0, 0.0), GripMode::Stretch).unwrap();
        let d = s.doc().unwrap();
        let w = d.arch.wall(ArchId(0)).unwrap();
        assert_eq!(w.end, Vec2::new(12.0, 0.0));
    }
}
