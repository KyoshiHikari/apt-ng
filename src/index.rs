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
        Ok(index)
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
        let tx = self.conn.unchecked_transaction()?;
        
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO packages (name, version, arch, provides, depends, size, checksum, repo_id, timestamp, filename)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
            )?;
            
            for manifest in manifests {
                stmt.execute(rusqlite::params![
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
                ])?;
            }
        }
        
        tx.commit()?;
        Ok(())
    }
    
    /// Sucht nach Paketen im Index
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

