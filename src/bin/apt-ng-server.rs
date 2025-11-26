use clap::Parser;
use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use apt_ng::repo_server::RepositoryServer;

#[derive(Parser)]
#[command(name = "apt-ng-server")]
#[command(about = "HTTP server for serving apt-ng repositories")]
struct Cli {
    /// Repository directory to serve
    #[arg(short, long, default_value = ".")]
    directory: PathBuf,
    
    /// Address to bind to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    address: String,
    
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Parse address
    let addr: SocketAddr = cli.address.parse()
        .map_err(|_| anyhow::anyhow!("Invalid address format. Use format: IP:PORT (e.g., 127.0.0.1:8080)"))?;
    
    // Check if directory exists
    if !cli.directory.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", cli.directory.display()));
    }
    
    if !cli.directory.is_dir() {
        return Err(anyhow::anyhow!("Path is not a directory: {}", cli.directory.display()));
    }
    
    if cli.verbose {
        println!("Starting repository server...");
        println!("  Directory: {}", cli.directory.display());
        println!("  Address: {}", addr);
    }
    
    // Create and start server
    let server = RepositoryServer::new(&cli.directory, addr);
    server.serve().await?;
    
    Ok(())
}

