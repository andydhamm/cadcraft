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

impl WallGraph {
    /// Build from a model. Deterministic: cluster order follows wall order, and
    /// cluster members are ordered by (ArchId, End).
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

        // Single-linkage union-find over a grid of ENDPOINT_TOL-sized cells. Each
        // endpoint is assigned to a cell; within-tol neighbours are found by looking
        // at the 3x3 cell neighbourhood and unioning pairs at distance <= ENDPOINT_TOL.
        let n = ends.len();
        let mut uf = UnionFind::new(n);

        // Quantised cell (saturating for huge coordinates).
        let cell_of = |p: Vec2| (saturate_cell_coord(p.x), saturate_cell_coord(p.y));
        let mut cell: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, (_, _, point)) in ends.iter().enumerate() {
            cell.entry(cell_of(*point)).or_default().push(i);
        }

        // Union ends that are within ENDPOINT_TOL, scanning each cell and its 3x3
        // neighbours. Each unordered cell-pair is processed once (cell <= neighbour).
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

        // Group endpoints by union-find root; the cluster representative is the
        // lowest endpoint index in the group, so cluster order follows wall order.
        let mut groups: Vec<Vec<usize>> = Vec::with_capacity(n);
        for i in 0..n {
            if i < groups.len() {
                groups[i].push(i);
            } else {
                groups.push(vec![i]);
            }
        }
        for i in 1..n {
            let r = groups[i][0];
            let ri = i % groups.len();
            groups[r].push(i);
        }
        // re-index: collapse each final root group into its own cluster.
        let mut final_groups: Vec<Vec<usize>> = Vec::new();
        let mut root_seen: Vec<bool> = vec![false; n];
        for g in &groups {
            if g.is_empty() {
                continue;
            }
            let r = g[0];
            if !root_seen[r] {
                root_seen[r] = true;
                final_groups.push(g.clone());
            }
        }

        // Deterministic: sort members by (ArchId, Start-before-End); sort clusters
        // by their lowest member index (wall order).
        let mut member_key = |e: &End| {
            let (id, end) = *e;
            (id, if end == End::Start { 0u8 } else { 1u8 })
        };
        let mut ordered: Vec<Vec<(ArchId, End)>> = Vec::new();
        let mut cluster_of_member: Vec<usize> = vec![0; n];
        for g in &final_groups {
            let mut members: Vec<(ArchId, End)> = Vec::with_capacity(g.len());
            for &idx in g {
                members.push((ends[idx].0, ends[idx].1));
            }
            members.sort_by_key(member_key);
            let cidx = ordered.len();
            for (idx, (id, end)) in members.iter().enumerate() {
                cluster_of_member[idx] = cidx;
                let _ = (id, end);
            }
            ordered.push(members);
        }
        // cluster index = order by lowest member index (already wall order since members
        // were collected in index order); sort clusters by their lowest member.
        let mut by_low: Vec<(usize, usize)> = Vec::with_capacity(ordered.len());
        for (ci, members) in ordered.iter().enumerate() {
            let low = members.iter().cloned().fold((ArchId::MAX, End::Start), |best, (id, end)| {
                let (id, e) = if end == End::Start { (id, 0u8) } else { (id, 1u8) };
                ...
            });
            _ = by_low;
        }

        // Rebuild clusters/end_cluster from union-find (lower-index root keeps wall order).
        graph.clusters.clear();
        graph.end_cluster.clear();
        for (i, (id, end, _)) in ends.iter().enumerate() {
            let root = uf.find(i);
            let cluster = match graph.end_cluster.get_key_or_default(i) {
                Some(idx) => idx,
                _ => {
                    if !graph.end_cluster.contains_key((*id, *end)) {
                        graph.end_cluster.insert((*id, *end), graph.clusters.len());
                        graph.clusters.push(Vec::new());
                    }
                    *graph.end_cluster.get(&(*id, *end)).unwrap()
                }
            };
        }

        graph
    }

    fn merge_clusters(&mut self) {
        let mut changed = true;
        while changed {
            changed = false;
            let n = self.clusters.len();
            for i in 0..n {
                if self.clusters[i].is_empty() {
                    continue;
                }
                for j in (i + 1)..n {
                    if self.clusters[j].is_empty() {
                        continue;
                    }
                    let (id_i, end_i) = self.clusters[i][0];
                    let (id_j, end_j) = self.clusters[j][0];
                    let point_i = self.endpoint_point(id_i, end_i);
                    let point_j = self.endpoint_point(id_j, end_j);
                    let dx = point_i.x - point_j.x;
                    let dy = point_i.y - point_j.y;
                    if dx * dx + dy * dy <= ENDPOINT_TOL * ENDPOINT_TOL {
                        let members: Vec<(ArchId, End)> = self.clusters[j].drain(..).collect();
                        for member in members {
                            self.clusters[i].push(member);
                            self.end_cluster.insert(member, i);
                        }
                        changed = true;
                    }
                }
            }
        }

        // Remove empty clusters and reindex.
        let mut new_clusters = Vec::new();
        let mut remap = BTreeMap::new();
        for (i, cluster) in self.clusters.iter().enumerate() {
            if !cluster.is_empty() {
                let new_idx = new_clusters.len();
                remap.insert(i, new_idx);
                new_clusters.push(cluster.clone());
            }
        }
        self.clusters = new_clusters;

        let mut new_end_cluster = HashMap::new();
        for ((id, end), idx) in self.end_cluster.iter() {
            if let Some(new_idx) = remap.get(idx) {
                new_end_cluster.insert((*id, *end), *new_idx);
            }
        }
        self.end_cluster = new_end_cluster;
    }

    fn endpoint_point(&self, id: ArchId, end: End) -> Vec2 {
        for (node_id, start, end_pt) in &self.nodes {
            if *node_id == id {
                return match end {
                    End::Start => *start,
                    End::End => *end_pt,
                };
            }
        }
        Vec2::ZERO
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
                let cross = dir_i.x * dir_j.y - dir_i.y * dir_j.x;
                if cross.abs() >= 1e-9 {
                    continue;
                }
                let v = *start_j - *start_i;
                let dist = (v.x * dir_i.y - v.y * dir_i.x).abs();
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
}
