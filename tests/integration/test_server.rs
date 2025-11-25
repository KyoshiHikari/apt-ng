use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Body};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use http_body_util::Full;
use bytes::Bytes;

/// Test HTTP Server für Repository-Tests
pub struct TestServer {
    addr: SocketAddr,
    routes: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl TestServer {
    /// Erstellt einen neuen Test-Server
    pub async fn new() -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        
        let routes = Arc::new(RwLock::new(HashMap::new()));
        let routes_clone = routes.clone();
        
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(_) => break,
                };
                
                let io = TokioIo::new(stream);
                let routes = routes_clone.clone();
                
                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                        let routes = routes.clone();
                        async move {
                            handle_request(req, routes).await
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
        });
        
        // Warte kurz damit der Server starten kann
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        Ok(TestServer { addr, routes })
    }
    
    /// Gibt die Base-URL des Servers zurück
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    
    /// Fügt eine Route zum Server hinzu
    pub async fn add_route(&self, path: &str, content: Vec<u8>) {
        let mut routes = self.routes.write().await;
        routes.insert(path.to_string(), content);
    }
    
    /// Fügt eine Route mit String-Content hinzu
    pub async fn add_route_str(&self, path: &str, content: &str) {
        self.add_route(path, content.as_bytes().to_vec()).await;
    }
    
    /// Erstellt eine Standard-Repository-Struktur
    pub async fn setup_test_repo(&self, suite: &str, component: &str, arch: &str) {
        let packages_content = create_test_packages_file();
        
        // Füge Packages-Dateien hinzu (verschiedene Kompressionen)
        let packages_path = format!("/dists/{}/{}/binary-{}/Packages", suite, component, arch);
        self.add_route_str(&packages_path, &packages_content).await;
        
        // Füge komprimierte Versionen hinzu
        let packages_gz_path = format!("/dists/{}/{}/binary-{}/Packages.gz", suite, component, arch);
        let packages_gz = compress_gz(&packages_content);
        self.add_route(&packages_gz_path, packages_gz).await;
        
        // Release-Datei
        let release_content = create_test_release_file(suite);
        let release_path = format!("/dists/{}/Release", suite);
        self.add_route_str(&release_path, &release_content).await;
        
        // InRelease-Datei (mit eingebetteter Signatur)
        let inrelease_path = format!("/dists/{}/InRelease", suite);
        self.add_route_str(&inrelease_path, &release_content).await;
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    routes: Arc<RwLock<HashMap<String, Vec<u8>>>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let path = req.uri().path();
    let method = req.method();
    
    match method {
        &Method::GET => {
            let routes = routes.read().await;
            if let Some(content) = routes.get(path) {
                let mut response = Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from(content.clone())))
                    .unwrap();
                
                // Setze Content-Type basierend auf Pfad
                if path.ends_with(".gz") {
                    response.headers_mut().insert(
                        hyper::header::CONTENT_TYPE,
                        "application/gzip".parse().unwrap(),
                    );
                } else if path.ends_with(".xz") {
                    response.headers_mut().insert(
                        hyper::header::CONTENT_TYPE,
                        "application/x-xz".parse().unwrap(),
                    );
                } else if path.contains("Packages") {
                    response.headers_mut().insert(
                        hyper::header::CONTENT_TYPE,
                        "text/plain".parse().unwrap(),
                    );
                }
                
                Ok(response)
            } else {
                Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::from("Not Found")))
                    .unwrap())
            }
        }
        _ => {
            Ok(Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Full::new(Bytes::from("Method Not Allowed")))
                .unwrap())
        }
    }
}

/// Erstellt eine Test-Packages-Datei
fn create_test_packages_file() -> String {
    r#"Package: test-package
Version: 1.0.0
Architecture: amd64
Depends: libc6 (>= 2.0)
Provides: test-tool
Size: 1024
SHA256: abc123def456
Filename: pool/main/t/test-package/test-package_1.0.0_amd64.deb
Description: A test package for integration tests
 This is a test package used for integration tests.

Package: another-package
Version: 2.0.0
Architecture: all
Depends: test-package
Size: 2048
SHA256: def456ghi789
Filename: pool/main/a/another-package/another-package_2.0.0_all.deb
Description: Another test package
 This package depends on test-package.

Package: simple-package
Version: 1.5.0
Architecture: amd64
Size: 512
SHA256: 111222333444
Filename: pool/main/s/simple-package/simple-package_1.5.0_amd64.deb
Description: A simple test package
 No dependencies.
"#.to_string()
}

/// Erstellt eine Test-Release-Datei
fn create_test_release_file(suite: &str) -> String {
    format!(r#"Origin: Test Repository
Label: Test Repository
Suite: {}
Codename: {}
Date: Thu, 01 Jan 2024 00:00:00 UTC
Architectures: amd64 all
Components: main
Description: Test Repository for Integration Tests
MD5Sum:
 abc123 1024 dists/{}/main/binary-amd64/Packages
 def456 2048 dists/{}/main/binary-all/Packages
SHA256:
 abc123def456 1024 dists/{}/main/binary-amd64/Packages
 def456ghi789 2048 dists/{}/main/binary-all/Packages
"#, suite, suite, suite, suite, suite, suite)
}

/// Komprimiert Content mit gzip
fn compress_gz(content: &str) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

