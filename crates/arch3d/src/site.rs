//! The site in 3D (phase 9): the terrain solid, draped roads, driveways and sidewalks, and the
//! spans of retaining walls. Plants are in [`crate::plant`].
//!
//! Materials: the terrain solid is `"terrain"`; a path is `"path-{kind}-{material}"` (e.g.
//! `"path-driveway-concrete"`), its mesh `object` being the modifier id; plants are
//! `"plant-crown"` / `"plant-trunk"` with the plant id. `probe::part_of` maps them to the parts
//! `terrain`, `path` and `plant`, and `io::glb` names their nodes `terrain`, `{kind}-tm{id}` and
//! `plant-pl{id}` / `plant-pl{id}-trunk`.

use std::collections::HashMap;

use crate::Mesh;
use cadcraft_arch::terrain::mesh::{SiteMesh, site_mesh};
use cadcraft_arch::terrain::surface::SiteSurface;
use cadcraft_doc::arch::{ArchId, ArchModel, MAX_RING_POINTS, ModifierKind, TERRAIN_SKIRT, Wall};
use cadcraft_geom::{Vec2, Vec3, point_in_polygon};

/// Everything the site adds to a scene.
#[derive(Clone, Debug, Default)]
pub struct SiteMeshes {
    pub meshes: Vec<Mesh>,
    /// 1 when the terrain solid was built.
    pub terrain: usize,
    pub paths: usize,
    pub plants: usize,
}

/// Whether a mesh material belongs to the site (never reused by `build_dirty`).
pub fn is_site_material(material: &str) -> bool {
    material == "terrain" || material.starts_with("path-") || material.starts_with("plant-")
}

/// The site's meshes: the terrain solid and its paths (`p9-arch3d-terrain-solid`) plus the
/// plants (`crate::plant::plant_meshes`).
pub fn site_meshes(model: &ArchModel) -> SiteMeshes {
    let plants = crate::plant::plant_meshes(model);
    // A tree is a crown and a trunk: count the plants, not their meshes.
    let n = plants.iter().map(|m| m.object).collect::<std::collections::BTreeSet<_>>().len();
    let mut out = SiteMeshes { meshes: Vec::new(), terrain: 0, paths: 0, plants: n };
    if let Some((terrain, paths)) = terrain_and_paths(model) {
        out.terrain = 1;
        out.paths = paths.len();
        out.meshes.push(terrain);
        out.meshes.extend(paths);
    }
    out.meshes.extend(plants);
    out
}

/// Two levels closer than this at one vertex are the same level.
const LEVEL_EPS: f64 = 1e-7;
/// Corner elevations differing by more than this on an edge make a step.
const STEP_EPS: f64 = 1e-9;

fn terrain_and_paths(model: &ArchModel) -> Option<(Mesh, Vec<Mesh>)> {
    let terrain_data = model.terrain.as_ref()?;
    let surf = SiteSurface::new(model).ok()?;
    let mesh = site_mesh(&surf).ok()?;
    let terrain = terrain_solid(&mesh)?;
    let mut paths = Vec::new();
    for m in &terrain_data.modifiers {
        if let ModifierKind::Path { kind, polygon, thickness, material } = &m.kind
            && let Some(p) = path_mesh(&mesh, polygon, *thickness, &format!("path-{}-{}", kind.name(), material), m.id)
        {
            paths.push(p);
        }
    }
    Some((terrain, paths))
}

/// The distinct elevations present at each mesh vertex (sorted, merged within `LEVEL_EPS`), so
/// walls can be split at them and every vertical segment is shared by exactly two faces.
struct Levels(Vec<Vec<f64>>);

impl Levels {
    fn new(vertices: usize, samples: impl Iterator<Item = (usize, f64)>) -> Levels {
        let mut per: Vec<Vec<f64>> = vec![Vec::new(); vertices];
        for (v, z) in samples {
            if let Some(l) = per.get_mut(v) {
                l.push(z);
            }
        }
        for l in &mut per {
            l.sort_by(|a, b| a.total_cmp(b));
            let mut merged: Vec<f64> = Vec::with_capacity(l.len());
            for &z in l.iter() {
                if merged.last().is_none_or(|&m| z - m > LEVEL_EPS) {
                    merged.push(z);
                }
            }
            *l = merged;
        }
        Levels(per)
    }

