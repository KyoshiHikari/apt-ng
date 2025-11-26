use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use sha2::{Sha256, Digest};
use hex;
use zstd::stream::Encoder;
use tar::{Builder, Header};
use crate::package::{PackageManifest, FileEntry};
use serde_json;

const APX_MAGIC: &[u8] = b"APX\x01";

/// Builder for creating .apx packages
pub struct ApxBuilder {
    source_dir: PathBuf,
    manifest: PackageManifest,
}

impl ApxBuilder {
    /// Create a new builder from a source directory
    pub fn new(source_dir: impl AsRef<Path>) -> Self {
        ApxBuilder {
            source_dir: source_dir.as_ref().to_path_buf(),
            manifest: PackageManifest {
                name: String::new(),
                version: String::new(),
                arch: String::new(),
                provides: vec![],
                depends: vec![],
                conflicts: vec![],
                replaces: vec![],
                files: vec![],
                size: 0,
                checksum: String::new(),
                timestamp: 0,
                filename: None,
                repo_id: None,
            },
        }
    }

    /// Set package manifest information
    pub fn set_manifest(&mut self, manifest: PackageManifest) {
        self.manifest = manifest;
    }

    /// Build the .apx package from the source directory
    pub fn build(&self, output_path: impl AsRef<Path>) -> Result<()> {
        let output_path = output_path.as_ref();
        
        // Create output file
        let mut output_file = BufWriter::new(File::create(output_path)?);
        
        // Write APX magic header
        output_file.write_all(APX_MAGIC)?;
        
        // Scan source directory and build file list
        let files = self.scan_directory(&self.source_dir)?;
        
        // Update manifest with file information
        let mut manifest = self.manifest.clone();
        manifest.files = files.clone();
        manifest.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        
        // Calculate total size
        manifest.size = files.iter().map(|f| f.size).sum();
        
        // Serialize manifest to JSON
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        
        // Compress manifest with zstd
        let mut encoder = Encoder::new(Vec::new(), 3)?; // Compression level 3
        encoder.write_all(manifest_json.as_bytes())?;
        let compressed_manifest = encoder.finish()?;
        
        // Write compressed manifest length (4 bytes, little-endian)
        let manifest_len = compressed_manifest.len() as u32;
        output_file.write_all(&manifest_len.to_le_bytes())?;
        
        // Write compressed manifest
        output_file.write_all(&compressed_manifest)?;
        
        // Create content.tar.zst
        let content_tar_zst = self.create_content_tar_zst(&files)?;
        
        // Write content.tar.zst
        output_file.write_all(&content_tar_zst)?;
        
        output_file.flush()?;
        
        // Calculate checksum (for future use)
        let mut hasher = Sha256::new();
        hasher.update(&compressed_manifest);
        hasher.update(&content_tar_zst);
        let _checksum = hex::encode(hasher.finalize());
        
        // Note: We can't update the checksum in the file now, but we can store it
        // In a full implementation, we'd either:
        // 1. Write checksum at the end and update it
        // 2. Calculate checksum before writing and include it in manifest
        // For now, the checksum is calculated but not stored in the package file
        
        Ok(())
    }

    /// Scan directory and build file list with checksums
    fn scan_directory(&self, dir: &Path) -> Result<Vec<FileEntry>> {
        let mut files = Vec::new();
        let base_path = &self.source_dir;
        
        self.scan_directory_recursive(dir, base_path, &mut files)?;
        
        Ok(files)
    }

    fn scan_directory_recursive(
        &self,
        dir: &Path,
        base_path: &Path,
        files: &mut Vec<FileEntry>,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::metadata(&path)?;
            
            if metadata.is_dir() {
                self.scan_directory_recursive(&path, base_path, files)?;
            } else if metadata.is_file() {
                // Calculate relative path
                let relative_path = path.strip_prefix(base_path)?
                    .to_string_lossy()
                    .to_string();
                
                // Read file and calculate checksum
                let file_data = fs::read(&path)?;
                let mut hasher = Sha256::new();
                hasher.update(&file_data);
                let checksum = hex::encode(hasher.finalize());
                
                // Get file mode
                let mode = metadata.permissions().mode();
                
                files.push(FileEntry {
                    path: relative_path,
                    checksum,
                    size: file_data.len() as u64,
                    mode,
                });
            }
        }
        
        Ok(())
    }

    /// Create content.tar.zst from file list
    fn create_content_tar_zst(&self, files: &[FileEntry]) -> Result<Vec<u8>> {
        let mut tar_data = Vec::new();
        {
            let mut tar = Builder::new(&mut tar_data);
            
            for file_entry in files {
                let file_path = self.source_dir.join(&file_entry.path);
                let mut file = File::open(&file_path)?;
                
                let mut header = Header::new_gnu();
                header.set_path(&file_entry.path)?;
                header.set_size(file_entry.size);
                header.set_mode(file_entry.mode);
                header.set_cksum();
                
                tar.append(&header, &mut file)?;
            }
            
            tar.finish()?;
        }
        
        // Compress with zstd
        let mut encoder = Encoder::new(Vec::new(), 3)?;
        encoder.write_all(&tar_data)?;
        let compressed = encoder.finish()?;
        
        Ok(compressed)
    }
}

