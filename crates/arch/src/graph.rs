//! Wall endpoint graph: clusters wall ends that coincide, finds neighbours,
//! connected components, collinear pairs, and closed loops.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use cadcraft_doc::{ArchId, ArchModel};
use cadcraft_geom::Vec2;

use crate::join::{ENDPOINT_TOL, End, MAX_WALLS};
use crate::wall::wall_polygon;

/// Graph of wall endpoints. Walls are nodes; ends that coincide (within
/// `ENDPOINT_TOL`) share a cluster.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallGraph {
    nodes: Vec<(ArchId, Vec2, Vec2)>,
    /// `(start, end)` of each node by id (the first node wins for a duplicated id).
    points: HashMap<ArchId, (Vec2, Vec2)>,
    clusters: Vec<Vec<(ArchId, End)>>,
    end_cluster: HashMap<(ArchId, End), usize>,
}

/// The grid cell of `p` for a cell size of `ENDPOINT_TOL`. Huge coordinates saturate into
/// the same cell, which only costs extra distance checks, never a false merge.
fn cell(p: Vec2) -> (i64, i64) {
    ((p.x / ENDPOINT_TOL).floor() as i64, (p.y / ENDPOINT_TOL).floor() as i64)
}

/// Union-find root of `i`, with path halving.
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while let Some(&p) = parent.get(i) {
        if p == i {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = gp;
        }
        i = gp;
    }
    i
}

