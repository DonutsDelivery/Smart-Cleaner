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

        // Index all packages by name (within same source prefix)
        let mut name_to_id: HashMap<String, String> = HashMap::new();
        for pkg in &packages {
            let id = pkg.qualified_id();
            name_to_id.insert(pkg.name.clone(), id.clone());
            tree.packages.insert(id, pkg.clone());
        }

        // Build edges
        for pkg in &packages {
            let pkg_id = pkg.qualified_id();
            let mut forward = Vec::new();

            for dep_name in &pkg.depends {
                // Strip version constraints (e.g., "glibc>=2.38" -> "glibc")
                let dep_base = dep_name.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '.')
                    .next()
                    .unwrap_or(dep_name);

                if let Some(dep_id) = name_to_id.get(dep_base) {
                    forward.push(dep_id.clone());
                    tree.depended_by
                        .entry(dep_id.clone())
                        .or_default()
                        .push(pkg_id.clone());
                }
            }

            tree.depends_on.insert(pkg_id, forward);
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
    /// are removed.
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
}
