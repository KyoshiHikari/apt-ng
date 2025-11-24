use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub paths: Paths,
    pub jobs: Option<usize>,
    pub repos: Vec<RepoConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub trusted_keys_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub url: String,
    pub priority: i32,
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        let config_dir = PathBuf::from("/etc/apt-ng");
        let state_dir = PathBuf::from("/var/lib/apt-ng");
        let cache_dir = PathBuf::from("/var/cache/apt-ng");
        let trusted_keys_dir = config_dir.join("trusted.gpg.d");
        
        Config {
            paths: Paths {
                config_dir,
                state_dir,
                cache_dir,
                trusted_keys_dir,
            },
            jobs: None,
            repos: Vec::new(),
        }
    }
}

impl Config {
    /// Lädt die Konfiguration aus einer TOML-Datei oder erstellt eine Default-Konfiguration
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let config_path = config_path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/etc/apt-ng/config.toml"));
        
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Erstelle Default-Konfiguration
            let config = Config::default();
            // Erstelle Verzeichnisse falls nötig
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Speichere Default-Konfiguration
            let toml_content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, toml_content)?;
            Ok(config)
        }
    }
    
    /// Gibt die Anzahl der Worker-Threads zurück
    pub fn jobs(&self) -> usize {
        self.jobs.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get() * 2)
                .unwrap_or(4)
        })
    }
    
    /// Gibt den Pfad zur Index-Datenbank zurück
    pub fn index_db_path(&self) -> PathBuf {
        self.paths.state_dir.join("index.db")
    }
    
    /// Gibt den Pfad zum Paket-Cache zurück
    pub fn cache_path(&self) -> &Path {
        &self.paths.cache_dir
    }
    
    /// Gibt den Pfad zum Trusted-Keys-Verzeichnis zurück
    #[allow(dead_code)]
    pub fn trusted_keys_dir(&self) -> &Path {
        &self.paths.trusted_keys_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.paths.config_dir, PathBuf::from("/etc/apt-ng"));
        assert_eq!(config.paths.state_dir, PathBuf::from("/var/lib/apt-ng"));
    }
    
    #[test]
    fn test_config_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let config = Config::load(Some(&config_path)).unwrap();
        assert_eq!(config.paths.config_dir, PathBuf::from("/etc/apt-ng"));
    }
}

