use anyhow::Result;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::fs;
use std::path::Path;
use std::convert::TryInto;

#[allow(dead_code)]
pub struct PackageVerifier {
    trusted_keys: Vec<VerifyingKey>,
}

impl PackageVerifier {
    /// Erstellt einen neuen Verifier mit vertrauenswürdigen Schlüsseln
    #[allow(dead_code)]
    pub fn new(trusted_keys_dir: &Path) -> Result<Self> {
        let mut trusted_keys = Vec::new();
        
        if trusted_keys_dir.exists() {
            for entry in fs::read_dir(trusted_keys_dir)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("pub") {
                    if let Ok(key_bytes) = fs::read(&path) {
                        if key_bytes.len() == 32 {
                            if let Ok(key_bytes_array) = key_bytes.as_slice().try_into() {
                                if let Ok(key) = VerifyingKey::from_bytes(&key_bytes_array) {
                                    trusted_keys.push(key);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(PackageVerifier { trusted_keys })
    }
    
    /// Verifiziert eine Signatur gegen die Metadaten
    #[allow(dead_code)]
    pub fn verify_signature(
        &self,
        metadata: &[u8],
        signature_bytes: &[u8],
        key: &VerifyingKey,
    ) -> Result<()> {
        let signature_bytes_array: [u8; 64] = signature_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length: expected 64 bytes"))?;
        let signature = Signature::from_bytes(&signature_bytes_array);
        
        key.verify(metadata, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?;
        
        Ok(())
    }
    
    /// Verifiziert eine Signatur gegen alle vertrauenswürdigen Schlüssel
    #[allow(dead_code)]
    pub fn verify_with_trusted_keys(
        &self,
        metadata: &[u8],
        signature_bytes: &[u8],
    ) -> Result<()> {
        if self.trusted_keys.is_empty() {
            return Err(anyhow::anyhow!("No trusted keys available"));
        }
        
        for key in &self.trusted_keys {
            if self.verify_signature(metadata, signature_bytes, key).is_ok() {
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("Signature verification failed with all trusted keys"))
    }
    
    /// Fügt einen neuen vertrauenswürdigen Schlüssel hinzu
    #[allow(dead_code)]
    pub fn add_trusted_key(&mut self, key_bytes: &[u8]) -> Result<()> {
        if key_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Invalid key length: expected 32 bytes"));
        }
        let key_bytes_array: [u8; 32] = key_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key format"))?;
        let key = VerifyingKey::from_bytes(&key_bytes_array)?;
        self.trusted_keys.push(key);
        Ok(())
    }
    
    /// Gibt die Anzahl der vertrauenswürdigen Schlüssel zurück
    pub fn trusted_key_count(&self) -> usize {
        self.trusted_keys.len()
    }
    
    /// Gibt eine Referenz auf alle vertrauenswürdigen Schlüssel zurück
    /// Gibt alle vertrauenswürdigen Schlüssel zurück
    #[allow(dead_code)]
    pub fn get_trusted_keys(&self) -> &[VerifyingKey] {
        &self.trusted_keys
    }
    
    /// Fügt einen Schlüssel aus einer Datei hinzu
    #[allow(dead_code)]
    pub fn add_key_from_file(&mut self, key_path: &Path) -> Result<()> {
        let key_bytes = std::fs::read(key_path)?;
        self.add_trusted_key(&key_bytes)
    }
    
    /// Speichert einen Schlüssel in eine Datei
    #[allow(dead_code)]
    pub fn save_key_to_file(&self, key: &VerifyingKey, path: &Path) -> Result<()> {
        std::fs::write(path, key.as_bytes())?;
        Ok(())
    }
    
    /// Verifiziert ein Paket-Signatur
    #[allow(dead_code)]
    pub fn verify_package_signature(
        &self,
        metadata: &[u8],
        signature_bytes: &[u8],
    ) -> Result<()> {
        self.verify_with_trusted_keys(metadata, signature_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};
    use tempfile::TempDir;
    
    #[test]
    fn test_verifier_creation() {
        let temp_dir = TempDir::new().unwrap();
        let verifier = PackageVerifier::new(temp_dir.path()).unwrap();
        assert_eq!(verifier.trusted_key_count(), 0);
    }
    
    #[test]
    fn test_signature_verification() {
        let temp_dir = TempDir::new().unwrap();
        let mut verifier = PackageVerifier::new(temp_dir.path()).unwrap();
        
        // Generiere Test-Schlüsselpaar
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        
        // Füge Schlüssel hinzu
        verifier.add_trusted_key(verifying_key.as_bytes()).unwrap();
        
        // Erstelle Signatur
        let message = b"test metadata";
        let signature = signing_key.sign(message);
        
        // Verifiziere Signatur
        assert!(verifier.verify_with_trusted_keys(message, signature.to_bytes().as_slice()).is_ok());
    }
}

