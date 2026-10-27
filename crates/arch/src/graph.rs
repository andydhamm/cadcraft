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
    end_points: HashMap<ArchId, (Vec2, Vec2)>,
    clusters: Vec<Vec<(ArchId, End)>>,
    end_cluster: HashMap<(ArchId, End), usize>,
}

/// Union-Find (disjoint set) with path compression and "lower index as root".
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    /// Create a union-find with `n` singleton sets.
    fn new(n: usize) -> Self {
        UnionFind { parent: (0..n).collect() }
    }

    /// Find the root of `i` with path compression.
    fn find(&mut self, i: usize) -> usize {
        if self.parent[i] == i {
            return i;
        }
        self.parent[i] = self.find(self.parent[i]);
        self.parent[i]
    }

    /// Union the sets containing `i` and `j`. The lower root becomes the new root.
    fn union(&mut self, i: usize, j: usize) {
        let ri = self.find(i);
        let rj = self.find(j);
        if ri == rj {
            return;
        }
        if ri < rj {
            self.parent[rj] = ri;
        } else {
            self.parent[ri] = rj;
        }
    }
}

/// Quantise a coordinate to a cell index, saturating for huge values.
fn saturate_cell_coord(x: f64) -> i64 {
    if !x.is_finite() {
        return i64::MIN;
    }
    let v = x / ENDPOINT_TOL;
    if v > i64::MAX as f64 {
        i64::MAX
    } else if v < i64::MIN as f64 {
        i64::MIN
    } else {
        v as i64
    }
}

/// Squared distance between two points.
fn dist_sq(a: &Vec2, b: &Vec2) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

