use anyhow::Result;
use crate::package::PackageManifest;
use crate::solver::DependencyRule;
use std::collections::HashMap;

/// Parst eine apt Packages-Datei
pub fn parse_packages_file(content: &str) -> Result<Vec<PackageManifest>> {
    let mut packages = Vec::new();
    let mut current_pkg: Option<HashMap<String, String>> = None;
    
    for line in content.lines() {
        let line = line.trim();
        
        if line.is_empty() {
            // Leere Zeile markiert Ende eines Paket-Eintrags
            if let Some(pkg_data) = current_pkg.take() {
                if let Ok(manifest) = parse_package_entry(&pkg_data) {
                    packages.push(manifest);
                }
            }
            continue;
        }
        
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            
            if current_pkg.is_none() {
                current_pkg = Some(HashMap::new());
            }
            
            if let Some(ref mut pkg) = current_pkg {
                // Bei mehrzeiligen Feldern (z.B. Description) wird der Wert angehängt
                if pkg.contains_key(&key) {
                    let existing = pkg.get(&key).unwrap().clone();
                    pkg.insert(key, format!("{}\n{}", existing, value));
                } else {
                    pkg.insert(key, value);
                }
            }
        }
    }
    
    // Verarbeite letztes Paket falls Datei nicht mit Leerzeile endet
    if let Some(pkg_data) = current_pkg.take() {
        if let Ok(manifest) = parse_package_entry(&pkg_data) {
            packages.push(manifest);
        }
    }
    
    Ok(packages)
}

fn parse_package_entry(data: &HashMap<String, String>) -> Result<PackageManifest> {
    let name = data.get("Package")
        .ok_or_else(|| anyhow::anyhow!("Missing Package field"))?
        .clone();
    
    let version = data.get("Version")
        .ok_or_else(|| anyhow::anyhow!("Missing Version field"))?
        .clone();
    
    let arch = data.get("Architecture")
        .unwrap_or(&"all".to_string())
        .clone();
    
    // Parse Depends
    let depends = data.get("Depends")
        .map(|d| parse_depends(d))
        .unwrap_or_default();
    
    // Parse Provides
    let provides = data.get("Provides")
        .map(|p| parse_provides(p))
        .unwrap_or_default();
    
    let size = data.get("Size")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    
    let checksum = data.get("SHA256")
        .or_else(|| data.get("MD5sum"))
        .cloned()
        .unwrap_or_default();
    
    let filename = data.get("Filename").cloned();
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    Ok(PackageManifest {
        name,
        version,
        arch,
        provides,
        depends,
        conflicts: vec![],
        replaces: vec![],
        files: vec![],
        size,
        checksum,
        timestamp,
        filename,
        repo_id: None, // Wird später beim Hinzufügen zum Index gesetzt
    })
}