impl WallGraph {
    /// Build from a model. Deterministic: cluster order follows wall order, and
    /// cluster members are ordered by (ArchId, End).
    ///
    /// Ends within `ENDPOINT_TOL` of each other (transitively) share a cluster. A grid of
    /// `ENDPOINT_TOL`-sized cells means each end is only compared with the ends in its own
    /// and the eight neighbouring cells, so building is linear in the number of walls.
    pub fn build(model: &ArchModel) -> WallGraph {
        let mut graph = WallGraph::default();

        // Collect valid walls and their endpoints.
        let mut ends: Vec<(ArchId, End, Vec2)> = Vec::new();
        for entry in &model.walls {
            if graph.nodes.len() >= MAX_WALLS {
                break;
            }
            if wall_polygon(entry).is_none() {
                continue;
            }
            let id = entry.id;
            graph.nodes.push((id, entry.start, entry.end));
            graph.points.entry(id).or_insert((entry.start, entry.end));
            ends.push((id, End::Start, entry.start));
            ends.push((id, End::End, entry.end));
        }

        // Single-linkage clustering through the cell grid.
        let mut parent: Vec<usize> = (0..ends.len()).collect();
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, &(_, _, p)) in ends.iter().enumerate() {
            let (cx, cy) = cell(p);
            for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    let Some(members) = grid.get(&(cx.saturating_add(dx), cy.saturating_add(dy))) else {
                        continue;
                    };
                    for &j in members {
                        let Some(&(_, _, q)) = ends.get(j) else {
                            continue;
                        };
                        let (dxp, dyp) = (p.x - q.x, p.y - q.y);
                        if dxp * dxp + dyp * dyp <= ENDPOINT_TOL * ENDPOINT_TOL {
                            let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                            if ri != rj {
                                // Keep the earlier end as the root so cluster order follows wall order.
                                let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                                if let Some(slot) = parent.get_mut(hi) {
                                    *slot = lo;
                                }
                            }
                        }
                    }
                }
            }
            grid.entry((cx, cy)).or_default().push(i);
        }

        // Number the clusters in order of their first member.
        let mut cluster_of_root: HashMap<usize, usize> = HashMap::new();
        for (i, &(id, end, _)) in ends.iter().enumerate() {
            let r = root(&mut parent, i);
            let idx = *cluster_of_root.entry(r).or_insert_with(|| {
                graph.clusters.push(Vec::new());
                graph.clusters.len() - 1
            });
            if let Some(c) = graph.clusters.get_mut(idx) {
                c.push((id, end));
            }
            graph.end_cluster.insert((id, end), idx);
        }

        // Sort cluster members for determinism.
        for cluster in &mut graph.clusters {
            cluster.sort_by_key(|&(id, end)| (id, end_rank(end)));
        }

        graph
    }

    fn endpoint_point(&self, id: ArchId, end: End) -> Vec2 {
        match (self.points.get(&id), end) {
            (Some(&(start, _)), End::Start) => start,
            (Some(&(_, end_pt)), End::End) => end_pt,
            (None, _) => Vec2::ZERO,
        }
    }

    /// The wall ends sharing the cluster of `(id, end)`, excluding `(id, end)` itself.
    pub fn neighbours(&self, id: ArchId, end: End) -> Vec<(ArchId, End)> {
        let cluster_idx = match self.end_cluster.get(&(id, end)) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };
        self.cluster(cluster_idx).iter().filter(|(nid, nend)| *nid != id || *nend != end).cloned().collect()
    }

    /// All wall ends whose point is within `ENDPOINT_TOL` of `p` (empty for non-finite `p`).
    pub fn ends_at(&self, p: Vec2) -> Vec<(ArchId, End)> {
        if !p.is_finite() {
            return Vec::new();
        }
        let mut result = Vec::new();
        for (id, end) in self.end_cluster.keys() {
            let point = self.endpoint_point(*id, *end);
            let dx = point.x - p.x;
            let dy = point.y - p.y;
            if dx * dx + dy * dy <= ENDPOINT_TOL * ENDPOINT_TOL {
                result.push((*id, *end));
            }
        }
        result.sort_by_key(|&(id, end)| (id, end_rank(end)));
        result
    }

    /// Connected components (walls joined by shared endpoint clusters), each sorted by
    /// ArchId; components themselves sorted by their smallest ArchId.
    pub fn components(&self) -> Vec<Vec<ArchId>> {
        if self.nodes.is_empty() {
            return Vec::new();
        }

        let mut adj: BTreeMap<ArchId, BTreeSet<ArchId>> = BTreeMap::new();
        for (id, _, _) in &self.nodes {
            adj.insert(*id, BTreeSet::new());
        }
        for (id, _, _) in &self.nodes {
            let start_neighbours = self.neighbours(*id, End::Start);
            let end_neighbours = self.neighbours(*id, End::End);
            if let Some(set) = adj.get_mut(id) {
                for (nid, _) in start_neighbours {
                    set.insert(nid);
                }
                for (nid, _) in end_neighbours {
                    set.insert(nid);
                }
            }
        }

        let mut visited = BTreeSet::new();
        let mut components = Vec::new();
        for id in adj.keys() {
            if visited.contains(id) {
                continue;
            }
            let mut component = Vec::new();
            let mut queue = vec![*id];
            visited.insert(*id);
            while let Some(current) = queue.pop() {
                component.push(current);
                if let Some(neighbours) = adj.get(&current) {
                    for neighbour in neighbours {
                        if !visited.contains(neighbour) {
                            visited.insert(*neighbour);
                            queue.push(*neighbour);
                        }
                    }
                }
            }
            component.sort();
            components.push(component);
        }
        components.sort_by_key(|c| c.first().copied());
        components
    }

    /// Pairs of walls whose centre lines are collinear within `ENDPOINT_TOL` and whose
    /// unit directions are parallel (|cross| < 1e-9). Each pair once, `(lo, hi)` by ArchId.
    pub fn collinear_pairs(&self) -> Vec<(ArchId, ArchId)> {
        let unit = |s: Vec2, e: Vec2| (e - s).normalized();
        let mut pairs = Vec::new();
        for (i, &(id_i, start_i, end_i)) in self.nodes.iter().enumerate() {
            let dir_i = unit(start_i, end_i);
            for &(id_j, start_j, end_j) in self.nodes.iter().skip(i + 1) {
                let dir_j = unit(start_j, end_j);
                if dir_i.cross(dir_j).abs() >= 1e-9 {
                    continue;
                }
                if (start_j - start_i).cross(dir_i).abs() <= ENDPOINT_TOL {
                    pairs.push((id_i.min(id_j), id_i.max(id_j)));
                }
            }
        }
        pairs
    }

    /// True when every wall in the graph has both ends in a cluster of size >= 2 and the
    /// graph is one component where every wall has exactly two distinct neighbour walls
    /// through its two ends (a simple closed ring of walls).
    pub fn is_closed_loop(&self) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        for (id, _, _) in &self.nodes {
            let start_cluster = match self.end_cluster.get(&(*id, End::Start)) {
                Some(idx) => *idx,
                None => return false,
            };
            let end_cluster = match self.end_cluster.get(&(*id, End::End)) {
                Some(idx) => *idx,
                None => return false,
            };
            if self.cluster(start_cluster).len() < 2 || self.cluster(end_cluster).len() < 2 {
                return false;
            }
        }
        let comps = self.components();
        if comps.len() != 1 {
            return false;
        }
        for (id, _, _) in &self.nodes {
            let start_neighbours = self.neighbours(*id, End::Start);
            let end_neighbours = self.neighbours(*id, End::End);
            let mut all_neighbours = BTreeSet::new();
            for (nid, _) in start_neighbours {
                all_neighbours.insert(nid);
            }
            for (nid, _) in end_neighbours {
                all_neighbours.insert(nid);
            }
            if all_neighbours.len() != 2 {
                return false;
            }
        }
        true
    }

    /// Cluster index containing `(id, end)`, or `None` for a degenerate/unknown wall.
    pub fn cluster_of(&self, id: ArchId, end: End) -> Option<usize> {
        self.end_cluster.get(&(id, end)).copied()
    }

    /// All wall ends in cluster `i` (empty for an out-of-range index).
    pub fn cluster(&self, i: usize) -> &[(ArchId, End)] {
        self.clusters.get(i).map_or(&[], Vec::as_slice)
    }

    /// Number of wall nodes (excludes degenerate walls).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when there are no wall nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Sort key putting `Start` before `End`.