impl WallGraph {
    /// Build from a model. Deterministic: cluster order follows wall order, and
    /// cluster members are ordered by (ArchId, Start-before-End).
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
            let start = entry.start;
            let end = entry.end;
            graph.nodes.push((id, start, end));
            graph.end_points.insert(id, (start, end));
            ends.push((id, End::Start, start));
            ends.push((id, End::End, end));
        }

        let n = ends.len();
        if n == 0 {
            return graph;
        }

        // Single-linkage union-find over a grid of ENDPOINT_TOL-sized cells.
        let mut uf = UnionFind::new(n);

        // Quantised cell (saturating for huge coordinates).
        let cell_of = |p: Vec2| (saturate_cell_coord(p.x), saturate_cell_coord(p.y));
        let mut cell: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, (_, _, point)) in ends.iter().enumerate() {
            cell.entry(cell_of(*point)).or_default().push(i);
        }

        // Union ends that are within ENDPOINT_TOL, scanning each cell and its 3x3
        // neighbours. Each unordered cell-pair is processed once.
        for (ckey, members) in &cell {
            for ddx in [-1, 0, 1] {
                for ddy in [-1, 0, 1] {
                    let nkey = (ckey.0 + ddx, ckey.1 + ddy);
                    if ckey > &nkey {
                        continue;
                    }
                    let Some(neighbours) = cell.get(&nkey) else {
                        continue;
                    };
                    for &i in members {
                        for &j in neighbours {
                            if i >= j {
                                continue;
                            }
                            if dist_sq(&ends[i].2, &ends[j].2) <= ENDPOINT_TOL * ENDPOINT_TOL {
                                uf.union(i, j);
                            }
                        }
                    }
                }
            }
        }

        // Group endpoints by union-find root. The root is the lowest endpoint
        // index in the component (union keeps lower index as root).
        let mut root_to_members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..n {
            let r = uf.find(i);
            root_to_members.entry(r).or_default().push(i);
        }

        // Build clusters: sort members by (ArchId, Start-before-End).
        // Sort clusters by root (which is the lowest endpoint index = wall order).
        let mut clusters: Vec<Vec<(ArchId, End)>> = Vec::new();
        for (r, members) in &root_to_members {
            let mut cluster: Vec<(ArchId, End)> = Vec::with_capacity(members.len());
            for &idx in members {
                cluster.push((ends[idx].0, ends[idx].1));
            }
            cluster.sort_by_key(|m| (m.0, if m.1 == End::Start { 0u8 } else { 1u8 }));
            clusters.push(cluster);
        }
        // Sort by root (lowest endpoint index = wall order).
        // BTreeMap iterates in key order, so clusters are already sorted by root.
        // Verify and sort explicitly for safety.
        let mut indexed: Vec<(usize, Vec<(ArchId, End)>)> = root_to_members
            .iter()
            .map(|(r, members)| {
                let mut cluster: Vec<(ArchId, End)> = Vec::with_capacity(members.len());
                for &idx in members {
                    cluster.push((ends[idx].0, ends[idx].1));
                }
                cluster.sort_by_key(|m| (m.0, if m.1 == End::Start { 0u8 } else { 1u8 }));
                (*r, cluster)
            })
            .collect();
        indexed.sort_by_key(|c| c.0);
        graph.clusters = indexed.into_iter().map(|(_, c)| c).collect();

        // Build end_cluster map: (ArchId, End) -> cluster index.
        let mut end_cluster = HashMap::with_capacity(graph.nodes.len() * 2);
        for (ci, members) in graph.clusters.iter().enumerate() {
            for &(id, end) in members {
                end_cluster.insert((id, end), ci);
            }
        }
        graph.end_cluster = end_cluster;

        graph
    }

    fn endpoint_point(&self, id: ArchId, end: End) -> Vec2 {
        match self.end_points.get(&id) {
            Some((start, e)) => match end {
                End::Start => *start,
                End::End => *e,
            },
            None => Vec2::ZERO,
        }
    }

    /// The wall ends sharing the cluster of `(id, end)`, excluding `(id, end)` itself.
    pub fn neighbours(&self, id: ArchId, end: End) -> Vec<(ArchId, End)> {
        let cluster_idx = match self.end_cluster.get(&(id, end)) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };
        self.clusters[cluster_idx].iter().filter(|(nid, nend)| *nid != id || *nend != end).cloned().collect()
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
        result.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| {
                let a_val = if a.1 == End::Start { 0u8 } else { 1u8 };
                let b_val = if b.1 == End::Start { 0u8 } else { 1u8 };
                a_val.cmp(&b_val)
            })
        });
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
        for (id, _) in adj.iter() {
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
        components.sort_by_key(|c| c[0]);
        components
    }

    /// Pairs of walls whose centre lines are collinear within `ENDPOINT_TOL` and whose
    /// directions are parallel (|cross| < 1e-9). Each pair once, `(lo, hi)` by ArchId.
    pub fn collinear_pairs(&self) -> Vec<(ArchId, ArchId)> {
        let mut pairs = Vec::new();
        for i in 0..self.nodes.len() {
            for j in (i + 1)..self.nodes.len() {
                let (id_i, start_i, end_i) = &self.nodes[i];
                let (id_j, start_j, end_j) = &self.nodes[j];
                let dir_i = *end_i - *start_i;
                let dir_j = *end_j - *start_j;

                // Check non-degenerate walls.
                let len_i = dir_i.x * dir_i.x + dir_i.y * dir_i.y;
                let len_j = dir_j.x * dir_j.x + dir_j.y * dir_j.y;
                if len_i <= 0.0 || len_j <= 0.0 {
                    continue;
                }

                // Unit directions for length-independent comparison.
                let sqrt_len_i = len_i.sqrt();
                let sqrt_len_j = len_j.sqrt();
                let ui = Vec2::new(dir_i.x / sqrt_len_i, dir_i.y / sqrt_len_i);
                let uj = Vec2::new(dir_j.x / sqrt_len_j, dir_j.y / sqrt_len_j);

                // |cross| < 1e-9 for parallel unit vectors.
                let cross = ui.x * uj.y - ui.y * uj.x;
                if cross.abs() >= 1e-9 {
                    continue;
                }

                // Perpendicular distance from start_j to the line of i.
                let v = *start_j - *start_i;
                let dist = (v.x * ui.y - v.y * ui.x).abs();
                if dist <= ENDPOINT_TOL {
                    let lo = if *id_i < *id_j { *id_i } else { *id_j };
                    let hi = if *id_i < *id_j { *id_j } else { *id_i };
                    pairs.push((lo, hi));
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
            if self.clusters[start_cluster].len() < 2 {
                return false;
            }
            if self.clusters[end_cluster].len() < 2 {
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
        if i >= self.clusters.len() {
            return &[];
        }
        &self.clusters[i]
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

    #[test]
    fn graph_build_is_linear_in_the_wall_count() {
        let mut walls = Vec::new();
        for i in 0..MAX_WALLS {
            let x = i as f64 * 20.0;
            walls.push(Wall::new(Vec2::new(x, 0.0), Vec2::new(x + 10.0, 0.0), 2.0));
        }
        let model = make_model(walls);

        let start = std::time::Instant::now();
        let graph = WallGraph::build(&model);
        let elapsed = start.elapsed();

        assert!(graph.len() == MAX_WALLS);
        assert!(elapsed < std::time::Duration::from_secs(5), "Build took {elapsed:?} which exceeds 5s");
    }

    #[test]
    fn graph_merges_ends_across_a_grid_cell_boundary() {
        // w1 ends at 5.0 + 1.9999998e-6, w2 starts at 5.0000022.
        // These are on either side of a cell edge (at 5.0/ENDPOINT_TOL) and within
        // ENDPOINT_TOL of each other, so they must share a cluster.
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(5.0 + 1.9999998e-6, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(5.0000022, 0.0), Vec2::new(10.0, 0.0), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let c1 = graph.cluster_of(ArchId(0), End::End).unwrap();
        let c2 = graph.cluster_of(ArchId(1), End::Start).unwrap();
        assert_eq!(c1, c2);
    }

    #[test]
    fn graph_chains_ends_transitively_and_keeps_distinct_points_apart() {
        let tol = ENDPOINT_TOL;

        // w1: (0,0) -> (0.9*tol, 0)
        // w2: (0.9*tol, 0) -> (1.8*tol, 0)
        // w3: (10*tol, 0) -> (10.1*tol, 0)  [far away]
        //
        // End of w1 = (0.9*tol, 0) is within tol of start of w2 = (0.9*tol, 0) (same point).
        // End of w1 at (0.9*tol, 0) is within tol of start of w1 at (0, 0) (dist = 0.9*tol < tol).
        // End of w2 at (1.8*tol, 0) is within tol of end of w1 at (0.9*tol, 0) (dist = 0.9*tol < tol).
        // By transitivity: start of w1, end of w1, end of w2 all in one cluster.
        // Start of w3 at (10*tol, 0) is at distance > 8.1*tol from end of w2, not in cluster.
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(0.9 * tol, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(0.9 * tol, 0.0), Vec2::new(1.8 * tol, 0.0), 2.0);
        let w3 = Wall::new(Vec2::new(10.0 * tol, 0.0), Vec2::new(10.1 * tol, 0.0), 2.0);
        let model = make_model(vec![w1, w2, w3]);

        let graph = WallGraph::build(&model);

        // Ends at 0, 0.9*tol, 1.8*tol all in the same cluster.
        let c_start1 = graph.cluster_of(ArchId(0), End::Start).unwrap();
        let c_end1 = graph.cluster_of(ArchId(0), End::End).unwrap();
        let c_end2 = graph.cluster_of(ArchId(1), End::End).unwrap();
        assert_eq!(c_start1, c_end1);
        assert_eq!(c_end1, c_end2);

        // End at 10*tol is NOT in that cluster.
        let c_start3 = graph.cluster_of(ArchId(2), End::Start).unwrap();
        assert_ne!(c_start3, c_end1);
    }

    #[test]
    fn graph_collinear_pairs_ignore_wall_length() {
        // Long walls: (0,0)-(1000,0) and (1000,0)-(2000, 1e-8)
        // Must be detected as collinear despite the 1000m wall length.
        let w1 = Wall::new(Vec2::new(0.0, 0.0), Vec2::new(1000.0, 0.0), 2.0);
        let w2 = Wall::new(Vec2::new(1000.0, 0.0), Vec2::new(2000.0, 1.0e-8), 2.0);
        let model = make_model(vec![w1, w2]);

        let graph = WallGraph::build(&model);

        let pairs = graph.collinear_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (ArchId(0), ArchId(1)));
    }
}
