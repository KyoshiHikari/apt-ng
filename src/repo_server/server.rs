use anyhow::Result;
use std::path::{Path, PathBuf};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncReadExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper::header::{CONTENT_TYPE, CONTENT_LENGTH, ACCEPT_RANGES, RANGE, CONTENT_RANGE};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use bytes::Bytes;
use tokio::net::TcpListener;

/// HTTP server for serving repository files
pub struct RepositoryServer {
    repo_dir: PathBuf,
    addr: SocketAddr,
}

impl RepositoryServer {
    /// Create a new repository server
    pub fn new(repo_dir: impl AsRef<Path>, addr: SocketAddr) -> Self {
        RepositoryServer {
            repo_dir: repo_dir.as_ref().to_path_buf(),
            addr,
        }
    }

    /// Start the HTTP server
    pub async fn serve(&self) -> Result<()> {
        let listener = TcpListener::bind(self.addr).await?;
        let repo_dir = Arc::new(self.repo_dir.clone());
        
        println!("Repository server listening on http://{}", self.addr);
        println!("Serving repository from: {}", repo_dir.display());
        
        loop {
            let (stream, _) = listener.accept().await?;
            let repo_dir = Arc::clone(&repo_dir);
            
            tokio::task::spawn(async move {
                let io = TokioIo::new(stream);
                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let repo_dir = Arc::clone(&repo_dir);
                    async move {
                        handle_request(req, &repo_dir).await
                    }
                });
                
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    repo_dir: &Path,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let path = req.uri().path();
    
    // Remove leading slash
    let path = path.strip_prefix('/').unwrap_or(path);
    
    // Security: Prevent directory traversal
    if path.contains("..") {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::new()))
                .unwrap());
    }
    
    let file_path = repo_dir.join(path);
    
    match req.method() {
        &Method::GET => {
            handle_get(&file_path, &req).await
        }
        &Method::HEAD => {
            handle_head(&file_path).await
        }
        _ => {
            Ok(Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Full::new(Bytes::new()))
                .unwrap())
        }
    }
}

async fn handle_get(
    file_path: &Path,
    req: &Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Check if file exists
    if !file_path.exists() || !file_path.is_file() {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .unwrap());
    }
    
    // Read file metadata
    let metadata = match fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .unwrap());
        }
    };
    
    let file_size = metadata.len();
    
    // Handle range requests
    if let Some(range_header) = req.headers().get(RANGE) {
        if let Ok(range_str) = range_header.to_str() {
            if let Some((start, end)) = parse_range(range_str, file_size) {
                return handle_range_request(&file_path, start, end, file_size).await;
            }
        }
    }
    
    // Read entire file
    match fs::read(&file_path).await {
        Ok(data) => {
            let content_type = determine_content_type(&file_path);
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, content_type)
                .header(CONTENT_LENGTH, file_size)
                .header(ACCEPT_RANGES, "bytes")
                .body(Full::new(Bytes::from(data)))
                .unwrap())
        }
        Err(_) => {
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .unwrap())
        }
    }
}

async fn handle_head(file_path: &Path) -> Result<Response<Full<Bytes>>, hyper::Error> {
    if !file_path.exists() || !file_path.is_file() {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .unwrap());
    }
    
    let metadata = match fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .unwrap());
        }
    };
    
    let content_type = determine_content_type(&file_path);
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header(CONTENT_LENGTH, metadata.len())
        .header(ACCEPT_RANGES, "bytes")
        .body(Full::new(Bytes::new()))
        .unwrap())
}

async fn handle_range_request(
    file_path: &Path,
    start: u64,
    end: u64,
    file_size: u64,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let mut file = match fs::File::open(&file_path).await {
        Ok(f) => f,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .unwrap());
        }
    };
    
    // Seek to start position
    use tokio::io::AsyncSeekExt;
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .body(Full::new(Bytes::new()))
            .unwrap());
    }
    
    // Read range
    let length = end - start + 1;
    let mut buffer = vec![0u8; length as usize];
    
    if file.read_exact(&mut buffer).await.is_err() {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .body(Full::new(Bytes::new()))
            .unwrap());
    }
    
    let content_range = format!("bytes {}-{}/{}", start, end, file_size);
    let content_type = determine_content_type(&file_path);
    
    Ok(Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(CONTENT_TYPE, content_type)
        .header(CONTENT_LENGTH, length)
        .header(CONTENT_RANGE, content_range)
        .header(ACCEPT_RANGES, "bytes")
        .body(Full::new(Bytes::from(buffer)))
        .unwrap())
}

fn parse_range(range_str: &str, file_size: u64) -> Option<(u64, u64)> {
    // Parse "bytes=start-end" format
    if let Some(range) = range_str.strip_prefix("bytes=") {
        if let Some((start_str, end_str)) = range.split_once('-') {
            let start = start_str.parse::<u64>().ok()?;
            let end = if end_str.is_empty() {
                file_size - 1
            } else {
                end_str.parse::<u64>().ok()?
            };
            
            if start <= end && end < file_size {
                return Some((start, end));
            }
        }
    }
    None
}

fn determine_content_type(file_path: &Path) -> &'static str {
    if let Some(ext) = file_path.extension() {
        match ext.to_str().unwrap_or("") {
            "gz" => "application/gzip",
            "xz" => "application/x-xz",
            "bz2" => "application/x-bzip2",
            "deb" => "application/vnd.debian.binary-package",
            "apx" => "application/vnd.apt-ng.package",
            "sig" | "gpg" => "application/pgp-signature",
            "json" => "application/json",
            _ => "application/octet-stream",
        }
    } else {
        // Check filename for common repository files
        if let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) {
            match filename {
                "Packages" | "Packages.gz" | "Packages.xz" => "text/plain",
                "Release" | "InRelease" => "text/plain",
                "Release.gpg" => "application/pgp-signature",
                _ => "application/octet-stream",
            }
        } else {
            "application/octet-stream"
        }
    }
}

