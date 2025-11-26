use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::{Arc, Mutex};
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
    installed_packages: HashSet<String>,
    installed_provides: HashMap<String, Vec<String>>, // Maps dependency name to list of installed packages that provide it
}

impl DependencySolver {
    #[allow(dead_code)]
    pub fn new() -> Self {
        DependencySolver {
            packages: HashMap::new(),
            installed_packages: HashSet::new(),
            installed_provides: HashMap::new(),
        }
    }
    
    /// Set the list of already-installed packages
    /// Dependencies satisfied by these packages will be skipped during resolution
    #[allow(dead_code)]
    pub fn set_installed_packages(&mut self, installed: HashSet<String>) {
        self.installed_packages = installed;
        // Rebuild installed_provides map
        self.installed_provides.clear();
        for (pkg_name, pkgs) in &self.packages {
            if self.installed_packages.contains(pkg_name) {
                for pkg in pkgs {
                    // Every package provides its own name
                    self.installed_provides
                        .entry(pkg.name.clone())
                        .or_insert_with(Vec::new)
                        .push(pkg.name.clone());
                    
                    // Add explicit provides
                    for provided in &pkg.provides {
                        self.installed_provides
                            .entry(provided.clone())
                            .or_insert_with(Vec::new)
                            .push(pkg.name.clone());
                    }
                }
            }
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
        let is_installed = self.installed_packages.contains(&pkg.name);
        
        self.packages
            .entry(pkg.name.clone())
            .or_insert_with(Vec::new)
            .push(pkg.clone());
        
        // Update installed_provides if this is an installed package
        if is_installed {
            // Every package provides its own name
            self.installed_provides
                .entry(pkg.name.clone())
                .or_insert_with(Vec::new)
                .push(pkg.name.clone());
            
            // Add explicit provides
            for provided in &pkg.provides {
                self.installed_provides
                    .entry(provided.clone())
                    .or_insert_with(Vec::new)
                    .push(pkg.name.clone());
            }
        }
    }
    
    /// Löst Abhängigkeiten für die angeforderten Pakete
    #[allow(dead_code)]
    pub fn solve(&self, requested: &[PackageSpec]) -> Result<Solution> {
        self.solve_parallel(requested, false)
    }
    
    /// Löst Abhängigkeiten für die angeforderten Pakete mit optionaler Parallelisierung
    /// 
    /// # Arguments
    /// * `requested` - Liste der angeforderten Pakete
    /// * `use_parallel` - Wenn true, verwendet parallele Verarbeitung für Dependency-Resolution
    /// 
    /// # Parallelisierung
    /// Wenn `use_parallel` aktiviert ist, werden mehrere Dependency-Resolutionen parallel durchgeführt.
    /// Dies kann die Performance bei großen Dependency-Graphen verbessern.
    pub fn solve_parallel(&self, requested: &[PackageSpec], use_parallel: bool) -> Result<Solution> {
        if use_parallel {
            self.solve_parallel_impl(requested)
        } else {
            self.solve_sequential(requested)
        }
    }
    
    /// Sequenzielle Dependency-Resolution (Standard)
    fn solve_sequential(&self, requested: &[PackageSpec]) -> Result<Solution> {
        let mut to_install = Vec::new();
        let mut visited = HashSet::new();
        let mut conflicts = Vec::new();
        
        for spec in requested {
            if let Some(packages) = self.packages.get(&spec.name) {
                // Wähle die passende Version
                let pkg = self.select_best_version(packages, spec)?;
                
                // Always resolve dependencies for requested packages, even if already installed
                // This ensures upgrades are handled correctly
                if !visited.contains(&pkg.name) {
                    self.resolve_dependencies(&pkg, &mut to_install, &mut visited, &mut conflicts)?;
                } else {
                    // Package was already visited (as a dependency), but we still need to add it
                    // if it was explicitly requested and not already in to_install
                    if !to_install.iter().any(|p| p.name == pkg.name) {
                        to_install.push(pkg.clone());
                    }
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
    
    /// Parallele Dependency-Resolution mit rayon
    fn solve_parallel_impl(&self, requested: &[PackageSpec]) -> Result<Solution> {
        use rayon::prelude::*;
        
        // Thread-safe Collections für parallele Zugriffe
        let to_install = Arc::new(Mutex::new(Vec::new()));
        let visited = Arc::new(Mutex::new(HashSet::new()));
        let conflicts = Arc::new(Mutex::new(Vec::new()));
        
        // Parallele Verarbeitung der angeforderten Pakete
        let results: Result<Vec<()>> = requested.par_iter()
            .map(|spec| {
                if let Some(packages) = self.packages.get(&spec.name) {
                    // Wähle die passende Version
                    let pkg = self.select_best_version(packages, spec)?;
                    
                    // Prüfe ob bereits besucht
                    let visited_guard = visited.lock().unwrap();
                    if !visited_guard.contains(&pkg.name) {
                        drop(visited_guard);
                        
                        // Resolve dependencies (thread-safe)
                        self.resolve_dependencies_parallel(
                            &pkg,
                            &to_install,
                            &visited,
                            &conflicts,
                        )?;
                    } else {
                        // Package was already visited, but we still need to add it if requested
                        let mut to_install_guard = to_install.lock().unwrap();
                        if !to_install_guard.iter().any(|p| p.name == pkg.name) {
                            to_install_guard.push(pkg.clone());
                        }
                    }
                    
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Package not found: {}", spec.name))
                }
            })
            .collect();
        
        results?;
        
        // Sammle Ergebnisse
        let to_install = Arc::try_unwrap(to_install).unwrap().into_inner().unwrap();
        let conflicts = Arc::try_unwrap(conflicts).unwrap().into_inner().unwrap();
        
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
    
    /// Parallele Version von resolve_dependencies mit thread-safe Collections
    fn resolve_dependencies_parallel(
        &self,
        pkg: &PackageInfo,
        to_install: &Arc<Mutex<Vec<PackageInfo>>>,
        visited: &Arc<Mutex<HashSet<String>>>,
        conflicts: &Arc<Mutex<Vec<String>>>,
    ) -> Result<()> {
        // Prüfe ob bereits besucht
        {
            let mut visited_guard = visited.lock().unwrap();
            if visited_guard.contains(&pkg.name) {
                return Ok(());
            }
            visited_guard.insert(pkg.name.clone());
        }
        
        // Prüfe Konflikte
        {
            let to_install_guard = to_install.lock().unwrap();
            for conflict in &pkg.conflicts {
                if to_install_guard.iter().any(|p| p.name == *conflict) {
                    let mut conflicts_guard = conflicts.lock().unwrap();
                    conflicts_guard.push(format!("{} conflicts with {}", pkg.name, conflict));
                }
            }
        }
        
        // Parallele Verarbeitung der Dependencies mit rayon
        use rayon::prelude::*;
        
        let dep_results: Result<Vec<()>> = pkg.depends.par_iter()
            .map(|dep| {
                // Check if dependency is already satisfied by an installed package
                if self.is_dependency_satisfied_by_installed(dep) {
                    return Ok(()); // Skip this dependency
                }
                
                // Try to find package by name
                if let Some(packages) = self.packages.get(&dep.name) {
                    let dep_pkg = self.select_best_version(packages, &PackageSpec {
                        name: dep.name.clone(),
                        version: dep.version_constraint.clone(),
                        arch: dep.arch.clone(),
                    })?;
                    
                    self.resolve_dependencies_parallel(dep_pkg, to_install, visited, conflicts)?;
                } else {
                    // Check if any package provides this dependency
                    // Parallele Suche durch alle Pakete
                    let mut found = false;
                    let packages_vec: Vec<_> = self.packages.iter().collect();
                    
                    for (_, pkgs) in &packages_vec {
                        for pkg_candidate in pkgs.iter() {
                            let provides_dep = pkg_candidate.name == dep.name || 
                                              pkg_candidate.provides.contains(&dep.name);
                            
                            if provides_dep {
                                // Check version constraint if specified
                                if let Some(ref constraint) = dep.version_constraint {
                                    if !Self::version_matches(&pkg_candidate.version, constraint) {
                                        continue;
                                    }
                                }
                                self.resolve_dependencies_parallel(pkg_candidate, to_install, visited, conflicts)?;
                                found = true;
                                break;
                            }
                        }
                        if found {
                            break;
                        }
                    }
                    
                    if !found {
                        // Last resort: check if dependency is satisfied by a system package
                        if Self::is_package_installed_on_system(&dep.name) || 
                           Self::is_dependency_provided_by_system(&dep.name) {
                            return Ok(()); // Dependency satisfied by system package
                        }
                        
                        return Err(anyhow::anyhow!("Dependency not found: {}", dep.name));
                    }
                }
                
                Ok(())
            })
            .collect();
        
        dep_results?;
        
        // Füge Paket hinzu, wenn noch nicht vorhanden
        {
            let mut to_install_guard = to_install.lock().unwrap();
            if !to_install_guard.iter().any(|p| p.name == pkg.name) {
                to_install_guard.push(pkg.clone());
            }
        }
        
        Ok(())
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
    
    /// Check if a package is installed on the system (via dpkg)
    /// Get version of a system package using dpkg -l
    fn get_system_package_version(package_name: &str) -> Option<String> {
        use std::process::Command;
        
        // Use dpkg-query to get version
        let output = Command::new("dpkg-query")
            .arg("-W")
            .arg("-f=${Version}")
            .arg(package_name)
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !version.is_empty() {
                    Some(version)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    
    fn is_package_installed_on_system(package_name: &str) -> bool {
        // Check if package is installed using dpkg-query
        let output = Command::new("dpkg-query")
            .arg("-W")
            .arg("-f=${Status}")
            .arg(package_name)
            .output();
        
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Status should contain "installed" for installed packages
                return stdout.contains("installed");
            }
        }
        
        // Fallback: try dpkg -l
        let output = Command::new("dpkg")
            .arg("-l")
            .arg(package_name)
            .output();
        
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // dpkg -l output format: "ii  package-name  version  description"
            // "ii" means installed and configured
            stdout.lines().any(|line| {
                line.starts_with("ii") && line.split_whitespace().nth(1) == Some(package_name)
            })
        } else {
            false
        }
    }
    
    /// Check if any system package provides a dependency (via apt-cache)
    fn is_dependency_provided_by_system(dep_name: &str) -> bool {
        // First, try dpkg-query to check if any installed package provides this
        // This is faster and more reliable than apt-cache
        let output = Command::new("dpkg-query")
            .arg("-W")
            .arg("-f=${Package} ${Provides}\n")
            .output();
        
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let package_name = parts[0];
                        // Check if this package provides the dependency
                        for part in parts.iter().skip(1) {
                            // Provides format: "libqt5core5t64 (= 5.15.13+dfsg-1)" or just "libqt5core5t64"
                            let provided = part.split('(').next().unwrap_or(part).trim();
                            if provided == dep_name {
                                if Self::is_package_installed_on_system(package_name) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback: Use apt-cache to find packages that provide this dependency
        let output = Command::new("apt-cache")
            .arg("showpkg")
            .arg(dep_name)
            .output();
        
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // apt-cache showpkg shows providers in the output
                // Look for "Reverse Provides:" section
                let mut in_reverse_provides = false;
                for line in stdout.lines() {
                    if line.starts_with("Reverse Provides:") {
                        in_reverse_provides = true;
                        continue;
                    }
                    if in_reverse_provides {
                        if line.trim().is_empty() {
                            break;
                        }
                        // Check if any provider is installed
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if !parts.is_empty() {
                            let provider_name = parts[0];
                            if Self::is_package_installed_on_system(provider_name) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        
        false
    }
    
    /// Check if a dependency is satisfied by an already-installed package
    fn is_dependency_satisfied_by_installed(&self, dep: &DependencyRule) -> bool {
        // Check if dependency name matches an installed package name directly
        if self.installed_packages.contains(&dep.name) {
            // If version constraint specified, we need to check versions
            if let Some(ref constraint) = dep.version_constraint {
                if let Some(pkgs) = self.packages.get(&dep.name) {
                    for pkg in pkgs {
                        if Self::version_matches(&pkg.version, constraint) {
                            return true;
                        }
                    }
                    return false; // Version constraint not satisfied
                }
            }
            return true; // No version constraint, installed package satisfies
        }
        
        // Check if any installed package provides this dependency
        if let Some(providers) = self.installed_provides.get(&dep.name) {
            if !providers.is_empty() {
                // If version constraint specified, we need to check versions
                if let Some(ref constraint) = dep.version_constraint {
                    // Find the providing package and check its version
                    for provider_name in providers {
                        if let Some(pkgs) = self.packages.get(provider_name) {
                            for pkg in pkgs {
                                if Self::version_matches(&pkg.version, constraint) {
                                    return true;
                                }
                            }
                        }
                    }
                } else {
                    // No version constraint, any provider is fine
                    return true;
                }
            }
        }
        
        // Check if dependency is satisfied by a system package (not managed by apt-ng)
        // This handles cases where packages are installed via apt/dpkg but not tracked by apt-ng
        if Self::is_package_installed_on_system(&dep.name) {
            // Check version constraint if specified
            if let Some(ref constraint) = dep.version_constraint {
                if let Some(installed_version) = Self::get_system_package_version(&dep.name) {
                    if !Self::version_matches(&installed_version, constraint) {
                        return false; // Version constraint not satisfied
                    }
                }
            }
            return true; // Package is installed and version matches (if constraint specified)
        }
        
        // Check if any system package provides this dependency
        if Self::is_dependency_provided_by_system(&dep.name) {
            return true;
        }
        
        false
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
            // Check if dependency is already satisfied by an installed package
            if self.is_dependency_satisfied_by_installed(dep) {
                continue; // Skip this dependency, it's already satisfied
            }
            
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
                // In Debian, every package implicitly provides its own name
                let mut found = false;
                for (_, pkgs) in &self.packages {
                    for pkg_candidate in pkgs {
                        // Check if package name matches dependency (implicit provide)
                        let provides_dep = pkg_candidate.name == dep.name || 
                                          pkg_candidate.provides.contains(&dep.name);
                        
                        if provides_dep {
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
                    // Last resort: check if dependency is satisfied by a system package
                    // This handles cases where packages are installed via apt/dpkg but not tracked by apt-ng
                    if Self::is_package_installed_on_system(&dep.name) || 
                       Self::is_dependency_provided_by_system(&dep.name) {
                        // Dependency is satisfied by system package, skip it
                        continue;
                    }
                    
                    // Try to find similar package names that might satisfy this dependency
                    // This handles transitional packages (e.g., libqt5core5t64 -> libqt5core5a)
                    // Simple approach: find packages that start with a common prefix
                    // For "libqt5core5t64", look for packages starting with "libqt5core5"
                    let mut similar_packages = Vec::new();
                    
                    // Try different base name extraction strategies
                    let mut bases = Vec::new();
                    
                    // Strategy 1: Remove trailing alphanumeric: "libqt5core5t64" -> "libqt5core5"
                    bases.push(dep.name.trim_end_matches(|c: char| c.is_ascii_alphanumeric() && c != '5'));
                    
                    // Strategy 2: Remove trailing digits and letters: "libqt5core5t64" -> "libqt5core5"
                    bases.push(dep.name.trim_end_matches(|c: char| c.is_ascii_alphabetic()));
                    
                    // Strategy 3: Use first part before last digit sequence
                    let mut base_str = dep.name.clone();
                    while base_str.len() > 5 && base_str.chars().last().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false) {
                        base_str.pop();
                    }
                    bases.push(&base_str);
                    
                    for dep_base in bases {
                        if dep_base.len() < 5 {
                            continue; // Skip too short bases
                        }
                        
                        // Look for packages that start with the base name
                        for (pkg_name, pkgs) in &self.packages {
                            if pkg_name.starts_with(dep_base) && *pkg_name != dep.name {
                                for pkg in pkgs {
                                    similar_packages.push((pkg_name.clone(), pkg.clone()));
                                    break;
                                }
                            }
                        }
                        
                        if !similar_packages.is_empty() {
                            break; // Found similar packages, stop searching
                        }
                    }
                    
                    // If we found similar packages, try to use the first one
                    if !similar_packages.is_empty() {
                        let (_similar_name, similar_pkg) = &similar_packages[0];
                        // Check version constraint if specified
                        let mut version_ok = true;
                        if let Some(ref constraint) = dep.version_constraint {
                            version_ok = Self::version_matches(&similar_pkg.version, constraint);
                        }
                        
                        if version_ok {
                            // Use the similar package as a substitute
                            self.resolve_dependencies(similar_pkg, to_install, visited, conflicts)?;
                            continue;
                        }
                    }
                    
                    // Try to find packages that provide this dependency for better error message
                    let mut providers = Vec::new();
                    let mut installed_providers = Vec::new();
                    
                    for (pkg_name, pkgs) in &self.packages {
                        for pkg in pkgs {
                            if pkg.provides.contains(&dep.name) || pkg.name == dep.name {
                                if self.installed_packages.contains(pkg_name) {
                                    installed_providers.push(format!("{} (installed)", pkg_name));
                                } else {
                                    providers.push(pkg_name.clone());
                                }
                                break;
                            }
                        }
                    }
                    
                    let mut error_msg = format!("Dependency not found: {}", dep.name);
                    if !installed_providers.is_empty() {
                        error_msg.push_str(&format!(" (installed providers: {})", installed_providers.join(", ")));
                    }
                    if !providers.is_empty() {
                        error_msg.push_str(&format!(" (available providers: {})", providers.join(", ")));
                    }
                    if !similar_packages.is_empty() {
                        error_msg.push_str(&format!(" (similar packages found: {})", similar_packages.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join(", ")));
                    }
                    
                    return Err(anyhow::anyhow!(error_msg));
                }
            }
        }
        
        // Füge Paket hinzu, wenn noch nicht vorhanden
        // Always add requested packages, even if already installed (needed for upgrades)
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

