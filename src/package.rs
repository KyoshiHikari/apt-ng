use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use zstd::stream::{Decoder, Encoder};
use tar::Archive;

#[allow(dead_code)]
const APX_MAGIC: &[u8] = b"APX\x01";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub provides: Vec<String>,
    pub depends: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub files: Vec<FileEntry>,
    pub size: u64,
    pub checksum: String,
    pub timestamp: i64,
    #[serde(default)]
    pub filename: Option<String>, // Pfad zum .deb-Paket im Repository (z.B. "pool/main/m/micro/micro_2.0.11-1_amd64.deb")
    #[serde(default)]
    pub repo_id: Option<i64>, // ID des Repositories
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub checksum: String,
    pub size: u64,
    pub mode: u32,
}

pub struct ApxPackage {
    pub manifest: PackageManifest,
    pub content_path: PathBuf,
}

impl ApxPackage {
    /// Öffnet ein .apx-Paket und lädt das Manifest
    pub fn open(apx_path: &Path) -> Result<Self> {
        let mut file = BufReader::new(File::open(apx_path)?);
        
        // Lese Header (Magic + Version)
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        if &header[..3] != &APX_MAGIC[..3] {
            return Err(anyhow::anyhow!("Invalid APX magic"));
        }
        
        // Lese metadata Länge (4 bytes, little-endian)
        let mut metadata_len_bytes = [0u8; 4];
        file.read_exact(&mut metadata_len_bytes)?;
        let metadata_len = u32::from_le_bytes(metadata_len_bytes) as usize;
        
        // Lese metadata.json.zst
        let mut metadata_compressed = vec![0u8; metadata_len];
        file.read_exact(&mut metadata_compressed)?;
        
        // Dekomprimiere metadata.json
        let metadata_json = Self::try_decode_zstd(&metadata_compressed)?;
        
        // Parse Manifest
        let manifest: PackageManifest = serde_json::from_str(&metadata_json)?;
        
        Ok(ApxPackage {
            manifest,
            content_path: apx_path.to_path_buf(),
        })
    }
    
    /// Versucht zstd-Daten zu dekodieren
    #[allow(dead_code)]
    fn try_decode_zstd(data: &[u8]) -> Result<String> {
        let decoder = Decoder::new(data)?;
        let mut reader = BufReader::new(decoder);
        let mut result = String::new();
        reader.read_to_string(&mut result)?;
        Ok(result)
    }
    
    /// Extrahiert den Inhalt des Pakets in ein Zielverzeichnis
    pub fn extract_to(&self, dest_dir: &Path) -> Result<()> {
        use std::fs;
        
        let mut file = BufReader::new(File::open(&self.content_path)?);
        
        // Überspringe Header
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        
        // Lese metadata Länge
        let mut metadata_len_bytes = [0u8; 4];
        file.read_exact(&mut metadata_len_bytes)?;
        let metadata_len = u32::from_le_bytes(metadata_len_bytes) as usize;
        
        // Überspringe metadata (wir haben es bereits beim Öffnen gelesen)
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Current(metadata_len as i64))?;
        
        // Lese content Länge
        let mut content_len_bytes = [0u8; 4];
        file.read_exact(&mut content_len_bytes)?;
        let content_len = u32::from_le_bytes(content_len_bytes) as usize;
        
        // Lese content.tar.zst
        let mut content_data = vec![0u8; content_len];
        file.read_exact(&mut content_data)?;
        
        // Dekomprimiere content.tar.zst
        let decoder = Decoder::new(content_data.as_slice())?;
        let mut tar_archive = Archive::new(decoder);
        
        // Stelle sicher, dass das Zielverzeichnis existiert
        fs::create_dir_all(dest_dir)?;
        
        // Extrahiere tar-Archiv
        tar_archive.unpack(dest_dir)?;
        
