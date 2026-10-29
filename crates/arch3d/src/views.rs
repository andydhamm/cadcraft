//! View generation (phase 7): elevations and sections of the model as 2D linework, poché rings and overall
//! dimensions, all from the one hidden-line / section kernel ([`crate::hlr`], [`crate::section`]).
//!
//! Output is deterministic: lines and rings are sorted with `total_cmp`, so the same model and view always give
//! identical geometry.

use crate::hlr::{Line2, OrthoView, merge_collinear, visible_lines};
use crate::section::{Plane, section_view};
use crate::{Mesh, Scene3d, build};
use cadcraft_doc::ArchModel;
use cadcraft_doc::arch::{ArchView, DimKind, MAX_VIEW_LINES, ViewDim, ViewGeometry, ViewKind, ViewLine, ViewRing};
use std::cmp::Ordering;

/// How far (metres) a dimension line sits outside the extent.
const DIM_OFFSET: f64 = 0.75;

/// An empty geometry for `revision` carrying `error`.
fn failed(revision: u64, error: impl Into<String>) -> ViewGeometry {
    ViewGeometry { revision, error: Some(error.into()), ..Default::default() }
}

/// Plan bounds `[x0, y0, x1, y1]` of a mesh, `None` when it has no vertices.
fn plan_bounds(m: &Mesh) -> Option<[f64; 4]> {
    let mut it = m.positions.iter();
    let f = it.next()?;
    let mut b = [f64::from(f[0]), f64::from(f[1]), f64::from(f[0]), f64::from(f[1])];
    for p in it {
        let (x, y) = (f64::from(p[0]), f64::from(p[1]));
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    Some(b)
}

/// The scene without the meshes outside the view's scope, or (sections) farther than its depth beyond `plane`.
fn restrict(scene: &Scene3d, view: &ArchView, plane: Option<&Plane>) -> Scene3d {
    let scope = view.scope.map(|s| [s[0][0].min(s[1][0]), s[0][1].min(s[1][1]), s[0][0].max(s[1][0]), s[0][1].max(s[1][1])]);
    let depth = view.depth.filter(|d| d.is_finite());
    let keep = |m: &Mesh| {
        if let (Some(s), Some(b)) = (scope, plan_bounds(m)) {
            if b[2] < s[0] || b[0] > s[2] || b[3] < s[1] || b[1] > s[3] {
                return false;
            }
        }
        if let (Some(pl), Some(d)) = (plane, depth) {
            let nearest = m.positions.iter().map(|p| pl.distance([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])])).fold(f64::INFINITY, f64::min);
            if nearest > d {
                return false;
            }
        }
        true
    };
    let meshes = scene.meshes.iter().filter(|m| keep(m)).cloned().collect();
    Scene3d { meshes, ..Scene3d::default() }
}

fn cmp_pt(a: [f64; 2], b: [f64; 2]) -> Ordering {
    a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1]))
}

