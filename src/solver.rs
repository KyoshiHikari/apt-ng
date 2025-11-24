use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::package::PackageManifest;
use crate::apt_parser::parse_dependency_rule;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct PackageSpec {
    pub name: String,
    pub version: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DependencyRule {
    pub name: String,
    pub version_constraint: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub provides: Vec<String>,
    pub depends: Vec<DependencyRule>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Solution {
    pub to_install: Vec<PackageInfo>,
    pub to_remove: Vec<String>,
    pub to_upgrade: Vec<PackageInfo>,
}

#[allow(dead_code)]
pub struct DependencySolver {
    packages: HashMap<String, Vec<PackageInfo>>,
}

impl DependencySolver {
    #[allow(dead_code)]
    pub fn new() -> Self {
        DependencySolver {
            packages: HashMap::new(),
        }
    }
    
    /// Convert PackageManifest to PackageInfo, parsing all dependency strings
    pub fn manifest_to_package_info(manifest: &PackageManifest) -> Result<PackageInfo> {
        // Parse depends strings into DependencyRule structs
        let mut depends_rules = Vec::new();
        for dep_str in &manifest.depends {
            // Each dependency string may contain multiple alternatives (separated by comma)
            // We need to parse each one
            let rules = parse_dependency_rule(dep_str)?;
            depends_rules.extend(rules);
        }
        
        // Parse conflicts (usually simple package names, but may have version constraints)
        let mut conflicts = Vec::new();
        for conflict_str in &manifest.conflicts {
            let rules = parse_dependency_rule(conflict_str)?;
            // For conflicts, we typically just need the package name
            for rule in rules {
                conflicts.push(rule.name);
            }
        }
        
        Ok(PackageInfo {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            arch: manifest.arch.clone(),
            provides: manifest.provides.clone(),
            depends: depends_rules,
            conflicts,
            replaces: manifest.replaces.clone(),
        })
    }
    
    /// Fügt ein Paket zum Solver hinzu
    #[allow(dead_code)]
    pub fn add_package(&mut self, pkg: PackageInfo) {
        self.packages
            .entry(pkg.name.clone())
            .or_insert_with(Vec::new)
            .push(pkg);
    }
    
    /// Löst Abhängigkeiten für die angeforderten Pakete
    #[allow(dead_code)]
    pub fn solve(&self, requested: &[PackageSpec]) -> Result<Solution> {
        let mut to_install = Vec::new();
        let mut visited = HashSet::new();
        let mut conflicts = Vec::new();
        
        for spec in requested {
            if let Some(packages) = self.packages.get(&spec.name) {
                // Wähle die passende Version
                let pkg = self.select_best_version(packages, spec)?;
                
                if !visited.contains(&pkg.name) {
                    self.resolve_dependencies(&pkg, &mut to_install, &mut visited, &mut conflicts)?;
                }
            } else {
                return Err(anyhow::anyhow!("Package not found: {}", spec.name));
            }
        }
        
        // Prüfe auf Konflikte
        if !conflicts.is_empty() {
            return Err(anyhow::anyhow!("Conflicts detected: {:?}", conflicts));
        }
        
        Ok(Solution {
            to_install,
            to_remove: Vec::new(),
            to_upgrade: Vec::new(),
        })
    }
    
    #[allow(dead_code)]
    fn select_best_version<'a>(&self, packages: &'a [PackageInfo], spec: &PackageSpec) -> Result<&'a PackageInfo> {
        // Filter packages by architecture if specified
        let mut candidates: Vec<&PackageInfo> = packages.iter()
            .filter(|p| {
                if let Some(ref arch) = spec.arch {
                    p.arch == *arch || p.arch == "all"
                } else {
                    true
                }
            })
            .collect();
        
        // If version constraint specified, filter by it
        if let Some(ref constraint) = spec.version {
            candidates = candidates.iter()
                .filter(|p| {
                    Self::version_matches(&p.version, constraint)
                })
                .copied()
                .collect();
        }
        
        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No matching package found for {} {}", spec.name, spec.version.as_deref().unwrap_or("any version")));
        }
        
        // Select newest version that matches constraints
        candidates.iter()
            .max_by(|a, b| Self::compare_versions(&a.version, &b.version))
            .copied()
            .ok_or_else(|| anyhow::anyhow!("No matching package found"))
    }
    
    /// Compare two Debian package versions
    /// Returns: Ordering::Less if v1 < v2, Ordering::Greater if v1 > v2, Ordering::Equal if v1 == v2
    pub fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
        // Simple version comparison - for production use, consider using debian-version crate
        // Format: [epoch:]upstream-version[-debian-revision]
        // This is a simplified implementation
        
        let parse_version = |v: &str| -> (u64, Vec<u64>, Vec<u64>) {
            // Split epoch
            let (epoch, rest) = if let Some(colon_pos) = v.find(':') {
                let e = v[..colon_pos].parse::<u64>().unwrap_or(0);
                (e, &v[colon_pos + 1..])
            } else {
                (0, v)
            };
            
            // Split upstream and debian revision
            let (upstream, debian) = if let Some(dash_pos) = rest.rfind('-') {
                (&rest[..dash_pos], &rest[dash_pos + 1..])
            } else {
                (rest, "")
            };
            
            // Parse upstream version (split by . and non-digit separators)
            let upstream_parts: Vec<u64> = upstream
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse::<u64>().ok())
                .collect();
            
            // Parse debian revision
            let debian_parts: Vec<u64> = debian
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse::<u64>().ok())
                .collect();
            
            (epoch, upstream_parts, debian_parts)
        };
        
        let (e1, u1, d1) = parse_version(v1);
        let (e2, u2, d2) = parse_version(v2);
        
        // Compare epoch
        match e1.cmp(&e2) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        
        // Compare upstream versions
        for (a, b) in u1.iter().zip(u2.iter()) {
            match a.cmp(b) {
                std::cmp::Ordering::Equal => {}
                other => return other,
            }
        }
        
        // If one has more parts, it's newer
        match u1.len().cmp(&u2.len()) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        
        // Compare debian revisions
        for (a, b) in d1.iter().zip(d2.iter()) {
            match a.cmp(b) {
                std::cmp::Ordering::Equal => {}
                other => return other,
            }
        }
        
        d1.len().cmp(&d2.len())
    }
    
    /// Check if a version matches a constraint
    fn version_matches(version: &str, constraint: &str) -> bool {
        // Parse constraint (format: ">= 1.0", "= 2.5", "<< 3.0", etc.)
        let constraint = constraint.trim();
        
        if constraint.starts_with(">=") {
            let req_version = constraint[2..].trim();
            Self::compare_versions(version, req_version) != std::cmp::Ordering::Less
        } else if constraint.starts_with("<=") {
            let req_version = constraint[2..].trim();
            Self::compare_versions(version, req_version) != std::cmp::Ordering::Greater
        } else if constraint.starts_with(">>") {
            let req_version = constraint[2..].trim();
            Self::compare_versions(version, req_version) == std::cmp::Ordering::Greater
        } else if constraint.starts_with("<<") {
            let req_version = constraint[2..].trim();
            Self::compare_versions(version, req_version) == std::cmp::Ordering::Less
        } else if constraint.starts_with(">") {
            let req_version = constraint[1..].trim();
            Self::compare_versions(version, req_version) == std::cmp::Ordering::Greater
        } else if constraint.starts_with("<") {
            let req_version = constraint[1..].trim();
            Self::compare_versions(version, req_version) == std::cmp::Ordering::Less
        } else if constraint.starts_with("=") {
            let req_version = constraint[1..].trim();
            Self::compare_versions(version, req_version) == std::cmp::Ordering::Equal
        } else {
            // No operator, treat as exact match
            Self::compare_versions(version, constraint) == std::cmp::Ordering::Equal
        }
    }
    
    #[allow(dead_code)]
    fn resolve_dependencies(
        &self,
        pkg: &PackageInfo,
        to_install: &mut Vec<PackageInfo>,
        visited: &mut HashSet<String>,
        conflicts: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(&pkg.name) {
            return Ok(());
        }
        
        visited.insert(pkg.name.clone());
        
        // Prüfe Konflikte
        for conflict in &pkg.conflicts {
            if to_install.iter().any(|p| p.name == *conflict) {
                conflicts.push(format!("{} conflicts with {}", pkg.name, conflict));
            }
        }
        
        // Löse Abhängigkeiten
        for dep in &pkg.depends {
            // Try to find package by name
            if let Some(packages) = self.packages.get(&dep.name) {
                let dep_pkg = self.select_best_version(packages, &PackageSpec {
                    name: dep.name.clone(),
                    version: dep.version_constraint.clone(),
                    arch: dep.arch.clone(),
                })?;
                
                self.resolve_dependencies(dep_pkg, to_install, visited, conflicts)?;
            } else {
                // Check if any package provides this dependency
                let mut found = false;
                for (_, pkgs) in &self.packages {
                    for pkg_candidate in pkgs {
                        if pkg_candidate.provides.contains(&dep.name) {
                            // Check version constraint if specified
                            if let Some(ref constraint) = dep.version_constraint {
                                if !Self::version_matches(&pkg_candidate.version, constraint) {
                                    continue;
                                }
                            }
                            self.resolve_dependencies(pkg_candidate, to_install, visited, conflicts)?;
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                
                if !found {
                    return Err(anyhow::anyhow!("Dependency not found: {}", dep.name));
                }
            }
        }
        
        // Füge Paket hinzu, wenn noch nicht vorhanden
        if !to_install.iter().any(|p| p.name == pkg.name) {
            to_install.push(pkg.clone());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_solver_basic() {
        let mut solver = DependencySolver::new();
        
        let pkg = PackageInfo {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            arch: "amd64".to_string(),
            provides: vec![],
            depends: vec![],
            conflicts: vec![],
            replaces: vec![],
        };
        
        solver.add_package(pkg);
        
        let solution = solver.solve(&[PackageSpec {
            name: "test-package".to_string(),
            version: None,
            arch: None,
        }]).unwrap();
        
        assert_eq!(solution.to_install.len(), 1);
    }
}

