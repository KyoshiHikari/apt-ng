use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs;
use crate::package::{PackageManifest, ApxPackage};
use sha2::{Sha256, Digest};
use hex;

/// Generates Packages files for repositories
pub struct RepositoryIndexGenerator {
    package_dir: PathBuf,
    suite: String,
    component: String,
    arch: String,
}

impl RepositoryIndexGenerator {
    /// Create a new index generator
    pub fn new(
        package_dir: impl AsRef<Path>,
        suite: impl Into<String>,
        component: impl Into<String>,
        arch: impl Into<String>,
    ) -> Self {
        RepositoryIndexGenerator {
            package_dir: package_dir.as_ref().to_path_buf(),
            suite: suite.into(),
            component: component.into(),
            arch: arch.into(),
        }
    }

    /// Generate Packages file from directory
    pub fn generate_packages_file(&self, output_path: impl AsRef<Path>) -> Result<()> {
        let mut packages_content = String::new();
        
        // Scan directory for .apx and .deb files
        let packages = self.scan_packages()?;
        
        for pkg in packages {
            packages_content.push_str(&self.format_package_entry(&pkg)?);
            packages_content.push_str("\n");
        }
        
        // Write Packages file
        fs::write(output_path, packages_content)?;
        
        Ok(())
    }

    /// Scan directory for package files
    fn scan_packages(&self) -> Result<Vec<PackageManifest>> {
        let mut packages = Vec::new();
        
        if !self.package_dir.exists() {
            return Ok(packages);
        }
        
        for entry in fs::read_dir(&self.package_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "apx" {
                        // Try to load APX package
                        if let Ok(apx) = ApxPackage::open(&path) {
                            packages.push(apx.manifest);
                        }
                    } else if ext == "deb" {
                        // For .deb files, we'd need to extract metadata
                        // This is a simplified version - in production would use dpkg-deb
                        // For now, skip .deb files or implement proper parsing
                    }
                }
            }
        }
        
        Ok(packages)
    }

    /// Format a package entry in Packages file format
    fn format_package_entry(&self, manifest: &PackageManifest) -> Result<String> {
        let mut entry = String::new();
        
        entry.push_str(&format!("Package: {}\n", manifest.name));
        entry.push_str(&format!("Version: {}\n", manifest.version));
        entry.push_str(&format!("Architecture: {}\n", manifest.arch));
        
        if !manifest.depends.is_empty() {
            entry.push_str(&format!("Depends: {}\n", manifest.depends.join(", ")));
        }
        
        if !manifest.provides.is_empty() {
            entry.push_str(&format!("Provides: {}\n", manifest.provides.join(", ")));
        }
        
        if !manifest.conflicts.is_empty() {
            entry.push_str(&format!("Conflicts: {}\n", manifest.conflicts.join(", ")));
        }
        
        entry.push_str(&format!("Size: {}\n", manifest.size));
        entry.push_str(&format!("SHA256: {}\n", manifest.checksum));
        
        if let Some(ref filename) = manifest.filename {
            entry.push_str(&format!("Filename: {}\n", filename));
        }
        
        entry.push_str("\n");
        
        Ok(entry)
    }

    /// Generate Release file with checksums
    pub fn generate_release_file(
        &self,
        packages_path: &Path,
        output_path: impl AsRef<Path>,
    ) -> Result<()> {
        let packages_content = fs::read_to_string(packages_path)?;
        
        // Calculate checksums
        let md5_sum = hex::encode(md5::compute(&packages_content).0);
        
        let mut sha1_hasher = sha1::Sha1::new();
        sha1_hasher.update(&packages_content);
        let sha1_sum = hex::encode(sha1_hasher.finalize());
        
        let mut sha256_hasher = Sha256::new();
        sha256_hasher.update(&packages_content);
        let sha256_sum = hex::encode(sha256_hasher.finalize());
        
        let file_size = packages_content.len();
        let packages_filename = packages_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Packages");
        
        // Format Release file
        let mut release_content = String::new();
        release_content.push_str(&format!("Suite: {}\n", self.suite));
        release_content.push_str(&format!("Component: {}\n", self.component));
        release_content.push_str(&format!("Architecture: {}\n", self.arch));
        release_content.push_str(&format!("Date: {}\n", 
            chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S %Z")));
        release_content.push_str("\n");
        release_content.push_str(&format!(" {} {} {} {}\n",
            md5_sum, file_size, packages_filename, "Packages"));
        release_content.push_str(&format!(" {} {} {} {}\n",
            sha1_sum, file_size, packages_filename, "Packages"));
        release_content.push_str(&format!(" {} {} {} {}\n",
            sha256_sum, file_size, packages_filename, "Packages"));
        
        fs::write(output_path, release_content)?;
        
        Ok(())
    }
}