fn end_rank(end: End) -> u8 {
    match end {
        End::Start => 0,
        End::End => 1,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use cadcraft_doc::Wall;

    fn make_model(walls: Vec<Wall>) -> ArchModel {
        let mut model = ArchModel::default();
        for w in walls {
            model.add_wall(w);
        }
        model
    }

    #[test]
    fn graph_endpoints_that_coincide_share_a_cluster() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let id1 = ArchId(0);
        let id2 = ArchId(1);

        let c1 = graph.cluster_of(id1, End::End).unwrap();
        let c2 = graph.cluster_of(id2, End::Start).unwrap();
        assert_eq!(c1, c2);
    }

    #[test]
    fn graph_neighbours_of_a_corner_wall() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let id1 = ArchId(0);
        let id2 = ArchId(1);

        let neighbours = graph.neighbours(id1, End::End);
        assert_eq!(neighbours, vec![(id2, End::Start)]);
    }

    #[test]
    fn graph_ends_at_finds_both_walls_at_a_point() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let ends = graph.ends_at(Vec2::new(10.0, 0.0));
        assert_eq!(ends.len(), 2);
    }

    #[test]
    fn graph_components_counts_disconnected_groups() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(20.0, 0.0), Vec2::new(30.0, 0.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let components = graph.components();
        assert_eq!(components.len(), 2);
    }

    #[test]
    fn graph_collinear_pairs_detected() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let pairs = graph.collinear_pairs();
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn graph_is_closed_loop_true_for_a_rectangle() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 2.0);
        let w3 = Wall::new(Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0), 2.0);
        let w4 = Wall::new(Vec2::new(0.0, 10.0), Vec2::new(0.0, 0.0), 2.0);
        let model = make_model(vec![w1, w2, w3, w4]);

        let graph = WallGraph::build(&model);

        assert!(graph.is_closed_loop());
    }

    #[test]
    fn graph_is_closed_loop_false_for_an_open_chain() {
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        assert!(!graph.is_closed_loop());
    }

    #[test]
    fn graph_ignores_non_finite_walls() {
        let w1 = Wall::new(Vec2::new(f64::NAN, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn graph_collinear_pairs_ignore_wall_length() {
        // 1 km walls: the unit-direction test must not scale the tolerance with length.
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1000.0, 0.0), 0.2);
        let w2 = Wall::new(Vec2::new(1000.0, 0.0), Vec2::new(2000.0, 1e-8), 0.2);
        let model = make_model(vec![w1, w2]);
        assert_eq!(WallGraph::build(&model).collinear_pairs().len(), 1);
    }

    #[test]
    fn graph_merges_ends_across_a_grid_cell_boundary() {
        // 0.4 µm apart but on either side of a cell edge: still one cluster.
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1.999_999_8e-6 + 5.0, 0.0), 0.2);
        let w2 = Wall::new(Vec2::new(5.000_002_2, 0.0), Vec2::new(5.0, 5.0), 0.2);
        let model = make_model(vec![w1, w2]);
        let graph = WallGraph::build(&model);
        assert_eq!(graph.cluster_of(ArchId(0), End::End), graph.cluster_of(ArchId(1), End::Start));
    }

    #[test]
    fn graph_chains_ends_transitively_and_keeps_distinct_points_apart() {
        let tol = ENDPOINT_TOL;
        let w1 = Wall::new(Vec2::new(-5.0, 0.0), Vec2::new(0.0, 0.0), 0.2);
        let w2 = Wall::new(Vec2::new(0.9 * tol, 0.0), Vec2::new(0.0, 5.0), 0.2);
        let w3 = Wall::new(Vec2::new(1.8 * tol, 0.0), Vec2::new(5.0, 0.0), 0.2);
        let w4 = Wall::new(Vec2::new(10.0 * tol, 0.0), Vec2::new(0.0, -5.0), 0.2);
        let graph = WallGraph::build(&make_model(vec![w1, w2, w3, w4]));
        let c = graph.cluster_of(ArchId(0), End::End);
        assert_eq!(c, graph.cluster_of(ArchId(1), End::Start));
        assert_eq!(c, graph.cluster_of(ArchId(2), End::Start));
        assert_ne!(c, graph.cluster_of(ArchId(3), End::Start));
    }

    #[test]
    fn graph_build_is_linear_in_the_wall_count() {
        // MAX_WALLS disjoint walls used to take a minute (all-pairs cluster merge with a
        // linear endpoint lookup); the grid build is a few milliseconds even in debug.
        let walls: Vec<Wall> =
            (0..MAX_WALLS).map(|i| Wall::new(Vec2::new(i as f64 * 20.0, 0.0), Vec2::new(i as f64 * 20.0 + 10.0, 0.0), 0.2)).collect();
        let model = make_model(walls);
        let t = std::time::Instant::now();
        let graph = WallGraph::build(&model);
        assert_eq!(graph.len(), MAX_WALLS);
        assert!(t.elapsed().as_secs_f64() < 5.0, "graph build took {:?}", t.elapsed());
    }

    #[test]
    fn graph_caps_at_max_walls_without_panicking() {
        let mut walls = Vec::new();
        for i in 0..(MAX_WALLS + 8) {
            let x = i as f64 * 20.0;
            walls.push(Wall::new(Vec2::new(x, 0.0), Vec2::new(x + 10.0, 0.0), 2.0));
        }
        let model = make_model(walls);

        let graph = WallGraph::build(&model);

        assert!(graph.len() <= MAX_WALLS);
    }
}
