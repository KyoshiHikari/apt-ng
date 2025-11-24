use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use sha2::{Sha256, Digest};
use hex;

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
        fs::write(&path, data)?;
        
        Ok(path)
    }
    
    /// Berechnet die Checksumme einer Datei
    #[allow(dead_code)]
    pub fn calculate_checksum(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }
    
    /// Räumt den Cache auf (entfernt alte Pakete)
    pub fn clean(&self) -> Result<()> {
        let packages_dir = self.cache_dir.join("packages");
        if packages_dir.exists() {
            for entry in fs::read_dir(&packages_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    fs::remove_file(&path)?;
                }
            }
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