        Ok(())
    }
    
    /// Verifiziert die Checksummen aller Dateien
    pub fn verify_checksums(&self, extracted_dir: &Path) -> Result<()> {
        use sha2::{Sha256, Digest};
        use hex;
        use std::fs;
        
        for file_entry in &self.manifest.files {
            let file_path = extracted_dir.join(&file_entry.path);
            
            if !file_path.exists() {
                return Err(anyhow::anyhow!("File not found: {}", file_entry.path));
            }
            
            // Berechne SHA256-Checksumme
            let mut file = fs::File::open(&file_path)?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; 8192];
            
            loop {
                let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
                if bytes_read == 0 {
                    break;
                }
                hasher.update(&buffer[..bytes_read]);
            }
            
            let calculated = hex::encode(hasher.finalize());
            if calculated != file_entry.checksum {
                return Err(anyhow::anyhow!(
                    "Checksum mismatch for {}: expected {}, got {}",
                    file_entry.path,
                    file_entry.checksum,
                    calculated
                ));
            }
        }
        
        Ok(())
    }
    
    /// Verifiziert die Signatur eines .apx-Pakets
    pub fn verify_signature(&self, apx_path: &Path, verifier: &crate::verifier::PackageVerifier) -> Result<()> {
        use std::io::Read;
        use ed25519_dalek::{Signature, Verifier};
        
        let mut file = BufReader::new(File::open(apx_path)?);
        
        // Überspringe Header
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        
        // Lese metadata Länge
        let mut metadata_len_bytes = [0u8; 4];
        file.read_exact(&mut metadata_len_bytes)?;
        let metadata_len = u32::from_le_bytes(metadata_len_bytes) as usize;
        
        // Lese metadata
        let mut metadata_data = vec![0u8; metadata_len];
        file.read_exact(&mut metadata_data)?;
        
        // Lese content Länge
        let mut content_len_bytes = [0u8; 4];
        file.read_exact(&mut content_len_bytes)?;
        let content_len = u32::from_le_bytes(content_len_bytes) as usize;
        
        // Lese content
        let mut content_data = vec![0u8; content_len];
        file.read_exact(&mut content_data)?;
        
        // Lese Signatur (64 bytes)
        let mut signature_bytes = [0u8; 64];
        if file.read_exact(&mut signature_bytes).is_err() {
            return Err(anyhow::anyhow!("Package is not signed"));
        }
        
        // Kombiniere metadata + content für Verifikation
        let mut data_to_verify = Vec::new();
        data_to_verify.extend_from_slice(&metadata_data);
        data_to_verify.extend_from_slice(&content_data);
        
        // Verifiziere mit allen vertrauenswürdigen Schlüsseln
        if verifier.trusted_key_count() == 0 {
            return Err(anyhow::anyhow!("No trusted keys available for verification"));
        }
        
        // Versuche Verifikation mit jedem Schlüssel
        let signature = Signature::from_bytes(&signature_bytes);
        for key in verifier.get_trusted_keys() {
            if key.verify(&data_to_verify, &signature).is_ok() {
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("Signature verification failed with all trusted keys"))
    }
}

/// Erstellt ein .apx-Paket aus einem Verzeichnis
#[allow(dead_code)]
pub fn create_apx_package(
    source_dir: &Path,
    manifest: PackageManifest,
    output_path: &Path,
    sign_key: Option<&[u8]>,
) -> Result<()> {
    use std::io::Write;
    use tar::Builder;
    
    let mut output = BufWriter::new(File::create(output_path)?);
    
    // Schreibe Header
    output.write_all(APX_MAGIC)?;
    
    // Komprimiere und schreibe metadata.json
    let metadata_json = serde_json::to_string(&manifest)?;
    let mut encoder = Encoder::new(Vec::new(), 3)?;
    encoder.write_all(metadata_json.as_bytes())?;
    let metadata_compressed = encoder.finish()?;
    
    // Schreibe Länge von metadata.json.zst (4 bytes, little-endian)
    let metadata_len = metadata_compressed.len() as u32;
    output.write_all(&metadata_len.to_le_bytes())?;
    output.write_all(&metadata_compressed)?;
    
    // Komprimiere und schreibe content.tar.zst
    let mut content_tar = Vec::new();
    {
        let mut builder = Builder::new(&mut content_tar);
        // Füge alle Dateien aus source_dir hinzu
        for entry in std::fs::read_dir(source_dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(source_dir)
                .map_err(|e| anyhow::anyhow!("Failed to get relative path: {}", e))?;
            
            if path.is_file() {
                builder.append_file(relative_path, &mut File::open(&path)?)?;
            } else if path.is_dir() {
                builder.append_dir_all(relative_path, &path)?;
            }
        }
        builder.finish()?;
    }
    
    // Komprimiere content.tar
    let mut content_encoder = Encoder::new(Vec::new(), 3)?;
    content_encoder.write_all(&content_tar)?;
    let content_compressed = content_encoder.finish()?;
    
    // Schreibe Länge von content.tar.zst (4 bytes, little-endian)
    let content_len = content_compressed.len() as u32;
    output.write_all(&content_len.to_le_bytes())?;
    output.write_all(&content_compressed)?;
    
    // Füge Signatur hinzu, falls Schlüssel vorhanden
    if let Some(key_bytes) = sign_key {
        use ed25519_dalek::{SigningKey, Signer};
        use std::convert::TryInto;
        
        // Erstelle SigningKey aus bytes
        let signing_key = SigningKey::from_bytes(key_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signing key length"))?);
        
        // Signiere metadata + content
        let mut data_to_sign = Vec::new();
        data_to_sign.extend_from_slice(&metadata_compressed);
        data_to_sign.extend_from_slice(&content_compressed);
        
        let signature = signing_key.sign(&data_to_sign);
        
        // Schreibe Signatur (64 bytes)
        output.write_all(signature.to_bytes().as_slice())?;
    }
    
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_manifest_serialization() {
        let manifest = PackageManifest {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            arch: "amd64".to_string(),
            provides: vec![],
            depends: vec!["libc".to_string()],
            conflicts: vec![],
            replaces: vec![],
            files: vec![],
            size: 1024,
            checksum: "abc123".to_string(),
            timestamp: 1234567890,
        };
        
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: PackageManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test-package");
    }
}


