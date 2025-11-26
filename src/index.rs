use rusqlite::{Connection, Result as SqliteResult};
use anyhow::Result;
use crate::package::PackageManifest;

pub struct Index {
    conn: Connection,
}

impl Index {
    /// Erstellt oder öffnet eine neue Index-Datenbank
    pub fn new(db_path: &str) -> Result<Self> {
        // Stelle sicher, dass das Verzeichnis existiert
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        let index = Index { conn };
        index.init_schema()?;
        index.optimize_for_bulk_inserts()?;
        Ok(index)
    }
    
    /// Optimiert SQLite für Bulk-Inserts (schnelleres Indexing)
    fn optimize_for_bulk_inserts(&self) -> SqliteResult<()> {
        // WAL-Mode für bessere Concurrency und Performance
        // Verwende execute_batch für PRAGMA-Befehle, die Werte zurückgeben können
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA cache_size = -10000;"
        )?;
        
        // Deaktiviere Foreign Keys während Bulk-Inserts (wird später wieder aktiviert)
        // self.conn.execute("PRAGMA foreign_keys = OFF", [])?; // Nur wenn nötig
        
        // Erhöhe Page Size für bessere Performance bei großen Datenmengen
        // self.conn.execute("PRAGMA page_size = 4096", [])?; // Nur beim Erstellen der DB
        
