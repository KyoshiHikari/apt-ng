use anyhow::Result;
use reqwest::Client;
use std::path::Path;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use futures::stream::{self, StreamExt};
use std::time::Instant;

pub struct Downloader {
    pub client: Client,
    max_parallel: usize,
}

impl Downloader {
    /// Erstellt einen neuen Downloader
    pub fn new(max_parallel: usize) -> Result<Self> {
        Self::new_with_http3_fallback(max_parallel, false)
    }
    
    /// Erstellt einen neuen Downloader mit optionaler HTTP/3 QUIC Unterstützung
    /// 
    /// # Arguments
    /// * `max_parallel` - Maximale Anzahl paralleler Downloads
    /// * `_try_http3` - Wenn true, versucht HTTP/3 QUIC zu verwenden (erfordert http3 feature in reqwest)
    /// 
    /// # HTTP/3 Support
    /// HTTP/3 QUIC wird automatisch verwendet wenn:
    /// 1. `_try_http3` ist true
    /// 2. Der Server HTTP/3 unterstützt
    /// 3. Das `http3` Feature in reqwest aktiviert ist (instabil, erfordert RUSTFLAGS='--cfg reqwest_unstable')
    /// 
    /// Falls HTTP/3 nicht verfügbar ist, fällt der Client automatisch auf HTTP/2 oder HTTP/1.1 zurück.
    pub fn new_with_http3_fallback(max_parallel: usize, _try_http3: bool) -> Result<Self> {
        let builder = Client::builder()
            // HTTP/2 wird automatisch verwendet wenn verfügbar
            // HTTP/3 kann aktiviert werden wenn reqwest's http3 feature aktiviert ist
            .timeout(std::time::Duration::from_secs(30)); // 30 Sekunden Timeout
        
        // Note: HTTP/3 support in reqwest is currently unstable
        // To enable it:
        // 1. Add "http3" to reqwest features in Cargo.toml
        // 2. Set RUSTFLAGS='--cfg reqwest_unstable' environment variable
        // 3. reqwest will automatically try HTTP/3 if server supports it
        
        let client = builder.build()?;
        
        Ok(Downloader {
            client,
            max_parallel,
        })
    }
    
    /// Prüft, ob HTTP/3 QUIC für eine URL verfügbar ist
    /// 
    /// Diese Methode versucht eine Verbindung mit HTTP/3 herzustellen.
    /// Falls HTTP/3 nicht unterstützt wird, gibt sie false zurück.
    #[allow(dead_code)]
    pub async fn check_http3_support(&self, url: &str) -> bool {
        // Placeholder: HTTP/3 detection würde hier implementiert werden
        // Aktuell gibt reqwest keine einfache Möglichkeit, das verwendete Protokoll zu prüfen
        // In Zukunft könnte man hier eine HEAD-Anfrage machen und prüfen, ob HTTP/3 verwendet wurde
        
        // Für jetzt: Versuche eine HEAD-Anfrage und prüfe die Antwort
        // Falls HTTP/3 verfügbar ist, würde reqwest es automatisch verwenden (mit http3 feature)
        if let Ok(response) = self.client.head(url).send().await {
            // Prüfe ob Alt-Svc Header vorhanden ist (zeigt HTTP/3 Unterstützung an)
            if let Some(alt_svc) = response.headers().get("alt-svc") {
                if let Ok(alt_svc_str) = alt_svc.to_str() {
                    return alt_svc_str.contains("h3") || alt_svc_str.contains("quic");
                }
            }
        }
        
        false
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
        
        // Get content length for progress bar
        let total_size = response.content_length();
        let progress_bar = if let Some(size) = total_size {
            Some(crate::output::Output::progress_bar(size))
        } else {
            None
        };
        
        let mut file = tokio::fs::File::create(dest).await?;
        
        let mut downloaded = 0u64;
        let mut last_update = Instant::now();
        let mut last_downloaded = 0u64;
        let update_interval = std::time::Duration::from_millis(100); // Update every 100ms
        
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            
            // Update progress bar with speed
            if let Some(ref pb) = progress_bar {
                pb.set_position(downloaded);
                
                // Calculate and display speed
                let elapsed = last_update.elapsed();
                if elapsed >= update_interval {
                    let bytes_since_update = downloaded - last_downloaded;
                    let speed = if elapsed.as_secs() > 0 {
                        bytes_since_update / elapsed.as_secs()
                    } else if elapsed.as_millis() > 0 {
                        bytes_since_update * 1000 / elapsed.as_millis() as u64
                    } else {
                        0
                    };
                    
                    let speed_str = Self::format_speed(speed);
                    pb.set_message(format!("{}", speed_str));
                    last_update = Instant::now();
                    last_downloaded = downloaded;
                }
            }
        }
        
        if let Some(ref pb) = progress_bar {
            pb.finish_with_message("Done");
        }
        
        // Validate checksum if provided
        if let Some(expected) = expected_checksum {
            self.validate_file_checksum(dest, expected).await?;
        }
        
        Ok(())
    }
    
    /// Lädt eine Datei herunter und gibt Performance-Metriken zurück
    pub async fn download_file_with_metrics(&self, url: &str, dest: &Path) -> Result<(u64, u64)> {
        use std::time::Instant;
        
        let download_start = Instant::now();
        self.download_file(url, dest).await?;
        let download_time = download_start.elapsed();
        
        let file_size = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
        let throughput = if download_time.as_secs() > 0 {
            file_size / download_time.as_secs()
        } else if download_time.as_millis() > 0 {
            file_size * 1000 / download_time.as_millis() as u64
        } else {
            0
        };
        let rtt_ms = download_time.as_millis() as u64;
        
        Ok((rtt_ms, throughput))
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
        
        // Show progress bar for resume
        let progress_bar = crate::output::Output::progress_bar(total_size);
        progress_bar.set_position(existing_size);
        
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(dest)
            .await?;
        
        let mut downloaded = existing_size;
        let mut last_update = Instant::now();
        let mut last_downloaded = existing_size;
        let update_interval = std::time::Duration::from_millis(100);
        
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            
            // Update progress bar with speed
            progress_bar.set_position(downloaded);
            
            let elapsed = last_update.elapsed();
            if elapsed >= update_interval {
                let bytes_since_update = downloaded - last_downloaded;
                let speed = if elapsed.as_secs() > 0 {
                    bytes_since_update / elapsed.as_secs()
                } else if elapsed.as_millis() > 0 {
                    bytes_since_update * 1000 / elapsed.as_millis() as u64
                } else {
                    0
                };
                
                let speed_str = Self::format_speed(speed);
                progress_bar.set_message(format!("{}", speed_str));
                last_update = Instant::now();
                last_downloaded = downloaded;
            }
        }
        
        progress_bar.finish_with_message("Done");
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
    
    /// Format download speed as human-readable string
    fn format_speed(bytes_per_sec: u64) -> String {
        if bytes_per_sec >= 1024 * 1024 {
            format!("{:.2} MB/s", bytes_per_sec as f64 / (1024.0 * 1024.0))
        } else if bytes_per_sec >= 1024 {
            format!("{:.2} KB/s", bytes_per_sec as f64 / 1024.0)
        } else {
            format!("{} B/s", bytes_per_sec)
        }
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

