use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: Option<i64>,
    pub url: String,
    pub priority: i32,
    pub enabled: bool,
    pub last_probe_ms: Option<u64>,
    pub rtt_ms: Option<u64>,
    pub suite: Option<String>,
    pub components: Vec<String>,
}

impl Repository {
    /// Fügt ein Repository zur Datenbank hinzu
    pub fn add_to_db(conn: &Connection, repo: &Repository) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO repos (url, priority, last_probe_ms, rtt_ms, enabled, suite, components)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                repo.url,
                repo.priority,
                repo.last_probe_ms,
                repo.rtt_ms,
                if repo.enabled { 1 } else { 0 },
                repo.suite.as_ref(),
                serde_json::to_string(&repo.components).ok()
            ],
        )?;
        Ok(())
    }
    
    /// Entfernt ein Repository aus der Datenbank
    #[allow(dead_code)]
    pub fn remove_from_db(conn: &Connection, url: &str) -> Result<()> {
        conn.execute("DELETE FROM repos WHERE url = ?1", [url])?;
        Ok(())
    }
    
    /// Lädt alle Repositories aus der Datenbank
    pub fn load_all(conn: &Connection) -> Result<Vec<Repository>> {
        let mut stmt = conn.prepare(
            "SELECT id, url, priority, last_probe_ms, rtt_ms, enabled, suite, components FROM repos WHERE enabled = 1 ORDER BY priority ASC, rtt_ms ASC"
        )?;
        
        let repos = stmt.query_map([], |row| {
            let components_str: Option<String> = row.get(7)?;
            let components = components_str
                .map(|s| serde_json::from_str(&s).unwrap_or_default())
                .unwrap_or_default();
            
            Ok(Repository {
                id: row.get(0)?,
                url: row.get(1)?,
                priority: row.get(2)?,
                enabled: row.get::<_, i32>(5)? != 0,
                last_probe_ms: row.get(3)?,
                rtt_ms: row.get(4)?,
                suite: row.get(6)?,
                components,
            })
        })?;
        
        let mut result = Vec::new();
        for repo in repos {
            result.push(repo?);
        }
        Ok(result)
    }
    
    /// Wählt das beste Repository basierend auf Performance aus
    #[allow(dead_code)]
    pub fn select_best_mirror(conn: &Connection, base_url: &str) -> Result<Option<Repository>> {
        // Finde alle Repositories mit ähnlicher Base-URL (verschiedene Mirrors)
        let mut stmt = conn.prepare(
            "SELECT id, url, priority, last_probe_ms, rtt_ms, enabled, suite, components 
             FROM repos 
             WHERE enabled = 1 AND url LIKE ?1
             ORDER BY priority ASC, rtt_ms ASC, last_probe_ms DESC
             LIMIT 1"
        )?;
        
        let pattern = format!("{}%", base_url);
        let result = stmt.query_row([&pattern], |row| {
            let components_str: Option<String> = row.get(7)?;
            let components = components_str
                .map(|s| serde_json::from_str(&s).unwrap_or_default())
                .unwrap_or_default();
            
            Ok(Repository {
                id: row.get(0)?,
                url: row.get(1)?,
                priority: row.get(2)?,
                enabled: row.get::<_, i32>(5)? != 0,
                last_probe_ms: row.get(3)?,
                rtt_ms: row.get(4)?,
                suite: row.get(6)?,
                components,
            })
        });
        
        match result {
            Ok(repo) => Ok(Some(repo)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Aktualisiert die Probe-Statistiken eines Repositories
    pub fn update_probe_stats(
        conn: &Connection,
        url: &str,
        rtt_ms: u64,
    ) -> Result<()> {
        conn.execute(
            "UPDATE repos SET last_probe_ms = ?1, rtt_ms = ?2 WHERE url = ?3",
            rusqlite::params![
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                rtt_ms,
                url
            ],
        )?;
        Ok(())
    }
    
    /// Importiert apt/apt-get Repositories aus /etc/apt/sources.list und sources.list.d/
    pub fn import_apt_repos(conn: &Connection) -> Result<usize> {
        let mut imported = 0;
        
        // Prüfe ob bereits Repositories vorhanden sind
        let existing_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM repos",
            [],
            |row| row.get(0)
        )?;
        
        if existing_count > 0 {
            // Bereits importiert, überspringe
            return Ok(0);
        }
        
        // Lese /etc/apt/sources.list
        let sources_list = Path::new("/etc/apt/sources.list");
        if sources_list.exists() {
            if let Ok(content) = fs::read_to_string(sources_list) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    
                    if let Some(repo) = Self::parse_apt_line(line) {
                        Self::add_to_db(conn, &repo)?;
                        imported += 1;
                    }
                }
            }
        }
        
        // Lese /etc/apt/sources.list.d/*.list
        let sources_list_d = Path::new("/etc/apt/sources.list.d");
        if sources_list_d.exists() {
            if let Ok(entries) = fs::read_dir(sources_list_d) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("list") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                for line in content.lines() {
                                    let line = line.trim();
                                    if line.is_empty() || line.starts_with('#') {
                                        continue;
                                    }
                                    
                                    if let Some(repo) = Self::parse_apt_line(line) {
                                        Self::add_to_db(conn, &repo)?;
                                        imported += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(imported)
    }
    
    /// Parst eine Zeile aus sources.list
    fn parse_apt_line(line: &str) -> Option<Repository> {
        // Format: deb [options] uri suite [component1] [component2] [...]
        // oder: deb-src [options] uri suite [component1] [component2] [...]
        
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }
        
        let mut idx = 0;
        
        // Überspringe deb/deb-src (nur deb, nicht deb-src)
        if parts[idx] == "deb-src" {
            return None; // Nur binary packages
        }
        if parts[idx] != "deb" {
            return None;
        }
        idx += 1;
        
        // Überspringe [options] falls vorhanden
        if parts[idx].starts_with('[') {
            while idx < parts.len() && !parts[idx].ends_with(']') {
                idx += 1;
            }
            idx += 1;
        }
        
        if idx >= parts.len() {
            return None;
        }
        
        // URI ist der nächste Teil
        let uri = parts[idx].to_string();
        
        // Konvertiere apt-URI zu HTTP-URL falls nötig
        let url = if uri.starts_with("http://") || uri.starts_with("https://") {
            uri
        } else if uri.starts_with("file://") {
            // Lokale Repositories werden übersprungen
            return None;
        } else {
            // cdrom und andere werden übersprungen
            return None;
        };
        
        idx += 1;
        if idx >= parts.len() {
            return None;
        }
        
        // Suite ist der nächste Teil
        let suite = parts[idx].to_string();
        idx += 1;
        
        // Components sind die restlichen Teile
        let components: Vec<String> = if idx < parts.len() {
            parts[idx..].iter().map(|s| s.to_string()).collect()
        } else {
            vec!["main".to_string()] // Default component
        };
        
        Some(Repository {
            id: None,
            url,
            priority: 500,
            enabled: true,
            last_probe_ms: None,
            rtt_ms: None,
            suite: Some(suite),
            components,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    
    #[test]
    fn test_repo_add() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE repos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                priority INTEGER DEFAULT 500,
                last_probe_ms INTEGER,
                rtt_ms INTEGER,
                enabled INTEGER DEFAULT 1,
                suite TEXT,
                components TEXT
            )",
            [],
        ).unwrap();
        
        let repo = Repository {
            id: None,
            url: "https://example.com/repo".to_string(),
            priority: 500,
            enabled: true,
            last_probe_ms: None,
            rtt_ms: None,
            suite: Some("stable".to_string()),
            components: vec!["main".to_string()],
        };
        
        Repository::add_to_db(&conn, &repo).unwrap();
        let repos = Repository::load_all(&conn).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].url, "https://example.com/repo");
    }
    
    #[test]
    fn test_parse_apt_line() {
        let repo = Repository::parse_apt_line("deb https://deb.debian.org/debian bookworm main").unwrap();
        assert_eq!(repo.url, "https://deb.debian.org/debian");
        
        let repo = Repository::parse_apt_line("deb [arch=amd64] https://deb.debian.org/debian bookworm main").unwrap();
        assert_eq!(repo.url, "https://deb.debian.org/debian");
        
        assert!(Repository::parse_apt_line("deb file:///mnt/cdrom").is_none());
    }
}
