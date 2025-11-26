use anyhow::{Result, Context};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::fs;
use std::env;
use std::process::Command;
use sha2::{Sha256, Digest};
use hex;

const GITHUB_API_URL: &str = "https://api.github.com/repos/KyoshiHikari/apt-ng/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<ReleaseAsset>,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

pub struct SelfUpdater {
    client: reqwest::Client,
}

impl SelfUpdater {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("apt-ng-self-updater")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        
        Ok(SelfUpdater { client })
    }

    /// Get current version from Cargo.toml
    pub fn get_current_version() -> String {
        CURRENT_VERSION.to_string()
    }

    /// Calculate SHA256 checksum of the current binary
    pub fn get_current_binary_checksum() -> Result<String> {
        let exe_path = SelfUpdater::get_current_binary_path()?;
        let data = fs::read(&exe_path)
            .context("Failed to read current binary")?;
        
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let checksum = hex::encode(hasher.finalize());
        
        Ok(checksum)
    }

    /// Get SHA256 checksum from GitHub release (from release notes or checksums file)
    pub async fn get_latest_binary_checksum(&self, asset: &ReleaseAsset) -> Result<Option<String>> {
        // First, try to find a checksums file in the release assets
        let release = self.check_for_latest_version().await?;
        
        // Look for checksums.txt or SHA256SUMS file
        for checksum_asset in &release.assets {
            if checksum_asset.name.contains("SHA256") || checksum_asset.name.contains("checksums") {
                // Download checksums file
                let response = self.client
                    .get(&checksum_asset.browser_download_url)
                    .send()
                    .await?;
                
                if response.status().is_success() {
                    let checksums_text = response.text().await?;
                    
                    // Parse checksums file (format: SHA256  filename)
                    for line in checksums_text.lines() {
                        let parts: Vec<&str> = line.trim().split_whitespace().collect();
                        if parts.len() >= 2 {
                            let checksum = parts[0];
                            let filename = parts[1];
                            
                            // Check if this checksum matches our asset
                            if filename == asset.name || filename.contains(&asset.name) {
                                return Ok(Some(checksum.to_string()));
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback: Try to extract checksum from release body/notes
        if let Some(body) = &release.body {
            // Look for SHA256 checksum in release notes (format: SHA256: abc123...)
            for line in body.lines() {
                if line.contains("SHA256") || line.contains("sha256") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for part in parts {
                        // Check if this looks like a SHA256 hash (64 hex characters)
                        if part.len() == 64 && part.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Ok(Some(part.to_string()));
                        }
                    }
                }
            }
        }
        
        Ok(None)
    }

    /// Check if update is available by comparing SHA256 checksums
    pub async fn check_update_available(&self) -> Result<bool> {
        let current_checksum = SelfUpdater::get_current_binary_checksum()?;
        let arch = SelfUpdater::get_architecture()?;
        
        let release = self.check_for_latest_version().await?;
        let asset = self.find_asset_for_architecture(&release, &arch)
            .ok_or_else(|| anyhow::anyhow!("No binary found for architecture: {}", arch))?;
        
        // Try to get checksum from GitHub
        if let Some(latest_checksum) = self.get_latest_binary_checksum(asset).await? {
            return Ok(current_checksum != latest_checksum);
        }
        
        // If checksum not available, fallback to version comparison
        let latest_version = release.tag_name.trim_start_matches('v');
        let comparison = SelfUpdater::compare_versions(&SelfUpdater::get_current_version(), latest_version);
        Ok(comparison == std::cmp::Ordering::Less)
    }

    /// Quick check for updates (non-blocking, returns immediately if GitHub API is slow)
    pub async fn quick_check_update_available(&self) -> Option<bool> {
        use tokio::time::{timeout, Duration};
        
        // Set a short timeout (2 seconds) to avoid blocking
        match timeout(Duration::from_secs(2), self.check_update_available()).await {
            Ok(Ok(available)) => Some(available),
            Ok(Err(_)) => None, // Error checking, don't show update message
            Err(_) => None, // Timeout, don't block
        }
    }

    /// Detect current system architecture
    pub fn get_architecture() -> Result<String> {
        // Try to detect architecture using uname
        let output = Command::new("uname")
            .arg("-m")
            .output()
            .context("Failed to execute uname")?;
        
        let arch = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in uname output")?
            .trim()
            .to_string();

        // Map common architectures to Rust target triplets
        let target_triplet = match arch.as_str() {
            "x86_64" => "x86_64-unknown-linux-gnu",
            "aarch64" | "arm64" => "aarch64-unknown-linux-gnu",
            "armv7l" | "armv7" => "armv7-unknown-linux-gnueabihf",
            "armv6l" => "arm-unknown-linux-gnueabihf",
            _ => {
                // Fallback: try to construct from arch
                return Ok(format!("{}-unknown-linux-gnu", arch));
            }
        };

        Ok(target_triplet.to_string())
    }

    /// Check for latest version on GitHub
    pub async fn check_for_latest_version(&self) -> Result<GitHubRelease> {
        let response = self.client
            .get(GITHUB_API_URL)
            .send()
            .await
            .context("Failed to fetch release information from GitHub")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "GitHub API returned error: {}",
                response.status()
            ));
        }

        let release: GitHubRelease = response
            .json()
            .await
            .context("Failed to parse GitHub API response")?;

        Ok(release)
    }

    /// Get current binary path
    pub fn get_current_binary_path() -> Result<PathBuf> {
        let exe = env::current_exe()
            .context("Failed to get current executable path")?;
        Ok(exe)
    }

    /// Compare two semantic versions
    pub fn compare_versions(current: &str, latest: &str) -> std::cmp::Ordering {
        // Remove 'v' prefix if present
        let current = current.trim_start_matches('v');
        let latest = latest.trim_start_matches('v');

        // Parse versions (simple implementation, assumes format: MAJOR.MINOR.PATCH)
        let parse_version = |v: &str| -> (u64, u64, u64) {
            let parts: Vec<&str> = v.split('.').collect();
            let major = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
            let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            (major, minor, patch)
        };

        let (c_major, c_minor, c_patch) = parse_version(current);
        let (l_major, l_minor, l_patch) = parse_version(latest);

        // Compare major version
        match c_major.cmp(&l_major) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }

        // Compare minor version
        match c_minor.cmp(&l_minor) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }

        // Compare patch version
        c_patch.cmp(&l_patch)
    }

    /// Find asset for current architecture
    pub fn find_asset_for_architecture<'a>(&self, release: &'a GitHubRelease, arch: &str) -> Option<&'a ReleaseAsset> {
        // Try exact match first
        for asset in &release.assets {
            if asset.name.contains(arch) {
                return Some(asset);
            }
        }

        // Try partial matches
        let arch_parts: Vec<&str> = arch.split('-').collect();
        if let Some(first_part) = arch_parts.first() {
            for asset in &release.assets {
                if asset.name.contains(first_part) {
                    return Some(asset);
                }
            }
        }

        None
    }

    /// Download binary from GitHub release
    pub async fn download_binary(&self, asset: &ReleaseAsset, dest: &Path, verbose: bool) -> Result<()> {
        if verbose {
            crate::output::Output::info(&format!("Downloading {}...", asset.name));
        }

        let mut response = self.client
            .get(&asset.browser_download_url)
            .send()
            .await
            .context("Failed to download binary")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let mut file = tokio::fs::File::create(dest).await
            .context("Failed to create temporary file")?;

        use tokio::io::AsyncWriteExt;

        // Use chunk() method to stream the download (same pattern as downloader.rs)
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
        }

        Ok(())
    }

    /// Extract binary from archive (if needed)
    pub fn extract_binary(&self, archive_path: &Path, dest: &Path) -> Result<()> {
        // Check if it's a tar.gz archive
        if archive_path.extension().and_then(|s| s.to_str()) == Some("gz") {
            // Extract tar.gz
            let file = fs::File::open(archive_path)?;
            let gz_decoder = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(gz_decoder);

            // Find apt-ng binary in archive
            for entry in archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?;
                
                if path.file_name().and_then(|n| n.to_str()) == Some("apt-ng") {
                    let mut outfile = fs::File::create(dest)?;
                    std::io::copy(&mut entry, &mut outfile)?;
                    return Ok(());
                }
            }

            return Err(anyhow::anyhow!("apt-ng binary not found in archive"));
        }

        // If not an archive, just copy the file
        fs::copy(archive_path, dest)?;
        Ok(())
    }


    /// Install binary atomically
    pub fn install_binary(&self, new_binary: &Path, verbose: bool) -> Result<()> {
        let current_binary = SelfUpdater::get_current_binary_path()?;

        if verbose {
            crate::output::Output::info(&format!(
                "Installing update to: {}",
                current_binary.display()
            ));
        }

        // Create temporary file next to the target
        let temp_path = current_binary.parent()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine binary directory"))?
            .join(format!(".apt-ng-update-{}", std::process::id()));

        // Copy new binary to temp location
        fs::copy(new_binary, &temp_path)
            .context("Failed to copy new binary to temp location")?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&temp_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&temp_path, perms)?;
        }

        // Atomically replace the binary
        fs::rename(&temp_path, &current_binary)
            .context("Failed to replace binary. You may need to run with sudo.")?;

        if verbose {
            crate::output::Output::success("Update installed successfully!");
        }

        Ok(())
    }
}