fn parse_depends(depends_str: &str) -> Vec<String> {
    depends_str
        .split(',')
        .map(|d| {
            // Entferne Version-Constraints (z.B. "libc (>= 2.0)" -> "libc")
            d.trim()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a dependency string into DependencyRule
/// Handles formats like:
/// - "package-name"
/// - "package-name (>= 1.0)"
/// - "package-name | alternative-package"
/// - "package-name (>= 1.0) | alternative-package"
pub fn parse_dependency_rule(dep_str: &str) -> Result<Vec<DependencyRule>> {
    let mut rules = Vec::new();
    
    // Split by pipe (|) for alternatives
    let alternatives: Vec<&str> = dep_str.split('|').map(|s| s.trim()).collect();
    
    for alt in alternatives {
        let alt = alt.trim();
        if alt.is_empty() {
            continue;
        }
        
        // Check for version constraint in parentheses
        let (name, version_constraint) = if let Some(open_paren) = alt.find('(') {
            let name = alt[..open_paren].trim().to_string();
            if let Some(close_paren) = alt[open_paren..].find(')') {
                let constraint_str = alt[open_paren + 1..open_paren + close_paren].trim();
                let version_constraint = parse_version_constraint(constraint_str)?;
                (name, version_constraint)
            } else {
                // Malformed parentheses, treat as package name
                (alt.to_string(), None)
            }
        } else {
            (alt.to_string(), None)
        };
        
        if !name.is_empty() {
            rules.push(DependencyRule {
                name,
                version_constraint,
                arch: None, // Architecture constraints are rare in Debian dependencies
            });
        }
    }
    
    Ok(rules)
}

/// Parse version constraint string (e.g., ">= 1.0", "= 2.5", "<< 3.0")
fn parse_version_constraint(constraint: &str) -> Result<Option<String>> {
    let constraint = constraint.trim();
    if constraint.is_empty() {
        return Ok(None);
    }
    
    // Supported operators: >=, <=, =, >>, <<, >, <
    let operators = ["<<", ">>", ">=", "<=", "=", ">", "<"];
    
    for op in &operators {
        if constraint.starts_with(op) {
            let version = constraint[op.len()..].trim();
            if !version.is_empty() {
                return Ok(Some(format!("{} {}", op, version)));
            }
        }
    }
    
    // If no operator found, treat entire string as version
    Ok(Some(constraint.to_string()))
}

fn parse_provides(provides_str: &str) -> Vec<String> {
    provides_str
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_packages_file() {
        let content = r#"Package: test-package
Version: 1.0.0
Architecture: amd64
Depends: libc6 (>= 2.0), libssl1.1
Provides: test-tool
Size: 1024
SHA256: abc123

Package: another-package
Version: 2.0.0
Architecture: all
Size: 2048
"#;
        
        let packages = parse_packages_file(content).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "test-package");
        assert_eq!(packages[0].depends.len(), 2);
    }
    
    #[test]
    fn test_parse_dependency_rule() {
        // Simple package name
        let rules = parse_dependency_rule("libc6").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "libc6");
        assert!(rules[0].version_constraint.is_none());
        
        // Package with version constraint
        let rules = parse_dependency_rule("libc6 (>= 2.0)").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "libc6");
        assert_eq!(rules[0].version_constraint.as_ref().unwrap(), ">= 2.0");
        
        // Alternatives
        let rules = parse_dependency_rule("libssl1.1 | libssl1.0").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "libssl1.1");
        assert_eq!(rules[1].name, "libssl1.0");
        
        // Complex: alternatives with version constraints
        let rules = parse_dependency_rule("libc6 (>= 2.0) | libc5").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "libc6");
        assert_eq!(rules[0].version_constraint.as_ref().unwrap(), ">= 2.0");
        assert_eq!(rules[1].name, "libc5");
        
        // Different operators
        let rules = parse_dependency_rule("package (<< 3.0)").unwrap();
        assert_eq!(rules[0].version_constraint.as_ref().unwrap(), "<< 3.0");
        
        let rules = parse_dependency_rule("package (>> 1.0)").unwrap();
        assert_eq!(rules[0].version_constraint.as_ref().unwrap(), ">> 1.0");
        
        let rules = parse_dependency_rule("package (= 2.5)").unwrap();
        assert_eq!(rules[0].version_constraint.as_ref().unwrap(), "= 2.5");
    }
    
    #[test]
    fn test_parse_version_constraint() {
        assert_eq!(parse_version_constraint(">= 1.0").unwrap(), Some(">= 1.0".to_string()));
        assert_eq!(parse_version_constraint("<= 2.0").unwrap(), Some("<= 2.0".to_string()));
        assert_eq!(parse_version_constraint("= 1.5").unwrap(), Some("= 1.5".to_string()));
        assert_eq!(parse_version_constraint("<< 3.0").unwrap(), Some("<< 3.0".to_string()));
        assert_eq!(parse_version_constraint(">> 0.5").unwrap(), Some(">> 0.5".to_string()));
        assert_eq!(parse_version_constraint("").unwrap(), None);
    }
}

