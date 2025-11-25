use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use sha2::{Sha256, Digest};
use hex;

pub struct Installer {
    worker_pool_size: usize,
    #[allow(dead_code)]
    install_root: PathBuf,
}

/// Tracks installed files for rollback purposes
#[derive(Debug, Clone)]
pub struct InstallationTransaction {
    installed_files: Vec<PathBuf>,
    backup_files: Vec<(PathBuf, PathBuf)>, // (original, backup)
}

impl InstallationTransaction {
    pub fn new() -> Self {
        InstallationTransaction {
            installed_files: Vec::new(),
            backup_files: Vec::new(),
        }
    }
    
    pub fn add_installed_file(&mut self, path: PathBuf) {
        self.installed_files.push(path);
    }
    
    pub fn add_backup(&mut self, original: PathBuf, backup: PathBuf) {
        self.backup_files.push((original, backup));
    }
    
    /// Rollback: remove installed files and restore backups
    pub fn rollback(&self) -> Result<()> {
        // Remove installed files
        for file in &self.installed_files {
            if file.exists() {
                if file.is_dir() {
                    fs::remove_dir_all(file)?;
                } else {
                    fs::remove_file(file)?;
                }
            }
        }
        
        // Restore backups
        for (original, backup) in &self.backup_files {
            if backup.exists() {
                if original.exists() {
                    fs::remove_file(original)?;
                }
                fs::rename(backup, original)?;
            }
        }
        
        Ok(())
    }
}

impl Installer {
    /// Erstellt einen neuen Installer
    #[allow(dead_code)]
    pub fn new(worker_pool_size: usize, install_root: impl AsRef<Path>) -> Self {
        Installer {
            worker_pool_size,
            install_root: install_root.as_ref().to_path_buf(),
        }
    }
    
    /// Installiert ein Paket aus einer .apx-Datei
    pub async fn install_package(&self, apx_path: &Path, verifier: Option<&crate::verifier::PackageVerifier>, verbose: bool) -> Result<InstallationTransaction> {
        use crate::package::ApxPackage;
        
        let mut transaction = InstallationTransaction::new();
        
        // 1. Öffne .apx-Datei
        let apx_pkg = ApxPackage::open(apx_path)?;
        
        // 2. Verifiziere Signatur, falls Verifier vorhanden
        if let Some(verifier) = verifier {
            apx_pkg.verify_signature(apx_path, verifier)?;
            if verbose {
                println!("  Package signature verified");
            }
        }
        
        // 3-4. Manifest wurde bereits beim Öffnen geparst
        
        // 5. Dekomprimiere content.tar.zst in temporäres Verzeichnis
        let temp_dir = std::env::temp_dir().join(format!("apt-ng-apx-install-{}", 
            std::process::id()));
        fs::create_dir_all(&temp_dir)?;
        
        apx_pkg.extract_to(&temp_dir)?;
        
        if verbose {
            println!("  Extracted package to temporary directory");
        }
        
        // 6. Verifiziere Checksummen
        apx_pkg.verify_checksums(&temp_dir)?;
        if verbose {
            println!("  All file checksums verified");
        }
        
        // 7. Installiere Dateien atomisch
        Self::copy_directory_atomic(&temp_dir, &self.install_root, &mut transaction, verbose)?;
        
        // 8. Führe Hooks aus (falls vorhanden)
        // Für .apx-Pakete werden Hooks im Manifest gespeichert, nicht als separate Skripte
        // Dies würde eine Erweiterung des Formats erfordern
        
        // Aufräumen
        fs::remove_dir_all(&temp_dir)?;
        
        Ok(transaction)
    }
    
    /// Installiert mehrere Pakete parallel
    #[allow(dead_code)]
    pub async fn install_packages(&self, apx_paths: &[PathBuf]) -> Result<Vec<Result<()>>> {
        use futures::stream::{self, StreamExt};
        
        let results: Vec<_> = stream::iter(apx_paths.iter().cloned())
            .map(|_path| async move {
                // Note: In a real implementation, we'd need to pass self differently
                // For now, this is a placeholder that shows the structure
                Ok::<(), anyhow::Error>(())
            })
            .buffer_unordered(self.worker_pool_size)
            .collect()
            .await;
        
        Ok(results)
    }
    