    /// The canonical level at vertex `v` nearest `z`.
    fn snap(&self, v: usize, z: f64) -> f64 {
        let Some(l) = self.0.get(v) else { return z };
        l.iter().copied().min_by(|a, b| (a - z).abs().total_cmp(&(b - z).abs())).unwrap_or(z)
    }

    /// The levels at `v` from `from` to `to` inclusive, in that order.
    fn chain(&self, v: usize, from: f64, to: f64) -> Vec<f64> {
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        let mut c: Vec<f64> =
            self.0.get(v).map(|l| l.iter().copied().filter(|z| *z >= lo - LEVEL_EPS && *z <= hi + LEVEL_EPS).collect()).unwrap_or_default();
        if c.is_empty() {
            c = vec![lo, hi];
        }
        if from > to {
            c.reverse();
        }
        c
    }
}

/// Append one flat-shaded triangle; with `hint` the winding is flipped to face that way.
fn tri(mesh: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, hint: Option<Vec3>) {
    let n = (b - a).cross(c - a);
    let (b, c, n) = match hint {
        Some(h) if n.dot(h) < 0.0 => (c, b, Vec3::new(-n.x, -n.y, -n.z)),
        _ => (b, c, n),
    };
    let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    let n = if len > 1e-30 { Vec3::new(n.x / len, n.y / len, n.z / len) } else { Vec3::new(0.0, 0.0, 1.0) };
    let base = mesh.positions.len() as u32;
    for p in [a, b, c] {
        mesh.positions.push([p.x as f32, p.y as f32, p.z as f32]);
        mesh.normals.push([n.x as f32, n.y as f32, n.z as f32]);
        let uv = if n.z.abs() > 0.5 {
            [p.x, p.y]
        } else if n.x.abs() > n.y.abs() {
            [p.y, p.z]
        } else {
            [p.x, p.z]
        };
        mesh.uvs.push([uv[0] as f32, uv[1] as f32]);
    }
    mesh.indices.extend([base, base + 1, base + 2]);
}

/// A vertical wall between two vertices, spanning `za` at `a` and `zb` at `b`, split at every
/// level of each vertex and facing `hint` (a horizontal direction).
fn wall(mesh: &mut Mesh, lv: &Levels, (a, pa): (usize, Vec2), (b, pb): (usize, Vec2), za: (f64, f64), zb: (f64, f64), hint: Vec2) {
    let left = lv.chain(a, za.0, za.1);
    let right = lv.chain(b, zb.0, zb.1);
    let (p, q) = (left.len().saturating_sub(1), right.len().saturating_sub(1));
    if p == 0 && q == 0 {
        return;
    }
    let h = Some(Vec3::new(hint.x, hint.y, 0.0));
    let pt = |v: Vec2, z: f64| Vec3::new(v.x, v.y, z);
    let (mut i, mut j) = (0usize, 0usize);
    while i < p || j < q {
        let (Some(&l0), Some(&r0)) = (left.get(i), right.get(j)) else { return };
        if i < p && (j == q || (i + 1) * q <= (j + 1) * p) {
            let Some(&l1) = left.get(i + 1) else { return };
            tri(mesh, pt(pa, l0), pt(pb, r0), pt(pa, l1), h);
            i += 1;
        } else {
            let Some(&r1) = right.get(j + 1) else { return };
            tri(mesh, pt(pa, l0), pt(pb, r0), pt(pb, r1), h);
            j += 1;
        }
    }
}

type Tri = ([usize; 3], [f64; 3]);

/// Triangle `t` of the site mesh: corner indices and elevations; `None` when malformed.
fn corners(m: &SiteMesh, t: usize) -> Option<Tri> {
    let idx = *m.triangles.get(t)?;
    let z = *m.z.get(t)?;
    if idx.iter().any(|&i| xy(m, i).is_none()) || z.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some((idx, z))
}

fn xy(m: &SiteMesh, i: usize) -> Option<Vec2> {
    m.xy.get(i).copied().filter(|p| p.is_finite())
}

/// Right-hand (outward for a counter-clockwise ring) normal of `a -> b`.
fn right_of(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(b.y - a.y, a.x - b.x)
}

/// Directed edge `(a, b)` -> `(triangle, corner)`; `None` when an edge is used twice the same way.
fn directed_edges(tris: &[Tri]) -> Option<HashMap<(usize, usize), (usize, usize)>> {
    let mut directed = HashMap::new();
    for (t, (idx, _)) in tris.iter().enumerate() {
        for k in 0..3 {
            let a = *idx.get(k)?;
            let b = *idx.get((k + 1) % 3)?;
            if directed.insert((a, b), (t, k)).is_some() {
                return None;
            }
        }
    }
    Some(directed)
}

