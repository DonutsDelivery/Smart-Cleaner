use std::collections::{HashMap, HashSet, VecDeque};

use crate::model::dep_tree::DependencyTree;
use crate::model::package_source::PackageSource;

pub const NODE_WIDTH: f64 = 225.0;
pub const NODE_HEIGHT: f64 = 33.0;
// Column gaps are now dynamic — see MIN_COL_GAP / GAP_PER_EDGE / MAX_COL_GAP in build()
const V_GAP: f64 = 3.0;
const PADDING: f64 = 18.0;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum GraphFilter {
    DesktopApps,
    #[default]
    All,
    Pacman,
    Aur,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum GraphSort {
    #[default]
    Alphabetical,
    InstallDate,
    InstalledSize,
    DependencyCount,
    Category,
}

/// Canonical category labels for display and filtering.
pub const CATEGORY_LABELS: &[&str] = &[
    "Audio/Video",
    "Development",
    "Education",
    "Game",
    "Graphics",
    "Network",
    "Office",
    "Science",
    "Settings",
    "System",
    "Utility",
    "Other",
];

/// Map a package's freedesktop categories list to a primary canonical label.
/// Returns `""` if the package has no categories at all (dependencies).
pub fn primary_category(categories: &[String]) -> &'static str {
    for cat in categories {
        match cat.as_str() {
            "AudioVideo" | "Audio" | "Video" | "Music" | "Player" | "Recorder"
            | "Midi" | "Mixer" | "Sequencer" | "Tuner" | "TV" => return "Audio/Video",

            "Development" | "IDE" | "TextEditor" | "Debugger" | "WebDevelopment"
            | "Building" | "Translation" | "GUIDesigner" | "Profiling"
            | "RevisionControl" | "ProjectManagement" => return "Development",

            "Education" | "Art" | "Construction" | "Languages" | "Humanities"
            | "Sports" => return "Education",

            "Game" | "ActionGame" | "AdventureGame" | "ArcadeGame" | "BoardGame"
            | "BlocksGame" | "CardGame" | "KidsGame" | "LogicGame" | "RolePlaying"
            | "Shooter" | "Simulation" | "SportsGame" | "StrategyGame" => return "Game",

            "Graphics" | "2DGraphics" | "3DGraphics" | "Photography" | "Viewer"
            | "RasterGraphics" | "VectorGraphics" | "Scanning" | "OCR"
            | "Publishing" => return "Graphics",

            "Network" | "WebBrowser" | "Email" | "InstantMessaging" | "Chat"
            | "IRCClient" | "Feed" | "FileTransfer" | "HamRadio" | "News"
            | "P2P" | "RemoteAccess" | "Telephony" | "VideoConference" => return "Network",

            "Office" | "Calendar" | "ContactManagement" | "Database" | "Dictionary"
            | "Chart" | "Finance" | "FlowChart" | "PDA" | "Presentation"
            | "Spreadsheet" | "WordProcessor" => return "Office",

            "Science" | "ArtificialIntelligence" | "Astronomy" | "Biology"
            | "Chemistry" | "ComputerScience" | "DataVisualization" | "Economy"
            | "Electricity" | "Geography" | "Geology" | "Geoscience"
            | "Math" | "MedicalSoftware" | "NumericalAnalysis" | "Parallel"
            | "Physics" | "Robotics" => return "Science",

            "Settings" | "DesktopSettings" | "HardwareSettings" | "Printing"
            | "PackageManager" | "Preferences" | "Security" => return "Settings",

            "System" | "Emulator" | "FileTools" | "FileManager" | "Monitor"
            | "TerminalEmulator" | "Filesystem" => return "System",

            "Utility" | "Accessibility" | "Archiving" | "Calculator" | "Clock"
            | "Compression" | "TextTools" | "Maps" => return "Utility",

            _ => continue,
        }
    }
    if categories.is_empty() {
        ""
    } else {
        "Other"
    }
}

