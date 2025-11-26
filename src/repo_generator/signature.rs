use anyhow::Result;
use std::path::Path;
use std::fs;
use ed25519_dalek::{SigningKey, Signer};
use std::convert::TryInto;

/// Signs repository Release files with Ed25519
pub struct RepositorySigner {
    signing_key: SigningKey,
}

impl RepositorySigner {
    /// Create a new repository signer from a key file
    pub fn from_key_file(key_path: &Path) -> Result<Self> {
        let key_bytes = fs::read(key_path)?;
        if key_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Invalid key length: expected 32 bytes, got {}", key_bytes.len()));
        }
        
        let key_array: [u8; 32] = key_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert key bytes"))?;
        
        let signing_key = SigningKey::from_bytes(&key_array);
        
        Ok(RepositorySigner { signing_key })
    }

    /// Sign a Release file and create InRelease (signed inline) and Release.gpg files
    pub fn sign_release(&self, release_path: &Path, output_dir: &Path) -> Result<()> {
        let release_content = fs::read_to_string(release_path)?;
        
        // Sign the release content
        let signature = self.signing_key.sign(release_content.as_bytes());
        let signature_bytes = signature.to_bytes();
        
        // Create InRelease (inline signed)
        let mut inrelease_content = release_content.clone();
        inrelease_content.push_str("\n");
        inrelease_content.push_str("-----BEGIN PGP SIGNATURE-----\n");
        use base64::Engine;
        inrelease_content.push_str(&base64::engine::general_purpose::STANDARD.encode(&signature_bytes));
        inrelease_content.push_str("\n-----END PGP SIGNATURE-----\n");
        
        let inrelease_path = output_dir.join("InRelease");
        fs::write(&inrelease_path, inrelease_content)?;
        
        // Create Release.gpg (detached signature)
        let release_gpg_path = output_dir.join("Release.gpg");
        fs::write(&release_gpg_path, signature_bytes)?;
        
        Ok(())
    }
}

