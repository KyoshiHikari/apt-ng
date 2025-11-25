use crate::helpers::TestEnvironment;
use apt_ng::repo::Repository;

#[tokio::test]
async fn test_update_command() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Füge Repository zur Datenbank hinzu
    let repo = Repository {
        id: None,
        url: env.repo_url.clone(),
        priority: 500,
        enabled: true,
        last_probe_ms: None,
        rtt_ms: None,
        suite: Some("stable".to_string()),
        components: vec!["main".to_string()],
    };
    
    Repository::add_to_db(env.index.conn(), &repo).unwrap();
    
    // Führe Update aus (vereinfacht - direkt die Funktion aufrufen)
    // In einer vollständigen Implementierung würde man hier den CLI-Befehl testen
    let repos = Repository::load_all(env.index.conn()).unwrap();
    assert!(!repos.is_empty());
    assert_eq!(repos[0].url, env.repo_url);
}

#[tokio::test]
async fn test_search_command() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Füge Test-Pakete zum Index hinzu
    use apt_ng::apt_parser::parse_packages_file;
    let packages_content = crate::helpers::create_test_packages_file();
    let packages = parse_packages_file(&packages_content).unwrap();
    
    // Füge Pakete zum Index hinzu
    for pkg in packages {
        env.index.add_package(pkg, 1).unwrap();
    }
    
    // Teste Suche
    let results = env.index.search("test-package").unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "test-package");
    
    let results = env.index.search("simple").unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "simple-package");
}

#[tokio::test]
async fn test_show_command() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Füge Test-Paket zum Index hinzu
    use apt_ng::apt_parser::parse_packages_file;
    let packages_content = crate::helpers::create_test_packages_file();
    let packages = parse_packages_file(&packages_content).unwrap();
    
    for pkg in packages {
        env.index.add_package(pkg, 1).unwrap();
    }
    
    // Teste show
    let pkg = env.index.show("test-package").unwrap();
    assert!(pkg.is_some());
    let pkg = pkg.unwrap();
    assert_eq!(pkg.name, "test-package");
    assert_eq!(pkg.version, "1.0.0");
}

#[tokio::test]
async fn test_repo_add() {
    let env = TestEnvironment::new().await.unwrap();
    
    let test_url = "http://example.com/repo";
    let repo = Repository {
        id: None,
        url: test_url.to_string(),
        priority: 500,
        enabled: true,
        last_probe_ms: None,
        rtt_ms: None,
        suite: None,
        components: vec!["main".to_string()],
    };
    
    Repository::add_to_db(env.index.conn(), &repo).unwrap();
    
    let repos = Repository::load_all(env.index.conn()).unwrap();
    assert!(repos.iter().any(|r| r.url == test_url));
}

#[tokio::test]
async fn test_cache_clean() {
    use apt_ng::cache::Cache;
    use std::fs;
    
    let env = TestEnvironment::new().await.unwrap();
    
    // Erstelle Cache
    let cache = Cache::new(env.config.cache_path()).unwrap();
    
    // Erstelle Test-Dateien im Cache
    let package_dir = cache.cache_dir.join("packages");
    fs::create_dir_all(&package_dir).unwrap();
    
    let test_file = package_dir.join("test-package.deb");
    fs::write(&test_file, b"test content").unwrap();
    
    // Teste Cache-Clean
    let size_before = cache.size().unwrap();
    assert!(size_before > 0);
    
    cache.clean().unwrap();
    
    let size_after = cache.size().unwrap();
    assert_eq!(size_after, 0);
}

#[tokio::test]
async fn test_install_dry_run() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Füge Repository und Pakete hinzu
    let repo = Repository {
        id: None,
        url: env.repo_url.clone(),
        priority: 500,
        enabled: true,
        last_probe_ms: None,
        rtt_ms: None,
        suite: Some("stable".to_string()),
        components: vec!["main".to_string()],
    };
    
    Repository::add_to_db(env.index.conn(), &repo).unwrap();
    
    use apt_ng::apt_parser::parse_packages_file;
    let packages_content = crate::helpers::create_test_packages_file();
    let packages = parse_packages_file(&packages_content).unwrap();
    
    for pkg in packages {
        env.index.add_package(pkg, 1).unwrap();
    }
    
    // Teste Dependency Resolution (dry-run würde hier die Solver-Logik testen)
    use apt_ng::solver::DependencySolver;
    let mut solver = DependencySolver::new();
    
    let all_packages = env.index.get_all_packages().unwrap();
    for manifest in &all_packages {
        if let Ok(pkg_info) = DependencySolver::manifest_to_package_info(manifest) {
            solver.add_package(pkg_info);
        }
    }
    
    use apt_ng::solver::PackageSpec;
    let solution = solver.solve(&[PackageSpec {
        name: "test-package".to_string(),
        version: None,
        arch: None,
    }]).unwrap();
    
    assert!(!solution.to_install.is_empty());
    assert!(solution.to_install.iter().any(|p| p.name == "test-package"));
}