/// RGBA color for a category (subtle background tint for dark theme).
pub fn category_color(cat: &str) -> (f64, f64, f64, f64) {
    match cat {
        "Audio/Video"  => (0.30, 0.20, 0.38, 0.95),
        "Development"  => (0.18, 0.25, 0.38, 0.95),
        "Education"    => (0.32, 0.22, 0.30, 0.95),
        "Game"         => (0.18, 0.32, 0.20, 0.95),
        "Graphics"     => (0.35, 0.26, 0.18, 0.95),
        "Network"      => (0.18, 0.28, 0.33, 0.95),
        "Office"       => (0.32, 0.30, 0.18, 0.95),
        "Science"      => (0.22, 0.32, 0.22, 0.95),
        "Settings"     => (0.26, 0.24, 0.30, 0.95),
        "System"       => (0.24, 0.24, 0.28, 0.95),
        "Utility"      => (0.22, 0.26, 0.30, 0.95),
        "Other"        => (0.25, 0.23, 0.23, 0.95),
        _              => (0.22, 0.22, 0.26, 0.95), // default explicit gray
    }
}

pub struct GraphLayout {
    pub node_positions: HashMap<String, (f64, f64)>,
    pub edges: Vec<(String, String)>,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub max_layer: usize,
}

// --- Van der Ploeg tidy tree internals ---

struct TNode {
    id: String,
    w: f64,
    h: f64,
    x: f64,
    y: f64,
    prelim: f64,
    mod_val: f64,
    shift: f64,
    change: f64,
    tl: Option<usize>,
    tr: Option<usize>,
    el: usize,
    er: usize,
    msel: f64,
    mser: f64,
    children: Vec<usize>,
}

struct IYL {
    low_y: f64,
    index: usize,
    nxt: Option<Box<IYL>>,
}

fn update_iyl(min_y: f64, i: usize, mut ih: Option<Box<IYL>>) -> Option<Box<IYL>> {
    while let Some(ref cur) = ih {
        if min_y >= cur.low_y {
            ih = ih.unwrap().nxt;
        } else {
            break;
        }
    }
    Some(Box::new(IYL { low_y: min_y, index: i, nxt: ih }))
}

fn bottom(nodes: &[TNode], i: usize) -> f64 {
    nodes[i].y + nodes[i].h
}

fn next_left(nodes: &[TNode], i: usize) -> Option<usize> {
    if nodes[i].children.is_empty() {
        nodes[i].tl
    } else {
        Some(nodes[i].children[0])
    }
}

fn next_right(nodes: &[TNode], i: usize) -> Option<usize> {
    if nodes[i].children.is_empty() {
        nodes[i].tr
    } else {
        Some(*nodes[i].children.last().unwrap())
    }
}

fn set_extremes(nodes: &mut [TNode], i: usize) {
    if nodes[i].children.is_empty() {
        nodes[i].el = i;
        nodes[i].er = i;
        nodes[i].msel = 0.0;
        nodes[i].mser = 0.0;
    } else {
        let first = nodes[i].children[0];
        let last = *nodes[i].children.last().unwrap();
        nodes[i].el = nodes[first].el;
        nodes[i].msel = nodes[first].msel;
        nodes[i].er = nodes[last].er;
        nodes[i].mser = nodes[last].mser;
    }
}

fn move_subtree(nodes: &mut [TNode], parent: usize, i: usize, si: usize, dist: f64) {
    let ci = nodes[parent].children[i];
    nodes[ci].mod_val += dist;
    nodes[ci].msel += dist;
    nodes[ci].mser += dist;
    distribute_extra(nodes, parent, i, si, dist);
}

fn distribute_extra(nodes: &mut [TNode], parent: usize, i: usize, si: usize, dist: f64) {
    if si != i - 1 {
        let nr = (i - si) as f64;
        let si1 = nodes[parent].children[si + 1];
        let ci = nodes[parent].children[i];
        nodes[si1].shift += dist / nr;
        nodes[ci].shift -= dist / nr;
        nodes[ci].change -= dist - dist / nr;
    }
}