/// The terrain solid: top (with step walls), skirt and flat bottom.
fn terrain_solid(m: &SiteMesh) -> Option<Mesh> {
    if m.triangles.is_empty() || m.triangles.len() != m.z.len() {
        return None;
    }
    let z_b = m.lowest()? - TERRAIN_SKIRT;
    let tris: Vec<Tri> = (0..m.triangles.len()).map(|t| corners(m, t)).collect::<Option<_>>()?;
    let lv =
        Levels::new(m.xy.len(), tris.iter().flat_map(|(i, z)| i.iter().copied().zip(z.iter().copied())).chain((0..m.xy.len()).map(|v| (v, z_b))));
    let mut mesh = Mesh::new("terrain", ArchId(0));
    let directed = directed_edges(&tris)?;
    let mut boundary: HashMap<usize, usize> = HashMap::new();
    for (t, (idx, z)) in tris.iter().enumerate() {
        let mut top = [Vec3::default(); 3];
        for k in 0..3 {
            let v = *idx.get(k)?;
            let p = xy(m, v)?;
            *top.get_mut(k)? = Vec3::new(p.x, p.y, lv.snap(v, *z.get(k)?));
        }
        tri(&mut mesh, *top.first()?, *top.get(1)?, *top.get(2)?, Some(Vec3::new(0.0, 0.0, 1.0)));
        for k in 0..3 {
            let (a, b) = (*idx.get(k)?, *idx.get((k + 1) % 3)?);
            let (pa3, pb3) = (*top.get(k)?, *top.get((k + 1) % 3)?);
            let (pa, pb) = (Vec2::new(pa3.x, pa3.y), Vec2::new(pb3.x, pb3.y));
            match directed.get(&(b, a)) {
                None => {
                    if boundary.insert(a, b).is_some() {
                        return None;
                    }
                    wall(&mut mesh, &lv, (a, pa), (b, pb), (pa3.z, z_b), (pb3.z, z_b), right_of(pa, pb));
                }
                Some(&(u, m2)) if u > t => {
                    let (_, uz) = tris.get(u)?;
                    let zb2 = lv.snap(b, *uz.get(m2)?);
                    let za2 = lv.snap(a, *uz.get((m2 + 1) % 3)?);
                    if (pa3.z - za2).abs() > STEP_EPS || (pb3.z - zb2).abs() > STEP_EPS {
                        let toward_other = if (pa3.z - za2) + (pb3.z - zb2) > 0.0 { right_of(pa, pb) } else { right_of(pb, pa) };
                        wall(&mut mesh, &lv, (a, pa), (b, pb), (pa3.z, za2), (pb3.z, zb2), toward_other);
                    }
                }
                Some(_) => {}
            }
        }
    }
    // Bottom: the boundary loop at z_b, facing down.
    let first = *boundary.keys().min()?;
    let mut ring_ix = vec![first];
    let mut cur = *boundary.get(&first)?;
    while cur != first {
        if ring_ix.len() > boundary.len() {
            return None;
        }
        ring_ix.push(cur);
        cur = *boundary.get(&cur)?;
    }
    if ring_ix.len() != boundary.len() {
        return None;
    }
    let ring: Vec<Vec2> = ring_ix.iter().map(|&i| xy(m, i)).collect::<Option<_>>()?;
    let bottom = crate::mesh::triangulate_simple(&ring);
    if bottom.is_empty() {
        return None;
    }
    let down = Some(Vec3::new(0.0, 0.0, -1.0));
    for t in bottom {
        let p = |k: usize| -> Option<Vec3> {
            let r = ring.get(*t.get(k)? as usize)?;
            Some(Vec3::new(r.x, r.y, z_b))
        };
        tri(&mut mesh, p(0)?, p(1)?, p(2)?, down);
    }
    Some(mesh)
}