        Ok(())
    }
    
    /// Aktiviert Bulk-Insert-Modus (deaktiviert Indizes temporär)
    pub fn begin_bulk_insert(&self) -> Result<()> {
        // Deaktiviere Indizes temporär für schnelleres Inserting
        self.conn.execute("DROP INDEX IF EXISTS idx_packages_name", [])?;
        self.conn.execute("DROP INDEX IF EXISTS idx_packages_timestamp", [])?;
        
        // Setze synchronous auf OFF für maximale Geschwindigkeit während Bulk-Inserts
        // Verwende execute_batch für PRAGMA-Befehle
        self.conn.execute_batch("PRAGMA synchronous = OFF")?;
        
        Ok(())
    }
    
    /// Beendet Bulk-Insert-Modus (reaktiviert Indizes)
    pub fn end_bulk_insert(&self) -> Result<()> {
        // Reaktiviere synchronous
        // Verwende execute_batch für PRAGMA-Befehle
        self.conn.execute_batch("PRAGMA synchronous = NORMAL")?;
        
        // Reaktiviere Indizes
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_timestamp ON packages(timestamp)",
            [],
        )?;
        
        Ok(())
    }
    
    /// Initialisiert das Datenbank-Schema
    fn init_schema(&self) -> SqliteResult<()> {
                self.conn.execute(
                    "CREATE TABLE IF NOT EXISTS packages (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        name TEXT NOT NULL,
                        version TEXT NOT NULL,
                        arch TEXT NOT NULL,
                        provides TEXT,
                        depends TEXT,
                        size INTEGER,
                        checksum TEXT,
                        repo_id INTEGER,
                        timestamp INTEGER,
                        filename TEXT,
                        UNIQUE(name, version, arch)
                    )",
                    [],
                )?;
        
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS repos (
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
        )?;
        
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS installed (
                pkg_id INTEGER PRIMARY KEY,
                install_time INTEGER NOT NULL,
                manifest TEXT,
                FOREIGN KEY(pkg_id) REFERENCES packages(id)
            )",
            [],
        )?;
        
        // Indexe für schnelle Suchen
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name)",
            [],
        )?;
        
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_timestamp ON packages(timestamp)",
            [],
        )?;
        
        // Migration: Füge fehlende Spalten hinzu, falls sie nicht existieren
        self.migrate_repos_table()?;
        self.migrate_packages_table()?;
        
        Ok(())
    }
    
    /// Migriert die packages-Tabelle, um neue Spalten hinzuzufügen
    fn migrate_packages_table(&self) -> SqliteResult<()> {
        // Prüfe ob filename-Spalte existiert
        let table_info: Result<String, rusqlite::Error> = self.conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='packages'",
            [],
            |row| row.get(0)
        );

        if let Ok(sql) = table_info {
            if !sql.contains("filename") {
                // Füge filename-Spalte hinzu
                self.conn.execute("ALTER TABLE packages ADD COLUMN filename TEXT", [])?;
            }
        }

        Ok(())
    }
    
    /// Migriert die repos-Tabelle, um neue Spalten hinzuzufügen
    fn migrate_repos_table(&self) -> SqliteResult<()> {
        // Prüfe ob suite-Spalte existiert
        let table_info: Result<String, rusqlite::Error> = self.conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='repos'",
            [],
            |row| row.get(0)
        );
        
        if let Ok(sql) = table_info {
            if !sql.contains("suite") {
                // Füge suite-Spalte hinzu
                self.conn.execute("ALTER TABLE repos ADD COLUMN suite TEXT", [])?;
            }
            if !sql.contains("components") {
                // Füge components-Spalte hinzu
                self.conn.execute("ALTER TABLE repos ADD COLUMN components TEXT", [])?;
            }
        }
        
        Ok(())
    }
    
    /// Gibt die Datenbank-Verbindung zurück (für erweiterte Operationen)
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
    
    /// Fügt oder aktualisiert ein Paket im Index
    pub fn add_package(&self, manifest: &PackageManifest, repo_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO packages 
             (name, version, arch, provides, depends, size, checksum, repo_id, timestamp, filename)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                manifest.name,
                manifest.version,
                manifest.arch,
                serde_json::to_string(&manifest.provides).unwrap_or_default(),
                serde_json::to_string(&manifest.depends).unwrap_or_default(),
                manifest.size as i64,
                manifest.checksum,
                repo_id,
                manifest.timestamp,
                manifest.filename.as_deref().unwrap_or(""),
            ],
        )?;
        Ok(())
    }
    
    /// Fügt mehrere Pakete in einer Transaktion hinzu (für bessere Performance)
    pub fn add_packages_batch(&self, manifests: &[PackageManifest], repo_id: i64) -> Result<()> {
        // Serialisiere JSON-Daten vorher für bessere Performance
        let serialized_data: Vec<(String, String, String, String, String, i64, String, i64, i64, String)> = manifests
            .iter()
            .map(|manifest| {
                (
                    manifest.name.clone(),
                    manifest.version.clone(),
                    manifest.arch.clone(),
                    serde_json::to_string(&manifest.provides).unwrap_or_default(),
                    serde_json::to_string(&manifest.depends).unwrap_or_default(),
                    manifest.size as i64,
                    manifest.checksum.clone(),
                    repo_id,
                    manifest.timestamp,
                    manifest.filename.as_deref().unwrap_or("").to_string(),
                )
            })
            .collect();
        
        let tx = self.conn.unchecked_transaction()?;
        
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO packages (name, version, arch, provides, depends, size, checksum, repo_id, timestamp, filename)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
            )?;
            
            for (name, version, arch, provides, depends, size, checksum, repo_id_val, timestamp, filename) in serialized_data {
                stmt.execute(rusqlite::params![
                    name,
                    version,
                    arch,
                    provides,
                    depends,
                    size,
                    checksum,
                    repo_id_val,
                    timestamp,
                    filename,
                ])?;
            }
        }
        
        tx.commit()?;
        Ok(())
    }
    
    /// Sucht nach Paketen im Index (fuzzy search - findet auch Teilstrings)
    pub fn search(&self, query: &str) -> Result<Vec<PackageManifest>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, arch, provides, depends, size, checksum, timestamp, repo_id, filename
             FROM packages
             WHERE name LIKE ?1 OR name LIKE ?2
             ORDER BY name, version DESC"
        )?;
        
        let pattern = format!("%{}%", query);
        let prefix_pattern = format!("{}%", query);
        
        let rows = stmt.query_map(
            rusqlite::params![pattern, prefix_pattern],
            |row| {
                Ok(PackageManifest {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    arch: row.get(2)?,
                    provides: serde_json::from_str(row.get::<_, String>(3)?.as_str()).unwrap_or_default(),
                    depends: serde_json::from_str(row.get::<_, String>(4)?.as_str()).unwrap_or_default(),
                    conflicts: vec![],
                    replaces: vec![],
                    files: vec![],
                    size: row.get(5)?,
                    checksum: row.get(6)?,
                    timestamp: row.get(7)?,
                    repo_id: row.get::<_, Option<i64>>(8)?,
                    filename: row.get::<_, Option<String>>(9)?.filter(|s| !s.is_empty()),
                })
            }
        )?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
    
    /// Sucht nach Paketen mit exaktem Namen (für Upgrades)
    pub fn search_exact(&self, package_name: &str) -> Result<Vec<PackageManifest>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, arch, provides, depends, size, checksum, timestamp, repo_id, filename
             FROM packages
             WHERE name = ?1
             ORDER BY version DESC"
        )?;
        
        let rows = stmt.query_map(
            [package_name],
            |row| {
                Ok(PackageManifest {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    arch: row.get(2)?,
                    provides: serde_json::from_str(row.get::<_, String>(3)?.as_str()).unwrap_or_default(),
                    depends: serde_json::from_str(row.get::<_, String>(4)?.as_str()).unwrap_or_default(),
                    conflicts: vec![],
                    replaces: vec![],
                    files: vec![],
                    size: row.get(5)?,
                    checksum: row.get(6)?,
                    timestamp: row.get(7)?,
                    repo_id: row.get::<_, Option<i64>>(8)?,
                    filename: row.get::<_, Option<String>>(9)?.filter(|s| !s.is_empty()),
                })
            }
        )?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
    
    /// Gibt Paketinformationen zurück
    /// Get all packages from the index (for solver population)
    pub fn get_all_packages(&self) -> Result<Vec<PackageManifest>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, arch, provides, depends, size, checksum, timestamp, repo_id, filename FROM packages"
        )?;
        
        let packages_iter = stmt.query_map([], |row| {
            Ok(PackageManifest {
                name: row.get(0)?,
                version: row.get(1)?,
                arch: row.get(2)?,
                provides: serde_json::from_str(row.get::<_, String>(3)?.as_str()).unwrap_or_default(),
                depends: serde_json::from_str(row.get::<_, String>(4)?.as_str()).unwrap_or_default(),
                conflicts: vec![],
                replaces: vec![],
                files: vec![],
                size: row.get(5)?,
                checksum: row.get(6)?,
                timestamp: row.get(7)?,
                repo_id: row.get::<_, Option<i64>>(8)?,
                filename: row.get::<_, Option<String>>(9)?.filter(|s| !s.is_empty()),
            })
        })?;
        
        let mut packages = Vec::new();
        for pkg in packages_iter {
            packages.push(pkg?);
        }
        
        Ok(packages)
    }
    
    pub fn show(&self, package_name: &str) -> Result<Option<PackageManifest>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, arch, provides, depends, size, checksum, timestamp, repo_id, filename
             FROM packages
             WHERE name = ?1
             ORDER BY version DESC
             LIMIT 1"
        )?;
        
        let result = stmt.query_row([package_name], |row| {
            Ok(PackageManifest {
                name: row.get(0)?,
                version: row.get(1)?,
                arch: row.get(2)?,
                provides: serde_json::from_str(row.get::<_, String>(3)?.as_str()).unwrap_or_default(),
                depends: serde_json::from_str(row.get::<_, String>(4)?.as_str()).unwrap_or_default(),
                conflicts: vec![],
                replaces: vec![],
                files: vec![],
                size: row.get(5)?,
                checksum: row.get(6)?,
                timestamp: row.get(7)?,
                repo_id: row.get::<_, Option<i64>>(8)?,
                filename: row.get::<_, Option<String>>(9)?.filter(|s| !s.is_empty()),
            })
        });
        
        match result {
            Ok(manifest) => Ok(Some(manifest)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Gibt die Repository-URL für eine repo_id zurück
    pub fn get_repo_url(&self, repo_id: i64) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT url FROM repos WHERE id = ?1"
        )?;
        
        let result = stmt.query_row([repo_id], |row| {
            Ok(row.get::<_, String>(0)?)
        });
        
        match result {
            Ok(url) => Ok(Some(url)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Database error: {}", e)),
        }
    }
    
    /// Wählt die beste Mirror-URL basierend auf Performance-Metriken
    /// Gibt die beste URL zurück, oder die ursprüngliche URL falls keine Metriken verfügbar sind
    pub fn select_best_mirror_url(&self, base_url: &str) -> Result<String> {
        use crate::repo::Repository;
        
        // Extrahiere Base-URL (ohne Pfad)
        let base = if let Some(slash_pos) = base_url.find('/') {
            if base_url[slash_pos..].starts_with("//") {
                // http:// oder https://
                if let Some(end_pos) = base_url[slash_pos+2..].find('/') {
                    &base_url[..slash_pos+2+end_pos]
                } else {
                    base_url
                }
            } else {
                &base_url[..slash_pos]
            }
        } else {
            base_url
        };
        
        // Suche nach dem besten Mirror für diese Base-URL
        if let Ok(Some(best_repo)) = Repository::select_best_mirror(self.conn(), base) {
            // Verwende die beste Mirror-URL, aber behalte den ursprünglichen Pfad
            if let Some(path_start) = base_url.find(base) {
                let path = &base_url[path_start + base.len()..];
                Ok(format!("{}{}", best_repo.url.trim_end_matches('/'), path))
            } else {
                Ok(best_repo.url)
            }
        } else {
            // Keine Mirror-Metriken verfügbar, verwende ursprüngliche URL
            Ok(base_url.to_string())
        }
    }
    
    /// Aktualisiert die Performance-Metriken für eine Mirror-URL nach einem Download
    pub fn update_mirror_performance(&self, url: &str, rtt_ms: u64, _throughput: u64) -> Result<()> {
        use crate::repo::Repository;
        
        // Extrahiere Base-URL
        let base_url = if let Some(path_start) = url.find('/') {
            if url[path_start..].starts_with("//") {
                if let Some(end_pos) = url[path_start+2..].find('/') {
                    &url[..path_start+2+end_pos]
                } else {
                    url
                }
            } else {
                &url[..path_start]
            }
        } else {
            url
        };
        
        // Aktualisiere RTT (Throughput wird nicht in der DB gespeichert, nur RTT)
        Repository::update_probe_stats(self.conn(), base_url, rtt_ms)?;
        
        Ok(())
    }
    
    /// Gibt alle installierten Pakete zurück
    #[allow(dead_code)]
    pub fn list_installed(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.name FROM packages p
             INNER JOIN installed i ON p.id = i.pkg_id"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok(row.get::<_, String>(0)?)
        })?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
    
    /// Markiert ein Paket als installiert
    #[allow(dead_code)]
    pub fn mark_installed(&self, package_name: &str, version: &str) -> Result<()> {
        // Finde Paket-ID
        let pkg_id: i64 = self.conn.query_row(
            "SELECT id FROM packages WHERE name = ?1 AND version = ?2",
            [package_name, version],
            |row| row.get(0)
        )?;
        
        // Füge zu installiert hinzu
        self.conn.execute(
            "INSERT OR REPLACE INTO installed (pkg_id, install_time, manifest)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![
                pkg_id,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                "{}" // Placeholder für Manifest
            ],
        )?;
        
        Ok(())
    }
    
    /// Entfernt ein Paket aus der installierten Liste
    pub fn mark_removed(&self, package_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM installed 
             WHERE pkg_id IN (SELECT id FROM packages WHERE name = ?1)",
            [package_name]
        )?;
        Ok(())
    }
    
    /// Gibt alle installierten Pakete mit ihren vollständigen Manifests zurück
    pub fn list_installed_packages_with_manifests(&self) -> Result<Vec<PackageManifest>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.name, p.version, p.arch, p.provides, p.depends, p.size, p.checksum, p.timestamp, p.repo_id, p.filename
             FROM packages p
             INNER JOIN installed i ON p.id = i.pkg_id"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok(PackageManifest {
                name: row.get(0)?,
                version: row.get(1)?,
                arch: row.get(2)?,
                provides: serde_json::from_str(row.get::<_, String>(3)?.as_str()).unwrap_or_default(),
                depends: serde_json::from_str(row.get::<_, String>(4)?.as_str()).unwrap_or_default(),
                conflicts: vec![],
                replaces: vec![],
                files: vec![],
                size: row.get(5)?,
                checksum: row.get(6)?,
                timestamp: row.get(7)?,
                repo_id: row.get::<_, Option<i64>>(8)?,
                filename: row.get::<_, Option<String>>(9)?.filter(|s| !s.is_empty()),
            })
        })?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    #[test]
    fn test_index_creation() {
        let test_db = "/tmp/test_apt_ng_index.db";
        let _ = fs::remove_file(test_db);
        
        let index = Index::new(test_db).unwrap();
        // Test that we can query the database
        let result: Result<i32, rusqlite::Error> = index.conn().query_row("SELECT 1", [], |row| row.get(0));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
        
        let _ = fs::remove_file(test_db);
    }
}