fn add_child_spacing(nodes: &mut [TNode], i: usize) {
    let mut d = 0.0;
    let mut modsumdelta = 0.0;
    let children: Vec<usize> = nodes[i].children.clone();
    for &ci in &children {
        d += nodes[ci].shift;
        modsumdelta += d + nodes[ci].change;
        nodes[ci].mod_val += modsumdelta;
    }
}

fn separate(nodes: &mut [TNode], parent: usize, i: usize, mut ih: Option<Box<IYL>>) -> Option<Box<IYL>> {
    let mut sr_idx = Some(nodes[parent].children[i - 1]);
    let mut mssr = nodes[sr_idx.unwrap()].mod_val;
    let mut cl_idx = Some(nodes[parent].children[i]);
    let mut mscl = nodes[cl_idx.unwrap()].mod_val;
    let mut first = true;

    while sr_idx.is_some() && cl_idx.is_some() {
        let sr = sr_idx.unwrap();
        let cl = cl_idx.unwrap();

        if let Some(ref cur_ih) = ih {
            if bottom(nodes, sr) > cur_ih.low_y {
                ih = ih.unwrap().nxt;
            }
        }

        let dist = (mssr + nodes[sr].prelim + nodes[sr].w) - (mscl + nodes[cl].prelim);
        if (first && dist < 0.0) || dist > 0.0 {
            let ih_index = ih.as_ref().map(|h| h.index).unwrap_or(0);
            mscl += dist;
            move_subtree(nodes, parent, i, ih_index, dist);
            first = false;
        }

        let sy = bottom(nodes, sr);
        let cy = bottom(nodes, cl);
        if sy <= cy {
            sr_idx = next_right(nodes, sr);
            if let Some(s) = sr_idx {
                mssr += nodes[s].mod_val;
            }
        }
        if sy >= cy {
            cl_idx = next_left(nodes, cl);
            if let Some(c) = cl_idx {
                mscl += nodes[c].mod_val;
            }
        }
    }

    if sr_idx.is_none() && cl_idx.is_some() {
        set_left_thread(nodes, parent, i, cl_idx.unwrap(), mscl);
    } else if sr_idx.is_some() && cl_idx.is_none() {
        set_right_thread(nodes, parent, i, sr_idx.unwrap(), mssr);
    }

    ih
}

fn set_left_thread(nodes: &mut [TNode], parent: usize, i: usize, cl: usize, modsumcl: f64) {
    let first_child = nodes[parent].children[0];
    let li = nodes[first_child].el;
    nodes[li].tl = Some(cl);
    let diff = (modsumcl - nodes[cl].mod_val) - nodes[first_child].msel;
    nodes[li].mod_val += diff;
    nodes[li].prelim -= diff;
    let ci = nodes[parent].children[i];
    nodes[first_child].el = nodes[ci].el;
    nodes[first_child].msel = nodes[ci].msel;
}

fn set_right_thread(nodes: &mut [TNode], parent: usize, i: usize, sr: usize, modsumsr: f64) {
    let ci = nodes[parent].children[i];
    let ri = nodes[ci].er;
    nodes[ri].tr = Some(sr);
    let diff = (modsumsr - nodes[sr].mod_val) - nodes[ci].mser;
    nodes[ri].mod_val += diff;
    nodes[ri].prelim -= diff;
    let prev = nodes[parent].children[i - 1];
    nodes[ci].er = nodes[prev].er;
    nodes[ci].mser = nodes[prev].mser;
}

fn position_root(nodes: &mut [TNode], i: usize) {
    let children = &nodes[i].children;
    if children.is_empty() {
        return;
    }
    let first = children[0];
    let last = *children.last().unwrap();
    nodes[i].prelim = (nodes[first].prelim + nodes[first].mod_val
        + nodes[last].mod_val + nodes[last].prelim + nodes[last].w) / 2.0
        - nodes[i].w / 2.0;
}

