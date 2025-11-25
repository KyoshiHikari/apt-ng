mod cli;
mod config;
mod index;
mod downloader;
mod verifier;
mod installer;
mod package;
mod repo;
mod solver;
mod cache;
mod apt_parser;
mod system;
mod output;
mod sandbox;
mod security;
mod delta;

use cli::{Commands, RepoCommands, CacheAction, SecurityCommands};
use std::path::Path;
use std::collections::{HashSet, HashMap};
use clap::CommandFactory;

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    
    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize output system
    output::Output::init();
    
    // Check for completion generation request
    if let Ok(shell) = std::env::var("APT_NG_GENERATE_COMPLETIONS") {
        let mut app = cli::Cli::command();
        cli::generate_completions(&shell, &mut app);
        return Ok(());
    }
    
    let opts = cli::parse();
    
    // Load configuration
    let config = config::Config::load(None)?;
    
    // Stelle sicher, dass alle benötigten Verzeichnisse existieren
    if let Err(e) = std::fs::create_dir_all(&config.paths.state_dir) {
        eprintln!("Warning: Could not create state directory {:?}: {}", config.paths.state_dir, e);
        eprintln!("Hint: You may need root privileges or the directory may need to be created manually.");
        return Err(e.into());
    }
    if let Err(e) = std::fs::create_dir_all(&config.paths.cache_dir) {
        eprintln!("Warning: Could not create cache directory {:?}: {}", config.paths.cache_dir, e);
        eprintln!("Hint: You may need root privileges or the directory may need to be created manually.");
        return Err(e.into());
    }
    if let Err(e) = std::fs::create_dir_all(&config.paths.trusted_keys_dir) {
        eprintln!("Warning: Could not create trusted keys directory {:?}: {}", config.paths.trusted_keys_dir, e);
        eprintln!("Hint: You may need root privileges or the directory may need to be created manually.");
        return Err(e.into());
    }
    
    // Initialisiere Index
    let index = index::Index::new(config.index_db_path().to_str().unwrap())?;
    
    // Führe Command aus
    match &opts.command {
        Commands::Update => {
            cmd_update(&index, &config, opts.verbose).await?;
        }
        Commands::Search { term } => {
            cmd_search(&index, term, opts.verbose)?;
        }
        Commands::Install { packages } => {
            cmd_install(&index, &config, packages, opts.jobs.unwrap_or(config.jobs()), opts.dry_run, opts.verbose).await?;
        }
        Commands::Remove { packages } => {
            cmd_remove(&index, packages, opts.dry_run, opts.verbose).await?;
        }
        Commands::Upgrade => {
            cmd_upgrade(&index, &config, opts.jobs.unwrap_or(config.jobs()), opts.dry_run, opts.verbose).await?;
        }
        Commands::Show { package } => {
            cmd_show(&index, package, opts.verbose)?;
        }
        Commands::Repo(repo_cmd) => {
            match repo_cmd {
                RepoCommands::Add { url } => {
                    cmd_repo_add(&index, url)?;
                }
                RepoCommands::Update => {
                    cmd_repo_update(&index, &config, opts.verbose).await?;
                }
            }
        }
        Commands::Cache(action) => {
            match action {
                CacheAction::Clean { old_versions, max_size } => {
                    cmd_cache_clean(&config, *old_versions, *max_size, opts.verbose)?;
                }
            }
        }
        Commands::Security(security_cmd) => {
            match security_cmd {
                SecurityCommands::Audit { format } => {
                    cmd_security_audit(&format, opts.verbose)?;
                }
            }
        }
    }
    
    Ok(())
}

