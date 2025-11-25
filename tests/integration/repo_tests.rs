use crate::helpers::TestEnvironment;
use apt_ng::repo::Repository;

#[tokio::test]
async fn test_repository_formats() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Test verschiedene Repository-Formate
    let repos = vec![
        ("http://deb.debian.org/debian", "stable", vec!["main".to_string()]),
        ("http://security.debian.org/debian-security", "stable-security", vec!["main".to_string()]),
    ];
    
    for (url, suite, components) in repos {
        let repo = Repository {
            id: None,
            url: url.to_string(),
            priority: 500,
            enabled: true,
            last_probe_ms: None,
            rtt_ms: None,
            suite: Some(suite.to_string()),
            components,
        };
        
        Repository::add_to_db(env.index.conn(), &repo).unwrap();
    }
    
    let loaded_repos = Repository::load_all(env.index.conn()).unwrap();
    assert_eq!(loaded_repos.len(), 2);
}

#[tokio::test]
async fn test_security_repository() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Test Security-Repository
    let security_repo = Repository {
        id: None,
        url: "http://security.debian.org/debian-security".to_string(),
        priority: 100, // Höhere Priorität für Security
        enabled: true,
        last_probe_ms: None,
        rtt_ms: None,
        suite: Some("stable-security".to_string()),
        components: vec!["main".to_string()],
    };
    
    Repository::add_to_db(env.index.conn(), &security_repo).unwrap();
    
    let repos = Repository::load_all(env.index.conn()).unwrap();
    assert!(repos.iter().any(|r| r.url.contains("security")));
}

#[tokio::test]
async fn test_repository_components() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Test Repository mit mehreren Components
    let repo = Repository {
        id: None,
        url: env.repo_url.clone(),
        priority: 500,
        enabled: true,
        last_probe_ms: None,
        rtt_ms: None,
        suite: Some("stable".to_string()),
        components: vec!["main".to_string(), "contrib".to_string(), "non-free".to_string()],
    };
    
    Repository::add_to_db(env.index.conn(), &repo).unwrap();
    
    let loaded_repos = Repository::load_all(env.index.conn()).unwrap();
    let loaded_repo = loaded_repos.iter().find(|r| r.url == env.repo_url).unwrap();
    assert_eq!(loaded_repo.components.len(), 3);
    assert!(loaded_repo.components.contains(&"main".to_string()));
    assert!(loaded_repo.components.contains(&"contrib".to_string()));
    assert!(loaded_repo.components.contains(&"non-free".to_string()));
}

#[tokio::test]
async fn test_repository_priority() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Test Repository-Priorität
    let repos = vec![
        Repository {
            id: None,
            url: "http://repo1.example.com".to_string(),
            priority: 100,
            enabled: true,
            last_probe_ms: None,
            rtt_ms: None,
            suite: None,
            components: vec!["main".to_string()],
        },
        Repository {
            id: None,
            url: "http://repo2.example.com".to_string(),
            priority: 500,
            enabled: true,
            last_probe_ms: None,
            rtt_ms: None,
            suite: None,
            components: vec!["main".to_string()],
        },
    ];
    
    for repo in repos {
        Repository::add_to_db(env.index.conn(), &repo).unwrap();
    }
    
    let loaded_repos = Repository::load_all(env.index.conn()).unwrap();
    // Repositories sollten nach Priorität sortiert sein
    assert!(loaded_repos[0].priority <= loaded_repos[1].priority);
}

#[tokio::test]
async fn test_repository_suite_variations() {
    let env = TestEnvironment::new().await.unwrap();
    
    // Test verschiedene Suites
    let suites = vec!["stable", "testing", "unstable", "bookworm", "trixie"];
    
    for suite in suites {
        let repo = Repository {
            id: None,
            url: format!("http://example.com/{}", suite),
            priority: 500,
            enabled: true,
            last_probe_ms: None,
            rtt_ms: None,
            suite: Some(suite.to_string()),
            components: vec!["main".to_string()],
        };
        
        Repository::add_to_db(env.index.conn(), &repo).unwrap();
    }
    
    let loaded_repos = Repository::load_all(env.index.conn()).unwrap();
    assert_eq!(loaded_repos.len(), suites.len());
}