fn cmp_pts(a: &[[f64; 2]], b: &[[f64; 2]]) -> Ordering {
    for (p, q) in a.iter().zip(b) {
        let o = cmp_pt(*p, *q);
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

/// The geometry of `view` over `scene`, stamped with `revision`. Never panics; a view that cannot be made (bad look
/// direction, no section line, too many lines) gives an empty geometry with `error` set.
pub fn generate(scene: &Scene3d, view: &ArchView, revision: u64) -> ViewGeometry {
    let (lines, rings): (Vec<Line2>, Vec<ViewRing>) = match view.kind {
        ViewKind::Elevation => {
            let Some(ov) = OrthoView::elevation(view.look) else { return failed(revision, "the look direction is not usable") };
            let sc = restrict(scene, view, None);
            (merge_collinear(visible_lines(&sc, &ov)), Vec::new())
        }
        ViewKind::Section => {
            let Some(cut) = view.cut else { return failed(revision, "a section needs a cut line") };
            let (dx, dy) = (cut[1][0] - cut[0][0], cut[1][1] - cut[0][1]);
            let len = dx.hypot(dy);
            if !len.is_finite() || len < 1e-9 {
                return failed(revision, "the cut line has no length");
            }
            // The cut line's normal on the side the viewer looks into.
            let n = [-dy / len, dx / len];
            let k = view.look[0] * n[0] + view.look[1] * n[1];
            if !k.is_finite() || k == 0.0 {
                return failed(revision, "the viewer looks along the cut line");
            }
            let s = k.signum();
            let Some(plane) = Plane::new([cut[0][0], cut[0][1], 0.0], [n[0] * s, n[1] * s, 0.0]) else {
                return failed(revision, "the cut plane is not usable");
            };
            let sc = restrict(scene, view, Some(&plane));
            let Some(sv) = section_view(&sc, &plane, [0.0, 0.0, 1.0]) else { return failed(revision, "the section view is not usable") };
            let mut lines = sv.cut.lines;
            lines.extend(sv.beyond);
            let rings =
                sv.cut.rings.into_iter().map(|r| ViewRing { kind: r.kind, object: r.object, material: r.material, points: r.points }).collect();
            (lines, rings)
        }
    };

    let points: usize = rings.iter().map(|r| r.points.len()).sum();
    if lines.len().saturating_add(points) > MAX_VIEW_LINES {
        return failed(revision, format!("the view has more than {MAX_VIEW_LINES} lines"));
    }

    let mut lines: Vec<ViewLine> = lines.into_iter().map(|l| ViewLine { a: l.a, b: l.b, kind: l.kind, object: l.object }).collect();
    lines.sort_by(|x, y| x.kind.cmp(&y.kind).then(x.object.cmp(&y.object)).then_with(|| cmp_pt(x.a, y.a)).then_with(|| cmp_pt(x.b, y.b)));
    let mut rings = rings;
    rings.sort_by(|x, y| x.kind.cmp(&y.kind).then(x.object.cmp(&y.object)).then_with(|| cmp_pts(&x.points, &y.points)));

    let mut ext: Option<[f64; 4]> = None;
    let mut grow = |p: [f64; 2]| {
        ext = Some(match ext {
            Some(e) => [e[0].min(p[0]), e[1].min(p[1]), e[2].max(p[0]), e[3].max(p[1])],
            None => [p[0], p[1], p[0], p[1]],
        });
    };
    for l in &lines {
        grow(l.a);
        grow(l.b);
    }
    for r in &rings {
        r.points.iter().for_each(|p| grow(*p));
    }

    let mut dims = Vec::new();
    if let (true, Some([u0, v0, u1, v1])) = (view.dimensions, ext) {
        let mm = |v: f64| format!("{:.0}", v * 1000.0);
        dims.push(ViewDim {
            kind: DimKind::Width,
            a: [u0, v0],
            b: [u1, v0],
            at: [(u0 + u1) / 2.0, v0 - DIM_OFFSET],
            value: u1 - u0,
            text: mm(u1 - u0),
        });
        dims.push(ViewDim {
            kind: DimKind::Height,
            a: [u0, v0],
            b: [u0, v1],
            at: [u0 - DIM_OFFSET, (v0 + v1) / 2.0],
            value: v1 - v0,
            text: mm(v1 - v0),
        });
    }

    ViewGeometry { revision, extent: ext.unwrap_or_default(), lines, rings, dims, error: None }
}

/// Regenerate every view of `model` whose geometry is missing or not for `revision`; returns how many. The scene is
/// built once. `arch_revision` is not touched (it lives on the drawing, not the model).
pub fn refresh(model: &mut ArchModel, revision: u64) -> usize {
    if !model.views.iter().any(|v| v.is_stale(revision)) {
        return 0;
    }
    let scene = build(model);
    let mut n = 0;
    for v in &mut model.views {
        if v.is_stale(revision) {
            v.generated = Some(generate(&scene, v, revision));
            n += 1;
        }
    }
    n
}