/// One path: the surface triangles inside `polygon`, lifted by `thickness`, with sides and an
/// underside on the surface.
fn path_mesh(m: &SiteMesh, polygon: &[Vec2], thickness: f64, material: &str, id: u32) -> Option<Mesh> {
    if !thickness.is_finite() || thickness <= 0.0 || polygon.len() < 3 || polygon.len() > MAX_RING_POINTS {
        return None;
    }
    if polygon.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let mut patch: Vec<Tri> = Vec::new();
    for t in 0..m.triangles.len() {
        let (idx, z) = corners(m, t)?;
        let (a, b, c) = (xy(m, *idx.first()?)?, xy(m, *idx.get(1)?)?, xy(m, *idx.get(2)?)?);
        let centroid = Vec2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
        if point_in_polygon(polygon, centroid) {
            patch.push((idx, z));
        }
    }
    if patch.is_empty() {
        return None;
    }
    let lv = Levels::new(
        m.xy.len(),
        patch.iter().flat_map(|(i, z)| i.iter().copied().zip(z.iter().copied())).flat_map(|(v, z)| [(v, z), (v, z + thickness)]),
    );
    let directed = directed_edges(&patch)?;
    let mut mesh = Mesh::new(material, ArchId(id));
    let up = Some(Vec3::new(0.0, 0.0, 1.0));
    let down = Some(Vec3::new(0.0, 0.0, -1.0));
    for (t, (idx, z)) in patch.iter().enumerate() {
        let mut lo = [Vec3::default(); 3];
        let mut top = [Vec3::default(); 3];
        for k in 0..3 {
            let v = *idx.get(k)?;
            let p = xy(m, v)?;
            let base = lv.snap(v, *z.get(k)?);
            *lo.get_mut(k)? = Vec3::new(p.x, p.y, base);
            *top.get_mut(k)? = Vec3::new(p.x, p.y, lv.snap(v, base + thickness));
        }
        tri(&mut mesh, *top.first()?, *top.get(1)?, *top.get(2)?, up);
        tri(&mut mesh, *lo.first()?, *lo.get(1)?, *lo.get(2)?, down);
        for k in 0..3 {
            let (a, b) = (*idx.get(k)?, *idx.get((k + 1) % 3)?);
            let (la, lb) = (*lo.get(k)?, *lo.get((k + 1) % 3)?);
            let (ta, tb) = (*top.get(k)?, *top.get((k + 1) % 3)?);
            let (pa, pb) = (Vec2::new(la.x, la.y), Vec2::new(lb.x, lb.y));
            match directed.get(&(b, a)) {
                None => wall(&mut mesh, &lv, (a, pa), (b, pb), (la.z, ta.z), (lb.z, tb.z), right_of(pa, pb)),
                Some(&(u, m2)) if u > t => {
                    let (_, uz) = patch.get(u)?;
                    let zb2 = lv.snap(b, *uz.get(m2)?);
                    let za2 = lv.snap(a, *uz.get((m2 + 1) % 3)?);
                    if (la.z - za2).abs() > STEP_EPS || (lb.z - zb2).abs() > STEP_EPS {
                        let (to_lower, to_higher) = if (la.z - za2) + (lb.z - zb2) > 0.0 {
                            (right_of(pa, pb), right_of(pb, pa))
                        } else {
                            (right_of(pb, pa), right_of(pa, pb))
                        };
                        let (ta2, tb2) = (lv.snap(a, za2 + thickness), lv.snap(b, zb2 + thickness));
                        wall(&mut mesh, &lv, (a, pa), (b, pb), (ta.z, ta2), (tb.z, tb2), to_lower);
                        wall(&mut mesh, &lv, (a, pa), (b, pb), (la.z, za2), (lb.z, zb2), to_higher);
                    }
                }
                Some(_) => {}
            }
        }
    }
    Some(mesh)
}

