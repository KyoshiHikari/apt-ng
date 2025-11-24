use anyhow::Result;
use reqwest::Client;
use std::path::Path;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use futures::stream::{self, StreamExt};

pub struct Downloader {
    pub client: Client,
    max_parallel: usize,
}

impl Downloader {
    /// Erstellt einen neuen Downloader
    pub fn new(max_parallel: usize) -> Result<Self> {
        let client = Client::builder()
            // Verwende Standard-HTTP-Verhandlung (HTTP/1.1 oder HTTP/2)
            .timeout(std::time::Duration::from_secs(30)) // 30 Sekunden Timeout
            .build()?;
        
        Ok(Downloader {
            client,
            max_parallel,
        })
    }
    
    /// Lädt eine Datei von einer URL herunter (mit Resume-Unterstützung und Checksum-Validierung)
    pub async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        self.download_file_with_checksum(url, dest, None).await
    }
    
    /// Lädt eine Datei von einer URL herunter mit optionaler Checksum-Validierung
    pub async fn download_file_with_checksum(&self, url: &str, dest: &Path, expected_checksum: Option<&str>) -> Result<()> {
        // Check if file already exists (for resume)
        let existing_size = if dest.exists() {
            tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        
        // Check if server supports range requests
        let head_response = self.client.head(url).send().await?;
        let supports_ranges = head_response.headers().contains_key("accept-ranges");
        let content_length = head_response.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        
        // Resume download if file exists and server supports ranges
        if existing_size > 0 && supports_ranges {
            if let Some(total_size) = content_length {
                if existing_size < total_size {
                    // Resume download
                    self.resume_download(url, dest, existing_size, total_size).await?;
                    // Validate checksum after resume
                    if let Some(expected) = expected_checksum {
                        self.validate_file_checksum(dest, expected).await?;
                    }
                    return Ok(());
                } else if existing_size == total_size {
                    // File already complete - validate checksum
                    if let Some(expected) = expected_checksum {
                        self.validate_file_checksum(dest, expected).await?;
                    }
                    return Ok(());
                }
            }
        }
        
        // Use chunked download if file is large (>10MB) and server supports ranges
        if let Some(size) = content_length {
            if size > 10 * 1024 * 1024 && supports_ranges {
                self.download_file_chunked(url, dest, size).await?;
                // Validate checksum after chunked download
                if let Some(expected) = expected_checksum {
                    self.validate_file_checksum(dest, expected).await?;
                }
                return Ok(());
            }
        }
        
        // Fallback to regular download
        let mut response = self.client.get(url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
        }
        
        let mut file = tokio::fs::File::create(dest).await?;
        
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
        }
        
        // Validate checksum if provided
        if let Some(expected) = expected_checksum {
            self.validate_file_checksum(dest, expected).await?;
        }
        
        Ok(())
    }
    
    /// Setzt einen unterbrochenen Download fort
    async fn resume_download(&self, url: &str, dest: &Path, existing_size: u64, total_size: u64) -> Result<()> {
        let range_header = format!("bytes={}-{}", existing_size, total_size - 1);
        let mut response = self.client
            .get(url)
            .header("Range", range_header)
            .send()
            .await?;
        
        if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(anyhow::anyhow!("HTTP error for resume: {}", response.status()));
        }
        
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(dest)
            .await?;
        
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
        }
        
        Ok(())
    }
    
    /// Lädt eine Datei in Chunks mit Range-Requests herunter
    async fn download_file_chunked(&self, url: &str, dest: &Path, total_size: u64) -> Result<()> {
        const CHUNK_SIZE: u64 = 2 * 1024 * 1024; // 2MB chunks
        let num_chunks = (total_size + CHUNK_SIZE - 1) / CHUNK_SIZE;
        
        // Create file and set size
        let file = tokio::fs::File::create(dest).await?;
        file.set_len(total_size).await?;
        
        // Download chunks in parallel
        let chunks: Vec<_> = (0..num_chunks).collect();
        let results: Vec<_> = stream::iter(chunks.iter())
            .map(|&chunk_idx| {
                let client = &self.client;
                let url = url.to_string();
                let dest_path = dest.to_path_buf();
                
                async move {
                    let start = chunk_idx * CHUNK_SIZE;
                    let end = std::cmp::min(start + CHUNK_SIZE - 1, total_size - 1);
                    
                    // Download chunk with range request
                    let range_header = format!("bytes={}-{}", start, end);
                    let mut response = client
                        .get(&url)
                        .header("Range", range_header)
                        .send()
                        .await?;
                    
                    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                        return Err(anyhow::anyhow!("HTTP error for chunk {}: {}", chunk_idx, response.status()));
                    }
                    
                    // Write chunk to file at correct position
                    let mut file = tokio::fs::OpenOptions::new()
                        .write(true)
                        .open(&dest_path)
                        .await?;
                    
                    file.seek(tokio::io::SeekFrom::Start(start)).await?;
                    
                    while let Some(chunk) = response.chunk().await? {
                        file.write_all(&chunk).await?;
                    }
                    
                    Ok::<(), anyhow::Error>(())
                }
            })
            .buffer_unordered(self.max_parallel)
            .collect()
            .await;
        
        // Check for errors
        for result in results {
            result?;
        }
        
        Ok(())
    }
    
    /// Validiert die SHA256-Checksumme einer Datei
    async fn validate_file_checksum(&self, file_path: &Path, expected: &str) -> Result<()> {
        use sha2::{Sha256, Digest};
        use hex;
        use tokio::io::AsyncReadExt;
        
        let mut file = tokio::fs::File::open(file_path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];
        
        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        
        let calculated = hex::encode(hasher.finalize());
        if calculated != expected {
            return Err(anyhow::anyhow!(
                "Checksum mismatch: expected {}, got {}",
                expected,
                calculated
            ));
        }
        
        Ok(())
    }
    
    /// Lädt mehrere Dateien parallel herunter
    #[allow(dead_code)]
    pub async fn download_files(&self, urls: &[(&str, &Path)]) -> Result<Vec<Result<()>>> {
        let results: Vec<_> = stream::iter(urls.iter())
            .map(|(url, dest)| {
                let client = &self.client;
                let url = *url;
                let dest = *dest;
                
                async move {
                    let mut response = client.get(url).send().await?;
                    let mut file = tokio::fs::File::create(dest).await?;
                    
                    while let Some(chunk) = response.chunk().await? {
                        file.write_all(&chunk).await?;
                    }
                    
                    Ok::<(), anyhow::Error>(())
                }
            })
            .buffer_unordered(self.max_parallel)
            .collect()
            .await;
        
        Ok(results)
    }
    
    /// Testet die Geschwindigkeit eines Mirrors (RTT + Throughput)
    pub async fn probe_mirror(&self, url: &str) -> Result<MirrorStats> {
        use std::time::Instant;
        
        // Measure RTT
        let start = Instant::now();
        let head_response = self.client.head(url).send().await?;
        let rtt_ms = start.elapsed().as_millis() as u64;
        
        // Measure throughput by downloading a small chunk
        let content_length = head_response.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        
        let throughput = if let Some(total_size) = content_length {
            // Download first 1MB or entire file if smaller
            let test_size = std::cmp::min(1024 * 1024, total_size);
            
            let download_start = Instant::now();
            let range_header = format!("bytes=0-{}", test_size - 1);
            let mut response = self.client
                .get(url)
                .header("Range", range_header)
                .send()
                .await?;
            
            if response.status().is_success() || response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                let mut bytes_downloaded = 0u64;
                while let Some(chunk) = response.chunk().await? {
                    bytes_downloaded += chunk.len() as u64;
                }
                
                let elapsed = download_start.elapsed();
                if elapsed.as_secs() > 0 {
                    bytes_downloaded / elapsed.as_secs()
                } else {
                    bytes_downloaded * 1000 / elapsed.as_millis() as u64
                }
            } else {
                0
            }
        } else {
            // If no content-length, try downloading first chunk
            let download_start = Instant::now();
            let mut response = self.client.get(url).send().await?;
            
            if response.status().is_success() {
                let mut bytes_downloaded = 0u64;
                let mut chunks = 0;
                while let Some(chunk) = response.chunk().await? {
                    bytes_downloaded += chunk.len() as u64;
                    chunks += 1;
                    // Only measure first few chunks for speed
                    if chunks >= 10 {
                        break;
                    }
                }
                
                let elapsed = download_start.elapsed();
                if elapsed.as_secs() > 0 {
                    bytes_downloaded / elapsed.as_secs()
                } else if elapsed.as_millis() > 0 {
                    bytes_downloaded * 1000 / elapsed.as_millis() as u64
                } else {
                    0
                }
            } else {
                0
            }
        };
        
        Ok(MirrorStats {
            url: url.to_string(),
            rtt_ms,
            throughput,
        })
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct MirrorStats {
    pub url: String,
    pub rtt_ms: u64,
    pub throughput: u64, // bytes per second
}

impl MirrorStats {
    /// Berechnet einen Score für die Mirror-Auswahl (niedriger ist besser)
    pub fn score(&self) -> f64 {
        // Kombiniere RTT und Throughput zu einem Score
        // Niedrige RTT und hoher Throughput = niedriger Score
        if self.throughput > 0 {
            // Score = RTT * (1 / normalized_throughput)
            // Normalisiere Throughput auf MB/s
            let throughput_mbps = self.throughput as f64 / (1024.0 * 1024.0);
            self.rtt_ms as f64 / throughput_mbps.max(0.1) // Vermeide Division durch 0
        } else {
            // Wenn kein Throughput gemessen wurde, verwende nur RTT
            self.rtt_ms as f64 * 1000.0 // Strafe für fehlende Throughput-Daten
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_downloader_creation() {
        let downloader = Downloader::new(4).unwrap();
        assert_eq!(downloader.max_parallel, 4);
    }
}