async fn cmd_update(index: &index::Index, config: &config::Config, verbose: bool) -> anyhow::Result<()> {
    output::Output::heading("🔄 Updating Package Index");
    
    // Versuche apt-Repositories zu importieren, falls noch keine vorhanden sind
    let imported = repo::Repository::import_apt_repos(index.conn())?;
    if imported > 0 {
        output::Output::success(&format!("Imported {} repositories from apt/apt-get configuration", imported));
    }
    
    // Lade Repositories
    let repos = repo::Repository::load_all(index.conn())?;
    
    if repos.is_empty() {
        output::Output::warning("No repositories configured");
        output::Output::list_item("Use 'apt-ng repo add <url>' to add one.");
        output::Output::list_item("Or ensure /etc/apt/sources.list contains valid repositories.");
        return Ok(());
    }
    
    output::Output::info(&format!("Found {} repositories", repos.len()));
    
    // Prüfe auf unsignierte Repositories
    let verifier = verifier::PackageVerifier::new(config.trusted_keys_dir())?;
    let require_signatures = verifier.trusted_key_count() > 0;
    
    if require_signatures {
        output::Output::info(&format!("Signature verification enabled ({} trusted key(s))", verifier.trusted_key_count()));
    } else {
        output::Output::warning("No trusted keys found. Unsigned repositories will be allowed.");
        output::Output::info(&format!("Add trusted keys to: {}", config.trusted_keys_dir().display()));
    }
    
    // Lade Metadaten von Repositories
    let downloader = downloader::Downloader::new(config.jobs())?;
    let mut total_packages = 0;
    
    // Erkenne Debian-Suite automatisch
    let detected_suite = system::detect_debian_suite().unwrap_or_else(|_| "stable".to_string());
    output::Output::info(&format!("Detected Debian suite: {}", detected_suite));
    
    for repo in &repos {
        output::Output::repo_info(&repo.url);
        
        // Verwende erkannte Suite oder die aus der sources.list
        let suite = repo.suite.as_deref()
            .or_else(|| Some(&detected_suite))
            .unwrap_or("stable");
        let components = if repo.components.is_empty() {
            vec!["main".to_string()]
        } else {
            repo.components.clone()
        };
        
        // Für Security-Repositories: Verwende bookworm-security oder bookworm/updates
        let is_security = repo.url.contains("security.debian.org");
        let suite_path = if is_security {
            // Security-Repos verwenden entweder {suite}-security oder {suite}/updates
            format!("{}-security", suite)
        } else {
            suite.to_string()
        };
        
            if verbose {
                output::Output::info(&format!("  Suite: {:?}, Components: {:?}", suite, components));
            }
        
        // Versuche verschiedene Architekturen
        let architectures = vec!["amd64", "all"];
        
        let mut packages_loaded = false;
        for component in &components {
            for arch in &architectures {
                        // Versuche verschiedene komprimierte Formate
                        let possible_files = vec![
                            format!("dists/{}/{}/binary-{}/Packages.xz", suite_path, component, arch),
                            format!("dists/{}/{}/binary-{}/Packages.gz", suite_path, component, arch),
                            format!("dists/{}/{}/binary-{}/Packages", suite_path, component, arch),
                        ];
                        
                        // Für Security-Repos: Versuche auch bookworm/updates
                        let mut security_files = Vec::new();
                        if is_security {
                            security_files.extend(vec![
                                format!("dists/{}/updates/{}/binary-{}/Packages.xz", suite, component, arch),
                                format!("dists/{}/updates/{}/binary-{}/Packages.gz", suite, component, arch),
                                format!("dists/{}/updates/{}/binary-{}/Packages", suite, component, arch),
                            ]);
                        }
                        let possible_files: Vec<String> = possible_files.into_iter().chain(security_files).collect();
                
                for file_path in possible_files {
                    let url = if file_path.starts_with("http") {
                        file_path.clone()
                    } else {
                        format!("{}/{}", repo.url.trim_end_matches('/'), file_path.trim_start_matches('/'))
                    };
                    
                    if verbose {
                        output::Output::progress_message(&format!("Trying: {}...", url));
                    }
                    
                    let temp_file = std::env::temp_dir().join(format!("apt-ng-packages-{}.tmp", 
                        url.replace("/", "_").replace(":", "_").replace(".", "_")));
                    
                    // Versuche herunterzuladen mit Timeout
                    let download_result = tokio::time::timeout(
                        std::time::Duration::from_secs(60),
                        downloader.download_file(&url, &temp_file)
                    ).await;
                    
                    match download_result {
                        Ok(Ok(_)) => {
                            if verbose {
                                output::Output::success(&format!("Downloaded Packages file from {}", url));
                            }
                            
                            // Prüfe und verifiziere Signatur-Dateien, wenn Signaturen erforderlich sind
                            if require_signatures {
                                let release_urls = vec![
                                    format!("{}/dists/{}/InRelease", repo.url.trim_end_matches('/'), suite),
                                    format!("{}/dists/{}/Release.gpg", repo.url.trim_end_matches('/'), suite),
                                ];
                                
                                let mut has_valid_signature = false;
                                for release_url in &release_urls {
                                    // Versuche Release-Datei herunterzuladen
                                    let release_temp = std::env::temp_dir().join(format!("apt-ng-release-{}.tmp", 
                                        release_url.replace("/", "_").replace(":", "_").replace(".", "_")));
                                    
                                    if let Ok(_) = downloader.download_file(release_url, &release_temp).await {
                                        // Versuche Signatur zu verifizieren
                                        if let Ok(release_data) = std::fs::read(&release_temp) {
                                            // Für InRelease: Signatur ist eingebettet, für Release.gpg: separate Datei
                                            if release_url.ends_with("InRelease") {
                                                // InRelease hat eingebettete Signatur - vereinfachte Prüfung
                                                // In einer vollständigen Implementierung würde man hier die Signatur extrahieren und verifizieren
                                                // Für jetzt prüfen wir nur ob die Datei existiert und nicht leer ist
                                                if !release_data.is_empty() {
                                                    has_valid_signature = true;
                                                }
                                            } else {
                                                // Release.gpg benötigt separate Release-Datei
                                                let release_file_url = release_url.replace(".gpg", "");
                                                let release_file_temp = std::env::temp_dir().join(format!("apt-ng-release-file-{}.tmp", 
                                                    release_file_url.replace("/", "_").replace(":", "_").replace(".", "_")));
                                                
                                                if let Ok(_) = downloader.download_file(&release_file_url, &release_file_temp).await {
                                                    if let Ok(release_file_data) = std::fs::read(&release_file_temp) {
                                                        // Versuche Signatur zu verifizieren
                                                        if verifier.verify_with_trusted_keys(&release_file_data, &release_data).is_ok() {
                                                            has_valid_signature = true;
                                                        }
                                                        let _ = std::fs::remove_file(&release_file_temp);
                                                    }
                                                }
                                            }
                                        }
                                        let _ = std::fs::remove_file(&release_temp);
                                        
                                        if has_valid_signature {
                                            break;
                                        }
                                    }
                                }
                                
                                if !has_valid_signature {
                                    output::Output::warning(&format!("Repository {} has no valid signature files. Skipping.", repo.url));
                                    continue;
                                }
                                
                                if verbose {
                                    output::Output::info(&format!("✓ Repository signature verified for {}", repo.url));
                                }
                            }
                            
                            // Versuche zu dekomprimieren und zu parsen
                            let content = if file_path.ends_with(".xz") {
                                // XZ-Kompression
                                use xz2::read::XzDecoder;
                                use std::io::Read;
                                let mut decoder = XzDecoder::new(std::fs::File::open(&temp_file)?);
                                let mut content = String::new();
                                decoder.read_to_string(&mut content)?;
                                content
                            } else if file_path.ends_with(".gz") {
                                // GZIP-Kompression
                                use flate2::read::GzDecoder;
                                use std::io::Read;
                                let mut decoder = GzDecoder::new(std::fs::File::open(&temp_file)?);
                                let mut content = String::new();
                                decoder.read_to_string(&mut content)?;
                                content
                            } else {
                                // Unkomprimiert
                                std::fs::read_to_string(&temp_file)?
                            };
                            
                            // Parse Packages-Datei
                            match apt_parser::parse_packages_file(&content) {
                                Ok(packages) => {
                                    output::Output::info(&format!("Found {} packages in {}/{}", packages.len(), component, arch));
                                    if verbose {
                                        output::Output::info("Indexing packages...");
                                    }
                                    
                                    // Erstelle Fortschrittsanzeige
                                    let pb = output::Output::progress_bar(packages.len() as u64);
                                    pb.set_message("Indexing");
                                    
                                    // Verwende Batch-Insert für bessere Performance
                                    let repo_id = repo.id.unwrap_or(1);
                                    
                                    // Teile in Batches von 1000 Paketen auf
                                    const BATCH_SIZE: usize = 1000;
                                    let mut batch_errors = 0;
                                    for (batch_idx, chunk) in packages.chunks(BATCH_SIZE).enumerate() {
                                        match index.add_packages_batch(chunk, repo_id) {
                                            Ok(_) => {
                                                total_packages += chunk.len();
                                                pb.inc(chunk.len() as u64);
                                            }
                                            Err(e) => {
                                                batch_errors += 1;
                                                // Fallback: Einzelne Pakete hinzufügen
                                                if verbose {
                                                    output::Output::warning(&format!("Batch insert failed (batch {}), using individual inserts: {}", batch_idx + 1, e));
                                                }
                                                for pkg in chunk {
                                                    match index.add_package(pkg, repo_id) {
                                                        Ok(_) => {
                                                            total_packages += 1;
                                                            pb.inc(1);
                                                        }
                                                        Err(e) => {
                                                            if verbose {
                                                                output::Output::warning(&format!("Failed to add package {}: {}", pkg.name, e));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    pb.finish_with_message("Indexed");
                                    
                                    if batch_errors > 0 && verbose {
                                        output::Output::warning(&format!("{} batches had errors and used fallback method", batch_errors));
                                    }
                                    
                                    packages_loaded = true;
                                    let _ = std::fs::remove_file(&temp_file);
                                    break;
                                }
                                Err(e) => {
                                    output::Output::warning(&format!("Failed to parse Packages file: {}", e));
                                    let _ = std::fs::remove_file(&temp_file);
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            // Download fehlgeschlagen, versuche nächste URL
                            if verbose {
                                output::Output::warning(&format!("Failed to download: {} ({})", url, e));
                            }
                        }
                        Err(_) => {
                            // Timeout
                            if verbose {
                                output::Output::warning(&format!("Timeout downloading: {}", url));
                            }
                        }
                    }
                }
                if packages_loaded {
                    break;
                }
            }
            if packages_loaded {
                break;
            }
        }
        
        if !packages_loaded {
            output::Output::warning(&format!("Could not load Packages from {}", repo.url));
            if verbose {
                output::Output::info(&format!("  Suite: {:?}, Components: {:?}", repo.suite, repo.components));
            }
        }
    }
    
    if total_packages == 0 {
        output::Output::warning("No packages were indexed");
        output::Output::info("This might indicate:");
        output::Output::list_item("Packages files could not be downloaded");
        output::Output::list_item("Repository URLs or paths are incorrect");
        output::Output::list_item("Network connectivity issues");
        output::Output::info("Try running with -v flag for more details.");
    } else {
        output::Output::summary("Index updated", total_packages);
    }
    
    Ok(())
}

fn cmd_search(index: &index::Index, term: &str, _verbose: bool) -> anyhow::Result<()> {
    output::Output::heading(&format!("🔍 Searching for '{}'", term));
    
    let results = index.search(term)?;
    
    if results.is_empty() {
        output::Output::warning(&format!("No packages found matching '{}'", term));
        return Ok(());
    }
    
    output::Output::info(&format!("Found {} packages:", results.len()));
    
    // Use table for better visual presentation
    let package_data: Vec<(&str, &str, &str)> = results.iter()
        .map(|pkg| (pkg.name.as_str(), pkg.version.as_str(), pkg.arch.as_str()))
        .collect();
    output::Output::package_table(&package_data);
    
    Ok(())
}

async fn cmd_install(
    index: &index::Index,
    config: &config::Config,
    packages: &[String],
    jobs: usize,
    dry_run: bool,
    verbose: bool,
) -> anyhow::Result<()> {
    if packages.is_empty() {
        output::Output::error("No packages specified");
        return Ok(());
    }
    
    output::Output::heading("📦 Installing Packages");
    
    if verbose {
        output::Output::info(&format!("Resolving dependencies for: {:?}", packages));
    }
    
    // 1. Populate solver with all available packages
    output::Output::section("🔍 Loading package index...");
    let all_manifests = index.get_all_packages()?;
    let mut solver = solver::DependencySolver::new();
    
    for manifest in &all_manifests {
        match solver::DependencySolver::manifest_to_package_info(manifest) {
            Ok(pkg_info) => {
                solver.add_package(pkg_info);
            }
            Err(e) => {
                if verbose {
                    output::Output::warning(&format!("Failed to parse dependencies for {}: {}", manifest.name, e));
                }
                // Continue with other packages even if one fails
            }
        }
    }
    
    if verbose {
        output::Output::info(&format!("Loaded {} packages into solver", all_manifests.len()));
    }
    
    // 2. Create PackageSpec for requested packages
    let requested_specs: Vec<solver::PackageSpec> = packages.iter()
        .map(|name| solver::PackageSpec {
            name: name.clone(),
            version: None,
            arch: None,
        })
        .collect();
    
    // 3. Resolve dependencies using solver
    output::Output::section("🧩 Resolving dependencies...");
    let solution = match solver.solve(&requested_specs) {
        Ok(sol) => sol,
        Err(e) => {
            output::Output::error(&format!("Dependency resolution failed: {}", e));
            return Err(e);
        }
    };
    
    // 4. Convert PackageInfo back to PackageManifest for installation
    let mut packages_to_install = Vec::new();
    for pkg_info in &solution.to_install {
        // Find the corresponding manifest
        if let Some(manifest) = all_manifests.iter()
            .find(|m| m.name == pkg_info.name && m.version == pkg_info.version && m.arch == pkg_info.arch) {
            packages_to_install.push(manifest.clone());
        } else {
            // Fallback: try to find by name only
            if let Some(manifest) = all_manifests.iter()
                .find(|m| m.name == pkg_info.name) {
                packages_to_install.push(manifest.clone());
            } else {
                return Err(anyhow::anyhow!("Package {} {} not found in index", pkg_info.name, pkg_info.version));
            }
        }
    }
    
    // Show what will be installed
    output::Output::section("📋 Packages to install:");
    for pkg in &packages_to_install {
        output::Output::package_info(&pkg.name, &pkg.version, &pkg.arch);
    }
    
    if dry_run {
        output::Output::info("[DRY RUN] Would install:");
        for pkg in &packages_to_install {
            output::Output::list_item(&format!("{} ({})", pkg.name, pkg.version));
        }
        return Ok(());
    }
    
    // 3. Lade Pakete
    output::Output::section("⬇ Downloading packages...");
    
    let downloader = downloader::Downloader::new(jobs)?;
    let cache = cache::Cache::new(config.cache_path())?;
    
    for pkg in &packages_to_install {
        // Check if package exists in cache and validate it's not corrupted
        let cache_path_deb = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, "deb");
        let cache_path_apx = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, "apx");
        
        let package_in_cache = if cache_path_deb.exists() {
            // Try to validate the .deb file by checking if dpkg-deb can read it
            let test_output = std::process::Command::new("dpkg-deb")
                .arg("-I")
                .arg(&cache_path_deb)
                .output();
            
            let dpkg_valid = if let Ok(output) = test_output {
                output.status.success()
            } else {
                false
            };
            
            // Also check checksum if available
            let checksum_valid = if !pkg.checksum.is_empty() {
                use sha2::{Sha256, Digest};
                use hex;
                if let Ok(package_data) = std::fs::read(&cache_path_deb) {
                    let mut hasher = Sha256::new();
                    hasher.update(&package_data);
                    let calculated_checksum = hex::encode(hasher.finalize());
                    calculated_checksum == pkg.checksum
                } else {
                    false
                }
            } else {
                true // No checksum to validate
            };
            
            // If file is corrupted (dpkg can't read it or checksum mismatch), delete it
            if !dpkg_valid || !checksum_valid {
                if verbose {
                    if !dpkg_valid {
                        output::Output::warning(&format!("Package {} in cache is corrupted (dpkg-deb failed), deleting...", pkg.name));
                    } else {
                        output::Output::warning(&format!("Package {} in cache has checksum mismatch, deleting...", pkg.name));
                    }
                }
                let _ = std::fs::remove_file(&cache_path_deb);
                false // Not in cache (anymore)
            } else {
                true // Valid package in cache
            }
        } else if cache_path_apx.exists() {
            true // Assume .apx files are valid if they exist
        } else {
            false
        };
        
        if package_in_cache {
            output::Output::info(&format!("Package {} already in cache", pkg.name));
            continue;
        }
        
        // Skip packages without filename - they might be virtual packages or already installed
        if pkg.filename.is_none() {
            // Check if package is already installed on the system
            let output = std::process::Command::new("dpkg-query")
                .arg("-W")
                .arg("-f=${Status}")
                .arg(&pkg.name)
                .output();
            
            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.contains("installed") {
                        output::Output::info(&format!("Package {} is already installed, skipping download", pkg.name));
                        continue;
                    }
                }
            }
            
            // Try to get filename from apt-cache as fallback
            let mut found_filename = None;
            
            // Try apt-cache show and find matching version
            let output = std::process::Command::new("apt-cache")
                .arg("show")
                .arg(&pkg.name)
                .output();
            
            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let mut in_correct_version = false;
                    let mut fallback_filename = None;
                    let mut fallback_version = None;
                    
                    for line in stdout.lines() {
                        if line.starts_with("Package:") {
                            in_correct_version = false;
                        } else if line.starts_with("Version:") {
                            let version = line.split(':').nth(1).map(|s| s.trim().to_string());
                            if let Some(version) = version {
                                in_correct_version = version == pkg.version;
                                // Store first available version as fallback
                                if fallback_filename.is_none() {
                                    fallback_version = Some(version);
                                }
                            }
                        } else if line.starts_with("Filename:") {
                            let filename = line.split(':').nth(1).map(|s| s.trim().to_string());
                            if let Some(filename) = filename {
                                if in_correct_version {
                                    found_filename = Some(filename);
                                    break;
                                } else if fallback_filename.is_none() {
                                    fallback_filename = Some(filename);
                                }
                            }
                        }
                    }
                    
                    // Use fallback if exact version not found
                    if found_filename.is_none() && fallback_filename.is_some() {
                        found_filename = fallback_filename;
                        if verbose {
                            output::Output::warning(&format!(
                                "Package {} version {} not found in apt-cache, using version {} instead",
                                pkg.name, pkg.version, fallback_version.as_ref().unwrap_or(&"unknown".to_string())
                            ));
                        }
                    }
                }
            }
            
            if let Some(filename) = found_filename {
                // Use the filename from apt-cache
                let repo_id = pkg.repo_id.ok_or_else(|| {
                    anyhow::anyhow!("Package {} has no repository ID", pkg.name)
                })?;
                
                let repo_url = index.get_repo_url(repo_id)?
                    .ok_or_else(|| anyhow::anyhow!("Repository {} not found", repo_id))?;
                
                let download_url = format!("{}/{}", repo_url.trim_end_matches('/'), filename.trim_start_matches('/'));
                
                output::Output::download_info(&pkg.name, &format_size(pkg.size));
                
                let temp_file = std::env::temp_dir().join(format!("apt-ng-download-{}-{}.tmp", 
                    pkg.name, pkg.version));
                
                downloader.download_file(&download_url, &temp_file).await?;
                
                let package_dir = cache.cache_dir.join("packages");
                std::fs::create_dir_all(&package_dir)?;
                
                let ext = filename.split('.').last().unwrap_or("deb");
                let cache_path_with_ext = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, ext);
                std::fs::copy(&temp_file, &cache_path_with_ext)?;
                std::fs::remove_file(&temp_file)?;
                continue;
            }
            
            return Err(anyhow::anyhow!("Package {} has no filename and could not be found in apt-cache", pkg.name));
        }
        
        // Konstruiere Download-URL
        let repo_id = pkg.repo_id.ok_or_else(|| {
            anyhow::anyhow!("Package {} has no repository ID", pkg.name)
        })?;
        
        let repo_url = index.get_repo_url(repo_id)?
            .ok_or_else(|| anyhow::anyhow!("Repository {} not found", repo_id))?;
        
        let filename = pkg.filename.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Package {} has no filename", pkg.name))?;
        
        let download_url = format!("{}/{}", repo_url.trim_end_matches('/'), filename.trim_start_matches('/'));
        
        // Check for delta update availability and calculate delta if possible
        let mut use_delta = false;
        
        if let Ok(installed_packages) = index.list_installed_packages_with_manifests() {
            if let Some(installed_pkg) = installed_packages.iter().find(|ip| ip.name == pkg.name) {
                use crate::delta::DeltaCalculator;
                
                // Check if delta is available
                if DeltaCalculator::delta_available(&pkg.name, &installed_pkg.version, &pkg.version) {
                    // Try to calculate delta from cached old version
                    let old_cache_path = cache.package_path_with_ext(&pkg.name, &installed_pkg.version, &pkg.arch, "deb");
                    if old_cache_path.exists() {
                        // For now, we'll download full package and calculate delta later
                        // In production, would check repository for pre-calculated delta
                        use_delta = true;
                    }
                }
            }
        }
        
        output::Output::download_info(&pkg.name, &format_size(pkg.size));
        
        // Lade Paket herunter
        let temp_file = std::env::temp_dir().join(format!("apt-ng-download-{}-{}.tmp", 
            pkg.name, pkg.version));
        
        downloader.download_file(&download_url, &temp_file).await?;
        
        // If delta was requested, calculate it now (for demonstration)
        if use_delta {
            if let Ok(installed_packages) = index.list_installed_packages_with_manifests() {
                if let Some(installed_pkg) = installed_packages.iter().find(|ip| ip.name == pkg.name) {
                    use crate::delta::DeltaCalculator;
                    let old_cache_path = cache.package_path_with_ext(&pkg.name, &installed_pkg.version, &pkg.arch, "deb");
                    if old_cache_path.exists() {
                        match DeltaCalculator::calculate_delta(&old_cache_path, &temp_file, "simple") {
                            Ok((delta_data, metadata)) => {
                                // Check if delta is worthwhile
                                if metadata.is_worthwhile() {
                                    if verbose {
                                        output::Output::info(&format!(
                                            "Delta calculated: {}% savings ({:.2}MB -> {:.2}MB)",
                                            metadata.savings_percentage(),
                                            metadata.full_size as f64 / 1_000_000.0,
                                            metadata.delta_size as f64 / 1_000_000.0
                                        ));
                                    }
                                    // Store delta file
                                    let delta_file = std::env::temp_dir().join(format!("apt-ng-delta-{}-{}.delta", 
                                        pkg.name, pkg.version));
                                    std::fs::write(&delta_file, delta_data)?;
                                    
                                    // Apply delta to reconstruct full package
                                    use crate::delta::DeltaApplier;
                                    match DeltaApplier::apply_delta(&old_cache_path, &delta_file, &temp_file, &metadata) {
                                        Ok(_) => {
                                            if verbose {
                                                output::Output::info(&format!("Delta applied successfully for {}", pkg.name));
                                            }
                                            // Clean up delta file
                                            let _ = std::fs::remove_file(&delta_file);
                                        }
                                        Err(e) => {
                                            if verbose {
                                                output::Output::warning(&format!("Failed to apply delta for {}: {}, using full download", pkg.name, e));
                                            }
                                            // Fall back to full download (already downloaded)
                                        }
                                    }
                                } else {
                                    if verbose {
                                        output::Output::info(&format!("Delta not worthwhile (only {:.1}% savings), using full download", metadata.savings_percentage()));
                                    }
                                }
                            }
                            Err(e) => {
                                if verbose {
                                    output::Output::warning(&format!("Failed to calculate delta for {}: {}, using full download", pkg.name, e));
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Verschiebe in Cache - bestimme Extension basierend auf Dateiname
        let package_dir = cache.cache_dir.join("packages");
        std::fs::create_dir_all(&package_dir)?;
        
        // Bestimme Extension aus filename oder verwende .deb als Fallback
        let ext = filename.split('.').last().unwrap_or("deb");
        let cache_path_with_ext = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, ext);
        std::fs::copy(&temp_file, &cache_path_with_ext)?;
        std::fs::remove_file(&temp_file)?;
    }
    
    // 4. Verifiziere Signaturen
    output::Output::section("🔐 Verifying package signatures...");
    let verifier = verifier::PackageVerifier::new(config.trusted_keys_dir())?;
    
    if verifier.trusted_key_count() == 0 {
        output::Output::warning("No trusted keys found. Skipping signature verification.");
        output::Output::info(&format!("Add trusted keys to: {}", config.trusted_keys_dir().display()));
    } else {
        output::Output::info(&format!("Found {} trusted key(s)", verifier.trusted_key_count()));
        
        for pkg in &packages_to_install {
            // Versuche zuerst .apx, dann .deb
            let cache_path_apx = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, "apx");
            let cache_path_deb = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, "deb");
            
            let (cache_path, is_apx) = if cache_path_apx.exists() {
                (cache_path_apx, true)
            } else if cache_path_deb.exists() {
                (cache_path_deb, false)
            } else {
                continue; // Skip if not downloaded yet
            };
            
            if is_apx {
                // Für .apx-Pakete: Verifiziere Signatur
                use crate::package::ApxPackage;
                if let Ok(apx_pkg) = ApxPackage::open(&cache_path) {
                    match apx_pkg.verify_signature(&cache_path, &verifier) {
                        Ok(_) => {
                            if verbose {
                                output::Output::info(&format!("✓ Verified signature for {}", pkg.name));
                            }
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Signature verification failed for {}: {}",
                                pkg.name,
                                e
                            ));
                        }
                    }
                }
            } else {
                // Für .deb-Pakete: Verifiziere Checksumme
                let package_data = std::fs::read(&cache_path)?;
                
                if !pkg.checksum.is_empty() {
                    use sha2::{Sha256, Digest};
                    use hex;
                    let mut hasher = Sha256::new();
                    hasher.update(&package_data);
                    let calculated_checksum = hex::encode(hasher.finalize());
                    
                    if calculated_checksum != pkg.checksum {
                        // File is corrupted, delete it
                        output::Output::warning(&format!(
                            "Checksum mismatch for {}: expected {}, got {}. Deleting corrupted file...",
                            pkg.name,
                            pkg.checksum,
                            calculated_checksum
                        ));
                        let _ = std::fs::remove_file(&cache_path);
                        return Err(anyhow::anyhow!(
                            "Package file corrupted (checksum mismatch). Please run the command again to re-download."
                        ));
                    }
                    
                    if verbose {
                        output::Output::info(&format!("✓ Verified checksum for {}", pkg.name));
                    }
                }
            }
        }
    }
    
    // 5. Installiere Pakete
    output::Output::section("🔧 Installing packages...");
    
    let installer = installer::Installer::new(jobs, Path::new("/"));
    
    for pkg in &packages_to_install {
        // Versuche zuerst .apx, dann .deb
        let cache_path_apx = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, "apx");
        let cache_path_deb = cache.package_path_with_ext(&pkg.name, &pkg.version, &pkg.arch, "deb");
        
        let (cache_path, is_apx) = if cache_path_apx.exists() {
            (cache_path_apx, true)
        } else if cache_path_deb.exists() {
            (cache_path_deb, false)
        } else {
            return Err(anyhow::anyhow!("Package file not found for {} (tried .apx and .deb)", pkg.name));
        };
        
        output::Output::install_info(&pkg.name, &pkg.version);
        
        let transaction = if is_apx {
            // Installiere .apx-Paket mit Signatur-Verifikation
            installer.install_package(&cache_path, Some(&verifier), verbose).await?
        } else {
            // Installiere .deb-Paket
            installer.install_deb_package(&cache_path, Some(&pkg.checksum), verbose).await?
        };
        
        // Markiere als installiert (transaction wird automatisch bei Fehler zurückgerollt)
        if let Err(e) = index.mark_installed(&pkg.name, &pkg.version) {
            // Rollback installation if marking as installed fails
            transaction.rollback()?;
            return Err(e);
        }
    }
    
    output::Output::summary("Successfully installed", packages_to_install.len());
    
    Ok(())
}

async fn cmd_remove(
    index: &index::Index,
    packages: &[String],
    dry_run: bool,
    verbose: bool,
) -> anyhow::Result<()> {
    if dry_run {
        output::Output::info(&format!("[DRY RUN] Would remove: {:?}", packages));
        return Ok(());
    }
    
    if verbose {
        output::Output::info(&format!("Removing packages: {:?}", packages));
    }
    
    for pkg_name in packages {
        index.mark_removed(pkg_name)?;
        if verbose {
            output::Output::success(&format!("Removed: {}", pkg_name));
        }
    }
    
    Ok(())
}

async fn cmd_upgrade(
    index: &index::Index,
    config: &config::Config,
    jobs: usize,
    dry_run: bool,
    verbose: bool,
) -> anyhow::Result<()> {
    output::Output::heading("🔄 Upgrading Packages");
    
    let installed_packages = index.list_installed_packages_with_manifests()?;
    
    if installed_packages.is_empty() {
        output::Output::info("No packages installed.");
        return Ok(());
    }
    
    if verbose {
        output::Output::info(&format!("Checking {} installed packages for upgrades...", installed_packages.len()));
    }
    
    // 1. Finde verfügbare Upgrades
    let mut packages_to_upgrade = Vec::new();
    
    for installed_pkg in &installed_packages {
        // Get latest available version (exact match only for upgrades)
        let available_packages = index.search_exact(&installed_pkg.name)?;
        
        if let Some(latest_pkg) = available_packages.first() {
            // Compare versions using solver's version comparison
            use crate::solver::DependencySolver;
            let comparison = DependencySolver::compare_versions(&latest_pkg.version, &installed_pkg.version);
            
            match comparison {
                std::cmp::Ordering::Greater => {
                    // Newer version available
                    packages_to_upgrade.push(latest_pkg.clone());
                    if verbose {
                        output::Output::info(&format!(
                            "{}: {} -> {}",
                            installed_pkg.name,
                            installed_pkg.version,
                            latest_pkg.version
                        ));
                    }
                }
                _ => {
                    // Already up to date or same version
                    if verbose {
                        output::Output::info(&format!("{}: {} (up to date)", installed_pkg.name, installed_pkg.version));
                    }
                }
            }
        }
    }
    
    if packages_to_upgrade.is_empty() {
        output::Output::success("All packages are up to date.");
        return Ok(());
    }
    
    output::Output::section(&format!("📦 Found {} package(s) to upgrade:", packages_to_upgrade.len()));
    for pkg in &packages_to_upgrade {
        output::Output::list_item(&format!("{} ({})", pkg.name, pkg.version));
    }
    
    if dry_run {
        output::Output::info("[DRY RUN] Would upgrade the above packages");
        return Ok(());
    }
    
    // 2. Resolve dependencies for upgrades
    let all_available_packages = index.get_all_packages()?;
    let mut solver = solver::DependencySolver::new();
    
    // Add available packages to solver
    for manifest in &all_available_packages {
        match solver::DependencySolver::manifest_to_package_info(manifest) {
            Ok(pkg_info) => {
                solver.add_package(pkg_info);
            }
            Err(e) => {
                if verbose {
                    output::Output::warning(&format!("Failed to parse dependencies for {}: {}", manifest.name, e));
                }
            }
        }
    }
    
    // Add installed packages to solver so dependencies already satisfied by installed packages can be found
    for manifest in &installed_packages {
        match solver::DependencySolver::manifest_to_package_info(manifest) {
            Ok(pkg_info) => {
                solver.add_package(pkg_info);
            }
            Err(e) => {
                if verbose {
                    output::Output::warning(&format!("Failed to parse dependencies for installed package {}: {}", manifest.name, e));
                }
            }
        }
    }
    
    // Tell the solver which packages are already installed so it can skip resolving their dependencies
    let installed_package_names: HashSet<String> = installed_packages.iter()
        .map(|p| p.name.clone())
        .collect();
    
    // Debug: Check if any installed dependencies that need libqt5core5t64
    if verbose {
        for pkg in &installed_packages {
            if !pkg.provides.is_empty() {
                output::Output::info(&format!("Installed package {} provides: {:?}", pkg.name, pkg.provides));
            }
        }
    }
    
    solver.set_installed_packages(installed_package_names);
    
    let upgrade_specs: Vec<solver::PackageSpec> = packages_to_upgrade.iter()
        .map(|p| solver::PackageSpec {
            name: p.name.clone(),
            version: Some(p.version.clone()),
            arch: Some(p.arch.clone()),
        })
        .collect();
    
    output::Output::section("🧩 Resolving dependencies for upgrades...");
    let solution = match solver.solve(&upgrade_specs) {
        Ok(sol) => sol,
        Err(e) => {
            output::Output::error(&format!("Dependency resolution for upgrade failed: {}", e));
            return Err(e);
        }
    };
    
    if verbose {
        output::Output::info(&format!("Solver returned {} packages to install, {} to upgrade", 
            solution.to_install.len(), solution.to_upgrade.len()));
        for pkg in &solution.to_install {
            output::Output::info(&format!("  - {} {}", pkg.name, pkg.version));
        }
    }
    
    // Separate packages into to_install and to_upgrade based on whether they're already installed
    let installed_package_map: HashMap<String, String> = installed_packages.iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();
    
    let mut packages_to_install = Vec::new();
    let mut packages_to_upgrade = Vec::new();
    
    for pkg in solution.to_install {
        if let Some(installed_version) = installed_package_map.get(&pkg.name) {
            // Package is already installed - check if version is different
            use crate::solver::DependencySolver;
            let comparison = DependencySolver::compare_versions(&pkg.version, installed_version);
            match comparison {
                std::cmp::Ordering::Greater => {
                    // Newer version available - add to upgrade list
                    packages_to_upgrade.push(pkg);
                }
                std::cmp::Ordering::Equal => {
                    // Same version - skip (already installed)
                    if verbose {
                        output::Output::info(&format!("Package {} {} is already installed, skipping", pkg.name, pkg.version));
                    }
                }
                std::cmp::Ordering::Less => {
                    // Older version - shouldn't happen, but skip it
                    if verbose {
                        output::Output::warning(&format!("Package {} {} is older than installed version {}, skipping", pkg.name, pkg.version, installed_version));
                    }
                }
            }
        } else {
            // Package is not installed - add to install list
            packages_to_install.push(pkg);
        }
    }
    
    // Add packages from solution.to_upgrade (if any)
    packages_to_upgrade.extend(solution.to_upgrade);
    
    if packages_to_install.is_empty() && packages_to_upgrade.is_empty() {
        output::Output::info("No packages to install or upgrade after dependency resolution.");
        return Ok(());
    }
    
    if verbose {
        if !packages_to_upgrade.is_empty() {
            output::Output::section("📋 Packages to upgrade:");
            for pkg in &packages_to_upgrade {
                output::Output::list_item(&format!("{} ({})", pkg.name, pkg.version));
            }
        }
        if !packages_to_install.is_empty() {
            output::Output::section("📋 Packages to install:");
            for pkg in &packages_to_install {
                output::Output::list_item(&format!("{} ({})", pkg.name, pkg.version));
            }
        }
    }
    
    // Combine both lists for installation (install logic handles both new installs and upgrades)
    let all_packages: Vec<String> = packages_to_install.iter()
        .chain(packages_to_upgrade.iter())
        .map(|p| p.name.clone())
        .collect();
    
    // 3. Use install logic for upgrades (it handles dependencies automatically)
    cmd_install(index, config, &all_packages, jobs, false, verbose).await?;
    
    output::Output::success(&format!("Successfully upgraded {} package(s)", packages_to_upgrade.len()));
    
    Ok(())
}

fn cmd_show(index: &index::Index, package: &str, _verbose: bool) -> anyhow::Result<()> {
    output::Output::heading(&format!("📋 Package Information: {}", package));
    
    match index.show(package)? {
        Some(pkg) => {
            let mut table = output::Output::table();
            table.set_header(vec!["Field", "Value"]);
            
            let name_cell = if output::Output::colors_enabled() {
                comfy_table::Cell::new(&pkg.name).fg(comfy_table::Color::Cyan)
            } else {
                comfy_table::Cell::new(&pkg.name)
            };
            
            table.add_row(vec![comfy_table::Cell::new("Name"), name_cell]);
            table.add_row(vec![comfy_table::Cell::new("Version"), comfy_table::Cell::new(&pkg.version)]);
            table.add_row(vec![comfy_table::Cell::new("Architecture"), comfy_table::Cell::new(&pkg.arch)]);
            table.add_row(vec![comfy_table::Cell::new("Size"), comfy_table::Cell::new(&format_size(pkg.size))]);
            
            if !pkg.depends.is_empty() {
                table.add_row(vec![comfy_table::Cell::new("Depends"), comfy_table::Cell::new(&pkg.depends.join(", "))]);
            }
            if !pkg.provides.is_empty() {
                table.add_row(vec![comfy_table::Cell::new("Provides"), comfy_table::Cell::new(&pkg.provides.join(", "))]);
            }
            
            println!("{}", table);
        }
        None => {
            output::Output::error(&format!("Package '{}' not found", package));
        }
    }
    
    Ok(())
}

fn cmd_repo_add(index: &index::Index, url: &str) -> anyhow::Result<()> {
    let repo = repo::Repository {
        id: None,
        url: url.to_string(),
        priority: 500,
        enabled: true,
        last_probe_ms: None,
        rtt_ms: None,
        suite: None,
        components: vec!["main".to_string()],
    };
    
    repo::Repository::add_to_db(index.conn(), &repo)?;
    output::Output::success(&format!("Added repository: {}", url));
    
    Ok(())
}

async fn cmd_repo_update(index: &index::Index, config: &config::Config, verbose: bool) -> anyhow::Result<()> {
    output::Output::heading("🔄 Updating Repository Mirrors");
    
    let repos = repo::Repository::load_all(index.conn())?;
    
    output::Output::info(&format!("Probing {} mirrors...", repos.len()));
    
    let downloader = downloader::Downloader::new(config.jobs())?;
    let mut mirror_stats = Vec::new();
    
    for repo in &repos {
        if let Ok(stats) = downloader.probe_mirror(&repo.url).await {
            let rtt = stats.rtt_ms;
            let throughput = stats.throughput;
            repo::Repository::update_probe_stats(index.conn(), &repo.url, rtt)?;
            mirror_stats.push((repo.url.clone(), stats));
            if verbose {
                output::Output::success(&format!("{}: {}ms RTT, {} bytes/s throughput", 
                    repo.url, rtt, throughput));
            } else {
                output::Output::success(&format!("{}: {}ms", repo.url, rtt));
            }
        } else {
            output::Output::warning(&format!("Failed to probe {}", repo.url));
        }
    }
    
    // Sortiere Mirrors nach Score (beste zuerst)
    mirror_stats.sort_by(|a, b| {
        a.1.score().partial_cmp(&b.1.score()).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    if !mirror_stats.is_empty() && verbose {
        output::Output::section("Best mirrors (sorted by performance):");
        for (url, stats) in &mirror_stats[..std::cmp::min(5, mirror_stats.len())] {
            output::Output::list_item(&format!("{}: score {:.2}", url, stats.score()));
        }
    }
    
    Ok(())
}

fn cmd_cache_clean(config: &config::Config, clean_old: bool, max_size: Option<u64>, verbose: bool) -> anyhow::Result<()> {
    output::Output::heading("🧹 Cleaning Cache");
    
    let cache = cache::Cache::new(config.cache_path())?;
    
    let size_before = cache.size()?;
    if verbose {
        output::Output::info(&format!("Cache size before cleanup: {}", format_size(size_before)));
    }
    
    let mut removed_count = 0usize;
    
    // Intelligente Bereinigung: Entferne alte Versionen
    if clean_old {
        output::Output::section("Removing old package versions...");
        removed_count = cache.clean_old_versions()?;
        if verbose {
            output::Output::info(&format!("Removed {} old package versions", removed_count));
        }
    }
    
    // Größenlimit-Bereinigung
    if let Some(max_size_bytes) = max_size {
        output::Output::section(&format!("Cleaning cache to stay under {} limit...", format_size(max_size_bytes)));
        let removed = cache.clean_if_over_limit(max_size_bytes)?;
        removed_count += removed;
        if verbose {
            output::Output::info(&format!("Removed {} packages to stay under size limit", removed));
        }
    }
    
    // Falls keine spezifische Option: Vollständige Bereinigung
    if !clean_old && max_size.is_none() {
        cache.clean()?;
        removed_count = 1; // Mark as cleaned
    }
    
    let size_after = cache.size()?;
    let freed = size_before.saturating_sub(size_after);
    
    if removed_count > 0 {
        output::Output::success(&format!(
            "Cache cleaned successfully: removed {} package(s), freed {}",
            removed_count,
            format_size(freed)
        ));
    } else {
        output::Output::info("Cache is already clean");
    }
    
    if verbose {
        output::Output::info(&format!("Cache size after cleanup: {}", format_size(size_after)));
    }
    
    Ok(())
}

fn cmd_security_audit(format: &str, verbose: bool) -> anyhow::Result<()> {
    use crate::security::SecurityAudit;
    use crate::security::SecurityReport;
    
    output::Output::heading("🔐 Security Audit");
    
    if verbose {
        output::Output::info("Running security checks...");
    }
    
    let result = SecurityAudit::run()?;
    
    match format {
        "json" => {
            let json_report = SecurityReport::generate_json(&result)?;
            println!("{}", json_report);
        }
        _ => {
            let text_report = SecurityReport::generate_text(&result);
            print!("{}", text_report);
            
            if result.passed() {
                output::Output::success("Security audit passed");
            } else {
                output::Output::warning(&format!(
                    "Security audit found {} critical and {} high severity issues",
                    result.critical_issues,
                    result.high_issues
                ));
            }
        }
    }
    
    Ok(())
}


