use std::collections::HashMap;

use crate::model::dep_tree::DependencyTree;

pub const NODE_WIDTH: f64 = 150.0;
pub const NODE_HEIGHT: f64 = 22.0;
const H_GAP: f64 = 10.0;
const V_GAP: f64 = 2.0;
const PADDING: f64 = 12.0;

pub struct GraphLayout {
    pub node_positions: HashMap<String, (f64, f64)>,
    pub edges: Vec<(String, String)>,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub max_layer: usize,
}

struct LNode {
    id: String,
    children: Vec<usize>,
}

impl GraphLayout {
    pub fn build(tree: &DependencyTree) -> Self {
        const BASE_THRESHOLD: usize = 50;

        let depended_by = tree.depended_by_map();
        let mut base_pkgs: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (id, parents) in depended_by.iter() {
            if parents.len() >= BASE_THRESHOLD {
                base_pkgs.insert(id.as_str());
            }
        }

        let mut root_sort: Vec<(String, String)> = Vec::new();
        for pkg in tree.all_packages() {
            if !pkg.is_explicit { continue; }
            let id = pkg.qualified_id();
            let sort_key = if pkg.display_name.is_empty() {
                pkg.name.to_lowercase()
            } else {
                pkg.display_name.to_lowercase()
            };
            root_sort.push((sort_key, id));
        }
        root_sort.sort_by(|a, b| a.0.cmp(&b.0));

        if root_sort.is_empty() {
            return Self {
                node_positions: HashMap::new(),
                edges: Vec::new(),
                canvas_width: 100.0,
                canvas_height: 100.0,
                max_layer: 0,
            };
        }

        let root_ids: std::collections::HashSet<&str> =
            root_sort.iter().map(|(_, id)| id.as_str()).collect();

        let mut nodes: Vec<LNode> = Vec::new();
        let mut edges: Vec<(String, String)> = Vec::new();
        let mut root_indices: Vec<usize> = Vec::new();

        // Per-root visited sets so each app shows its own deps.
        // Depth capped at 10 to prevent explosion with per-root visited.
        for (_, root_id) in &root_sort {
            let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
            visited.insert(root_id.clone());
            // Also mark all other roots as visited so they don't appear as deps
            for (_, other_id) in &root_sort {
                visited.insert(other_id.clone());
            }
            let ri = Self::build_subtree(
                root_id, tree, &base_pkgs, &root_ids,
                &mut visited, &mut nodes, &mut edges, 10,
            );
            root_indices.push(ri);
        }

        // Single-pass placement: walk depth-first, each leaf gets the next y slot.
        // Parents get y = average of their children's y.
        let row_h = NODE_HEIGHT + V_GAP;
        let mut positions: Vec<(f64, f64)> = vec![(0.0, 0.0); nodes.len()];
        let mut cursor_y = PADDING + NODE_HEIGHT / 2.0;

        for &ri in &root_indices {
            Self::place_dfs(&nodes, &mut positions, ri, PADDING + NODE_WIDTH / 2.0, &mut cursor_y, row_h);
        }

        // Find max x for mirroring
        let mut max_x: f64 = 0.0;
        for &(x, _) in &positions {
            if x > max_x { max_x = x; }
        }
        let canvas_width = max_x + NODE_WIDTH / 2.0 + PADDING;
        let canvas_height = cursor_y + PADDING;

        // Mirror x so apps (roots) are on right, deps on left
        let mut node_positions = HashMap::with_capacity(nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            let (x, y) = positions[i];
            let mx = canvas_width - x;
            node_positions.insert(node.id.clone(), (mx, y));
        }

        eprintln!("LAYOUT: {} nodes, {} edges, canvas {}x{}", node_positions.len(), edges.len(), canvas_width, canvas_height);

        Self { node_positions, edges, canvas_width, canvas_height, max_layer: 0 }
    }

    fn build_subtree(
        pkg_id: &str,
        tree: &DependencyTree,
        base_pkgs: &std::collections::HashSet<&str>,
        root_ids: &std::collections::HashSet<&str>,
        visited: &mut std::collections::HashSet<String>,
        nodes: &mut Vec<LNode>,
        edges: &mut Vec<(String, String)>,
        remaining: usize,
    ) -> usize {
        let idx = nodes.len();
        nodes.push(LNode { id: pkg_id.to_string(), children: Vec::new() });

        if remaining > 0 {
            if let Some(deps) = tree.depends_on_map().get(pkg_id) {
                let mut dep_list: Vec<&String> = deps.iter()
                    .filter(|d| {
                        !base_pkgs.contains(d.as_str())
                        && !root_ids.contains(d.as_str())
                        && !visited.contains(d.as_str())
                    })
                    .collect();
                dep_list.sort();

                for dep_id in dep_list {
                    visited.insert(dep_id.clone());
                    edges.push((pkg_id.to_string(), dep_id.clone()));
                    let ci = Self::build_subtree(
                        dep_id, tree, base_pkgs, root_ids,
                        visited, nodes, edges, remaining - 1,
                    );
                    nodes[idx].children.push(ci);
                }
            }
        }
        idx
    }

    /// DFS placement: every node gets a y slot in DFS order.
    /// Children placed directly after parent, indented right.
    fn place_dfs(
        nodes: &[LNode],
        positions: &mut Vec<(f64, f64)>,
        idx: usize,
        x: f64,
        cursor_y: &mut f64,
        row_h: f64,
    ) {
        // This node takes the current slot
        positions[idx] = (x, *cursor_y);
        *cursor_y += row_h;

        // Children placed right after, indented
        let child_x = x + NODE_WIDTH + H_GAP;
        let children: Vec<usize> = nodes[idx].children.clone();
        for &ci in &children {
            Self::place_dfs(nodes, positions, ci, child_x, cursor_y, row_h);
        }
    }

    pub fn node_rect(&self, id: &str) -> Option<(f64, f64, f64, f64)> {
        self.node_positions.get(id).map(|&(cx, cy)| {
            (cx - NODE_WIDTH / 2.0, cy - NODE_HEIGHT / 2.0, NODE_WIDTH, NODE_HEIGHT)
        })
    }

    pub fn hit_test(&self, x: f64, y: f64) -> Option<&str> {
        for (id, &(cx, cy)) in &self.node_positions {
            let left = cx - NODE_WIDTH / 2.0;
            let top = cy - NODE_HEIGHT / 2.0;
            if x >= left && x <= left + NODE_WIDTH && y >= top && y <= top + NODE_HEIGHT {
                return Some(id.as_str());
            }
        }
        None
    }
}
