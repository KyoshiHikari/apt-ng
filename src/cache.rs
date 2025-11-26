use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::collections::HashMap;
use sha2::{Sha256, Digest};
use hex;
use std::os::unix::fs::MetadataExt;

pub struct Cache {
    pub cache_dir: PathBuf,
}

impl Cache {
    pub fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;
        
        Ok(Cache { cache_dir })
    }
    
    /// Gibt den Cache-Pfad für ein Paket zurück
    pub fn package_path(&self, name: &str, version: &str, arch: &str) -> PathBuf {
        // Verwende .deb für Debian-Pakete (könnte später auf .apx erweitert werden)
        let filename = format!("{}_{}_{}.deb", name, version, arch);
        self.cache_dir.join("packages").join(filename)
    }
    
    /// Gibt den Cache-Pfad für ein Paket mit beliebiger Extension zurück
    pub fn package_path_with_ext(&self, name: &str, version: &str, arch: &str, ext: &str) -> PathBuf {
        let filename = format!("{}_{}_{}.{}", name, version, arch, ext);
        self.cache_dir.join("packages").join(filename)
    }
    
    /// Prüft, ob ein Paket im Cache vorhanden ist
    #[allow(dead_code)]
    pub fn has_package(&self, name: &str, version: &str, arch: &str) -> bool {
        self.package_path(name, version, arch).exists()
    }
    
    /// Fügt ein Paket zum Cache hinzu
    #[allow(dead_code)]
    pub fn add_package(&self, name: &str, version: &str, arch: &str, data: &[u8]) -> Result<PathBuf> {
        let package_dir = self.cache_dir.join("packages");
        fs::create_dir_all(&package_dir)?;
        
        let path = self.package_path(name, version, arch);
        
        // Berechne Checksumme für Deduplikation
        let checksum = Self::calculate_checksum(data);
        
        // Prüfe, ob bereits ein Paket mit derselben Checksumme existiert
        if let Some(existing_path) = self.find_package_by_checksum(&checksum)? {
            // Erstelle Hardlink statt Datei zu kopieren
            if let Err(_e) = fs::hard_link(&existing_path, &path) {
                // Falls Hardlink fehlschlägt (z.B. auf verschiedenen Dateisystemen), kopiere die Datei
                fs::copy(&existing_path, &path)?;
            }
        } else {
            // Neue Datei schreiben
            fs::write(&path, data)?;
            
            // Speichere Checksumme in Index
            self.update_checksum_index(&checksum, &path)?;
        }
        
        Ok(path)
    }
    
    /// Fügt ein Paket aus einer Datei zum Cache hinzu (mit Deduplikation)
    pub fn add_package_from_file(&self, name: &str, version: &str, arch: &str, ext: &str, source_file: &Path) -> Result<PathBuf> {
        let package_dir = self.cache_dir.join("packages");
        fs::create_dir_all(&package_dir)?;
        
        let path = self.package_path_with_ext(name, version, arch, ext);
        
        // Berechne Checksumme der Quelldatei (streaming für große Dateien)
        let checksum = Self::calculate_file_checksum(source_file)?;
        
        // Prüfe, ob bereits ein Paket mit derselber Checksumme existiert
        if let Some(existing_path) = self.find_package_by_checksum(&checksum)? {
            // Erstelle Hardlink statt Datei zu kopieren
            if let Err(_e) = fs::hard_link(&existing_path, &path) {
                // Falls Hardlink fehlschlägt, kopiere die Datei
                fs::copy(&existing_path, &path)?;
            }
        } else {
            // Versuche rename zuerst (schneller als copy, atomisch)
            if let Err(_) = fs::rename(source_file, &path) {
                // Falls rename fehlschlägt (verschiedene Dateisysteme), kopiere
                fs::copy(source_file, &path)?;
            }
            
            // Speichere Checksumme in Index
            self.update_checksum_index(&checksum, &path)?;
        }
        
        Ok(path)
    }
    
    /// Findet ein Paket anhand seiner Checksumme
    fn find_package_by_checksum(&self, checksum: &str) -> Result<Option<PathBuf>> {
        let checksum_index = self.load_checksum_index()?;
        Ok(checksum_index.get(checksum).cloned())
    }
    
    /// Lädt den Checksum-Index
    fn load_checksum_index(&self) -> Result<HashMap<String, PathBuf>> {
        let index_path = self.cache_dir.join("checksums.json");
        
        if !index_path.exists() {
            return Ok(HashMap::new());
        }
        
        let content = fs::read_to_string(&index_path)?;
        let index: HashMap<String, String> = serde_json::from_str(&content)
            .unwrap_or_default();
        
        // Konvertiere String-Pfade zu PathBuf
        let mut result = HashMap::new();
        for (checksum, path_str) in index {
            let path = PathBuf::from(path_str);
            // Prüfe, ob die Datei noch existiert
            if path.exists() {
                result.insert(checksum, path);
            }
        }
        
        Ok(result)
    }
    
    /// Aktualisiert den Checksum-Index (mit Batch-Updates für bessere Performance)
    fn update_checksum_index(&self, checksum: &str, path: &Path) -> Result<()> {
        let mut index = self.load_checksum_index()?;
        
        // Füge neuen Eintrag hinzu, falls noch nicht vorhanden
        if !index.contains_key(checksum) {
            index.insert(checksum.to_string(), path.to_path_buf());
            
            // Speichere Index (nur wenn sich etwas geändert hat)
            let index_path = self.cache_dir.join("checksums.json");
            let index_str: HashMap<String, String> = index.iter()
                .map(|(k, v)| (k.clone(), v.to_string_lossy().to_string()))
                .collect();
            let content = serde_json::to_string(&index_str)?; // Kein pretty-print für bessere Performance
            fs::write(&index_path, content)?;
        }
        
        Ok(())
    }
    
    /// Prüft, ob eine Datei ein Hardlink ist und ob andere Hardlinks existieren
    fn count_hardlinks(&self, path: &Path) -> Result<u64> {
        let metadata = fs::metadata(path)?;
        Ok(metadata.nlink())
    }
    
    /// Berechnet die Checksumme von Daten im Speicher
    pub fn calculate_checksum(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }
    
    /// Berechnet die Checksumme einer Datei (streaming für große Dateien)
    fn calculate_file_checksum(file_path: &Path) -> Result<String> {
        use std::io::Read;
        use std::fs::File;
        
        let mut file = File::open(file_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB Buffer für bessere Performance
        
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        
        Ok(hex::encode(hasher.finalize()))
    }
    
    /// Räumt den Cache auf (entfernt alte Pakete)
    pub fn clean(&self) -> Result<()> {
        let packages_dir = self.cache_dir.join("packages");
        if packages_dir.exists() {
            for entry in fs::read_dir(&packages_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    // Prüfe Hardlinks: Wenn nlink > 1, ist es ein Hardlink
                    // Entferne nur, wenn es der letzte Link ist
                    let nlink = self.count_hardlinks(&path).unwrap_or(1);
                    if nlink == 1 {
                        fs::remove_file(&path)?;
                    }
                }
            }
        }
        
        // Bereinige Checksum-Index
        self.clean_checksum_index()?;
        
        Ok(())
    }
    
    /// Bereinigt den Checksum-Index von nicht mehr existierenden Dateien
    fn clean_checksum_index(&self) -> Result<()> {
        let mut index = self.load_checksum_index()?;
        let mut updated = false;
        
        index.retain(|_checksum, path| {
            let exists = path.exists();
            if !exists {
                updated = true;
            }
            exists
        });
        
        if updated {
            let index_path = self.cache_dir.join("checksums.json");
            let index_str: HashMap<String, String> = index.iter()
                .map(|(k, v)| (k.clone(), v.to_string_lossy().to_string()))
                .collect();
            let content = serde_json::to_string_pretty(&index_str)?;
            fs::write(&index_path, content)?;
        }
        
        Ok(())
    }
    
    /// Intelligente Cache-Bereinigung: Entfernt alte Versionen von Paketen, behält nur die neueste
    pub fn clean_old_versions(&self) -> Result<usize> {
        use std::collections::HashMap;
        use std::time::SystemTime;
        
        let packages_dir = self.cache_dir.join("packages");
        if !packages_dir.exists() {
            return Ok(0);
        }
        
        // Sammle alle Pakete gruppiert nach Name
        let mut packages_by_name: HashMap<String, Vec<(PathBuf, SystemTime)>> = HashMap::new();
        
        for entry in fs::read_dir(&packages_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                // Parse filename: name_version_arch.deb
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some((name_part, _)) = filename.rsplit_once('_') {
                        if let Some((name, _)) = name_part.rsplit_once('_') {
                            let modified = fs::metadata(&path)?.modified()?;
                            packages_by_name
                                .entry(name.to_string())
                                .or_insert_with(Vec::new)
                                .push((path.clone(), modified));
                        }
                    }
                }
            }
        }
        
        // Für jedes Paket: Behalte nur die neueste Version
        let mut removed_count = 0;
        for (_, mut versions) in packages_by_name {
            if versions.len() > 1 {
                // Sortiere nach Änderungsdatum (neueste zuerst)
                versions.sort_by(|a, b| b.1.cmp(&a.1));
                
                // Entferne alle außer der neuesten Version
                for (path, _) in versions.iter().skip(1) {
                    fs::remove_file(path)?;
                    removed_count += 1;
                }
            }
        }
        
        Ok(removed_count)
    }
    
    /// Bereinigt den Cache, wenn die Größe das Limit überschreitet
    pub fn clean_if_over_limit(&self, max_size_bytes: u64) -> Result<usize> {
        let current_size = self.size()?;
        
        if current_size <= max_size_bytes {
            return Ok(0);
        }
        
        // Berechne wie viel entfernt werden muss
        let to_remove = current_size - max_size_bytes;
        
        // Sammle alle Pakete mit Größe und Änderungsdatum
        let packages_dir = self.cache_dir.join("packages");
        if !packages_dir.exists() {
            return Ok(0);
        }
        
        let mut packages: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        for entry in fs::read_dir(&packages_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                let metadata = fs::metadata(&path)?;
                let size = metadata.len();
                let modified = metadata.modified()?;
                packages.push((path, size, modified));
            }
        }
        
        // Sortiere nach Änderungsdatum (älteste zuerst)
        packages.sort_by(|a, b| a.2.cmp(&b.2));
        
        // Entferne Pakete bis das Limit erreicht ist
        let mut removed_size = 0u64;
        let mut removed_count = 0usize;
        
        for (path, size, _) in packages {
            if removed_size >= to_remove {
                break;
            }
            
            fs::remove_file(&path)?;
            removed_size += size;
            removed_count += 1;
        }
        
        Ok(removed_count)
    }
    
    /// Gibt die Größe des Caches zurück
    pub fn size(&self) -> Result<u64> {
        let mut total_size = 0u64;
        
        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    total_size += fs::metadata(&path)?.len();
                } else if path.is_dir() {
                    total_size += self.dir_size(&path)?;
                }
            }
        }
        
        Ok(total_size)
    }
    
    fn dir_size(&self, dir: &Path) -> Result<u64> {
        let mut total = 0u64;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                total += fs::metadata(&path)?.len();
            } else if path.is_dir() {
                total += self.dir_size(&path)?;
            }
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_cache_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::new(temp_dir.path()).unwrap();
        assert!(cache.cache_dir.exists());
    }
    
    #[test]
    fn test_cache_add_package() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::new(temp_dir.path()).unwrap();
        
        let data = b"test package data";
        cache.add_package("test", "1.0", "amd64", data).unwrap();
        
        assert!(cache.has_package("test", "1.0", "amd64"));
    }
    
    #[test]
    fn test_checksum() {
        let data = b"test";
        let checksum = Cache::calculate_checksum(data);
        assert_eq!(checksum.len(), 64); // SHA256 hex string length
    }
}

