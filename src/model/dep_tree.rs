use std::collections::{HashMap, HashSet, VecDeque};

use super::package::PackageInfo;

pub struct DependencyTree {
    /// All packages by their qualified ID
    packages: HashMap<String, PackageInfo>,
    /// Forward edges: package -> what it depends on
    depends_on: HashMap<String, Vec<String>>,
    /// Reverse edges: package -> what depends on it
    depended_by: HashMap<String, Vec<String>>,
}

impl DependencyTree {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            depends_on: HashMap::new(),
            depended_by: HashMap::new(),
        }
    }

    pub fn build(packages: Vec<PackageInfo>) -> Self {
        let mut tree = Self::new();

        // Index all packages by real name AND by their virtual provides
        let mut name_to_id: HashMap<String, String> = HashMap::new();
        for pkg in &packages {
            let id = pkg.qualified_id();
            name_to_id.insert(pkg.name.clone(), id.clone());
            // Also index virtual provides: e.g. "python3" provided by "python"
            for prov in &pkg.provides {
                name_to_id.entry(prov.clone()).or_insert_with(|| id.clone());
            }
            tree.packages.insert(id, pkg.clone());
        }

        // Build forward edges (depends_on) using provides-aware lookup
        for pkg in &packages {
            let pkg_id = pkg.qualified_id();
            let mut forward = Vec::new();

            for dep_name in &pkg.depends {
                if let Some(dep_id) = name_to_id.get(dep_name) {
                    if dep_id != &pkg_id {
                        forward.push(dep_id.clone());
                    }
                }
            }

            tree.depends_on.insert(pkg_id, forward);
        }

        // Build reverse edges (depended_by) from pacman's Required By field,
        // which correctly resolves virtual packages / provides.
        // Fall back to inverting depends_on for non-pacman packages.
        for pkg in &packages {
            let pkg_id = pkg.qualified_id();

            if !pkg.required_by.is_empty() {
                // Use pacman's authoritative Required By data
                for parent_name in &pkg.required_by {
                    if let Some(parent_id) = name_to_id.get(parent_name) {
                        tree.depended_by
                            .entry(pkg_id.clone())
                            .or_default()
                            .push(parent_id.clone());
                    }
                }
            }
        }

        // For any package that has depends_on entries but the target has no
        // depended_by entry yet (non-pacman packages), fill in from forward edges
        let all_ids: Vec<String> = tree.packages.keys().cloned().collect();
        for pkg_id in &all_ids {
            if let Some(deps) = tree.depends_on.get(pkg_id) {
                for dep_id in deps.clone() {
                    let parents = tree.depended_by.entry(dep_id).or_default();
                    if !parents.contains(pkg_id) {
                        parents.push(pkg_id.clone());
                    }
                }
            }
        }

        tree
    }

    pub fn get(&self, id: &str) -> Option<&PackageInfo> {
        self.packages.get(id)
    }

    pub fn all_packages(&self) -> impl Iterator<Item = &PackageInfo> {
        self.packages.values()
    }

    /// Explicitly installed packages — the tree roots
    pub fn root_packages(&self) -> Vec<&PackageInfo> {
        self.packages
            .values()
            .filter(|p| p.is_explicit)
            .collect()
    }

    /// Direct dependencies of a package
    pub fn children_of(&self, id: &str) -> Vec<&PackageInfo> {
        self.depends_on
            .get(id)
            .map(|deps| {
                deps.iter()
                    .filter_map(|dep_id| self.packages.get(dep_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// What depends on this package
    pub fn parents_of(&self, id: &str) -> Vec<&PackageInfo> {
        self.depended_by
            .get(id)
            .map(|parents| {
                parents
                    .iter()
                    .filter_map(|pid| self.packages.get(pid))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Compute the full removal branch: given selected packages, find all
    /// transitive deps that would become orphans if the selected packages
    /// are removed. Only non-explicit (dependency-installed) packages are
    /// considered for orphan auto-selection — explicitly installed packages
    /// are never auto-added.
    pub fn compute_removal_branch(&self, selected_ids: &HashSet<String>) -> HashSet<String> {
        let mut to_remove = selected_ids.clone();
        let mut queue: VecDeque<String> = selected_ids.iter().cloned().collect();

        while let Some(pkg_id) = queue.pop_front() {
            // Look at deps of this package
            if let Some(deps) = self.depends_on.get(&pkg_id) {
                for dep_id in deps {
                    if to_remove.contains(dep_id) {
                        continue;
                    }

                    // Never auto-orphan explicitly installed packages —
                    // the user installed them on purpose.
                    if let Some(dep_pkg) = self.packages.get(dep_id) {
                        if dep_pkg.is_explicit {
                            continue;
                        }
                    }

                    // Check if this dep would become an orphan
                    // (all its reverse deps are being removed)
                    let would_be_orphan = self
                        .depended_by
                        .get(dep_id)
                        .map(|parents| parents.iter().all(|p| to_remove.contains(p)))
                        .unwrap_or(true);

                    if would_be_orphan {
                        to_remove.insert(dep_id.clone());
                        queue.push_back(dep_id.clone());
                    }
                }
            }
        }

        to_remove
    }

    /// Calculate total size of a set of packages
    pub fn total_size(&self, ids: &HashSet<String>) -> u64 {
        ids.iter()
            .filter_map(|id| self.packages.get(id))
            .map(|p| p.installed_size)
            .sum()
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Direct access to forward edges (package -> what it depends on).
    pub fn depends_on_map(&self) -> &HashMap<String, Vec<String>> {
        &self.depends_on
    }

    /// Direct access to reverse edges (package -> what depends on it).
    pub fn depended_by_map(&self) -> &HashMap<String, Vec<String>> {
        &self.depended_by
    }

    /// Mark packages as protected (cannot be selected for removal).
    /// `names` is a set of raw package names (not qualified IDs).
    pub fn mark_protected(&mut self, names: &HashSet<String>) {
        for pkg in self.packages.values_mut() {
            if names.contains(&pkg.name) {
                pkg.is_protected = true;
            }
        }
    }
}