fn first_walk(nodes: &mut [TNode], i: usize) {
    if nodes[i].children.is_empty() {
        set_extremes(nodes, i);
        return;
    }
    let children: Vec<usize> = nodes[i].children.clone();
    first_walk(nodes, children[0]);
    let mut ih = update_iyl(bottom(nodes, nodes[children[0]].el), 0, None);
    for ci in 1..children.len() {
        first_walk(nodes, children[ci]);
        let min_y = bottom(nodes, nodes[children[ci]].er);
        ih = separate(nodes, i, ci, ih);
        ih = update_iyl(min_y, ci, ih);
    }
    position_root(nodes, i);
    set_extremes(nodes, i);
}

fn second_walk(nodes: &mut [TNode], i: usize, modsum: f64) -> f64 {
    let ms = modsum + nodes[i].mod_val;
    nodes[i].x = nodes[i].prelim + ms;
    let mut min_x = nodes[i].x;
    add_child_spacing(nodes, i);
    let children: Vec<usize> = nodes[i].children.clone();
    for &ci in &children {
        let c_min = second_walk(nodes, ci, ms);
        if c_min < min_x {
            min_x = c_min;
        }
    }
    min_x
}

fn third_walk(nodes: &mut [TNode], i: usize, shift: f64) {
    nodes[i].x += shift;
    let children: Vec<usize> = nodes[i].children.clone();
    for &ci in &children {
        third_walk(nodes, ci, shift);
    }
}

/// Set y coordinates: depth-first, each child's y = parent.y + parent.h + V_GAP
fn set_y(nodes: &mut [TNode], i: usize) {
    let children: Vec<usize> = nodes[i].children.clone();
    for &ci in &children {
        nodes[ci].y = nodes[i].y + nodes[i].h + V_GAP;
        set_y(nodes, ci);
    }
}

// --- Public API ---