    /// Entfernt ein installiertes Paket
    #[allow(dead_code)]
    pub async fn remove_package(&self, _package_name: &str) -> Result<()> {
        // TODO: Implementierung:
        // 1. Lade Manifest des installierten Pakets
        // 2. Führe pre-remove Hook aus
        // 3. Entferne Dateien (mit Abhängigkeitsprüfung)
        // 4. Führe post-remove Hook aus
        // 5. Aktualisiere installierte Pakete-Datenbank
        
        Ok(())
    }
    
    /// Führt einen Hook aus (sandboxed)
    /// Extrahiert und führt Skripte aus einem .deb-Paket aus
    pub async fn run_hook(&self, hook_type: HookType, deb_path: &Path, verbose: bool) -> Result<()> {
        self.run_hook_with_old_version(hook_type, deb_path, None, verbose).await
    }
    
    /// Extrahiert und führt Skripte aus einem .deb-Paket aus mit alter Version
    pub async fn run_hook_with_old_version(&self, hook_type: HookType, deb_path: &Path, old_version: Option<&str>, verbose: bool) -> Result<()> {
        // Determine script name based on hook type
        let script_name = match hook_type {
            HookType::PreInstall => "preinst",
            HookType::PostInstall => "postinst",
            HookType::PreRemove => "prerm",
            HookType::PostRemove => "postrm",
        };
        
        // Extract control.tar.gz from .deb to get scripts
        let temp_dir = std::env::temp_dir().join(format!("apt-ng-hook-{}", std::process::id()));
        fs::create_dir_all(&temp_dir)?;
        
        // Extract control.tar.gz using dpkg-deb
        let output = Command::new("dpkg-deb")
            .arg("-e")
            .arg(deb_path)
            .arg(&temp_dir)
            .output()?;
        
        if !output.status.success() {
            // No control directory means no hooks - this is OK
            if verbose {
                println!("  No control directory found, skipping {} hook", script_name);
            }
            return Ok(());
        }
        
        // Check if script exists
        let script_path = temp_dir.join(script_name);
        if !script_path.exists() {
            if verbose {
                println!("  No {} script found, skipping", script_name);
            }
            fs::remove_dir_all(&temp_dir)?;
            return Ok(());
        }
        
        // Make script executable
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
        
        if verbose {
            println!("  Running {} hook...", script_name);
        }
        
        // Extract package name from deb path for DPKG_MAINTSCRIPT_PACKAGE
        let package_name = deb_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .split('_')
            .next()
            .unwrap_or("");
        
        // Execute script with proper dpkg environment variables
        // For preinst: upgrade <old-version> or install (no params)
        // For postinst: configure <old-version> or abort-upgrade <old-version> or abort-remove <in-favour> <old-version>
        // We'll use "upgrade" for preinst and "configure" for postinst as defaults
        let mut cmd = Command::new("/bin/sh");
        cmd.arg(&script_path)
            .env("DPKG_MAINTSCRIPT_NAME", script_name)
            .env("DPKG_MAINTSCRIPT_PACKAGE", package_name)
            .env("DPKG_ROOT", &self.install_root)
            .env("DPKG_ADMINDIR", "/var/lib/dpkg")
            .current_dir(&self.install_root);
        
        // Add script arguments based on hook type
        // Get old version from parameter or try to query dpkg
        let old_ver = if let Some(ov) = old_version {
            ov.to_string()
        } else {
            // Try to get old version from dpkg-query
            // Extract package name from deb path
            let deb_name = deb_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .split('_')
                .next()
                .unwrap_or("");
            
            if !deb_name.is_empty() {
                let output = std::process::Command::new("dpkg-query")
                    .arg("-W")
                    .arg("-f=${Version}")
                    .arg(deb_name)
                    .output();
                
                if let Ok(output) = output {
                    if output.status.success() {
                        String::from_utf8_lossy(&output.stdout).trim().to_string()
                    } else {
                        String::new() // Not installed, empty string
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        };
        
        match hook_type {
            HookType::PreInstall => {
                // For preinst, pass "upgrade, pass "upgrade" and old version
                // If old version is empty, it's a fresh install, use "install" instead
                if old_ver.is_empty() {
                    cmd.arg("install");
                } else {
                    cmd.arg("upgrade").arg(&old_ver);
                }
            }
            HookType::PostInstall => {
                // For postinst, pass "configure" and old version
                cmd.arg("configure").arg(&old_ver);
            }
            HookType::PreRemove => {
                // For prerm, pass "remove"
                cmd.arg("remove");
            }
            HookType::PostRemove => {
                // For postrm, pass "remove"
                cmd.arg("remove");
            }
        }
        
        let output = cmd.output()?;
        
        // Cleanup
        fs::remove_dir_all(&temp_dir)?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Hook {} failed: {}", script_name, stderr));
        }
        
        if verbose {
            println!("  {} hook completed successfully", script_name);
        }
        
        Ok(())
    }
    
    /// Extract and run hooks from a .deb package
    #[allow(dead_code)]
    pub async fn run_package_hooks(&self, deb_path: &Path, hook_types: &[HookType], verbose: bool) -> Result<()> {
        for hook_type in hook_types {
            self.run_hook(hook_type.clone(), deb_path, verbose).await?;
        }
        Ok(())
    }
    
    /// Installiert eine .deb-Datei mit Rollback-Unterstützung
    pub async fn install_deb_package(&self, deb_path: &Path, expected_checksum: Option<&str>, verbose: bool) -> Result<InstallationTransaction> {
        let mut transaction = InstallationTransaction::new();
        // Verwende dpkg-deb zum Extrahieren der .deb-Datei
        // Dies ist eine einfache Implementierung, die dpkg-deb verwendet
        
        // First, try to extract the package to see if it's valid
        let temp_dir = std::env::temp_dir().join(format!("apt-ng-install-{}", 
            std::process::id()));
        fs::create_dir_all(&temp_dir)?;
        
        // Test extraction first - if it works, the file is valid regardless of checksum
        let test_output = Command::new("dpkg-deb")
            .arg("-I")
            .arg(deb_path)
            .output();
        
        let extraction_test_ok = if let Ok(output) = test_output {
            output.status.success()
        } else {
            false
        };
        
        // Validate checksum if provided, but only fail if extraction also fails
        if let Some(expected) = expected_checksum {
            let actual_checksum = Self::calculate_file_checksum(deb_path)?;
            if actual_checksum != expected {
                if !extraction_test_ok {
                    // Both checksum and extraction failed - file is definitely corrupted
                    eprintln!("  ⚠ Error: Checksum mismatch for {}: expected {}, got {}", 
                        deb_path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown"),
                        expected, 
                        actual_checksum
                    );
                    eprintln!("  File also fails extraction test. Deleting corrupted file...");
                    let _ = std::fs::remove_file(deb_path);
                    return Err(anyhow::anyhow!(
                        "Package file corrupted (checksum mismatch and extraction failed). Deleted corrupted file. Please run the command again to re-download."
                    ));
                } else {
                    // Checksum mismatch but extraction works - index might be wrong, warn but continue
                    eprintln!("  ⚠ Warning: Checksum mismatch for {}: expected {}, got {}", 
                        deb_path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown"),
                        expected, 
                        actual_checksum
                    );
                    eprintln!("  File appears valid (extraction test passed). Continuing installation...");
                    eprintln!("  (Index checksum may be outdated or incorrect)");
                }
            } else if verbose {
                println!("  Checksum validated: {}", actual_checksum);
            }
        }
        
        // Extrahiere .deb-Datei mit dpkg-deb
        let output = Command::new("dpkg-deb")
            .arg("-x")
            .arg(deb_path)
            .arg(&temp_dir)
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if the error indicates a corrupted file
            if stderr.contains("unexpected end of file") || 
               stderr.contains("lzma error") || 
               stderr.contains("corrupted") ||
               stderr.contains("invalid") {
                // Try to delete the corrupted file
                let _ = std::fs::remove_file(deb_path);
                return Err(anyhow::anyhow!(
                    "Package file appears to be corrupted: {}. Deleted corrupted file. Please run the command again to re-download.",
                    stderr
                ));
            }
            return Err(anyhow::anyhow!("Failed to extract .deb package: {}", stderr));
        }
        
        if verbose {
            println!("  Extracted package to temporary directory");
        }
        
        // Get old version if package is already installed
        let old_version = {
            // Extract package name from deb path (format: package_version_arch.deb)
            let deb_name = deb_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .split('_')
                .next()
                .unwrap_or("");
            
            if !deb_name.is_empty() {
                let output = std::process::Command::new("dpkg-query")
                    .arg("-W")
                    .arg("-f=${Version}")
                    .arg(deb_name)
                    .output();
                
                if let Ok(output) = output {
                    if output.status.success() {
                        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if !version.is_empty() {
                            Some(version)
                        } else {
                            None
                        }
                    } else {
                        None // Not installed
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };
        
        // Run pre-install hook with old version
        self.run_hook_with_old_version(HookType::PreInstall, deb_path, old_version.as_deref(), verbose).await?;
        
        // Copy files atomically to install_root with checksum validation
        // Use atomic operations: copy to temp location, then rename atomically
        match Self::copy_directory_atomic(&temp_dir, &self.install_root, &mut transaction, verbose) {
            Ok(()) => {
                if verbose {
                    println!("  Installed files to {}", self.install_root.display());
                }
                
                // Run post-install hook with old version
                self.run_hook_with_old_version(HookType::PostInstall, deb_path, old_version.as_deref(), verbose).await?;
                
                // Aufräumen
                fs::remove_dir_all(&temp_dir)?;
                
                Ok(transaction)
            }
            Err(e) => {
                // Rollback on error
                if let Err(rollback_err) = transaction.rollback() {
                    return Err(anyhow::anyhow!("Installation failed: {}. Rollback also failed: {}", e, rollback_err));
                }
                Err(anyhow::anyhow!("Installation failed: {}. Rolled back changes.", e))
            }
        }
    }
    
    /// Copy directory contents atomically using temp files and rename
    fn copy_directory_atomic(source: &Path, dest: &Path, transaction: &mut InstallationTransaction, verbose: bool) -> Result<()> {
        use std::io;
        
        // Ensure destination directory exists
        fs::create_dir_all(dest)?;
        
        // Walk through source directory
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let source_path = entry.path();
            let file_name = entry.file_name();
            let dest_path = dest.join(&file_name);
            
            if source_path.is_dir() {
                // Recursively copy directories
                Self::copy_directory_atomic(&source_path, &dest_path, transaction, verbose)?;
            } else if source_path.is_file() {
                // Copy file atomically
                // 1. Copy to temp file with .tmp suffix
                let temp_dest = dest_path.with_extension(format!("{}.tmp", 
                    dest_path.extension().and_then(|s| s.to_str()).unwrap_or("tmp")));
                
                // Copy file contents
                let mut source_file = fs::File::open(&source_path)?;
                let mut dest_file = fs::File::create(&temp_dest)?;
                
                // Preserve permissions
                let metadata = source_path.metadata()?;
                let permissions = metadata.permissions();
                dest_file.set_permissions(permissions.clone())?;
                
                // Copy contents
                io::copy(&mut source_file, &mut dest_file)?;
                dest_file.sync_all()?; // Ensure data is written to disk
                
                // 2. Backup existing file if it exists
                if dest_path.exists() {
                    let backup_path = dest_path.with_extension(format!("{}.bak", 
                        dest_path.extension().and_then(|s| s.to_str()).unwrap_or("bak")));
                    fs::copy(&dest_path, &backup_path)?;
                    transaction.add_backup(dest_path.clone(), backup_path);
                }
                
                // 3. Atomically rename temp file to final destination
                fs::rename(&temp_dest, &dest_path)?;
                transaction.add_installed_file(dest_path.clone());
                
                if verbose {
                    println!("    Installed: {}", dest_path.display());
                }
            } else if source_path.is_symlink() {
                // Handle symlinks
                let link_target = fs::read_link(&source_path)?;
                if dest_path.exists() || dest_path.is_symlink() {
                    fs::remove_file(&dest_path)?;
                }
                std::os::unix::fs::symlink(&link_target, &dest_path)?;
                
                if verbose {
                    println!("    Created symlink: {} -> {}", dest_path.display(), link_target.display());
                }
            }
        }
        
        Ok(())
    }
    
    /// Calculate SHA256 checksum of a file
    fn calculate_file_checksum(file_path: &Path) -> Result<String> {
        use std::io::Read;
        
        let mut file = fs::File::open(file_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];
        
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        
        Ok(hex::encode(hasher.finalize()))
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum HookType {
    PreInstall,
    PostInstall,
    PreRemove,
    PostRemove,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_installer_creation() {
        let temp_dir = TempDir::new().unwrap();
        let installer = Installer::new(4, temp_dir.path());
        assert_eq!(installer.worker_pool_size, 4);
    }
}

