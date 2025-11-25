use serde::{Deserialize, Serialize};

/// Metadata for a delta package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaMetadata {
    pub from_version: String,
    pub to_version: String,
    pub package_name: String,
    pub delta_size: u64,
    pub full_size: u64,
    pub algorithm: String, // e.g., "bsdiff", "xdelta3"
    pub checksum: String,
}

impl DeltaMetadata {
    /// Calculate size savings percentage
    pub fn savings_percentage(&self) -> f64 {
        if self.full_size == 0 {
            return 0.0;
        }
        ((self.full_size - self.delta_size) as f64 / self.full_size as f64) * 100.0
    }
    
    /// Check if delta is worth using (e.g., saves at least 10%)
    pub fn is_worthwhile(&self) -> bool {
        self.savings_percentage() >= 10.0
    }
}

