use crate::test_server::TestServer;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use std::fs;

/// Setup-Funktion für Tests - erstellt temporäres Test-Repository
pub async fn setup_test_repo(suite: &str, component: &str, arch: &str) -> anyhow::Result<(TestServer, String)> {
    let server = TestServer::new().await?;
    let base_url = server.url();
    
    server.setup_test_repo(suite, component, arch).await;
    
    Ok((server, base_url))
}

/// Erstellt eine Test-Konfiguration mit Test-Repository
pub fn create_test_config(repo_url: &str, temp_dir: &Path) -> apt_ng::config::Config {
    let config_dir = temp_dir.join("etc");
    let state_dir = temp_dir.join("var/lib");
    let cache_dir = temp_dir.join("var/cache");
    let trusted_keys_dir = config_dir.join("trusted.gpg.d");
    
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::create_dir_all(&trusted_keys_dir).unwrap();
    
    apt_ng::config::Config {
        paths: apt_ng::config::Paths {
            config_dir,
            state_dir,
            cache_dir,
            trusted_keys_dir,
        },
        jobs: Some(2),
        repos: vec![apt_ng::config::RepoConfig {
            url: repo_url.to_string(),
            priority: 500,
            enabled: true,
        }],
    }
}

/// Erstellt einen temporären Index für Tests
pub fn create_test_index(temp_dir: &Path) -> anyhow::Result<apt_ng::index::Index> {
    let db_path = temp_dir.join("index.db");
    apt_ng::index::Index::new(db_path.to_str().unwrap())
}

/// Cleanup-Funktion für Tests
pub fn cleanup_test_env(_temp_dir: &TempDir) {
    // TempDir wird automatisch aufgeräumt wenn es droppt
}

/// Erstellt eine Test-Packages-Datei-Content
pub fn create_test_packages_file() -> String {
    r#"Package: test-package
Version: 1.0.0
Architecture: amd64
Depends: libc6 (>= 2.0)
Provides: test-tool
Size: 1024
SHA256: abc123def456
Filename: pool/main/t/test-package/test-package_1.0.0_amd64.deb
Description: A test package for integration tests
 This is a test package used for testing tests.

Package: another-package
Version: 2.0.0
Architecture: all
Depends: test-package
Size: 2048
SHA256: def456ghi789
Filename: pool/main/a/another-package/another-package_2.0.0_all.deb
Description: Another test package
 This package depends on test-package.

Package: simple-package
Version: 1.5.0
Architecture: amd64
Size: 512
SHA256: 111222333444
Filename: pool/main/s/simple-package/simple-package_1.5.0_amd64.deb
Description: A simple test package
 No dependencies.
"#.to_string()
}

/// Helper um Test-Umgebung zu erstellen
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub config: apt_ng::config::Config,
    pub index: apt_ng::index::Index,
    pub server: TestServer,
    pub repo_url: String,
}

impl TestEnvironment {
    pub async fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let (server, repo_url) = setup_test_repo("stable", "main", "amd64").await?;
        let config = create_test_config(&repo_url, temp_dir.path());
        let index = create_test_index(temp_dir.path())?;
        
        Ok(TestEnvironment {
            temp_dir,
            config,
            index,
            server,
            repo_url,
        })
    }
}

