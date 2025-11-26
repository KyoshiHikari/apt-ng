use anyhow::Result;
use std::path::Path;
use std::fs;
use std::convert::TryInto;
use ed25519_dalek::{SigningKey, Signer, Signature};
use rand::rngs::OsRng;

/// Signer for .apx packages using Ed25519
pub struct ApxSigner {
    signing_key: SigningKey,
}

impl ApxSigner {
    /// Create a new signer from a signing key file
    pub fn from_key_file(key_path: &Path) -> Result<Self> {
        let key_bytes = fs::read(key_path)?;
        if key_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Invalid key length: expected 32 bytes, got {}", key_bytes.len()));
        }
        
        let key_array: [u8; 32] = key_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert key bytes"))?;
        
        let signing_key = SigningKey::from_bytes(&key_array);
        
        Ok(ApxSigner { signing_key })
    }

    /// Generate a new signing key pair
    pub fn generate_key() -> (SigningKey, ed25519_dalek::VerifyingKey) {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    /// Sign package data
    pub fn sign(&self, data: &[u8]) -> Signature {
        self.signing_key.sign(data)
    }

    /// Sign a package file and write signature to a separate file
    pub fn sign_package(&self, package_path: &Path, signature_path: &Path) -> Result<()> {
        // Read package file
        let package_data = fs::read(package_path)?;
        
        // Sign the package
        let signature = self.sign(&package_data);
        
        // Write signature to file
        fs::write(signature_path, signature.to_bytes())?;
        
        Ok(())
    }

    /// Verify a package signature
    pub fn verify_package(package_path: &Path, signature_path: &Path, verifying_key: &ed25519_dalek::VerifyingKey) -> Result<bool> {
        use ed25519_dalek::Verifier;
        
        // Read package and signature
        let package_data = fs::read(package_path)?;
        let signature_bytes = fs::read(signature_path)?;
        
        if signature_bytes.len() != 64 {
            return Err(anyhow::anyhow!("Invalid signature length: expected 64 bytes"));
        }
        
        let signature_array: [u8; 64] = signature_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert signature bytes"))?;
        
        let signature = Signature::from_bytes(&signature_array);
        
        // Verify signature
        match verifying_key.verify(&package_data, &signature) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

