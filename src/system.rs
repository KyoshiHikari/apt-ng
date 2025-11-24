use anyhow::Result;
use std::fs;
use std::path::Path;

/// Erkennt die Debian-Version und gibt den Suite-Namen zurück
pub fn detect_debian_suite() -> Result<String> {
    // Versuche zuerst /etc/os-release
    let os_release = Path::new("/etc/os-release");
    if os_release.exists() {
        if let Ok(content) = fs::read_to_string(os_release) {
            for line in content.lines() {
                if line.starts_with("VERSION_CODENAME=") {
                    let suite = line.split('=').nth(1)
                        .map(|s| s.trim_matches('"').trim().to_string())
                        .unwrap_or_else(|| "stable".to_string());
                    return Ok(suite);
                }
            }
        }
    }
    
    // Versuche /etc/debian_version
    let debian_version = Path::new("/etc/debian_version");
    if debian_version.exists() {
        if let Ok(content) = fs::read_to_string(debian_version) {
            let version = content.trim();
            // Mappe bekannte Versionen zu Suite-Namen
            let suite = match version {
                v if v.starts_with("bookworm") => "bookworm",
                v if v.starts_with("bullseye") => "bullseye",
                v if v.starts_with("buster") => "buster",
                v if v.starts_with("stretch") => "stretch",
                v if v.starts_with("12") => "bookworm",
                v if v.starts_with("11") => "bullseye",
                v if v.starts_with("10") => "buster",
                v if v.starts_with("9") => "stretch",
                _ => "stable",
            };
            return Ok(suite.to_string());
        }
    }
    
    // Fallback: Versuche aus sources.list zu lesen
    let sources_list = Path::new("/etc/apt/sources.list");
    if sources_list.exists() {
        if let Ok(content) = fs::read_to_string(sources_list) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                
                // Parse erste deb-Zeile
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && (parts[0] == "deb" || parts[0] == "deb-src") {
                    let mut idx = 1;
                    // Überspringe [options]
                    if parts[idx].starts_with('[') {
                        while idx < parts.len() && !parts[idx].ends_with(']') {
                            idx += 1;
                        }
                        idx += 1;
                    }
                    if idx < parts.len() {
                        // Suite ist der Teil nach der URL
                        return Ok(parts[idx].to_string());
                    }
                }
            }
        }
    }
    
    // Fallback zu "stable"
    Ok("stable".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_debian_suite() {
        // Test sollte auf einem Debian-System funktionieren
        let suite = detect_debian_suite().unwrap();
        assert!(!suite.is_empty());
    }
}