/// `(base, top)` of a retaining wall from the ground on each side, or `None` (then the wall
/// keeps its level span).
pub fn retaining_span(model: &ArchModel, w: &Wall) -> Option<(f64, f64)> {
    cadcraft_arch::terrain::retaining::retaining_span(model, w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene3d;
    use cadcraft_doc::arch::{PathKind, Terrain, TerrainModifier};

    fn site() -> Terrain {
        Terrain {
            boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, 30.0), Vec2::new(0.0, 30.0)],
            points: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 2.0), Vec3::new(40.0, 30.0, 2.0), Vec3::new(0.0, 30.0, 0.0)],
            pad: false,
            ..Default::default()
        }
    }

    fn model_with(modifiers: Vec<ModifierKind>) -> ArchModel {
        let mut t = site();
        for (i, kind) in modifiers.into_iter().enumerate() {
            t.modifiers.push(TerrainModifier { id: i as u32, kind });
        }
        ArchModel { terrain: Some(t), ..Default::default() }
    }

    fn scene(model: &ArchModel) -> Scene3d {
        Scene3d { meshes: site_meshes(model).meshes, ..Default::default() }
    }

    type Key = Vec<i64>;

    /// Every undirected edge (welded at 1e-6) is used exactly twice.
    fn closed(mesh: &Mesh) -> bool {
        let key = |i: u32| -> Key {
            let p = mesh.positions.get(i as usize).copied().unwrap_or([0.0; 3]);
            p.iter().map(|v| (f64::from(*v) * 1e6).round() as i64).collect()
        };
        let mut count: HashMap<(Key, Key), u32> = HashMap::new();
        for t in mesh.indices.chunks(3) {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (key(t[i]), key(t[j]));
                let k = if a <= b { (a, b) } else { (b, a) };
                *count.entry(k).or_insert(0) += 1;
            }
        }
        !count.is_empty() && count.values().all(|&c| c == 2)
    }

    fn part(s: &Scene3d, x: f64, y: f64, z: f64) -> Option<String> {
        s.probe(Vec3::new(x, y, z)).map(|h| h.part)
    }

    #[test]
    fn terrain_solid_is_closed() {
        let model = model_with(Vec::new());
        let out = site_meshes(&model);
        assert_eq!((out.terrain, out.paths), (1, 0));
        let terrain = &out.meshes[0];
        assert_eq!(terrain.material, "terrain");
        assert!(closed(terrain));
        let s = scene(&model);
        assert_eq!(part(&s, 25.0, 12.0, 1.2).as_deref(), Some("terrain"));
        assert_eq!(part(&s, 25.0, 12.0, 1.3), None);
    }

    #[test]
    fn terrain_solid_skirt_reaches_one_metre_below() {
        let s = scene(&model_with(Vec::new()));
        assert_eq!(part(&s, 5.0, 5.0, -0.95).as_deref(), Some("terrain"));
        assert_eq!(part(&s, 5.0, 5.0, -1.05), None);
    }

    #[test]
    fn terrain_solid_closes_a_step() {
        let square = vec![Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0), Vec2::new(20.0, 20.0), Vec2::new(10.0, 20.0)];
        let model = model_with(vec![ModifierKind::Flat { polygon: square, elevation: 3.0, margin: 0.0 }]);
        let out = site_meshes(&model);
        assert_eq!(out.terrain, 1);
        assert!(closed(&out.meshes[0]));
        let s = scene(&model);
        assert!(part(&s, 15.0, 15.0, 2.9).is_some());
        assert_eq!(part(&s, 15.0, 15.0, 3.1), None);
    }

    #[test]
    fn paths_drape_on_the_surface_with_their_thickness() {
        let poly = vec![Vec2::new(18.5, 0.0), Vec2::new(21.5, 0.0), Vec2::new(21.5, 10.0), Vec2::new(18.5, 10.0)];
        let model = model_with(vec![ModifierKind::Path { kind: PathKind::Driveway, polygon: poly, thickness: 0.1, material: "concrete".into() }]);
        let out = site_meshes(&model);
        assert_eq!((out.terrain, out.paths), (1, 1));
        let path = &out.meshes[1];
        assert_eq!(path.material, "path-driveway-concrete");
        assert_eq!(path.object, ArchId(0));
        assert!(closed(path));
        let s = scene(&model);
        assert_eq!(part(&s, 20.0, 5.0, 1.05).as_deref(), Some("path"));
        assert_eq!(part(&s, 20.0, 5.0, 1.15), None);
    }

    #[test]
    fn no_terrain_builds_no_site_meshes() {
        let out = site_meshes(&ArchModel::default());
        assert!(out.meshes.is_empty());
        assert_eq!((out.terrain, out.paths, out.plants), (0, 0, 0));
    }

    #[test]
    fn plants_count_plants_not_their_meshes() {
        // A tree is a crown and a trunk: three trees are three plants (check 07's build3d `plants`).
        let plant = |id: u32, x: f64| cadcraft_doc::arch::Plant {
            id: ArchId(id),
            species: "Colorado Blue Spruce".into(),
            position: Vec2::new(x, 10.0),
            age: 15.0,
            ..Default::default()
        };
        let model = ArchModel { plants: vec![plant(0, 10.0), plant(1, 20.0), plant(2, 30.0)], ..Default::default() };
        let out = site_meshes(&model);
        assert_eq!(out.meshes.len(), 6, "crown and trunk each");
        assert_eq!(out.plants, 3);
    }
}