impl GraphLayout {
    /// Build a graph layout.
    ///
    /// `max_dep_fans`: only include dependency nodes required by at most this
    /// many packages. Set to `usize::MAX` to disable. Useful for finding
    /// rarely-shared deps that are easy to clean up.
    pub fn build(
        tree: &DependencyTree,
        filter: GraphFilter,
        sort: GraphSort,
        depth: usize,
        max_dep_fans: usize,
        category_filter: Option<&str>,
    ) -> Self {
        const BASE_THRESHOLD: usize = 50;

        let depended_by = tree.depended_by_map();

        // Filter base packages (>= 50 reverse deps)
        let mut base_pkgs: HashSet<&str> = HashSet::new();
        for (id, parents) in depended_by.iter() {
            if parents.len() >= BASE_THRESHOLD {
                base_pkgs.insert(id.as_str());
            }
        }

        // Collect roots based on filter + category
        let mut roots: Vec<(String, &crate::model::package::PackageInfo)> = Vec::new();
        for pkg in tree.all_packages() {
            if !pkg.is_explicit {
                continue;
            }
            let passes = match filter {
                GraphFilter::DesktopApps => !pkg.display_name.is_empty(),
                GraphFilter::All => true,
                GraphFilter::Pacman => pkg.source == PackageSource::Pacman,
                GraphFilter::Aur => pkg.source == PackageSource::Aur,
            };
            if !passes {
                continue;
            }
            // Category filter
            if let Some(cat_filter) = category_filter {
                let pcat = primary_category(&pkg.categories);
                if pcat != cat_filter {
                    continue;
                }
            }
            let id = pkg.qualified_id();
            roots.push((id, pkg));
        }

        // Sort roots
        let name_key = |pkg: &crate::model::package::PackageInfo| -> String {
            if pkg.display_name.is_empty() {
                pkg.name.to_lowercase()
            } else {
                pkg.display_name.to_lowercase()
            }
        };
        match sort {
            GraphSort::Alphabetical => {
                roots.sort_by(|a, b| name_key(a.1).cmp(&name_key(b.1)));
            }
            GraphSort::InstallDate => {
                roots.sort_by(|a, b| b.1.install_date.cmp(&a.1.install_date));
            }
            GraphSort::InstalledSize => {
                roots.sort_by(|a, b| b.1.installed_size.cmp(&a.1.installed_size));
            }
            GraphSort::DependencyCount => {
                roots.sort_by(|a, b| b.1.depends.len().cmp(&a.1.depends.len()));
            }
            GraphSort::Category => {
                roots.sort_by(|a, b| {
                    let ca = primary_category(&a.1.categories);
                    let cb = primary_category(&b.1.categories);
                    ca.cmp(cb).then_with(|| name_key(a.1).cmp(&name_key(b.1)))
                });
            }
        }

        let root_sort: Vec<(String, String)> = roots
            .iter()
            .map(|(id, pkg)| {
                let sort_key = if pkg.display_name.is_empty() {
                    pkg.name.to_lowercase()
                } else {
                    pkg.display_name.to_lowercase()
                };
                (sort_key, id.clone())
            })
            .collect();

        if root_sort.is_empty() {
            return Self {
                node_positions: HashMap::new(),
                edges: Vec::new(),
                canvas_width: 100.0,
                canvas_height: 100.0,
                max_layer: 0,
            };
        }

        let root_ids: HashSet<&str> = root_sort.iter().map(|(_, id)| id.as_str()).collect();
        let depends_on = tree.depends_on_map();

        // --- Step 1: Best-parent spanning tree with depth limiting ---
        // BFS from roots, tracking depth. Only include deps within max depth.
        let mut all_deps: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = root_sort
            .iter()
            .map(|(_, id)| (id.clone(), 0))
            .collect();
        let mut visited: HashSet<String> = root_ids.iter().map(|s| s.to_string()).collect();

        while let Some((pkg_id, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            if let Some(deps) = depends_on.get(&pkg_id) {
                for dep in deps {
                    if root_ids.contains(dep.as_str()) {
                        continue;
                    }
                    let is_base = base_pkgs.contains(dep.as_str());
                    // Filter by max reverse-dep count (max_dep_fans)
                    let fan_count = depended_by
                        .get(dep)
                        .map(|v| v.len())
                        .unwrap_or(0);
                    let too_shared = fan_count > max_dep_fans;

                    if visited.insert(dep.clone()) {
                        // Still traverse through base/shared packages to discover
                        // deps behind them, but don't add them as visible nodes
                        if !is_base && !too_shared {
                            all_deps.insert(dep.clone());
                        }
                        queue.push_back((dep.clone(), d + 1));
                    }
                }
            }
        }

        // All nodes in the graph (roots + deps)
        let all_node_ids: HashSet<&str> = root_ids
            .iter()
            .copied()
            .chain(all_deps.iter().map(|s| s.as_str()))
            .collect();

        // Best-parent selection: assign each dep to parent with fewest deps
        let mut best_parent: HashMap<String, String> = HashMap::new();
        for dep_id in &all_deps {
            if let Some(parents) = depended_by.get(dep_id) {
                let mut best: Option<(&str, usize)> = None;
                for p in parents {
                    if !all_node_ids.contains(p.as_str()) || base_pkgs.contains(p.as_str()) {
                        continue;
                    }
                    let dep_count = depends_on.get(p).map(|d| d.len()).unwrap_or(0);
                    match best {
                        None => best = Some((p.as_str(), dep_count)),
                        Some((bp, bc)) => {
                            if dep_count < bc || (dep_count == bc && p.as_str() < bp) {
                                best = Some((p.as_str(), dep_count));
                            }
                        }
                    }
                }
                if let Some((bp, _)) = best {
                    best_parent.insert(dep_id.clone(), bp.to_string());
                }
            }
        }

        // Build spanning tree children map
        let mut tree_children: HashMap<String, Vec<String>> = HashMap::new();
        for (dep, parent) in &best_parent {
            tree_children.entry(parent.clone()).or_default().push(dep.clone());
        }
        for children in tree_children.values_mut() {
            children.sort();
        }

        // Collect ALL actual dep edges between nodes in the graph
        let mut edges: Vec<(String, String)> = Vec::new();
        let mut edge_set: HashSet<(String, String)> = HashSet::new();
        for pkg_id in &all_node_ids {
            if let Some(deps) = depends_on.get(*pkg_id) {
                for dep in deps {
                    if all_node_ids.contains(dep.as_str()) && !base_pkgs.contains(dep.as_str()) {
                        let pair = (pkg_id.to_string(), dep.clone());
                        if edge_set.insert(pair.clone()) {
                            edges.push(pair);
                        }
                    }
                }
            }
        }

        // --- Identify no-dep roots BEFORE building layout ---
        let mut no_dep_root_ids: HashSet<String> = HashSet::new();
        for (_, root_id) in &root_sort {
            let has_tree_children = tree_children.contains_key(root_id);
            let has_graph_deps = depends_on
                .get(root_id)
                .map(|deps| {
                    deps.iter()
                        .any(|d| all_node_ids.contains(d.as_str()) && !base_pkgs.contains(d.as_str()))
                })
                .unwrap_or(false);
            if !has_tree_children && !has_graph_deps {
                no_dep_root_ids.insert(root_id.clone());
            }
        }

        // Roots that participate in the tree layout (have deps)
        let dep_root_sort: Vec<&(String, String)> = root_sort
            .iter()
            .filter(|(_, id)| !no_dep_root_ids.contains(id))
            .collect();

        // --- Step 2: Build TNode arena from spanning tree ---
        let mut nodes: Vec<TNode> = Vec::new();
        let mut id_to_idx: HashMap<String, usize> = HashMap::new();

        fn build_tnode(
            pkg_id: &str,
            tree_children: &HashMap<String, Vec<String>>,
            nodes: &mut Vec<TNode>,
            id_to_idx: &mut HashMap<String, usize>,
        ) -> usize {
            let idx = nodes.len();
            id_to_idx.insert(pkg_id.to_string(), idx);
            nodes.push(TNode {
                id: pkg_id.to_string(),
                w: NODE_HEIGHT + V_GAP,
                h: NODE_HEIGHT + V_GAP,
                x: 0.0,
                y: 0.0,
                prelim: 0.0,
                mod_val: 0.0,
                shift: 0.0,
                change: 0.0,
                tl: None,
                tr: None,
                el: idx,
                er: idx,
                msel: 0.0,
                mser: 0.0,
                children: Vec::new(),
            });

            if let Some(children) = tree_children.get(pkg_id) {
                let mut child_indices = Vec::new();
                for child_id in children {
                    if id_to_idx.contains_key(child_id) {
                        continue;
                    }
                    let ci = build_tnode(child_id, tree_children, nodes, id_to_idx);
                    child_indices.push(ci);
                }
                nodes[idx].children = child_indices;
            }
            idx
        }

        // Virtual super-root
        let super_root = nodes.len();
        nodes.push(TNode {
            id: "__super_root__".to_string(),
            w: 0.0,
            h: 0.0,
            x: 0.0,
            y: 0.0,
            prelim: 0.0,
            mod_val: 0.0,
            shift: 0.0,
            change: 0.0,
            tl: None,
            tr: None,
            el: super_root,
            er: super_root,
            msel: 0.0,
            mser: 0.0,
            children: Vec::new(),
        });

        let mut root_indices = Vec::new();
        for (_, root_id) in &dep_root_sort {
            if id_to_idx.contains_key(root_id) {
                continue;
            }
            let ri = build_tnode(root_id, &tree_children, &mut nodes, &mut id_to_idx);
            root_indices.push(ri);
        }
        nodes[super_root].children = root_indices;

        // --- Step 3: Van der Ploeg layout ---
        first_walk(&mut nodes, super_root);
        let min_x = second_walk(&mut nodes, super_root, 0.0);
        if min_x != 0.0 {
            third_walk(&mut nodes, super_root, -min_x);
        }

        // Set y coordinates (depth from super-root)
        nodes[super_root].y = 0.0;
        set_y(&mut nodes, super_root);

        // --- Step 4: Rotate + Mirror with dynamic column gaps ---

        // Compute depth level for each node
        let stride = NODE_HEIGHT + V_GAP;
        let mut node_depth: Vec<usize> = vec![0; nodes.len()];
        let mut max_depth: usize = 0;
        for (i, node) in nodes.iter().enumerate() {
            if node.id == "__super_root__" {
                continue;
            }
            let d = if stride > 0.0 {
                (node.y / stride).round() as usize
            } else {
                0
            };
            node_depth[i] = d;
            max_depth = max_depth.max(d);
        }

        // Count edges crossing each column gap (gap d = between depth d and d+1)
        let mut gap_edges: Vec<usize> = vec![0; max_depth.saturating_add(1)];
        for (from_id, to_id) in &edges {
            let fd = id_to_idx.get(from_id).map(|&i| node_depth[i]).unwrap_or(0);
            let td = id_to_idx.get(to_id).map(|&i| node_depth[i]).unwrap_or(0);
            if fd != td {
                let lo = fd.min(td);
                let hi = fd.max(td);
                for g in lo..hi {
                    if g < gap_edges.len() {
                        gap_edges[g] += 1;
                    }
                }
            }
        }

        // Build cumulative column x-positions with gaps proportional to edge density
        const MIN_COL_GAP: f64 = 50.0;
        const GAP_PER_EDGE: f64 = 3.0;
        const MAX_COL_GAP: f64 = 250.0;
        let mut col_x = vec![PADDING + NODE_WIDTH / 2.0];
        for d in 0..max_depth {
            let ec = gap_edges.get(d).copied().unwrap_or(0);
            let gap = (MIN_COL_GAP + ec as f64 * GAP_PER_EDGE).min(MAX_COL_GAP);
            col_x.push(col_x[d] + NODE_WIDTH + gap);
        }

        // Apply rotated positions using dynamic column x
        let mut max_x: f64 = 0.0;
        let mut max_y: f64 = 0.0;
        for (i, node) in nodes.iter_mut().enumerate() {
            let old_x = node.x;
            let d = node_depth[i];
            node.x = col_x.get(d).copied().unwrap_or(col_x[0]);
            node.y = PADDING + old_x + NODE_HEIGHT / 2.0;
            if node.x > max_x {
                max_x = node.x;
            }
            if node.y > max_y {
                max_y = node.y;
            }
        }

        let canvas_width = max_x + NODE_WIDTH / 2.0 + PADDING;
        let canvas_height = max_y + NODE_HEIGHT / 2.0 + PADDING;

        // Mirror x so roots (depth=1) are on the right, deep deps on the left.
        let mut node_positions = HashMap::with_capacity(nodes.len());
        for node in &nodes {
            if node.id == "__super_root__" {
                continue;
            }
            let mx = canvas_width - node.x + PADDING;
            node_positions.insert(node.id.clone(), (mx, node.y));
        }

        // Place no-dep roots in a separate column to the right, stacked vertically
        let no_dep_offset = if no_dep_root_ids.is_empty() {
            0.0
        } else {
            NODE_WIDTH + MIN_COL_GAP
        };
        let no_dep_col_x = canvas_width + no_dep_offset;
        let mut no_dep_y = PADDING + NODE_HEIGHT / 2.0;
        for (_, root_id) in &root_sort {
            if no_dep_root_ids.contains(root_id) {
                node_positions.insert(root_id.clone(), (no_dep_col_x, no_dep_y));
                no_dep_y += NODE_HEIGHT + V_GAP;
            }
        }
        let effective_width = if no_dep_root_ids.is_empty() {
            canvas_width
        } else {
            no_dep_col_x + NODE_WIDTH / 2.0 + PADDING
        };
        let effective_height = canvas_height.max(no_dep_y + PADDING);

        Self {
            node_positions,
            edges,
            canvas_width: effective_width,
            canvas_height: effective_height,
            max_layer: 0,
        }
    }

    pub fn node_rect(&self, id: &str) -> Option<(f64, f64, f64, f64)> {
        self.node_positions.get(id).map(|&(cx, cy)| {
            (
                cx - NODE_WIDTH / 2.0,
                cy - NODE_HEIGHT / 2.0,
                NODE_WIDTH,
                NODE_HEIGHT,
            )
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
